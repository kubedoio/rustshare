//! Dual-write adapters for migration
//!
//! These adapters write to both the old (Postgres) and new (RustFS) backends,
//! allowing for safe migration with verification.

use async_trait::async_trait;
use rustshare_core::domain::{FileId, FolderId, ShareId, UserId};
use std::sync::Arc;
use tracing::{error, warn};
use uuid::Uuid;

use super::{RepositoryError, *};
use crate::metadata_v2::schemas::*;

/// Configuration for dual-write behavior
#[derive(Debug, Clone)]
pub struct DualWriteConfig {
    /// Continue on error from secondary backend
    pub continue_on_secondary_error: bool,
    /// Log mismatches for analysis
    pub log_mismatches: bool,
    /// Verify reads (compare both backends)
    pub verify_reads: bool,
    /// Primary backend for reads ("postgres" or "rustfs")
    pub primary_backend: String,
}

impl Default for DualWriteConfig {
    fn default() -> Self {
        Self {
            continue_on_secondary_error: true,
            log_mismatches: true,
            verify_reads: false,
            primary_backend: "postgres".to_string(),
        }
    }
}

/// Dual-write folder repository
pub struct DualWriteFolderRepository {
    primary: Arc<dyn FolderRepository>,
    secondary: Arc<dyn FolderRepository>,
    config: DualWriteConfig,
}

impl DualWriteFolderRepository {
    pub fn new(
        primary: Arc<dyn FolderRepository>,
        secondary: Arc<dyn FolderRepository>,
        config: DualWriteConfig,
    ) -> Self {
        Self {
            primary,
            secondary,
            config,
        }
    }
    
    /// Log a mismatch between primary and secondary
    fn log_mismatch(&self, operation: &str, primary_result: &str, secondary_result: &str) {
        if self.config.log_mismatches {
            warn!(
                operation = operation,
                primary = primary_result,
                secondary = secondary_result,
                "Dual-write mismatch detected"
            );
        }
    }
}

#[async_trait]
impl FolderRepository for DualWriteFolderRepository {
    async fn get(&self, id: FolderId) -> Result<Option<FolderDocument>, RepositoryError> {
        if self.config.verify_reads {
            // Read from both and compare
            let primary_result = self.primary.get(id).await;
            let secondary_result = self.secondary.get(id).await;
            
            match (&primary_result, &secondary_result) {
                (Ok(Some(p)), Ok(Some(s))) => {
                    if p.id != s.id || p.version != s.version {
                        self.log_mismatch(
                            &format!("folder_get:{}", id),
                            &format!("{:?}", p),
                            &format!("{:?}", s),
                        );
                    }
                }
                (Ok(None), Ok(Some(s))) => {
                    self.log_mismatch(
                        &format!("folder_get:{}", id),
                        "None",
                        &format!("{:?}", s),
                    );
                }
                (Ok(Some(p)), Ok(None)) => {
                    self.log_mismatch(
                        &format!("folder_get:{}", id),
                        &format!("{:?}", p),
                        "None",
                    );
                }
                (Err(e1), Ok(_)) => {
                    warn!(error = %e1, "Primary backend error, using secondary");
                    return secondary_result;
                }
                _ => {}
            }
            
            primary_result
        } else {
            // Read from primary only
            self.primary.get(id).await
        }
    }
    
    async fn create(&self, folder: &FolderDocument) -> Result<(), RepositoryError> {
        // Write to primary first
        self.primary.create(folder).await?;
        
        // Write to secondary
        if let Err(e) = self.secondary.create(folder).await {
            error!(error = %e, folder_id = %folder.id, "Failed to write folder to secondary backend");
            
            if !self.config.continue_on_secondary_error {
                return Err(e);
            }
        }
        
        Ok(())
    }
    
    async fn update(&self, folder: &FolderDocument) -> Result<(), RepositoryError> {
        // Write to primary first
        self.primary.update(folder).await?;
        
        // Write to secondary
        if let Err(e) = self.secondary.update(folder).await {
            error!(error = %e, folder_id = %folder.id, "Failed to update folder in secondary backend");
            
            if !self.config.continue_on_secondary_error {
                return Err(e);
            }
        }
        
        Ok(())
    }
    
    async fn delete(&self, id: FolderId, deleted_by: UserId) -> Result<(), RepositoryError> {
        // Write to primary first
        self.primary.delete(id, deleted_by).await?;
        
        // Write to secondary
        if let Err(e) = self.secondary.delete(id, deleted_by).await {
            error!(error = %e, folder_id = %id, "Failed to delete folder in secondary backend");
            
            if !self.config.continue_on_secondary_error {
                return Err(e);
            }
        }
        
        Ok(())
    }
    
    async fn hard_delete(&self, id: FolderId) -> Result<(), RepositoryError> {
        // Write to primary first
        self.primary.hard_delete(id).await?;
        
        // Write to secondary
        if let Err(e) = self.secondary.hard_delete(id).await {
            error!(error = %e, folder_id = %id, "Failed to hard-delete folder in secondary backend");
            
            if !self.config.continue_on_secondary_error {
                return Err(e);
            }
        }
        
        Ok(())
    }
    
    async fn list_descendants(&self, folder_id: FolderId) -> Result<Vec<FolderDocument>, RepositoryError> {
        // Always use primary for complex operations during migration
        self.primary.list_descendants(folder_id).await
    }
    
    async fn get_user_roots(&self, user_id: UserId) -> Result<Vec<FolderDocument>, RepositoryError> {
        self.primary.get_user_roots(user_id).await
    }
    
    async fn name_exists(
        &self,
        parent_id: Option<FolderId>,
        name: &str,
        owner_id: UserId,
    ) -> Result<bool, RepositoryError> {
        self.primary.name_exists(parent_id, name, owner_id).await
    }
}

/// Dual-write file repository
pub struct DualWriteFileRepository {
    primary: Arc<dyn FileRepository>,
    secondary: Arc<dyn FileRepository>,
    config: DualWriteConfig,
}

impl DualWriteFileRepository {
    pub fn new(
        primary: Arc<dyn FileRepository>,
        secondary: Arc<dyn FileRepository>,
        config: DualWriteConfig,
    ) -> Self {
        Self {
            primary,
            secondary,
            config,
        }
    }
}

#[async_trait]
impl FileRepository for DualWriteFileRepository {
    async fn get(&self, id: FileId) -> Result<Option<FileDocument>, RepositoryError> {
        self.primary.get(id).await
    }
    
    async fn create(&self, file: &FileDocument) -> Result<(), RepositoryError> {
        self.primary.create(file).await?;
        
        if let Err(e) = self.secondary.create(file).await {
            error!(error = %e, file_id = %file.id, "Failed to write file to secondary backend");
            
            if !self.config.continue_on_secondary_error {
                return Err(e);
            }
        }
        
        Ok(())
    }
    
    async fn update(&self, file: &FileDocument) -> Result<(), RepositoryError> {
        self.primary.update(file).await?;
        
        if let Err(e) = self.secondary.update(file).await {
            error!(error = %e, file_id = %file.id, "Failed to update file in secondary backend");
            
            if !self.config.continue_on_secondary_error {
                return Err(e);
            }
        }
        
        Ok(())
    }
    
    async fn delete(&self, id: FileId, deleted_by: UserId) -> Result<(), RepositoryError> {
        self.primary.delete(id, deleted_by).await?;
        
        if let Err(e) = self.secondary.delete(id, deleted_by).await {
            error!(error = %e, file_id = %id, "Failed to delete file in secondary backend");
            
            if !self.config.continue_on_secondary_error {
                return Err(e);
            }
        }
        
        Ok(())
    }
    
    async fn hard_delete(&self, id: FileId) -> Result<(), RepositoryError> {
        self.primary.hard_delete(id).await?;
        
        if let Err(e) = self.secondary.hard_delete(id).await {
            error!(error = %e, file_id = %id, "Failed to hard-delete file in secondary backend");
            
            if !self.config.continue_on_secondary_error {
                return Err(e);
            }
        }
        
        Ok(())
    }
    
    async fn name_exists(
        &self,
        parent_id: Option<FolderId>,
        name: &str,
        owner_id: UserId,
    ) -> Result<bool, RepositoryError> {
        self.primary.name_exists(parent_id, name, owner_id).await
    }
}

/// Dual-write share repository
pub struct DualWriteShareRepository {
    primary: Arc<dyn ShareRepository>,
    secondary: Arc<dyn ShareRepository>,
    config: DualWriteConfig,
}

impl DualWriteShareRepository {
    pub fn new(
        primary: Arc<dyn ShareRepository>,
        secondary: Arc<dyn ShareRepository>,
        config: DualWriteConfig,
    ) -> Self {
        Self {
            primary,
            secondary,
            config,
        }
    }
}

#[async_trait]
impl ShareRepository for DualWriteShareRepository {
    async fn get(&self, id: ShareId) -> Result<Option<ShareDocument>, RepositoryError> {
        self.primary.get(id).await
    }
    
    async fn get_by_token(&self, token_hash: &str) -> Result<Option<ShareDocument>, RepositoryError> {
        self.primary.get_by_token(token_hash).await
    }
    
    async fn create(&self, share: &ShareDocument) -> Result<(), RepositoryError> {
        self.primary.create(share).await?;
        
        if let Err(e) = self.secondary.create(share).await {
            error!(error = %e, share_id = %share.id, "Failed to write share to secondary backend");
            
            if !self.config.continue_on_secondary_error {
                return Err(e);
            }
        }
        
        Ok(())
    }
    
    async fn update(&self, share: &ShareDocument) -> Result<(), RepositoryError> {
        self.primary.update(share).await?;
        
        if let Err(e) = self.secondary.update(share).await {
            error!(error = %e, share_id = %share.id, "Failed to update share in secondary backend");
            
            if !self.config.continue_on_secondary_error {
                return Err(e);
            }
        }
        
        Ok(())
    }
    
    async fn revoke(&self, id: ShareId, revoked_by: UserId) -> Result<(), RepositoryError> {
        self.primary.revoke(id, revoked_by).await?;
        
        if let Err(e) = self.secondary.revoke(id, revoked_by).await {
            error!(error = %e, share_id = %id, "Failed to revoke share in secondary backend");
            
            if !self.config.continue_on_secondary_error {
                return Err(e);
            }
        }
        
        Ok(())
    }
    
    async fn delete(&self, id: ShareId) -> Result<(), RepositoryError> {
        self.primary.delete(id).await?;
        
        if let Err(e) = self.secondary.delete(id).await {
            error!(error = %e, share_id = %id, "Failed to delete share in secondary backend");
            
            if !self.config.continue_on_secondary_error {
                return Err(e);
            }
        }
        
        Ok(())
    }
    
    async fn list_by_resource(
        &self,
        resource_type: &str,
        resource_id: Uuid,
    ) -> Result<Vec<ShareDocument>, RepositoryError> {
        self.primary.list_by_resource(resource_type, resource_id).await
    }
    
    async fn list_by_creator(&self, user_id: UserId) -> Result<Vec<ShareDocument>, RepositoryError> {
        self.primary.list_by_creator(user_id).await
    }
    
    async fn list_by_recipient(&self, user_id: UserId) -> Result<Vec<ShareDocument>, RepositoryError> {
        self.primary.list_by_recipient(user_id).await
    }
    
    async fn increment_access_count(&self, id: ShareId) -> Result<(), RepositoryError> {
        self.primary.increment_access_count(id).await?;
        
        if let Err(e) = self.secondary.increment_access_count(id).await {
            error!(error = %e, share_id = %id, "Failed to increment access count in secondary backend");
            
            if !self.config.continue_on_secondary_error {
                return Err(e);
            }
        }
        
        Ok(())
    }
}

/// Dual-write file version repository
pub struct DualWriteFileVersionRepository {
    primary: Arc<dyn FileVersionRepository>,
    secondary: Arc<dyn FileVersionRepository>,
    config: DualWriteConfig,
}

impl DualWriteFileVersionRepository {
    pub fn new(
        primary: Arc<dyn FileVersionRepository>,
        secondary: Arc<dyn FileVersionRepository>,
        config: DualWriteConfig,
    ) -> Self {
        Self {
            primary,
            secondary,
            config,
        }
    }
}

#[async_trait]
impl FileVersionRepository for DualWriteFileVersionRepository {
    async fn get(&self, version_id: Uuid) -> Result<Option<FileVersionDocument>, RepositoryError> {
        self.primary.get(version_id).await
    }
    
    async fn get_by_number(
        &self,
        file_id: FileId,
        version_number: i32,
    ) -> Result<Option<FileVersionDocument>, RepositoryError> {
        self.primary.get_by_number(file_id, version_number).await
    }
    
    async fn create(&self, version: &FileVersionDocument) -> Result<(), RepositoryError> {
        self.primary.create(version).await?;
        
        if let Err(e) = self.secondary.create(version).await {
            error!(error = %e, version_id = %version.id, "Failed to write version to secondary backend");
            
            if !self.config.continue_on_secondary_error {
                return Err(e);
            }
        }
        
        Ok(())
    }
    
    async fn list_by_file(&self, file_id: FileId) -> Result<Vec<FileVersionDocument>, RepositoryError> {
        self.primary.list_by_file(file_id).await
    }
    
    async fn get_latest(&self, file_id: FileId) -> Result<Option<FileVersionDocument>, RepositoryError> {
        self.primary.get_latest(file_id).await
    }
}

/// Verification tool for comparing primary and secondary backends
pub struct DualWriteVerifier {
    postgres: Arc<dyn MetadataRepository>,
    rustfs: Arc<dyn MetadataRepository>,
}

impl DualWriteVerifier {
    pub fn new(postgres: Arc<dyn MetadataRepository>, rustfs: Arc<dyn MetadataRepository>) -> Self {
        Self { postgres, rustfs }
    }
    
    /// Verify a folder exists in both backends with same data
    pub async fn verify_folder(&self, folder_id: FolderId) -> Result<bool, RepositoryError> {
        let pg_result = self.postgres.folders().get(folder_id).await?;
        let rustfs_result = self.rustfs.folders().get(folder_id).await?;
        
        match (pg_result, rustfs_result) {
            (Some(pg), Some(rustfs)) => {
                let matches = pg.id == rustfs.id
                    && pg.name == rustfs.name
                    && pg.parent_id == rustfs.parent_id
                    && pg.owner_id == rustfs.owner_id
                    && pg.version == rustfs.version;
                
                if !matches {
                    warn!(
                        folder_id = %folder_id,
                        pg_version = pg.version,
                        rustfs_version = rustfs.version,
                        "Folder mismatch detected"
                    );
                }
                
                Ok(matches)
            }
            (None, None) => Ok(true),
            _ => {
                warn!(folder_id = %folder_id, "Folder exists in only one backend");
                Ok(false)
            }
        }
    }
    
    /// Verify a file exists in both backends with same data
    pub async fn verify_file(&self, file_id: FileId) -> Result<bool, RepositoryError> {
        let pg_result = self.postgres.files().get(file_id).await?;
        let rustfs_result = self.rustfs.files().get(file_id).await?;
        
        match (pg_result, rustfs_result) {
            (Some(pg), Some(rustfs)) => {
                let matches = pg.id == rustfs.id
                    && pg.name == rustfs.name
                    && pg.content_ref == rustfs.content_ref
                    && pg.version == rustfs.version;
                
                if !matches {
                    warn!(
                        file_id = %file_id,
                        pg_version = pg.version,
                        rustfs_version = rustfs.version,
                        "File mismatch detected"
                    );
                }
                
                Ok(matches)
            }
            (None, None) => Ok(true),
            _ => {
                warn!(file_id = %file_id, "File exists in only one backend");
                Ok(false)
            }
        }
    }
    
    /// Run verification on a sample of entities
    pub async fn verify_sample(&self, _sample_size: usize) -> Result<VerificationReport, RepositoryError> {
        // This would scan and verify a random sample
        // For now, return an empty report
        Ok(VerificationReport {
            folders_checked: 0,
            folders_matched: 0,
            files_checked: 0,
            files_matched: 0,
            shares_checked: 0,
            shares_matched: 0,
        })
    }
}

/// Verification report
#[derive(Debug, Clone)]
pub struct VerificationReport {
    pub folders_checked: usize,
    pub folders_matched: usize,
    pub files_checked: usize,
    pub files_matched: usize,
    pub shares_checked: usize,
    pub shares_matched: usize,
}

impl VerificationReport {
    /// Overall match percentage
    pub fn match_percentage(&self) -> f64 {
        let total = self.folders_checked + self.files_checked + self.shares_checked;
        let matched = self.folders_matched + self.files_matched + self.shares_matched;
        
        if total == 0 {
            100.0
        } else {
            (matched as f64 / total as f64) * 100.0
        }
    }
    
    /// Check if verification passed (all matched)
    pub fn passed(&self) -> bool {
        self.folders_matched == self.folders_checked
            && self.files_matched == self.files_checked
            && self.shares_matched == self.shares_checked
    }
}
