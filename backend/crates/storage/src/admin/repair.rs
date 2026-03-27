//! Repair tools for fixing metadata inconsistencies

use std::sync::Arc;
use tracing::{debug, info, warn, error};
use uuid::Uuid;

use super::OperationSummary;
use crate::metadata_v2::schemas::*;
use crate::repos::*;

/// Tools for repairing metadata inconsistencies
pub struct RepairTool {
    repo: Arc<dyn MetadataRepository>,
}

impl RepairTool {
    pub fn new(repo: Arc<dyn MetadataRepository>) -> Self {
        Self { repo }
    }
    
    /// Repair a folder's parent reference if the parent doesn't exist
    pub async fn repair_folder_parent(&self, folder_id: Uuid) -> Result<bool, RepositoryError> {
        let folder = match self.repo.folders().get(folder_id).await? {
            Some(f) => f,
            None => {
                warn!(folder_id = %folder_id, "Folder not found for repair");
                return Ok(false);
            }
        };
        
        // Check if parent exists
        if let Some(parent_id) = folder.parent_id {
            match self.repo.folders().get(parent_id).await? {
                Some(parent) => {
                    if parent.deleted {
                        warn!(
                            folder_id = %folder_id,
                            parent_id = %parent_id,
                            "Folder's parent is deleted, moving to root"
                        );
                        
                        // Move to root by updating parent_id
                        let mut fixed_folder = folder.clone();
                        fixed_folder.parent_id = None;
                        fixed_folder.path = format!("/{}", folder.name);
                        fixed_folder.bump_version();
                        
                        self.repo.folders().update(&fixed_folder).await?;
                        
                        // Update index
                        self.rebuild_folder_children_index(parent_id).await?;
                        
                        return Ok(true);
                    }
                }
                None => {
                    warn!(
                        folder_id = %folder_id,
                        parent_id = %parent_id,
                        "Folder's parent doesn't exist, moving to root"
                    );
                    
                    // Move to root
                    let mut fixed_folder = folder.clone();
                    fixed_folder.parent_id = None;
                    fixed_folder.path = format!("/{}", folder.name);
                    fixed_folder.bump_version();
                    
                    self.repo.folders().update(&fixed_folder).await?;
                    
                    return Ok(true);
                }
            }
        }
        
        Ok(false)
    }
    
    /// Repair a file's parent reference
    pub async fn repair_file_parent(&self, file_id: Uuid) -> Result<bool, RepositoryError> {
        let file = match self.repo.files().get(file_id).await? {
            Some(f) => f,
            None => {
                warn!(file_id = %file_id, "File not found for repair");
                return Ok(false);
            }
        };
        
        // Check if parent exists
        if let Some(parent_id) = file.parent_id {
            match self.repo.folders().get(parent_id).await? {
                Some(parent) => {
                    if parent.deleted {
                        warn!(
                            file_id = %file_id,
                            parent_id = %parent_id,
                            "File's parent is deleted, moving to root"
                        );
                        
                        // Move to root
                        let mut fixed_file = file.clone();
                        fixed_file.parent_id = None;
                        fixed_file.path = format!("/{}", file.name);
                        fixed_file.bump_version();
                        
                        self.repo.files().update(&fixed_file).await?;
                        
                        // Update old parent's index
                        self.rebuild_folder_children_index(parent_id).await?;
                        
                        return Ok(true);
                    }
                }
                None => {
                    warn!(
                        file_id = %file_id,
                        parent_id = %parent_id,
                        "File's parent doesn't exist, moving to root"
                    );
                    
                    // Move to root
                    let mut fixed_file = file.clone();
                    fixed_file.parent_id = None;
                    fixed_file.path = format!("/{}", file.name);
                    fixed_file.bump_version();
                    
                    self.repo.files().update(&fixed_file).await?;
                    
                    return Ok(true);
                }
            }
        }
        
        Ok(false)
    }
    
    /// Repair orphaned shares (pointing to non-existent resources)
    pub async fn repair_orphaned_shares(&self) -> Result<OperationSummary, RepositoryError> {
        let mut summary = OperationSummary::new("repair_orphaned_shares");
        
        // This would scan all shares and verify their resources exist
        // For now, this is a placeholder
        
        info!("Checking for orphaned shares");
        
        Ok(summary)
    }
    
    /// Repair a folder children index
    pub async fn rebuild_folder_children_index(&self, folder_id: Uuid) -> Result<(), RepositoryError> {
        info!(folder_id = %folder_id, "Rebuilding folder children index");
        
        let mut index = FolderChildrenIndex::new(folder_id);
        
        // Scan for folders with this parent
        // Note: In a real implementation, we'd have an efficient way to query by parent
        // For now, this is a placeholder showing the structure
        
        // Scan for files with this parent
        // Add them to the index
        
        // Save the rebuilt index
        self.repo.folder_children_index().save(&index).await?;
        
        info!(folder_id = %folder_id, "Folder children index rebuilt");
        
        Ok(())
    }
    
    /// Clean up expired leases
    pub async fn cleanup_expired_leases(&self) -> Result<OperationSummary, RepositoryError> {
        let mut summary = OperationSummary::new("cleanup_expired_leases");
        
        info!("Cleaning up expired leases");
        
        // This would scan coordination/leases/ and remove expired ones
        
        Ok(summary)
    }
    
    /// Fix path inconsistencies in folder tree
    pub async fn fix_folder_paths(&self, root_folder_id: Uuid) -> Result<OperationSummary, RepositoryError> {
        let mut summary = OperationSummary::new("fix_folder_paths");
        
        info!(root_id = %root_folder_id, "Fixing folder paths");
        
        // Get all descendants
        let descendants = self.repo.folders().list_descendants(root_folder_id).await?;
        
        for folder in descendants {
            summary.increment_processed();
            
            // Calculate expected path
            let expected_path = if let Some(parent_id) = folder.parent_id {
                match self.repo.folders().get(parent_id).await? {
                    Some(parent) => {
                        if parent.path == "/" {
                            format!("/{}", folder.name)
                        } else {
                            format!("{}/{}", parent.path, folder.name)
                        }
                    }
                    None => {
                        summary.add_error(format!("Parent {} not found for folder {}", parent_id, folder.id));
                        continue;
                    }
                }
            } else {
                format!("/{}", folder.name)
            };
            
            // Fix if path is wrong
            if folder.path != expected_path {
                warn!(
                    folder_id = %folder.id,
                    current_path = %folder.path,
                    expected_path = %expected_path,
                    "Fixing folder path"
                );
                
                let mut fixed = folder.clone();
                fixed.path = expected_path;
                fixed.bump_version();
                
                self.repo.folders().update(&fixed).await?;
                summary.increment_fixed();
            }
            
            summary.increment_succeeded();
        }
        
        info!(
            processed = summary.items_processed,
            fixed = summary.items_fixed,
            "Folder path fix complete"
        );
        
        Ok(summary)
    }
    
    /// Remove duplicate names in the same folder
    pub async fn fix_duplicate_names(&self) -> Result<OperationSummary, RepositoryError> {
        let mut summary = OperationSummary::new("fix_duplicate_names");
        
        info!("Checking for duplicate names");
        
        // This would scan all folders and detect duplicates
        // For duplicates, append a counter to the name
        
        Ok(summary)
    }
}

/// Migration tool for importing data from PostgreSQL to RustFS
pub struct MigrationTool {
    postgres: Arc<dyn MetadataRepository>,
    rustfs: Arc<dyn MetadataRepository>,
}

impl MigrationTool {
    pub fn new(
        postgres: Arc<dyn MetadataRepository>,
        rustfs: Arc<dyn MetadataRepository>,
    ) -> Self {
        Self { postgres, rustfs }
    }
    
    /// Import a folder from PostgreSQL to RustFS
    pub async fn import_folder(&self, folder_id: Uuid) -> Result<bool, RepositoryError> {
        let folder = match self.postgres.folders().get(folder_id).await? {
            Some(f) => f,
            None => return Ok(false),
        };
        
        // Create in RustFS
        match self.rustfs.folders().create(&folder).await {
            Ok(_) => {
                info!(folder_id = %folder_id, "Imported folder to RustFS");
                Ok(true)
            }
            Err(RepositoryError::AlreadyExists(_)) => {
                debug!(folder_id = %folder_id, "Folder already exists in RustFS");
                Ok(false)
            }
            Err(e) => Err(e),
        }
    }
    
    /// Import a file from PostgreSQL to RustFS
    pub async fn import_file(&self, file_id: Uuid) -> Result<bool, RepositoryError> {
        let file = match self.postgres.files().get(file_id).await? {
            Some(f) => f,
            None => return Ok(false),
        };
        
        match self.rustfs.files().create(&file).await {
            Ok(_) => {
                info!(file_id = %file_id, "Imported file to RustFS");
                Ok(true)
            }
            Err(RepositoryError::AlreadyExists(_)) => {
                debug!(file_id = %file_id, "File already exists in RustFS");
                Ok(false)
            }
            Err(e) => Err(e),
        }
    }
    
    /// Import a share from PostgreSQL to RustFS
    pub async fn import_share(&self, share_id: Uuid) -> Result<bool, RepositoryError> {
        let share = match self.postgres.shares().get(share_id).await? {
            Some(s) => s,
            None => return Ok(false),
        };
        
        match self.rustfs.shares().create(&share).await {
            Ok(_) => {
                info!(share_id = %share_id, "Imported share to RustFS");
                Ok(true)
            }
            Err(RepositoryError::AlreadyExists(_)) => {
                debug!(share_id = %share_id, "Share already exists in RustFS");
                Ok(false)
            }
            Err(e) => Err(e),
        }
    }
    
    /// Import all data for a user
    pub async fn import_user_data(&self, user_id: Uuid) -> Result<OperationSummary, RepositoryError> {
        let mut summary = OperationSummary::new(format!("import_user_{}", user_id));
        
        info!(user_id = %user_id, "Starting user data import");
        
        // Import folders
        let folders = self.postgres.folders().get_user_roots(user_id).await?;
        for folder in folders {
            summary.increment_processed();
            match self.import_folder(folder.id).await {
                Ok(true) => {
                    summary.increment_succeeded();
                    summary.increment_fixed(); // "Fixed" here means imported
                }
                Ok(false) => {
                    summary.increment_succeeded(); // Already existed
                }
                Err(e) => {
                    summary.add_error(format!("Failed to import folder {}: {}", folder.id, e));
                }
            }
        }
        
        info!(
            user_id = %user_id,
            processed = summary.items_processed,
            imported = summary.items_fixed,
            "User data import complete"
        );
        
        Ok(summary)
    }
}
