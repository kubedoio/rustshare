use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use glob::Pattern;
use rusqlite::{params, Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};
use uuid::Uuid;

const MAX_QUEUE_BYTES: i64 = 256 * 1024 * 1024;
const MAX_COMMAND_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub server: String,
    pub instance_id: Uuid,
    pub queue: PathBuf,
    #[serde(default)]
    pub exclude_commands: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RecordInput {
    pub command: String,
    pub cwd: String,
    pub host: String,
    pub exit_code: i32,
    pub duration_ms: u64,
}

#[derive(Debug, Clone)]
pub struct Event {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub command: String,
    pub cwd: String,
    pub host: String,
    pub exit_code: i32,
    pub duration_ms: u64,
    pub revision: String,
    pub content: Vec<u8>,
}

pub struct Queue {
    path: PathBuf,
    connection: Connection,
}

/// Cross-process exclusion for queue mutations and remote uploads.
pub struct QueueLock {
    _file: fs::File,
}

impl QueueLock {
    /// Acquires the short-lived queue mutation lock.
    pub fn acquire(queue_path: &Path) -> Result<Self> {
        let file = open_lock_file(queue_path, "queue.lock")?;
        file.lock()?;
        Ok(Self { _file: file })
    }
}

/// Serializes upload rounds, credential rotation, and lifecycle controls.
pub struct UploadLock {
    _file: fs::File,
}

impl UploadLock {
    /// Acquires the lifecycle lock. Acquire this before `QueueLock` when both are needed.
    pub fn acquire(queue_path: &Path) -> Result<Self> {
        let file = open_lock_file(queue_path, "upload.lock")?;
        file.lock()?;
        Ok(Self { _file: file })
    }
}

fn open_lock_file(queue_path: &Path, extension: &str) -> Result<fs::File> {
    if let Some(parent) = queue_path.parent() {
        fs::create_dir_all(parent)?;
    }
    open_lock_file_at(&queue_path.with_extension(extension))
}

#[cfg(unix)]
fn open_lock_file_at(path: &Path) -> Result<fs::File> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)?;
    if !file.metadata()?.is_file() {
        bail!("queue lock path must be a regular file");
    }
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    Ok(file)
}

#[cfg(not(unix))]
fn open_lock_file_at(path: &Path) -> Result<fs::File> {
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(path)?;
    if !file.metadata()?.is_file() {
        bail!("queue lock path must be a regular file");
    }
    Ok(file)
}

impl Queue {
    pub fn is_disabled(path: &Path) -> Result<bool> {
        match fs::symlink_metadata(path.with_extension("disabled")) {
            Ok(metadata) if metadata.is_file() => Ok(true),
            Ok(_) => bail!("disabled marker must be a regular file"),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error.into()),
        }
    }

    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        prepare_queue_file(path)?;
        let connection = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        connection.pragma_update(None, "journal_mode", "DELETE")?;
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS shell_events (
                id TEXT PRIMARY KEY,
                instance_id TEXT NOT NULL,
                created_at TEXT NOT NULL,
                command TEXT NOT NULL,
                cwd TEXT NOT NULL,
                host TEXT NOT NULL,
                exit_code INTEGER NOT NULL,
                duration_ms INTEGER NOT NULL,
                revision TEXT NOT NULL,
                content BLOB NOT NULL
            );",
        )?;
        Ok(Self {
            path: path.to_owned(),
            connection,
        })
    }

    pub fn record(
        &mut self,
        instance_id: Uuid,
        input: RecordInput,
        exclusions: &[String],
    ) -> Result<bool> {
        if self.disabled() {
            return Ok(false);
        }
        if input.command.len() > MAX_COMMAND_BYTES {
            bail!("command exceeds the local capture limit");
        }
        let patterns = exclusions
            .iter()
            .map(|pattern| Pattern::new(pattern))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        if excluded(&input.command, &patterns) {
            return Ok(false);
        }
        let command = redact_command(&input.command);
        let created_at = Utc::now();
        let id = Uuid::new_v4();
        let content = render_markdown(id, created_at, &command, &input)?;
        let revision = format!("sha256:{}", hex::encode(Sha256::digest(&content)));
        self.connection.execute(
            "INSERT INTO shell_events (id, instance_id, created_at, command, cwd, host, exit_code, duration_ms, revision, content)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![id.to_string(), instance_id.to_string(), created_at.to_rfc3339(), command,
                input.cwd, input.host, input.exit_code, input.duration_ms.min(i64::MAX as u64) as i64,
                revision, content],
        )?;
        let queued_bytes: i64 = self.connection.query_row(
            "SELECT COALESCE(SUM(length(content)), 0) FROM shell_events",
            [],
            |row| row.get(0),
        )?;
        if queued_bytes > MAX_QUEUE_BYTES {
            self.connection
                .execute("DELETE FROM shell_events WHERE id = ?1", [id.to_string()])?;
            bail!("offline queue is full; oldest records were preserved");
        }
        Ok(true)
    }

    pub fn pending(&self, instance_id: Uuid, limit: usize) -> Result<Vec<Event>> {
        let mut statement = self.connection.prepare(
            "SELECT id, created_at, command, cwd, host, exit_code, duration_ms, revision, content
             FROM shell_events WHERE instance_id = ?1 ORDER BY rowid LIMIT ?2",
        )?;
        let rows = statement.query_map(
            params![instance_id.to_string(), limit.min(100) as i64],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, i32>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, Vec<u8>>(8)?,
                ))
            },
        )?;
        rows.map(|row| {
            let (id, created_at, command, cwd, host, exit_code, duration_ms, revision, content) =
                row?;
            Ok(Event {
                id: Uuid::parse_str(&id)?,
                created_at: DateTime::parse_from_rfc3339(&created_at)?.with_timezone(&Utc),
                command,
                cwd,
                host,
                exit_code,
                duration_ms: duration_ms.max(0) as u64,
                revision,
                content,
            })
        })
        .collect()
    }

    pub fn acknowledge(&self, id: Uuid) -> Result<bool> {
        Ok(self
            .connection
            .execute("DELETE FROM shell_events WHERE id = ?1", [id.to_string()])?
            == 1)
    }

    pub fn forget(self) -> Result<()> {
        set_marker(&self.path.with_extension("disabled"), true)?;
        drop(self.connection);
        let sidecar = |suffix: &str| PathBuf::from(format!("{}-{suffix}", self.path.display()));
        for path in [
            &self.path,
            &sidecar("journal"),
            &sidecar("wal"),
            &sidecar("shm"),
        ] {
            match fs::remove_file(path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        let _ = fs::remove_file(self.path.with_extension("paused"));
        Ok(())
    }

    pub fn paused(&self) -> bool {
        self.path.with_extension("paused").exists()
    }
    pub fn disabled(&self) -> bool {
        self.path.with_extension("disabled").exists()
    }
    pub fn set_paused(&self, paused: bool) -> Result<()> {
        set_marker(&self.path.with_extension("paused"), paused)
    }
    pub fn set_disabled(&self, disabled: bool) -> Result<()> {
        set_marker(&self.path.with_extension("disabled"), disabled)
    }
}

/// Records an event under the short queue lock, without waiting for an upload round.
pub fn record_if_enabled(
    path: &Path,
    instance_id: Uuid,
    input: RecordInput,
    exclusions: &[String],
) -> Result<bool> {
    let _lock = QueueLock::acquire(path)?;
    if Queue::is_disabled(path)? {
        return Ok(false);
    }
    let mut queue = Queue::open(path)?;
    if queue.disabled() {
        return Ok(false);
    }
    queue.record(instance_id, input, exclusions)
}

pub fn redact_command(command: &str) -> String {
    if command.split_whitespace().any(|token| {
        let lower = token.to_ascii_lowercase();
        let key = token.split_once('=').map_or(token, |(key, _)| key);
        sensitive_key(key)
            || lower.contains("bearer")
            || matches!(lower.as_str(), "-u" | "--user")
            || authenticated_url(token)
    }) {
        // shortcut: unlabelled secrets need explicit excludeCommands; add source-specific parsing when required.
        return "<redacted command>".into();
    }
    command.to_owned()
}

fn sensitive_key(key: &str) -> bool {
    let normalized = key
        .trim_matches(|ch: char| !ch.is_ascii_alphanumeric())
        .to_ascii_lowercase()
        .replace('-', "_");
    [
        "password",
        "passwd",
        "secret",
        "token",
        "authorization",
        "credential",
        "api_key",
        "apikey",
        "access_key",
        "private_key",
    ]
    .iter()
    .any(|part| normalized.contains(part))
}

fn authenticated_url(token: &str) -> bool {
    token.find("://").is_some_and(|scheme| {
        token[scheme + 3..]
            .split('/')
            .next()
            .is_some_and(|authority| authority.contains('@'))
    })
}

fn excluded(command: &str, patterns: &[Pattern]) -> bool {
    let trimmed = command.trim_start();
    let tokens = trimmed.split_whitespace().collect::<Vec<_>>();
    let first = tokens.first().copied().unwrap_or("");
    let kubectl_secrets = tokens
        .iter()
        .position(|token| is_kubectl(token))
        .and_then(|kubectl| {
            tokens[kubectl + 1..]
                .iter()
                .position(|token| *token == "create")
                .map(|create| kubectl + 1 + create)
        })
        .is_some_and(|create| tokens[create + 1..].contains(&"secret"));
    ["env", "printenv", "set", "export"].contains(&first)
        || tokens.contains(&"sshpass")
        || kubectl_secrets
        || patterns.iter().any(|pattern| pattern.matches(command))
}

fn is_kubectl(token: &str) -> bool {
    token
        .trim_matches(|ch| ch == '\'' || ch == '"')
        .rsplit('/')
        .next()
        == Some("kubectl")
}

fn render_markdown(
    id: Uuid,
    created_at: DateTime<Utc>,
    command: &str,
    input: &RecordInput,
) -> Result<Vec<u8>> {
    let fence = "`".repeat(command.split('`').map(str::len).max().unwrap_or(0).max(3) + 1);
    Ok(format!(
        "---\nsource: shell-memory\nevent_id: {id}\noccurred_at: {}\nhost: {}\nworking_directory: {}\nexit_code: {}\nduration_ms: {}\n---\n\n{fence}text\n{command}\n{fence}\n",
        created_at.to_rfc3339(), serde_json::to_string(&input.host)?,
        serde_json::to_string(&input.cwd)?, input.exit_code, input.duration_ms,
    ).into_bytes())
}

fn set_marker(path: &Path, enabled: bool) -> Result<()> {
    if enabled {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, b"1")?;
        restrict_queue_permissions(path)?;
    } else {
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

#[cfg(unix)]
fn prepare_queue_file(path: &Path) -> Result<()> {
    use std::{
        fs::{OpenOptions, Permissions},
        os::unix::fs::{OpenOptionsExt, PermissionsExt},
    };

    let file = match OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(path)?,
        Err(error) => return Err(error.into()),
    };
    if !file.metadata()?.is_file() {
        bail!("queue path must be a regular file");
    }
    file.set_permissions(Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn prepare_queue_file(path: &Path) -> Result<()> {
    use std::fs::OpenOptions;
    let file = match OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            OpenOptions::new().read(true).write(true).open(path)?
        }
        Err(error) => return Err(error.into()),
    };
    if !file.metadata()?.is_file() {
        bail!("queue path must be a regular file");
    }
    Ok(())
}

#[cfg(unix)]
fn restrict_queue_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn restrict_queue_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(command: &str) -> RecordInput {
        RecordInput {
            command: command.into(),
            cwd: "/tmp/synthetic".into(),
            host: "test-host".into(),
            exit_code: 0,
            duration_ms: 12,
        }
    }

    #[test]
    fn redacts_synthetic_secrets_and_excludes_commands_before_queueing() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let mut queue = Queue::open(&directory.path().join("queue.sqlite3"))?;
        let id = Uuid::new_v4();
        assert!(!queue.record(id, input("env"), &[])?);
        assert!(!queue.record(id, input("git status"), &["git status".into()])?);
        assert!(queue.record(
            id,
            input("curl -H 'Authorization: Bearer synthetic-shell-secret' https://example.test"),
            &[]
        )?);
        let events = queue.pending(id, 10)?;
        assert_eq!(events.len(), 1);
        assert!(String::from_utf8_lossy(&events[0].content).contains("<redacted command>"));
        assert!(!String::from_utf8_lossy(&events[0].content).contains("synthetic-shell-secret"));
        Ok(())
    }

    #[test]
    fn excludes_commands_that_directly_provision_or_pass_secrets() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let mut queue = Queue::open(&directory.path().join("queue.sqlite3"))?;
        let instance_id = Uuid::new_v4();

        assert!(!queue.record(
            instance_id,
            input("sshpass -p synthetic-secret ssh user@example.test"),
            &[]
        )?);
        assert!(!queue.record(
            instance_id,
            input("kubectl create secret generic db --from-literal=password=synthetic-secret"),
            &[]
        )?);
        assert!(!queue.record(
            instance_id,
            input("kubectl -n prod create secret generic db --from-literal=db=synthetic-unlabelled-secret"),
            &[]
        )?);
        assert!(!queue.record(
            instance_id,
            input("/usr/local/bin/kubectl -n prod create secret generic db --from-literal=db=synthetic-path-secret"),
            &[]
        )?);
        assert!(queue.pending(instance_id, 10)?.is_empty());
        Ok(())
    }

    #[test]
    fn retry_keeps_resource_identity_and_control_markers_stop_capture() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("queue.sqlite3");
        let mut queue = Queue::open(&path)?;
        let instance_id = Uuid::new_v4();
        queue.record(instance_id, input("echo synthetic"), &[])?;
        let first = queue.pending(instance_id, 1)?;
        let retry = queue.pending(instance_id, 1)?;
        assert_eq!(first[0].id, retry[0].id);
        queue.set_paused(true)?;
        assert!(queue.paused());
        assert!(queue.record(instance_id, input("echo while paused"), &[])?);
        assert_eq!(queue.pending(instance_id, 10)?.len(), 2);
        queue.set_disabled(true)?;
        assert!(queue.disabled());
        assert!(!queue.record(instance_id, input("echo disabled"), &[])?);
        assert!(queue.acknowledge(first[0].id)?);
        assert_eq!(queue.pending(instance_id, 10)?.len(), 1);
        queue.set_paused(false)?;
        queue.set_disabled(false)?;
        assert!(!queue.paused() && !queue.disabled());
        queue.forget()?;
        assert!(!path.exists());
        let mut queue = Queue::open(&path)?;
        assert!(queue.disabled());
        assert!(!queue.record(instance_id, input("echo after forget"), &[])?);
        assert!(queue.pending(instance_id, 10)?.is_empty());
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn queue_permissions_are_private_before_and_after_reopen() -> Result<()> {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir()?;
        let path = directory.path().join("queue.sqlite3");
        let queue = Queue::open(&path)?;
        assert_eq!(fs::metadata(&path)?.permissions().mode() & 0o777, 0o600);
        drop(queue);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644))?;
        let queue = Queue::open(&path)?;
        assert_eq!(fs::metadata(&path)?.permissions().mode() & 0o777, 0o600);
        drop(queue);
        Ok(())
    }
}
