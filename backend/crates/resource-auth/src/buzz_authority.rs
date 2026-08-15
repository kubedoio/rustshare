//! Buzz source-authorization contract and the coarse local fallback gate.
//!
//! Elembra never holds a human's Buzz signing key and never re-derives channel
//! membership itself. Channel/message visibility is decided by the community's
//! authoritative Buzz relay; this module defines the query shape and the
//! decision surface used by the Elembra-side authority client. The relay
//! remains the final authority and every failure mode fails closed.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures_util::{stream, StreamExt};
use rustshare_core::domain::TenantId;
use serde::{Deserialize, Serialize};

/// Maximum number of [`BuzzAuthority::can_read`] calls the default
/// [`BuzzAuthority::can_read_batch`] keeps in flight at once.
const AUTHORIZATION_BATCH_CONCURRENCY: usize = 16;

/// Buzz channel classification, wire-identical to
/// `rustshare_memory::event::ChatChannelKind` (`workspace|dm|private|excluded`).
/// The Memory crate must not leak into resource-auth, so this is a local copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuzzChannelKind {
    Workspace,
    Dm,
    Private,
    Excluded,
}

/// Ask whether `pubkey` may currently read `channel_id`/`message_id` in the
/// mapped community, as decided by the community's current Buzz authority.
#[derive(Debug, Clone)]
pub struct BuzzReadRequest {
    pub tenant_id: TenantId,
    pub community_id: String,
    pub relay_url: String,
    pub relay_pubkey: Option<String>,
    pub channel_id: String,
    pub channel_kind: BuzzChannelKind,
    pub message_id: Option<String>,
    pub pubkey: String,
    pub event_created_at: DateTime<Utc>,
}

/// A channel from the relay's authoritative channel registry
/// (`GET /api/v1/relay/channels`): one the queried pubkey may currently read —
/// member channels (including private ones) plus open channels — with the
/// `member` flag stating active membership. Wire-identical to the relay's
/// serialized channel shape (`channel_type` uses the relay's vocabulary
/// `stream|forum|dm|workflow`, `visibility` is `open|private`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuzzChannelInfo {
    pub channel_id: String,
    pub name: String,
    pub channel_type: String,
    pub visibility: String,
    pub member: bool,
}

/// Final read decision from the current Buzz authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuzzReadDecision {
    Allow,
    Deny,
    NotFound,
}

impl BuzzReadDecision {
    pub fn is_allow(&self) -> bool {
        matches!(self, Self::Allow)
    }
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum BuzzAuthorityError {
    #[error("transport error: {0}")]
    Transport(String),
    #[error("upstream refused or rejected the request")]
    Unauthorized,
    #[error("invalid or unverifiable upstream response: {0}")]
    InvalidResponse(String),
    #[error("authority configuration error: {0}")]
    Config(String),
}

/// Source-authorization contract implemented by the authoritative Buzz
/// authority for a community. The final decision comes from CURRENT Buzz
/// state; any failure must fail closed (see the v1alpha1 upstream spec).
#[async_trait]
pub trait BuzzAuthority: Send + Sync {
    async fn can_read(&self, req: &BuzzReadRequest)
        -> Result<BuzzReadDecision, BuzzAuthorityError>;

    /// Evaluate a batch of read requests in bounded parallel fan-out,
    /// preserving input order.
    ///
    /// The default implementation calls [`Self::can_read`] for every request,
    /// keeping at most [`AUTHORIZATION_BATCH_CONCURRENCY`] in flight at once,
    /// and re-orders the results back to the input order. One result is
    /// returned per input request: an error for one request is reported at
    /// its own position and never aborts the other requests — each item
    /// fails closed individually.
    async fn can_read_batch(
        &self,
        reqs: &[BuzzReadRequest],
    ) -> Vec<Result<BuzzReadDecision, BuzzAuthorityError>> {
        // Fan out over owned indices (not per-item references): the closure's
        // returned future captures `reqs` from the outer scope, so it stays
        // valid for any stream item lifetime.
        let mut decisions = stream::iter(0..reqs.len())
            .map(|index| async move {
                let decision = self.can_read(&reqs[index]).await;
                (index, decision)
            })
            .buffer_unordered(AUTHORIZATION_BATCH_CONCURRENCY)
            .collect::<Vec<_>>()
            .await;
        decisions.sort_unstable_by_key(|(index, _)| *index);
        decisions
            .into_iter()
            .map(|(_, decision)| decision)
            .collect()
    }
}

/// Coarse community-level gate preserving today's behavior when no upstream
/// Buzz authority is configured: workspace channels are allowed, everything
/// else is denied.
///
/// This is NOT final per-channel authorization — an upstream
/// [`BuzzAuthority`] remains authoritative and must fail closed.
pub struct LocalFallbackAuthority;

#[async_trait]
impl BuzzAuthority for LocalFallbackAuthority {
    async fn can_read(
        &self,
        req: &BuzzReadRequest,
    ) -> Result<BuzzReadDecision, BuzzAuthorityError> {
        Ok(match req.channel_kind {
            BuzzChannelKind::Workspace => BuzzReadDecision::Allow,
            _ => BuzzReadDecision::Deny,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tokio::sync::{oneshot, Notify};
    use uuid::Uuid;

    fn read_request(channel_kind: BuzzChannelKind) -> BuzzReadRequest {
        BuzzReadRequest {
            tenant_id: TenantId(Uuid::new_v4()),
            community_id: "c1".into(),
            relay_url: "wss://chat.example.test".into(),
            relay_pubkey: None,
            channel_id: "ch1".into(),
            channel_kind,
            message_id: None,
            pubkey: "a".repeat(64),
            event_created_at: Utc::now(),
        }
    }

    #[test]
    fn channel_kind_serde_round_trip_is_snake_case() {
        for (kind, wire) in [
            (BuzzChannelKind::Workspace, "workspace"),
            (BuzzChannelKind::Dm, "dm"),
            (BuzzChannelKind::Private, "private"),
            (BuzzChannelKind::Excluded, "excluded"),
        ] {
            assert_eq!(serde_json::to_string(&kind).unwrap(), format!("\"{wire}\""));
            let parsed: BuzzChannelKind = serde_json::from_str(&format!("\"{wire}\"")).unwrap();
            assert_eq!(parsed, kind);
        }
    }

    #[test]
    fn channel_kind_rejects_unknown_wire_values() {
        assert!(serde_json::from_str::<BuzzChannelKind>("\"public\"").is_err());
    }

    #[tokio::test]
    async fn local_fallback_allows_only_workspace_channels() {
        let authority = LocalFallbackAuthority;
        let cases = [
            (BuzzChannelKind::Workspace, true),
            (BuzzChannelKind::Dm, false),
            (BuzzChannelKind::Private, false),
            (BuzzChannelKind::Excluded, false),
        ];
        for (kind, expected) in cases {
            let decision = authority.can_read(&read_request(kind)).await.unwrap();
            assert_eq!(
                decision.is_allow(),
                expected,
                "unexpected decision for {kind:?}: {decision:?}"
            );
        }
    }

    #[test]
    fn read_decision_is_allow_is_correct() {
        assert!(BuzzReadDecision::Allow.is_allow());
        assert!(!BuzzReadDecision::Deny.is_allow());
        assert!(!BuzzReadDecision::NotFound.is_allow());
    }

    /// Stub authority that records every `can_read` call and returns a
    /// deterministic decision per channel kind (workspace → Allow, else Deny).
    struct RecordingStub {
        calls: Mutex<Vec<BuzzChannelKind>>,
    }

    impl RecordingStub {
        fn new() -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
            }
        }

        fn recorded(&self) -> Vec<BuzzChannelKind> {
            self.calls.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl BuzzAuthority for RecordingStub {
        async fn can_read(
            &self,
            req: &BuzzReadRequest,
        ) -> Result<BuzzReadDecision, BuzzAuthorityError> {
            self.calls.lock().unwrap().push(req.channel_kind);
            Ok(match req.channel_kind {
                BuzzChannelKind::Workspace => BuzzReadDecision::Allow,
                _ => BuzzReadDecision::Deny,
            })
        }
    }

    /// Stub authority whose workspace request blocks until released while its
    /// other requests complete immediately — so the workspace request starts
    /// first but finishes LAST, forcing the batch default to re-order.
    struct GatedStub {
        started: Mutex<Vec<BuzzChannelKind>>,
        first_started: Mutex<Option<oneshot::Sender<()>>>,
        release: Notify,
    }

    impl GatedStub {
        fn new() -> (Arc<Self>, oneshot::Receiver<()>) {
            let (first_started_tx, first_started_rx) = oneshot::channel();
            (
                Arc::new(Self {
                    started: Mutex::new(Vec::new()),
                    first_started: Mutex::new(Some(first_started_tx)),
                    release: Notify::new(),
                }),
                first_started_rx,
            )
        }
    }

    #[async_trait]
    impl BuzzAuthority for GatedStub {
        async fn can_read(
            &self,
            req: &BuzzReadRequest,
        ) -> Result<BuzzReadDecision, BuzzAuthorityError> {
            self.started.lock().unwrap().push(req.channel_kind);
            match req.channel_kind {
                BuzzChannelKind::Workspace => {
                    if let Some(tx) = self.first_started.lock().unwrap().take() {
                        let _ = tx.send(());
                    }
                    self.release.notified().await;
                    Ok(BuzzReadDecision::Allow)
                }
                _ => Ok(BuzzReadDecision::Deny),
            }
        }
    }

    /// Stub authority that fails dm requests and returns a decision for
    /// every other channel kind.
    struct MixedStub {
        calls: Mutex<usize>,
    }

    impl MixedStub {
        fn new() -> Self {
            Self {
                calls: Mutex::new(0),
            }
        }
    }

    #[async_trait]
    impl BuzzAuthority for MixedStub {
        async fn can_read(
            &self,
            req: &BuzzReadRequest,
        ) -> Result<BuzzReadDecision, BuzzAuthorityError> {
            *self.calls.lock().unwrap() += 1;
            match req.channel_kind {
                BuzzChannelKind::Dm => Err(BuzzAuthorityError::Transport("boom".to_string())),
                BuzzChannelKind::Workspace => Ok(BuzzReadDecision::Allow),
                _ => Ok(BuzzReadDecision::NotFound),
            }
        }
    }

    #[tokio::test]
    async fn can_read_batch_fans_out_exactly_once_per_request_in_input_order() {
        let stub = RecordingStub::new();
        let reqs = [
            read_request(BuzzChannelKind::Workspace),
            read_request(BuzzChannelKind::Dm),
            read_request(BuzzChannelKind::Private),
        ];
        let results = stub.can_read_batch(&reqs).await;
        assert_eq!(results.len(), 3);
        assert!(matches!(results[0], Ok(BuzzReadDecision::Allow)));
        assert!(matches!(results[1], Ok(BuzzReadDecision::Deny)));
        assert!(matches!(results[2], Ok(BuzzReadDecision::Deny)));
        // Exactly one `can_read` per request, started in input order.
        assert_eq!(
            stub.recorded(),
            vec![
                BuzzChannelKind::Workspace,
                BuzzChannelKind::Dm,
                BuzzChannelKind::Private,
            ]
        );
    }

    #[tokio::test]
    async fn can_read_batch_preserves_input_order_when_completions_are_out_of_order() {
        let (stub, first_started) = GatedStub::new();
        let reqs = [
            read_request(BuzzChannelKind::Workspace), // slow: blocks until released
            read_request(BuzzChannelKind::Dm),        // fast: completes immediately
        ];
        let stub_for_task = stub.clone();
        let handle = tokio::spawn(async move { stub_for_task.can_read_batch(&reqs).await });

        // Wait until the slow request is in flight, let the fast request
        // complete first, then release the slow one: the results must still
        // come back in input order.
        first_started.await.expect("the slow request must start");
        tokio::task::yield_now().await;
        stub.release.notify_one();

        let results = handle.await.expect("batch task must complete");
        assert_eq!(results.len(), 2);
        assert!(
            matches!(results[0], Ok(BuzzReadDecision::Allow)),
            "input index 0 must map to the slow request's decision"
        );
        assert!(
            matches!(results[1], Ok(BuzzReadDecision::Deny)),
            "input index 1 must map to the fast request's decision"
        );
        // Both requests were started, in input order.
        assert_eq!(
            *stub.started.lock().unwrap(),
            vec![BuzzChannelKind::Workspace, BuzzChannelKind::Dm]
        );
    }

    #[tokio::test]
    async fn can_read_batch_propagates_errors_per_item_without_aborting_others() {
        let stub = MixedStub::new();
        let reqs = [
            read_request(BuzzChannelKind::Workspace),
            read_request(BuzzChannelKind::Dm), // → Err
            read_request(BuzzChannelKind::Private),
        ];
        let results = stub.can_read_batch(&reqs).await;
        assert_eq!(results.len(), 3);
        assert!(matches!(&results[0], Ok(BuzzReadDecision::Allow)));
        assert!(matches!(&results[1], Err(BuzzAuthorityError::Transport(_))));
        assert!(matches!(&results[2], Ok(BuzzReadDecision::NotFound)));
        assert_eq!(
            *stub.calls.lock().unwrap(),
            3,
            "an error for one request must not skip the remaining requests"
        );
    }

    #[tokio::test]
    async fn can_read_batch_with_empty_input_returns_empty() {
        let stub = RecordingStub::new();
        let results = stub.can_read_batch(&[]).await;
        assert!(results.is_empty());
        assert!(
            stub.recorded().is_empty(),
            "no can_read call may be made for empty input"
        );
    }
}
