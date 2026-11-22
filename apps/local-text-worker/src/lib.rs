use anyhow::{bail, Context, Result};
use glob::Pattern;
use rusqlite::{params, Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
#[cfg(not(unix))]
use std::fs::OpenOptions;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use uuid::Uuid;

pub const MAX_TEXT_FILE_BYTES: u64 = 4 * 1024 * 1024;
pub const MAX_FILES_PER_SCAN: usize = 10_000;
pub const MAX_SCAN_BYTES: u64 = 256 * 1024 * 1024;
const MAX_QUEUE_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_SCAN_ENTRIES: usize = 50_000;
const MAX_DIRECTORY_DEPTH: usize = 64;
const DEFAULT_INCLUDES: &[&str] = &[
    "*.md",
    "*.markdown",
    "*.txt",
    "**/*.md",
    "**/*.markdown",
    "**/*.txt",
];

#[derive(Debug, Clone, Deserialize)]
pub struct RootConfig {
    pub path: PathBuf,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    pub root_id: String,
    pub relative_path: String,
    pub revision: String,
    pub observed_modified_at: i64,
    pub content: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct ScanSummary {
    pub observations: Vec<Observation>,
    pub excluded: usize,
}

pub fn scan_root(config: &RootConfig) -> Result<ScanSummary> {
    let original = fs::symlink_metadata(&config.path).with_context(|| {
        format!(
            "unable to inspect configured root: {}",
            config.path.display()
        )
    })?;
    if original.file_type().is_symlink() || !original.is_dir() {
        bail!("configured root must be a real directory, not a symlink");
    }
    let root = config.path.canonicalize().with_context(|| {
        format!(
            "unable to resolve configured root: {}",
            config.path.display()
        )
    })?;
    let root_id = hex::encode(Sha256::digest(root.as_os_str().as_encoded_bytes()));
    let includes = compile_patterns(if config.include.is_empty() {
        DEFAULT_INCLUDES
            .iter()
            .map(|pattern| (*pattern).to_owned())
            .collect()
    } else {
        config.include.clone()
    })?;
    let excludes = compile_patterns(config.exclude.clone())?;

    let mut scan = RootScan {
        root: &root,
        root_id: &root_id,
        includes: &includes,
        excludes: &excludes,
        summary: ScanSummary {
            observations: Vec::new(),
            excluded: 0,
        },
        examined: 0,
        total_bytes: 0,
    };
    scan.directory(&root, 0)?;
    Ok(scan.summary)
}

fn compile_patterns(patterns: Vec<String>) -> Result<Vec<Pattern>> {
    patterns
        .into_iter()
        .map(|pattern| {
            Pattern::new(&pattern).with_context(|| format!("invalid path pattern: {pattern}"))
        })
        .collect()
}

struct RootScan<'a> {
    root: &'a Path,
    root_id: &'a str,
    includes: &'a [Pattern],
    excludes: &'a [Pattern],
    summary: ScanSummary,
    examined: usize,
    total_bytes: u64,
}

impl RootScan<'_> {
    fn directory(&mut self, directory: &Path, depth: usize) -> Result<()> {
        if depth > MAX_DIRECTORY_DEPTH {
            bail!("configured root exceeds the bounded directory depth");
        }
        for entry in
            fs::read_dir(directory).with_context(|| "unable to read a configured text root")?
        {
            self.examined += 1;
            if self.examined > MAX_SCAN_ENTRIES {
                bail!("configured root exceeds the bounded scan entry count");
            }
            let entry = entry?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.file_type().is_symlink() {
                self.summary.excluded += 1;
                continue;
            }
            let relative = path
                .strip_prefix(self.root)
                .context("scanner produced a path outside its configured root")?;
            if has_unsafe_component(relative) {
                self.summary.excluded += 1;
                continue;
            }
            if metadata.is_dir() {
                if is_private_path(relative) || matches_any(self.excludes, relative) {
                    self.summary.excluded += 1;
                } else {
                    self.directory(&path, depth + 1)?;
                }
                continue;
            }
            if !metadata.is_file()
                || is_private_path(relative)
                || matches_any(self.excludes, relative)
                || !is_supported_text_path(relative)
                || !matches_any(self.includes, relative)
            {
                self.summary.excluded += 1;
                continue;
            }
            if metadata.len() > MAX_TEXT_FILE_BYTES {
                bail!("a selected text file exceeds the configured safety limit");
            }
            if self.total_bytes.saturating_add(metadata.len()) > MAX_SCAN_BYTES {
                bail!("configured root exceeds the bounded scan byte limit");
            }
            let canonical = path.canonicalize()?;
            if !canonical.starts_with(self.root) {
                self.summary.excluded += 1;
                continue;
            }
            if self.summary.observations.len() >= MAX_FILES_PER_SCAN {
                bail!("configured root exceeds the bounded scan limit");
            }
            let mut file = open_selected_file(self.root, relative)?.take(MAX_TEXT_FILE_BYTES + 1);
            let mut content = Vec::with_capacity(metadata.len() as usize);
            file.read_to_end(&mut content)?;
            if content.len() as u64 > MAX_TEXT_FILE_BYTES {
                bail!("a selected text file exceeds the configured safety limit");
            }
            self.total_bytes = self.total_bytes.saturating_add(content.len() as u64);
            std::str::from_utf8(&content).context("a selected text file is not valid UTF-8")?;
            let relative_path = relative
                .components()
                .map(|component| match component {
                    Component::Normal(value) => value.to_str().map(str::to_owned),
                    _ => None,
                })
                .collect::<Option<Vec<_>>>()
                .context("source path is not valid UTF-8")?
                .join("/");
            let revision = format!("sha256:{}", hex::encode(Sha256::digest(&content)));
            let observed_modified_at = metadata
                .modified()?
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
                .min(i64::MAX as u64) as i64;
            self.summary.observations.push(Observation {
                root_id: self.root_id.to_owned(),
                relative_path,
                revision,
                observed_modified_at,
                content,
            });
        }
        Ok(())
    }
}

fn matches_any(patterns: &[Pattern], path: &Path) -> bool {
    patterns.iter().any(|pattern| pattern.matches_path(path))
}

fn open_selected_file(root: &Path, relative: &Path) -> Result<std::fs::File> {
    #[cfg(unix)]
    {
        use std::{
            ffi::CString,
            os::fd::{AsRawFd, FromRawFd},
            os::unix::ffi::OsStrExt,
        };
        let components = relative
            .components()
            .map(|component| match component {
                Component::Normal(name) => {
                    CString::new(name.as_bytes()).context("invalid source filename")
                }
                _ => bail!("source path is not relative"),
            })
            .collect::<Result<Vec<_>>>()?;
        let mut directory = std::fs::File::open(root)?;
        for name in &components[..components.len().saturating_sub(1)] {
            let descriptor = unsafe {
                libc::openat(
                    directory.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                )
            };
            if descriptor < 0 {
                return Err(std::io::Error::last_os_error().into());
            }
            directory = unsafe { std::fs::File::from_raw_fd(descriptor) };
        }
        let name = components.last().context("source path is empty")?;
        let descriptor = unsafe {
            libc::openat(
                directory.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK,
            )
        };
        if descriptor < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let file = unsafe { std::fs::File::from_raw_fd(descriptor) };
        if !file.metadata()?.is_file() {
            bail!("selected source is not a regular file");
        }
        Ok(file)
    }

    #[cfg(not(unix))]
    {
        let path = root.join(relative);
        let canonical = path.canonicalize()?;
        if !canonical.starts_with(root) {
            bail!("selected source is outside its configured root");
        }
        let mut options = OpenOptions::new();
        options.read(true);
        let file = options.open(canonical)?;
        if !file.metadata()?.is_file() {
            bail!("selected source is not a regular file");
        }
        Ok(file)
    }
}

fn has_unsafe_component(path: &Path) -> bool {
    path.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    })
}

fn is_private_path(path: &Path) -> bool {
    path.components().any(|component| {
        let Component::Normal(component) = component else {
            return true;
        };
        let Some(name) = component.to_str() else {
            return true;
        };
        let name = name.to_ascii_lowercase();
        name.starts_with('.')
            || name.contains("credential")
            || name.contains("secret")
            || name.contains("password")
            || name.contains("token")
            || name.starts_with("id_")
            || matches!(
                name.as_str(),
                "credentials"
                    | "secrets"
                    | "id_rsa"
                    | "id_ed25519"
                    | "authorized_keys"
                    | "known_hosts"
            )
            || [
                ".env", ".pem", ".key", ".p12", ".pfx", ".kdbx", ".crt", ".cer", ".der",
            ]
            .iter()
            .any(|suffix| name.ends_with(suffix))
    })
}

fn is_supported_text_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "md" | "markdown" | "txt"
            )
        })
}

pub struct OfflineQueue {
    connection: Connection,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuedObservation {
    pub id: Uuid,
    pub instance_id: Uuid,
    pub observation: Observation,
}

impl OfflineQueue {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        if let Ok(metadata) = fs::symlink_metadata(path) {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                bail!("offline queue path must be a regular file");
            }
        }
        let connection = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        )?;
        restrict_local_state_permissions(path, 0o600)?;
        connection.pragma_update(None, "journal_mode", "DELETE")?;
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS local_text_queue (
                id TEXT PRIMARY KEY,
                instance_id TEXT NOT NULL,
                root_id TEXT NOT NULL,
                relative_path TEXT NOT NULL,
                revision TEXT NOT NULL,
                observed_modified_at INTEGER NOT NULL,
                content BLOB NOT NULL,
                acknowledged INTEGER NOT NULL DEFAULT 0,
                target_resource_ref TEXT,
                UNIQUE(instance_id, root_id, relative_path, revision)
            );",
        )?;
        Ok(Self { connection })
    }

    pub fn enqueue(&mut self, instance_id: Uuid, observation: &Observation) -> Result<Uuid> {
        let id = Uuid::new_v4();
        let inserted = self.connection.execute(
            "INSERT INTO local_text_queue (id, instance_id, root_id, relative_path, revision, observed_modified_at, content)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(instance_id, root_id, relative_path, revision) DO NOTHING",
            params![id.to_string(), instance_id.to_string(), observation.root_id,
                observation.relative_path, observation.revision, observation.observed_modified_at,
                observation.content],
        )?;
        if inserted == 1 {
            let queued_bytes: i64 = self.connection.query_row(
                "SELECT COALESCE(SUM(length(content)), 0) FROM local_text_queue",
                [],
                |row| row.get(0),
            )?;
            if queued_bytes as u64 > MAX_QUEUE_BYTES {
                self.connection.execute(
                    "DELETE FROM local_text_queue WHERE id = ?1",
                    [id.to_string()],
                )?;
                bail!("offline queue exceeds its configured byte limit");
            }
        }
        let stored_id: String = self.connection.query_row(
            "SELECT id FROM local_text_queue WHERE instance_id = ?1 AND root_id = ?2 AND relative_path = ?3 AND revision = ?4",
            params![instance_id.to_string(), observation.root_id, observation.relative_path,
                observation.revision],
            |row| row.get(0),
        )?;
        Ok(Uuid::parse_str(&stored_id)?)
    }

    pub fn pending(&self, instance_id: Uuid, limit: usize) -> Result<Vec<QueuedObservation>> {
        let mut statement = self.connection.prepare(
            "SELECT id, root_id, relative_path, revision, observed_modified_at, content
             FROM local_text_queue WHERE instance_id = ?1 AND acknowledged = 0
             ORDER BY rowid LIMIT ?2",
        )?;
        let rows = statement.query_map(params![instance_id.to_string(), limit as i64], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, Vec<u8>>(5)?,
            ))
        })?;
        rows.map(|row| {
            let (id, root_id, relative_path, revision, observed_modified_at, content) = row?;
            Ok(QueuedObservation {
                id: Uuid::parse_str(&id)?,
                instance_id,
                observation: Observation {
                    root_id,
                    relative_path,
                    revision,
                    observed_modified_at,
                    content,
                },
            })
        })
        .collect()
    }

    pub fn acknowledge(&self, id: Uuid, target_resource_ref: &str) -> Result<bool> {
        Ok(self.connection.execute(
            "UPDATE local_text_queue SET acknowledged = 1, target_resource_ref = ?2 WHERE id = ?1",
            params![id.to_string(), target_resource_ref],
        )? == 1)
    }
}

#[cfg(unix)]
fn restrict_local_state_permissions(path: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    Ok(())
}

#[cfg(not(unix))]
fn restrict_local_state_permissions(_path: &Path, _mode: u32) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn config(path: PathBuf) -> RootConfig {
        RootConfig {
            path,
            include: Vec::new(),
            exclude: Vec::new(),
        }
    }

    #[test]
    fn scan_only_returns_safe_utf8_text_and_skips_private_and_symlink_paths() {
        let root = TempDir::new().unwrap();
        fs::create_dir(root.path().join(".private")).unwrap();
        fs::write(root.path().join("notes.md"), "hello").unwrap();
        fs::write(root.path().join(".env"), "API_KEY=secret").unwrap();
        fs::write(root.path().join(".private/hidden.md"), "private").unwrap();
        fs::write(root.path().join("bad.md"), [0xff, 0xfe]).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("/etc/passwd", root.path().join("escape.md")).unwrap();
        let result = scan_root(&config(root.path().to_path_buf()));
        assert!(
            result.is_err(),
            "malformed selected content must fail closed"
        );
        fs::remove_file(root.path().join("bad.md")).unwrap();
        let result = scan_root(&config(root.path().to_path_buf())).unwrap();
        assert_eq!(result.observations.len(), 1);
        assert_eq!(result.observations[0].relative_path, "notes.md");
    }

    #[test]
    fn privacy_filter_rejects_common_secret_and_key_names() {
        for path in [
            ".ssh/id_ed25519",
            "notes/passwords.md",
            "exports/api-token.txt",
            "keys/id_ecdsa",
            "certificates/service.crt",
        ] {
            assert!(is_private_path(Path::new(path)), "{path}");
        }
    }

    #[test]
    fn queue_survives_reopen_and_deduplicates_the_same_revision() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("queue.sqlite3");
        let instance_id = Uuid::new_v4();
        let observation = Observation {
            root_id: "root".into(),
            relative_path: "notes.md".into(),
            revision: "sha256:abc".into(),
            observed_modified_at: 1,
            content: b"hello".to_vec(),
        };
        let queue_id = {
            let mut queue = OfflineQueue::open(&path).unwrap();
            let id = queue.enqueue(instance_id, &observation).unwrap();
            assert_eq!(queue.enqueue(instance_id, &observation).unwrap(), id);
            id
        };
        let queue = OfflineQueue::open(&path).unwrap();
        let pending = queue.pending(instance_id, 10).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, queue_id);
        assert_eq!(pending[0].observation.content, observation.content);
        assert!(queue
            .acknowledge(queue_id, "elembra://io.elembra.files/file/1")
            .unwrap());
        assert!(queue.pending(instance_id, 10).unwrap().is_empty());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn equal_relative_paths_in_distinct_roots_keep_distinct_queue_identity() {
        let first_root = TempDir::new().unwrap();
        let second_root = TempDir::new().unwrap();
        fs::write(first_root.path().join("notes.md"), "first").unwrap();
        fs::write(second_root.path().join("notes.md"), "second").unwrap();
        let first = scan_root(&config(first_root.path().to_path_buf())).unwrap();
        let second = scan_root(&config(second_root.path().to_path_buf())).unwrap();
        let dir = TempDir::new().unwrap();
        let instance_id = Uuid::new_v4();
        let mut queue = OfflineQueue::open(&dir.path().join("queue.sqlite3")).unwrap();
        queue.enqueue(instance_id, &first.observations[0]).unwrap();
        queue.enqueue(instance_id, &second.observations[0]).unwrap();
        assert_eq!(queue.pending(instance_id, 10).unwrap().len(), 2);
    }

    #[test]
    fn configured_exclusions_and_file_size_limit_are_enforced() {
        let root = TempDir::new().unwrap();
        fs::write(root.path().join("keep.md"), "ok").unwrap();
        fs::write(root.path().join("skip.md"), "private").unwrap();
        let mut cfg = config(root.path().to_path_buf());
        cfg.exclude.push("skip.md".into());
        assert_eq!(scan_root(&cfg).unwrap().observations.len(), 1);
        std::fs::write(root.path().join("binary.pdf"), b"not text").unwrap();
        cfg.include = vec!["**/*".into()];
        assert_eq!(scan_root(&cfg).unwrap().observations.len(), 1);
        let mut large = std::fs::File::create(root.path().join("large.md")).unwrap();
        large
            .write_all(&vec![b'x'; MAX_TEXT_FILE_BYTES as usize + 1])
            .unwrap();
        assert!(scan_root(&cfg).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn root_symlinks_and_symlinked_files_are_never_followed() {
        let parent = TempDir::new().unwrap();
        let root = parent.path().join("root");
        fs::create_dir(&root).unwrap();
        fs::write(parent.path().join("outside.md"), "outside").unwrap();
        std::os::unix::fs::symlink(parent.path().join("outside.md"), root.join("escape.md"))
            .unwrap();
        std::os::unix::fs::symlink(&root, parent.path().join("root-link")).unwrap();
        assert!(scan_root(&config(parent.path().join("root-link"))).is_err());
        let scan = scan_root(&config(root)).unwrap();
        assert!(scan.observations.is_empty());
        assert_eq!(scan.excluded, 1);
    }

    #[test]
    fn parent_components_in_configured_patterns_cannot_select_outside_files() {
        let parent = TempDir::new().unwrap();
        let root = parent.path().join("root");
        fs::create_dir(&root).unwrap();
        fs::write(parent.path().join("outside.md"), "outside").unwrap();
        fs::write(root.join("inside.md"), "inside").unwrap();
        let mut cfg = config(root);
        cfg.include = vec!["../**/*.md".into()];
        assert!(scan_root(&cfg).unwrap().observations.is_empty());
    }
}
