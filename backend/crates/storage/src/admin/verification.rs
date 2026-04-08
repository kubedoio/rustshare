//! Verification tools for metadata consistency
//!
//! Provides tools to verify:
//! - Parity between PostgreSQL and RustFS backends
//! - Internal consistency of metadata documents
//! - Index consistency with canonical documents

use std::sync::Arc;
use tracing::info;
use uuid::Uuid;

use super::OperationSummary;
use crate::repos::*;

/// Result of verifying a single entity
#[derive(Debug, Clone)]
pub struct VerificationResult {
    pub entity_type: String,
    pub entity_id: Uuid,
    pub passed: bool,
    pub errors: Vec<String>,
}

impl VerificationResult {
    pub fn new(entity_type: impl Into<String>, entity_id: Uuid) -> Self {
        Self {
            entity_type: entity_type.into(),
            entity_id,
            passed: true,
            errors: Vec::new(),
        }
    }

    pub fn add_error(&mut self, error: impl Into<String>) {
        self.errors.push(error.into());
        self.passed = false;
    }
}

/// Comprehensive verification report
#[derive(Debug, Clone)]
pub struct VerificationReport {
    pub folders_verified: usize,
    pub folders_passed: usize,
    pub files_verified: usize,
    pub files_passed: usize,
    pub shares_verified: usize,
    pub shares_passed: usize,
    pub versions_verified: usize,
    pub versions_passed: usize,
    pub indexes_verified: usize,
    pub indexes_passed: usize,
    pub errors: Vec<String>,
}

impl VerificationReport {
    pub fn new() -> Self {
        Self {
            folders_verified: 0,
            folders_passed: 0,
            files_verified: 0,
            files_passed: 0,
            shares_verified: 0,
            shares_passed: 0,
            versions_verified: 0,
            versions_passed: 0,
            indexes_verified: 0,
            indexes_passed: 0,
            errors: Vec::new(),
        }
    }

    pub fn overall_pass_rate(&self) -> f64 {
        let total = self.folders_verified
            + self.files_verified
            + self.shares_verified
            + self.versions_verified
            + self.indexes_verified;
        let passed = self.folders_passed
            + self.files_passed
            + self.shares_passed
            + self.versions_passed
            + self.indexes_passed;

        if total == 0 {
            100.0
        } else {
            (passed as f64 / total as f64) * 100.0
        }
    }

    pub fn all_passed(&self) -> bool {
        self.folders_passed == self.folders_verified
            && self.files_passed == self.files_verified
            && self.shares_passed == self.shares_verified
            && self.versions_passed == self.versions_verified
            && self.indexes_passed == self.indexes_verified
    }
}

impl Default for VerificationReport {
    fn default() -> Self {
        Self::new()
    }
}

/// Verifies parity between two metadata backends
pub struct ParityVerifier {
    postgres: Arc<dyn MetadataRepository>,
    rustfs: Arc<dyn MetadataRepository>,
}

impl ParityVerifier {
    pub fn new(postgres: Arc<dyn MetadataRepository>, rustfs: Arc<dyn MetadataRepository>) -> Self {
        Self { postgres, rustfs }
    }

    /// Verify a single folder exists in both backends with same data
    pub async fn verify_folder(
        &self,
        folder_id: Uuid,
    ) -> Result<VerificationResult, RepositoryError> {
        let mut result = VerificationResult::new("folder", folder_id);

        let pg_folder = self.postgres.folders().get(folder_id).await?;
        let rustfs_folder = self.rustfs.folders().get(folder_id).await?;

        match (pg_folder, rustfs_folder) {
            (Some(pg), Some(rustfs)) => {
                // Compare key fields
                if pg.id != rustfs.id {
                    result.add_error(format!("ID mismatch: {} vs {}", pg.id, rustfs.id));
                }
                if pg.name != rustfs.name {
                    result.add_error(format!("Name mismatch: '{}' vs '{}'", pg.name, rustfs.name));
                }
                if pg.parent_id != rustfs.parent_id {
                    result.add_error(format!(
                        "Parent mismatch: {:?} vs {:?}",
                        pg.parent_id, rustfs.parent_id
                    ));
                }
                if pg.owner_id != rustfs.owner_id {
                    result.add_error(format!(
                        "Owner mismatch: {} vs {}",
                        pg.owner_id, rustfs.owner_id
                    ));
                }
                if pg.version != rustfs.version {
                    result.add_error(format!(
                        "Version mismatch: {} vs {}",
                        pg.version, rustfs.version
                    ));
                }
                if pg.deleted != rustfs.deleted {
                    result.add_error(format!(
                        "Deleted flag mismatch: {} vs {}",
                        pg.deleted, rustfs.deleted
                    ));
                }
            }
            (Some(_), None) => {
                result.add_error("Folder exists in PostgreSQL but not in RustFS".to_string());
            }
            (None, Some(_)) => {
                result.add_error("Folder exists in RustFS but not in PostgreSQL".to_string());
            }
            (None, None) => {
                result.add_error("Folder does not exist in either backend".to_string());
            }
        }

        Ok(result)
    }

    /// Verify a single file exists in both backends with same data
    pub async fn verify_file(&self, file_id: Uuid) -> Result<VerificationResult, RepositoryError> {
        let mut result = VerificationResult::new("file", file_id);

        let pg_file = self.postgres.files().get(file_id).await?;
        let rustfs_file = self.rustfs.files().get(file_id).await?;

        match (pg_file, rustfs_file) {
            (Some(pg), Some(rustfs)) => {
                if pg.id != rustfs.id {
                    result.add_error(format!("ID mismatch: {} vs {}", pg.id, rustfs.id));
                }
                if pg.name != rustfs.name {
                    result.add_error(format!("Name mismatch: '{}' vs '{}'", pg.name, rustfs.name));
                }
                if pg.content_ref != rustfs.content_ref {
                    result.add_error(format!(
                        "Content ref mismatch: {} vs {}",
                        pg.content_ref, rustfs.content_ref
                    ));
                }
                if pg.size != rustfs.size {
                    result.add_error(format!("Size mismatch: {} vs {}", pg.size, rustfs.size));
                }
                if pg.parent_id != rustfs.parent_id {
                    result.add_error(format!(
                        "Parent mismatch: {:?} vs {:?}",
                        pg.parent_id, rustfs.parent_id
                    ));
                }
                if pg.owner_id != rustfs.owner_id {
                    result.add_error(format!(
                        "Owner mismatch: {} vs {}",
                        pg.owner_id, rustfs.owner_id
                    ));
                }
                if pg.version != rustfs.version {
                    result.add_error(format!(
                        "Version mismatch: {} vs {}",
                        pg.version, rustfs.version
                    ));
                }
            }
            (Some(_), None) => {
                result.add_error("File exists in PostgreSQL but not in RustFS".to_string());
            }
            (None, Some(_)) => {
                result.add_error("File exists in RustFS but not in PostgreSQL".to_string());
            }
            (None, None) => {
                result.add_error("File does not exist in either backend".to_string());
            }
        }

        Ok(result)
    }

    /// Verify a share exists in both backends
    pub async fn verify_share(
        &self,
        share_id: Uuid,
    ) -> Result<VerificationResult, RepositoryError> {
        let mut result = VerificationResult::new("share", share_id);

        let pg_share = self.postgres.shares().get(share_id).await?;
        let rustfs_share = self.rustfs.shares().get(share_id).await?;

        match (pg_share, rustfs_share) {
            (Some(pg), Some(rustfs)) => {
                if pg.id != rustfs.id {
                    result.add_error(format!("ID mismatch: {} vs {}", pg.id, rustfs.id));
                }
                if pg.resource_type != rustfs.resource_type {
                    result.add_error(format!(
                        "Resource type mismatch: {} vs {}",
                        pg.resource_type, rustfs.resource_type
                    ));
                }
                if pg.resource_id != rustfs.resource_id {
                    result.add_error(format!(
                        "Resource ID mismatch: {} vs {}",
                        pg.resource_id, rustfs.resource_id
                    ));
                }
                if pg.permissions != rustfs.permissions {
                    result.add_error(format!(
                        "Permissions mismatch: {:?} vs {:?}",
                        pg.permissions, rustfs.permissions
                    ));
                }
                if pg.revoked_at != rustfs.revoked_at {
                    result.add_error(format!(
                        "Revoked_at mismatch: {:?} vs {:?}",
                        pg.revoked_at, rustfs.revoked_at
                    ));
                }
            }
            (Some(_), None) => {
                result.add_error("Share exists in PostgreSQL but not in RustFS".to_string());
            }
            (None, Some(_)) => {
                result.add_error("Share exists in RustFS but not in PostgreSQL".to_string());
            }
            (None, None) => {
                result.add_error("Share does not exist in either backend".to_string());
            }
        }

        Ok(result)
    }

    /// Run full verification on a sample of entities
    pub async fn verify_sample(
        &self,
        folder_limit: usize,
        file_limit: usize,
        share_limit: usize,
    ) -> Result<VerificationReport, RepositoryError> {
        let report = VerificationReport::new();

        // Note: In a real implementation, we would scan from PostgreSQL
        // and verify each entity in RustFS. For now, this is a placeholder
        // that demonstrates the structure.

        info!(
            folder_limit,
            file_limit, share_limit, "Starting sample verification"
        );

        // Verification would iterate through entities from primary backend
        // and verify each exists in secondary with matching data

        Ok(report)
    }

    /// Verify all entities for a specific user
    pub async fn verify_user(&self, user_id: Uuid) -> Result<VerificationReport, RepositoryError> {
        let mut report = VerificationReport::new();

        info!(user_id = %user_id, "Starting user verification");

        // Get user's folders from both backends
        let pg_folders = self.postgres.folders().get_user_roots(user_id).await?;
        let rustfs_folders = self.rustfs.folders().get_user_roots(user_id).await?;

        // Compare folder counts
        if pg_folders.len() != rustfs_folders.len() {
            report.errors.push(format!(
                "Folder count mismatch for user {}: PostgreSQL={}, RustFS={}",
                user_id,
                pg_folders.len(),
                rustfs_folders.len()
            ));
        }

        // Verify each folder
        for folder in &pg_folders {
            report.folders_verified += 1;
            let result = self.verify_folder(folder.id).await?;
            if result.passed {
                report.folders_passed += 1;
            } else {
                report.errors.extend(result.errors);
            }
        }

        info!(
            user_id = %user_id,
            folders_verified = report.folders_verified,
            folders_passed = report.folders_passed,
            "User verification complete"
        );

        Ok(report)
    }
}

/// Verifies internal consistency of metadata documents
pub struct ConsistencyVerifier {
    _repo: Arc<dyn MetadataRepository>,
}

impl ConsistencyVerifier {
    pub fn new(repo: Arc<dyn MetadataRepository>) -> Self {
        Self { _repo: repo }
    }

    /// Verify folder hierarchy consistency
    pub async fn verify_folder_hierarchy(&self) -> Result<OperationSummary, RepositoryError> {
        let summary = OperationSummary::new("folder_hierarchy_verification");

        // This would:
        // 1. Scan all folders
        // 2. Verify parent references exist
        // 3. Detect circular references
        // 4. Verify path consistency

        info!("Starting folder hierarchy verification");

        Ok(summary)
    }

    /// Verify file version consistency
    pub async fn verify_file_versions(&self) -> Result<OperationSummary, RepositoryError> {
        let summary = OperationSummary::new("file_version_verification");

        // This would:
        // 1. Scan all files
        // 2. Verify current_version_id points to existing version
        // 3. Verify version_number sequence is continuous
        // 4. Verify content_ref matches checksum

        info!("Starting file version verification");

        Ok(summary)
    }

    /// Verify share consistency
    pub async fn verify_shares(&self) -> Result<OperationSummary, RepositoryError> {
        let summary = OperationSummary::new("share_verification");

        // This would:
        // 1. Verify all shares point to valid resources
        // 2. Verify recipient users exist (for user shares)
        // 3. Verify token uniqueness (for public shares)

        info!("Starting share verification");

        Ok(summary)
    }

    /// Verify index consistency with canonical documents
    pub async fn verify_indexes(&self) -> Result<OperationSummary, RepositoryError> {
        let summary = OperationSummary::new("index_verification");

        // This would:
        // 1. Scan all folder children indexes
        // 2. Verify each entry matches canonical document
        // 3. Detect missing or extra entries

        info!("Starting index verification");

        Ok(summary)
    }
}

/// Quick health check for the metadata system
pub struct HealthChecker {
    repo: Arc<dyn MetadataRepository>,
}

impl HealthChecker {
    pub fn new(repo: Arc<dyn MetadataRepository>) -> Self {
        Self { repo }
    }

    /// Perform a quick health check
    pub async fn check(&self) -> HealthStatus {
        let mut status = HealthStatus::new();

        // Check that we can read from the repository
        match self.repo.folders().get_user_roots(Uuid::nil()).await {
            Ok(_) => {
                status.healthy = true;
                status.checks.push("folder_read".to_string());
            }
            Err(e) => {
                status.healthy = false;
                status.errors.push(format!("Folder read failed: {}", e));
            }
        }

        // Check file repository
        match self.repo.files().get(Uuid::nil()).await {
            Ok(_) => {
                status.checks.push("file_read".to_string());
            }
            Err(e) => {
                status.healthy = false;
                status.errors.push(format!("File read failed: {}", e));
            }
        }

        status
    }
}

/// Health status result
#[derive(Debug, Clone)]
pub struct HealthStatus {
    pub healthy: bool,
    pub checks: Vec<String>,
    pub errors: Vec<String>,
}

impl HealthStatus {
    pub fn new() -> Self {
        Self {
            healthy: false,
            checks: Vec::new(),
            errors: Vec::new(),
        }
    }
}

impl Default for HealthStatus {
    fn default() -> Self {
        Self::new()
    }
}
