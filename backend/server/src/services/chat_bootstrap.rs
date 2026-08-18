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

use rustshare_resource_auth::BuzzAuthorityError;

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
    #[error("Buzz rejected Elembra's service identity (HTTP 401 Unauthorized). Verify that the public key corresponding to `RUSTSHARE_CHAT_BRIDGE_SECRET_KEY` / `BUZZ_SERVICE_SK` is configured in the relay's `RELAY_TRUSTED_SERVICE_PUBKEYS`, then recreate the backend and relay containers.")]
    ServiceIdentityRejected,
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
            .map_err(|error| map_gateway_error(&self.bootstrap_relay_url, error))?;

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
            tracing::warn!(
                relay_url = %self.bootstrap_relay_url,
                relay_community = %identity.community_id,
                mapped_community = %existing.community_id,
                "chat bootstrap: mapping conflict between relay and existing workspace mapping"
            );
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
                    tracing::warn!(
                        relay_url = %self.bootstrap_relay_url,
                        relay_community = %identity.community_id,
                        mapped_community = %existing.community_id,
                        "chat bootstrap: mapping conflict detected after idempotent insert race"
                    );
                    Err(ChatBootstrapError::CommunityMismatch {
                        relay: identity.community_id,
                        mapped: existing.community_id,
                    })
                }
            }
            Err(ProvisionMappingError::CommunityInUse) => {
                tracing::warn!(
                    relay_url = %self.bootstrap_relay_url,
                    community_id = %identity.community_id,
                    "chat bootstrap: community already mapped to another workspace"
                );
                Err(ChatBootstrapError::CommunityInUse {
                    community_id: identity.community_id,
                })
            }
            Err(error) => Err(ChatBootstrapError::Storage(error)),
        }
    }
}

/// Heuristic to detect stale relay responses inside [`BuzzAuthorityError::InvalidResponse`].
/// The gateway reports freshness failures using the string "stale" or the
/// `evaluated_at` field name; either is safe, operator-useful context.
fn is_stale_response(msg: &str) -> bool {
    msg.contains("stale") || msg.contains("evaluated_at")
}

/// Map a Buzz gateway error into a [`ChatBootstrapError`] and log safe context
/// that lets operators distinguish the main failure modes without exposing
/// secrets, private keys, or raw auth headers.
fn map_gateway_error(relay_url: &str, error: BuzzAuthorityError) -> ChatBootstrapError {
    match error {
        BuzzAuthorityError::Unauthorized => {
            tracing::warn!(
                relay_url = %relay_url,
                "chat bootstrap 401: Buzz rejected Elembra's service identity; verify RELAY_TRUSTED_SERVICE_PUBKEYS"
            );
            ChatBootstrapError::ServiceIdentityRejected
        }
        BuzzAuthorityError::Transport(_) => {
            tracing::warn!(
                relay_url = %relay_url,
                error = %error,
                "chat bootstrap: relay unreachable or transport failure"
            );
            ChatBootstrapError::Discovery(error.to_string())
        }
        BuzzAuthorityError::InvalidResponse(ref msg) if is_stale_response(msg) => {
            tracing::warn!(
                relay_url = %relay_url,
                error = %error,
                "chat bootstrap: stale relay response"
            );
            ChatBootstrapError::Discovery(error.to_string())
        }
        BuzzAuthorityError::InvalidResponse(_) => {
            tracing::warn!(
                relay_url = %relay_url,
                error = %error,
                "chat bootstrap: invalid relay response or signature"
            );
            ChatBootstrapError::Discovery(error.to_string())
        }
        BuzzAuthorityError::Config(_) => {
            tracing::warn!(
                relay_url = %relay_url,
                error = %error,
                "chat bootstrap: relay configuration error"
            );
            ChatBootstrapError::Discovery(error.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unauthorized_maps_to_service_identity_rejected() {
        let error = map_gateway_error("wss://relay.example.test", BuzzAuthorityError::Unauthorized);
        assert!(
            matches!(error, ChatBootstrapError::ServiceIdentityRejected),
            "expected ServiceIdentityRejected, got {error:?}"
        );
        let message = error.to_string();
        assert!(
            message.contains("HTTP 401 Unauthorized"),
            "diagnostic must mention HTTP 401 Unauthorized: {message:?}"
        );
        assert!(
            message.contains("RELAY_TRUSTED_SERVICE_PUBKEYS"),
            "diagnostic must mention RELAY_TRUSTED_SERVICE_PUBKEYS: {message:?}"
        );
        assert!(
            message.contains("RUSTSHARE_CHAT_BRIDGE_SECRET_KEY"),
            "diagnostic must mention RUSTSHARE_CHAT_BRIDGE_SECRET_KEY: {message:?}"
        );
    }

    #[test]
    fn transport_config_and_invalid_response_map_to_discovery() {
        for error in [
            BuzzAuthorityError::Transport("connection refused".to_string()),
            BuzzAuthorityError::InvalidResponse(
                "response signature verification failed".to_string(),
            ),
            BuzzAuthorityError::Config("bad URL".to_string()),
        ] {
            let mapped = map_gateway_error("wss://relay.example.test", error);
            assert!(
                matches!(mapped, ChatBootstrapError::Discovery(_)),
                "expected Discovery, got {mapped:?}"
            );
        }
    }

    #[test]
    fn stale_response_maps_to_discovery() {
        for msg in [
            "batch evaluated_at is stale",
            "response evaluated_at is 120s from the client clock (max 60s)",
        ] {
            let mapped = map_gateway_error(
                "wss://relay.example.test",
                BuzzAuthorityError::InvalidResponse(msg.to_string()),
            );
            assert!(
                matches!(mapped, ChatBootstrapError::Discovery(_)),
                "expected Discovery for stale response, got {mapped:?}"
            );
        }
    }
}
