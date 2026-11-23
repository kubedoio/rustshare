//! Scoped Files snapshots consumed by the independently running Memory worker.

use rustshare_core::services::IndexAclProjection;
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// Connector identity kept separately from the current Files version/content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectorSource {
    pub instance_id: Uuid,
    pub resource_ref: String,
    pub revision: String,
    pub relative_path: String,
    pub observed_modified_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Files metadata and owner-authorized content prepared by the Files service.
///
/// The Memory worker receives this value only over the authenticated internal
/// Files contract. It never supplies or serializes a user `PrincipalContext`.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct FilesIndexSnapshot {
    pub tenant_id: Uuid,
    pub file_id: Uuid,
    pub current_version: String,
    pub current_version_number: i64,
    pub owner_id: Uuid,
    pub name: String,
    pub path: String,
    pub mime_type: String,
    pub connector: Option<ConnectorSource>,
    pub acl: IndexAclProjection,
    pub content: String,
}

/// Result of resolving one Connector-backed Files resource for Memory.
impl fmt::Debug for FilesIndexSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FilesIndexSnapshot")
            .field("tenant_id", &self.tenant_id)
            .field("file_id", &self.file_id)
            .field("current_version", &self.current_version)
            .field("current_version_number", &self.current_version_number)
            .field("owner_id", &self.owner_id)
            .field("connector", &self.connector)
            .field("content", &"[REDACTED]")
            .finish()
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum FilesProjectionSnapshot {
    /// The authoritative Files service no longer has this resource.
    Deleted { tenant_id: Uuid, file_id: Uuid },
    /// The resource is current but is not eligible for the Memory note index.
    Ignored { tenant_id: Uuid, file_id: Uuid },
    /// The source is live and was authorized by Files for projection.
    Available(FilesIndexSnapshot),
}

/// One bounded source-authoritative reconciliation page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FilesReconcilePage {
    pub snapshots: Vec<FilesProjectionSnapshot>,
    pub next_cursor: Option<Uuid>,
}

/// One bounded page of tenants with Connector-backed Files mappings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilesReconcileTenantPage {
    pub tenant_ids: Vec<Uuid>,
    pub next_cursor: Option<Uuid>,
}

impl fmt::Debug for FilesProjectionSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Deleted { tenant_id, file_id } => formatter
                .debug_struct("Deleted")
                .field("tenant_id", tenant_id)
                .field("file_id", file_id)
                .finish(),
            Self::Ignored { tenant_id, file_id } => formatter
                .debug_struct("Ignored")
                .field("tenant_id", tenant_id)
                .field("file_id", file_id)
                .finish(),
            Self::Available(snapshot) => {
                formatter.debug_tuple("Available").field(snapshot).finish()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustshare_core::services::ai::{EmbeddingPolicy, IndexPrincipal, IndexVisibility};

    #[test]
    fn snapshot_debug_redacts_content_but_wire_format_keeps_it_separate() {
        let snapshot = FilesIndexSnapshot {
            tenant_id: Uuid::new_v4(),
            file_id: Uuid::new_v4(),
            current_version: "sha256:version".into(),
            current_version_number: 1,
            owner_id: Uuid::new_v4(),
            name: "note.md".into(),
            path: "/Workspace/note.md".into(),
            mime_type: "text/markdown".into(),
            connector: None,
            acl: IndexAclProjection {
                tenant_id: Uuid::new_v4(),
                workspace_id: Uuid::new_v4(),
                object_id: Uuid::new_v4(),
                source_folder_id: None,
                owner_id: Uuid::new_v4(),
                read_principals: vec![IndexPrincipal::Owner(Uuid::new_v4())],
                visibility: IndexVisibility::Private,
                acl_hash: "acl".into(),
                acl_version: 1,
                embedding_policy: EmbeddingPolicy::Allowed,
            },
            content: "sensitive source text".into(),
        };

        assert!(!format!("{snapshot:?}").contains("sensitive source text"));
        let wire = serde_json::to_value(snapshot).unwrap();
        assert_eq!(wire["content"], "sensitive source text");
        assert!(wire.get("source_metadata").is_none());
    }
}
