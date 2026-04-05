//! Filesystem watcher for real-time change detection
//!
//! Uses the `notify` crate to watch synced folders for changes
//! and emits events that the sync engine can process.

use anyhow::Result;
use notify::{Config, Event as NotifyEvent, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Filesystem event types
#[derive(Debug, Clone, PartialEq)]
pub enum WatchEvent {
    /// File or folder created
    Created { path: PathBuf, folder_id: Uuid },
    /// File or folder modified
    Modified { path: PathBuf, folder_id: Uuid },
    /// File or folder deleted
    Deleted { path: PathBuf, folder_id: Uuid },
    /// File or folder renamed/moved
    Renamed {
        from_path: PathBuf,
        to_path: PathBuf,
        folder_id: Uuid,
    },
}

/// Filesystem watcher for synced folders
pub struct FsWatcher {
    watcher: RecommendedWatcher,
    watch_roots: Arc<RwLock<HashMap<PathBuf, Uuid>>>,
    #[allow(dead_code)]
    event_tx: Sender<WatchEvent>,
    event_rx: Receiver<WatchEvent>,
}

impl FsWatcher {
    /// Create a new filesystem watcher
    pub fn new() -> Result<Self> {
        let (event_tx, event_rx) = channel(1000);
        let watch_roots = Arc::new(RwLock::new(HashMap::new()));
        
        let event_tx_clone = event_tx.clone();
        let watch_roots_clone = watch_roots.clone();

        let watcher = RecommendedWatcher::new(
            move |res: std::result::Result<NotifyEvent, notify::Error>| {
                match res {
                    Ok(event) => {
                        if let Err(e) = Self::handle_notify_event(
                            event,
                            &event_tx_clone,
                            &watch_roots_clone,
                        ) {
                            error!("Error handling notify event: {}", e);
                        }
                    }
                    Err(e) => {
                        error!("Notify error: {}", e);
                    }
                }
            },
            Config::default(),
        )?;

        Ok(FsWatcher {
            watcher,
            watch_roots,
            event_tx,
            event_rx,
        })
    }

    /// Watch a folder for changes
    pub fn watch_folder(&mut self, path: &Path, folder_id: Uuid) -> Result<()> {
        let canonical_path = path.canonicalize()?;
        
        self.watcher.watch(&canonical_path, RecursiveMode::Recursive)?;
        
        let mut roots = self.watch_roots.blocking_write();
        roots.insert(canonical_path.clone(), folder_id);
        
        info!("Watching folder {} at {}", folder_id, canonical_path.display());
        Ok(())
    }

    /// Stop watching a folder
    pub fn unwatch_folder(&mut self, path: &Path) -> Result<()> {
        let canonical_path = path.canonicalize()?;
        
        self.watcher.unwatch(&canonical_path)?;
        
        let mut roots = self.watch_roots.blocking_write();
        roots.remove(&canonical_path);
        
        info!("Stopped watching {}", canonical_path.display());
        Ok(())
    }

    /// Get the event receiver
    pub fn event_receiver(&mut self) -> &mut Receiver<WatchEvent> {
        &mut self.event_rx
    }

    /// Handle a notify event and convert to WatchEvent
    fn handle_notify_event(
        event: NotifyEvent,
        event_tx: &Sender<WatchEvent>,
        watch_roots: &Arc<RwLock<HashMap<PathBuf, Uuid>>>,
    ) -> Result<()> {
        let roots = watch_roots.blocking_read();

        // Find which watched folder this event belongs to
        let folder_id = event
            .paths
            .first()
            .and_then(|p| Self::find_folder_for_path(p, &roots));

        let Some(folder_id) = folder_id else {
            return Ok(());
        };

        let events = Self::convert_event(event, folder_id)?;

        for watch_event in events {
            if let Err(e) = event_tx.try_send(watch_event) {
                warn!("Failed to send watch event: {}", e);
            }
        }

        Ok(())
    }

    /// Find which folder a path belongs to
    fn find_folder_for_path(path: &Path, roots: &HashMap<PathBuf, Uuid>) -> Option<Uuid> {
        for (root, folder_id) in roots {
            if path.starts_with(root) {
                return Some(*folder_id);
            }
        }
        None
    }

    /// Convert notify event to watch events
    fn convert_event(event: NotifyEvent, folder_id: Uuid) -> Result<Vec<WatchEvent>> {
        let mut events = Vec::new();

        match event.kind {
            EventKind::Create(_) => {
                for path in event.paths {
                    events.push(WatchEvent::Created { path, folder_id });
                }
            }
            EventKind::Modify(modify_kind) => {
                match modify_kind {
                    notify::event::ModifyKind::Name(rename_mode) => {
                        match rename_mode {
                            notify::event::RenameMode::From => {
                                // Rename source - ignore, we'll handle on the 'to' event
                            }
                            notify::event::RenameMode::To => {
                                // Try to match with a previous 'from' event
                                // For now, treat as create
                                for path in event.paths {
                                    events.push(WatchEvent::Created { path, folder_id });
                                }
                            }
                            notify::event::RenameMode::Both => {
                                if event.paths.len() >= 2 {
                                    events.push(WatchEvent::Renamed {
                                        from_path: event.paths[0].clone(),
                                        to_path: event.paths[1].clone(),
                                        folder_id,
                                    });
                                }
                            }
                            _ => {
                                for path in event.paths {
                                    events.push(WatchEvent::Modified { path, folder_id });
                                }
                            }
                        }
                    }
                    _ => {
                        for path in event.paths {
                            events.push(WatchEvent::Modified { path, folder_id });
                        }
                    }
                }
            }
            EventKind::Remove(_) => {
                for path in event.paths {
                    events.push(WatchEvent::Deleted { path, folder_id });
                }
            }
            _ => {
                // Ignore other event types
            }
        }

        Ok(events)
    }
}

impl Drop for FsWatcher {
    fn drop(&mut self) {
        debug!("Filesystem watcher dropped");
    }
}

/// Debounced filesystem watcher
/// 
/// Groups rapid successive events to reduce noise
pub struct DebouncedFsWatcher {
    watcher: FsWatcher,
    #[allow(dead_code)]
    debounce_tx: Sender<DebouncedEvent>,
    #[allow(dead_code)]
    debounce_rx: Receiver<DebouncedEvent>,
}

/// Debounced event
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct DebouncedEvent {
    path: PathBuf,
    folder_id: Uuid,
    kind: DebouncedEventKind,
}

#[derive(Debug, Clone, PartialEq)]
enum DebouncedEventKind {
    Create,
    Modify,
    Delete,
}

impl DebouncedFsWatcher {
    /// Create a new debounced watcher
    pub fn new() -> Result<Self> {
        let watcher = FsWatcher::new()?;
        let (debounce_tx, debounce_rx) = channel(1000);

        Ok(DebouncedFsWatcher {
            watcher,
            debounce_tx,
            debounce_rx,
        })
    }

    /// Watch a folder
    pub fn watch_folder(&mut self, path: &Path, folder_id: Uuid) -> Result<()> {
        self.watcher.watch_folder(path, folder_id)
    }

    /// Stop watching a folder
    pub fn unwatch_folder(&mut self, path: &Path) -> Result<()> {
        self.watcher.unwatch_folder(path)
    }

    /// Start the debounce processor
    pub async fn run_debounce(&mut self, debounce_duration: std::time::Duration) {
        use tokio::time::interval;

        let mut event_buffer: HashMap<(PathBuf, Uuid), DebouncedEventKind> = HashMap::new();
        let mut ticker = interval(debounce_duration);

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    // Flush buffered events
                    for ((path, folder_id), kind) in event_buffer.drain() {
                        let event = match kind {
                            DebouncedEventKind::Create => WatchEvent::Created { path, folder_id },
                            DebouncedEventKind::Modify => WatchEvent::Modified { path, folder_id },
                            DebouncedEventKind::Delete => WatchEvent::Deleted { path, folder_id },
                        };
                        debug!("Debounced event: {:?}", event);
                    }
                }
                Some(event) = self.watcher.event_receiver().recv() => {
                    // Buffer the event
                    let (path, folder_id, kind) = match event {
                        WatchEvent::Created { path, folder_id } => {
                            (path, folder_id, DebouncedEventKind::Create)
                        }
                        WatchEvent::Modified { path, folder_id } => {
                            (path, folder_id, DebouncedEventKind::Modify)
                        }
                        WatchEvent::Deleted { path, folder_id } => {
                            (path, folder_id, DebouncedEventKind::Delete)
                        }
                        WatchEvent::Renamed { from_path, to_path, folder_id: _ } => {
                            // Handle rename immediately
                            debug!("Renamed: {:?} -> {:?}", from_path, to_path);
                            continue;
                        }
                    };

                    // Update buffer - delete always wins
                    let key = (path, folder_id);
                    match (event_buffer.get(&key), &kind) {
                        (Some(DebouncedEventKind::Delete), DebouncedEventKind::Create) => {
                            // Delete followed by create = modify
                            event_buffer.insert(key, DebouncedEventKind::Modify);
                        }
                        (Some(_), DebouncedEventKind::Delete) => {
                            // Any followed by delete = delete
                            event_buffer.insert(key, DebouncedEventKind::Delete);
                        }
                        _ => {
                            event_buffer.insert(key, kind);
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_find_folder_for_path() {
        let mut roots = HashMap::new();
        let folder1 = Uuid::new_v4();
        let folder2 = Uuid::new_v4();

        roots.insert(PathBuf::from("/home/user/folder1"), folder1);
        roots.insert(PathBuf::from("/home/user/folder2"), folder2);

        assert_eq!(
            FsWatcher::find_folder_for_path(Path::new("/home/user/folder1/file.txt"), &roots),
            Some(folder1)
        );
        assert_eq!(
            FsWatcher::find_folder_for_path(Path::new("/home/user/folder2/sub/file.txt"), &roots),
            Some(folder2)
        );
        assert_eq!(
            FsWatcher::find_folder_for_path(Path::new("/home/user/other/file.txt"), &roots),
            None
        );
    }
}
