use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};
use reqwest::{multipart, redirect::Policy, StatusCode};
use serde::{Deserialize, Serialize};
use shell_memory_collector::{
    record_if_enabled, Config, Event, Queue, QueueLock, RecordInput, UploadLock,
};
use std::{
    io::Read,
    net::IpAddr,
    path::{Path, PathBuf},
    time::Duration,
};
use url::Url;
use uuid::Uuid;

const CREDENTIAL_SERVICE: &str = "io.elembra.shell-memory";
const CONNECTOR_ID: &str = "io.elembra.connector.shell-memory";
const EVENTS_PER_UPLOAD_ROUND: usize = 1;

#[derive(Debug, Parser)]
#[command(name = "elembra-shell-memory")]
struct Args {
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    InitConfig {
        #[arg(long)]
        server: String,
        #[arg(long)]
        instance_id: Uuid,
    },
    Hook {
        shell: String,
    },
    SessionId,
    StoreCredential,
    Record,
    Run,
    Pause,
    Resume,
    Disable,
    Enable,
    Forget,
    List,
    ForgetLast,
    ForgetSelected {
        ids: Vec<Uuid>,
    },
    ForgetSession,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkerSession {
    id: Uuid,
    connector_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExecutionLease {
    lease_id: Uuid,
    fence: i64,
    checkpoint_version: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportResponse {
    target_resource_ref: Option<String>,
    #[serde(default)]
    forgotten: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ForgetRequest<'a> {
    source_resource_ref: String,
    source_revision: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportMetadata<'a> {
    lease_id: Uuid,
    fence: i64,
    source_resource_ref: String,
    source_revision: &'a str,
    relative_path: String,
    observed_modified_at: DateTime<Utc>,
    parent_folder_id: Option<Uuid>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Checkpoint {
    revision: i64,
    queue_id: Uuid,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    if matches!(&args.command, Command::SessionId) {
        println!("{}", Uuid::new_v4());
        return Ok(());
    }
    let config_path = config_path(args.config)?;
    match args.command {
        Command::InitConfig {
            server,
            instance_id,
        } => init_config(&config_path, server, instance_id),
        Command::Hook { shell } => {
            if shell != "zsh" {
                bail!("only the explicit zsh hook is supported");
            }
            print!("{}", include_str!("../shell/elembra-shell-memory.zsh"));
            Ok(())
        }
        Command::SessionId => unreachable!("handled before configuration loading"),
        Command::StoreCredential => store_credential(&config_path),
        Command::Record => record(&config_path),
        Command::Run => run(&config_path).await,
        Command::Pause => set_control(&config_path, Control::Pause, true),
        Command::Resume => set_control(&config_path, Control::Pause, false),
        Command::Disable => set_control(&config_path, Control::Disable, true),
        Command::Enable => set_control(&config_path, Control::Disable, false),
        Command::Forget => forget(&config_path),
        Command::List => list(&config_path),
        Command::ForgetLast => forget_last(&config_path).await,
        Command::ForgetSelected { ids } => forget_selected(&config_path, ids).await,
        Command::ForgetSession => forget_session(&config_path).await,
    }
}

fn config_path(path: Option<PathBuf>) -> Result<PathBuf> {
    match path {
        Some(path) => Ok(path),
        None => dirs::config_dir()
            .map(|dir| dir.join("elembra/shell-memory.json"))
            .context("no user config directory is available; pass --config"),
    }
}

fn init_config(path: &PathBuf, server: String, instance_id: Uuid) -> Result<()> {
    let server = normalize_server(&server)?;
    let queue = dirs::data_local_dir()
        .context("no local data directory is available")?
        .join("elembra/shell-memory.sqlite3");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let config = Config {
        server,
        instance_id,
        queue,
        exclude_commands: vec!["git push*".into(), "ssh *".into(), "scp *".into()],
    };
    std::fs::write(path, serde_json::to_vec_pretty(&config)?)?;
    println!(
        "Wrote {}. Review excludeCommands before opting in.",
        path.display()
    );
    Ok(())
}

fn load_config(path: &PathBuf) -> Result<Config> {
    let mut config: Config = serde_json::from_slice(
        &std::fs::read(path)
            .with_context(|| format!("unable to read config: {}", path.display()))?,
    )?;
    config.server = normalize_server(&config.server)?;
    Ok(config)
}

fn record(config_path: &PathBuf) -> Result<()> {
    let config = load_config(config_path)?;
    let mut bytes = Vec::new();
    std::io::stdin().read_to_end(&mut bytes)?;
    let fields = bytes
        .split(|byte| *byte == 0)
        .filter(|field| !field.is_empty())
        .collect::<Vec<_>>();
    if !matches!(fields.len(), 5 | 6) {
        bail!("record input must contain five metadata fields and an optional session ID");
    }
    let input = RecordInput {
        command: std::str::from_utf8(fields[0])?.to_owned(),
        cwd: std::str::from_utf8(fields[1])?.to_owned(),
        host: std::str::from_utf8(fields[2])?.to_owned(),
        exit_code: std::str::from_utf8(fields[3])?
            .parse()
            .context("invalid command exit code")?,
        duration_ms: std::str::from_utf8(fields[4])?
            .parse()
            .context("invalid command duration")?,
        session_id: fields
            .get(5)
            .and_then(|field| std::str::from_utf8(field).ok())
            .and_then(|id| Uuid::parse_str(id).ok()),
    };
    record_if_enabled(
        &config.queue,
        config.instance_id,
        input,
        &config.exclude_commands,
    )?;
    Ok(())
}

async fn run(config_path: &PathBuf) -> Result<()> {
    let config = load_config(config_path)?;
    let client = reqwest::Client::builder()
        .redirect(Policy::none())
        .timeout(Duration::from_secs(30))
        .build()?;
    loop {
        let delay = match upload_round(&client, &config, || {
            read_keyring_credential(config.instance_id)
        })
        .await
        {
            Ok(UploadRound::Disabled) => {
                bail!("collector is disabled; run enable before starting the uploader");
            }
            Ok(outcome) => outcome.next_delay(),
            Err(error) => {
                eprintln!("Shell Memory upload deferred; queued events remain local: {error:#}");
                Duration::from_secs(15)
            }
        };
        tokio::select! {
            _ = tokio::time::sleep(delay) => {},
            signal = tokio::signal::ctrl_c() => { signal?; return Ok(()); }
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum UploadRound {
    Disabled,
    Idle,
    NoLease,
    Completed,
}

impl UploadRound {
    fn next_delay(&self) -> Duration {
        match self {
            Self::Completed => Duration::from_millis(100),
            Self::Disabled | Self::Idle | Self::NoLease => Duration::from_secs(15),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum UploadResult {
    NoLease,
    Completed,
    Paused,
    Disabled,
}

fn upload_stop(queue_path: &Path) -> Result<Option<UploadResult>> {
    let _queue_lock = QueueLock::acquire(queue_path)?;
    if Queue::is_disabled(queue_path)? {
        return Ok(Some(UploadResult::Disabled));
    }
    let queue = Queue::open(queue_path)?;
    Ok(queue.paused().then_some(UploadResult::Paused))
}

async fn upload_round<F>(
    client: &reqwest::Client,
    config: &Config,
    credential: F,
) -> Result<UploadRound>
where
    F: FnOnce() -> Result<String>,
{
    let _upload_lock = UploadLock::acquire(&config.queue)?;
    let (events, pending_forgets, disabled) = {
        let _queue_lock = QueueLock::acquire(&config.queue)?;
        let disabled = Queue::is_disabled(&config.queue)?;
        let queue = Queue::open(&config.queue)?;
        let forgets = queue.pending_forgets(config.instance_id, EVENTS_PER_UPLOAD_ROUND)?;
        let events = if !forgets.is_empty() || queue.paused() || disabled {
            Vec::new()
        } else {
            queue.pending(config.instance_id, EVENTS_PER_UPLOAD_ROUND)?
        };
        (events, forgets, disabled)
    };
    if !pending_forgets.is_empty() {
        let credential = credential()?;
        sync_forgets(client, config, &credential, &pending_forgets).await?;
        return Ok(UploadRound::Completed);
    }
    if disabled {
        return Ok(UploadRound::Disabled);
    }
    if events.is_empty() {
        return Ok(UploadRound::Idle);
    }
    let credential = credential()?;
    match upload_pending(client, config, &credential, events).await? {
        UploadResult::NoLease => Ok(UploadRound::NoLease),
        UploadResult::Completed => Ok(UploadRound::Completed),
        UploadResult::Paused => Ok(UploadRound::Idle),
        UploadResult::Disabled => Ok(UploadRound::Disabled),
    }
}

async fn upload_pending(
    client: &reqwest::Client,
    config: &Config,
    credential: &str,
    events: Vec<Event>,
) -> Result<UploadResult> {
    let session: WorkerSession = client
        .get(format!(
            "{}/api/v1/connectors/worker/session",
            config.server
        ))
        .header("Authorization", format!("Connector {credential}"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    if session.id != config.instance_id || session.connector_id != CONNECTOR_ID {
        bail!("credential is not bound to the configured Shell Memory Connector instance");
    }
    let Some(lease) = client
        .post(format!("{}/api/v1/connectors/worker/lease", config.server))
        .header("Authorization", format!("Connector {credential}"))
        .send()
        .await?
        .error_for_status()?
        .json::<Option<ExecutionLease>>()
        .await?
    else {
        return Ok(UploadResult::NoLease);
    };
    let mut completed = Vec::with_capacity(events.len());
    let mut stopped = None;
    for event in events {
        if let Some(reason) = upload_stop(&config.queue)? {
            stopped = Some(reason);
            break;
        }
        // shortcut: one Files object per command; batch daily views if object counts become material.
        let relative_path = format!("shell-command-{}.md", event.id);
        let source_resource_ref = format!("elembra://{CONNECTOR_ID}/file/{}", event.id);
        let metadata = serde_json::to_string(&ImportMetadata {
            lease_id: lease.lease_id,
            fence: lease.fence,
            source_resource_ref,
            source_revision: &event.revision,
            relative_path: relative_path.clone(),
            observed_modified_at: event.created_at,
            parent_folder_id: None,
        })?;
        let form = multipart::Form::new().text("metadata", metadata).part(
            "file",
            multipart::Part::bytes(event.content.clone())
                .file_name(format!("{}.md", event.id))
                .mime_str("text/markdown; charset=utf-8")?,
        );
        let response = client
            .post(format!(
                "{}/api/v1/connectors/worker/files/import",
                config.server
            ))
            .header("Authorization", format!("Connector {credential}"))
            .multipart(form)
            .send()
            .await?;
        if !response.status().is_success() {
            if let Err(error) = report_failure(
                client,
                config,
                credential,
                &lease,
                failure_category(response.status()),
            )
            .await
            {
                eprintln!(
                    "Shell Memory failure report was rejected; queued event retained: {error}"
                );
            }
            bail!("Files import failed with HTTP {}", response.status());
        }
        let imported: ImportResponse = response.json().await?;
        match (imported.forgotten, imported.target_resource_ref) {
            (true, None) => completed.push(event.id),
            (false, Some(_)) => {
                let _lock = QueueLock::acquire(&config.queue)?;
                Queue::open(&config.queue)?.remember_import(config.instance_id, &event)?;
                completed.push(event.id);
            }
            _ => bail!("Files import response had an invalid forgotten state"),
        }
    }
    let Some(queue_id) = completed.last() else {
        return stopped
            .ok_or_else(|| anyhow::anyhow!("upload round completed without a Files import"));
    };
    let response = client
        .post(format!(
            "{}/api/v1/connectors/worker/lease/{}/ack",
            config.server, lease.lease_id
        ))
        .header("Authorization", format!("Connector {credential}"))
        .json(&serde_json::json!({
            "fence": lease.fence,
            "expectedCheckpointVersion": lease.checkpoint_version,
            "cursor": Checkpoint { revision: lease.checkpoint_version + 1, queue_id: *queue_id },
        }))
        .send()
        .await?;
    if response.status() != StatusCode::NO_CONTENT {
        bail!(
            "Connector checkpoint rejected with HTTP {}",
            response.status()
        );
    }
    let _queue_lock = QueueLock::acquire(&config.queue)?;
    let queue = Queue::open(&config.queue)?;
    for id in completed {
        queue.acknowledge(id)?;
    }
    Ok(stopped.unwrap_or(UploadResult::Completed))
}

async fn validate_worker_session(
    client: &reqwest::Client,
    config: &Config,
    credential: &str,
) -> Result<()> {
    let session: WorkerSession = client
        .get(format!(
            "{}/api/v1/connectors/worker/session",
            config.server
        ))
        .header("Authorization", format!("Connector {credential}"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    if session.id != config.instance_id || session.connector_id != CONNECTOR_ID {
        bail!("credential is not bound to the configured Shell Memory Connector instance");
    }
    Ok(())
}

async fn sync_forgets(
    client: &reqwest::Client,
    config: &Config,
    credential: &str,
    forgets: &[shell_memory_collector::SourceMapping],
) -> Result<()> {
    validate_worker_session(client, config, credential).await?;
    for mapping in forgets {
        let source_resource_ref = format!("elembra://{CONNECTOR_ID}/file/{}", mapping.id);
        let response = client
            .post(format!(
                "{}/api/v1/connectors/worker/files/forget",
                config.server
            ))
            .header("Authorization", format!("Connector {credential}"))
            .json(&ForgetRequest {
                source_resource_ref,
                source_revision: &mapping.revision,
            })
            .send()
            .await?;
        if response.status() != StatusCode::NO_CONTENT {
            bail!("Connector forget deferred with HTTP {}", response.status());
        }
        let _lock = QueueLock::acquire(&config.queue)?;
        Queue::open(&config.queue)?.acknowledge_forget(config.instance_id, mapping.id)?;
    }
    Ok(())
}

fn list(config_path: &PathBuf) -> Result<()> {
    let config = load_config(config_path)?;
    let _lock = QueueLock::acquire(&config.queue)?;
    let queue = Queue::open(&config.queue)?;
    for record in queue.list(config.instance_id, 100)? {
        println!(
            "{}\t{}\t{}",
            record.id,
            record.created_at.to_rfc3339(),
            if record.uploaded {
                "uploaded"
            } else {
                "queued"
            }
        );
    }
    Ok(())
}

async fn forget_last(config_path: &PathBuf) -> Result<()> {
    let config = load_config(config_path)?;
    let id = {
        let _lock = QueueLock::acquire(&config.queue)?;
        Queue::open(&config.queue)?.last_id(config.instance_id)?
    };
    let Some(id) = id else {
        println!("No local Shell Memory records.");
        return Ok(());
    };
    forget_selected(config_path, vec![id]).await
}

async fn forget_session(config_path: &PathBuf) -> Result<()> {
    let session_id = std::env::var("ELEMBRA_SHELL_MEMORY_SESSION_ID")
        .context("current shell session ID is unavailable; source the zsh hook again")?;
    let session_id = Uuid::parse_str(&session_id).context("current shell session ID is invalid")?;
    let config = load_config(config_path)?;
    let _upload_lock = UploadLock::acquire(&config.queue)?;
    {
        let _lock = QueueLock::acquire(&config.queue)?;
        Queue::open(&config.queue)?.request_forget_session(config.instance_id, session_id)?;
    }
    sync_pending_forgets(config_path).await
}

async fn forget_selected(config_path: &PathBuf, ids: Vec<Uuid>) -> Result<()> {
    if ids.is_empty() {
        bail!("provide at least one local record ID");
    }
    let config = load_config(config_path)?;
    let _upload_lock = UploadLock::acquire(&config.queue)?;
    {
        let _lock = QueueLock::acquire(&config.queue)?;
        let queue = Queue::open(&config.queue)?;
        for id in ids {
            queue.request_forget(config.instance_id, id)?;
        }
    }
    sync_pending_forgets(config_path).await
}

async fn sync_pending_forgets(config_path: &PathBuf) -> Result<()> {
    let config = load_config(config_path)?;
    let client = reqwest::Client::builder()
        .redirect(Policy::none())
        .timeout(Duration::from_secs(30))
        .build()?;
    let pending = {
        let _lock = QueueLock::acquire(&config.queue)?;
        Queue::open(&config.queue)?.pending_forgets(config.instance_id, 100)?
    };
    if pending.is_empty() {
        println!("No remote Shell Memory forgets are pending.");
        return Ok(());
    }
    let credential = read_keyring_credential(config.instance_id)?;
    let forgotten = sync_all_pending_forgets(&client, &config, &credential).await?;
    println!("Forgot {forgotten} Shell Memory record(s).");
    Ok(())
}

async fn sync_all_pending_forgets(
    client: &reqwest::Client,
    config: &Config,
    credential: &str,
) -> Result<usize> {
    let mut total = 0;
    loop {
        let pending = {
            let _lock = QueueLock::acquire(&config.queue)?;
            Queue::open(&config.queue)?.pending_forgets(config.instance_id, 100)?
        };
        if pending.is_empty() {
            return Ok(total);
        }
        let count = pending.len();
        sync_forgets(client, config, credential, &pending).await?;
        total += count;
    }
}

async fn report_failure(
    client: &reqwest::Client,
    config: &Config,
    credential: &str,
    lease: &ExecutionLease,
    error: &str,
) -> Result<()> {
    let response = client
        .post(format!(
            "{}/api/v1/connectors/worker/lease/{}/fail",
            config.server, lease.lease_id
        ))
        .header("Authorization", format!("Connector {credential}"))
        .json(&serde_json::json!({ "fence": lease.fence, "error": error }))
        .send()
        .await
        .map_err(|_| anyhow::anyhow!("Connector failure report request failed"))?;
    if response.status() != StatusCode::NO_CONTENT {
        bail!(
            "Connector failure report rejected with HTTP {}",
            response.status()
        );
    }
    Ok(())
}

fn failure_category(status: StatusCode) -> &'static str {
    match status {
        StatusCode::UNAUTHORIZED => "authentication",
        StatusCode::FORBIDDEN => "policy_denied",
        StatusCode::TOO_MANY_REQUESTS => "rate_limit",
        StatusCode::BAD_REQUEST => "invalid_content",
        _ => "source_unavailable",
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Control {
    Pause,
    Disable,
}

fn update_control(queue_path: &Path, control: Control, value: bool) -> Result<()> {
    let _lock = QueueLock::acquire(queue_path)?;
    let queue = Queue::open(queue_path)?;
    match control {
        Control::Pause => queue.set_paused(value),
        Control::Disable => queue.set_disabled(value),
    }
}

fn set_control(config_path: &PathBuf, control: Control, value: bool) -> Result<()> {
    let config = load_config(config_path)?;
    if value {
        update_control(&config.queue, control, true)?;
    }
    let _upload_lock = UploadLock::acquire(&config.queue)?;
    update_control(&config.queue, control, value)
}

fn store_credential(config_path: &PathBuf) -> Result<()> {
    let config = load_config(config_path)?;
    let mut credential = String::new();
    std::io::stdin().read_to_string(&mut credential)?;
    let credential = credential.trim();
    if !credential.starts_with("ecw1_") || credential.len() > 128 {
        bail!("invalid Connector worker credential");
    }
    let _upload_lock = UploadLock::acquire(&config.queue)?;
    keyring::Entry::new(CREDENTIAL_SERVICE, &config.instance_id.to_string())?
        .set_password(credential)?;
    Ok(())
}

fn read_keyring_credential(instance_id: Uuid) -> Result<String> {
    keyring::Entry::new(CREDENTIAL_SERVICE, &instance_id.to_string())?
        .get_password()
        .context("Shell Memory worker credential is unavailable in the OS credential store")
}

fn forget_queue(config: &Config) -> Result<()> {
    update_control(&config.queue, Control::Disable, true)?;
    let _upload_lock = UploadLock::acquire(&config.queue)?;
    let _lock = QueueLock::acquire(&config.queue)?;
    let queue = Queue::open(&config.queue)?;
    queue.forget()?;
    Ok(())
}

fn forget(config_path: &PathBuf) -> Result<()> {
    let config = load_config(config_path)?;
    forget_queue(&config)?;
    match keyring::Entry::new(CREDENTIAL_SERVICE, &config.instance_id.to_string())?
        .delete_credential()
    {
        Ok(()) | Err(keyring::Error::NoEntry) => {}
        Err(error) => return Err(error.into()),
    }
    println!("Local Shell Memory queue and credential removed. Ask a Connector owner to rotate or disconnect the remote credential to revoke it.");
    Ok(())
}

fn normalize_server(server: &str) -> Result<String> {
    let mut url = Url::parse(server).context("invalid Connector server URL")?;
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !matches!(url.path(), "" | "/")
    {
        bail!("Connector server URL must not contain credentials, query, fragment, or path");
    }
    match url.scheme() {
        "https" => {}
        "http"
            if url
                .host_str()
                .and_then(|host| host.parse::<IpAddr>().ok())
                .is_some_and(|address| address.is_loopback()) => {}
        _ => bail!("Connector server requires HTTPS except for loopback development"),
    }
    url.set_path("");
    Ok(url.as_str().trim_end_matches('/').to_owned())
}

#[cfg(test)]
#[path = "../../../backend/server/tests/common/mod.rs"]
mod server_test_common;

#[cfg(test)]
mod tests {
    use super::server_test_common as server_common;

    use super::{
        failure_category, forget_queue, normalize_server, set_control, sync_all_pending_forgets,
        sync_forgets, upload_pending, upload_round, Control, UploadResult, UploadRound,
    };
    use anyhow::{Context, Result};
    use axum::extract::FromRef;
    use reqwest::StatusCode;
    use rustshare_core::{
        domain::{
            ApplicationRegistry, ConnectorAuthority, ConnectorCapabilities, ConnectorDescriptor,
            ConnectorMetadata, ConnectorModes, ConnectorOperations, ConnectorPublications,
            ConnectorSource, CONNECTOR_API_VERSION,
        },
        services::{KeywordIndexer, VectorStore},
    };
    use rustshare_infrastructure::{repositories::PermissionResolverRepository, PgVectorStore};
    use rustshare_resource_auth::{ResourceOwnerRegistry, SourceAuthorizer};
    use rustshare_server::{
        authz::FilesResourceOwner,
        handlers::memory_reconcile::reconcile_files_memory_page,
        routes,
        services::{note_index_sink::KeywordIndexerNoteSink, unified_search::UnifiedSearchService},
        state::DatabaseState,
    };
    use rustshare_storage::MemoryFilesCatalogStore;
    use shell_memory_collector::{
        record_if_enabled, Config, Queue, QueueLock, RecordInput, UploadLock,
    };
    use std::process::Command;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::{TcpListener, TcpStream},
        sync::{mpsc, oneshot},
    };
    use uuid::Uuid;

    struct RegisteredWorker {
        instance_id: Uuid,
        credential: String,
    }

    async fn register_worker(
        state: &rustshare_server::AppState,
        tenant_id: Uuid,
        owner_id: Uuid,
    ) -> Result<RegisteredWorker> {
        let descriptor = ConnectorDescriptor {
            api_version: CONNECTOR_API_VERSION.into(),
            kind: "Connector".into(),
            metadata: ConnectorMetadata {
                id: "io.elembra.connector.shell-memory".into(),
                name: "Shell Memory connected acceptance".into(),
                version: "test".into(),
            },
            source: ConnectorSource {
                system: "shell-history".into(),
                authority: ConnectorAuthority::Local,
            },
            supports: ConnectorCapabilities {
                operations: ConnectorOperations {
                    read: true,
                    ..Default::default()
                },
                modes: ConnectorModes {
                    import: true,
                    ..Default::default()
                },
            },
            publishes: ConnectorPublications::default(),
        };
        let database = DatabaseState::from_ref(state);
        let (instance, _) = database
            .connector_service
            .register(
                tenant_id,
                owner_id,
                &format!("shell-memory-e2e-{}", Uuid::new_v4()),
                rustshare_server::services::connector_service::RegisterConnector {
                    descriptor,
                    source_config: serde_json::json!({}),
                    credential_ref: None,
                    mode: "import".into(),
                },
            )
            .await?;
        let issued = database
            .connector_service
            .rotate_worker_credential(tenant_id, owner_id, Uuid::from(instance.id))
            .await?;
        Ok(RegisteredWorker {
            instance_id: Uuid::from(instance.id),
            credential: issued.credential,
        })
    }

    async fn search(
        client: &reqwest::Client,
        server: &str,
        token: &str,
        query: &str,
    ) -> Result<serde_json::Value> {
        let response = client
            .post(format!("{server}/api/v1/search"))
            .bearer_auth(token)
            .json(&serde_json::json!({ "query": query, "sources": ["files"] }))
            .send()
            .await?;
        anyhow::ensure!(
            response.status().is_success(),
            "search failed: {}",
            response.status()
        );
        Ok(response.json().await?)
    }

    async fn reconcile(state: &rustshare_server::AppState, tenant_id: Uuid) -> Result<()> {
        reconcile_files_memory_page(
            &state.metadata_store,
            &state.note_service,
            &MemoryFilesCatalogStore::new(state.db_pool.clone()),
            &state.source_authorizer,
            tenant_id,
            None,
            100,
        )
        .await
        .map(|_| ())
        .map_err(|(_, error)| anyhow::anyhow!("Files Memory reconcile failed: {error:?}"))
    }

    async fn read_request(stream: &mut TcpStream) -> std::io::Result<(String, String, Vec<u8>)> {
        let mut header = Vec::new();
        let mut byte = [0_u8; 1];
        while !header.ends_with(b"\r\n\r\n") {
            stream.read_exact(&mut byte).await?;
            header.push(byte[0]);
        }
        let header_text = String::from_utf8_lossy(&header).into_owned();
        let request_line = header_text.lines().next().unwrap_or_default().to_owned();
        let content_length = header_text
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().ok())
                    .flatten()
            })
            .unwrap_or(0);
        let mut body = vec![0; content_length];
        stream.read_exact(&mut body).await?;
        Ok((request_line, header_text, body))
    }

    async fn respond(stream: &mut TcpStream, status: &str, body: &[u8], content_type: &str) {
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: {content_type}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
        stream.write_all(body).await.unwrap();
    }

    async fn fake_connector(
        listener: TcpListener,
        instance_id: Uuid,
        forgotten: bool,
        import_started: mpsc::Sender<Vec<u8>>,
        import_release: oneshot::Receiver<()>,
        acknowledged: oneshot::Sender<()>,
    ) -> std::io::Result<()> {
        let mut import_release = Some(import_release);
        let mut acknowledged = Some(acknowledged);
        loop {
            let (mut stream, _) = listener.accept().await?;
            let (request, _, body) = read_request(&mut stream).await?;
            let path = request.split_whitespace().nth(1).unwrap_or_default();
            match path {
                "/api/v1/connectors/worker/session" => {
                    let body = serde_json::json!({
                        "id": instance_id,
                        "connectorId": "io.elembra.connector.shell-memory"
                    });
                    respond(
                        &mut stream,
                        "200 OK",
                        body.to_string().as_bytes(),
                        "application/json",
                    )
                    .await;
                }
                "/api/v1/connectors/worker/lease" => {
                    let body = serde_json::json!({
                        "leaseId": Uuid::new_v4(),
                        "fence": 7,
                        "checkpointVersion": 0,
                        "expiresAt": chrono::Utc::now() + chrono::Duration::minutes(1),
                        "cursor": null
                    });
                    respond(
                        &mut stream,
                        "200 OK",
                        body.to_string().as_bytes(),
                        "application/json",
                    )
                    .await;
                }
                "/api/v1/connectors/worker/files/import" => {
                    import_started
                        .send(body)
                        .await
                        .map_err(std::io::Error::other)?;
                    if let Some(release) = import_release.take() {
                        release.await.map_err(std::io::Error::other)?;
                    }
                    let body = serde_json::json!({
                        "sourceResourceRef": "elembra://io.elembra.connector.shell-memory/file/synthetic",
                        "sourceRevision": "sha256:synthetic",
                        "targetResourceRef": if forgotten { serde_json::Value::Null } else { serde_json::json!("elembra://io.elembra.files/file/synthetic") },
                        "forgotten": forgotten
                    });
                    respond(
                        &mut stream,
                        "200 OK",
                        body.to_string().as_bytes(),
                        "application/json",
                    )
                    .await;
                }
                path if path.ends_with("/ack") => {
                    respond(&mut stream, "204 No Content", b"", "application/json").await;
                    if let Some(done) = acknowledged.take() {
                        let _ = done.send(());
                    }
                    return Ok(());
                }
                _ => {
                    respond(&mut stream, "404 Not Found", b"{}", "application/json").await;
                }
            }
        }
    }

    async fn fake_connector_without_lease(
        listener: TcpListener,
        instance_id: Uuid,
        sessions_remaining: usize,
        authorizations: mpsc::Sender<String>,
    ) -> std::io::Result<()> {
        let mut sessions_remaining = sessions_remaining;
        loop {
            let (mut stream, _) = listener.accept().await?;
            let (request, headers, _) = read_request(&mut stream).await?;
            let path = request.split_whitespace().nth(1).unwrap_or_default();
            match path {
                "/api/v1/connectors/worker/session" => {
                    let authorization = headers
                        .lines()
                        .find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            name.eq_ignore_ascii_case("authorization")
                                .then(|| value.trim().to_owned())
                        })
                        .ok_or_else(|| std::io::Error::other("authorization header missing"))?;
                    authorizations
                        .send(authorization)
                        .await
                        .map_err(std::io::Error::other)?;
                    let body = serde_json::json!({
                        "id": instance_id,
                        "connectorId": "io.elembra.connector.shell-memory"
                    });
                    respond(
                        &mut stream,
                        "200 OK",
                        body.to_string().as_bytes(),
                        "application/json",
                    )
                    .await;
                }
                "/api/v1/connectors/worker/lease" => {
                    respond(&mut stream, "200 OK", b"null", "application/json").await;
                    sessions_remaining -= 1;
                    if sessions_remaining == 0 {
                        return Ok(());
                    }
                }
                _ => {
                    return Err(std::io::Error::other(
                        "no-lease flow must not import or acknowledge",
                    ));
                }
            }
        }
    }

    async fn fake_forget_server(
        listener: TcpListener,
        instance_id: Uuid,
        source_ref: String,
        revision: String,
    ) -> std::io::Result<()> {
        for attempt in 0..2 {
            let (mut stream, _) = listener.accept().await?;
            let (request, _headers, _body) = read_request(&mut stream).await?;
            let path = request.split_whitespace().nth(1).unwrap_or_default();
            if path == "/api/v1/connectors/worker/session" {
                let response = serde_json::json!({ "id": instance_id, "connectorId": "io.elembra.connector.shell-memory" });
                respond(
                    &mut stream,
                    "200 OK",
                    response.to_string().as_bytes(),
                    "application/json",
                )
                .await;
                let (mut stream, _) = listener.accept().await?;
                let (request, headers, body) = read_request(&mut stream).await?;
                assert!(request.starts_with("POST /api/v1/connectors/worker/files/forget "));
                assert!(headers
                    .to_ascii_lowercase()
                    .contains("authorization: connector ecw1_synthetic"));
                let payload: serde_json::Value =
                    serde_json::from_slice(&body).map_err(std::io::Error::other)?;
                assert_eq!(payload["sourceRevision"].as_str(), Some(revision.as_str()));
                assert_eq!(
                    payload["sourceResourceRef"].as_str(),
                    Some(source_ref.as_str())
                );
                assert!(payload.get("targetResourceRef").is_none());
                let (status, response) = if attempt == 0 {
                    ("503 Service Unavailable", b"{}".as_slice())
                } else {
                    ("204 No Content", b"".as_slice())
                };
                respond(&mut stream, status, response, "application/json").await;
            } else {
                return Err(std::io::Error::other(format!("unexpected request: {path}")));
            }
        }
        Ok(())
    }

    async fn fake_forget_batch_server(
        listener: TcpListener,
        instance_id: Uuid,
        expected_forgets: usize,
    ) -> std::io::Result<()> {
        let source_prefix = "elembra://io.elembra.connector.shell-memory/file/";
        let mut session_requests = 0;
        let mut forget_requests = 0;
        while forget_requests < expected_forgets {
            let (mut stream, _) = listener.accept().await?;
            let (request, headers, body) = read_request(&mut stream).await?;
            let path = request.split_whitespace().nth(1).unwrap_or_default();
            match path {
                "/api/v1/connectors/worker/session" => {
                    session_requests += 1;
                    let response = serde_json::json!({
                        "id": instance_id,
                        "connectorId": "io.elembra.connector.shell-memory"
                    });
                    respond(
                        &mut stream,
                        "200 OK",
                        response.to_string().as_bytes(),
                        "application/json",
                    )
                    .await;
                }
                "/api/v1/connectors/worker/files/forget" => {
                    assert!(headers
                        .to_ascii_lowercase()
                        .contains("authorization: connector ecw1_synthetic-forget-batch"));
                    let payload: serde_json::Value =
                        serde_json::from_slice(&body).map_err(std::io::Error::other)?;
                    assert!(payload["sourceResourceRef"]
                        .as_str()
                        .is_some_and(|value| value.starts_with(source_prefix)));
                    assert!(payload["sourceRevision"]
                        .as_str()
                        .is_some_and(|value| value.starts_with("sha256:")));
                    assert!(payload.get("targetResourceRef").is_none());
                    forget_requests += 1;
                    respond(&mut stream, "204 No Content", b"", "application/json").await;
                }
                _ => {
                    return Err(std::io::Error::other(format!("unexpected request: {path}")));
                }
            }
        }
        if session_requests != 2 {
            return Err(std::io::Error::other(format!(
                "expected two bounded pages, got {session_requests} session checks"
            )));
        }
        Ok(())
    }

    async fn fake_forgotten_import_server(
        listener: TcpListener,
        instance_id: Uuid,
    ) -> std::io::Result<()> {
        let lease_id = Uuid::new_v4();
        for expected_path in [
            "/api/v1/connectors/worker/session",
            "/api/v1/connectors/worker/lease",
            "/api/v1/connectors/worker/files/import",
        ] {
            let (mut stream, _) = listener.accept().await?;
            let (request, _, body) = read_request(&mut stream).await?;
            assert!(
                request.starts_with(&format!("POST {expected_path} "))
                    || request.starts_with(&format!("GET {expected_path} "))
            );
            match expected_path {
                "/api/v1/connectors/worker/session" => {
                    let response = serde_json::json!({ "id": instance_id, "connectorId": "io.elembra.connector.shell-memory" });
                    respond(
                        &mut stream,
                        "200 OK",
                        response.to_string().as_bytes(),
                        "application/json",
                    )
                    .await;
                }
                "/api/v1/connectors/worker/lease" => {
                    let response = serde_json::json!({ "leaseId": lease_id, "fence": 3, "checkpointVersion": 0, "expiresAt": chrono::Utc::now() + chrono::Duration::minutes(1), "cursor": null });
                    respond(
                        &mut stream,
                        "200 OK",
                        response.to_string().as_bytes(),
                        "application/json",
                    )
                    .await;
                }
                "/api/v1/connectors/worker/files/import" => {
                    assert!(
                        String::from_utf8_lossy(&body).contains("echo synthetic-already-forgotten")
                    );
                    let response =
                        serde_json::json!({ "targetResourceRef": null, "forgotten": true });
                    respond(
                        &mut stream,
                        "200 OK",
                        response.to_string().as_bytes(),
                        "application/json",
                    )
                    .await;
                }
                _ => unreachable!(),
            }
        }
        let (mut stream, _) = listener.accept().await?;
        let (request, _, _) = read_request(&mut stream).await?;
        assert!(request.starts_with(&format!(
            "POST /api/v1/connectors/worker/lease/{lease_id}/ack "
        )));
        respond(&mut stream, "204 No Content", b"", "application/json").await;
        Ok(())
    }

    #[test]
    fn server_url_rejects_credentials_query_fragment_and_path() {
        for server in [
            "https://user:secret@example.test",
            "https://example.test/?token=secret",
            "https://example.test/#secret",
            "https://example.test/prefix",
        ] {
            let error = normalize_server(server).unwrap_err().to_string();
            assert!(!error.contains("secret") && !error.contains("token"));
        }
    }

    #[test]
    fn server_url_accepts_https_and_loopback_http() {
        assert_eq!(
            normalize_server("https://example.test/").unwrap(),
            "https://example.test"
        );
        assert_eq!(
            normalize_server("http://127.0.0.1:18333").unwrap(),
            "http://127.0.0.1:18333"
        );
        assert!(normalize_server("http://example.test").is_err());
    }

    #[test]
    fn zsh_hook_does_not_capture_forget_control_commands() {
        let directory = tempfile::tempdir().unwrap();
        let log_path = directory.path().join("captured-command");
        let script = r#"
function elembra-shell-memory() {
  case "$1" in
    session-id) print -r -- synthetic-session ;;
    record) cat > "$ELEMBRA_HOOK_CAPTURE_LOG" ;;
  esac
}
source "$ELEMBRA_HOOK_PATH"
_elembra_shell_memory_preexec 'elembra-shell-memory forget-session'
_elembra_shell_memory_precmd
[[ ! -e "$ELEMBRA_HOOK_CAPTURE_LOG" ]] || exit 11
_elembra_shell_memory_preexec 'elembra-shell-memory forget-last'
_elembra_shell_memory_precmd
[[ ! -e "$ELEMBRA_HOOK_CAPTURE_LOG" ]] || exit 12
_elembra_shell_memory_preexec 'elembra-shell-memory --config /tmp/synthetic-config.json forget-session'
_elembra_shell_memory_precmd
[[ ! -e "$ELEMBRA_HOOK_CAPTURE_LOG" ]] || exit 13
_elembra_shell_memory_preexec 'elembra-shell-memory --config=/tmp/synthetic-config.json forget-last'
_elembra_shell_memory_precmd
[[ ! -e "$ELEMBRA_HOOK_CAPTURE_LOG" ]] || exit 14
_elembra_shell_memory_preexec 'echo keep-this-command'
_elembra_shell_memory_precmd
[[ -f "$ELEMBRA_HOOK_CAPTURE_LOG" ]] || exit 15
grep -F 'echo keep-this-command' "$ELEMBRA_HOOK_CAPTURE_LOG" >/dev/null || exit 16
"#;
        let output = Command::new("zsh")
            .args(["-f", "-c", script])
            .env(
                "ELEMBRA_HOOK_PATH",
                concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/shell/elembra-shell-memory.zsh"
                ),
            )
            .env("ELEMBRA_HOOK_CAPTURE_LOG", &log_path)
            .output()
            .expect("zsh is required to validate the supported zsh hook");
        assert!(
            output.status.success(),
            "zsh hook check failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn failure_categories_match_connector_status_error_values() {
        assert_eq!(failure_category(StatusCode::UNAUTHORIZED), "authentication");
        assert_eq!(failure_category(StatusCode::FORBIDDEN), "policy_denied");
        assert_eq!(
            failure_category(StatusCode::TOO_MANY_REQUESTS),
            "rate_limit"
        );
        assert_eq!(failure_category(StatusCode::BAD_REQUEST), "invalid_content");
        assert_eq!(
            failure_category(StatusCode::SERVICE_UNAVAILABLE),
            "source_unavailable"
        );
    }

    #[tokio::test]
    async fn remote_forget_retries_after_local_mapping_write_failure() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let queue_path = directory.path().join("queue.sqlite3");
        let instance_id = Uuid::new_v4();
        let mut queue = Queue::open(&queue_path)?;
        let input = RecordInput {
            command: "echo synthetic-forget-retry".into(),
            cwd: "/tmp/synthetic".into(),
            host: "synthetic-host".into(),
            exit_code: 0,
            duration_ms: 1,
            session_id: None,
        };
        assert!(queue.record(instance_id, input, &[])?);
        let event = queue.pending(instance_id, 1)?.remove(0);
        drop(queue);
        rusqlite::Connection::open(&queue_path)?.execute_batch(
            "CREATE TRIGGER fail_shell_import_mapping BEFORE INSERT ON shell_imports
             BEGIN SELECT RAISE(ABORT, 'simulated local mapping write failure'); END;",
        )?;
        assert!(Queue::open(&queue_path)?
            .remember_import(instance_id, &event)
            .is_err());
        rusqlite::Connection::open(&queue_path)?
            .execute_batch("DROP TRIGGER fail_shell_import_mapping")?;
        let queue = Queue::open(&queue_path)?;
        queue.request_forget(instance_id, event.id)?;
        let source_ref = format!(
            "elembra://io.elembra.connector.shell-memory/file/{}",
            event.id
        );
        let revision = event.revision;
        drop(queue);
        assert!(Queue::open(&queue_path)?
            .pending(instance_id, 1)?
            .is_empty());

        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let config = Config {
            server: format!("http://{}", listener.local_addr()?),
            instance_id,
            queue: queue_path.clone(),
            exclude_commands: vec![],
        };
        let server = tokio::spawn(fake_forget_server(
            listener,
            instance_id,
            source_ref,
            revision,
        ));
        let client = reqwest::Client::new();
        let credential = "ecw1_synthetic";
        let pending = Queue::open(&queue_path)?.pending_forgets(instance_id, 10)?;
        assert!(sync_forgets(&client, &config, credential, &pending)
            .await
            .is_err());
        assert_eq!(
            Queue::open(&queue_path)?
                .pending_forgets(instance_id, 10)?
                .len(),
            1
        );
        let pending = Queue::open(&queue_path)?.pending_forgets(instance_id, 10)?;
        sync_forgets(&client, &config, credential, &pending).await?;
        assert!(Queue::open(&queue_path)?
            .pending_forgets(instance_id, 10)?
            .is_empty());
        server.await??;
        Ok(())
    }

    #[tokio::test]
    async fn remote_forget_drains_more_than_one_bounded_page_while_paused_and_disabled(
    ) -> Result<()> {
        const RECORD_COUNT: usize = 101;
        let directory = tempfile::tempdir()?;
        let queue_path = directory.path().join("queue.sqlite3");
        let instance_id = Uuid::new_v4();
        let session_id = Uuid::new_v4();
        let mut queue = Queue::open(&queue_path)?;
        for index in 0..RECORD_COUNT {
            assert!(queue.record(
                instance_id,
                RecordInput {
                    command: format!("echo synthetic-forget-batch-{index}"),
                    cwd: "/tmp/synthetic".into(),
                    host: "synthetic-host".into(),
                    exit_code: 0,
                    duration_ms: 1,
                    session_id: Some(session_id),
                },
                &[],
            )?);
            let event = queue.pending(instance_id, 1)?.remove(0);
            queue.remember_import(instance_id, &event)?;
            queue.acknowledge(event.id)?;
        }
        assert_eq!(
            queue.request_forget_session(instance_id, session_id)?,
            RECORD_COUNT
        );
        assert!(queue.pending(instance_id, RECORD_COUNT)?.is_empty());
        queue.set_paused(true)?;
        queue.set_disabled(true)?;
        drop(queue);

        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let config = Config {
            server: format!("http://{}", listener.local_addr()?),
            instance_id,
            queue: queue_path.clone(),
            exclude_commands: vec![],
        };
        let server = tokio::spawn(fake_forget_batch_server(
            listener,
            instance_id,
            RECORD_COUNT,
        ));
        let forgotten = sync_all_pending_forgets(
            &reqwest::Client::new(),
            &config,
            "ecw1_synthetic-forget-batch",
        )
        .await?;
        assert_eq!(forgotten, RECORD_COUNT);
        server.await??;

        let queue = Queue::open(&queue_path)?;
        assert!(queue.pending_forgets(instance_id, RECORD_COUNT)?.is_empty());
        assert!(queue.list(instance_id, RECORD_COUNT)?.is_empty());
        assert!(queue.paused());
        assert!(queue.disabled());
        Ok(())
    }

    #[tokio::test]
    async fn forgotten_import_replay_is_acknowledged_without_retaining_a_mapping() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let queue_path = directory.path().join("queue.sqlite3");
        let instance_id = Uuid::new_v4();
        assert!(record_if_enabled(
            &queue_path,
            instance_id,
            RecordInput {
                command: "echo synthetic-already-forgotten".into(),
                cwd: "/tmp/synthetic".into(),
                host: "synthetic-host".into(),
                exit_code: 0,
                duration_ms: 1,
                session_id: None,
            },
            &[],
        )?);
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let config = Config {
            server: format!("http://{}", listener.local_addr()?),
            instance_id,
            queue: queue_path.clone(),
            exclude_commands: vec![],
        };
        let server = tokio::spawn(fake_forgotten_import_server(listener, instance_id));
        assert_eq!(
            upload_round(&reqwest::Client::new(), &config, || {
                Ok("ecw1_synthetic".into())
            })
            .await?,
            UploadRound::Completed
        );
        server.await??;
        let queue = Queue::open(&queue_path)?;
        assert!(queue.pending(instance_id, 10)?.is_empty());
        assert!(queue.list(instance_id, 10)?.is_empty());
        assert!(queue.pending_forgets(instance_id, 10)?.is_empty());
        Ok(())
    }

    #[test]
    fn no_lease_uses_backoff() {
        assert_eq!(UploadRound::NoLease.next_delay(), Duration::from_secs(15));
        assert_eq!(
            UploadRound::Completed.next_delay(),
            Duration::from_millis(100)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn no_lease_rounds_reuse_pending_event_and_reread_rotated_credentials() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let queue_path = directory.path().join("queue.sqlite3");
        let instance_id = Uuid::new_v4();
        let config = Config {
            server: String::new(),
            instance_id,
            queue: queue_path.clone(),
            exclude_commands: Vec::new(),
        };
        assert!(record_if_enabled(
            &queue_path,
            instance_id,
            RecordInput {
                command: "echo pending-without-lease".into(),
                cwd: "/tmp/synthetic".into(),
                host: "synthetic-host".into(),
                exit_code: 0,
                duration_ms: 1,
                session_id: None,
            },
            &[]
        )?);

        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let mut config = config;
        config.server = format!("http://{}", listener.local_addr()?);
        let (authorization_tx, mut authorization_rx) = mpsc::channel(2);
        let server = tokio::spawn(fake_connector_without_lease(
            listener,
            instance_id,
            2,
            authorization_tx,
        ));
        let client = reqwest::Client::new();

        assert_eq!(
            upload_round(&client, &config, || Ok("ecw1_synthetic_first".into())).await?,
            UploadRound::NoLease
        );
        assert_eq!(
            upload_round(&client, &config, || Ok("ecw1_synthetic_rotated".into())).await?,
            UploadRound::NoLease
        );
        server.await??;

        assert_eq!(
            authorization_rx.recv().await.as_deref(),
            Some("Connector ecw1_synthetic_first")
        );
        assert_eq!(
            authorization_rx.recv().await.as_deref(),
            Some("Connector ecw1_synthetic_rotated")
        );
        let queue = Queue::open(&queue_path)?;
        assert_eq!(queue.pending(instance_id, 10)?.len(), 1);
        assert_eq!(UploadRound::NoLease.next_delay(), Duration::from_secs(15));
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn forgotten_import_replay_is_acknowledged_and_removed_from_queue() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let queue_path = directory.path().join("queue.sqlite3");
        let instance_id = Uuid::new_v4();
        assert!(record_if_enabled(
            &queue_path,
            instance_id,
            RecordInput {
                command: "echo synthetic-tombstoned-command".into(),
                cwd: "/tmp/synthetic".into(),
                host: "synthetic-host".into(),
                exit_code: 0,
                duration_ms: 1,
                session_id: None,
            },
            &[]
        )?);
        let mut config = Config {
            server: String::new(),
            instance_id,
            queue: queue_path.clone(),
            exclude_commands: Vec::new(),
        };
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        config.server = format!("http://{}", listener.local_addr()?);
        let (import_started_tx, mut import_started_rx) = mpsc::channel(1);
        let (import_release_tx, import_release_rx) = oneshot::channel();
        let (acknowledged_tx, acknowledged_rx) = oneshot::channel();
        let server = tokio::spawn(fake_connector(
            listener,
            instance_id,
            true,
            import_started_tx,
            import_release_rx,
            acknowledged_tx,
        ));
        let client = reqwest::Client::new();
        let upload_config = config.clone();
        let uploader = tokio::spawn(async move {
            upload_round(&client, &upload_config, || {
                Ok("ecw1_synthetic_worker_credential".into())
            })
            .await
        });
        let import_body = tokio::time::timeout(Duration::from_secs(2), import_started_rx.recv())
            .await?
            .expect("fake server receives the tombstoned import replay");
        assert!(String::from_utf8_lossy(&import_body).contains("synthetic-tombstoned-command"));
        import_release_tx
            .send(())
            .expect("release forgotten response");
        assert_eq!(uploader.await??, UploadRound::Completed);
        tokio::time::timeout(Duration::from_secs(2), acknowledged_rx)
            .await?
            .expect("fake server observes terminal checkpoint acknowledgment");
        server.await??;

        let queue = Queue::open(&queue_path)?;
        assert!(queue.pending(instance_id, 10)?.is_empty());
        assert_eq!(
            upload_round(&reqwest::Client::new(), &config, || {
                anyhow::bail!("credential must not be read after terminal acknowledgment")
            })
            .await?,
            UploadRound::Idle
        );
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn forget_waits_for_real_upload_round_while_record_stays_unblocked() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let queue_path = directory.path().join("queue.sqlite3");
        let instance_id = Uuid::new_v4();
        let config = Config {
            server: String::new(),
            instance_id,
            queue: queue_path.clone(),
            exclude_commands: Vec::new(),
        };
        assert!(record_if_enabled(
            &queue_path,
            instance_id,
            RecordInput {
                command: "echo synthetic-upload".into(),
                cwd: "/tmp/synthetic".into(),
                host: "synthetic-host".into(),
                exit_code: 0,
                duration_ms: 1,
                session_id: None,
            },
            &[]
        )?);
        assert!(record_if_enabled(
            &queue_path,
            instance_id,
            RecordInput {
                command: "echo synthetic-next-round".into(),
                cwd: "/tmp/synthetic".into(),
                host: "synthetic-host".into(),
                exit_code: 0,
                duration_ms: 1,
                session_id: None,
            },
            &[]
        )?);

        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let mut config = config;
        config.server = format!("http://{address}");
        let (import_started_tx, mut import_started_rx) = mpsc::channel(1);
        let (import_release_tx, import_release_rx) = oneshot::channel();
        let (acknowledged_tx, acknowledged_rx) = oneshot::channel();
        let server = tokio::spawn(fake_connector(
            listener,
            instance_id,
            false,
            import_started_tx,
            import_release_rx,
            acknowledged_tx,
        ));
        let client = reqwest::Client::new();
        let upload_config = config.clone();
        let uploader = tokio::spawn(async move {
            upload_round(&client, &upload_config, || {
                Ok("synthetic-worker-credential".into())
            })
            .await
        });
        let import_body = tokio::time::timeout(Duration::from_secs(2), import_started_rx.recv())
            .await?
            .expect("fake server receives Files import");
        let import_body = String::from_utf8_lossy(&import_body);
        assert!(import_body.contains("echo synthetic-upload"));
        assert!(!import_body.contains("echo synthetic-next-round"));
        assert!(import_body.contains("io.elembra.connector.shell-memory"));

        let record_path = queue_path.clone();
        let record_task = tokio::task::spawn_blocking(move || {
            record_if_enabled(
                &record_path,
                instance_id,
                RecordInput {
                    command: "echo queued-during-upload".into(),
                    cwd: "/tmp/synthetic".into(),
                    host: "synthetic-host".into(),
                    exit_code: 0,
                    duration_ms: 2,
                    session_id: None,
                },
                &[],
            )
        });
        let recorded = tokio::time::timeout(Duration::from_secs(2), record_task)
            .await
            .expect("zsh capture is not blocked by HTTP")??;
        assert!(recorded);

        let forget_path = queue_path.clone();
        let (forget_attempt_tx, forget_attempt_rx) = oneshot::channel();
        let (forget_acquired_tx, mut forget_acquired_rx) = mpsc::channel(1);
        let forgetter = tokio::task::spawn_blocking(move || -> Result<()> {
            let _ = forget_attempt_tx.send(());
            let _upload_lock = UploadLock::acquire(&forget_path)?;
            let _queue_lock = QueueLock::acquire(&forget_path)?;
            let _ = forget_acquired_tx.blocking_send(());
            Queue::open(&forget_path)?.forget()
        });
        tokio::time::timeout(Duration::from_secs(2), forget_attempt_rx)
            .await?
            .expect("forget task started");
        assert!(
            tokio::time::timeout(Duration::from_millis(50), forget_acquired_rx.recv())
                .await
                .is_err()
        );
        assert!(!forgetter.is_finished());

        import_release_tx
            .send(())
            .expect("release fake import response");
        assert_eq!(uploader.await??, UploadRound::Completed);
        tokio::time::timeout(Duration::from_secs(2), acknowledged_rx)
            .await?
            .expect("fake server observes worker ack");
        forgetter.await??;
        server.await??;

        assert!(Queue::is_disabled(&queue_path)?);
        assert!(!queue_path.exists());
        assert!(!record_if_enabled(
            &queue_path,
            instance_id,
            RecordInput {
                command: "echo after-forget".into(),
                cwd: "/tmp/synthetic".into(),
                host: "synthetic-host".into(),
                exit_code: 0,
                duration_ms: 3,
                session_id: None,
            },
            &[]
        )?);
        assert!(!queue_path.exists());
        assert_eq!(
            upload_round(&reqwest::Client::new(), &config, || {
                anyhow::bail!("credential must not be read after forget")
            })
            .await?,
            UploadRound::Disabled
        );
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn pause_disable_and_forget_stop_later_imports_in_the_current_batch() -> Result<()> {
        for (control, expected) in [
            (Some(Control::Pause), UploadResult::Paused),
            (Some(Control::Disable), UploadResult::Disabled),
            (None, UploadResult::Disabled),
        ] {
            let directory = tempfile::tempdir()?;
            let queue_path = directory.path().join("queue.sqlite3");
            let instance_id = Uuid::new_v4();
            let config = Config {
                server: String::new(),
                instance_id,
                queue: queue_path.clone(),
                exclude_commands: Vec::new(),
            };
            for command in ["echo first-queued-command", "echo second-queued-command"] {
                assert!(record_if_enabled(
                    &queue_path,
                    instance_id,
                    RecordInput {
                        command: command.into(),
                        cwd: "/tmp/synthetic".into(),
                        host: "synthetic-host".into(),
                        exit_code: 0,
                        duration_ms: 1,
                        session_id: None,
                    },
                    &[]
                )?);
            }
            let events = Queue::open(&queue_path)?.pending(instance_id, 2)?;
            let listener = TcpListener::bind("127.0.0.1:0").await?;
            let mut config = config;
            config.server = format!("http://{}", listener.local_addr()?);
            let (import_started_tx, mut import_started_rx) = mpsc::channel(2);
            let (import_release_tx, import_release_rx) = oneshot::channel();
            let (acknowledged_tx, acknowledged_rx) = oneshot::channel();
            let server = tokio::spawn(fake_connector(
                listener,
                instance_id,
                false,
                import_started_tx,
                import_release_rx,
                acknowledged_tx,
            ));
            let upload_config = config.clone();
            let uploader = tokio::spawn(async move {
                let _upload_lock = UploadLock::acquire(&upload_config.queue)?;
                upload_pending(
                    &reqwest::Client::new(),
                    &upload_config,
                    "ecw1_synthetic_worker_credential",
                    events,
                )
                .await
            });
            let first_import =
                tokio::time::timeout(Duration::from_secs(2), import_started_rx.recv())
                    .await?
                    .expect("fake server receives the first queued event");
            assert!(String::from_utf8_lossy(&first_import).contains("first-queued-command"));

            let config_path = directory.path().join("config.json");
            std::fs::write(&config_path, serde_json::to_vec(&config)?)?;
            let control_config = config.clone();
            let control_path = config_path.clone();
            let controller = tokio::task::spawn_blocking(move || match control {
                Some(control) => set_control(&control_path, control, true),
                None => forget_queue(&control_config),
            });
            let marker_path = queue_path.with_extension(if control == Some(Control::Pause) {
                "paused"
            } else {
                "disabled"
            });
            tokio::time::timeout(Duration::from_secs(2), async {
                while !marker_path.exists() {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await
            .expect("control marker is visible before waiting for the upload lock");

            import_release_tx
                .send(())
                .expect("release the in-flight import response");
            assert_eq!(uploader.await??, expected);
            tokio::time::timeout(Duration::from_secs(2), acknowledged_rx)
                .await?
                .expect("the completed prefix is acknowledged");
            controller.await??;
            server.await??;
            assert!(
                import_started_rx.try_recv().is_err(),
                "no later event reached the Files import endpoint"
            );

            if control.is_none() {
                assert!(!queue_path.exists());
            } else {
                assert_eq!(Queue::open(&queue_path)?.pending(instance_id, 2)?.len(), 1);
            }
        }
        Ok(())
    }

    #[tokio::test]
    #[ignore = "requires disposable PostgreSQL and RustFS"]
    async fn connected_upload_forget_reconcile_and_replay_stays_absent() -> Result<()> {
        let (mut state, _) = server_common::setup_test_server().await;
        let pool = state.db_pool.clone();
        let fixture = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let username = format!("shell-memory-{fixture}");
        let email = format!("shell-memory-{fixture}@example.test");
        sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, $2)")
            .bind(tenant_id)
            .bind(format!("shell-memory-{fixture}"))
            .execute(&pool)
            .await?;
        sqlx::query("INSERT INTO users (id, username, email, password_hash, tenant_id, display_name, storage_quota) VALUES ($1,$2,$3,'test-hash',$4,'Shell Memory owner',10737418240)")
            .bind(owner_id)
            .bind(&username)
            .bind(&email)
            .bind(tenant_id)
            .execute(&pool)
            .await?;

        let registry = ApplicationRegistry::first_party()?;
        let mut owners = ResourceOwnerRegistry::new();
        owners.register(
            Arc::new(FilesResourceOwner::new(
                Arc::clone(&state.permission_resolver),
                Arc::new(PermissionResolverRepository::new(pool.clone())),
                Arc::clone(&state.metadata_store),
                Arc::clone(&state.object_store),
            )),
            &registry,
        )?;
        state.source_authorizer = Arc::new(SourceAuthorizer::new(owners));

        let store: Arc<dyn VectorStore> = Arc::new(PgVectorStore::new(pool.clone()));
        let keyword_indexer = Arc::new(KeywordIndexer::new(store));
        let note_sink = Arc::new(KeywordIndexerNoteSink::new(Arc::clone(&keyword_indexer)));
        state.note_service = Arc::new(
            (*state.note_service)
                .clone()
                .with_index_sink(Some(note_sink)),
        );
        state.unified_search_service = Arc::new(
            UnifiedSearchService::new(
                Arc::clone(&state.source_authorizer),
                Arc::clone(&state.metadata_store),
                None,
                Arc::clone(&state.memory_catalog_store),
            )
            .with_keyword_indexer(keyword_indexer, Arc::clone(&state.permission_resolver))
            .with_memory_files_catalog(Arc::new(MemoryFilesCatalogStore::new(pool.clone()))),
        );

        let worker = register_worker(&state, tenant_id, owner_id).await?;
        let token = state.jwt_manager.generate(owner_id, &email, tenant_id)?;
        let app = routes::connector_routes()
            .merge(routes::search_routes())
            .with_state(state.clone());
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let server_url = format!("http://{address}");
        let server = tokio::spawn(async move { axum::serve(listener, app).await });
        let client = reqwest::Client::new();
        let directory = tempfile::tempdir()?;
        let queue_path = directory.path().join("shell-memory.sqlite3");
        let config = Config {
            server: server_url.clone(),
            instance_id: worker.instance_id,
            queue: queue_path.clone(),
            exclude_commands: vec![],
        };
        let marker = format!("shellmemorymarker{}", Uuid::new_v4().simple());
        let mut queue = Queue::open(&queue_path)?;
        assert!(queue.record(
            worker.instance_id,
            RecordInput {
                command: format!("printf {marker}"),
                cwd: "/tmp/synthetic-shell-memory".into(),
                host: "synthetic-host".into(),
                exit_code: 0,
                duration_ms: 1,
                session_id: None,
            },
            &[],
        )?);
        let event = queue.pending(worker.instance_id, 1)?.remove(0);
        drop(queue);

        assert_eq!(
            upload_round(&client, &config, || Ok(worker.credential.clone())).await?,
            UploadRound::Completed
        );
        let source_ref = format!(
            "elembra://io.elembra.connector.shell-memory/file/{}",
            event.id
        );
        let target_uri: String = sqlx::query_scalar(
            "SELECT target_resource_ref FROM connector_external_mappings WHERE tenant_id=$1 AND instance_id=$2 AND external_resource_ref=$3 AND external_revision=$4",
        )
        .bind(tenant_id)
        .bind(worker.instance_id)
        .bind(&source_ref)
        .bind(&event.revision)
        .fetch_one(&pool)
        .await?;
        let target = rustshare_resource_auth::ResourceRef::from_uri(&target_uri)?;
        let file_id = Uuid::parse_str(&target.resource_id)?;
        let storage_key: String = sqlx::query_scalar("SELECT storage_key FROM files WHERE id=$1")
            .bind(file_id)
            .fetch_one(&pool)
            .await?;

        reconcile(&state, tenant_id).await?;
        let before = search(&client, &server_url, &token, &marker).await?;
        let results = before["results"]
            .as_array()
            .context("search response has no results")?;
        let citation = results
            .iter()
            .find(|result| {
                result["snippet"]
                    .as_str()
                    .unwrap_or_default()
                    .contains(&marker)
            })
            .and_then(|result| result["resource_ref"].as_str())
            .context("published shell command is not searchable with a source citation")?
            .to_owned();
        let opened = client
            .post(format!("{server_url}/api/v1/memory/citations/open"))
            .bearer_auth(&token)
            .json(&serde_json::json!({ "resource_ref": citation }))
            .send()
            .await?;
        anyhow::ensure!(
            opened.status().is_success(),
            "authorized citation did not open: {}",
            opened.status()
        );

        Queue::open(&queue_path)?.request_forget(worker.instance_id, event.id)?;
        assert_eq!(
            sync_all_pending_forgets(&client, &config, &worker.credential).await?,
            1
        );
        let deleted_at: Option<chrono::DateTime<chrono::Utc>> =
            sqlx::query_scalar("SELECT deleted_at FROM files WHERE id=$1")
                .bind(file_id)
                .fetch_one(&pool)
                .await?;
        assert!(
            deleted_at.is_some(),
            "remote forget soft-deletes its Files target"
        );

        reconcile(&state, tenant_id).await?;
        let after = search(&client, &server_url, &token, &marker).await?;
        assert!(after["results"].as_array().is_some_and(Vec::is_empty));
        let citation_after = client
            .post(format!("{server_url}/api/v1/memory/citations/open"))
            .bearer_auth(&token)
            .json(&serde_json::json!({ "resource_ref": citation }))
            .send()
            .await?;
        assert_eq!(citation_after.status(), StatusCode::NOT_FOUND);
        let index_rows: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM note_index_chunks WHERE tenant_id=$1 AND source_file_id=$2",
        )
        .bind(tenant_id)
        .bind(file_id)
        .fetch_one(&pool)
        .await?;
        assert_eq!(
            index_rows, 0,
            "reconcile removes the forgotten search projection"
        );
        let catalog_deleted: bool = sqlx::query_scalar(
            "SELECT deleted_at IS NOT NULL FROM memory_files_catalog WHERE tenant_id=$1 AND file_id=$2",
        )
        .bind(tenant_id)
        .bind(file_id)
        .fetch_one(&pool)
        .await?;
        assert!(catalog_deleted, "Memory catalog retains a tombstone");

        assert_eq!(
            upload_pending(&client, &config, &worker.credential, vec![event.clone()],).await?,
            UploadResult::Completed
        );
        let file_count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM files WHERE tenant_id=$1 AND id=$2")
                .bind(tenant_id)
                .bind(file_id)
                .fetch_one(&pool)
                .await?;
        assert_eq!(
            file_count, 1,
            "forgotten event replay does not create another Files target"
        );
        reconcile(&state, tenant_id).await?;
        let replayed = search(&client, &server_url, &token, &marker).await?;
        assert!(replayed["results"].as_array().is_some_and(Vec::is_empty));
        let replay_tombstone: bool = sqlx::query_scalar(
            "SELECT deleted_at IS NOT NULL FROM memory_files_catalog WHERE tenant_id=$1 AND file_id=$2",
        )
        .bind(tenant_id)
        .bind(file_id)
        .fetch_one(&pool)
        .await?;
        assert!(replay_tombstone, "replay leaves Memory's tombstone intact");
        let mapping_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM connector_external_mappings WHERE tenant_id=$1 AND instance_id=$2 AND external_resource_ref=$3 AND external_revision=$4",
        )
        .bind(tenant_id)
        .bind(worker.instance_id)
        .bind(&source_ref)
        .bind(&event.revision)
        .fetch_one(&pool)
        .await?;
        assert_eq!(mapping_count, 1, "replay preserves one source mapping");
        let catalog_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM memory_files_catalog WHERE tenant_id=$1 AND connector_instance_id=$2 AND source_resource_ref=$3 AND source_revision=$4 AND deleted_at IS NOT NULL",
        )
        .bind(tenant_id)
        .bind(worker.instance_id)
        .bind(&source_ref)
        .bind(&event.revision)
        .fetch_one(&pool)
        .await?;
        assert_eq!(
            catalog_count, 1,
            "replay preserves one tombstoned catalog row"
        );
        let owner_files: i64 =
            sqlx::query_scalar("SELECT count(*) FROM files WHERE tenant_id=$1 AND owner_id=$2")
                .bind(tenant_id)
                .bind(owner_id)
                .fetch_one(&pool)
                .await?;
        assert_eq!(
            owner_files, 1,
            "replay must not create a second Files target"
        );

        server.abort();
        let _ = state.object_store.delete(&storage_key).await;
        sqlx::query("DELETE FROM connector_instances WHERE tenant_id=$1 AND id=$2")
            .bind(tenant_id)
            .bind(worker.instance_id)
            .execute(&pool)
            .await?;
        sqlx::query(
            "DELETE FROM memory_files_source_access_audit WHERE tenant_id=$1 AND file_id=$2",
        )
        .bind(tenant_id)
        .bind(file_id)
        .execute(&pool)
        .await?;
        sqlx::query("DELETE FROM note_index_chunks WHERE tenant_id=$1 AND source_file_id=$2")
            .bind(tenant_id)
            .bind(file_id)
            .execute(&pool)
            .await?;
        sqlx::query("DELETE FROM memory_files_catalog WHERE tenant_id=$1 AND file_id=$2")
            .bind(tenant_id)
            .bind(file_id)
            .execute(&pool)
            .await?;
        sqlx::query("DELETE FROM files WHERE id=$1")
            .bind(file_id)
            .execute(&pool)
            .await?;
        sqlx::query("DELETE FROM users WHERE id=$1")
            .bind(owner_id)
            .execute(&pool)
            .await?;
        sqlx::query("DELETE FROM tenants WHERE id=$1")
            .bind(tenant_id)
            .execute(&pool)
            .await?;
        Ok(())
    }
}
