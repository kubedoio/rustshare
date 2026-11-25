use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};
use reqwest::{multipart, redirect::Policy, StatusCode};
use serde::{Deserialize, Serialize};
use shell_memory_collector::{Config, Queue, QueueLock, RecordInput};
use std::{io::Read, net::IpAddr, path::PathBuf, time::Duration};
use url::Url;
use uuid::Uuid;

const CREDENTIAL_SERVICE: &str = "io.elembra.shell-memory";
const CONNECTOR_ID: &str = "io.elembra.connector.shell-memory";

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
    target_resource_ref: String,
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
    let _lock = QueueLock::acquire(&config.queue)?;
    let mut queue = Queue::open(&config.queue)?;
    if queue.disabled() {
        return Ok(());
    }
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
    queue.record(config.instance_id, input, &config.exclude_commands)?;
    Ok(())
}

async fn run(config_path: &PathBuf) -> Result<()> {
    let config = load_config(config_path)?;
    let credential = read_keyring_credential(config.instance_id)?;
    let client = reqwest::Client::builder()
        .redirect(Policy::none())
        .timeout(Duration::from_secs(30))
        .build()?;
    loop {
        {
            let _lock = QueueLock::acquire(&config.queue)?;
            let queue = Queue::open(&config.queue)?;
            if queue.disabled() {
                bail!("collector is disabled; run enable before starting the uploader");
            }
            if !queue.paused() {
                if let Err(error) = upload_pending(&client, &config, &credential, &queue).await {
                    eprintln!(
                        "Shell Memory upload deferred; queued events remain local: {error:#}"
                    );
                }
            }
        }
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(15)) => {},
            signal = tokio::signal::ctrl_c() => { signal?; return Ok(()); }
        }
    }
}

async fn upload_pending(
    client: &reqwest::Client,
    config: &Config,
    credential: &str,
    queue: &Queue,
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
    let events = queue.pending(config.instance_id, 100)?;
    if events.is_empty() {
        return Ok(());
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
        return Ok(());
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
        completed.push((event.id, imported.target_resource_ref));
    }
    let Some((queue_id, _)) = completed.last() else {
        return Ok(());
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
    for (id, _) in completed {
        queue.acknowledge(id)?;
    }
    Ok(())
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
    let _lock = QueueLock::acquire(&config.queue)?;
    let queue = Queue::open(&config.queue)?;
    match control {
        Control::Pause => queue.set_paused(value),
        Control::Disable => queue.set_disabled(value),
    }
}

fn store_credential(config_path: &PathBuf) -> Result<()> {
    let config = load_config(config_path)?;
    let _lock = QueueLock::acquire(&config.queue)?;
    let mut credential = String::new();
    std::io::stdin().read_to_string(&mut credential)?;
    let credential = credential.trim();
    if !credential.starts_with("ecw1_") || credential.len() > 128 {
        bail!("invalid Connector worker credential");
    }
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
    use super::{failure_category, normalize_server};
    use reqwest::StatusCode;

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
}
