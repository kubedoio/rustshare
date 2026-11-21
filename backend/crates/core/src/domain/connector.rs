//! Connector contracts are separate from Applications: they describe an
//! external source boundary, not a product module or executable plugin.

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use super::{PrincipalId, TenantId, WorkspaceId};

pub const CONNECTOR_API_VERSION: &str = "elembra.io/v1alpha1";
const MAX_RESOURCE_TYPE_LEN: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConnectorDescriptor {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: String,
    pub metadata: ConnectorMetadata,
    pub source: ConnectorSource,
    pub supports: ConnectorCapabilities,
    #[serde(default)]
    pub publishes: ConnectorPublications,
}

impl ConnectorDescriptor {
    pub fn validate(&self) -> Result<(), ConnectorDescriptorError> {
        if self.api_version != CONNECTOR_API_VERSION {
            return Err(ConnectorDescriptorError::ApiVersion);
        }
        if self.kind != "Connector" {
            return Err(ConnectorDescriptorError::Kind);
        }
        if !valid_connector_id(&self.metadata.id) {
            return Err(ConnectorDescriptorError::Id);
        }
        if self.metadata.name.trim().is_empty() || self.metadata.version.trim().is_empty() {
            return Err(ConnectorDescriptorError::Metadata);
        }
        if !valid_source_system(&self.source.system) {
            return Err(ConnectorDescriptorError::Source);
        }

        if !self.supports.operations.any() || !self.supports.modes.any() {
            return Err(ConnectorDescriptorError::EmptyCapabilities);
        }

        for mode in self.supports.modes.enabled() {
            let required = match mode {
                ConnectorMode::Reference
                | ConnectorMode::Mirror
                | ConnectorMode::Import
                | ConnectorMode::Archive => &[ConnectorOperation::Read][..],
                ConnectorMode::Export => &[ConnectorOperation::Write][..],
                ConnectorMode::BidirectionalSync => &[
                    ConnectorOperation::Read,
                    ConnectorOperation::Write,
                    ConnectorOperation::Delete,
                ][..],
            };
            if required
                .iter()
                .any(|operation| !self.supports.operations.supports(*operation))
            {
                return Err(ConnectorDescriptorError::MissingModeOperation(mode));
            }
        }

        if self
            .publishes
            .resources
            .iter()
            .any(|resource| !valid_resource_type(resource))
        {
            return Err(ConnectorDescriptorError::Publication);
        }
        let resources: HashSet<_> = self.publishes.resources.iter().collect();
        if resources.len() != self.publishes.resources.len() {
            return Err(ConnectorDescriptorError::DuplicatePublication);
        }
        Ok(())
    }
}

fn valid_connector_id(value: &str) -> bool {
    let parts: Vec<_> = value.split('.').collect();
    parts.len() >= 3
        && parts.iter().all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        })
        && parts.iter().all(|part| {
            part.as_bytes()[0].is_ascii_lowercase() && part.as_bytes()[part.len() - 1] != b'-'
        })
}

fn valid_source_system(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && value.as_bytes()[0].is_ascii_lowercase()
        && value.as_bytes()[value.len() - 1] != b'-'
}

fn valid_resource_type(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_RESOURCE_TYPE_LEN
        && value.split('.').all(|part| {
            !part.is_empty()
                && part.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'-' | b'_')
                })
        })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConnectorMetadata {
    pub id: String,
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ConnectorSource {
    pub system: String,
    pub authority: ConnectorAuthority,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ConnectorAuthority {
    External,
    Local,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ConnectorCapabilities {
    pub operations: ConnectorOperations,
    pub modes: ConnectorModes,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConnectorOperations {
    pub list: bool,
    pub read: bool,
    pub watch: bool,
    pub write: bool,
    pub delete: bool,
    pub versions: bool,
    pub external_sharing_metadata: bool,
}

impl ConnectorOperations {
    fn any(&self) -> bool {
        self.list
            || self.read
            || self.watch
            || self.write
            || self.delete
            || self.versions
            || self.external_sharing_metadata
    }

    fn supports(&self, operation: ConnectorOperation) -> bool {
        match operation {
            ConnectorOperation::Read => self.read,
            ConnectorOperation::Write => self.write,
            ConnectorOperation::Delete => self.delete,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConnectorModes {
    pub reference: bool,
    pub mirror: bool,
    pub import: bool,
    pub archive: bool,
    pub export: bool,
    pub bidirectional_sync: bool,
}

impl ConnectorModes {
    fn any(&self) -> bool {
        self.reference
            || self.mirror
            || self.import
            || self.archive
            || self.export
            || self.bidirectional_sync
    }

    fn enabled(&self) -> impl Iterator<Item = ConnectorMode> {
        [
            (self.reference, ConnectorMode::Reference),
            (self.mirror, ConnectorMode::Mirror),
            (self.import, ConnectorMode::Import),
            (self.archive, ConnectorMode::Archive),
            (self.export, ConnectorMode::Export),
            (self.bidirectional_sync, ConnectorMode::BidirectionalSync),
        ]
        .into_iter()
        .filter_map(|(enabled, mode)| enabled.then_some(mode))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConnectorOperation {
    Read,
    Write,
    Delete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum ConnectorMode {
    Reference,
    Mirror,
    Import,
    Archive,
    Export,
    BidirectionalSync,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ConnectorPublications {
    #[serde(default)]
    pub resources: Vec<String>,
    #[serde(default)]
    pub memory: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorHealth {
    Healthy,
    Degraded,
    AuthRequired,
    RateLimited,
    Paused,
    Failed,
}

/// Bounded error categories keep credentials and provider responses out of status payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorStatusError {
    Authentication,
    RateLimit,
    SourceUnavailable,
    InvalidContent,
    PolicyDenied,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorInstanceStatus {
    pub health: ConnectorHealth,
    pub last_success_at: Option<DateTime<Utc>>,
    pub queued_count: u64,
    pub retry_count: u64,
    pub last_error: Option<ConnectorStatusError>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(transparent)]
pub struct ConnectorInstanceId(Uuid);

impl ConnectorInstanceId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ConnectorInstanceId {
    fn default() -> Self {
        Self::new()
    }
}

impl From<Uuid> for ConnectorInstanceId {
    fn from(value: Uuid) -> Self {
        Self(value)
    }
}

impl From<ConnectorInstanceId> for Uuid {
    fn from(value: ConnectorInstanceId) -> Self {
        value.0
    }
}

impl std::fmt::Display for ConnectorInstanceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorInstance {
    pub id: ConnectorInstanceId,
    pub connector_id: String,
    pub tenant_id: TenantId,
    pub workspace_id: WorkspaceId,
    pub principal_id: PrincipalId,
    pub status: ConnectorInstanceStatus,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ConnectorDescriptorError {
    #[error("unsupported Connector API version")]
    ApiVersion,
    #[error("descriptor kind must be Connector")]
    Kind,
    #[error("invalid Connector identifier")]
    Id,
    #[error("Connector name and version must be non-empty")]
    Metadata,
    #[error("invalid source system identifier")]
    Source,
    #[error("at least one operation and mode must be declared")]
    EmptyCapabilities,
    #[error("mode {0:?} requires an undeclared operation")]
    MissingModeOperation(ConnectorMode),
    #[error("publication resource identifiers must be non-empty tokens")]
    Publication,
    #[error("publication resource is declared more than once")]
    DuplicatePublication,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn descriptor() -> ConnectorDescriptor {
        ConnectorDescriptor {
            api_version: CONNECTOR_API_VERSION.into(),
            kind: "Connector".into(),
            metadata: ConnectorMetadata {
                id: "io.elembra.connector.local-text".into(),
                name: "Local Text Connector".into(),
                version: "0.1.0".into(),
            },
            source: ConnectorSource {
                system: "local-filesystem".into(),
                authority: ConnectorAuthority::Local,
            },
            supports: ConnectorCapabilities {
                operations: ConnectorOperations {
                    read: true,
                    ..Default::default()
                },
                modes: ConnectorModes {
                    import: true,
                    ..Default::default()
                },
            },
            publishes: ConnectorPublications::default(),
        }
    }

    #[test]
    fn valid_descriptor_passes_capability_validation() {
        let value = descriptor();
        assert_eq!(value.validate(), Ok(()));
        let wire = serde_json::to_value(value).unwrap();
        assert_eq!(wire["supports"]["operations"]["read"], true);
        assert_eq!(wire["supports"]["modes"]["import"], true);
    }

    #[test]
    fn descriptor_rejects_invalid_identity_and_kind() {
        let mut value = descriptor();
        value.metadata.id = "Connector".into();
        assert_eq!(value.validate(), Err(ConnectorDescriptorError::Id));
        value = descriptor();
        value.kind = "Application".into();
        assert_eq!(value.validate(), Err(ConnectorDescriptorError::Kind));
        value = descriptor();
        value.api_version = "elembra.io/v2".into();
        assert_eq!(value.validate(), Err(ConnectorDescriptorError::ApiVersion));
        value = descriptor();
        value.source.system = "Local Filesystem".into();
        assert_eq!(value.validate(), Err(ConnectorDescriptorError::Source));
    }

    #[test]
    fn descriptor_rejects_empty_capabilities_and_duplicate_wire_fields() {
        let mut value = descriptor();
        value.supports.operations = ConnectorOperations::default();
        assert_eq!(
            value.validate(),
            Err(ConnectorDescriptorError::EmptyCapabilities)
        );
        let duplicate = r#"{"apiVersion":"elembra.io/v1alpha1","kind":"Connector","metadata":{"id":"io.elembra.connector.local-text","name":"Local Text","version":"0.1.0"},"source":{"system":"local-filesystem","authority":"local"},"supports":{"operations":{"list":false,"read":true,"read":false,"watch":false,"write":false,"delete":false,"versions":false,"externalSharingMetadata":false},"modes":{"reference":false,"mirror":false,"import":true,"archive":false,"export":false,"bidirectionalSync":false}}}"#;
        assert!(serde_json::from_str::<ConnectorDescriptor>(duplicate).is_err());
    }

    #[test]
    fn descriptor_rejects_modes_that_overclaim_operations() {
        let mut value = descriptor();
        value.supports.operations = ConnectorOperations {
            write: true,
            ..Default::default()
        };
        assert_eq!(
            value.validate(),
            Err(ConnectorDescriptorError::MissingModeOperation(
                ConnectorMode::Import
            ))
        );
        value.supports.modes = ConnectorModes {
            bidirectional_sync: true,
            ..Default::default()
        };
        value.supports.operations = ConnectorOperations {
            read: true,
            write: true,
            ..Default::default()
        };
        assert_eq!(
            value.validate(),
            Err(ConnectorDescriptorError::MissingModeOperation(
                ConnectorMode::BidirectionalSync
            ))
        );
    }

    #[test]
    fn descriptor_rejects_unknown_wire_values() {
        let mut value = serde_json::to_value(descriptor()).unwrap();
        value["supports"]["operations"]["execute"] = true.into();
        assert!(serde_json::from_value::<ConnectorDescriptor>(value).is_err());
    }

    #[test]
    fn descriptor_rejects_invalid_and_duplicate_publications() {
        let mut value = descriptor();
        value.publishes.resources = vec!["drive_file".into()];
        assert_eq!(value.validate(), Ok(()));
        value.publishes.resources = vec!["drive/file".into()];
        assert_eq!(value.validate(), Err(ConnectorDescriptorError::Publication));
        value.publishes.resources = vec!["r".repeat(MAX_RESOURCE_TYPE_LEN + 1)];
        assert_eq!(value.validate(), Err(ConnectorDescriptorError::Publication));
        value.publishes.resources = vec!["drive file".into()];
        assert_eq!(value.validate(), Err(ConnectorDescriptorError::Publication));
        value.publishes.resources = vec!["drive_file".into(), "drive_file".into()];
        assert_eq!(
            value.validate(),
            Err(ConnectorDescriptorError::DuplicatePublication)
        );
    }

    #[test]
    fn instance_id_and_status_round_trip_without_free_form_errors() {
        let id = ConnectorInstanceId::new();
        assert_eq!(
            serde_json::to_value(id).unwrap(),
            serde_json::Value::String(id.to_string())
        );
        let decoded: ConnectorInstanceId =
            serde_json::from_str(&serde_json::to_string(&id).unwrap()).unwrap();
        assert_eq!(decoded, id);

        let status = ConnectorInstanceStatus {
            health: ConnectorHealth::AuthRequired,
            last_success_at: None,
            queued_count: 2,
            retry_count: 1,
            last_error: Some(ConnectorStatusError::Authentication),
        };
        assert_eq!(
            serde_json::from_str::<ConnectorInstanceStatus>(
                &serde_json::to_string(&status).unwrap()
            )
            .unwrap(),
            status
        );

        for (health, wire_value) in [
            (ConnectorHealth::Healthy, "healthy"),
            (ConnectorHealth::Degraded, "degraded"),
            (ConnectorHealth::AuthRequired, "auth_required"),
            (ConnectorHealth::RateLimited, "rate_limited"),
            (ConnectorHealth::Paused, "paused"),
            (ConnectorHealth::Failed, "failed"),
        ] {
            assert_eq!(serde_json::to_value(health).unwrap(), wire_value);
        }
    }
}
