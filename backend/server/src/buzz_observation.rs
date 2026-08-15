//! Buzz → Elembra Memory projection: the observation half of the bridge.
//!
//! [`BuzzObservationService`] is the authenticated ingestion path where the
//! external Nostr-based chat engine ("Buzz", `io.elembra.chat`) pushes signed
//! message events. The service:
//!
//! 1. verifies the HMAC over the raw request body (`X-RustShare-Signature`,
//!    timestamped form) and enforces the replay window — fail closed;
//! 2. parses the push and validates the Chat context — fail closed;
//! 3. deserializes the signed Nostr event and cryptographically verifies it
//!    ([`nostr::Event::verify`] checks both the id — sha256 of the canonical
//!    NIP-01 serialization — and the Schnorr signature over the id), rejecting
//!    non-chat-message kinds (TextNote legacy, stream kinds 9/40002) — fail
//!    closed;
//! 4. maps community → Workspace and author pubkey → Principal (active
//!    binding only);
//! 5. in ONE transaction records the observation row in `chat_observed_events`
//!    and, only on first observation, publishes the deterministic durable
//!    integration event `io.elembra.chat.buzz.event.observed.v1` into the
//!    transactional outbox. Duplicate observations of the same Buzz event id
//!    are a no-op: the durable event was already published on first
//!    observation.
//!
//! Buzz remains authoritative for messages, channels and membership. This
//! module never reads Buzz tables and never stores a Buzz private key.

use std::fmt;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use nostr::{Event as NostrEvent, Kind};
use rustshare_core::domain::{ApplicationId, PrincipalId, TenantId, WorkspaceId};
use rustshare_crypto::WebhookSigner;
use rustshare_integration_events::event::{ActorRef, IntegrationEvent};
use rustshare_integration_events::event_types::CHAT_BUZZ_EVENT_OBSERVED_V1;
use rustshare_memory::event::{
    BuzzEventMeta, ChatChannelKind, ChatContext, ObservedChatEventData, ObservedEventType,
    PrincipalMeta,
};
use rustshare_memory::observed::ChatObservedEvent;
use rustshare_resource_auth::resource_ref::ResourceRef;
use rustshare_resource_auth::BindingStatus;
use rustshare_storage::{
    ChatIdentityStore, ChatObservationStore, CommunityMappingError, OutboxStore, UpsertOutcome,
};
use serde::{Deserialize, Serialize};

/// Maximum acceptable future skew for a pushed event's author-chosen
/// `created_at` (seconds). See the check in [`BuzzObservationService::validate_and_build`].
const MAX_CREATED_AT_FUTURE_SKEW_SECS: i64 = 15 * 60;

/// Push payload: the signed Nostr event (opaque JSON, cryptographically
/// verified after parse) plus the Buzz Chat context describing where the event
/// was observed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuzzEventPush {
    /// The Nostr event as raw JSON; parsed into [`nostr::Event`] for
    /// cryptographic verification. The raw `id` string is never trusted — it
    /// is only ever compared against the parsed, verified event id.
    pub event: serde_json::Value,
    pub context: BuzzPushContext,
}

/// Chat context of an observed Buzz event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuzzPushContext {
    pub community_id: String,
    pub channel_id: String,
    pub channel_kind: ChatChannelKind,
    pub thread_root_id: Option<String>,
    pub message_id: String,
    pub event_type: ObservedEventType,
    pub supersedes_event_id: Option<String>,
}

/// Fail-closed errors from [`BuzzObservationService::verify_and_ingest`],
/// [`BuzzObservationService::validate_and_build`], and
/// [`BuzzObservationService::ingest_without_outbox`].
#[derive(Debug)]
pub enum BuzzPushError {
    /// HMAC missing/invalid or the replay window was exceeded.
    Unauthorized,
    /// Unparseable body / invalid context fields / context mismatches event.
    Malformed(String),
    /// Nostr id or Schnorr signature verification failed, or the kind is not
    /// an accepted chat-message kind (TextNote legacy, stream kinds 9/40002).
    VerificationFailed,
    /// `community_id` maps to no active mapping.
    UnknownCommunity,
    /// Author pubkey has no live (active) binding in the mapped tenant.
    UnboundAuthor,
    /// More than one active workspace↔community mapping exists for the
    /// `community_id` (data-integrity violation). The caller must fail closed;
    /// this is distinct from [`BuzzPushError::Persistence`] so the API can
    /// return a conflict rather than an opaque internal error.
    AmbiguousCommunity {
        community_id: String,
        row_count: usize,
    },
    /// DB/outbox failure (server-side).
    Persistence(String),
}

impl fmt::Display for BuzzPushError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BuzzPushError::Unauthorized => {
                write!(f, "unauthorized: HMAC missing/invalid or expired")
            }
            BuzzPushError::Malformed(reason) => write!(f, "malformed request: {reason}"),
            BuzzPushError::VerificationFailed => {
                write!(
                    f,
                    "Nostr event verification failed (id, signature, or kind)"
                )
            }
            BuzzPushError::UnknownCommunity => write!(f, "unknown community"),
            BuzzPushError::UnboundAuthor => write!(f, "unbound author"),
            BuzzPushError::AmbiguousCommunity {
                community_id,
                row_count,
            } => write!(
                f,
                "ambiguous community mapping: {community_id} is active in {row_count} tenants"
            ),
            BuzzPushError::Persistence(reason) => write!(f, "persistence failure: {reason}"),
        }
    }
}

impl std::error::Error for BuzzPushError {}

/// Outcome of one [`BuzzObservationService::verify_and_ingest`] or
/// [`BuzzObservationService::ingest_without_outbox`] call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IngestOutcome {
    /// The observation row was inserted and the durable event published.
    FirstObservation,
    /// The Buzz event id was already observed; nothing was written.
    DuplicateObservation,
}

/// Authenticated ingestion of signed Buzz chat events.
pub struct BuzzObservationService {
    pool: sqlx::PgPool,
    chat_identity: ChatIdentityStore,
    observations: ChatObservationStore,
    outbox: Arc<OutboxStore>,
    signer: WebhookSigner,
    max_age_seconds: u64,
    broadcaster: Arc<rustshare_core::events::EventBroadcaster>,
}

impl BuzzObservationService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        pool: sqlx::PgPool,
        chat_identity: ChatIdentityStore,
        observations: ChatObservationStore,
        outbox: Arc<OutboxStore>,
        signer: WebhookSigner,
        max_age_seconds: u64,
        broadcaster: Arc<rustshare_core::events::EventBroadcaster>,
    ) -> Self {
        Self {
            pool,
            chat_identity,
            observations,
            outbox,
            signer,
            max_age_seconds,
            broadcaster,
        }
    }

    /// Verify HMAC + replay window, parse, verify the signed Nostr event, map
    /// author→Principal and community→Workspace, and in ONE transaction upsert
    /// the observation row and (only on first observation) publish the
    /// deterministic durable integration event. Duplicate observations of the
    /// same Buzz event id are a no-op (event already durably published).
    pub async fn verify_and_ingest(
        &self,
        payload: &[u8],
        signature: &str,
    ) -> Result<IngestOutcome, BuzzPushError> {
        // 1. HMAC + replay window (fail closed).
        self.verify_hmac(payload, signature)?;

        // 2. Parse the push envelope.
        let push: BuzzEventPush = serde_json::from_slice(payload)
            .map_err(|e| BuzzPushError::Malformed(format!("invalid request body: {e}")))?;

        // 3–8. Validate + build the observation row; no writes yet.
        let (tenant, workspace, row, data) = self.validate_and_build(&push).await?;

        // 9. One transaction: upsert the observation row and publish the
        //    durable event — but only on first observation.
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| BuzzPushError::Persistence(e.to_string()))?;
        let outcome = self
            .observations
            .upsert_event_in_tx(&mut tx, &row)
            .await
            .map_err(|e| BuzzPushError::Persistence(e.to_string()))?;
        match outcome {
            // Identical Buzz event already observed; its durable event was
            // published on first observation — never publish again.
            UpsertOutcome::Duplicate => {
                tx.rollback()
                    .await
                    .map_err(|e| BuzzPushError::Persistence(e.to_string()))?;
                return Ok(IngestOutcome::DuplicateObservation);
            }
            // The relay re-served the same event id as a snapshot tombstone:
            // the flip must COMMIT, but the durable Created envelope was
            // already published on first observation — never publish again
            // (a second envelope for the same Buzz event id would collide).
            UpsertOutcome::FlippedToTombstone => {
                tx.commit()
                    .await
                    .map_err(|e| BuzzPushError::Persistence(e.to_string()))?;
                return Ok(IngestOutcome::DuplicateObservation);
            }
            UpsertOutcome::Inserted => {}
        }
        let latest_created_at = sqlx::query_scalar::<_, Option<DateTime<Utc>>>(
            "SELECT MAX(event_created_at) FROM chat_observed_events
             WHERE tenant_id = $1 AND community_id = $2",
        )
        .bind(tenant.0)
        .bind(&data.context.community_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| BuzzPushError::Persistence(e.to_string()))?;
        metrics::gauge!(
            "chat_observation_lag_seconds",
            "community_id" => data.context.community_id.clone()
        )
        .set(
            (Utc::now() - latest_created_at.unwrap_or(data.buzz.created_at))
                .num_milliseconds()
                .max(0) as f64
                / 1000.0,
        );
        let envelope = build_envelope(tenant, workspace, &data, data.principal.principal_id)?;
        self.outbox
            .insert_in_tx(&mut tx, &envelope)
            .await
            .map_err(|e| BuzzPushError::Persistence(e.to_string()))?;
        tx.commit()
            .await
            .map_err(|e| BuzzPushError::Persistence(e.to_string()))?;
        // Best-effort realtime fan-out (ADR-0031: ephemeral, lossy by design).
        // The durable path is the outbox event above; a publish failure must
        // never fail the ingest.
        let author_user_id = data.principal.principal_id.0;
        self.broadcaster.publish(rustshare_core::events::Event::new(
            rustshare_core::events::EventType::ChatMessageObserved,
            uuid::Uuid::new_v4(),
            rustshare_core::events::AggregateType::ChatMessage,
            serde_json::json!(rustshare_core::events::ChatMessageObservedPayload {
                tenant_id: tenant.0,
                workspace_id: workspace.0,
                community_id: data.context.community_id.clone(),
                channel_id: data.context.channel_id.clone(),
                channel_kind: data.context.channel_kind.as_str().to_string(),
                message_id: data.buzz.message_id.clone(),
                event_id: data.buzz.event_id.clone(),
            }),
            author_user_id,
        ));
        Ok(IngestOutcome::FirstObservation)
    }

    /// Steps 3–8 of ingestion: validate the Chat context (fail closed), verify
    /// the signed Nostr event, map community → Workspace and author pubkey →
    /// Principal (active binding only), apply the body gate, and build the
    /// observation row + payload. No DB writes happen here: the caller owns
    /// the transaction so the webhook path can publish the durable outbox
    /// event on first observation while the reconcile path repairs the
    /// observation index without touching the outbox.
    async fn validate_and_build(
        &self,
        push: &BuzzEventPush,
    ) -> Result<
        (
            TenantId,
            WorkspaceId,
            ChatObservedEvent,
            ObservedChatEventData,
        ),
        BuzzPushError,
    > {
        // 3. Chat context sanity (fail closed).
        validate_context(push)?;

        // 4. Cryptographic verification of the signed Nostr event.
        let raw_event_id = push
            .event
            .get("id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| BuzzPushError::Malformed("Nostr event missing `id`".to_string()))?;
        let event: NostrEvent = serde_json::from_value(push.event.clone())
            .map_err(|e| BuzzPushError::Malformed(format!("invalid Nostr event: {e}")))?;
        if !is_chat_message_kind(event.kind) {
            return Err(BuzzPushError::VerificationFailed);
        }
        // The raw JSON `id` must equal the parsed event id. `Event::verify`
        // recomputes the id from the canonical serialization (so a lying raw
        // field can never survive verification), but be explicit: the
        // observation identity is derived from the parsed event only.
        let event_id_hex = event.id.to_hex();
        if raw_event_id != event_id_hex {
            return Err(BuzzPushError::VerificationFailed);
        }
        event
            .verify()
            .map_err(|_| BuzzPushError::VerificationFailed)?;

        // A future-dated `created_at` is author-controlled inside the signed
        // event and `Event::verify` does not bound it. Accepting one would let
        // a bound author pin a message above any later delete/edit — the
        // timeline fold orders by `event_created_at DESC` and the tombstone
        // window is `>= since` — so reject anything beyond a small clock-skew
        // window. Past timestamps stay valid: relay reconciliation legitimately
        // replays history. Reported as `Malformed` so the webhook answers 400
        // (permanent; the bridge must not retry a bad event).
        let created_secs = event.created_at.as_secs() as i64;
        if created_secs.saturating_sub(Utc::now().timestamp()) > MAX_CREATED_AT_FUTURE_SKEW_SECS {
            return Err(BuzzPushError::Malformed(
                "Buzz event created_at is too far in the future".to_string(),
            ));
        }

        // 5. Community → Workspace mapping (workspace == tenant invariant).
        let mapping = match self
            .chat_identity
            .mapping_by_community(&push.context.community_id)
            .await
        {
            Ok(mapping) => mapping.ok_or(BuzzPushError::UnknownCommunity)?,
            Err(CommunityMappingError::Ambiguous {
                community_id,
                row_count,
            }) => {
                return Err(BuzzPushError::AmbiguousCommunity {
                    community_id,
                    row_count,
                })
            }
            Err(CommunityMappingError::Database(e)) => {
                return Err(BuzzPushError::Persistence(e.to_string()))
            }
        };
        let tenant = mapping.tenant_id;
        let workspace = WorkspaceId(mapping.workspace_id.0);
        if workspace != WorkspaceId(mapping.tenant_id.0) {
            // Platform invariant: one workspace per tenant. A mapping that
            // violates it is a server-side integrity failure — fail closed
            // rather than derive a scope the envelope validation would reject
            // later.
            return Err(BuzzPushError::Persistence(
                "community mapping violates the workspace == tenant invariant".to_string(),
            ));
        }

        // 6. Author pubkey → Principal binding; only an active binding is a
        //    live author. `binding_by_pubkey` already excludes revoked rows
        //    (`revoked_at IS NULL`), but a `pending` binding must not pass.
        let binding = self
            .chat_identity
            .binding_by_pubkey(tenant, &event.pubkey.to_hex())
            .await
            .map_err(|e| BuzzPushError::Persistence(e.to_string()))?
            .filter(|binding| binding.status == BindingStatus::Active)
            .ok_or(BuzzPushError::UnboundAuthor)?;

        // 7. Body gate: message bodies are stored only when the tenant has
        //    `content_indexing` enabled AND the event is in a `workspace`
        //    channel; otherwise reference-first. Bodies from never-eligible
        //    channels (`dm`/`private`/`excluded`) are never captured, so they
        //    can never leak into an indexing copy under the tenant's opt-in.
        let policy = self
            .chat_identity
            .projection_policy(tenant, workspace)
            .await
            .map_err(|e| BuzzPushError::Persistence(e.to_string()))?;
        let body =
            if policy.content_indexing && push.context.channel_kind == ChatChannelKind::Workspace {
                Some(event.content.clone())
            } else {
                None
            };

        // 8. Build the observation payload + row. `observed_at` is captured
        //    once, before the transaction, so the row and the durable envelope
        //    agree; because the envelope is published only on first
        //    observation this is deterministic per Buzz event id.
        let observed_at = Utc::now();
        let created_at = DateTime::<Utc>::from_timestamp(event.created_at.as_secs() as i64, 0)
            .ok_or_else(|| {
                BuzzPushError::Persistence("Buzz event created_at out of range".to_string())
            })?;
        // Checksum: the Nostr `event.id` IS the sha256 of the canonical NIP-01
        // serialization, so `sha256:{event.id}` is the canonical checksum. The
        // JSON re-serialization of the full event (id + sig included) does not
        // match the canonical id derivation — see the unit tests.
        let checksum = format!("sha256:{event_id_hex}");
        let data = ObservedChatEventData {
            buzz: BuzzEventMeta {
                event_id: event_id_hex,
                message_id: push.context.message_id.clone(),
                event_type: push.context.event_type,
                supersedes_event_id: push.context.supersedes_event_id.clone(),
                created_at,
                pubkey: event.pubkey.to_hex(),
                signature: event.sig.to_string(),
                checksum,
                signature_verified: true,
            },
            context: ChatContext {
                community_id: push.context.community_id.clone(),
                channel_id: push.context.channel_id.clone(),
                channel_kind: push.context.channel_kind,
                thread_root_id: push.context.thread_root_id.clone(),
            },
            principal: PrincipalMeta {
                principal_id: binding.principal_id,
            },
            observed_at,
        };
        let row = ChatObservedEvent::from_observed_data(tenant, workspace, &data, body);
        Ok((tenant, workspace, row, data))
    }

    /// Repair-path ingestion used by admin reconciliation: validate + build
    /// and upsert the observation row in ONE transaction, with no durable
    /// envelope and no outbox insert — consumer receipts stay untouched so the
    /// durable pipeline is never replayed. Idempotent by `(tenant_id,
    /// event_id)`: re-running a reconcile over the same Buzz events changes
    /// nothing. Entry events are independently signature-verified by
    /// [`Self::validate_and_build`].
    pub(crate) async fn ingest_without_outbox(
        &self,
        push: &BuzzEventPush,
    ) -> Result<IngestOutcome, BuzzPushError> {
        let (_, _, row, _) = self.validate_and_build(push).await?;
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| BuzzPushError::Persistence(e.to_string()))?;
        let outcome = self
            .observations
            .upsert_event_in_tx(&mut tx, &row)
            .await
            .map_err(|e| BuzzPushError::Persistence(e.to_string()))?;
        match outcome {
            // Identical Buzz event already observed during an earlier
            // reconcile or webhook push; nothing to repair.
            UpsertOutcome::Duplicate => {
                tx.rollback()
                    .await
                    .map_err(|e| BuzzPushError::Persistence(e.to_string()))?;
                return Ok(IngestOutcome::DuplicateObservation);
            }
            // The relay re-served the same event id as a snapshot tombstone:
            // the flip must COMMIT (the repair applies the deletion).
            UpsertOutcome::FlippedToTombstone => {
                tx.commit()
                    .await
                    .map_err(|e| BuzzPushError::Persistence(e.to_string()))?;
                return Ok(IngestOutcome::DuplicateObservation);
            }
            UpsertOutcome::Inserted => {}
        }
        tx.commit()
            .await
            .map_err(|e| BuzzPushError::Persistence(e.to_string()))?;
        Ok(IngestOutcome::FirstObservation)
    }

    /// Verify the HMAC over the raw payload and enforce the replay window.
    ///
    /// Only the timestamped signature form (`t=<ts>,v1=<hex>`) is accepted:
    /// without a timestamp the replay window cannot be enforced, so a plain
    /// `v1=<hex>` signature fails closed as unauthorized.
    fn verify_hmac(&self, payload: &[u8], signature: &str) -> Result<(), BuzzPushError> {
        let (timestamp_part, sig_part) = signature
            .split_once(',')
            .ok_or(BuzzPushError::Unauthorized)?;
        if !sig_part.starts_with("v1=") {
            return Err(BuzzPushError::Unauthorized);
        }
        let timestamp = timestamp_part
            .strip_prefix("t=")
            .and_then(|value| value.parse::<i64>().ok())
            .ok_or(BuzzPushError::Unauthorized)?;
        let verified = self
            .signer
            .verify(signature, payload)
            .map_err(|_| BuzzPushError::Unauthorized)?;
        if !verified {
            return Err(BuzzPushError::Unauthorized);
        }
        let now = Utc::now().timestamp();
        let age = now.saturating_sub(timestamp);
        if timestamp > now || age as u64 > self.max_age_seconds {
            return Err(BuzzPushError::Unauthorized);
        }
        Ok(())
    }
}

/// Build the deterministic durable envelope for a first observation.
///
/// Idempotency is keyed on the Buzz event identity: the outbox event id is
/// [`rustshare_memory::event::integration_event_id_for`] over the Nostr event
/// id, and `time` is the Buzz event's own creation time — not the observation
/// time. Publishing happens only on first observation, so the payload (which
/// carries `observed_at`) is deterministic per event id too.
fn build_envelope(
    tenant_id: TenantId,
    workspace_id: WorkspaceId,
    data: &ObservedChatEventData,
    principal_id: PrincipalId,
) -> Result<IntegrationEvent, BuzzPushError> {
    let id = rustshare_memory::event::integration_event_id_for(&data.buzz.event_id);
    let resource = ResourceRef::new(
        ApplicationId::new("io.elembra.chat"),
        "message",
        &data.buzz.message_id,
    );
    let data_json = serde_json::to_value(data)
        .map_err(|e| BuzzPushError::Persistence(format!("payload serialization failed: {e}")))?;
    let mut envelope = IntegrationEvent::builder()
        .source("elembra://io.elembra.chat")
        .r#type(CHAT_BUZZ_EVENT_OBSERVED_V1)
        .subject(resource.to_uri())
        .tenant_id(tenant_id)
        .workspace_id(workspace_id)
        .actor(ActorRef::Principal(principal_id))
        .resource(resource)
        .data(data_json)
        .build()
        .map_err(|e| BuzzPushError::Persistence(format!("envelope validation failed: {e}")))?;
    // The builder does not expose `.id()`/`.time()` setters; assign the
    // deterministic identity and the Buzz event's creation time after the
    // validated build (all fields are `pub`).
    envelope.id = id;
    envelope.time = data.buzz.created_at;
    Ok(envelope)
}

/// Whether `kind` is an accepted Buzz chat-message kind on ingestion.
///
/// Whitelist: kind 1 (`TextNote`, legacy during the transition) plus the Buzz
/// stream-message kinds 9 (`KIND_STREAM_MESSAGE`) and 40002
/// (`KIND_STREAM_MESSAGE_V2`) — the relay's channel-scoped chat kinds, named
/// in the Buzz relay at `crates/buzz-core/src/kind.rs` and adopted by the
/// amended contract in `docs/specs/buzz-upstream-authorization-v1alpha1.md`
/// ("Canonical publish tags and kinds"). Every other kind fails closed.
fn is_chat_message_kind(kind: Kind) -> bool {
    // Compare by the numeric kind, not by enum variant: the `nostr` crate
    // gives kind 9 the named variant `Kind::ChatMessage`, so a pattern like
    // `Kind::Custom(9)` would silently reject every parsed kind-9 event.
    matches!(kind.as_u16(), 1 | 9 | 40002)
}

/// Fail-closed Chat context sanity checks (step 3 of `verify_and_ingest`).
fn validate_context(push: &BuzzEventPush) -> Result<(), BuzzPushError> {
    let context = &push.context;
    if !is_lower_hex(&context.message_id, 64) {
        return Err(BuzzPushError::Malformed(
            "context.message_id must be 64 lowercase hex characters".to_string(),
        ));
    }
    if let Some(thread_root) = &context.thread_root_id {
        if !is_lower_hex(thread_root, 64) {
            return Err(BuzzPushError::Malformed(
                "context.thread_root_id must be 64 lowercase hex characters".to_string(),
            ));
        }
    }
    if let Some(supersedes) = &context.supersedes_event_id {
        if !is_lower_hex(supersedes, 64) {
            return Err(BuzzPushError::Malformed(
                "context.supersedes_event_id must be 64 lowercase hex characters".to_string(),
            ));
        }
    }
    // The raw `id` field is only ever a comparison input here; the parsed,
    // cryptographically verified event id is authoritative (step 4).
    let raw_event_id = push
        .event
        .get("id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| BuzzPushError::Malformed("Nostr event missing `id`".to_string()))?;
    match context.event_type {
        // A created event IS the message root: its id must equal the message
        // id, and there is nothing it can supersede.
        ObservedEventType::Created => {
            if raw_event_id != context.message_id {
                return Err(BuzzPushError::Malformed(
                    "created event message_id must equal the Nostr event id".to_string(),
                ));
            }
            if context.supersedes_event_id.is_some() {
                return Err(BuzzPushError::Malformed(
                    "created events must not supersede an earlier event".to_string(),
                ));
            }
        }
        // An edit is a different event from the root message. It may
        // supersede the root — whose event id IS the message id — so
        // `supersedes == message_id` is the correct first-edit contract;
        // only self-supersede (`supersedes == this event's id`) is invalid.
        ObservedEventType::Edited => {
            if raw_event_id == context.message_id {
                return Err(BuzzPushError::Malformed(
                    "edited event message_id must differ from the Nostr event id".to_string(),
                ));
            }
            if let Some(supersedes) = &context.supersedes_event_id {
                if supersedes == raw_event_id {
                    return Err(BuzzPushError::Malformed(
                        "context.supersedes_event_id must not equal the Nostr event id".to_string(),
                    ));
                }
            }
        }
        // A deletion has TWO accepted forms:
        // 1. Webhook form: a separate deletion event superseding the message
        //    — `raw_event_id != message_id` (same identity rules as Edited).
        // 2. Snapshot form (the relay's `state/events` tombstone): the
        //    message itself is the entry, marked deleted —
        //    `raw_event_id == message_id` with `supersedes_event_id: null`
        //    (a snapshot tombstone supersedes nothing). Reconcile applies it
        //    by re-ingesting the same event id as a Deleted observation.
        ObservedEventType::Deleted => {
            if raw_event_id == context.message_id {
                // Snapshot form.
                if context.supersedes_event_id.is_some() {
                    return Err(BuzzPushError::Malformed(
                        "snapshot-form deleted events must not carry supersedes_event_id"
                            .to_string(),
                    ));
                }
            } else if let Some(supersedes) = &context.supersedes_event_id {
                if supersedes == raw_event_id {
                    return Err(BuzzPushError::Malformed(
                        "context.supersedes_event_id must not equal the Nostr event id".to_string(),
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Whether `s` is exactly `len` ASCII lowercase hex characters.
fn is_lower_hex(s: &str, len: usize) -> bool {
    s.len() == len
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::{EventBuilder, Keys};
    use rustshare_memory::event::integration_event_id_for;
    use serde_json::json;
    use uuid::Uuid;

    const HEX64: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn signed_text_note(content: &str) -> (Keys, NostrEvent) {
        let keys = Keys::generate();
        let event = EventBuilder::text_note(content)
            .sign_with_keys(&keys)
            .expect("sign the text note");
        (keys, event)
    }

    fn push(event: &NostrEvent, message_id: &str, event_type: ObservedEventType) -> BuzzEventPush {
        BuzzEventPush {
            event: serde_json::to_value(event).expect("serialize nostr event"),
            context: BuzzPushContext {
                community_id: "community-1".to_string(),
                channel_id: "channel-1".to_string(),
                channel_kind: ChatChannelKind::Workspace,
                thread_root_id: None,
                message_id: message_id.to_string(),
                event_type,
                supersedes_event_id: None,
            },
        }
    }

    #[test]
    fn buzz_event_push_serde_round_trips() {
        let (_, event) = signed_text_note("hello buzz");
        let original = push(&event, &event.id.to_hex(), ObservedEventType::Created);
        let json = serde_json::to_value(&original).unwrap();
        let back: BuzzEventPush = serde_json::from_value(json).unwrap();
        assert_eq!(back.event, original.event);
        assert_eq!(back.context.community_id, "community-1");
        assert_eq!(back.context.event_type, ObservedEventType::Created);
        assert_eq!(back.context.message_id, event.id.to_hex());
    }

    #[test]
    fn context_validation_rejects_invalid_fields() {
        let (_, event) = signed_text_note("hello buzz");
        let event_id = event.id.to_hex();

        // Bad message_id hex (uppercase).
        let mut p = push(&event, &event_id, ObservedEventType::Created);
        p.context.message_id = "A".repeat(64);
        assert!(matches!(
            validate_context(&p),
            Err(BuzzPushError::Malformed(_))
        ));

        // Bad thread_root_id.
        let mut p = push(&event, &event_id, ObservedEventType::Edited);
        p.context.thread_root_id = Some("x".repeat(63));
        assert!(matches!(
            validate_context(&p),
            Err(BuzzPushError::Malformed(_))
        ));

        // Bad supersedes_event_id.
        let mut p = push(&event, &event_id, ObservedEventType::Edited);
        p.context.supersedes_event_id = Some("Z".repeat(64));
        assert!(matches!(
            validate_context(&p),
            Err(BuzzPushError::Malformed(_))
        ));

        // Valid created passes.
        let p = push(&event, &event_id, ObservedEventType::Created);
        assert!(validate_context(&p).is_ok());
    }

    #[test]
    fn context_validation_enforces_event_identity_rules() {
        let (_, event) = signed_text_note("hello buzz");
        let event_id = event.id.to_hex();
        // A deterministic 64-hex message id that differs from the event id.
        let mut other = event_id.clone();
        let last = other.pop().unwrap();
        other.push(if last == 'a' { 'b' } else { 'a' });

        // Created with supersedes present → Malformed.
        let mut p = push(&event, &event_id, ObservedEventType::Created);
        p.context.supersedes_event_id = Some(other.clone());
        assert!(matches!(
            validate_context(&p),
            Err(BuzzPushError::Malformed(_))
        ));

        // Created with message_id != event id → Malformed.
        let mut p = push(&event, &event_id, ObservedEventType::Created);
        p.context.message_id = other.clone();
        assert!(matches!(
            validate_context(&p),
            Err(BuzzPushError::Malformed(_))
        ));

        // Edited with supersedes == message_id (first edit superseding the
        // root, whose event id IS the message id) → VALID. This is the key
        // regression test for the push-context contract.
        let mut p = push(&event, &other, ObservedEventType::Edited);
        p.context.supersedes_event_id = Some(other.clone());
        assert!(validate_context(&p).is_ok());

        // Edited with supersedes == event id (self-supersede) → Malformed.
        let mut p = push(&event, &other, ObservedEventType::Edited);
        p.context.supersedes_event_id = Some(event_id.clone());
        assert!(matches!(
            validate_context(&p),
            Err(BuzzPushError::Malformed(_))
        ));

        // Edited with message_id == event id → Malformed.
        let p = push(&event, &event_id, ObservedEventType::Edited);
        assert!(matches!(
            validate_context(&p),
            Err(BuzzPushError::Malformed(_))
        ));
    }

    #[test]
    fn wrong_event_type_string_fails_serde_parse() {
        let (_, event) = signed_text_note("hello buzz");
        let event_id = event.id.to_hex();
        let value = json!({
            "event": serde_json::to_value(&event).unwrap(),
            "context": {
                "community_id": "community-1",
                "channel_id": "channel-1",
                "channel_kind": "workspace",
                "thread_root_id": null,
                "message_id": event_id,
                "event_type": "expired",
                "supersedes_event_id": null,
            }
        });
        assert!(serde_json::from_value::<BuzzEventPush>(value).is_err());
    }

    #[test]
    fn integration_event_id_is_deterministic_and_pinned() {
        // (b) The SAME event id produces the SAME UUIDv5 across calls.
        let first = integration_event_id_for(HEX64);
        let second = integration_event_id_for(HEX64);
        assert_eq!(first, second, "v5 of a fixed input is deterministic");
        assert_ne!(first, Uuid::new_v4());
        // (c) RFC 4122 v5 reference value for this fixed input — the stable,
        // Buzz-specific identity of the integration event.
        assert_eq!(first.to_string(), "c137b36c-4f81-5dcb-b5a0-9bcae9d22466");
    }

    #[test]
    fn integration_event_ids_differ_across_events() {
        // (a) Two DIFFERENT event ids produce DIFFERENT UUIDs: the identity is
        // Buzz-event-specific, so distinct events never collide.
        let mut other = HEX64.to_string();
        let last = other.pop().unwrap();
        other.push(if last == 'a' { 'b' } else { 'a' });
        assert_ne!(
            integration_event_id_for(HEX64),
            integration_event_id_for(&other),
            "each Buzz event must map to its own integration-event UUID"
        );
    }

    #[test]
    fn signed_text_note_round_trips_and_verifies() {
        let (_, event) = signed_text_note("hello buzz");
        // JSON round-trip into the nostr crate.
        let json = serde_json::to_value(&event).unwrap();
        let parsed: NostrEvent = serde_json::from_value(json).unwrap();
        assert_eq!(parsed.id, event.id);
        assert_eq!(parsed.sig, event.sig);
        assert!(event.verify().is_ok(), "event must verify");
        assert_eq!(event.kind, Kind::TextNote);
        assert_eq!(
            event.sig.to_string().len(),
            128,
            "Schnorr signature is 128 hex"
        );
    }

    #[test]
    fn chat_message_kind_whitelist_accepts_text_note_and_stream_kinds() {
        // Kind 1 (TextNote) — legacy during the transition.
        assert!(is_chat_message_kind(Kind::TextNote));
        // Kind 9 (KIND_STREAM_MESSAGE) and 40002 (KIND_STREAM_MESSAGE_V2) —
        // Buzz's channel-scoped chat kinds (see `is_chat_message_kind`).
        // `Kind::ChatMessage` is the nostr crate's NAMED variant for 9 — the
        // form a parsed kind-9 event actually carries — so the whitelist must
        // match it too, not just the `Custom(9)` constructor form.
        assert!(is_chat_message_kind(Kind::ChatMessage));
        assert!(is_chat_message_kind(Kind::Custom(9)));
        assert!(is_chat_message_kind(Kind::Custom(40002)));
    }

    #[test]
    fn chat_message_kind_whitelist_rejects_every_other_kind() {
        for kind in [
            Kind::Metadata,
            Kind::ContactList,
            Kind::Reaction,
            Kind::EventDeletion,
            Kind::HttpAuth,
            Kind::Custom(1000),
            Kind::Custom(19_030),
            Kind::Custom(u16::MAX),
        ] {
            assert!(!is_chat_message_kind(kind), "kind {kind} must fail closed");
        }
    }

    #[test]
    fn signed_stream_message_round_trips_and_verifies() {
        // Stream-kind events (9/40002) must survive the same parse + verify
        // path `validate_and_build` uses — and the PARSED kind must still pass
        // the ingestion whitelist (kind 9 parses as the named
        // `Kind::ChatMessage` variant, not `Kind::Custom(9)`).
        for kind in [Kind::ChatMessage, Kind::Custom(40002)] {
            let keys = Keys::generate();
            let event = EventBuilder::new(kind, "hello stream")
                .sign_with_keys(&keys)
                .expect("sign stream message");
            let json = serde_json::to_value(&event).unwrap();
            let parsed: NostrEvent = serde_json::from_value(json).unwrap();
            assert_eq!(parsed.kind, kind);
            assert_eq!(parsed.id, event.id);
            assert_eq!(parsed.sig, event.sig);
            assert!(parsed.verify().is_ok(), "stream message must verify");
            assert!(
                is_chat_message_kind(parsed.kind),
                "parsed stream kind {} must pass the ingestion whitelist",
                parsed.kind.as_u16()
            );
        }
    }

    #[test]
    fn context_validation_accepts_both_deletion_forms() {
        let (_, event) = signed_text_note("hello buzz");
        let event_id = event.id.to_hex();
        // A deterministic 64-hex message id that differs from the event id.
        let mut other = event_id.clone();
        let last = other.pop().unwrap();
        other.push(if last == 'a' { 'b' } else { 'a' });

        // Snapshot form (the relay's state/events tombstone): the message
        // itself is the entry, marked deleted — id == message_id and
        // supersedes None.
        let p = push(&event, &event_id, ObservedEventType::Deleted);
        assert!(
            validate_context(&p).is_ok(),
            "a snapshot-form tombstone (id == message_id, supersedes None) must be accepted"
        );

        // Snapshot form with a supersedes reference is malformed (a snapshot
        // tombstone supersedes nothing).
        let mut p = push(&event, &event_id, ObservedEventType::Deleted);
        p.context.supersedes_event_id = Some(other.clone());
        assert!(matches!(
            validate_context(&p),
            Err(BuzzPushError::Malformed(_))
        ));

        // Webhook form: a separate deletion event superseding the message
        // root (message_id == the root's id, which differs from this event's
        // id).
        let mut p = push(&event, &event_id, ObservedEventType::Deleted);
        p.context.message_id = other.clone();
        p.context.supersedes_event_id = Some(other.clone());
        assert!(
            validate_context(&p).is_ok(),
            "a webhook-form deletion superseding the message root must be accepted"
        );

        // Webhook form must not self-supersede.
        let mut p = push(&event, &event_id, ObservedEventType::Deleted);
        p.context.message_id = other.clone();
        p.context.supersedes_event_id = Some(event_id);
        assert!(matches!(
            validate_context(&p),
            Err(BuzzPushError::Malformed(_))
        ));
    }

    #[test]
    fn checksum_is_sha256_of_the_canonical_event_id() {
        // Decision note: the checksum is `sha256:{event.id}` because the Nostr
        // event id IS the sha256 of the canonical NIP-01 serialization. The
        // JSON re-serialization of the full event (id + sig included) does NOT
        // match that canonical form, so hashing `serde_json::to_vec(&event)`
        // would NOT reproduce the id — verified here explicitly. Preferring
        // the `event.id` form avoids a re-implementation of NIP-01
        // canonicalization.
        let (_, event) = signed_text_note("hello buzz");
        use sha2::{Digest, Sha256};
        let json_digest = hex::encode(Sha256::digest(serde_json::to_vec(&event).unwrap()));
        assert_ne!(
            json_digest,
            event.id.to_hex(),
            "full-event JSON serialization must not equal the canonical id"
        );
        assert_eq!(event.id.to_hex().len(), 64, "canonical event id is 64 hex");
    }

    #[test]
    fn tampered_event_fails_verification() {
        let (_, mut event) = signed_text_note("hello buzz");
        // Flip the first byte of the signature.
        let mut sig = event.sig.to_string().into_bytes();
        let flipped = if sig[0] == b'0' { b'1' } else { b'0' };
        sig[0] = flipped;
        let tampered_sig = String::from_utf8(sig).unwrap();
        event.sig = tampered_sig.parse().unwrap();
        assert!(
            event.verify().is_err(),
            "tampered signature must fail verify"
        );
    }
}
