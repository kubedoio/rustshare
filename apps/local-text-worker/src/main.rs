use anyhow::{bail, Context, Result};
use clap::Parser;
use local_text_worker::{scan_root, OfflineQueue, RootConfig};
use reqwest::{multipart, redirect::Policy, StatusCode};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs::OpenOptions, io::Read, path::PathBuf, time::Duration};
use uuid::Uuid;

#[derive(Debug, Parser)]
#[command(name = "elembra-local-text-worker")]
struct Args {
    #[arg(long)]
    instance_id: Uuid,
    #[arg(long)]
    server: String,
    #[arg(long, conflicts_with_all = ["credential_keyring", "migrate_credential_file"])]
    credential_file: Option<PathBuf>,
    #[arg(long, conflicts_with = "migrate_credential_file")]
    credential_keyring: bool,
    #[arg(long)]
    migrate_credential_file: Option<PathBuf>,
    #[arg(long, required = true)]
    root: Vec<PathBuf>,
    #[arg(long)]
    queue: PathBuf,
    #[arg(long, default_value_t = 60)]
    reconcile_seconds: u64,
    #[arg(long = "include")]
    include: Vec<String>,
    #[arg(long = "exclude")]
    exclude: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkerSession {
    id: Uuid,
    connector_id: String,
    source_config: serde_json::Value,
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
    target_resource_ref: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportMetadata<'a> {
    lease_id: Uuid,
    fence: i64,
    source_resource_ref: String,
    source_revision: &'a str,
    relative_path: &'a str,
    observed_modified_at: chrono::DateTime<chrono::Utc>,
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
    if args.reconcile_seconds == 0 || args.reconcile_seconds > 86_400 {
        bail!("reconcile interval must be between 1 and 86400 seconds");
    }
    let credential = match (args.credential_file, args.credential_keyring, args.migrate_credential_file) {
        (Some(path), false, None) => read_credential(&path)?,
        (None, true, None) => read_keyring_credential(args.instance_id)?,
        (None, false, Some(path)) => migrate_credential(&path, args.instance_id)?,
        _ => bail!("choose exactly one of --credential-file, --credential-keyring, or --migrate-credential-file"),
    };
    let server = args.server.trim_end_matches('/');
    if !server.starts_with("https://") && !is_loopback_http(server) {
        bail!("Connector worker requires HTTPS except for loopback development");
    }
    let client = reqwest::Client::builder()
        .redirect(Policy::none())
        .timeout(Duration::from_secs(30))
        .build()?;
    let mut queue = OfflineQueue::open(&args.queue)?;
    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        if event.is_ok() {
            let _ = event_tx.send(());
        }
    })?;
    for root in &args.root {
        use notify::Watcher;
        watcher.watch(root, notify::RecursiveMode::Recursive)?;
    }
    loop {
        let mut scanned = 0usize;
        let mut excluded = 0usize;
        for root in &args.root {
            let scan = scan_root(&RootConfig {
                path: root.clone(),
                include: args.include.clone(),
                exclude: args.exclude.clone(),
            })?;
            for observation in &scan.observations {
                queue.enqueue(args.instance_id, observation)?;
            }
            scanned += scan.observations.len();
            excluded += scan.excluded;
        }
        let uploaded = match worker_session(&client, server, &credential).await {
            Ok(session) if session.id == args.instance_id => {
                let parent_folder_id = destination_folder(&session.source_config)?;
                match upload_pending(
                    &client,
                    server,
                    &credential,
                    &session,
                    parent_folder_id,
                    &queue,
                    args.instance_id,
                )
                .await
                {
                    Ok(uploaded) => uploaded,
                    Err(_) => {
                        eprintln!(
                            "Upload deferred; queued observations will retry after the next scan"
                        );
                        0
                    }
                }
            }
            Ok(_) => bail!("worker credential is bound to a different Connector instance"),
            Err(_) => {
                eprintln!(
                    "Connector unavailable; queued observations will retry after the next scan"
                );
                0
            }
        };
        println!(
            "Scanned {scanned} eligible observations, uploaded {uploaded}, excluded {excluded}"
        );
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(args.reconcile_seconds)) => {},
            event = event_rx.recv() => {
                if event.is_none() { bail!("local file watcher stopped"); }
                tokio::time::sleep(Duration::from_millis(250)).await;
                while event_rx.try_recv().is_ok() {}
            }
        }
    }
}

async fn worker_session(
    client: &reqwest::Client,
    server: &str,
    credential: &str,
) -> Result<WorkerSession> {
    client
        .get(format!("{server}/api/v1/connectors/worker/session"))
        .header("Authorization", format!("Connector {credential}"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await
        .context("invalid worker session response")
}

async fn upload_pending(
    client: &reqwest::Client,
    server: &str,
    credential: &str,
    session: &WorkerSession,
    parent_folder_id: Option<Uuid>,
    queue: &OfflineQueue,
    instance_id: Uuid,
) -> Result<usize> {
    let pending = queue.pending(instance_id, 100)?;
    if pending.is_empty() {
        return Ok(0);
    }
    let Some(lease) = client
        .post(format!("{server}/api/v1/connectors/worker/lease"))
        .header("Authorization", format!("Connector {credential}"))
        .send()
        .await?
        .error_for_status()?
        .json::<Option<ExecutionLease>>()
        .await?
    else {
        return Ok(0);
    };
    let mut completed = Vec::new();
    for queued in pending {
        let modified_at =
            chrono::DateTime::from_timestamp(queued.observation.observed_modified_at, 0)
                .context("source timestamp is outside the supported range")?;
        let source_id = hex::encode(Sha256::digest(
            format!(
                "{}\0{}",
                queued.observation.root_id, queued.observation.relative_path
            )
            .as_bytes(),
        ));
        let metadata = serde_json::to_string(&ImportMetadata {
            lease_id: lease.lease_id,
            fence: lease.fence,
            source_resource_ref: format!("elembra://{}/file/{source_id}", session.connector_id),
            source_revision: &queued.observation.revision,
            relative_path: &queued.observation.relative_path,
            observed_modified_at: modified_at,
            parent_folder_id,
        })?;
        let form = multipart::Form::new().text("metadata", metadata).part(
            "file",
            multipart::Part::bytes(queued.observation.content.clone())
                .file_name("source.txt")
                .mime_str("text/plain; charset=utf-8")?,
        );
        let response = match client
            .post(format!("{server}/api/v1/connectors/worker/files/import"))
            .header("Authorization", format!("Connector {credential}"))
            .multipart(form)
            .send()
            .await
        {
            Ok(response) => response,
            Err(_) => {
                report_failure(client, server, credential, &lease, "SourceUnavailable").await;
                bail!("Connector import request failed");
            }
        };
        if !response.status().is_success() {
            let error = match response.status() {
                StatusCode::UNAUTHORIZED => "Authentication",
                StatusCode::FORBIDDEN => "PolicyDenied",
                StatusCode::TOO_MANY_REQUESTS => "RateLimit",
                StatusCode::BAD_REQUEST => "InvalidContent",
                _ => "SourceUnavailable",
            };
            report_failure(client, server, credential, &lease, error).await;
            bail!("Connector import failed with HTTP {}", response.status());
        }
        let result: ImportResponse = response
            .json()
            .await
            .context("invalid Files import response")?;
        completed.push((queued.id, result.target_resource_ref));
    }
    if let Some((queue_id, _)) = completed.last() {
        let next_version = lease.checkpoint_version + 1;
        let response = client
            .post(format!(
                "{server}/api/v1/connectors/worker/lease/{}/ack",
                lease.lease_id
            ))
            .header("Authorization", format!("Connector {credential}"))
            .json(&serde_json::json!({
                "fence": lease.fence,
                "expectedCheckpointVersion": lease.checkpoint_version,
                "cursor": Checkpoint { revision: next_version, queue_id: *queue_id }
            }))
            .send()
            .await?;
        if response.status() != StatusCode::NO_CONTENT {
            bail!(
                "Connector checkpoint was rejected with HTTP {}",
                response.status()
            );
        }
    }
    for (id, target_ref) in &completed {
        queue.acknowledge(*id, target_ref)?;
    }
    Ok(completed.len())
}

async fn report_failure(
    client: &reqwest::Client,
    server: &str,
    credential: &str,
    lease: &ExecutionLease,
    error: &str,
) {
    let _ = client
        .post(format!(
            "{server}/api/v1/connectors/worker/lease/{}/fail",
            lease.lease_id
        ))
        .header("Authorization", format!("Connector {credential}"))
        .json(&serde_json::json!({"fence": lease.fence, "error": error}))
        .send()
        .await;
}

fn read_credential(path: &std::path::Path) -> Result<String> {
    #[cfg(not(unix))]
    bail!("credential files are supported only on Unix; use --credential-keyring on this platform");

    #[cfg(unix)]
    {
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
        }
        let mut file = options
            .open(path)
            .context("unable to open worker credential file")?;
        let metadata = file.metadata()?;
        if !metadata.is_file() || metadata.len() > 128 {
            bail!("worker credential file is not a small regular file");
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o077 != 0 {
                bail!("worker credential file must not be accessible by group or others");
            }
        }
        let mut credential = String::new();
        file.read_to_string(&mut credential)?;
        let credential = credential.trim().to_owned();
        if !credential.starts_with("ecw1_") || credential.len() > 128 {
            bail!("worker credential file contains an invalid credential");
        }
        Ok(credential)
    }
}

fn read_keyring_credential(instance_id: Uuid) -> Result<String> {
    let entry = keyring::Entry::new("io.elembra.local-text-worker", &instance_id.to_string())
        .context("unable to access the operating system credential store")?;
    let credential = entry
        .get_password()
        .context("no worker credential is stored in the operating system credential store")?;
    validate_credential(credential)
}

fn migrate_credential(path: &std::path::Path, instance_id: Uuid) -> Result<String> {
    let credential = read_credential(path)?;
    let entry = keyring::Entry::new("io.elembra.local-text-worker", &instance_id.to_string())
        .context("unable to access the operating system credential store")?;
    entry
        .set_password(&credential)
        .context("unable to save worker credential to the operating system credential store")?;
    std::fs::remove_file(path).context("credential was stored in the operating system store, but the legacy file could not be removed")?;
    Ok(credential)
}

fn validate_credential(credential: String) -> Result<String> {
    let credential = credential.trim().to_owned();
    if !credential.starts_with("ecw1_") || credential.len() > 128 {
        bail!("worker credential contains an invalid credential");
    }
    Ok(credential)
}

fn destination_folder(source_config: &serde_json::Value) -> Result<Option<Uuid>> {
    match source_config.get("destinationFolderId") {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(value)) => Ok(Some(
            Uuid::parse_str(value).context("invalid approved destination folder")?,
        )),
        _ => bail!("invalid approved destination folder"),
    }
}

fn is_loopback_http(server: &str) -> bool {
    server.strip_prefix("http://").is_some_and(|host| {
        host.starts_with("localhost:")
            || host.starts_with("127.0.0.1:")
            || host.starts_with("[::1]:")
    })
}

#[cfg(test)]
mod credential_tests {
    use super::validate_credential;

    #[test]
    fn accepts_only_connector_worker_credentials() {
        assert_eq!(
            validate_credential("ecw1_test".into()).unwrap(),
            "ecw1_test"
        );
        assert!(validate_credential("owner-password".into()).is_err());
        assert!(validate_credential(format!("ecw1_{}", "x".repeat(124))).is_err());
    }
}
