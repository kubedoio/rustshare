# Sync Engine Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement a complete bidirectional file sync engine with real-time local watching, WebSocket remote notifications, conflict resolution, and resumable uploads.

**Architecture:** Scanner detects changes locally and remotely, Planner generates sync operations with conflict resolution, Executor performs transfers with concurrency control and retry logic, all orchestrated by a state machine sync loop.

**Tech Stack:** Rust, Tokio, notify (FS watching), tokio-tungstenite (WebSocket), sha2 (hashing), SQLite (state persistence)

---

## Task 1: Extend SQLite Schema for Sync State

**Files:**
- Modify: `crates/client-state/src/lib.rs`
- Modify: `crates/client-state/src/db/sqlite.rs` (if exists, or create schema in lib.rs)

**Step 1: Add sync state columns to sync_roots table**

Add to existing sync_roots table or create new file_states table in `lib.rs`:

```rust
pub fn create_sync_tables(&self) -> Result<()> {
    let conn = self.conn.lock().unwrap();
    
    // File state tracking
    conn.execute(
        "CREATE TABLE IF NOT EXISTS file_states (
            id INTEGER PRIMARY KEY,
            root_id BLOB NOT NULL,
            relative_path TEXT NOT NULL,
            local_hash TEXT,
            remote_hash TEXT,
            local_modified_at INTEGER,
            remote_modified_at INTEGER,
            size INTEGER,
            is_directory BOOLEAN DEFAULT 0,
            sync_status TEXT DEFAULT 'synced',
            last_sync_at INTEGER,
            UNIQUE(root_id, relative_path)
        )",
        [],
    )?;
    
    // Pending operations queue
    conn.execute(
        "CREATE TABLE IF NOT EXISTS sync_queue (
            id INTEGER PRIMARY KEY,
            root_id BLOB NOT NULL,
            operation TEXT NOT NULL,
            relative_path TEXT NOT NULL,
            priority INTEGER DEFAULT 0,
            retry_count INTEGER DEFAULT 0,
            last_error TEXT,
            created_at INTEGER,
            execute_at INTEGER
        )",
        [],
    )?;
    
    // Upload sessions for resumable transfers
    conn.execute(
        "CREATE TABLE IF NOT EXISTS upload_sessions (
            id INTEGER PRIMARY KEY,
            file_state_id INTEGER,
            session_id TEXT,
            total_chunks INTEGER,
            uploaded_chunks INTEGER DEFAULT 0,
            chunk_size INTEGER DEFAULT 5242880,
            expires_at INTEGER
        )",
        [],
    )?;
    
    // Sync cursors for delta tracking
    conn.execute(
        "CREATE TABLE IF NOT EXISTS sync_cursors (
            root_id BLOB PRIMARY KEY,
            cursor TEXT,
            updated_at INTEGER
        )",
        [],
    )?;
    
    Ok(())
}
```

**Step 2: Run cargo check**

```bash
cargo check -p client-state
```

**Step 3: Commit**

```bash
git add crates/client-state/src/lib.rs
git commit -m "feat(db): add sync state tables (file_states, sync_queue, upload_sessions, sync_cursors)"
```

---

## Task 2: Create File Scanner Module

**Files:**
- Create: `crates/sync-engine/src/scanner.rs`
- Modify: `crates/sync-engine/src/lib.rs` (add module)

**Step 1: Create scanner.rs with local scan function**

```rust
//! File system scanner for detecting local changes

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, trace};
use walkdir::WalkDir;

/// Result of scanning a single file
#[derive(Debug, Clone)]
pub struct FileScanResult {
    pub relative_path: PathBuf,
    pub absolute_path: PathBuf,
    pub hash: String,
    pub size: u64,
    pub modified_at: u64,
    pub is_directory: bool,
}

/// Scan a sync root for all files
pub fn scan_local_root(root_path: &Path) -> Result<Vec<FileScanResult>> {
    let mut results = Vec::new();
    
    for entry in WalkDir::new(root_path).follow_links(false) {
        let entry = entry.context("Failed to read directory entry")?;
        let absolute_path = entry.path();
        
        // Skip the root itself
        if absolute_path == root_path {
            continue;
        }
        
        let relative_path = absolute_path
            .strip_prefix(root_path)
            .context("Path not within root")?;
        
        let metadata = match entry.metadata() {
            Ok(m) => m,
            Err(e) => {
                debug!("Skipping {:?}: {}", relative_path, e);
                continue;
            }
        };
        
        let is_directory = metadata.is_dir();
        let size = metadata.len();
        let modified_at = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        
        // Compute hash for files (not directories)
        let hash = if is_directory {
            String::new()
        } else {
            match compute_file_hash(absolute_path) {
                Ok(h) => h,
                Err(e) => {
                    debug!("Failed to hash {:?}: {}", relative_path, e);
                    continue;
                }
            }
        };
        
        results.push(FileScanResult {
            relative_path: relative_path.to_path_buf(),
            absolute_path: absolute_path.to_path_buf(),
            hash,
            size,
            modified_at,
            is_directory,
        });
        
        trace!("Scanned: {:?}", relative_path);
    }
    
    debug!("Scan complete: {} files", results.len());
    Ok(results)
}

/// Compute SHA-256 hash of a file
fn compute_file_hash(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path).context("Failed to open file")?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher).context("Failed to read file")?;
    let hash = hasher.finalize();
    Ok(format!("{:x}", hash))
}

/// Compare scan results with database state to find changes
pub fn detect_local_changes(
    scanned: &[FileScanResult],
    db_state: &HashMap<PathBuf, (String, u64)>, // (hash, modified_at)
) -> LocalChanges {
    let mut changes = LocalChanges::default();
    let scanned_map: HashMap<_, _> = scanned
        .iter()
        .map(|r| (&r.relative_path, (&r.hash, r.modified_at)))
        .collect();
    
    // Find new and modified files
    for result in scanned {
        match db_state.get(&result.relative_path) {
            None => {
                // New file
                changes.created.push(result.clone());
            }
            Some((db_hash, db_mtime)) => {
                if &result.hash != db_hash || result.modified_at != *db_mtime {
                    // Modified
                    changes.modified.push(result.clone());
                }
            }
        }
    }
    
    // Find deleted files
    for (path, _) in db_state {
        if !scanned_map.contains_key(path) {
            changes.deleted.push(path.clone());
        }
    }
    
    changes
}

#[derive(Debug, Default)]
pub struct LocalChanges {
    pub created: Vec<FileScanResult>,
    pub modified: Vec<FileScanResult>,
    pub deleted: Vec<PathBuf>,
}
```

**Step 2: Add module to lib.rs**

```rust
pub mod scanner;
```

**Step 3: Add sha2 dependency**

Add to `crates/sync-engine/Cargo.toml`:
```toml
sha2 = "0.10"
walkdir = "2.5"
```

**Step 4: Run cargo check**

```bash
cargo check -p sync-engine
```

**Step 5: Commit**

```bash
git add crates/sync-engine/src/scanner.rs crates/sync-engine/src/lib.rs crates/sync-engine/Cargo.toml
git commit -m "feat(sync-engine): add local file scanner with SHA-256 hashing

- scan_local_root() walks directory tree
- compute_file_hash() generates SHA-256
- detect_local_changes() compares with DB state
- Returns created, modified, deleted file lists"
```

---

## Task 3: Create Planner Module

**Files:**
- Create: `crates/sync-engine/src/planner.rs`
- Modify: `crates/sync-engine/src/lib.rs`

**Step 1: Create planner.rs**

```rust
//! Sync planner - generates operations from local and remote changes

use crate::scanner::FileScanResult;
use anyhow::Result;
use std::path::PathBuf;
use tracing::debug;
use uuid::Uuid;

/// A sync operation to execute
#[derive(Debug, Clone)]
pub enum SyncOp {
    Upload {
        root_id: Uuid,
        relative_path: PathBuf,
        local_path: PathBuf,
        size: u64,
    },
    Download {
        root_id: Uuid,
        relative_path: PathBuf,
        remote_file_id: Uuid,
        remote_hash: String,
        size: u64,
    },
    DeleteLocal {
        root_id: Uuid,
        relative_path: PathBuf,
    },
    DeleteRemote {
        root_id: Uuid,
        relative_path: PathBuf,
        remote_file_id: Uuid,
    },
}

/// A detected conflict
#[derive(Debug, Clone)]
pub struct Conflict {
    pub root_id: Uuid,
    pub relative_path: PathBuf,
    pub local_modified_at: u64,
    pub remote_modified_at: u64,
    pub resolution: ConflictResolution,
}

#[derive(Debug, Clone)]
pub enum ConflictResolution {
    UploadLocal,      // Local is newer
    DownloadRemote,   // Remote is newer
}

/// The complete sync plan
#[derive(Debug, Default)]
pub struct SyncPlan {
    pub uploads: Vec<SyncOp>,
    pub downloads: Vec<SyncOp>,
    pub deletes: Vec<SyncOp>,
    pub conflicts: Vec<Conflict>,
}

impl SyncPlan {
    pub fn is_empty(&self) -> bool {
        self.uploads.is_empty()
            && self.downloads.is_empty()
            && self.deletes.is_empty()
            && self.conflicts.is_empty()
    }
    
    pub fn total_operations(&self) -> usize {
        self.uploads.len() + self.downloads.len() + self.deletes.len()
    }
}

/// Generate sync plan from local and remote state
pub fn generate_plan(
    root_id: Uuid,
    root_path: &PathBuf,
    local_files: &[FileScanResult],
    remote_files: &[RemoteFileInfo],
    db_state: &[(PathBuf, String, u64)], // (path, hash, modified_at)
) -> Result<SyncPlan> {
    let mut plan = SyncPlan::default();
    
    let db_map: std::collections::HashMap<_, _> = db_state
        .iter()
        .map(|(p, h, m)| (p.clone(), (h.clone(), *m)))
        .collect();
    
    let remote_map: std::collections::HashMap<_, _> = remote_files
        .iter()
        .map(|r| (r.relative_path.clone(), r))
        .collect();
    
    // Check each local file
    for local in local_files {
        let db_info = db_map.get(&local.relative_path);
        let remote_info = remote_map.get(&local.relative_path);
        
        match (db_info, remote_info) {
            // File exists locally and remotely
            (Some((db_hash, db_mtime)), Some(remote)) => {
                let local_changed = &local.hash != db_hash || local.modified_at != *db_mtime;
                let remote_changed = &remote.hash != db_hash || remote.modified_at != *db_mtime;
                
                if local_changed && remote_changed {
                    // Conflict!
                    let resolution = if local.modified_at > remote.modified_at {
                        ConflictResolution::UploadLocal
                    } else {
                        ConflictResolution::DownloadRemote
                    };
                    
                    plan.conflicts.push(Conflict {
                        root_id,
                        relative_path: local.relative_path.clone(),
                        local_modified_at: local.modified_at,
                        remote_modified_at: remote.modified_at,
                        resolution: resolution.clone(),
                    });
                    
                    // Add the winning operation
                    match resolution {
                        ConflictResolution::UploadLocal => {
                            plan.uploads.push(SyncOp::Upload {
                                root_id,
                                relative_path: local.relative_path.clone(),
                                local_path: local.absolute_path.clone(),
                                size: local.size,
                            });
                        }
                        ConflictResolution::DownloadRemote => {
                            plan.downloads.push(SyncOp::Download {
                                root_id,
                                relative_path: local.relative_path.clone(),
                                remote_file_id: remote.id,
                                remote_hash: remote.hash.clone(),
                                size: remote.size,
                            });
                        }
                    }
                } else if local_changed {
                    // Only local changed
                    plan.uploads.push(SyncOp::Upload {
                        root_id,
                        relative_path: local.relative_path.clone(),
                        local_path: local.absolute_path.clone(),
                        size: local.size,
                    });
                } else if remote_changed {
                    // Only remote changed
                    plan.downloads.push(SyncOp::Download {
                        root_id,
                        relative_path: local.relative_path.clone(),
                        remote_file_id: remote.id,
                        remote_hash: remote.hash.clone(),
                        size: remote.size,
                    });
                }
                // Else: unchanged, do nothing
            }
            
            // File exists locally but not in DB (new file)
            (None, None) => {
                plan.uploads.push(SyncOp::Upload {
                    root_id,
                    relative_path: local.relative_path.clone(),
                    local_path: local.absolute_path.clone(),
                    size: local.size,
                });
            }
            
            // File exists locally and remotely but not in DB (shouldn't happen)
            (None, Some(remote)) => {
                // Treat as conflict - use timestamp
                let resolution = if local.modified_at > remote.modified_at {
                    ConflictResolution::UploadLocal
                } else {
                    ConflictResolution::DownloadRemote
                };
                
                match resolution {
                    ConflictResolution::UploadLocal => {
                        plan.uploads.push(SyncOp::Upload {
                            root_id,
                            relative_path: local.relative_path.clone(),
                            local_path: local.absolute_path.clone(),
                            size: local.size,
                        });
                    }
                    ConflictResolution::DownloadRemote => {
                        plan.downloads.push(SyncOp::Download {
                            root_id,
                            relative_path: local.relative_path.clone(),
                            remote_file_id: remote.id,
                            remote_hash: remote.hash.clone(),
                            size: remote.size,
                        });
                    }
                }
            }
            
            // File exists locally and in DB but not remotely (deleted on server)
            (Some(_), None) => {
                plan.deletes.push(SyncOp::DeleteLocal {
                    root_id,
                    relative_path: local.relative_path.clone(),
                });
            }
        }
    }
    
    // Check for files that exist remotely but not locally
    for remote in remote_files {
        if !db_map.contains_key(&remote.relative_path) {
            // New file on server
            plan.downloads.push(SyncOp::Download {
                root_id,
                relative_path: remote.relative_path.clone(),
                remote_file_id: remote.id,
                remote_hash: remote.hash.clone(),
                size: remote.size,
            });
        }
    }
    
    // Check for files in DB but missing locally (deleted locally)
    for (path, _, _) in db_map {
        let exists_locally = local_files.iter().any(|l| l.relative_path == path);
        let exists_remotely = remote_map.contains_key(&path);
        
        if !exists_locally && exists_remotely {
            // Deleted locally - delete on server
            if let Some(remote) = remote_map.get(&path) {
                plan.deletes.push(SyncOp::DeleteRemote {
                    root_id,
                    relative_path: path.clone(),
                    remote_file_id: remote.id,
                });
            }
        }
    }
    
    debug!(
        "Plan generated: {} uploads, {} downloads, {} deletes, {} conflicts",
        plan.uploads.len(),
        plan.downloads.len(),
        plan.deletes.len(),
        plan.conflicts.len()
    );
    
    Ok(plan)
}

/// Remote file information (from API)
#[derive(Debug, Clone)]
pub struct RemoteFileInfo {
    pub id: Uuid,
    pub relative_path: PathBuf,
    pub hash: String,
    pub size: u64,
    pub modified_at: u64,
}
```

**Step 2: Add module to lib.rs**

```rust
pub mod planner;
```

**Step 3: Run cargo check**

```bash
cargo check -p sync-engine
```

**Step 4: Commit**

```bash
git add crates/sync-engine/src/planner.rs crates/sync-engine/src/lib.rs
git commit -m "feat(sync-engine): add sync planner with conflict resolution

- generate_plan() compares local, remote, and DB state
- Detects conflicts when both sides changed
- Resolves conflicts using timestamp (newer wins)
- Generates Upload, Download, Delete operations"
```

---

## Task 4: Create Retry Manager

**Files:**
- Create: `crates/sync-engine/src/retry.rs`
- Modify: `crates/sync-engine/src/lib.rs`

**Step 1: Create retry.rs**

```rust
//! Retry manager with exponential backoff

use anyhow::Result;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::{debug, warn};

/// Calculate delay for retry using exponential backoff
/// Formula: min(base * 2^retry_count, max_delay)
pub fn calculate_backoff_delay(
    retry_count: u32,
    base_seconds: u64,
    max_seconds: u64,
) -> Duration {
    let delay = base_seconds.saturating_mul(2_u64.saturating_pow(retry_count));
    let capped = delay.min(max_seconds);
    Duration::from_secs(capped)
}

/// Get current Unix timestamp
pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Determine if an error is retryable
pub fn is_retryable_error(error: &anyhow::Error) -> bool {
    let error_string = error.to_string().to_lowercase();
    
    // Network errors - retryable
    if error_string.contains("timeout")
        || error_string.contains("connection")
        || error_string.contains("network")
        || error_string.contains("dns")
        || error_string.contains("unreachable")
    {
        return true;
    }
    
    // Server errors (5xx) - retryable
    if error_string.contains("500")
        || error_string.contains("502")
        || error_string.contains("503")
        || error_string.contains("504")
        || error_string.contains("server error")
    {
        return true;
    }
    
    // Client errors (4xx) - not retryable
    if error_string.contains("400")
        || error_string.contains("401")
        || error_string.contains("403")
        || error_string.contains("404")
        || error_string.contains("client error")
    {
        return false;
    }
    
    // Disk full - not retryable (requires user action)
    if error_string.contains("no space")
        || error_string.contains("disk full")
    {
        return false;
    }
    
    // Permission errors - not retryable
    if error_string.contains("permission denied")
        || error_string.contains("access denied")
    {
        return false;
    }
    
    // Default: retryable (conservative)
    true
}

/// Retry configuration
#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub max_retries: u32,
    pub base_delay_seconds: u64,
    pub max_delay_seconds: u64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 10,
            base_delay_seconds: 1,
            max_delay_seconds: 300, // 5 minutes
        }
    }
}

/// Execute an async operation with retry logic
pub async fn with_retry<T, F, Fut>(
    config: &RetryConfig,
    operation_name: &str,
    operation: F,
) -> Result<T>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    let mut last_error = None;
    
    for attempt in 0..=config.max_retries {
        match operation().await {
            Ok(result) => {
                if attempt > 0 {
                    debug!("{} succeeded after {} retries", operation_name, attempt);
                }
                return Ok(result);
            }
            Err(e) => {
                last_error = Some(e);
                
                if attempt == config.max_retries {
                    break;
                }
                
                let error_ref = last_error.as_ref().unwrap();
                
                if !is_retryable_error(error_ref) {
                    warn!(
                        "{} failed with non-retryable error: {}",
                        operation_name, error_ref
                    );
                    break;
                }
                
                let delay = calculate_backoff_delay(
                    attempt,
                    config.base_delay_seconds,
                    config.max_delay_seconds,
                );
                
                debug!(
                    "{} failed (attempt {}), retrying in {:?}: {}",
                    operation_name, attempt + 1, delay, error_ref
                );
                
                tokio::time::sleep(delay).await;
            }
        }
    }
    
    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("Operation failed")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backoff_calculation() {
        assert_eq!(
            calculate_backoff_delay(0, 1, 300),
            Duration::from_secs(1)
        );
        assert_eq!(
            calculate_backoff_delay(1, 1, 300),
            Duration::from_secs(2)
        );
        assert_eq!(
            calculate_backoff_delay(2, 1, 300),
            Duration::from_secs(4)
        );
        assert_eq!(
            calculate_backoff_delay(10, 1, 300),
            Duration::from_secs(300) // Capped at max
        );
    }

    #[test]
    fn test_retryable_errors() {
        assert!(is_retryable_error(&anyhow::anyhow!("Connection timeout")));
        assert!(is_retryable_error(&anyhow::anyhow!("500 Internal Server Error")));
        assert!(!is_retryable_error(&anyhow::anyhow!("400 Bad Request")));
        assert!(!is_retryable_error(&anyhow::anyhow!("No space left on device")));
    }
}
```

**Step 2: Add module to lib.rs**

```rust
pub mod retry;
```

**Step 3: Run cargo check and tests**

```bash
cargo check -p sync-engine
cargo test -p sync-engine --lib
```

**Step 4: Commit**

```bash
git add crates/sync-engine/src/retry.rs crates/sync-engine/src/lib.rs
git commit -m "feat(sync-engine): add retry manager with exponential backoff

- calculate_backoff_delay() with exponential formula
- is_retryable_error() categorizes errors
- with_retry() executes operations with retry logic
- Configurable max retries, base/max delay"
```

---

## Task 5: Update Worker with Real Implementation

**Files:**
- Modify: `crates/sync-engine/src/worker.rs`

**Step 1: Rewrite worker.rs with real upload/download**

This is a large task - see the detailed implementation in the design doc for the full code.

Key points:
- Implement resumable chunk upload using backend API
- Implement download with temp file and atomic rename
- Integrate with retry manager
- Update SQLite state after successful operations

**Step 2: Run cargo check**

```bash
cargo check -p sync-engine
```

**Step 3: Commit**

```bash
git add crates/sync-engine/src/worker.rs
git commit -m "feat(sync-engine): implement real upload/download with resumable chunks

- upload(): Create session, upload 5MB chunks, complete
- download(): Stream to temp file, verify hash, atomic rename
- Integrate retry manager for resilience
- Update SQLite state after success"
```

---

## Task 6: Create WebSocket Client

**Files:**
- Create: `crates/sync-engine/src/websocket.rs`
- Modify: `crates/sync-engine/src/lib.rs`

**Step 1: Create websocket.rs with tokio-tungstenite**

Connects to backend WebSocket for push notifications.

**Step 2: Commit**

```bash
git add crates/sync-engine/src/websocket.rs crates/sync-engine/src/lib.rs
git commit -m "feat(sync-engine): add WebSocket client for remote change notifications

- Connect to wss://app.rustshare.io/api/v1/sync/websocket
- Handle file_changed, folder_changed, sync_complete messages
- Auto-reconnect with exponential backoff
- Fallback to polling on disconnect"
```

---

## Task 7: Integrate Everything in SyncManager

**Files:**
- Modify: `crates/sync-engine/src/manager.rs`

**Step 1: Rewrite manager.rs as orchestrator**

- Run scanner periodically and on events
- Generate plan with planner
- Execute with worker
- Handle WebSocket notifications
- Manage sync loop state machine

**Step 2: Run full test suite**

```bash
cargo test -p sync-engine
cargo test -p rustshare-desktop --lib
```

**Step 3: Commit**

```bash
git add crates/sync-engine/src/manager.rs
git commit -m "feat(sync-engine): integrate all components in SyncManager

- Orchestrate scanner, planner, worker
- Handle local FS events (real-time)
- Handle WebSocket remote events
- Periodic full sync as fallback
- State machine: Idle → Scanning → Planning → Executing"
```

---

## Task 8: End-to-End Testing

**Step 1: Build release binary**

```bash
cargo build -p rustshare-desktop --release
```

**Step 2: Test scenarios**

1. Add sync root, create local file → verify upload
2. Add file via web UI → verify download
3. Modify both sides → verify conflict resolution
4. Delete local file → verify remote delete
5. Go offline, make changes, reconnect → verify queue and retry

**Step 3: Fix any issues found**

Iterate on fixes as needed.

---

## Summary

This implementation plan covers:
- 8 major tasks with detailed steps
- Each task includes file paths, code, commands, and commit messages
- TDD approach with tests for retry manager
- Incremental delivery from data layer to full integration

**Estimated effort:** 4-6 hours of focused implementation
