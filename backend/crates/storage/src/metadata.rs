//! DEPRECATED: This module is kept for type compatibility only.
//! All functionality has been moved to metadata_v2.

use uuid::Uuid;
use chrono::{DateTime, Utc};

/// DEPRECATED: Use audit log instead
pub struct ShareAccessLogEntry {
    pub share_id: Uuid,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub action: String,
    pub success: bool,
    pub actor_type: Option<String>,
    pub actor_label: Option<String>,
    pub share_session_id: Option<Uuid>,
    pub share_session_subject: Option<String>,
}

/// DEPRECATED: Use audit log instead  
pub struct UserSecurityEventRecord<'a> {
    pub user_id: Uuid,
    pub event_type: &'a str,
    pub description: &'a str,
    pub ip_address: Option<&'a str>,
    pub user_agent: Option<&'a str>,
    pub session_id: Option<Uuid>,
}

/// DEPRECATED: Use AuditLogEntry instead
pub struct UserSecurityEvent {
    pub id: Uuid,
    pub event_type: String,
    pub description: String,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub session_id: Option<Uuid>,
    pub occurred_at: DateTime<Utc>,
}

/// DEPRECATED: Stub implementation
pub struct MetadataStore;

impl MetadataStore {
    pub fn new(_: ()) -> Self {
        Self
    }

    async fn err<T>() -> anyhow::Result<T> {
        Err(anyhow::anyhow!("MetadataStore has been removed. Use metadata_v2 instead."))
    }

    pub async fn create_user(&self, _: &rustshare_core::domain::User) -> anyhow::Result<()> {
        Self::err().await
    }

    pub async fn create_user_security_event(&self, _: UserSecurityEventRecord<'_>) -> anyhow::Result<()> {
        // Silently ignore - security events now use audit log
        Ok(())
    }

    pub async fn log_share_access(&self, _: ShareAccessLogEntry) -> anyhow::Result<()> {
        // Silently ignore - share access now uses audit log
        Ok(())
    }

    pub async fn has_users(&self) -> anyhow::Result<bool> {
        Self::err().await
    }

    pub async fn find_user_by_email(&self, _: &str) -> anyhow::Result<Option<rustshare_core::domain::User>> {
        Self::err().await
    }

    pub async fn create_user_session(&self, _: &rustshare_core::domain::UserSession) -> anyhow::Result<()> {
        Self::err().await
    }

    pub async fn find_user_session_by_token_hash(&self, _: &str) -> anyhow::Result<Option<rustshare_core::domain::UserSession>> {
        Self::err().await
    }

    pub async fn touch_user_session(&self, _: Uuid) -> anyhow::Result<()> {
        Self::err().await
    }

    pub async fn delete_user_session_by_token_hash(&self, _: &str) -> anyhow::Result<()> {
        Self::err().await
    }

    pub async fn delete_user_session_by_id(&self, _: Uuid, _: Uuid) -> anyhow::Result<()> {
        Self::err().await
    }

    pub async fn list_user_sessions(&self, _: Uuid) -> anyhow::Result<Vec<rustshare_core::domain::UserSession>> {
        Self::err().await
    }

    pub async fn list_user_security_events(&self, _: Uuid, _: i64) -> anyhow::Result<Vec<UserSecurityEvent>> {
        Self::err().await
    }

    pub async fn list_files(&self, _: Option<Uuid>, _: Uuid) -> anyhow::Result<Vec<rustshare_core::domain::File>> {
        Self::err().await
    }

    pub async fn list_folders(&self, _: Option<Uuid>, _: Uuid) -> anyhow::Result<Vec<rustshare_core::domain::Folder>> {
        Self::err().await
    }

    pub async fn increment_share_access(&self, _: Uuid) -> anyhow::Result<()> {
        // Silently ignore - use audit log instead
        Ok(())
    }

    pub async fn get_share_by_token(&self, _: &str) -> anyhow::Result<Option<rustshare_core::domain::Share>> {
        Self::err().await
    }

    pub async fn find_file_by_id(&self, _: Uuid) -> anyhow::Result<Option<rustshare_core::domain::File>> {
        Self::err().await
    }

    pub async fn find_descendant_folders(&self, _: Uuid) -> anyhow::Result<Vec<rustshare_core::domain::Folder>> {
        Self::err().await
    }

    pub async fn find_folder_by_id(&self, _: Uuid) -> anyhow::Result<Option<rustshare_core::domain::Folder>> {
        Self::err().await
    }

    pub async fn create_oidc_login_state(&self, _: &rustshare_core::domain::OidcLoginState) -> anyhow::Result<()> {
        Self::err().await
    }

    pub async fn find_oidc_login_state(&self, _: &str) -> anyhow::Result<Option<rustshare_core::domain::OidcLoginState>> {
        Self::err().await
    }

    pub async fn delete_oidc_login_state(&self, _: &str) -> anyhow::Result<()> {
        Self::err().await
    }
}

/// DEPRECATED: Stub implementation
pub struct EventStore;

impl EventStore {
    pub fn new(_: ()) -> Self {
        Self
    }

    pub async fn append(&self, _: &rustshare_core::events::Event, _: &rustshare_core::events::EventBroadcaster) -> anyhow::Result<()> {
        // Events are now handled by EventLogStore
        Ok(())
    }

    pub async fn get_events_since(
        &self,
        _: Uuid,
        _: Option<Uuid>,
        _: i64,
    ) -> anyhow::Result<Vec<rustshare_core::events::Event>> {
        // EventStore is deprecated - use EventLogStore from metadata_v2
        Err(anyhow::anyhow!("EventStore has been deprecated. Use metadata_v2::EventLogStore instead."))
    }
}
