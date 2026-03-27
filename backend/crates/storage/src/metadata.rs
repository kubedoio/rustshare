//! DEPRECATED: PostgreSQL-based MetadataStore
//!
//! This module is deprecated as part of the PostgreSQL removal migration.
//! The storage crate no longer supports PostgreSQL as a metadata backend.
//!
//! Use `metadata_v2` module instead, which provides:
//! - `MetadataRepository` trait for metadata operations
//! - `MetadataDocumentStore` for document storage
//! - `CombinedMetadataRepository` for the main implementation
//!
//! Migration guide:
//! - Replace `MetadataStore` with `Arc<dyn MetadataRepository>` from `metadata_v2`
//! - Use `RustFsDocumentStore` or `LocalFsDocumentStore` for storage backends

/// DEPRECATED: MetadataStore has been removed.
///
/// This struct no longer functions. Use `metadata_v2::CombinedMetadataRepository` instead.
#[deprecated(
    since = "0.2.0",
    note = "PostgreSQL-based MetadataStore has been removed. Use metadata_v2::CombinedMetadataRepository instead."
)]
pub struct MetadataStore {
    _private: (),
}

#[deprecated(
    since = "0.2.0",
    note = "Use metadata_v2 types instead"
)]
pub struct OwnedPublicShare {
    pub share: rustshare_core::domain::Share,
    pub resource_id: uuid::Uuid,
    pub resource_type: String,
    pub resource_name: String,
}

#[deprecated(
    since = "0.2.0",
    note = "Use metadata_v2 types instead"
)]
pub struct PublicShareAccessLogEntry {
    pub accessed_at: chrono::DateTime<chrono::Utc>,
    pub action: String,
    pub success: bool,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub actor_type: Option<String>,
    pub actor_label: Option<String>,
    pub share_session_id: Option<uuid::Uuid>,
    pub share_session_subject: Option<String>,
}

#[deprecated(
    since = "0.2.0",
    note = "Use metadata_v2 types instead"
)]
pub struct ReplicationAttemptRecord<'a> {
    pub job_id: uuid::Uuid,
    pub target_id: uuid::Uuid,
    pub attempt_number: i32,
    pub status: &'a str,
    pub error_message: Option<&'a str>,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: chrono::DateTime<chrono::Utc>,
}

#[deprecated(
    since = "0.2.0",
    note = "Use metadata_v2 types instead"
)]
pub struct ShareAccessLogEntry {
    pub share_id: uuid::Uuid,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub action: String,
    pub success: bool,
    pub actor_type: Option<String>,
    pub actor_label: Option<String>,
    pub share_session_id: Option<uuid::Uuid>,
    pub share_session_subject: Option<String>,
}

#[deprecated(
    since = "0.2.0",
    note = "Use metadata_v2 types instead"
)]
pub struct UserSecurityEventRecord<'a> {
    pub user_id: uuid::Uuid,
    pub event_type: &'a str,
    pub description: &'a str,
    pub ip_address: Option<&'a str>,
    pub user_agent: Option<&'a str>,
    pub session_id: Option<uuid::Uuid>,
}

#[deprecated(
    since = "0.2.0",
    note = "Use metadata_v2 types instead"
)]
pub struct UserSecurityEvent {
    pub id: uuid::Uuid,
    pub event_type: String,
    pub description: String,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub session_id: Option<uuid::Uuid>,
    pub occurred_at: chrono::DateTime<chrono::Utc>,
}

#[allow(deprecated)]
impl MetadataStore {
    /// DEPRECATED: Creates a stub that will panic if used.
    #[deprecated(
        since = "0.2.0",
        note = "MetadataStore::new is no longer available. Use metadata_v2 instead."
    )]
    pub fn new(_pool: ()) -> Self {
        Self { _private: () }
    }

    fn err<T>() -> anyhow::Result<T> {
        Err(anyhow::anyhow!(
            "MetadataStore has been deprecated. Use metadata_v2::CombinedMetadataRepository instead."
        ))
    }

    pub async fn create_user(&self, _user: &rustshare_core::domain::User) -> anyhow::Result<()> {
        Self::err()
    }

    pub async fn create_user_session(
        &self,
        _session: &rustshare_core::domain::UserSession,
    ) -> anyhow::Result<()> {
        Self::err()
    }

    pub async fn find_user_session_by_token_hash(
        &self,
        _token_hash: &str,
    ) -> anyhow::Result<Option<rustshare_core::domain::UserSession>> {
        Self::err()
    }

    pub async fn touch_user_session(&self, _session_id: uuid::Uuid) -> anyhow::Result<()> {
        Self::err()
    }

    pub async fn delete_user_session_by_token_hash(&self, _token_hash: &str) -> anyhow::Result<()> {
        Self::err()
    }

    pub async fn list_user_sessions(
        &self,
        _user_id: uuid::Uuid,
    ) -> anyhow::Result<Vec<rustshare_core::domain::UserSession>> {
        Self::err()
    }

    pub async fn delete_user_session_by_id(
        &self,
        _user_id: uuid::Uuid,
        _session_id: uuid::Uuid,
    ) -> anyhow::Result<()> {
        Self::err()
    }

    pub async fn create_user_security_event(
        &self,
        _event: UserSecurityEventRecord<'_>,
    ) -> anyhow::Result<()> {
        Self::err()
    }

    pub async fn list_user_security_events(
        &self,
        _user_id: uuid::Uuid,
        _limit: i64,
    ) -> anyhow::Result<Vec<UserSecurityEvent>> {
        Self::err()
    }

    pub async fn create_oidc_login_state(
        &self,
        _login_state: &rustshare_core::domain::OidcLoginState,
    ) -> anyhow::Result<()> {
        Self::err()
    }

    pub async fn find_oidc_login_state(
        &self,
        _state: &str,
    ) -> anyhow::Result<Option<rustshare_core::domain::OidcLoginState>> {
        Self::err()
    }

    pub async fn delete_oidc_login_state(&self, _state: &str) -> anyhow::Result<()> {
        Self::err()
    }

    pub async fn find_user_by_email(
        &self,
        _email: &str,
    ) -> anyhow::Result<Option<rustshare_core::domain::User>> {
        Self::err()
    }

    pub async fn find_user_by_username(
        &self,
        _username: &str,
    ) -> anyhow::Result<Option<rustshare_core::domain::User>> {
        Self::err()
    }

    pub async fn find_user_by_id(
        &self,
        _id: uuid::Uuid,
    ) -> anyhow::Result<Option<rustshare_core::domain::User>> {
        Self::err()
    }

    pub async fn update_user_password_hash(
        &self,
        _id: uuid::Uuid,
        _password_hash: &str,
    ) -> anyhow::Result<()> {
        Self::err()
    }

    pub async fn has_users(&self) -> anyhow::Result<bool> {
        Self::err()
    }

    pub async fn update_user_theme(&self, _user_id: uuid::Uuid, _theme: &str) -> anyhow::Result<()> {
        Self::err()
    }

    pub async fn update_user_profile(
        &self,
        _user_id: uuid::Uuid,
        _name: Option<&str>,
        _surname: Option<&str>,
        _display_name: Option<&str>,
        _email_sharing_enabled: Option<bool>,
        _theme: Option<String>,
    ) -> anyhow::Result<()> {
        Self::err()
    }

    pub async fn update_user_avatar(
        &self,
        _user_id: uuid::Uuid,
        _avatar_path: Option<&str>,
    ) -> anyhow::Result<()> {
        Self::err()
    }

    pub async fn create_file(&self, _file: &rustshare_core::domain::File) -> anyhow::Result<()> {
        Self::err()
    }

    pub async fn find_file_by_id(
        &self,
        _id: uuid::Uuid,
    ) -> anyhow::Result<Option<rustshare_core::domain::File>> {
        Self::err()
    }

    pub async fn find_file_by_path(
        &self,
        _path: &str,
        _owner_id: uuid::Uuid,
    ) -> anyhow::Result<Option<rustshare_core::domain::File>> {
        Self::err()
    }

    pub async fn update_file(&self, _file: &rustshare_core::domain::File) -> anyhow::Result<()> {
        Self::err()
    }

    pub async fn delete_file(&self, _id: uuid::Uuid) -> anyhow::Result<()> {
        Self::err()
    }

    pub async fn list_files(
        &self,
        _parent_id: Option<uuid::Uuid>,
        _owner_id: uuid::Uuid,
    ) -> anyhow::Result<Vec<rustshare_core::domain::File>> {
        Self::err()
    }

    pub async fn create_file_version(
        &self,
        _version: &rustshare_core::domain::FileVersion,
    ) -> anyhow::Result<()> {
        Self::err()
    }

    pub async fn list_file_versions(
        &self,
        _file_id: uuid::Uuid,
    ) -> anyhow::Result<Vec<rustshare_core::domain::FileVersion>> {
        Self::err()
    }

    pub async fn find_file_version(
        &self,
        _file_id: uuid::Uuid,
        _version: i32,
    ) -> anyhow::Result<Option<rustshare_core::domain::FileVersion>> {
        Self::err()
    }

    pub async fn count_enabled_replication_targets(&self) -> anyhow::Result<i64> {
        Self::err()
    }

    pub async fn create_replication_job(
        &self,
        _job: &rustshare_core::domain::ReplicationJob,
    ) -> anyhow::Result<()> {
        Self::err()
    }

    pub async fn update_file_version_replication_state(
        &self,
        _version_id: uuid::Uuid,
        _state: rustshare_core::domain::ReplicationState,
    ) -> anyhow::Result<()> {
        Self::err()
    }

    pub async fn list_enabled_replication_targets(
        &self,
    ) -> anyhow::Result<Vec<rustshare_core::domain::ReplicationTarget>> {
        Self::err()
    }

    pub async fn create_replication_target(
        &self,
        _target: &rustshare_core::domain::ReplicationTarget,
    ) -> anyhow::Result<()> {
        Self::err()
    }

    pub async fn list_replication_attempts(
        &self,
        _job_id: uuid::Uuid,
    ) -> anyhow::Result<Vec<ReplicationAttemptRecord<'_>>> {
        Self::err()
    }

    pub async fn create_replication_attempt(
        &self,
        _attempt: ReplicationAttemptRecord<'_>,
    ) -> anyhow::Result<()> {
        Self::err()
    }

    pub async fn acquire_replication_job_lease(
        &self,
        _batch_size: i32,
    ) -> anyhow::Result<Vec<rustshare_core::domain::ReplicationJob>> {
        Self::err()
    }

    pub async fn update_replication_job(
        &self,
        _job: &rustshare_core::domain::ReplicationJob,
    ) -> anyhow::Result<()> {
        Self::err()
    }

    pub async fn create_folder(
        &self,
        _folder: &rustshare_core::domain::Folder,
    ) -> anyhow::Result<()> {
        Self::err()
    }

    pub async fn find_folder_by_id(
        &self,
        _id: uuid::Uuid,
    ) -> anyhow::Result<Option<rustshare_core::domain::Folder>> {
        Self::err()
    }

    pub async fn find_folder_by_path(
        &self,
        _path: &str,
        _owner_id: uuid::Uuid,
    ) -> anyhow::Result<Option<rustshare_core::domain::Folder>> {
        Self::err()
    }

    pub async fn update_folder(
        &self,
        _folder: &rustshare_core::domain::Folder,
    ) -> anyhow::Result<()> {
        Self::err()
    }

    pub async fn delete_folder(&self, _id: uuid::Uuid) -> anyhow::Result<()> {
        Self::err()
    }

    pub async fn list_folders(
        &self,
        _parent_id: Option<uuid::Uuid>,
        _owner_id: uuid::Uuid,
    ) -> anyhow::Result<Vec<rustshare_core::domain::Folder>> {
        Self::err()
    }

    pub async fn find_descendant_folders(
        &self,
        _folder_id: uuid::Uuid,
    ) -> anyhow::Result<Vec<rustshare_core::domain::Folder>> {
        Self::err()
    }

    pub async fn create_share(&self, _share: &rustshare_core::domain::Share) -> anyhow::Result<()> {
        Self::err()
    }

    pub async fn get_share(
        &self,
        _id: uuid::Uuid,
    ) -> anyhow::Result<Option<rustshare_core::domain::Share>> {
        Self::err()
    }

    pub async fn get_share_by_token(
        &self,
        _token: &str,
    ) -> anyhow::Result<Option<rustshare_core::domain::Share>> {
        Self::err()
    }

    pub async fn get_file_shares(
        &self,
        _file_id: uuid::Uuid,
    ) -> anyhow::Result<Vec<rustshare_core::domain::Share>> {
        Self::err()
    }

    pub async fn get_folder_shares(
        &self,
        _folder_id: uuid::Uuid,
    ) -> anyhow::Result<Vec<rustshare_core::domain::Share>> {
        Self::err()
    }

    pub async fn revoke_share(&self, _share_id: uuid::Uuid) -> anyhow::Result<()> {
        Self::err()
    }

    pub async fn update_share(&self, _share: &rustshare_core::domain::Share) -> anyhow::Result<()> {
        Self::err()
    }

    pub async fn get_public_share_by_token(
        &self,
        _token: &str,
    ) -> anyhow::Result<Option<OwnedPublicShare>> {
        Self::err()
    }

    pub async fn log_share_access(&self, _entry: ShareAccessLogEntry) -> anyhow::Result<()> {
        Self::err()
    }

    pub async fn get_public_share_access_logs(
        &self,
        _share_id: uuid::Uuid,
        _limit: i64,
    ) -> anyhow::Result<Vec<PublicShareAccessLogEntry>> {
        Self::err()
    }
}
