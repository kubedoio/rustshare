use std::{sync::Arc, time::Duration};

use axum::{extract::State, http::StatusCode, routing::get, Router};
use metrics_exporter_prometheus::PrometheusHandle;
use rustshare_core::{
    domain::ApplicationRegistry,
    services::{ContentIndexer, KeywordIndexer, SimpleEmbeddingGenerator, VectorStore},
};
use rustshare_server::{
    config::OutboxWorkerConfig,
    memory_files_external_reconcile::{self, ExternalReconcileConfig},
    memory_files_worker_consumer::MemoryFilesWorkerConsumer,
    memory_worker_files_client::MemoryWorkerFilesClient,
    outbox_dispatcher::{OutboxDispatcher, OutboxStatus},
    services::note_index_sink::{ContentIndexerNoteSink, KeywordIndexerNoteSink, NoteIndexSink},
};
use rustshare_storage::{
    MemoryExecutionMode, MemoryExecutionModeStore, MemoryFilesCatalogStore, OutboxStore,
};
use sqlx::postgres::PgPoolOptions;

#[derive(Clone)]
struct HealthState {
    outbox: Arc<OutboxStatus>,
    mode_active: Arc<std::sync::atomic::AtomicBool>,
    reconcile_healthy: Arc<std::sync::atomic::AtomicBool>,
    metrics: PrometheusHandle,
    stale_after: Duration,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    anyhow::ensure!(
        std::env::var("RUSTSHARE_MEMORY_EXECUTION_MODE")
            .unwrap_or_else(|_| "embedded".into())
            .parse::<MemoryExecutionMode>()?
            == MemoryExecutionMode::External,
        "elembra-memory-worker requires RUSTSHARE_MEMORY_EXECUTION_MODE=external"
    );

    let database_url = std::env::var("MEMORY_DATABASE_URL")
        .map_err(|_| anyhow::anyhow!("MEMORY_DATABASE_URL is required"))?;
    let max_connections = std::env::var("MEMORY_WORKER_DB_MAX_CONNECTIONS")
        .map_or(Ok(4), |value| {
            value.parse::<u32>().map_err(anyhow::Error::from)
        })?;
    anyhow::ensure!(
        (4..=8).contains(&max_connections),
        "MEMORY_WORKER_DB_MAX_CONNECTIONS must be between 4 and 8 to reserve connections for fenced indexing"
    );
    let pool = PgPoolOptions::new()
        .max_connections(max_connections)
        .connect(&database_url)
        .await?;
    let metrics = rustshare_server::metrics::init_metrics();
    let files = MemoryWorkerFilesClient::from_env()?;
    let mode = MemoryExecutionMode::External;
    let mode_store = MemoryExecutionModeStore::new(pool.clone());
    let mode_process_id = uuid::Uuid::new_v4();
    let mode_generation = mode_store
        .claim(
            mode,
            mode_process_id,
            rustshare_server::memory_execution_lease::MODE_LEASE_DURATION,
        )
        .await?;
    let execution_fence = rustshare_server::memory_execution_lease::MemoryExecutionFence::new(
        mode_store.clone(),
        mode,
        mode_process_id,
        mode_generation,
    );
    let mode_active = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let (shutdown_tx, _) = tokio::sync::broadcast::channel(1);
    let mode_task = rustshare_server::memory_execution_lease::spawn_memory_execution_lease(
        mode_store,
        mode,
        mode_process_id,
        mode_generation,
        Arc::clone(&mode_active),
        shutdown_tx.subscribe(),
    );
    let catalog = MemoryFilesCatalogStore::new(pool.clone());
    let vector_store: Arc<dyn VectorStore> = Arc::new(
        rustshare_infrastructure::PgVectorStore::for_authorized_files_snapshots(pool.clone()),
    );
    let ai_enabled =
        std::env::var("RUSTSHARE_AI_ENABLED").map_or(Ok(true), |value| value.parse::<bool>())?;
    let index_sink: Arc<dyn NoteIndexSink> = if ai_enabled {
        let content = Arc::new(ContentIndexer::new(
            Arc::new(SimpleEmbeddingGenerator::new()),
            Arc::clone(&vector_store),
        ));
        Arc::new(ContentIndexerNoteSink::new(content))
    } else {
        let keywords = Arc::new(KeywordIndexer::new(vector_store));
        Arc::new(KeywordIndexerNoteSink::new(keywords))
    };
    let consumer = Arc::new(
        MemoryFilesWorkerConsumer::new(pool.clone(), catalog, files, index_sink)
            .with_execution_guard(Arc::clone(&mode_active))
            .with_execution_fence(execution_fence),
    );
    let (reconcile_task, reconcile_healthy) = memory_files_external_reconcile::spawn(
        pool.clone(),
        MemoryWorkerFilesClient::from_env()?,
        Arc::clone(&consumer),
        ExternalReconcileConfig::from_env()?,
        shutdown_tx.subscribe(),
    );
    let registry = Arc::new(ApplicationRegistry::first_party()?);
    let outbox = Arc::new(OutboxStore::new(pool, registry));
    let mut config = OutboxWorkerConfig::from_env();
    anyhow::ensure!(
        config.enabled,
        "RUSTSHARE_OUTBOX_WORKER_ENABLED must be true for the Memory worker"
    );
    config.claim_batch_size = config.claim_batch_size.clamp(1, 10);
    // Outbox retention remains owned by the backend dispatcher.
    config.retention_hours = 0;
    let dispatcher = Arc::new(OutboxDispatcher::new(
        outbox,
        vec![consumer],
        config.clone(),
        format!("memory-worker-{}", uuid::Uuid::new_v4()),
    ));
    let status = Arc::clone(dispatcher.status());
    let worker_task = dispatcher.spawn(shutdown_tx.subscribe());

    let health_state = HealthState {
        outbox: status,
        mode_active: Arc::clone(&mode_active),
        reconcile_healthy,
        metrics,
        stale_after: Duration::from_secs(config.readiness_staleness_secs.max(1)),
    };
    let app = Router::new()
        .route("/health", get(liveness))
        .route("/ready", get(readiness))
        .route("/metrics", get(metrics_endpoint))
        .with_state(health_state);
    let health_addr =
        std::env::var("MEMORY_WORKER_HEALTH_ADDR").unwrap_or_else(|_| "0.0.0.0:8081".into());
    let listener = tokio::net::TcpListener::bind(&health_addr).await?;
    tracing::info!(address = %health_addr, "Elembra Memory worker started");
    let result = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await;
    let _ = shutdown_tx.send(());
    worker_task.await?;
    reconcile_task.await?;
    mode_task.await?;
    result?;
    Ok(())
}

async fn liveness() -> StatusCode {
    StatusCode::OK
}

async fn readiness(State(state): State<HealthState>) -> StatusCode {
    let recent = state
        .outbox
        .last_tick_at
        .lock()
        .ok()
        .and_then(|last| *last)
        .is_some_and(|last| last.elapsed() <= state.stale_after);
    if recent
        && state.mode_active.load(std::sync::atomic::Ordering::Acquire)
        && state
            .reconcile_healthy
            .load(std::sync::atomic::Ordering::Acquire)
        && state
            .outbox
            .last_tick_ok
            .load(std::sync::atomic::Ordering::Relaxed)
    {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}

async fn metrics_endpoint(State(state): State<HealthState>) -> String {
    state.metrics.render()
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("install SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {},
            _ = terminate.recv() => {},
        }
    }
    #[cfg(not(unix))]
    tokio::signal::ctrl_c()
        .await
        .expect("install Ctrl-C handler");
}
