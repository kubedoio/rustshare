//! Vault sync domain types for RustShare Vault Sync.
//!
//! Obsidian is a trademark of Dynalist Inc. RustShare is not affiliated with,
//! endorsed by, or sponsored by Obsidian.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Adapter types supported for vault sync.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, utoipa::ToSchema,
)]
#[sqlx(type_name = "VARCHAR", rename_all = "snake_case")]
pub enum VaultAdapter {
    /// Obsidian vault adapter.
    ObsidianVault,
}

impl std::fmt::Display for VaultAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VaultAdapter::ObsidianVault => write!(f, "obsidian_vault"),
        }
    }
}

impl std::str::FromStr for VaultAdapter {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "obsidian_vault" => Ok(VaultAdapter::ObsidianVault),
            _ => Err(format!("Invalid vault adapter: {}", s)),
        }
    }
}

/// Write policy controlling who may modify a vault's files.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, utoipa::ToSchema,
)]
#[sqlx(type_name = "VARCHAR", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum VaultWritePolicy {
    /// No WebUI writes. This is the default.
    ReadOnly,
    /// WebUI may edit eligible files.
    WebEditingEnabled,
    /// Only sync clients may write; WebUI remains read-only.
    SyncClientOnly,
}

impl std::fmt::Display for VaultWritePolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VaultWritePolicy::ReadOnly => write!(f, "read_only"),
            VaultWritePolicy::WebEditingEnabled => write!(f, "web_editing_enabled"),
            VaultWritePolicy::SyncClientOnly => write!(f, "sync_client_only"),
        }
    }
}

impl std::str::FromStr for VaultWritePolicy {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "read_only" => Ok(VaultWritePolicy::ReadOnly),
            "web_editing_enabled" => Ok(VaultWritePolicy::WebEditingEnabled),
            "sync_client_only" => Ok(VaultWritePolicy::SyncClientOnly),
            _ => Err(format!("Invalid vault write policy: {}", s)),
        }
    }
}

/// A syncable vault container.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct Vault {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub owner_user_id: Uuid,
    pub name: String,
    pub adapter: VaultAdapter,
    pub root_path: Option<String>,
    pub write_policy: VaultWritePolicy,
    pub server_rev: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Metadata for a file inside a vault.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct VaultFile {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub vault_id: Uuid,
    pub relative_path: String,
    pub content_type: Option<String>,
    pub sha256: Option<String>,
    pub size: Option<i64>,
    pub server_rev: i64,
    pub mtime_client: Option<i64>,
    pub mtime_server: DateTime<Utc>,
    pub deleted: bool,
    pub deleted_at: Option<DateTime<Utc>>,
    pub last_writer_device_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A device authorized to sync a vault.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct VaultDevice {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub user_id: Uuid,
    pub vault_id: Option<Uuid>,
    pub device_name: String,
    pub client_type: String,
    pub client_version: Option<String>,
    pub last_sync_rev: Option<i64>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
}

/// Request to create a new vault.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateVaultRequest {
    pub name: String,
    pub adapter: VaultAdapter,
    pub client_vault_id: Option<String>,
    pub device_id: String,
}

/// Request to upload a file into a vault.
#[derive(Debug, Clone, PartialEq)]
pub struct UploadVaultFileRequest {
    pub vault_id: Uuid,
    pub relative_path: String,
    pub content_type: Option<String>,
    pub sha256: String,
    pub size: i64,
    pub base_server_rev: i64,
    pub device_id: String,
    pub content: bytes::Bytes,
}

/// Request to delete a file from a vault.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DeleteVaultFileRequest {
    pub vault_id: Uuid,
    pub relative_path: String,
    pub base_server_rev: i64,
    pub device_id: String,
}

/// Request to rename a file within a vault.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct RenameVaultFileRequest {
    #[serde(default)]
    pub vault_id: Uuid,
    pub old_path: String,
    pub new_path: String,
    #[serde(default)]
    pub base_server_rev: i64,
    #[serde(default)]
    pub device_id: String,
}

/// A manifest representing the current state of a vault.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct VaultManifest {
    pub vault_id: Uuid,
    pub adapter: VaultAdapter,
    pub server_rev: i64,
    pub generated_at: DateTime<Utc>,
    pub files: Vec<VaultManifestEntry>,
}

/// Result of a manifest query, including truncation status.
#[derive(Debug, Clone, PartialEq)]
pub struct VaultManifestResult {
    pub manifest: VaultManifest,
    pub truncated: bool,
}

/// An entry in a vault manifest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct VaultManifestEntry {
    pub path: String,
    pub sha256: Option<String>,
    pub size: Option<i64>,
    pub content_type: Option<String>,
    pub server_rev: i64,
    pub mtime_server: DateTime<Utc>,
    pub deleted: bool,
    pub deleted_at: Option<DateTime<Utc>>,
}

/// Request to save vault file content from the WebUI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SaveVaultFileContentRequest {
    pub content: String,
    pub expected_revision: i64,
}

/// Response when loading vault file content for the WebUI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct VaultFileContentResponse {
    pub path: String,
    pub content: String,
    pub server_rev: i64,
    pub content_type: Option<String>,
    pub size: i64,
}

/// Response after saving vault file content from the WebUI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct VaultFileContentSavedResponse {
    pub path: String,
    pub server_rev: i64,
    pub updated_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_vault_adapter_display() {
        assert_eq!(VaultAdapter::ObsidianVault.to_string(), "obsidian_vault");
    }

    #[test]
    fn test_vault_adapter_from_str() {
        assert_eq!(
            VaultAdapter::from_str("obsidian_vault").unwrap(),
            VaultAdapter::ObsidianVault
        );
        assert!(VaultAdapter::from_str("unknown").is_err());
    }

    #[test]
    fn test_vault_adapter_roundtrip() {
        let adapter = VaultAdapter::ObsidianVault;
        let s = adapter.to_string();
        let parsed = VaultAdapter::from_str(&s).unwrap();
        assert_eq!(adapter, parsed);
    }

    #[test]
    fn test_vault_write_policy_display() {
        assert_eq!(VaultWritePolicy::ReadOnly.to_string(), "read_only");
        assert_eq!(VaultWritePolicy::WebEditingEnabled.to_string(), "web_editing_enabled");
        assert_eq!(VaultWritePolicy::SyncClientOnly.to_string(), "sync_client_only");
    }

    #[test]
    fn test_vault_write_policy_from_str() {
        assert_eq!(VaultWritePolicy::from_str("read_only").unwrap(), VaultWritePolicy::ReadOnly);
        assert_eq!(
            VaultWritePolicy::from_str("web_editing_enabled").unwrap(),
            VaultWritePolicy::WebEditingEnabled
        );
        assert!(VaultWritePolicy::from_str("unknown").is_err());
    }

    #[test]
    fn test_vault_write_policy_roundtrip() {
        let policy = VaultWritePolicy::WebEditingEnabled;
        let s = policy.to_string();
        let parsed = VaultWritePolicy::from_str(&s).unwrap();
        assert_eq!(policy, parsed);
    }
}
