//! Storage layer for RustShare.
//!
//! Handles persistence to RustFS with optional Redis coordination.

pub mod admin;
pub mod coordination;
pub mod event_store;
pub mod metadata;
pub mod metadata_v2;
pub mod object_store;
pub mod repos;
pub mod session;

pub use event_store::EventStore;
pub use metadata::{
    MetadataStore, PublicShareAccessLogEntry, ReplicationAttemptRecord, SecurityConfig,
    ShareAccessLogEntry, UserSecurityEvent, UserSecurityEventRecord,
};
pub use object_store::ObjectStore;

// Implement service layer traits for storage types
use anyhow::Result;
use rustshare_core::services::{
    FileEventStoreOps, FileMetadataStoreOps, FolderEventStoreOps, FolderMetadataStoreOps,
    ObjectStoreOps as CoreObjectStoreOps, ShareEventStoreOps, ShareMetadataStoreOps,
};

// EventStore implements both File and Folder event store traits
impl FileEventStoreOps for EventStore {
    async fn append(
        &self,
        event: &rustshare_core::events::Event,
        broadcaster: &rustshare_core::events::EventBroadcaster,
    ) -> Result<()> {
        self.append(event, broadcaster).await
    }
}

impl FolderEventStoreOps for EventStore {
    async fn append(
        &self,
        event: &rustshare_core::events::Event,
        broadcaster: &rustshare_core::events::EventBroadcaster,
    ) -> Result<()> {
        self.append(event, broadcaster).await
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
    async fn create_file(&self, file: &rustshare_core::domain::File) -> Result<()> {
        self.create_file(file).await
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

    async fn find_folder_by_id(
        &self,
        id: uuid::Uuid,
        owner_id: uuid::Uuid,
    ) -> Result<Option<rustshare_core::domain::Folder>> {
        self.find_folder_by_id(id, owner_id).await
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

    async fn delete_file(&self, id: uuid::Uuid, owner_id: uuid::Uuid) -> Result<()> {
        self.delete_file(id, owner_id).await
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
    async fn create_folder(&self, folder: &rustshare_core::domain::Folder) -> Result<()> {
        self.create_folder(folder).await
    }

    async fn find_folder_by_id(
        &self,
        id: uuid::Uuid,
        owner_id: uuid::Uuid,
    ) -> Result<Option<rustshare_core::domain::Folder>> {
        self.find_folder_by_id(id, owner_id).await
    }

    async fn update_folder(&self, folder: &rustshare_core::domain::Folder) -> Result<()> {
        self.update_folder(folder).await
    }

    async fn delete_folder(&self, id: uuid::Uuid, owner_id: uuid::Uuid) -> Result<()> {
        self.delete_folder(id, owner_id).await
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

    async fn get_share_by_token(
        &self,
        token: &str,
    ) -> Result<Option<rustshare_core::domain::Share>> {
        self.get_share_by_token(token).await
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

    async fn revoke_share(&self, share_id: uuid::Uuid, actor_id: rustshare_core::domain::UserId) -> Result<()> {
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

// ObjectStore implements ObjectStoreOps trait
impl CoreObjectStoreOps for ObjectStore {
    async fn put(&self, key: &str, data: bytes::Bytes) -> Result<()> {
        self.put(key, data).await
    }

    async fn exists(&self, key: &str) -> Result<bool> {
        self.exists(key).await
    }

    async fn get_presigned_url(&self, key: &str, expires_in_secs: u64) -> Result<String> {
        self.get_presigned_url(key, expires_in_secs).await
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
