//! Zero-config Chat bootstrap (ADR-0036): discover the deployment Buzz
//! community over the authoritative relay and create the explicit
//! Workspace↔Community mapping, idempotently and without ever overwriting an
//! existing mapping.

use std::sync::Arc;

use rustshare_core::domain::{TenantId, WorkspaceId};
use rustshare_resource_auth::chat_identity::WorkspaceCommunityMapping;
use rustshare_storage::chat_identity::{
    ChatIdentityStore, ProvisionMappingError, ProvisionMappingOutcome,
};

use crate::buzz_gateway::BuzzGatewayClient;

/// Result of a provisioning attempt — both variants are idempotent successes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProvisionOutcome {
    /// This call created the mapping row.
    Inserted {
        community_id: String,
        relay_url: String,
        relay_pubkey: String,
    },
    /// A mapping already existed for the workspace and matches the discovered
    /// community (or a concurrent provision won with the same community).
    AlreadyConfigured {
        community_id: String,
        relay_url: String,
        relay_pubkey: Option<String>,
    },
}

/// Provisioning failures. `CommunityInUse`/`CommunityMismatch` are expected
/// admin-facing conflicts (HTTP 409); `Discovery` means the relay was
/// reachable-but-invalid or unreachable and Chat stays safely unconfigured.
#[derive(Debug, thiserror::Error)]
pub enum ChatBootstrapError {
    #[error("relay discovery failed: {0}")]
    Discovery(String),
    #[error("community {community_id} is already mapped to another workspace")]
    CommunityInUse { community_id: String },
    #[error(
        "community mismatch: the relay identifies community {relay}, but the workspace is mapped to {mapped}"
    )]
    CommunityMismatch { relay: String, mapped: String },
    #[error(transparent)]
    Storage(#[from] ProvisionMappingError),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

/// Bootstrap service. Constructed only in buzz authority mode (see
/// `bootstrap.rs`); in local/development mode it is `None` and provisioning
/// is unavailable.
pub struct ChatBootstrapService {
    gateway: Arc<BuzzGatewayClient>,
    store: Arc<ChatIdentityStore>,
    bootstrap_relay_url: String,
}

impl ChatBootstrapService {
    pub fn new(
        gateway: Arc<BuzzGatewayClient>,
        store: Arc<ChatIdentityStore>,
        bootstrap_relay_url: String,
    ) -> Self {
        Self {
            gateway,
            store,
            bootstrap_relay_url,
        }
    }

    /// Discover → verify → map. Never overwrites an existing mapping; never
    /// creates a mapping from a read request — only this service (called from
    /// enable-Chat in auto mode and from the admin provision endpoint) writes.
    pub async fn provision(
        &self,
        tenant_id: TenantId,
        workspace_id: WorkspaceId,
    ) -> Result<ProvisionOutcome, ChatBootstrapError> {
        // 1. Discovery (read-only): the relay is the authority on community
        //    identity and its own pubkey (signature-verified, see gateway).
        let identity = self
            .gateway
            .community_identity(&self.bootstrap_relay_url)
            .await
            .map_err(|error| ChatBootstrapError::Discovery(error.to_string()))?;

        // 2. Existing mapping: verify, never overwrite. An existing row — even
        //    an inactive one — is treated as configured; there is no
        //    deactivation path today, so this cannot currently produce a
        //    silent unconfigured state.
        if let Some(existing) = self
            .store
            .mapping(tenant_id, workspace_id)
            .await
            .map_err(|error| ChatBootstrapError::Internal(anyhow::anyhow!(error.to_string())))?
        {
            if existing.community_id == identity.community_id {
                return Ok(ProvisionOutcome::AlreadyConfigured {
                    community_id: existing.community_id,
                    relay_url: existing.relay_url,
                    relay_pubkey: existing.relay_pubkey,
                });
            }
            return Err(ChatBootstrapError::CommunityMismatch {
                relay: identity.community_id,
                mapped: existing.community_id,
            });
        }

        // 3. Idempotent, race-safe insert with a pinned relay pubkey.
        let mapping = WorkspaceCommunityMapping {
            tenant_id,
            workspace_id,
            community_id: identity.community_id.clone(),
            relay_url: self.bootstrap_relay_url.clone(),
            relay_pubkey: Some(identity.relay_pubkey.clone()),
            active: true,
        };
        match self.store.provision_mapping(&mapping).await {
            Ok(ProvisionMappingOutcome::Inserted) => Ok(ProvisionOutcome::Inserted {
                community_id: identity.community_id,
                relay_url: self.bootstrap_relay_url.clone(),
                relay_pubkey: identity.relay_pubkey,
            }),
            Ok(ProvisionMappingOutcome::AlreadyExists(existing)) => {
                if existing.community_id == identity.community_id {
                    Ok(ProvisionOutcome::AlreadyConfigured {
                        community_id: existing.community_id,
                        relay_url: existing.relay_url,
                        relay_pubkey: existing.relay_pubkey,
                    })
                } else {
                    Err(ChatBootstrapError::CommunityMismatch {
                        relay: identity.community_id,
                        mapped: existing.community_id,
                    })
                }
            }
            Err(ProvisionMappingError::CommunityInUse) => Err(ChatBootstrapError::CommunityInUse {
                community_id: identity.community_id,
            }),
            Err(error) => Err(ChatBootstrapError::Storage(error)),
        }
    }
}
