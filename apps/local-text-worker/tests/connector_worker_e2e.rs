use base64::Engine as _;
use reqwest::{Client, StatusCode};
use rusqlite::Connection;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::postgres::PgPoolOptions;
use std::{
    collections::BTreeMap,
    env,
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    process::{Child, Command, Stdio},
    time::Duration,
};
use tempfile::TempDir;
use uuid::Uuid;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

struct Worker(Child);

impl Drop for Worker {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

impl Worker {
    fn stop_gracefully(&mut self) {
        signal_process(self.0.id(), "-TERM");
        self.0.wait().expect("wait for graceful worker shutdown");
    }

    fn kill(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct ResumeOnDrop(u32);

impl Drop for ResumeOnDrop {
    fn drop(&mut self) {
        let _ = Command::new("kill")
            .args(["-CONT", &self.0.to_string()])
            .status();
    }
}

fn signal_process(pid: u32, signal: &str) {
    assert!(Command::new("kill")
        .args([signal, &pid.to_string()])
        .status()
        .expect("send process signal")
        .success());
}

fn spawn_worker(
    instance_id: &str,
    server: &str,
    credential_file: &Path,
    queue: &Path,
    root: &Path,
) -> Worker {
    Worker(
        Command::new(env!("CARGO_BIN_EXE_elembra-local-text-worker"))
            .args([
                "--instance-id",
                instance_id,
                "--server",
                server,
                "--credential-file",
            ])
            .arg(credential_file)
            .arg("--queue")
            .arg(queue)
            .arg("--root")
            .arg(root)
            .args(["--reconcile-seconds", "1"])
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("start independent Local Text worker process"),
    )
}

async fn spawn_memory_worker(server: &str, wait_until_ready: bool) -> Worker {
    let worker = env::var("RUSTSHARE_MEMORY_WORKER_BIN").expect("set Memory worker binary path");
    let health_listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let health_addr = health_listener.local_addr().unwrap();
    drop(health_listener);
    let mut worker = Worker(
        Command::new(worker)
            .env("RUSTSHARE_MEMORY_EXECUTION_MODE", "external")
            .env("RUST_LOG", "warn")
            .env(
                "MEMORY_DATABASE_URL",
                env::var("MEMORY_DATABASE_URL").unwrap(),
            )
            .env(
                "RUSTSHARE_MEMORY_WORKER_SECRET",
                env::var("RUSTSHARE_MEMORY_WORKER_SECRET").unwrap(),
            )
            .env("RUSTSHARE_API_BASE_URL", server)
            .env("RUSTSHARE_AI_ENABLED", "false")
            .env("RUSTSHARE_OUTBOX_WORKER_ENABLED", "true")
            .env("RUSTSHARE_OUTBOX_POLL_INTERVAL_MS", "250")
            .env("RUSTSHARE_OUTBOX_CLAIM_BATCH_SIZE", "1")
            .env("RUSTSHARE_OUTBOX_LEASE_SECS", "10")
            .env("RUSTSHARE_OUTBOX_PROCESS_TIMEOUT_SECS", "9")
            .env("RUSTSHARE_OUTBOX_READINESS_STALENESS_SECS", "60")
            .env("RUSTSHARE_OUTBOX_RETENTION_HOURS", "0")
            .env("MEMORY_WORKER_HEALTH_ADDR", health_addr.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("start separate Elembra Memory worker process"),
    );
    let health_path = if wait_until_ready { "ready" } else { "health" };
    let health_url = format!("http://{health_addr}/{health_path}");
    let client = Client::new();
    for _ in 0..120 {
        if let Some(status) = worker.0.try_wait().expect("check Memory worker startup") {
            panic!("Memory worker exited before readiness: {status}");
        }
        if client
            .get(&health_url)
            .send()
            .await
            .is_ok_and(|response| response.status() == StatusCode::OK)
        {
            return worker;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    panic!("timed out waiting for Memory worker {health_path}");
}

fn spawn_backend(database_url: &str, log_path: &Path) -> Worker {
    let binary = env::var("RUSTSHARE_CONNECTOR_TEST_BACKEND_BIN")
        .expect("set the disposable backend binary path");
    let log = fs::File::create(log_path).expect("create restarted backend log");
    Worker(
        Command::new(binary)
            .env("DATABASE_URL", database_url)
            .env("SERVER_HOST", "127.0.0.1")
            .env("SERVER_PORT", "18333")
            .env("RUSTSHARE_CHAT_AUTHORITY", "local")
            .env("RUSTSHARE_CHAT_PROVISIONING", "manual")
            .env("RUSTSHARE_MEMORY_EXECUTION_MODE", "external")
            .env(
                "RUSTSHARE_MEMORY_WORKER_SECRET",
                env::var("RUSTSHARE_MEMORY_WORKER_SECRET").unwrap(),
            )
            .stdout(Stdio::from(log))
            .stderr(Stdio::null())
            .spawn()
            .expect("restart backend while Memory worker remains active"),
    )
}

async fn wait_for_backend(client: &Client, server: &str, ready: bool) {
    for _ in 0..150 {
        let result = client
            .get(format!(
                "{server}/health{}",
                if ready { "/ready" } else { "" }
            ))
            .send()
            .await;
        let matches = result.map_or(!ready, |response| response.status().is_success() == ready);
        if matches {
            return;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    panic!("timed out waiting for backend readiness change");
}

fn save_credential(path: &Path, credential: &str) {
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)
        .expect("create worker credential file");
    file.write_all(credential.as_bytes())
        .expect("write worker credential file");
    #[cfg(unix)]
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .expect("restrict worker credential file");
}

fn queue_pending(queue: &Path, instance_id: &str) -> i64 {
    Connection::open(queue)
        .expect("open worker queue")
        .query_row(
            "SELECT count(*) FROM local_text_queue WHERE instance_id = ?1 AND acknowledged = 0",
            [instance_id],
            |row| row.get(0),
        )
        .expect("read pending queue count")
}

fn revision(content: &[u8]) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(content)))
}

async fn imported_files(
    client: &Client,
    server: &str,
    owner_token: &str,
    scope: &str,
) -> Vec<Value> {
    let response = client
        .get(format!("{server}/api/v1/files"))
        .bearer_auth(owner_token)
        .send()
        .await
        .expect("request Files list");
    assert_eq!(response.status(), StatusCode::OK);
    response
        .json::<Vec<Value>>()
        .await
        .expect("decode Files list")
        .into_iter()
        .filter(|file| {
            file["path"]
                .as_str()
                .is_some_and(|path| path.starts_with(&format!("/{scope}/")))
        })
        .collect()
}

async fn wait_for_versions(
    client: &Client,
    server: &str,
    owner_token: &str,
    scope: &str,
    phase: &str,
    queue: (&Path, &str),
    expected: &[(&str, i64)],
) -> Vec<Value> {
    let mut last_versions = BTreeMap::new();
    for _ in 0..150 {
        let files = imported_files(client, server, owner_token, scope).await;
        last_versions = files
            .iter()
            .filter_map(|file| {
                Some((
                    file["path"].as_str()?.to_owned(),
                    file["current_version"].as_i64()?,
                ))
            })
            .collect();
        if expected.iter().all(|(path, version)| {
            last_versions
                .get(*path)
                .is_some_and(|found| found >= version)
        }) {
            return files;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    panic!("timed out waiting for Local Text Files import during {phase}; expected {expected:?}, observed {last_versions:?}, queued observations {}", queue_pending(queue.0, queue.1));
}

async fn connector_events(pool: &sqlx::PgPool, instance_id: &str) -> Vec<Value> {
    sqlx::query_scalar(
        "SELECT event_json->'data'->'sourceProvenance' FROM integration_outbox \
         WHERE application_id = 'io.elembra.files' \
           AND event_json->'data'->'sourceProvenance'->>'connector_instance_id' = $1 \
         ORDER BY created_at",
    )
    .bind(instance_id)
    .fetch_all(pool)
    .await
    .expect("read Files publication provenance")
}

async fn connector_mappings(
    pool: &sqlx::PgPool,
    instance_id: Uuid,
) -> Vec<(String, String, String)> {
    sqlx::query_as(
        "SELECT external_resource_ref, external_revision, target_resource_ref \
         FROM connector_external_mappings WHERE instance_id = $1 ORDER BY external_revision",
    )
    .bind(instance_id)
    .fetch_all(pool)
    .await
    .expect("read Connector-to-Files mappings")
}

async fn wait_for_connector_mappings(
    pool: &sqlx::PgPool,
    instance_id: Uuid,
    expected_count: usize,
    phase: &str,
) -> Vec<(String, String, String)> {
    let mut mappings = Vec::new();
    for _ in 0..150 {
        mappings = connector_mappings(pool, instance_id).await;
        if mappings.len() >= expected_count {
            return mappings;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    panic!(
        "timed out waiting for Connector mappings during {phase}; expected at least {expected_count}, observed {}",
        mappings.len()
    );
}

async fn wait_for_memory_projection(
    pool: &sqlx::PgPool,
    file_id: Uuid,
    expected_version: &str,
    expected_revision: &str,
    expected_version_number: i64,
) {
    for _ in 0..150 {
        let projected: Option<(Option<String>, Option<String>, Option<i64>)> = sqlx::query_as(
            "SELECT current_version, source_revision, current_version_number \
             FROM memory_files_catalog WHERE file_id = $1 AND deleted_at IS NULL",
        )
        .bind(file_id)
        .fetch_optional(pool)
        .await
        .expect("read Memory Files provenance projection");
        let chunks: i64 =
            sqlx::query_scalar("SELECT count(*) FROM note_index_chunks WHERE source_file_id = $1")
                .bind(file_id)
                .fetch_one(pool)
                .await
                .expect("read rebuildable Memory keyword projection");
        if projected.is_some_and(|(version, revision, number)| {
            version.as_deref() == Some(expected_version)
                && revision.as_deref() == Some(expected_revision)
                && number == Some(expected_version_number)
        }) && chunks > 0
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    let catalog: Option<(Option<String>, Option<String>, Option<i64>)> = sqlx::query_as(
        "SELECT current_version, source_revision, current_version_number \
         FROM memory_files_catalog WHERE file_id=$1 AND deleted_at IS NULL",
    )
    .bind(file_id)
    .fetch_optional(pool)
    .await
    .expect("inspect timed-out Memory catalog state");
    let chunk_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM note_index_chunks WHERE source_file_id=$1")
            .bind(file_id)
            .fetch_one(pool)
            .await
            .expect("inspect timed-out Memory search projection");
    let deliveries: Vec<(String, String, i32)> = sqlx::query_as(
        "SELECT d.event_type, d.state, d.attempt_count FROM integration_deliveries d \
         JOIN integration_outbox o USING (source, event_id) \
         WHERE d.consumer_id='io.elembra.memory.files-connector.v1' \
           AND o.event_json->'elembraResource'->>'resourceId'=$1 \
         ORDER BY d.available_at DESC LIMIT 10",
    )
    .bind(file_id.to_string())
    .fetch_all(pool)
    .await
    .expect("inspect recent Memory delivery state");
    let tenant_id: Option<Uuid> = sqlx::query_scalar("SELECT tenant_id FROM files WHERE id=$1")
        .bind(file_id)
        .fetch_optional(pool)
        .await
        .expect("inspect Files tenant");
    let reconcile = if let Some(tenant_id) = tenant_id {
        sqlx::query_as::<_, (String, i32, i64, i64, i64, i64, Option<String>)>(
            "SELECT status, retry_count, processed, indexed, removed, unavailable, last_error \
             FROM memory_files_reconcile_jobs WHERE tenant_id=$1",
        )
        .bind(tenant_id)
        .fetch_optional(pool)
        .await
        .expect("inspect Memory reconciliation state")
    } else {
        None
    };
    let source_audits: Vec<(String, String)> = sqlx::query_as(
        "SELECT outcome, file_version FROM memory_files_source_access_audit \
         WHERE file_id=$1 ORDER BY occurred_at DESC LIMIT 3",
    )
    .bind(file_id)
    .fetch_all(pool)
    .await
    .expect("inspect content-free Memory source access outcomes");
    panic!(
        "timed out waiting for authorized Memory Files projection: catalog={catalog:?}, chunks={chunk_count}, recent_deliveries={deliveries:?}, reconcile={reconcile:?}, source_audits={source_audits:?}"
    );
}

async fn insert_memory_search_fixture(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    file_id: Uuid,
    read_principal: &str,
) -> Uuid {
    let id = Uuid::new_v4();
    let rows = sqlx::query(
        "INSERT INTO note_index_chunks (id,tenant_id,workspace_id,source_folder_id,note_id,source_file_id,file_name,file_path,content,mime_type,owner_id,embedding,acl_hash,acl_version,read_acl,visibility,embedding_policy) \
         SELECT $1,tenant_id,tenant_id,parent_folder_id,id,id,name,path,'alpha v2','text/markdown',owner_id,NULL,'stale-test',1,ARRAY[$3],'private','allowed' \
         FROM files WHERE tenant_id=$2 AND id=$4 AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(tenant_id)
    .bind(read_principal)
    .bind(file_id)
    .execute(pool)
    .await
    .expect("insert Memory search fixture");
    assert_eq!(rows.rows_affected(), 1);
    id
}

async fn wait_for_memory_fixture_removed(pool: &sqlx::PgPool, fixture_id: Uuid) {
    for _ in 0..100 {
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM note_index_chunks WHERE id=$1)")
                .bind(fixture_id)
                .fetch_one(pool)
                .await
                .expect("check share ACL refresh effect");
        if !exists {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("timed out waiting for share ACL refresh to remove its Memory fixture");
}

async fn memory_search_fixture_exists(pool: &sqlx::PgPool, id: Uuid, principal: &str) -> bool {
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM note_index_chunks WHERE id=$1 AND content='alpha v2' AND read_acl=ARRAY[$2])",
    )
    .bind(id)
    .bind(principal)
    .fetch_one(pool)
    .await
    .expect("verify deliberately stale Memory search row")
}

async fn assert_memory_worker_delivery_scope(owner_pool: &sqlx::PgPool, memory_event_id: Uuid) {
    const MEMORY: &str = "io.elembra.memory.files-connector.v1";
    let worker_database_url = env::var("MEMORY_DATABASE_URL").expect("set worker DATABASE_URL");
    let worker_pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&worker_database_url)
        .await
        .expect("connect with restricted Memory worker login");
    let source = format!("memory-rls-test-{}", Uuid::new_v4());
    let memory_event = Uuid::new_v4();
    let chat_event = Uuid::new_v4();
    let buzz_event = Uuid::new_v4();
    let chat_outbox_event = Uuid::new_v4();
    let cross_consumer_event = Uuid::new_v4();
    for event_id in [
        memory_event,
        chat_event,
        buzz_event,
        chat_outbox_event,
        cross_consumer_event,
    ] {
        sqlx::query(
            "INSERT INTO integration_outbox (source,event_id,event_type,application_id,tenant_id,workspace_id,event_json) VALUES ($1,$2,'io.elembra.chat.test.v1','io.elembra.chat',$3,$3,'{}'::jsonb)",
        )
        .bind(&source)
        .bind(event_id)
        .bind(Uuid::nil())
        .execute(owner_pool)
        .await
        .expect("create isolated non-Files outbox fixture");
    }
    for (consumer, event_id) in [
        (MEMORY, memory_event),
        ("io.elembra.memory.chat-projection.v1", chat_event),
        ("io.elembra.chat.buzz-bridge.v1", buzz_event),
    ] {
        sqlx::query(
            "INSERT INTO integration_deliveries (consumer_id,source,event_id,event_type,tenant_id,workspace_id) VALUES ($1,$2,$3,'files.test.v1',$4,$4)",
        )
        .bind(consumer)
        .bind(&source)
        .bind(event_id)
        .bind(Uuid::nil())
        .execute(owner_pool)
        .await
        .expect("create isolated delivery scope fixture");
    }
    let visible: Vec<String> = sqlx::query_scalar(
        "SELECT consumer_id FROM integration_deliveries WHERE source=$1 ORDER BY consumer_id",
    )
    .bind(&source)
    .fetch_all(&worker_pool)
    .await
    .expect("read Memory-scoped delivery rows");
    assert_eq!(visible, [MEMORY]);
    let visible_event_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM integration_outbox WHERE source=$1")
            .bind(&source)
            .fetch_one(&worker_pool)
            .await
            .expect("read event types allowed to Memory");
    assert_eq!(visible_event_count, 0);
    let lockable_event: Option<Uuid> =
        sqlx::query_scalar("SELECT event_id FROM integration_outbox WHERE event_id=$1 FOR UPDATE")
            .bind(memory_event_id)
            .fetch_optional(&worker_pool)
            .await
            .expect("Memory can row-lock Files events without payload UPDATE rights");
    assert_eq!(lockable_event, Some(memory_event_id));
    let hidden_update = sqlx::query(
        "UPDATE integration_deliveries SET state='processed' WHERE consumer_id='io.elembra.memory.chat-projection.v1' AND source=$1 AND event_id=$2",
    )
    .bind(&source)
    .bind(chat_event)
    .execute(&worker_pool)
    .await
    .expect("cross-consumer update is filtered by RLS");
    assert_eq!(hidden_update.rows_affected(), 0);
    let cross_consumer_insert = sqlx::query(
        "INSERT INTO integration_deliveries (consumer_id,source,event_id,event_type,tenant_id,workspace_id) VALUES ('io.elembra.memory.chat-projection.v1',$1,$2,'files.test.v1',$3,$3)",
    )
    .bind(&source)
    .bind(cross_consumer_event)
    .bind(Uuid::nil())
    .execute(&worker_pool)
    .await;
    assert!(cross_consumer_insert.is_err());
    let allowed_update = sqlx::query(
        "UPDATE integration_deliveries SET state='processed' WHERE consumer_id=$1 AND source=$2 AND event_id=$3",
    )
    .bind(MEMORY)
    .bind(&source)
    .bind(memory_event)
    .execute(&worker_pool)
    .await
    .expect("Memory worker can update its own delivery");
    assert_eq!(allowed_update.rows_affected(), 1);
    sqlx::query("DELETE FROM integration_deliveries WHERE source=$1")
        .bind(&source)
        .execute(owner_pool)
        .await
        .expect("remove isolated delivery scope fixtures");
    sqlx::query("DELETE FROM integration_outbox WHERE source=$1")
        .bind(&source)
        .execute(owner_pool)
        .await
        .expect("remove isolated outbox fixture");
    worker_pool.close().await;
}

async fn memory_delivery_id(pool: &sqlx::PgPool, source_revision: &str) -> Uuid {
    sqlx::query_scalar(
        "SELECT event_id FROM integration_outbox \
         WHERE event_json->'data'->'sourceProvenance'->>'source_revision' = $1 \
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(source_revision)
    .fetch_one(pool)
    .await
    .expect("find durable Files provenance event")
}

async fn file_event_id(pool: &sqlx::PgPool, file_id: Uuid, event_type: &str) -> Uuid {
    sqlx::query_scalar(
        "SELECT event_id FROM integration_outbox \
         WHERE event_json->>'type'=$1 AND event_json->'elembraResource'->>'resourceId'=$2 \
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(event_type)
    .bind(file_id.to_string())
    .fetch_one(pool)
    .await
    .expect("find durable Files event")
}

async fn set_memory_delivery_pending(pool: &sqlx::PgPool, event_id: Uuid, delay_seconds: i64) {
    for _ in 0..150 {
        let result = sqlx::query(
            "UPDATE integration_deliveries SET state='pending', available_at=NOW()+($2 * INTERVAL '1 second'), \
             claimed_by=NULL, claim_token=NULL, claim_expires_at=NULL, processed_at=NULL \
             WHERE consumer_id='io.elembra.memory.files-connector.v1' AND event_id=$1",
        )
        .bind(event_id)
        .bind(delay_seconds)
        .execute(pool)
        .await
        .expect("schedule Memory event delivery");
        if result.rows_affected() == 1 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("timed out waiting for Memory event delivery row");
}

async fn redeliver_memory_event(pool: &sqlx::PgPool, event_id: Uuid) {
    let result = sqlx::query(
        "UPDATE integration_deliveries SET state='pending', available_at=NOW(), \
         claimed_by=NULL, claim_token=NULL, claim_expires_at=NULL, processed_at=NULL \
         WHERE consumer_id='io.elembra.memory.files-connector.v1' AND event_id=$1 \
         AND state='processed'",
    )
    .bind(event_id)
    .execute(pool)
    .await
    .expect("redeliver Files event to Memory consumer");
    assert_eq!(result.rows_affected(), 1);
}

async fn wait_for_memory_redelivery(pool: &sqlx::PgPool, event_id: Uuid) {
    for _ in 0..150 {
        let state: Option<(String, i32)> = sqlx::query_as(
            "SELECT state, attempt_count FROM integration_deliveries \
             WHERE consumer_id='io.elembra.memory.files-connector.v1' AND event_id=$1",
        )
        .bind(event_id)
        .fetch_optional(pool)
        .await
        .expect("read Memory event delivery status");
        if state.is_some_and(|(state, attempts)| state == "processed" && attempts >= 2) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    panic!("timed out waiting for Memory event redelivery");
}

async fn wait_for_memory_initial_delivery(pool: &sqlx::PgPool, event_id: Uuid) {
    for _ in 0..150 {
        let state: Option<(String, i32)> = sqlx::query_as(
            "SELECT state, attempt_count FROM integration_deliveries \
             WHERE consumer_id='io.elembra.memory.files-connector.v1' AND event_id=$1",
        )
        .bind(event_id)
        .fetch_optional(pool)
        .await
        .expect("read initial Memory event delivery status");
        if state.is_some_and(|(state, attempts)| state == "processed" && attempts >= 1) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    panic!("timed out waiting for initial Memory event delivery");
}

async fn wait_for_memory_claim(pool: &sqlx::PgPool, event_id: Uuid) {
    for _ in 0..200 {
        let state: Option<String> = sqlx::query_scalar(
            "SELECT state FROM integration_deliveries \
             WHERE consumer_id='io.elembra.memory.files-connector.v1' AND event_id=$1",
        )
        .bind(event_id)
        .fetch_optional(pool)
        .await
        .expect("read Memory event claim state");
        if state.as_deref() == Some("claimed") {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("timed out waiting for independent Memory worker to claim event");
}

async fn wait_for_memory_attempt(pool: &sqlx::PgPool, event_id: Uuid, attempts: i32) {
    for _ in 0..240 {
        let state: Option<(String, i32)> = sqlx::query_as(
            "SELECT state, attempt_count FROM integration_deliveries \
             WHERE consumer_id='io.elembra.memory.files-connector.v1' AND event_id=$1",
        )
        .bind(event_id)
        .fetch_optional(pool)
        .await
        .expect("read recovered Memory delivery");
        if state
            .is_some_and(|(state, attempt_count)| state == "processed" && attempt_count >= attempts)
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    panic!("timed out waiting for Memory worker lease recovery");
}

async fn keyword_search(client: &Client, server: &str, token: &str, query: &str) -> Value {
    let response = client
        .post(format!("{server}/api/v1/search"))
        .bearer_auth(token)
        .json(&json!({ "query": query, "sources": ["files"], "limit": 10 }))
        .send()
        .await
        .expect("run current Files-authorized keyword search");
    assert_eq!(response.status(), StatusCode::OK);
    response.json().await.expect("decode keyword search")
}

async fn create_user(client: &Client, server: &str, admin_token: &str) -> (String, String) {
    let id = Uuid::new_v4().simple().to_string();
    let username = format!("memory_e2e_{id}");
    let email = format!("{username}@test.local");
    let password = "memory-e2e-password-123";
    let response = client
        .post(format!("{server}/api/v1/admin/users"))
        .bearer_auth(admin_token)
        .json(&json!({
            "username": username,
            "email": email,
            "password": password,
            "display_name": username,
            "is_admin": false
        }))
        .send()
        .await
        .expect("create isolated Memory search principal");
    assert_eq!(response.status(), StatusCode::CREATED);
    (email, password.to_owned())
}

async fn login(
    client: &Client,
    server: &str,
    email: &str,
    password: &str,
    tenant_id: Option<Uuid>,
) -> String {
    let response = client
        .post(format!("{server}/api/v1/auth/login"))
        .json(&json!({ "email": email, "password": password, "tenant_id": tenant_id }))
        .send()
        .await
        .expect("login isolated Memory search principal");
    assert_eq!(response.status(), StatusCode::OK);
    response.json::<Value>().await.expect("decode login")["token"]
        .as_str()
        .expect("login token")
        .to_owned()
}

async fn schedule_external_memory_reconciliation(pool: &sqlx::PgPool, tenant_id: Uuid) {
    sqlx::query(
        "INSERT INTO memory_files_reconcile_jobs (tenant_id, status, next_run_at) \
         VALUES ($1, 'idle', NOW()) ON CONFLICT (tenant_id) DO UPDATE SET \
         status='idle', cursor_file_id=NULL, retry_count=0, last_error=NULL, \
         next_run_at=NOW(), lease_owner=NULL, lease_until=NULL, heartbeat_at=NULL, \
         lease_generation=memory_files_reconcile_jobs.lease_generation+1, updated_at=NOW()",
    )
    .bind(tenant_id)
    .execute(pool)
    .await
    .expect("schedule independent Memory worker reconciliation");
}

#[tokio::test]
#[ignore = "requires a disposable loopback backend, PostgreSQL, and object store"]
async fn independent_worker_imports_retries_recovers_and_stops_after_revocation() {
    assert_eq!(env::var("RUSTSHARE_TEST_DISPOSABLE_DB").as_deref(), Ok("1"));
    assert_eq!(
        env::var("RUSTSHARE_TEST_DISPOSABLE_OBJECT_STORE").as_deref(),
        Ok("1")
    );
    let server = env::var("RUSTSHARE_CONNECTOR_TEST_SERVER").expect("set test backend URL");
    let server_url = url::Url::parse(&server).expect("test backend URL must be valid");
    assert!(
        matches!(
            server_url.host_str(),
            Some("localhost" | "127.0.0.1" | "::1")
        ),
        "Connector worker E2E is restricted to loopback backends"
    );
    let database_url = env::var("DATABASE_URL").expect("set disposable DATABASE_URL");
    let parsed_database_url = url::Url::parse(&database_url).expect("DATABASE_URL must be valid");
    assert!(
        matches!(
            parsed_database_url.host_str(),
            Some("localhost" | "127.0.0.1" | "::1")
        ) && parsed_database_url
            .path()
            .trim_start_matches('/')
            .starts_with("rustshare_test"),
        "Connector worker E2E only reads a local rustshare_test database"
    );
    let owner_token = env::var("RUSTSHARE_CONNECTOR_TEST_OWNER_TOKEN")
        .expect("set a disposable test owner's bearer token");
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await
        .expect("connect to disposable PostgreSQL");
    let client = Client::new();
    let instance_response = client
        .post(format!("{server}/api/v1/connectors"))
        .bearer_auth(&owner_token)
        .header("Idempotency-Key", Uuid::new_v4().to_string())
        .json(&json!({
            "descriptor": {
                "apiVersion":"elembra.io/v1alpha1",
                "kind":"Connector",
                "metadata":{"id":"io.elembra.connector.local-text","name":"Local Text E2E","version":"0.1.0"},
                "source":{"system":"local-filesystem","authority":"local"},
                "supports":{
                    "operations":{"read":true,"write":false,"delete":false,"list":false,"watch":false,"versions":false,"externalSharingMetadata":false},
                    "modes":{"reference":false,"mirror":false,"import":true,"archive":false,"export":false,"bidirectionalSync":false}
                },
                "publishes":{"resources":[]}
            },
            "sourceConfig":{"destinationFolderId":null},
            "credentialRef":null,
            "mode":"import"
        }))
        .send()
        .await
        .expect("register test Connector");
    assert_eq!(instance_response.status(), StatusCode::CREATED);
    let instance: Value = instance_response.json().await.expect("decode instance");
    let instance_id = instance["id"].as_str().expect("instance id").to_owned();
    let instance_uuid = Uuid::parse_str(&instance_id).expect("valid instance id");

    let credential_response = client
        .post(format!(
            "{server}/api/v1/connectors/{instance_id}/worker-credentials"
        ))
        .bearer_auth(&owner_token)
        .send()
        .await
        .expect("issue worker credential");
    assert_eq!(credential_response.status(), StatusCode::CREATED);
    let credential: Value = credential_response.json().await.expect("decode credential");
    let credential = credential["credential"]
        .as_str()
        .expect("one-time worker credential")
        .to_owned();

    let source = TempDir::new().expect("create source root");
    let scope = format!("connector-{}", Uuid::new_v4());
    let root = source.path();
    let paths = [
        ("people/alex/shared.md", b"alpha v1\n".as_slice()),
        ("people/sam/shared.md", b"sam note\n".as_slice()),
    ];
    for (relative, content) in paths {
        let path = root.join(&scope).join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }
    fs::write(root.join(&scope).join(".env"), b"secret=excluded\n").unwrap();
    fs::write(root.join(&scope).join(".hidden.md"), b"hidden\n").unwrap();
    fs::create_dir_all(root.join(&scope).join("private")).unwrap();
    fs::write(root.join(&scope).join("private/secrets.md"), b"private\n").unwrap();

    let runtime = TempDir::new().expect("create worker state directory");
    let credential_file = runtime.path().join("credential");
    save_credential(&credential_file, &credential);
    let queue = runtime.path().join("queue.sqlite3");
    let mut memory_worker = Some(spawn_memory_worker(&server, true).await);
    let first_path = format!("/{scope}/people/alex/shared.md");
    let second_path = format!("/{scope}/people/sam/shared.md");
    let mut worker = spawn_worker(&instance_id, &server, &credential_file, &queue, root);
    let files = wait_for_versions(
        &client,
        &server,
        &owner_token,
        &scope,
        "initial import",
        (&queue, &instance_id),
        &[(&first_path, 1), (&second_path, 1)],
    )
    .await;
    assert_eq!(
        files.len(),
        2,
        "hidden and private source files were excluded"
    );
    assert!(worker.0.try_wait().unwrap().is_none());
    drop(worker);

    let first_v1 = b"alpha v1\n";
    let first_v2 = b"alpha v2\n";
    fs::write(root.join(&scope).join("people/alex/shared.md"), first_v2).unwrap();
    let worker = spawn_worker(&instance_id, &server, &credential_file, &queue, root);
    wait_for_versions(
        &client,
        &server,
        &owner_token,
        &scope,
        "source update",
        (&queue, &instance_id),
        &[(&first_path, 2), (&second_path, 1)],
    )
    .await;
    drop(worker);

    let initial_events = connector_events(&pool, &instance_id).await;
    assert_eq!(initial_events.len(), 3);
    for (expected_path, expected_revision) in [
        (format!("{scope}/people/alex/shared.md"), revision(first_v1)),
        (format!("{scope}/people/alex/shared.md"), revision(first_v2)),
        (
            format!("{scope}/people/sam/shared.md"),
            revision(b"sam note\n"),
        ),
    ] {
        assert!(initial_events.iter().any(|event| {
            event["relative_path"] == expected_path
                && event["source_revision"] == expected_revision
                && event["source_resource_ref"].as_str().is_some_and(|value| {
                    value.starts_with("elembra://io.elembra.connector.local-text/file/")
                })
                && event["connector_instance_id"] == instance_id
                && event["observed_modified_at"].is_string()
        }));
    }
    // Files metadata and outbox visibility precede the separate mapping write
    // in the import handler. Wait for that durable write before asserting its
    // count instead of racing the handler's final step.
    let initial_mappings =
        wait_for_connector_mappings(&pool, instance_uuid, 3, "initial import").await;
    assert_eq!(initial_mappings.len(), 3);
    assert!(initial_mappings.iter().all(|(source, revision, target)| {
        target.starts_with("elembra://io.elembra.files/file/")
            && target.contains(&revision.replace(":", "%3A"))
            && initial_events.iter().any(|event| {
                event["source_resource_ref"].as_str() == Some(source.as_str())
                    && event["source_revision"].as_str() == Some(revision.as_str())
            })
            && files
                .iter()
                .any(|file| target.contains(file["id"].as_str().unwrap()))
    }));

    let first_file_id = Uuid::parse_str(
        files
            .iter()
            .find(|file| file["path"] == first_path)
            .expect("imported first Files resource")["id"]
            .as_str()
            .expect("Files id"),
    )
    .expect("valid Files id");
    let first_v2_revision = revision(first_v2);
    let first_v2_version = format!("sha256:{}", first_v2_revision.trim_start_matches("sha256:"));
    wait_for_memory_projection(
        &pool,
        first_file_id,
        &first_v2_version,
        &first_v2_revision,
        2,
    )
    .await;
    let tenant_id: Uuid = sqlx::query_scalar("SELECT tenant_id FROM files WHERE id=$1")
        .bind(first_file_id)
        .fetch_one(&pool)
        .await
        .expect("read imported File tenant");
    let catalog_rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM memory_files_catalog WHERE file_id=$1 AND tenant_id=$2",
    )
    .bind(first_file_id)
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .expect("count Memory provenance rows");
    assert_eq!(
        catalog_rows, 1,
        "one logical Memory catalog row per Files resource"
    );
    let catalog_provenance: (Uuid, String, String, String, bool) = sqlx::query_as(
        "SELECT connector_instance_id, source_resource_ref, source_revision, relative_path, observed_modified_at IS NOT NULL \
         FROM memory_files_catalog WHERE tenant_id=$1 AND file_id=$2",
    )
    .bind(tenant_id)
    .bind(first_file_id)
    .fetch_one(&pool)
    .await
    .expect("read Memory source provenance");
    assert_eq!(catalog_provenance.0, instance_uuid);
    assert_eq!(catalog_provenance.2, first_v2_revision);
    assert_eq!(
        catalog_provenance.3,
        format!("{scope}/people/alex/shared.md")
    );
    assert!(catalog_provenance
        .1
        .starts_with("elembra://io.elembra.connector.local-text/file/"));
    assert!(catalog_provenance.4);
    let latest_memory_event = memory_delivery_id(&pool, &first_v2_revision).await;
    assert_memory_worker_delivery_scope(&pool, latest_memory_event).await;
    let access_audit: Option<(String, String)> = sqlx::query_as(
        "SELECT outcome, file_version FROM memory_files_source_access_audit \
         WHERE tenant_id=$1 AND file_id=$2 AND principal_id=$3 AND file_version=$4 AND outcome='authorized' \
         ORDER BY occurred_at DESC LIMIT 1",
    )
    .bind(tenant_id)
    .bind(first_file_id)
    .bind(Uuid::from_u128(
        0x51b1_7a4e_83ba_4f29_9a41_4776_3e2d_0d1a,
    ))
    .bind(&first_v2_version)
    .fetch_optional(&pool)
    .await
    .expect("read content-free Files access audit");
    assert_eq!(
        access_audit,
        Some(("authorized".into(), first_v2_version.clone()))
    );

    let search = keyword_search(&client, &server, &owner_token, "alpha v2").await;
    let result = search["results"]
        .as_array()
        .unwrap()
        .iter()
        .find(|result| result["provenance"]["file_id"] == first_file_id.to_string())
        .expect("find the imported Files citation");
    assert!(result["snippet"].as_str().unwrap().contains("alpha v2"));
    let source_read_audits: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM memory_files_source_access_audit \
         WHERE tenant_id=$1 AND file_id=$2 AND file_version=$3 AND outcome='authorized'",
    )
    .bind(tenant_id)
    .bind(first_file_id)
    .bind(&first_v2_version)
    .fetch_one(&pool)
    .await
    .expect("count content-free Memory source access audits");
    assert!(
        source_read_audits >= 2,
        "indexing and search reads are audited"
    );
    let citation = url::Url::parse(result["resource_ref"].as_str().unwrap())
        .expect("parse versioned Files citation");
    assert_eq!(
        citation.path().rsplit('/').next(),
        Some(first_file_id.to_string().as_str())
    );
    assert_eq!(
        citation
            .query_pairs()
            .find(|(key, _)| key == "version")
            .map(|(_, value)| value.into_owned())
            .as_deref(),
        Some(first_v2_version.as_str())
    );

    let content_hash = first_v2_revision.trim_start_matches("sha256:");
    let object_store = rustshare_storage::ObjectStore::new(
        env::var("S3_ENDPOINT")
            .or_else(|_| env::var("RUSTFS_ENDPOINT"))
            .unwrap_or_else(|_| "http://localhost:9000".into()),
        env::var("S3_REGION")
            .or_else(|_| env::var("RUSTFS_REGION"))
            .unwrap_or_else(|_| "us-east-1".into()),
        env::var("S3_BUCKET")
            .or_else(|_| env::var("RUSTFS_BUCKET"))
            .unwrap_or_else(|_| "rustshare".into()),
    )
    .await
    .expect("connect to the Files object store");
    let object_key = format!("blobs/{content_hash}");
    let original_object = object_store
        .get(&object_key)
        .await
        .expect("read the authoritative Files version object");
    object_store
        .delete(&object_key)
        .await
        .expect("make the live Files version unavailable");
    let unavailable_source_search =
        keyword_search(&client, &server, &owner_token, "alpha v2").await;
    assert!(
        unavailable_source_search["results"]
            .as_array()
            .unwrap()
            .iter()
            .all(|result| result["provenance"]["file_id"] != first_file_id.to_string()),
        "a missing Files object cannot leak through a stale Memory search projection"
    );
    let stale_content_remains_indexed: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM note_index_chunks WHERE source_file_id=$1 AND content LIKE '%alpha v2%')",
    )
    .bind(first_file_id)
    .fetch_one(&pool)
    .await
    .expect("confirm stale search content remains present");
    assert!(stale_content_remains_indexed);
    object_store
        .put(&object_key, original_object)
        .await
        .expect("restore authoritative Files version object");

    let (recipient_email, recipient_password) = create_user(&client, &server, &owner_token).await;
    let recipient_token = login(
        &client,
        &server,
        &recipient_email,
        &recipient_password,
        Some(tenant_id),
    )
    .await;
    let share_refresh_fixture =
        insert_memory_search_fixture(&pool, tenant_id, first_file_id, "share-refresh").await;
    let share = client
        .post(format!("{server}/api/v1/files/{first_file_id}/share"))
        .bearer_auth(&owner_token)
        .json(&json!({ "recipient_email": recipient_email, "permission": "View" }))
        .send()
        .await
        .expect("share imported Files resource with recipient");
    assert_eq!(share.status(), StatusCode::CREATED);
    wait_for_memory_fixture_removed(&pool, share_refresh_fixture).await;
    let share_id = share.json::<Value>().await.expect("decode share")["share_id"]
        .as_str()
        .expect("share id")
        .to_owned();
    let mut recipient_result = None;
    for _ in 0..20 {
        // The shared Files resource remains discoverable after Notes ACL
        // refresh removes the Connector file's Memory content projection.
        let search = keyword_search(&client, &server, &recipient_token, "shared.md").await;
        if let Some(result) = search["results"]
            .as_array()
            .unwrap()
            .iter()
            .find(|result| result["provenance"]["file_id"] == first_file_id.to_string())
        {
            recipient_result = Some(result.clone());
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    let recipient_result =
        recipient_result.expect("current Files share makes the source searchable");
    assert!(recipient_result["snippet"]
        .as_str()
        .unwrap()
        .contains("alpha v2"));
    let stale_acl_user = sqlx::query_scalar::<_, String>("SELECT 'user:' || $1::text")
        .bind(
            sqlx::query_scalar::<_, Uuid>("SELECT id FROM users WHERE email=$1")
                .bind(&recipient_email)
                .fetch_one(&pool)
                .await
                .expect("resolve recipient identity"),
        )
        .fetch_one(&pool)
        .await
        .expect("construct existing ACL principal shape");
    let revoke_refresh_fixture =
        insert_memory_search_fixture(&pool, tenant_id, first_file_id, &stale_acl_user).await;
    let revoke = client
        .delete(format!("{server}/api/v1/shares/{share_id}/recipient"))
        .bearer_auth(&owner_token)
        .send()
        .await
        .expect("revoke recipient source access");
    assert_eq!(revoke.status(), StatusCode::NO_CONTENT);
    // Share ACL refresh is best-effort and may remove this non-Notes source's
    // projection. Recreate the stale row explicitly so this assertion proves
    // current Files authorization hides indexed content after revocation.
    wait_for_memory_fixture_removed(&pool, revoke_refresh_fixture).await;
    let stale_index_id =
        insert_memory_search_fixture(&pool, tenant_id, first_file_id, &stale_acl_user).await;
    assert!(memory_search_fixture_exists(&pool, stale_index_id, &stale_acl_user).await);
    let revoked_search = keyword_search(&client, &server, &recipient_token, "alpha v2").await;
    assert!(memory_search_fixture_exists(&pool, stale_index_id, &stale_acl_user).await);
    assert!(
        revoked_search["results"]
            .as_array()
            .unwrap()
            .iter()
            .all(|result| {
                result["provenance"]["file_id"] != first_file_id.to_string()
                    && !result["resource_ref"]
                        .as_str()
                        .unwrap_or_default()
                        .contains(&first_file_id.to_string())
            }),
        "revoked source cannot return a result or citation through a stale index ACL"
    );

    let (foreign_email, foreign_password) = create_user(&client, &server, &owner_token).await;
    let foreign_user_id: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE email=$1")
        .bind(&foreign_email)
        .fetch_one(&pool)
        .await
        .expect("resolve foreign search principal");
    let foreign_tenant_id = Uuid::new_v4();
    sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, $2)")
        .bind(foreign_tenant_id)
        .bind(format!("Memory E2E {foreign_tenant_id}"))
        .execute(&pool)
        .await
        .expect("create isolated tenant");
    sqlx::query("UPDATE users SET tenant_id=$2 WHERE id=$1")
        .bind(foreign_user_id)
        .bind(foreign_tenant_id)
        .execute(&pool)
        .await
        .expect("move search principal to isolated tenant");
    let foreign_token = login(
        &client,
        &server,
        &foreign_email,
        &foreign_password,
        Some(foreign_tenant_id),
    )
    .await;
    let foreign_search = keyword_search(&client, &server, &foreign_token, "alpha v2").await;
    assert!(
        foreign_search["results"]
            .as_array()
            .unwrap()
            .iter()
            .all(|result| {
                result["provenance"]["file_id"] != first_file_id.to_string()
                    && !result["resource_ref"]
                        .as_str()
                        .unwrap_or_default()
                        .contains(&first_file_id.to_string())
            }),
        "another tenant cannot retrieve this Files source"
    );

    // Simulate a lost Memory projection after durable Files/Connector state
    // was committed. The normal reconciler rebuilds provenance and content.
    sqlx::query("DELETE FROM memory_files_catalog WHERE tenant_id=$1 AND file_id=$2")
        .bind(tenant_id)
        .bind(first_file_id)
        .execute(&pool)
        .await
        .expect("simulate missing Memory catalog row");
    sqlx::query("DELETE FROM note_index_chunks WHERE tenant_id=$1 AND source_file_id=$2")
        .bind(tenant_id)
        .bind(first_file_id)
        .execute(&pool)
        .await
        .expect("simulate missing Memory search projection");
    schedule_external_memory_reconciliation(&pool, tenant_id).await;
    wait_for_memory_projection(
        &pool,
        first_file_id,
        &first_v2_version,
        &first_v2_revision,
        2,
    )
    .await;

    // The durable consumer tolerates duplicate delivery and fences a
    // reordered older revision without regressing the projection.
    let v1_event = memory_delivery_id(&pool, &revision(first_v1)).await;
    let v2_event = latest_memory_event;
    wait_for_memory_initial_delivery(&pool, v1_event).await;
    wait_for_memory_initial_delivery(&pool, v2_event).await;
    redeliver_memory_event(&pool, v1_event).await;
    wait_for_memory_redelivery(&pool, v1_event).await;
    redeliver_memory_event(&pool, v2_event).await;
    wait_for_memory_redelivery(&pool, v2_event).await;
    wait_for_memory_projection(
        &pool,
        first_file_id,
        &first_v2_version,
        &first_v2_revision,
        2,
    )
    .await;
    let catalog_rows_after_replay: i64 =
        sqlx::query_scalar("SELECT count(*) FROM memory_files_catalog WHERE file_id=$1")
            .bind(first_file_id)
            .fetch_one(&pool)
            .await
            .expect("count Memory provenance rows after replay");
    assert_eq!(catalog_rows_after_replay, 1);

    Connection::open(&queue)
        .unwrap()
        .execute(
            "UPDATE local_text_queue SET acknowledged = 0 WHERE instance_id = ?1 \
             AND relative_path = ?2 AND revision = ?3",
            rusqlite::params![
                instance_id,
                format!("{scope}/people/alex/shared.md"),
                first_v2_revision
            ],
        )
        .unwrap();
    let worker = spawn_worker(&instance_id, &server, &credential_file, &queue, root);
    for _ in 0..150 {
        if queue_pending(&queue, &instance_id) == 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    assert_eq!(queue_pending(&queue, &instance_id), 0);
    assert_eq!(connector_mappings(&pool, instance_uuid).await.len(), 3);
    assert_eq!(connector_events(&pool, &instance_id).await.len(), 3);
    drop(worker);

    let first_v3 = b"alpha v3 after offline edit\n";
    fs::write(root.join(&scope).join("people/alex/shared.md"), first_v3).unwrap();
    let worker = spawn_worker(&instance_id, &server, &credential_file, &queue, root);
    wait_for_versions(
        &client,
        &server,
        &owner_token,
        &scope,
        "reconciliation update",
        (&queue, &instance_id),
        &[(&first_path, 3), (&second_path, 1)],
    )
    .await;
    drop(worker);

    let first_v3_revision = revision(first_v3);
    let first_v3_version = format!("sha256:{}", first_v3_revision.trim_start_matches("sha256:"));
    wait_for_memory_projection(
        &pool,
        first_file_id,
        &first_v3_version,
        &first_v3_revision,
        3,
    )
    .await;
    let search = keyword_search(
        &client,
        &server,
        &owner_token,
        "alpha v3 after offline edit",
    )
    .await;
    let updated = search["results"]
        .as_array()
        .unwrap()
        .iter()
        .find(|result| result["provenance"]["file_id"] == first_file_id.to_string())
        .expect("find updated Files citation");
    assert!(updated["snippet"]
        .as_str()
        .unwrap()
        .contains("alpha v3 after offline edit"));

    // An unavailable Memory catalog cannot make the worker's authorized Files
    // import or Files edits fail. Let a manual Files update overtake the
    // initial Connector event, then prove mapping-backed recovery.
    let unavailable_memory_content = b"files survives memory outage\n";
    let unavailable_memory_source_path = format!("{scope}/people/outage.md");
    let unavailable_memory_path = format!("/{unavailable_memory_source_path}");
    fs::write(
        root.join(&unavailable_memory_source_path),
        unavailable_memory_content,
    )
    .unwrap();
    sqlx::query("ALTER TABLE memory_files_catalog RENAME TO memory_files_catalog_unavailable")
        .execute(&pool)
        .await
        .expect("simulate unavailable Memory catalog");
    let worker = spawn_worker(&instance_id, &server, &credential_file, &queue, root);
    wait_for_versions(
        &client,
        &server,
        &owner_token,
        &scope,
        "Files import with Memory catalog unavailable",
        (&queue, &instance_id),
        &[(&unavailable_memory_path, 1)],
    )
    .await;
    drop(worker);
    let outage_file_id = Uuid::parse_str(
        imported_files(&client, &server, &owner_token, &scope)
            .await
            .iter()
            .find(|file| file["path"] == unavailable_memory_path)
            .expect("Files import completed while Memory was unavailable")["id"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    let outage_revision = revision(unavailable_memory_content);
    // The unconsumed create and update are held until the update can be sent
    // first after Memory returns.
    let create_event =
        file_event_id(&pool, outage_file_id, "io.elembra.files.file.created.v1").await;
    set_memory_delivery_pending(&pool, create_event, 600).await;

    // This normal Files edit has no Connector sourceProvenance.
    let manual_files_content = b"manually edited Files content after import\n";
    let edited = client
        .post(format!("{server}/api/v1/files/{outage_file_id}/edit"))
        .bearer_auth(&owner_token)
        .json(&json!({
            "content": base64::engine::general_purpose::STANDARD.encode(manual_files_content),
            "save_mode": "new_version",
            "change_description": "Manual Files edit"
        }))
        .send()
        .await
        .expect("edit imported resource through Files");
    assert_eq!(edited.status(), StatusCode::OK);
    let edited: Value = edited.json().await.expect("decode edited Files version");
    assert_eq!(edited["current_version"], 2);
    let update_event =
        file_event_id(&pool, outage_file_id, "io.elembra.files.file.updated.v1").await;
    set_memory_delivery_pending(&pool, update_event, 600).await;
    sqlx::query("ALTER TABLE memory_files_catalog_unavailable RENAME TO memory_files_catalog")
        .execute(&pool)
        .await
        .expect("restore Memory catalog");
    set_memory_delivery_pending(&pool, update_event, 0).await;
    let manual_files_revision = revision(manual_files_content);
    let manual_files_version = format!(
        "sha256:{}",
        manual_files_revision.trim_start_matches("sha256:")
    );
    wait_for_memory_projection(
        &pool,
        outage_file_id,
        &manual_files_version,
        &outage_revision,
        2,
    )
    .await;
    set_memory_delivery_pending(&pool, create_event, 0).await;
    wait_for_memory_initial_delivery(&pool, create_event).await;
    let catalog_rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM memory_files_catalog WHERE tenant_id=$1 AND file_id=$2 AND deleted_at IS NULL",
    )
    .bind(tenant_id)
    .bind(outage_file_id)
    .fetch_one(&pool)
    .await
    .expect("verify one logical catalog row after reordered delivery");
    assert_eq!(catalog_rows, 1);
    let manual_search =
        keyword_search(&client, &server, &owner_token, "manually edited Files").await;
    let manual_result = manual_search["results"]
        .as_array()
        .unwrap()
        .iter()
        .find(|result| result["provenance"]["file_id"] == outage_file_id.to_string())
        .expect("find the Files-edited Connector source");
    assert!(manual_result["snippet"]
        .as_str()
        .unwrap()
        .contains("manually edited Files content"));
    let manual_citation = url::Url::parse(manual_result["resource_ref"].as_str().unwrap())
        .expect("parse Files-edited citation");
    assert_eq!(
        manual_citation
            .query_pairs()
            .find(|(key, _)| key == "version")
            .map(|(_, value)| value.into_owned())
            .as_deref(),
        Some(manual_files_version.as_str())
    );

    // Files and Connector remain available while only Memory is offline.
    memory_worker
        .take()
        .expect("Memory worker is running")
        .stop_gracefully();
    let offline_content = b"durable Memory backlog recovered after restart\n";
    let offline_relative = format!("{scope}/people/memory-offline.md");
    let offline_path = format!("/{offline_relative}");
    fs::write(root.join(&offline_relative), offline_content).unwrap();
    let local_text = spawn_worker(&instance_id, &server, &credential_file, &queue, root);
    let files = wait_for_versions(
        &client,
        &server,
        &owner_token,
        &scope,
        "Files import with Memory worker offline",
        (&queue, &instance_id),
        &[(&offline_path, 1)],
    )
    .await;
    drop(local_text);
    let offline_file_id = Uuid::parse_str(
        files
            .iter()
            .find(|file| file["path"] == offline_path)
            .expect("Files import completed with Memory offline")["id"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    let offline_event =
        file_event_id(&pool, offline_file_id, "io.elembra.files.file.created.v1").await;
    let pending: String = sqlx::query_scalar(
        "SELECT state FROM integration_deliveries WHERE consumer_id='io.elembra.memory.files-connector.v1' AND event_id=$1",
    )
    .bind(offline_event)
    .fetch_one(&pool)
    .await
    .expect("read durable Memory backlog");
    assert_eq!(pending, "pending");
    memory_worker = Some(spawn_memory_worker(&server, true).await);
    wait_for_memory_initial_delivery(&pool, offline_event).await;
    let offline_revision = revision(offline_content);
    let offline_version = format!("sha256:{}", offline_revision.trim_start_matches("sha256:"));
    wait_for_memory_projection(
        &pool,
        offline_file_id,
        &offline_version,
        &offline_revision,
        1,
    )
    .await;
    let offline_search = keyword_search(
        &client,
        &server,
        &owner_token,
        "durable Memory backlog recovered",
    )
    .await;
    assert!(offline_search["results"]
        .as_array()
        .unwrap()
        .iter()
        .any(|result| {
            if result["provenance"]["file_id"] != offline_file_id.to_string() {
                return false;
            }
            url::Url::parse(result["resource_ref"].as_str().unwrap_or_default())
                .ok()
                .and_then(|reference| {
                    reference
                        .query_pairs()
                        .find(|(key, _)| key == "version")
                        .map(|(_, value)| value.into_owned())
                })
                .as_deref()
                == Some(offline_version.as_str())
        }));

    // A claimed event survives a worker kill and its expired outbox lease.
    memory_worker
        .take()
        .expect("Memory worker is running")
        .stop_gracefully();
    let crash_content = b"Memory event recovered after process kill\n";
    let crash_relative = format!("{scope}/people/memory-kill.md");
    let crash_path = format!("/{crash_relative}");
    fs::write(root.join(&crash_relative), crash_content).unwrap();
    let local_text = spawn_worker(&instance_id, &server, &credential_file, &queue, root);
    let files = wait_for_versions(
        &client,
        &server,
        &owner_token,
        &scope,
        "Files import before Memory worker restart",
        (&queue, &instance_id),
        &[(&crash_path, 1)],
    )
    .await;
    drop(local_text);
    let crash_file_id = Uuid::parse_str(
        files
            .iter()
            .find(|file| file["path"] == crash_path)
            .expect("Files import completed before Memory restart")["id"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    let crash_event = file_event_id(&pool, crash_file_id, "io.elembra.files.file.created.v1").await;
    let backend_pid = env::var("RUSTSHARE_CONNECTOR_TEST_BACKEND_PID")
        .expect("set disposable backend process id")
        .parse::<u32>()
        .expect("valid backend process id");
    signal_process(backend_pid, "-STOP");
    let resume_backend = ResumeOnDrop(backend_pid);
    let mut killed_memory_worker = spawn_memory_worker(&server, false).await;
    wait_for_memory_claim(&pool, crash_event).await;
    killed_memory_worker.kill();
    drop(resume_backend);
    wait_for_backend(&client, &server, true).await;
    memory_worker = Some(spawn_memory_worker(&server, true).await);
    wait_for_memory_attempt(&pool, crash_event, 2).await;
    let crash_revision = revision(crash_content);
    let crash_version = format!("sha256:{}", crash_revision.trim_start_matches("sha256:"));
    wait_for_memory_projection(&pool, crash_file_id, &crash_version, &crash_revision, 1).await;

    // The Memory worker remains independent across an HTTP backend restart.
    signal_process(backend_pid, "-TERM");
    wait_for_backend(&client, &server, false).await;
    let _restarted_backend = spawn_backend(
        &env::var("DATABASE_URL").expect("set disposable DATABASE_URL"),
        &runtime.path().join("restarted-backend.log"),
    );
    wait_for_backend(&client, &server, true).await;
    assert!(memory_worker
        .as_mut()
        .expect("Memory worker remains running")
        .0
        .try_wait()
        .unwrap()
        .is_none());
    let backend_restart_content = b"Memory worker stayed up during backend restart\n";
    let backend_restart_relative = format!("{scope}/people/backend-restart.md");
    let backend_restart_path = format!("/{backend_restart_relative}");
    fs::write(
        root.join(&backend_restart_relative),
        backend_restart_content,
    )
    .unwrap();
    let local_text = spawn_worker(&instance_id, &server, &credential_file, &queue, root);
    let files = wait_for_versions(
        &client,
        &server,
        &owner_token,
        &scope,
        "Files import after backend restart",
        (&queue, &instance_id),
        &[(&backend_restart_path, 1)],
    )
    .await;
    drop(local_text);
    let backend_restart_file_id = Uuid::parse_str(
        files
            .iter()
            .find(|file| file["path"] == backend_restart_path)
            .expect("Files import completed after backend restart")["id"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    let backend_restart_revision = revision(backend_restart_content);
    let backend_restart_version = format!(
        "sha256:{}",
        backend_restart_revision.trim_start_matches("sha256:")
    );
    wait_for_memory_projection(
        &pool,
        backend_restart_file_id,
        &backend_restart_version,
        &backend_restart_revision,
        1,
    )
    .await;

    let rotated = client
        .post(format!(
            "{server}/api/v1/connectors/{instance_id}/worker-credentials"
        ))
        .bearer_auth(&owner_token)
        .send()
        .await
        .expect("rotate worker credential");
    assert_eq!(rotated.status(), StatusCode::CREATED);
    let rotated_credential = rotated
        .json::<Value>()
        .await
        .expect("decode rotated credential")["credential"]
        .as_str()
        .expect("new worker credential")
        .to_owned();

    let revoked_path = format!("{scope}/people/post-rotation.md");
    fs::write(
        root.join(&revoked_path),
        b"old credential must not ingest\n",
    )
    .unwrap();
    let old_credential_worker = spawn_worker(&instance_id, &server, &credential_file, &queue, root);
    tokio::time::sleep(Duration::from_secs(2)).await;
    drop(old_credential_worker);
    assert!(
        !imported_files(&client, &server, &owner_token, &scope)
            .await
            .iter()
            .any(|file| file["path"] == format!("/{revoked_path}")),
        "rotated Connector credential blocks subsequent ingestion"
    );
    save_credential(&credential_file, &rotated_credential);
    let renewed_worker = spawn_worker(&instance_id, &server, &credential_file, &queue, root);
    let revoked_file_path = format!("/{revoked_path}");
    wait_for_versions(
        &client,
        &server,
        &owner_token,
        &scope,
        "Files import after credential rotation",
        (&queue, &instance_id),
        &[(&revoked_file_path, 1)],
    )
    .await;
    drop(renewed_worker);

    let files_before_delete = imported_files(&client, &server, &owner_token, &scope).await;
    assert_eq!(files_before_delete.len(), 7);
    let deleted = client
        .delete(format!("{server}/api/v1/files/{first_file_id}"))
        .bearer_auth(&owner_token)
        .send()
        .await
        .expect("delete imported Files resource");
    assert!(matches!(
        deleted.status(),
        StatusCode::OK | StatusCode::NO_CONTENT
    ));
    let deleted_search = keyword_search(
        &client,
        &server,
        &owner_token,
        "alpha v3 after offline edit",
    )
    .await;
    assert!(!deleted_search["results"]
        .as_array()
        .unwrap()
        .iter()
        .any(|result| result["provenance"]["file_id"] == first_file_id.to_string()));
    for _ in 0..150 {
        let deleted_at: Option<chrono::DateTime<chrono::Utc>> =
            sqlx::query_scalar("SELECT deleted_at FROM memory_files_catalog WHERE file_id=$1")
                .bind(first_file_id)
                .fetch_optional(&pool)
                .await
                .expect("read Memory Files tombstone")
                .flatten();
        if deleted_at.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    let remaining_chunks: i64 =
        sqlx::query_scalar("SELECT count(*) FROM note_index_chunks WHERE source_file_id=$1")
            .bind(first_file_id)
            .fetch_one(&pool)
            .await
            .expect("check stale keyword projection cleanup");
    assert_eq!(remaining_chunks, 0);

    let restored = client
        .post(format!(
            "{server}/api/v1/files/{first_file_id}/restore-from-trash"
        ))
        .bearer_auth(&owner_token)
        .send()
        .await
        .expect("restore imported Files resource");
    assert_eq!(restored.status(), StatusCode::NO_CONTENT);
    schedule_external_memory_reconciliation(&pool, tenant_id).await;
    wait_for_memory_projection(
        &pool,
        first_file_id,
        &first_v3_version,
        &first_v3_revision,
        3,
    )
    .await;

    let current = client
        .get(format!("{server}/api/v1/connectors/{instance_id}"))
        .bearer_auth(&owner_token)
        .send()
        .await
        .expect("read Connector for cleanup")
        .json::<Value>()
        .await
        .expect("decode Connector");
    client
        .patch(format!("{server}/api/v1/connectors/{instance_id}"))
        .bearer_auth(&owner_token)
        .json(&json!({
            "expectedVersion": current["version"],
            "lifecycleState":"disconnected"
        }))
        .send()
        .await
        .expect("disconnect test Connector")
        .error_for_status()
        .expect("disconnect response");
    if let Some(mut memory_worker) = memory_worker.take() {
        memory_worker.stop_gracefully();
    }
    pool.close().await;
}
