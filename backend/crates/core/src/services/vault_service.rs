//! Vault sync service and storage traits.
//!
//! Obsidian is a trademark of Dynalist Inc. RustShare is not affiliated with,
//! endorsed by, or sponsored by Obsidian.

use crate::domain::{Vault, VaultDevice, VaultFile};
use crate::services::VaultSyncError;
use uuid::Uuid;

/// Trait for vault storage operations.
///
/// This trait abstracts the metadata store to allow for testing without database dependencies.
#[allow(async_fn_in_trait, clippy::too_many_arguments)]
pub trait VaultStore: Send + Sync {
    /// Create a new vault.
    async fn create_vault(&self, vault: &Vault) -> Result<Vault, VaultSyncError>;

    /// Get a vault by ID.
    async fn get_vault(&self, vault_id: Uuid, tenant_id: Uuid) -> Result<Vault, VaultSyncError>;

    /// List vaults for an owner within a tenant.
    async fn list_vaults(
        &self,
        tenant_id: Uuid,
        owner_id: Uuid,
    ) -> Result<Vec<Vault>, VaultSyncError>;

    /// Get a file by vault ID and relative path.
    async fn get_file(
        &self,
        vault_id: Uuid,
        relative_path: &str,
        tenant_id: Uuid,
    ) -> Result<VaultFile, VaultSyncError>;

    /// Get a file by vault ID and relative path, including tombstones.
    async fn get_file_including_deleted(
        &self,
        vault_id: Uuid,
        relative_path: &str,
        tenant_id: Uuid,
    ) -> Result<VaultFile, VaultSyncError>;

    /// List all files in a vault.
    async fn list_files(
        &self,
        vault_id: Uuid,
        tenant_id: Uuid,
        limit: Option<i64>,
    ) -> Result<Vec<VaultFile>, VaultSyncError>;

    /// Atomically increment vault revision and update an existing file ONLY if
    /// its current server_rev matches base_server_rev.  Returns the updated
    /// file on success, or `None` if the revision did not match.
    async fn update_file_conditional_atomic(
        &self,
        file: &VaultFile,
        base_server_rev: i64,
    ) -> Result<Option<VaultFile>, VaultSyncError>;

    /// Insert a new file in a vault, atomically incrementing the vault revision.
    /// Returns the inserted file on success.
    async fn insert_file_atomic(&self, file: &VaultFile) -> Result<VaultFile, VaultSyncError>;

    /// Atomically increment vault revision and tombstone a file ONLY if its
    /// current server_rev matches base_server_rev.  Returns the updated file
    /// on success, or `None` if the revision did not match.
    async fn tombstone_file_conditional_atomic(
        &self,
        vault_id: Uuid,
        relative_path: &str,
        tenant_id: Uuid,
        base_server_rev: i64,
        device_id: &str,
    ) -> Result<Option<VaultFile>, VaultSyncError>;

    /// Atomically increment vault revision and rename a file ONLY if its
    /// current server_rev matches base_server_rev.  Returns the updated file
    /// on success, or `None` if the revision did not match.
    async fn rename_file_conditional_atomic(
        &self,
        vault_id: Uuid,
        old_path: &str,
        new_path: &str,
        tenant_id: Uuid,
        base_server_rev: i64,
        device_id: &str,
    ) -> Result<Option<VaultFile>, VaultSyncError>;

    /// Register a new device for vault sync.
    async fn register_device(&self, device: &VaultDevice) -> Result<VaultDevice, VaultSyncError>;

    /// Get a device by its device ID string.
    async fn get_device(
        &self,
        device_id: &str,
        tenant_id: Uuid,
    ) -> Result<VaultDevice, VaultSyncError>;

    /// Bind an unbound device to a vault.
    async fn bind_device_to_vault(
        &self,
        device_id: &str,
        tenant_id: Uuid,
        vault_id: Uuid,
    ) -> Result<VaultDevice, VaultSyncError>;

    /// Update an existing vault.
    async fn update_vault(&self, vault: &Vault) -> Result<Vault, VaultSyncError>;

    /// Look up an existing WebUI device for a user/vault pair.
    async fn get_webui_device(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
        vault_id: Uuid,
    ) -> Result<Option<VaultDevice>, VaultSyncError>;

    /// Create a WebUI device row for a vault.
    async fn create_webui_device(
        &self,
        device: &VaultDevice,
    ) -> Result<VaultDevice, VaultSyncError>;

    /// Revoke a device.
    async fn revoke_device(&self, device_id: Uuid, tenant_id: Uuid) -> Result<(), VaultSyncError>;

    /// Update the last_seen_at timestamp for a device.
    async fn update_device_last_seen(
        &self,
        device_id: &str,
        tenant_id: Uuid,
    ) -> Result<(), VaultSyncError>;
}
