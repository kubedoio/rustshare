use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use glob::Pattern;
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
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
    #[serde(default)]
    pub session_id: Option<Uuid>,
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
    pub session_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceMapping {
    pub id: Uuid,
    pub revision: String,
    pub created_at: DateTime<Utc>,
    pub session_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalRecord {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub uploaded: bool,
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
        connection.pragma_update(None, "secure_delete", "ON")?;
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
                content BLOB NOT NULL,
                session_id TEXT
            );",
        )?;
        let has_session_id: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('shell_events') WHERE name = 'session_id')",
            [], |row| row.get(0),
        )?;
        if !has_session_id {
            connection.execute("ALTER TABLE shell_events ADD COLUMN session_id TEXT", [])?;
        }
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS shell_imports (
                id TEXT PRIMARY KEY,
                instance_id TEXT NOT NULL,
                revision TEXT NOT NULL,
                created_at TEXT NOT NULL,
                session_id TEXT,
                forget_requested INTEGER NOT NULL DEFAULT 0
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
            "INSERT INTO shell_events (id, instance_id, created_at, command, cwd, host, exit_code, duration_ms, revision, content, session_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![id.to_string(), instance_id.to_string(), created_at.to_rfc3339(), command,
                input.cwd, input.host, input.exit_code, input.duration_ms.min(i64::MAX as u64) as i64,
                revision, content, input.session_id.map(|id| id.to_string())],
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
            "SELECT id, created_at, command, cwd, host, exit_code, duration_ms, revision, content, session_id
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
                    row.get::<_, Option<String>>(9)?,
                ))
            },
        )?;
        rows.map(|row| {
            let (
                id,
                created_at,
                command,
                cwd,
                host,
                exit_code,
                duration_ms,
                revision,
                content,
                session_id,
            ) = row?;
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
                session_id: session_id.map(|id| Uuid::parse_str(&id)).transpose()?,
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

    /// Persist only source identity and revision after successful import. Never stores the Files target.
    pub fn remember_import(&self, instance_id: Uuid, event: &Event) -> Result<()> {
        self.connection.execute(
            "INSERT INTO shell_imports (id, instance_id, revision, created_at, session_id)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET instance_id=excluded.instance_id,
               revision=excluded.revision, created_at=excluded.created_at, session_id=excluded.session_id
             WHERE shell_imports.instance_id=excluded.instance_id AND shell_imports.revision=excluded.revision",
            params![event.id.to_string(), instance_id.to_string(), event.revision,
                event.created_at.to_rfc3339(), event.session_id.map(|id| id.to_string())],
        )?;
        let stored: (String, String) = self.connection.query_row(
            "SELECT instance_id, revision FROM shell_imports WHERE id=?1",
            [event.id.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        if stored != (instance_id.to_string(), event.revision.clone()) {
            bail!("local Shell Memory source mapping conflicts with an existing revision");
        }
        Ok(())
    }

    pub fn list(&self, instance_id: Uuid, limit: usize) -> Result<Vec<LocalRecord>> {
        let mut statement = self.connection.prepare(
            "SELECT id, MAX(created_at), MAX(uploaded) FROM (
               SELECT id, created_at, 0 AS uploaded FROM shell_events WHERE instance_id=?1
               UNION ALL
               SELECT id, created_at, 1 AS uploaded FROM shell_imports WHERE instance_id=?1
             ) GROUP BY id ORDER BY MAX(created_at) DESC, id LIMIT ?2",
        )?;
        let rows = statement.query_map(
            params![instance_id.to_string(), limit.min(100) as i64],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, bool>(2)?,
                ))
            },
        )?;
        rows.map(|row| {
            let (id, created_at, uploaded) = row?;
            Ok(LocalRecord {
                id: Uuid::parse_str(&id)?,
                created_at: DateTime::parse_from_rfc3339(&created_at)?.with_timezone(&Utc),
                uploaded,
            })
        })
        .collect()
    }

    pub fn last_id(&self, instance_id: Uuid) -> Result<Option<Uuid>> {
        self.connection
            .query_row(
                "SELECT id FROM (
               SELECT id, created_at FROM shell_events WHERE instance_id=?1
               UNION ALL SELECT id, created_at FROM shell_imports WHERE instance_id=?1
             ) ORDER BY created_at DESC LIMIT 1",
                [instance_id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .map(|id| Uuid::parse_str(&id).map_err(Into::into))
            .transpose()
    }

    /// Marks a forget durably and erases queued command content before any network operation.
    pub fn request_forget(&self, instance_id: Uuid, id: Uuid) -> Result<Option<SourceMapping>> {
        let tx = self.connection.unchecked_transaction()?;
        let queued: Option<(String, String, Option<String>)> = tx.query_row(
            "SELECT revision, created_at, session_id FROM shell_events WHERE id=?1 AND instance_id=?2",
            params![id.to_string(), instance_id.to_string()], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ).optional()?;
        let mapped: Option<(String, String, Option<String>, bool)> = tx.query_row(
            "SELECT revision, created_at, session_id, forget_requested FROM shell_imports WHERE id=?1 AND instance_id=?2",
            params![id.to_string(), instance_id.to_string()], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        ).optional()?;
        let values = match (queued, mapped) {
            (Some((revision, _created_at, _session_id)), Some((mapped_revision, _, _, _)))
                if revision != mapped_revision =>
            {
                bail!("queued source revision conflicts with imported mapping")
            }
            (Some(values), _) => values,
            (None, Some((revision, created_at, session_id, _))) => {
                (revision, created_at, session_id)
            }
            (None, None) => {
                tx.commit()?;
                return Ok(None);
            }
        };
        tx.execute(
            "INSERT INTO shell_imports (id, instance_id, revision, created_at, session_id, forget_requested)
             VALUES (?1, ?2, ?3, ?4, ?5, 1)
             ON CONFLICT(id) DO UPDATE SET forget_requested=1
             WHERE shell_imports.instance_id=excluded.instance_id AND shell_imports.revision=excluded.revision",
            params![id.to_string(), instance_id.to_string(), values.0, values.1, values.2],
        )?;
        tx.execute(
            "DELETE FROM shell_events WHERE id=?1 AND instance_id=?2",
            params![id.to_string(), instance_id.to_string()],
        )?;
        tx.commit()?;
        let created_at = DateTime::parse_from_rfc3339(&values.1)?.with_timezone(&Utc);
        Ok(Some(SourceMapping {
            id,
            revision: values.0,
            created_at,
            session_id: values.2.map(|id| Uuid::parse_str(&id)).transpose()?,
        }))
    }

    pub fn pending_forgets(&self, instance_id: Uuid, limit: usize) -> Result<Vec<SourceMapping>> {
        let mut statement = self.connection.prepare("SELECT id, revision, created_at, session_id FROM shell_imports WHERE instance_id=?1 AND forget_requested=1 ORDER BY created_at LIMIT ?2")?;
        let rows = statement.query_map(
            params![instance_id.to_string(), limit.min(100) as i64],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            },
        )?;
        rows.map(|row| {
            let (id, revision, created_at, session_id) = row?;
            Ok(SourceMapping {
                id: Uuid::parse_str(&id)?,
                revision,
                created_at: DateTime::parse_from_rfc3339(&created_at)?.with_timezone(&Utc),
                session_id: session_id.map(|id| Uuid::parse_str(&id)).transpose()?,
            })
        })
        .collect()
    }

    pub fn acknowledge_forget(&self, instance_id: Uuid, id: Uuid) -> Result<()> {
        self.connection.execute(
            "DELETE FROM shell_imports WHERE id=?1 AND instance_id=?2 AND forget_requested=1",
            params![id.to_string(), instance_id.to_string()],
        )?;
        Ok(())
    }

    pub fn request_forget_session(&self, instance_id: Uuid, session_id: Uuid) -> Result<usize> {
        let ids = {
            let mut stmt = self.connection.prepare("SELECT id FROM shell_events WHERE instance_id=?1 AND session_id=?2 UNION SELECT id FROM shell_imports WHERE instance_id=?1 AND session_id=?2")?;
            stmt.query_map(
                params![instance_id.to_string(), session_id.to_string()],
                |row| row.get::<_, String>(0),
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?
        };
        let mut count = 0;
        for id in ids {
            let id = Uuid::parse_str(&id)?;
            if self.request_forget(instance_id, id)?.is_some() {
                count += 1;
            }
        }
        Ok(count)
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
    let tokens = command.split_whitespace().collect::<Vec<_>>();
    if tokens.iter().copied().any(|token| {
        let lower = token.to_ascii_lowercase();
        let key = token.split_once('=').map_or(token, |(key, _)| key);
        sensitive_key(key)
            || lower.contains("bearer")
            || matches!(lower.as_str(), "-u" | "--user")
            || authenticated_url(token)
    }) || docker_login_uses_short_password(&tokens)
    {
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
        "pass",
        "pwd",
        "secret",
        "token",
        "authorization",
        "credential",
        "cookie",
        "session",
        "api_key",
        "apikey",
        "access_key",
        "private_key",
    ]
    .iter()
    .any(|part| normalized.contains(part))
}

fn docker_login_uses_short_password(tokens: &[&str]) -> bool {
    tokens
        .iter()
        .position(|token| is_program(token, "docker"))
        .and_then(|docker| {
            tokens[docker + 1..]
                .iter()
                .position(|token| *token == "login")
                .map(|login| docker + 1 + login)
        })
        .is_some_and(|login| {
            tokens[login + 1..]
                .iter()
                .any(|token| *token == "-p" || token.starts_with("-p="))
        })
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
    let literal_secret = tokens.iter().any(|token| {
        token
            .trim_matches(|ch| ch == '\'' || ch == '"')
            .starts_with("--from-literal")
    });
    ["env", "printenv", "set", "export"].contains(&first)
        || tokens.iter().any(|token| is_program(token, "sshpass"))
        || kubectl_secrets
        || literal_secret
        || patterns.iter().any(|pattern| pattern.matches(command))
}

fn is_kubectl(token: &str) -> bool {
    is_program(token, "kubectl")
}

fn is_program(token: &str, program: &str) -> bool {
    token
        .trim_matches(|ch| ch == '\'' || ch == '"')
        .rsplit('/')
        .next()
        == Some(program)
}

fn render_markdown(
    id: Uuid,
    created_at: DateTime<Utc>,
    command: &str,
    input: &RecordInput,
) -> Result<Vec<u8>> {
    let fence = "`".repeat(command.split('`').map(str::len).max().unwrap_or(0).max(3) + 1);
    Ok(format!(
        "---\nsource: shell-memory\nshell: zsh\nevent_id: {id}\noccurred_at: {}\nhost: {}\nworking_directory: {}\nexit_code: {}\nduration_ms: {}\n---\n\n{fence}text\n{command}\n{fence}\n",
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

    #[test]
    fn config_round_trip_uses_camel_case_exclusion_key() -> Result<()> {
        let config = Config {
            server: "https://example.test".into(),
            instance_id: Uuid::new_v4(),
            queue: PathBuf::from("/tmp/synthetic.sqlite3"),
            exclude_commands: vec!["deploy *".into()],
        };
        let value = serde_json::to_value(&config)?;
        assert_eq!(value["excludeCommands"][0], "deploy *");
        assert!(value.get("exclude_commands").is_none());
        let decoded: Config = serde_json::from_value(value)?;
        assert_eq!(decoded.exclude_commands, ["deploy *"]);
        Ok(())
    }

    #[test]
    fn queue_enables_sqlite_secure_delete() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let queue = Queue::open(&directory.path().join("queue.sqlite3"))?;
        let enabled: bool = queue
            .connection
            .query_row("PRAGMA secure_delete", [], |row| row.get(0))?;
        assert!(enabled);
        Ok(())
    }

    fn input(command: &str) -> RecordInput {
        RecordInput {
            command: command.into(),
            cwd: "/tmp/synthetic".into(),
            host: "test-host".into(),
            exit_code: 0,
            duration_ms: 12,
            session_id: None,
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
        let content = String::from_utf8_lossy(&events[0].content);
        assert!(content.contains("shell: zsh\n"));
        assert!(content.contains("<redacted command>"));
        assert!(!content.contains("synthetic-shell-secret"));
        Ok(())
    }

    #[test]
    fn redacts_cookie_and_session_credentials_before_queueing() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let mut queue = Queue::open(&directory.path().join("queue.sqlite3"))?;
        let instance_id = Uuid::new_v4();

        assert!(queue.record(
            instance_id,
            input("curl -H 'Cookie: synthetic_value_9876' https://example.test"),
            &[],
        )?);
        assert!(queue.record(
            instance_id,
            input("curl -H 'X-Session-Id: synthetic_value_1234' https://example.test"),
            &[],
        )?);

        let events = queue.pending(instance_id, 10)?;
        assert_eq!(events.len(), 2);
        for event in &events {
            assert_eq!(event.command, "<redacted command>");
            let content = String::from_utf8_lossy(&event.content);
            assert!(content.contains("<redacted command>"));
            assert!(!content.contains("synthetic_value_9876"));
            assert!(!content.contains("synthetic_value_1234"));
        }
        Ok(())
    }

    #[test]
    fn redacts_common_password_forms_before_queueing() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let mut queue = Queue::open(&directory.path().join("queue.sqlite3"))?;
        let instance_id = Uuid::new_v4();
        let commands = [
            "DB_PASS=synthetic_db_password ./client status",
            "MYSQL_PWD=synthetic_mysql_password mysql -e 'select 1'",
            "docker login -p synthetic_docker_password registry.example.test",
            "docker login --pass=synthetic_option_password registry.example.test",
        ];

        for command in commands {
            assert!(queue.record(instance_id, input(command), &[])?);
        }

        let events = queue.pending(instance_id, 10)?;
        assert_eq!(events.len(), commands.len());
        for event in events {
            assert_eq!(event.command, "<redacted command>");
            let content = String::from_utf8_lossy(&event.content);
            assert!(content.contains("<redacted command>"));
            for secret in [
                "synthetic_db_password",
                "synthetic_mysql_password",
                "synthetic_docker_password",
                "synthetic_option_password",
            ] {
                assert!(!content.contains(secret));
            }
        }
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
            input("/usr/bin/sshpass -p synthetic-path-secret ssh user@example.test"),
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
        assert!(!queue.record(
            instance_id,
            input("k create secret generic db '--from-literal=db=synthetic-alias-secret'"),
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

    #[test]
    fn forget_erases_queued_content_and_retains_only_minimal_retry_identity() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("queue.sqlite3");
        let instance_id = Uuid::new_v4();
        let session_id = Uuid::new_v4();
        let mut queue = Queue::open(&path)?;
        let mut input = input("echo synthetic-forget-content");
        input.session_id = Some(session_id);
        queue.record(instance_id, input, &[])?;
        let event = queue.pending(instance_id, 1)?.remove(0);
        queue.remember_import(instance_id, &event)?;
        queue.acknowledge(event.id)?;

        let requested = queue.request_forget(instance_id, event.id)?.unwrap();
        assert_eq!(requested.revision, event.revision);
        assert_eq!(requested.session_id, Some(session_id));
        assert!(queue.pending(instance_id, 1)?.is_empty());
        assert_eq!(queue.pending_forgets(instance_id, 1)?.len(), 1);
        assert_eq!(queue.last_id(instance_id)?, Some(event.id));
        assert_eq!(
            queue.request_forget(instance_id, event.id)?.unwrap(),
            requested
        );

        let columns = {
            let mut statement = queue
                .connection
                .prepare("SELECT name FROM pragma_table_info('shell_imports')")?;
            statement
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?
        };
        assert_eq!(
            columns.join(","),
            "id,instance_id,revision,created_at,session_id,forget_requested"
        );
        let bytes = fs::read(&path)?;
        assert!(!bytes
            .windows(b"synthetic-forget-content".len())
            .any(|window| window == b"synthetic-forget-content"));

        queue.acknowledge_forget(instance_id, event.id)?;
        assert!(queue.pending_forgets(instance_id, 1)?.is_empty());
        Ok(())
    }

    #[test]
    fn forget_session_marks_queued_records_and_erases_their_content() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("queue.sqlite3");
        let instance_id = Uuid::new_v4();
        let session_id = Uuid::new_v4();
        let mut queue = Queue::open(&path)?;
        let mut first = input("echo synthetic-session-one");
        first.session_id = Some(session_id);
        let mut second = input("echo synthetic-session-two");
        second.session_id = Some(session_id);
        let mut other = input("echo synthetic-other-session");
        other.session_id = Some(Uuid::new_v4());
        queue.record(instance_id, first, &[])?;
        queue.record(instance_id, second, &[])?;
        queue.record(instance_id, other, &[])?;
        assert_eq!(queue.request_forget_session(instance_id, session_id)?, 2);
        assert_eq!(queue.pending(instance_id, 10)?.len(), 1);
        assert_eq!(queue.pending_forgets(instance_id, 10)?.len(), 2);
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
