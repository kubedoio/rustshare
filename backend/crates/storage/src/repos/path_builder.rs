//! Shared path builder for RustFS repository implementations

use crate::metadata_v2::schemas::EventDocument;
use rustshare_core::domain::{FileId, FolderId, ShareId, UserId};
use uuid::Uuid;

/// Path builder for RustFS storage layout
#[derive(Clone)]
pub struct PathBuilder {
    base_prefix: String,
    namespace: String,
}

impl PathBuilder {
    pub fn new(base_prefix: String, namespace: String) -> Self {
        Self {
            base_prefix,
            namespace,
        }
    }

    pub fn base_prefix(&self) -> &str {
        &self.base_prefix
    }

    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    /// Path for notification document
    pub fn notification_path(&self, user_id: Uuid, notification_id: Uuid) -> String {
        format!(
            "{}/{}/notifications/{}/{}.json",
            self.base_prefix, self.namespace, user_id, notification_id
        )
    }

    /// Path for job document
    pub fn job_path(&self, job_id: Uuid) -> String {
        format!(
            "{}/{}/jobs/{}.json",
            self.base_prefix, self.namespace, job_id
        )
    }

    /// Path for user document
    pub fn user_path(&self, user_id: Uuid) -> String {
        format!(
            "{}/{}/users/{}.json",
            self.base_prefix, self.namespace, user_id
        )
    }

    /// Path for folder document
    pub fn folder_path(&self, id: FolderId) -> String {
        format!(
            "{}/{}/meta/folders/{}.json",
            self.base_prefix, self.namespace, id
        )
    }

    /// Path for file document
    pub fn file_path(&self, id: FileId) -> String {
        format!(
            "{}/{}/meta/files/{}.json",
            self.base_prefix, self.namespace, id
        )
    }

    /// Path for file version document
    pub fn file_version_path(&self, file_id: FileId, version_id: Uuid) -> String {
        format!(
            "{}/{}/meta/file_versions/{}/{}.json",
            self.base_prefix, self.namespace, file_id, version_id
        )
    }

    /// Path for share document
    pub fn share_path(&self, id: ShareId) -> String {
        format!(
            "{}/{}/meta/shares/{}.json",
            self.base_prefix, self.namespace, id
        )
    }

    /// Path for event document
    pub fn event_path(&self, event: &EventDocument) -> String {
        use chrono::Datelike;
        format!(
            "{}/{}/meta/events/{:04}/{:02}/{:02}/{}.json",
            self.base_prefix,
            self.namespace,
            event.occurred_at.year(),
            event.occurred_at.month(),
            event.occurred_at.day(),
            event.id
        )
    }

    /// Path for tombstone document
    pub fn tombstone_path(&self, resource_type: &str, resource_id: Uuid) -> String {
        format!(
            "{}/{}/meta/tombstones/{}/{}.json",
            self.base_prefix, self.namespace, resource_type, resource_id
        )
    }

    /// Path for folder children index
    pub fn folder_children_index_path(&self, folder_id: FolderId) -> String {
        format!(
            "{}/{}/indexes/folders/{}/children.json",
            self.base_prefix, self.namespace, folder_id
        )
    }

    /// Path for user notification index
    pub fn user_index_path(&self, user_id: Uuid) -> String {
        format!(
            "{}/{}/indexes/notifications/by-user/{}.json",
            self.base_prefix, self.namespace, user_id
        )
    }

    /// Path for job queue index
    pub fn queue_index_path(&self) -> String {
        format!(
            "{}/{}/indexes/jobs/queue.json",
            self.base_prefix, self.namespace
        )
    }

    /// Path for email index entry
    pub fn email_index_path(&self, email: &str) -> String {
        let email_hash = Self::hash_string(email.to_lowercase().as_str());
        format!(
            "{}/{}/indexes/users/by-email/{}.json",
            self.base_prefix, self.namespace, email_hash
        )
    }

    /// Path for username index entry
    pub fn username_index_path(&self, username: &str) -> String {
        let username_hash = Self::hash_string(username.to_lowercase().as_str());
        format!(
            "{}/{}/indexes/users/by-username/{}.json",
            self.base_prefix, self.namespace, username_hash
        )
    }

    /// Path for user list index
    pub fn user_list_path(&self) -> String {
        format!(
            "{}/{}/indexes/users/all.json",
            self.base_prefix, self.namespace
        )
    }

    /// Path for user roots index
    pub fn user_roots_index(&self, user_id: UserId) -> String {
        format!(
            "{}/{}/indexes/users/{}/roots.json",
            self.base_prefix, self.namespace, user_id
        )
    }

    /// Path for shared with me index
    pub fn shared_with_me_index(&self, user_id: UserId) -> String {
        format!(
            "{}/{}/indexes/users/{}/shared_with_me.json",
            self.base_prefix, self.namespace, user_id
        )
    }

    /// Path for sync cursor document
    pub fn sync_cursor_path(&self, user_id: Uuid, device_id: Uuid) -> String {
        format!(
            "{}/{}/sync/cursors/{}/{}.json",
            self.base_prefix, self.namespace, user_id, device_id
        )
    }

    /// Path for user's groups list index
    pub fn user_groups_path(&self, user_id: Uuid) -> String {
        format!(
            "{}/{}/indexes/users/{}/groups.json",
            self.base_prefix, self.namespace, user_id
        )
    }

    /// Path for group's members list index
    pub fn group_members_path(&self, group_id: Uuid) -> String {
        format!(
            "{}/{}/indexes/groups/{}/members.json",
            self.base_prefix, self.namespace, group_id
        )
    }

    /// Path for tenant config document
    pub fn tenant_config_path(&self, tenant_id: Uuid) -> String {
        format!(
            "{}/{}/config/tenants/{}.json",
            self.base_prefix, self.namespace, tenant_id
        )
    }

    /// Simple hash for index keys
    fn hash_string(s: &str) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(s.as_bytes());
        hex::encode(hasher.finalize())
    }
}
