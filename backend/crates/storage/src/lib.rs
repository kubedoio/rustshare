//! Storage layer for RustShare.
//!
//! Handles persistence to RustFS with optional Redis coordination.
//!
//! Obsidian is a trademark of Dynalist Inc. RustShare is not affiliated with,
//! endorsed by, or sponsored by Obsidian.

pub mod chat_integration_impl;
pub mod coordination;
pub mod event_store;
pub mod metadata;
pub mod object_store;
pub mod repos;
pub mod session;
pub mod upload_doc_store;

pub use event_store::EventStore;
use metadata::VaultFileStoreError;
pub use metadata::{
    MetadataStore, PublicShareAccessLogEntry, ReplicationAttemptRecord, SecurityConfig,
    ShareAccessLogEntry, UserSecurityEvent, UserSecurityEventRecord,
};
pub use object_store::{ObjectStore, ObjectStoreOptions};

// Implement service layer traits for storage types
use anyhow::Result;
use rustshare_core::domain::{Vault, VaultDevice, VaultFile};
use rustshare_core::services::{
    FileEventStoreOps, FileMetadataStoreOps, FolderEventStoreOps, FolderMetadataStoreOps,
    ObjectStoreOps as CoreObjectStoreOps, ShareEventStoreOps, ShareMetadataStoreOps, VaultStore,
    VaultSyncError,
};

// EventStore implements both File and Folder event store traits
impl FileEventStoreOps for EventStore {
    type Tx = sqlx::Transaction<'static, sqlx::Postgres>;

    async fn append(
        &self,
        event: &rustshare_core::events::Event,
        broadcaster: &rustshare_core::events::EventBroadcaster,
    ) -> Result<()> {
        self.append(event, broadcaster).await
    }

    async fn begin_transaction(&self) -> Result<Self::Tx> {
        self.begin_transaction().await
    }

    async fn commit_transaction(&self, tx: Self::Tx) -> Result<()> {
        tx.commit().await?;
        Ok(())
    }

    async fn append_in_tx(
        &self,
        tx: &mut Self::Tx,
        event: &rustshare_core::events::Event,
    ) -> Result<()> {
        self.append_in_tx(tx, event).await
    }
}

impl FolderEventStoreOps for EventStore {
    type Tx = sqlx::Transaction<'static, sqlx::Postgres>;

    async fn append(
        &self,
        event: &rustshare_core::events::Event,
        broadcaster: &rustshare_core::events::EventBroadcaster,
    ) -> Result<()> {
        self.append(event, broadcaster).await
    }

    async fn begin_transaction(&self) -> Result<Self::Tx> {
        self.begin_transaction().await
    }

    async fn commit_transaction(&self, tx: Self::Tx) -> Result<()> {
        tx.commit().await?;
        Ok(())
    }

    async fn append_in_tx(
        &self,
        tx: &mut Self::Tx,
        event: &rustshare_core::events::Event,
    ) -> Result<()> {
        self.append_in_tx(tx, event).await
    }
}

impl ShareEventStoreOps for EventStore {
    async fn append(
        &self,
        event: &rustshare_core::events::Event,
        broadcaster: &rustshare_core::events::EventBroadcaster,
    ) -> Result<()> {
        self.append(event, broadcaster).await
    }
}

// MetadataStore implements both File and Folder metadata store traits
impl FileMetadataStoreOps for MetadataStore {
    type Tx = sqlx::Transaction<'static, sqlx::Postgres>;

    async fn create_file(&self, file: &rustshare_core::domain::File) -> Result<()> {
        self.create_file(file).await
    }

    async fn create_file_in_tx(
        &self,
        tx: &mut Self::Tx,
        file: &rustshare_core::domain::File,
    ) -> Result<()> {
        self.create_file_in_tx(tx, file).await
    }

    async fn find_file_by_path(
        &self,
        path: &str,
        owner_id: uuid::Uuid,
    ) -> Result<Option<rustshare_core::domain::File>> {
        self.find_file_by_path(path, owner_id).await
    }

    async fn create_file_version(
        &self,
        version: &rustshare_core::domain::FileVersion,
    ) -> Result<()> {
        self.create_file_version(version).await
    }

    async fn create_file_version_in_tx(
        &self,
        tx: &mut Self::Tx,
        version: &rustshare_core::domain::FileVersion,
    ) -> Result<()> {
        self.create_file_version_in_tx(tx, version).await
    }

    async fn find_folder_by_id(
        &self,
        id: uuid::Uuid,
        owner_id: uuid::Uuid,
    ) -> Result<Option<rustshare_core::domain::Folder>> {
        self.find_folder_by_id(id, owner_id).await
    }

    async fn find_folder_by_id_unchecked(
        &self,
        id: uuid::Uuid,
    ) -> Result<Option<rustshare_core::domain::Folder>> {
        self.find_folder_by_id_unchecked(id).await
    }

    async fn find_file_by_id(
        &self,
        id: uuid::Uuid,
        owner_id: uuid::Uuid,
    ) -> Result<Option<rustshare_core::domain::File>> {
        self.find_file_by_id(id, owner_id).await
    }

    async fn find_file_by_id_unchecked(
        &self,
        id: uuid::Uuid,
    ) -> Result<Option<rustshare_core::domain::File>> {
        self.find_file_by_id_unchecked(id).await
    }

    async fn update_file(&self, file: &rustshare_core::domain::File) -> Result<()> {
        self.update_file(file).await
    }

    async fn update_file_in_tx(
        &self,
        tx: &mut Self::Tx,
        file: &rustshare_core::domain::File,
    ) -> Result<()> {
        self.update_file_in_tx(tx, file).await
    }

    async fn delete_file(&self, id: uuid::Uuid, owner_id: uuid::Uuid) -> Result<()> {
        self.delete_file(id, owner_id).await
    }

    async fn delete_file_in_tx(
        &self,
        tx: &mut Self::Tx,
        id: uuid::Uuid,
        owner_id: uuid::Uuid,
    ) -> Result<()> {
        self.delete_file_in_tx(tx, id, owner_id).await
    }

    async fn list_file_versions(
        &self,
        file_id: uuid::Uuid,
        owner_id: uuid::Uuid,
    ) -> Result<Vec<rustshare_core::domain::FileVersion>> {
        self.list_file_versions(file_id, owner_id).await
    }

    async fn find_file_version(
        &self,
        file_id: uuid::Uuid,
        version: i32,
        owner_id: uuid::Uuid,
    ) -> Result<Option<rustshare_core::domain::FileVersion>> {
        self.find_file_version(file_id, version, owner_id).await
    }

    async fn count_enabled_replication_targets(&self) -> Result<i64> {
        self.count_enabled_replication_targets().await
    }

    async fn create_replication_job(
        &self,
        job: &rustshare_core::domain::ReplicationJob,
    ) -> Result<()> {
        self.create_replication_job(job).await
    }

    async fn update_file_version_replication_state(
        &self,
        version_id: uuid::Uuid,
        state: rustshare_core::domain::ReplicationState,
    ) -> Result<()> {
        self.update_file_version_replication_state(version_id, state)
            .await
    }
}

impl FolderMetadataStoreOps for MetadataStore {
    type Tx = sqlx::Transaction<'static, sqlx::Postgres>;

    async fn create_folder(&self, folder: &rustshare_core::domain::Folder) -> Result<()> {
        self.create_folder(folder).await
    }

    async fn create_folder_in_tx(
        &self,
        tx: &mut Self::Tx,
        folder: &rustshare_core::domain::Folder,
    ) -> Result<()> {
        self.create_folder_in_tx(tx, folder).await
    }

    async fn find_folder_by_id(
        &self,
        id: uuid::Uuid,
        owner_id: uuid::Uuid,
    ) -> Result<Option<rustshare_core::domain::Folder>> {
        self.find_folder_by_id(id, owner_id).await
    }

    async fn find_folder_by_id_unchecked(
        &self,
        id: uuid::Uuid,
    ) -> Result<Option<rustshare_core::domain::Folder>> {
        self.find_folder_by_id_unchecked(id).await
    }

    async fn update_folder(&self, folder: &rustshare_core::domain::Folder) -> Result<()> {
        self.update_folder(folder).await
    }

    async fn update_folder_in_tx(
        &self,
        tx: &mut Self::Tx,
        folder: &rustshare_core::domain::Folder,
    ) -> Result<()> {
        self.update_folder_in_tx(tx, folder).await
    }

    async fn delete_folder(&self, id: uuid::Uuid, owner_id: uuid::Uuid) -> Result<()> {
        self.delete_folder(id, owner_id).await
    }

    async fn delete_folder_in_tx(
        &self,
        tx: &mut Self::Tx,
        id: uuid::Uuid,
        owner_id: uuid::Uuid,
    ) -> Result<()> {
        self.delete_folder_in_tx(tx, id, owner_id).await
    }

    async fn list_folders(
        &self,
        parent_id: Option<uuid::Uuid>,
        owner_id: uuid::Uuid,
        tenant_id: uuid::Uuid,
    ) -> Result<Vec<rustshare_core::domain::Folder>> {
        self.list_folders(parent_id, owner_id, tenant_id).await
    }

    async fn list_folders_by_parent(
        &self,
        parent_id: Option<uuid::Uuid>,
        tenant_id: uuid::Uuid,
    ) -> Result<Vec<rustshare_core::domain::Folder>> {
        self.list_folders_by_parent(parent_id, tenant_id).await
    }

    async fn find_descendant_folders(
        &self,
        folder_id: uuid::Uuid,
        owner_id: uuid::Uuid,
    ) -> Result<Vec<rustshare_core::domain::Folder>> {
        self.find_descendant_folders(folder_id, owner_id).await
    }

    async fn list_files(
        &self,
        parent_id: Option<uuid::Uuid>,
        owner_id: uuid::Uuid,
        tenant_id: uuid::Uuid,
    ) -> Result<Vec<rustshare_core::domain::File>> {
        self.list_files(parent_id, owner_id, tenant_id).await
    }

    async fn list_files_by_parent(
        &self,
        parent_id: Option<uuid::Uuid>,
        tenant_id: uuid::Uuid,
    ) -> Result<Vec<rustshare_core::domain::File>> {
        self.list_files_by_parent(parent_id, tenant_id).await
    }
}

impl ShareMetadataStoreOps for MetadataStore {
    async fn find_user_by_id(
        &self,
        id: uuid::Uuid,
    ) -> Result<Option<rustshare_core::domain::User>> {
        self.find_user_by_id(id).await
    }

    async fn find_file_by_id(
        &self,
        id: uuid::Uuid,
        owner_id: uuid::Uuid,
    ) -> Result<Option<rustshare_core::domain::File>> {
        self.find_file_by_id(id, owner_id).await
    }

    async fn find_file_by_id_unchecked(
        &self,
        id: uuid::Uuid,
    ) -> Result<Option<rustshare_core::domain::File>> {
        self.find_file_by_id_unchecked(id).await
    }

    async fn find_folder_by_id(
        &self,
        id: uuid::Uuid,
        owner_id: uuid::Uuid,
    ) -> Result<Option<rustshare_core::domain::Folder>> {
        self.find_folder_by_id(id, owner_id).await
    }

    async fn find_folder_by_id_unchecked(
        &self,
        id: uuid::Uuid,
    ) -> Result<Option<rustshare_core::domain::Folder>> {
        self.find_folder_by_id_unchecked(id).await
    }

    async fn create_share(&self, share: &rustshare_core::domain::Share) -> Result<()> {
        self.create_share(share).await
    }

    async fn get_share_by_id(
        &self,
        id: uuid::Uuid,
        actor_id: rustshare_core::domain::UserId,
    ) -> Result<Option<rustshare_core::domain::Share>> {
        self.get_share(id, actor_id).await
    }

    async fn get_share_by_id_unchecked(
        &self,
        id: uuid::Uuid,
    ) -> Result<Option<rustshare_core::domain::Share>> {
        self.get_share_unchecked(id).await
    }

    async fn get_share_by_token(
        &self,
        token: &str,
        tenant_id: uuid::Uuid,
    ) -> Result<Option<rustshare_core::domain::Share>> {
        self.get_share_by_token(token, tenant_id).await
    }

    async fn get_share_by_token_unscoped(
        &self,
        token: &str,
    ) -> Result<Option<rustshare_core::domain::Share>> {
        self.get_share_by_token_unscoped(token).await
    }

    async fn get_file_shares(
        &self,
        file_id: uuid::Uuid,
    ) -> Result<Vec<rustshare_core::domain::Share>> {
        self.get_file_shares(file_id).await
    }

    async fn get_folder_shares(
        &self,
        folder_id: uuid::Uuid,
    ) -> Result<Vec<rustshare_core::domain::Share>> {
        self.get_folder_shares(folder_id).await
    }

    async fn list_files(
        &self,
        parent_id: Option<uuid::Uuid>,
        owner_id: uuid::Uuid,
        tenant_id: uuid::Uuid,
    ) -> Result<Vec<rustshare_core::domain::File>> {
        self.list_files(parent_id, owner_id, tenant_id).await
    }

    async fn list_files_by_parent(
        &self,
        parent_id: Option<uuid::Uuid>,
        tenant_id: uuid::Uuid,
    ) -> Result<Vec<rustshare_core::domain::File>> {
        self.list_files_by_parent(parent_id, tenant_id).await
    }

    async fn list_folders(
        &self,
        parent_id: Option<uuid::Uuid>,
        owner_id: uuid::Uuid,
        tenant_id: uuid::Uuid,
    ) -> Result<Vec<rustshare_core::domain::Folder>> {
        self.list_folders(parent_id, owner_id, tenant_id).await
    }

    async fn list_folders_by_parent(
        &self,
        parent_id: Option<uuid::Uuid>,
        tenant_id: uuid::Uuid,
    ) -> Result<Vec<rustshare_core::domain::Folder>> {
        self.list_folders_by_parent(parent_id, tenant_id).await
    }

    async fn find_descendant_folders(
        &self,
        folder_id: uuid::Uuid,
        owner_id: uuid::Uuid,
    ) -> Result<Vec<rustshare_core::domain::Folder>> {
        self.find_descendant_folders(folder_id, owner_id).await
    }

    async fn find_descendant_folders_unchecked(
        &self,
        folder_id: uuid::Uuid,
    ) -> Result<Vec<rustshare_core::domain::Folder>> {
        self.find_descendant_folders_unchecked(folder_id).await
    }

    async fn revoke_share(
        &self,
        share_id: uuid::Uuid,
        actor_id: rustshare_core::domain::UserId,
    ) -> Result<()> {
        self.revoke_share(share_id, actor_id).await
    }

    async fn update_share(&self, share: &rustshare_core::domain::Share) -> Result<()> {
        self.update_share(share).await
    }

    async fn is_user_in_group(
        &self,
        user_id: rustshare_core::domain::UserId,
        group_id: uuid::Uuid,
    ) -> Result<bool> {
        self.is_user_in_group(user_id, group_id).await
    }
}

// MetadataStore implements VaultStore trait
impl VaultStore for MetadataStore {
    async fn create_vault(&self, vault: &Vault) -> Result<Vault, VaultSyncError> {
        self.create_vault(vault)
            .await
            .map_err(|e| VaultSyncError::Database(e.to_string()))
    }

    async fn get_vault(
        &self,
        vault_id: uuid::Uuid,
        tenant_id: uuid::Uuid,
    ) -> Result<Vault, VaultSyncError> {
        self.get_vault(vault_id, tenant_id)
            .await
            .map_err(|e| VaultSyncError::Database(e.to_string()))?
            .ok_or(VaultSyncError::VaultNotFound(vault_id))
    }

    async fn list_vaults(
        &self,
        tenant_id: uuid::Uuid,
        owner_id: uuid::Uuid,
    ) -> Result<Vec<Vault>, VaultSyncError> {
        self.list_vaults(tenant_id, owner_id)
            .await
            .map_err(|e| VaultSyncError::Database(e.to_string()))
    }

    async fn get_file(
        &self,
        vault_id: uuid::Uuid,
        relative_path: &str,
        tenant_id: uuid::Uuid,
    ) -> Result<VaultFile, VaultSyncError> {
        self.get_vault_file(vault_id, relative_path, tenant_id)
            .await
            .map_err(|e| VaultSyncError::Database(e.to_string()))?
            .ok_or_else(|| VaultSyncError::FileNotFound(relative_path.to_string()))
    }

    async fn get_file_including_deleted(
        &self,
        vault_id: uuid::Uuid,
        relative_path: &str,
        tenant_id: uuid::Uuid,
    ) -> Result<VaultFile, VaultSyncError> {
        self.get_vault_file_including_deleted(vault_id, relative_path, tenant_id)
            .await
            .map_err(|e| VaultSyncError::Database(e.to_string()))?
            .ok_or_else(|| VaultSyncError::FileNotFound(relative_path.to_string()))
    }

    async fn list_files(
        &self,
        vault_id: uuid::Uuid,
        tenant_id: uuid::Uuid,
        limit: Option<i64>,
    ) -> Result<Vec<VaultFile>, VaultSyncError> {
        self.list_vault_files(vault_id, tenant_id, limit)
            .await
            .map_err(|e| VaultSyncError::Database(e.to_string()))
    }

    async fn insert_file_atomic(&self, file: &VaultFile) -> Result<VaultFile, VaultSyncError> {
        self.insert_vault_file_atomic(file)
            .await
            .map_err(|e| match e {
                sqlx::Error::RowNotFound => VaultSyncError::VaultNotFound(file.vault_id),
                _ => VaultSyncError::Database(e.to_string()),
            })
    }

    async fn update_file_conditional_atomic(
        &self,
        file: &VaultFile,
        base_server_rev: i64,
    ) -> Result<Option<VaultFile>, VaultSyncError> {
        self.update_vault_file_conditional_atomic(file, base_server_rev)
            .await
            .map_err(|e| match e {
                sqlx::Error::RowNotFound => VaultSyncError::VaultNotFound(file.vault_id),
                _ => VaultSyncError::Database(e.to_string()),
            })
    }

    async fn tombstone_file_conditional_atomic(
        &self,
        vault_id: uuid::Uuid,
        relative_path: &str,
        tenant_id: uuid::Uuid,
        base_server_rev: i64,
        device_id: &str,
    ) -> Result<Option<VaultFile>, VaultSyncError> {
        self.tombstone_vault_file_conditional_atomic(
            vault_id,
            relative_path,
            tenant_id,
            base_server_rev,
            device_id,
        )
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => VaultSyncError::VaultNotFound(vault_id),
            _ => VaultSyncError::Database(e.to_string()),
        })
    }

    async fn rename_file_conditional_atomic(
        &self,
        vault_id: uuid::Uuid,
        old_path: &str,
        new_path: &str,
        tenant_id: uuid::Uuid,
        base_server_rev: i64,
        device_id: &str,
    ) -> Result<Option<VaultFile>, VaultSyncError> {
        self.rename_vault_file_conditional_atomic(
            vault_id,
            old_path,
            new_path,
            tenant_id,
            base_server_rev,
            device_id,
        )
        .await
        .map_err(|e| {
            if let Some(err) = e.downcast_ref::<VaultFileStoreError>() {
                match err {
                    VaultFileStoreError::NotFound => {
                        VaultSyncError::FileNotFound(old_path.to_string())
                    }
                    VaultFileStoreError::DestinationExists => {
                        VaultSyncError::FileAlreadyExists(new_path.to_string())
                    }
                }
            } else if e
                .downcast_ref::<sqlx::Error>()
                .map(|se| matches!(se, sqlx::Error::RowNotFound))
                .unwrap_or(false)
            {
                VaultSyncError::VaultNotFound(vault_id)
            } else {
                VaultSyncError::Database(e.to_string())
            }
        })
    }

    async fn register_device(&self, device: &VaultDevice) -> Result<VaultDevice, VaultSyncError> {
        self.create_vault_device(device)
            .await
            .map_err(|e| VaultSyncError::Database(e.to_string()))
    }

    async fn get_device(
        &self,
        device_id: &str,
        tenant_id: uuid::Uuid,
    ) -> Result<VaultDevice, VaultSyncError> {
        self.get_vault_device(device_id, tenant_id)
            .await
            .map_err(|e| VaultSyncError::Database(e.to_string()))?
            .ok_or_else(|| VaultSyncError::DeviceNotFound(device_id.to_string()))
    }

    async fn bind_device_to_vault(
        &self,
        device_id: &str,
        tenant_id: uuid::Uuid,
        vault_id: uuid::Uuid,
    ) -> Result<VaultDevice, VaultSyncError> {
        match self
            .bind_vault_device_to_vault(device_id, tenant_id, vault_id)
            .await
        {
            Ok(device) => Ok(device),
            Err(sqlx::Error::RowNotFound) => {
                match self.get_vault_device(device_id, tenant_id).await {
                    Ok(Some(device)) if device.revoked_at.is_some() => {
                        Err(VaultSyncError::DeviceRevoked)
                    }
                    Ok(Some(_)) => Err(VaultSyncError::Unauthorized),
                    Ok(None) => Err(VaultSyncError::DeviceNotFound(device_id.to_string())),
                    Err(e) => Err(VaultSyncError::Database(e.to_string())),
                }
            }
            Err(e) => Err(VaultSyncError::Database(e.to_string())),
        }
    }

    async fn revoke_device(
        &self,
        device_id: uuid::Uuid,
        tenant_id: uuid::Uuid,
    ) -> Result<(), VaultSyncError> {
        self.revoke_vault_device(device_id, tenant_id)
            .await
            .map_err(|e| match e {
                sqlx::Error::RowNotFound => VaultSyncError::DeviceNotFound(device_id.to_string()),
                _ => VaultSyncError::Database(e.to_string()),
            })
    }

    async fn update_device_last_seen(
        &self,
        device_id: &str,
        tenant_id: uuid::Uuid,
    ) -> Result<(), VaultSyncError> {
        self.update_vault_device_last_seen(device_id, tenant_id)
            .await
            .map_err(|e| match e {
                sqlx::Error::RowNotFound => VaultSyncError::DeviceRevoked,
                _ => VaultSyncError::Database(e.to_string()),
            })
    }
}

// ObjectStore implements ObjectStoreOps trait
impl CoreObjectStoreOps for ObjectStore {
    async fn put(&self, key: &str, data: bytes::Bytes) -> Result<()> {
        self.put(key, data).await
    }

    async fn put_from_path(&self, key: &str, path: &std::path::Path) -> Result<()> {
        self.put_from_path(key, path).await
    }

    async fn exists(&self, key: &str) -> Result<bool> {
        self.exists(key).await
    }

    async fn get(&self, key: &str) -> Result<bytes::Bytes> {
        self.get(key).await
    }

    async fn delete(&self, key: &str) -> Result<()> {
        self.delete(key).await
    }
}

// ShareNotificationRepoImpl implements the core ShareNotificationRepo trait
#[async_trait::async_trait]
impl rustshare_core::services::ShareNotificationRepo for repos::ShareNotificationRepoImpl {
    async fn was_notified(
        &self,
        user_id: rustshare_core::domain::UserId,
        share_id: uuid::Uuid,
    ) -> Result<bool, sqlx::Error> {
        self.was_notified(user_id, share_id).await
    }

    async fn record_notification(
        &self,
        user_id: rustshare_core::domain::UserId,
        share_id: uuid::Uuid,
    ) -> Result<(), sqlx::Error> {
        self.record_notification(user_id, share_id).await
    }
}
