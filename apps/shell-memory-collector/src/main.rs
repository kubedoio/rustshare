use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};
use reqwest::{multipart, redirect::Policy, StatusCode};
use serde::{Deserialize, Serialize};
use shell_memory_collector::{
    record_if_enabled, Config, Event, Queue, QueueLock, RecordInput, UploadLock,
};
use std::{io::Read, net::IpAddr, path::PathBuf, time::Duration};
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
    StoreCredential,
    Record,
    Run,
    Pause,
    Resume,
    Disable,
    Enable,
    Forget,
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
        Command::StoreCredential => store_credential(&config_path),
        Command::Record => record(&config_path),
        Command::Run => run(&config_path).await,
        Command::Pause => set_control(&config_path, Control::Pause, true),
        Command::Resume => set_control(&config_path, Control::Pause, false),
        Command::Disable => set_control(&config_path, Control::Disable, true),
        Command::Enable => set_control(&config_path, Control::Disable, false),
        Command::Forget => forget(&config_path),
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
    if fields.len() != 5 {
        bail!("record input must contain five NUL-delimited fields");
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
    let events = {
        let _queue_lock = QueueLock::acquire(&config.queue)?;
        if Queue::is_disabled(&config.queue)? {
            return Ok(UploadRound::Disabled);
        }
        let queue = Queue::open(&config.queue)?;
        if queue.paused() {
            Vec::new()
        } else {
            queue.pending(config.instance_id, EVENTS_PER_UPLOAD_ROUND)?
        }
    };
    if events.is_empty() {
        return Ok(UploadRound::Idle);
    }
    let credential = credential()?;
    match upload_pending(client, config, &credential, events).await? {
        UploadResult::NoLease => Ok(UploadRound::NoLease),
        UploadResult::Completed => Ok(UploadRound::Completed),
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
    for event in events {
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
            multipart::Part::bytes(event.content)
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
            (true, None) | (false, Some(_)) => completed.push(event.id),
            _ => bail!("Files import response had an invalid forgotten state"),
        }
    }
    let Some(queue_id) = completed.last() else {
        bail!("upload round completed without a Files import");
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
    Ok(UploadResult::Completed)
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

enum Control {
    Pause,
    Disable,
}
fn set_control(config_path: &PathBuf, control: Control, value: bool) -> Result<()> {
    let config = load_config(config_path)?;
    let _upload_lock = UploadLock::acquire(&config.queue)?;
    let _lock = QueueLock::acquire(&config.queue)?;
    let queue = Queue::open(&config.queue)?;
    match control {
        Control::Pause => queue.set_paused(value),
        Control::Disable => queue.set_disabled(value),
    }
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

fn forget(config_path: &PathBuf) -> Result<()> {
    let config = load_config(config_path)?;
    let _upload_lock = UploadLock::acquire(&config.queue)?;
    let _lock = QueueLock::acquire(&config.queue)?;
    let queue = Queue::open(&config.queue)?;
    queue.forget()?;
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
mod tests {
    use super::{failure_category, normalize_server, upload_round, UploadRound};
    use anyhow::Result;
    use reqwest::StatusCode;
    use shell_memory_collector::{
        record_if_enabled, Config, Queue, QueueLock, RecordInput, UploadLock,
    };
    use std::time::Duration;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::{TcpListener, TcpStream},
        sync::{mpsc, oneshot},
    };
    use uuid::Uuid;

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
                    import_release
                        .take()
                        .expect("only one event is imported")
                        .await
                        .map_err(std::io::Error::other)?;
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
}
