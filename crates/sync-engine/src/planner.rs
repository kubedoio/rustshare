use std::collections::HashMap;
use std::path::{Path, PathBuf};
use sync_domain::{LocalEntry, RemoteEntry, SyncEvent};
use uuid::Uuid;

pub struct Planner;

impl Planner {
    pub fn reconcile(
        local_inventory: HashMap<PathBuf, LocalEntry>,
        remote_entries: Vec<RemoteEntry>,
    ) -> Vec<SyncEvent> {
        let mut events = Vec::new();
        let mut remote_map: HashMap<PathBuf, RemoteEntry> = HashMap::new();

        // In a real implementation, we'd map remote IDs to local paths using the database.
        // For Phase 1, we assume remote paths are mirrored locally.
        
        for remote in remote_entries {
            // Simplified: Mapping remote name to PathBuf
            let path = PathBuf::from(&remote.name); 
            
            if let Some(local) = local_inventory.get(&path) {
                if local.hash != remote.hash {
                    // Conflict detection (Phase 1 policy: remote change + local change = conflict)
                    events.push(SyncEvent::RemoteUpdated(remote.clone()));
                }
            } else {
                events.push(SyncEvent::RemoteCreated(remote.clone()));
            }
            remote_map.insert(path, remote);
        }

        for (path, local) in local_inventory {
            if !remote_map.contains_key(&path) {
                events.push(SyncEvent::LocalCreated(local));
            }
        }

        events
    }
}
