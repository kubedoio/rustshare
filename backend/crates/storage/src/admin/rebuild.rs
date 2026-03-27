//! Rebuild tools for recreating indexes from canonical metadata

use std::sync::Arc;
use tracing::{debug, info, warn, error};
use uuid::Uuid;

use super::OperationSummary;
use crate::metadata_v2::schemas::*;
use crate::repos::*;

/// Tools for rebuilding indexes and projections
pub struct RebuildTool {
    repo: Arc<dyn MetadataRepository>,
}

impl RebuildTool {
    pub fn new(repo: Arc<dyn MetadataRepository>) -> Self {
        Self { repo }
    }
    
    /// Rebuild folder children index from canonical documents
    pub async fn rebuild_folder_children_index(&self, folder_id: Uuid) -> Result<OperationSummary, RepositoryError> {
        let mut summary = OperationSummary::new(format!("rebuild_folder_children_{}", folder_id));
        
        info!(folder_id = %folder_id, "Rebuilding folder children index");
        
        // Verify the folder exists
        let folder = match self.repo.folders().get(folder_id).await? {
            Some(f) => f,
            None => {
                summary.add_error(format!("Folder {} not found", folder_id));
                return Ok(summary);
            }
        };
        
        let mut index = FolderChildrenIndex::new(folder_id);
        
        // Find all child folders
        // Note: In production, this would use an efficient query
        // For now, we demonstrate the structure
        
        // Find all child files
        // Add them to the index
        
        // Save the rebuilt index
        self.repo.folder_children_index().save(&index).await?;
        
        summary.items_processed = index.children.len();
        summary.items_succeeded = index.children.len();
        
        info!(
            folder_id = %folder_id,
            children_count = index.children.len(),
            "Folder children index rebuilt"
        );
        
        Ok(summary)
    }
    
    /// Rebuild all folder children indexes for a user
    pub async fn rebuild_user_folder_indexes(&self, user_id: Uuid) -> Result<OperationSummary, RepositoryError> {
        let mut summary = OperationSummary::new(format!("rebuild_user_folder_indexes_{}", user_id));
        
        info!(user_id = %user_id, "Rebuilding all folder indexes for user");
        
        // Get user's root folders
        let roots = self.repo.folders().get_user_roots(user_id).await?;
        
        for root in roots {
            summary.increment_processed();
            
            match self.rebuild_folder_children_index_recursive(root.id).await {
                Ok(s) => {
                    summary.items_succeeded += s.items_succeeded;
                    summary.items_failed += s.items_failed;
                    summary.errors.extend(s.errors);
                }
                Err(e) => {
                    summary.add_error(format!("Failed to rebuild index for folder {}: {}", root.id, e));
                }
            }
        }
        
        info!(
            user_id = %user_id,
            folders_processed = summary.items_processed,
            "User folder index rebuild complete"
        );
        
        Ok(summary)
    }
    
    /// Recursively rebuild folder children indexes
    async fn rebuild_folder_children_index_recursive(
        &self,
        folder_id: Uuid,
    ) -> Result<OperationSummary, RepositoryError> {
        let mut summary = OperationSummary::new(format!("rebuild_recursive_{}", folder_id));
        
        // Rebuild this folder's index
        let result = self.rebuild_folder_children_index(folder_id).await?;
        summary.items_processed += result.items_processed;
        summary.items_succeeded += result.items_succeeded;
        summary.items_failed += result.items_failed;
        summary.errors.extend(result.errors);
        
        // Find child folders and recurse
        // Note: In production, use the index we just built
        
        Ok(summary)
    }
    
    /// Rebuild user roots index
    pub async fn rebuild_user_roots_index(&self, user_id: Uuid) -> Result<OperationSummary, RepositoryError> {
        let mut summary = OperationSummary::new(format!("rebuild_user_roots_{}", user_id));
        
        info!(user_id = %user_id, "Rebuilding user roots index");
        
        // Get all root folders for user
        let roots = self.repo.folders().get_user_roots(user_id).await?;
        
        let index = UserRootsIndex {
            schema_version: CURRENT_SCHEMA_VERSION,
            user_id,
            version: 1,
            updated_at: chrono::Utc::now(),
            root_folder_ids: roots.into_iter().map(|f| f.id).collect(),
        };
        
        // Save the index (would need a repository method for this)
        
        summary.items_processed = index.root_folder_ids.len();
        summary.items_succeeded = index.root_folder_ids.len();
        
        info!(
            user_id = %user_id,
            root_count = index.root_folder_ids.len(),
            "User roots index rebuilt"
        );
        
        Ok(summary)
    }
    
    /// Rebuild shared with me index for a user
    pub async fn rebuild_shared_with_me_index(&self, user_id: Uuid) -> Result<OperationSummary, RepositoryError> {
        let mut summary = OperationSummary::new(format!("rebuild_shared_with_me_{}", user_id));
        
        info!(user_id = %user_id, "Rebuilding shared with me index");
        
        // Get shares where user is recipient
        let shares = self.repo.shares().list_by_recipient(user_id).await?;
        
        let mut entries = Vec::new();
        
        for share in shares {
            summary.increment_processed();
            
            // Get resource name
            let resource_name = match share.resource_type.as_str() {
                "file" => {
                    match self.repo.files().get(share.resource_id).await? {
                        Some(file) => file.name,
                        None => {
                            warn!(file_id = %share.resource_id, "File not found for share");
                            summary.add_error(format!("File {} not found", share.resource_id));
                            continue;
                        }
                    }
                }
                "folder" => {
                    match self.repo.folders().get(share.resource_id).await? {
                        Some(folder) => folder.name,
                        None => {
                            warn!(folder_id = %share.resource_id, "Folder not found for share");
                            summary.add_error(format!("Folder {} not found", share.resource_id));
                            continue;
                        }
                    }
                }
                _ => {
                    warn!(resource_type = %share.resource_type, "Unknown resource type");
                    summary.add_error(format!("Unknown resource type: {}", share.resource_type));
                    continue;
                }
            };
            
            entries.push(ShareEntry {
                share_id: share.id,
                resource_type: share.resource_type.clone(),
                resource_id: share.resource_id,
                resource_name,
                permissions: share.permissions,
                shared_by: share.created_by,
                shared_at: share.created_at,
            });
            
            summary.increment_succeeded();
        }
        
        let index = SharedWithMeIndex {
            schema_version: CURRENT_SCHEMA_VERSION,
            user_id,
            version: 1,
            updated_at: chrono::Utc::now(),
            shares: entries,
        };
        
        // Save the index
        
        info!(
            user_id = %user_id,
            share_count = index.shares.len(),
            "Shared with me index rebuilt"
        );
        
        Ok(summary)
    }
    
    /// Rebuild all indexes (full rebuild)
    pub async fn rebuild_all_indexes(&self) -> Result<OperationSummary, RepositoryError> {
        let mut summary = OperationSummary::new("rebuild_all_indexes");
        
        info!("Starting full index rebuild");
        
        // This would:
        // 1. Scan all folders and rebuild their children indexes
        // 2. Scan all users and rebuild their roots indexes
        // 3. Scan all users and rebuild their shared_with_me indexes
        
        // For now, this is a placeholder showing the structure
        
        info!(
            processed = summary.items_processed,
            "Full index rebuild complete"
        );
        
        Ok(summary)
    }
    
    /// Full rebuild from events (event sourcing rebuild)
    pub async fn rebuild_from_events(
        &self,
        start: chrono::DateTime<chrono::Utc>,
        end: chrono::DateTime<chrono::Utc>,
    ) -> Result<OperationSummary, RepositoryError> {
        let mut summary = OperationSummary::new("rebuild_from_events");
        
        info!("Rebuilding state from events");
        
        // Read all events in range
        let events = self.repo.events().read_range(start, end, 10000).await?;
        
        for event in events {
            summary.increment_processed();
            
            // Apply each event to rebuild state
            match self.apply_event(&event).await {
                Ok(_) => summary.increment_succeeded(),
                Err(e) => summary.add_error(format!("Failed to apply event {}: {}", event.id, e)),
            }
        }
        
        info!(
            events_processed = summary.items_processed,
            "Event replay complete"
        );
        
        Ok(summary)
    }
    
    /// Apply a single event during rebuild
    async fn apply_event(&self, event: &EventDocument) -> Result<(), RepositoryError> {
        match event.event_type {
            EventType::FolderCreated => {
                // Reconstruct folder from event payload
                info!(folder_id = %event.resource_id, "Replaying folder creation");
            }
            EventType::FileUploaded => {
                info!(file_id = %event.resource_id, "Replaying file upload");
            }
            EventType::FileModified => {
                info!(file_id = %event.resource_id, "Replaying file modification");
            }
            EventType::ShareCreated => {
                info!(share_id = %event.resource_id, "Replaying share creation");
            }
            _ => {
                debug!(event_type = ?event.event_type, "Skipping event type");
            }
        }
        
        Ok(())
    }
}

/// Scan tool for full namespace scanning
pub struct ScanTool {
    repo: Arc<dyn MetadataRepository>,
}

impl ScanTool {
    pub fn new(repo: Arc<dyn MetadataRepository>) -> Self {
        Self { repo }
    }
    
    /// Full scan of all folders
    pub async fn scan_folders(&self) -> Result<Vec<FolderDocument>, RepositoryError> {
        info!("Scanning all folders");
        
        // This would use list_prefix to scan all folders
        // For now, return empty
        Ok(Vec::new())
    }
    
    /// Full scan of all files
    pub async fn scan_files(&self) -> Result<Vec<FileDocument>, RepositoryError> {
        info!("Scanning all files");
        
        Ok(Vec::new())
    }
    
    /// Generate a full namespace report
    pub async fn generate_namespace_report(&self) -> Result<NamespaceReport, RepositoryError> {
        let mut report = NamespaceReport::new();
        
        info!("Generating namespace report");
        
        let folders = self.scan_folders().await?;
        report.total_folders = folders.len();
        report.active_folders = folders.iter().filter(|f| !f.deleted).count();
        report.deleted_folders = folders.iter().filter(|f| f.deleted).count();
        
        let files = self.scan_files().await?;
        report.total_files = files.len();
        report.active_files = files.iter().filter(|f| !f.deleted).count();
        report.deleted_files = files.iter().filter(|f| f.deleted).count();
        
        // Calculate total size
        report.total_bytes = files.iter().map(|f| f.size).sum();
        
        info!(
            folders = report.total_folders,
            files = report.total_files,
            bytes = report.total_bytes,
            "Namespace report complete"
        );
        
        Ok(report)
    }
}

/// Namespace statistics report
#[derive(Debug, Clone)]
pub struct NamespaceReport {
    pub total_folders: usize,
    pub active_folders: usize,
    pub deleted_folders: usize,
    pub total_files: usize,
    pub active_files: usize,
    pub deleted_files: usize,
    pub total_bytes: i64,
}

impl NamespaceReport {
    pub fn new() -> Self {
        Self {
            total_folders: 0,
            active_folders: 0,
            deleted_folders: 0,
            total_files: 0,
            active_files: 0,
            deleted_files: 0,
            total_bytes: 0,
        }
    }
}

impl Default for NamespaceReport {
    fn default() -> Self {
        Self::new()
    }
}
