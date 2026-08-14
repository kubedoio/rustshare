//! Buzz gateway HTTP client: NIP-98-authenticated access checks and state
//! paging against the community's authoritative Buzz relay.
//!
//! Elembra's server workload holds only a Nostr *service* key — never a human
//! user's signing key. Every request carries a NIP-98 `Authorization` header
//! (kind-27235 event) signed with that service key. Every response must be a
//! raw signed Nostr event of kind 19030 whose `pubkey` is the pinned relay
//! public key, whose content echoes the request verbatim, and whose
//! `evaluated_at` is within the freshness window. Any failure fails closed
//! (see `docs/specs/buzz-upstream-authorization-v1alpha1.md`).
//!
//! Redirects are disabled on the HTTP client: a hostile relay must not be able
//! to coerce the client into following a redirect to an unconfigured host
//! (SSRF protection) — the client talks only to the configured relay URL.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use chrono::Utc;
use futures_util::StreamExt;
use nostr::{Event as NostrEvent, EventBuilder, JsonUtil, Keys, Kind, Tag};
use reqwest::{Client, Response, StatusCode};
use rustshare_core::validation::resolve_public_socket_addrs;
use rustshare_resource_auth::{
    BuzzAuthority, BuzzAuthorityError, BuzzChannelInfo, BuzzChannelKind, BuzzReadDecision,
    BuzzReadRequest,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tracing::warn;
use url::Url;

/// Default per-request timeout for relay calls.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);
/// Relay responses are kind-19030 events (unregistered, private replaceable
/// range) — never published, only returned inline as an HTTP response.
const RELAY_RESPONSE_KIND: u16 = 19_030;
/// `evaluated_at` must be within this many seconds of the client clock when
/// received; an older (or future) response is stale and fails closed.
const MAX_EVALUATED_AT_AGE_SECS: u64 = 60;
/// Hard cap on the relay response body we are willing to buffer; a larger
/// body (8 MiB covers the largest signed state page) is treated as hostile
/// and fails closed instead of exhausting server memory.
const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
/// Maximum checks per batch round-trip (the relay's contract cap; the client
/// splits larger batches into sequential round-trips of at most this many).
const MAX_BATCH_CHECKS: usize = 64;

/// Access-check request sent to `POST /api/v1/relay/access/check`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuzzAccessCheckRequest {
    pub pubkey: String,
    pub channel_id: String,
    pub channel_kind: BuzzChannelKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    /// Optional informational unix-seconds of the checked event; the relay MAY
    /// ignore it and the client sends it without any validation change.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_created_at: Option<i64>,
}

/// Signed access-check result returned by the relay (kind-19030 content).
///
/// The `pubkey`, `channel_id`, and `message_id` fields echo the request
/// verbatim (`message_id: null` when the request had none); the client rejects
/// any response whose echoed values do not match the request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuzzAccessCheckResult {
    pub decision: String,
    pub reason: String,
    pub evaluated_at: i64,
    pub pubkey: String,
    pub channel_id: String,
    pub message_id: Option<String>,
}

/// Batch access-check response envelope (`POST /api/v1/relay/access/check-batch`):
/// order-preserving `results` plus the envelope-level `evaluated_at` — the
/// freshness authority for the whole response (each item's `evaluated_at`
/// mirrors it and is informational).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct BuzzAccessCheckBatchResponse {
    results: Vec<BuzzAccessCheckResult>,
    evaluated_at: i64,
}

/// Channel-registry response envelope (`GET /api/v1/relay/channels`): the
/// channels the queried pubkey may read, the response `evaluated_at`, and the
/// echoed query `pubkey`.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct BuzzChannelRegistry {
    channels: Vec<BuzzChannelInfo>,
    evaluated_at: i64,
    pubkey: String,
}

/// One page of the relay's signed event state
/// (`GET /api/v1/relay/state/events`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuzzStatePage {
    pub entries: Vec<BuzzStateEntry>,
    /// Opaque continuation token for the next page; `None` on the final page.
    pub cursor: Option<String>,
    /// `true` terminates the stream (the final page).
    pub complete: bool,
}

/// A raw signed kind-1 event plus its chat context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuzzStateEntry {
    /// The raw signed kind-1 event JSON; verified by the reconcile consumer,
    /// never by the paging client.
    pub event: Value,
    pub context: BuzzStateContext,
}

/// Chat context of a paged state entry — field-for-field the webhook
/// `BuzzPushContext` shape, so the reconcile consumer reuses its existing
/// validation unchanged. `event_type` is `created|edited|deleted`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuzzStateContext {
    pub community_id: String,
    pub channel_id: String,
    pub channel_kind: BuzzChannelKind,
    pub thread_root_id: Option<String>,
    pub message_id: String,
    pub event_type: String,
    pub supersedes_event_id: Option<String>,
}

/// NIP-98-authenticated HTTP client for the authoritative Buzz relay.
pub struct BuzzGatewayClient {
    /// The workload's service key; every request is signed with it.
    keys: Keys,
    http: Client,
    timeout: Duration,
    allow_private_targets: bool,
}

impl BuzzGatewayClient {
    /// Build a client with the default [`DEFAULT_TIMEOUT`].
    ///
    /// The `http` builder is used with redirects disabled: a hostile relay
    /// must not be able to coerce the client into following a redirect to an
    /// unconfigured host (SSRF protection), so the client talks only to the
    /// configured relay URL. The only failure mode is an invalid builder,
    /// reported as a configuration error.
    pub fn new(keys: Keys, http: reqwest::ClientBuilder) -> Result<Self, BuzzAuthorityError> {
        Self::with_timeout(keys, http, DEFAULT_TIMEOUT)
    }

    /// Build a client with an explicit per-request timeout.
    pub fn with_timeout(
        keys: Keys,
        http: reqwest::ClientBuilder,
        timeout: Duration,
    ) -> Result<Self, BuzzAuthorityError> {
        Self::with_timeout_policy(keys, http, timeout, false)
    }

    fn with_timeout_policy(
        keys: Keys,
        http: reqwest::ClientBuilder,
        timeout: Duration,
        allow_private_targets: bool,
    ) -> Result<Self, BuzzAuthorityError> {
        let http = http.redirect(reqwest::redirect::Policy::none());
        let client = http
            .build()
            .map_err(|e| BuzzAuthorityError::Config(format!("cannot build HTTP client: {e}")))?;
        Ok(Self {
            keys,
            http: client,
            timeout,
            allow_private_targets,
        })
    }

    /// Build a client for local relay test doubles. Production callers must
    /// use [`Self::new`] or [`Self::with_timeout`], which always enforce the
    /// public-target SSRF policy.
    #[cfg(any(test, debug_assertions))]
    #[doc(hidden)]
    pub fn new_for_test(
        keys: Keys,
        http: reqwest::ClientBuilder,
    ) -> Result<Self, BuzzAuthorityError> {
        Self::with_timeout_policy(keys, http, DEFAULT_TIMEOUT, true)
    }

    /// Ask the relay whether `req` may currently read its channel/message.
    ///
    /// The request is NIP-98-signed (POST ⇒ `payload` tag), and the signed
    /// kind-19030 response is verified (kind, Schnorr signature, pinned
    /// pubkey), checked for request echo and freshness, then mapped to a
    /// [`BuzzReadDecision`]. Non-200 statuses fail closed: 401 becomes
    /// [`BuzzAuthorityError::Unauthorized`]; other 4xx/5xx become
    /// [`BuzzAuthorityError::Transport`] — the relay was reachable but did not
    /// answer with a signed decision, which is the same Deny outcome as a
    /// transport failure and is kept as `Transport` rather than
    /// `InvalidResponse` because the failure is in the request/status, not in
    /// a signed response we received.
    pub async fn check_access(
        &self,
        relay_url: &str,
        relay_pubkey: &str,
        req: &BuzzAccessCheckRequest,
    ) -> Result<BuzzReadDecision, BuzzAuthorityError> {
        let (base, http) = self.validated_http(relay_url).await?;
        let url = base.join("/api/v1/relay/access/check").map_err(|e| {
            BuzzAuthorityError::Config(format!("cannot build relay access-check URL: {e}"))
        })?;
        let body = serde_json::to_vec(req).map_err(|e| {
            BuzzAuthorityError::Config(format!("cannot serialize access-check request: {e}"))
        })?;
        let header = self.nip98_header("POST", &url, Some(&body)).await?;
        let response = http
            .post(url)
            .timeout(self.timeout)
            .header("Authorization", header)
            .header("Content-Type", "application/json")
            .body(body)
            .send()
            .await
            .map_err(|e| {
                log_relay_error(relay_url, BuzzAuthorityError::Transport(e.to_string()))
            })?;
        let raw = read_response_json(response)
            .await
            .map_err(|e| log_relay_error(relay_url, e))?;
        self.decision_from_19030(&raw, relay_pubkey, req)
            .map_err(|e| log_relay_error(relay_url, e))
    }

    /// Page the relay's signed event state for reconciliation.
    ///
    /// The response envelope is verified exactly like an access-check response
    /// (kind 19030, Schnorr signature, pinned pubkey), then parsed as a
    /// [`BuzzStatePage`]. Individual entry events are NOT verified here — the
    /// reconcile consumer verifies each kind-1 event. The page envelope IS
    /// validated: an incomplete page must carry a continuation cursor, or the
    /// response is malformed and fails closed.
    pub async fn page_state(
        &self,
        relay_url: &str,
        relay_pubkey: &str,
        since: Option<i64>,
        limit: u32,
        cursor: Option<&str>,
    ) -> Result<BuzzStatePage, BuzzAuthorityError> {
        // Clamp the page size client-side before building the query so a
        // caller cannot ask the relay for an unbounded page.
        let limit = limit.clamp(1, 500);
        let (base, http) = self.validated_http(relay_url).await?;
        let mut url = base.join("/api/v1/relay/state/events").map_err(|e| {
            BuzzAuthorityError::Config(format!("cannot build relay state-events URL: {e}"))
        })?;
        {
            let mut query = url.query_pairs_mut();
            if let Some(since) = since {
                query.append_pair("since", &since.to_string());
            }
            query.append_pair("limit", &limit.to_string());
            if let Some(cursor) = cursor {
                query.append_pair("cursor", cursor);
            }
        }
        let header = self.nip98_header("GET", &url, None).await?;
        let response = http
            .get(url)
            .timeout(self.timeout)
            .header("Authorization", header)
            .send()
            .await
            .map_err(|e| {
                log_relay_error(relay_url, BuzzAuthorityError::Transport(e.to_string()))
            })?;
        let raw = read_response_json(response)
            .await
            .map_err(|e| log_relay_error(relay_url, e))?;
        let event = self
            .verify_19030(&raw, relay_pubkey)
            .map_err(|e| log_relay_error(relay_url, e))?;
        let page: BuzzStatePage = serde_json::from_str(&event.content).map_err(|e| {
            log_relay_error(
                relay_url,
                BuzzAuthorityError::InvalidResponse(format!("state page content is invalid: {e}")),
            )
        })?;
        validate_page(&page).map_err(|e| log_relay_error(relay_url, e))?;
        Ok(page)
    }

    /// Derive the HTTP base URL from the stored websocket `relay_url`.
    ///
    /// `ws://` → `http://` and `wss://` → `https://`, keeping host and port
    /// unchanged. Any other scheme is a configuration error — the community
    /// mapping must store a ws/wss relay URL. The path is normalized to `/`;
    /// the API endpoints are appended with a leading-slash join, so any path
    /// on the stored URL is not carried over.
    fn http_base(relay_url: &str) -> Result<Url, BuzzAuthorityError> {
        let parsed = Url::parse(relay_url).map_err(|e| {
            BuzzAuthorityError::Config(format!("invalid relay_url {relay_url:?}: {e}"))
        })?;
        if !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
        {
            return Err(BuzzAuthorityError::Config(
                "relay_url must not contain credentials, query parameters, or fragments".into(),
            ));
        }
        let scheme = match parsed.scheme() {
            "ws" => "http",
            "wss" => "https",
            other => {
                return Err(BuzzAuthorityError::Config(format!(
                    "relay_url uses unsupported scheme {other:?} (expected ws or wss)"
                )))
            }
        };
        let mut base = parsed;
        base.set_scheme(scheme).map_err(|_| {
            BuzzAuthorityError::Config(format!("cannot map relay_url scheme to {scheme}"))
        })?;
        base.set_path("/");
        base.set_query(None);
        base.set_fragment(None);
        Ok(base)
    }

    async fn validated_http(
        &self,
        relay_url: &str,
    ) -> Result<(Url, reqwest::Client), BuzzAuthorityError> {
        let base = Self::http_base(relay_url)?;
        if self.allow_private_targets {
            return Ok((base, self.http.clone()));
        }
        let host = base
            .host_str()
            .ok_or_else(|| BuzzAuthorityError::Config("relay_url must include a host".into()))?;
        let port = base.port_or_known_default().ok_or_else(|| {
            BuzzAuthorityError::Config("relay_url must include a valid port".into())
        })?;
        let addrs = resolve_public_socket_addrs(host, port).await.map_err(|_| {
            BuzzAuthorityError::Config("relay target failed SSRF validation".into())
        })?;
        let http = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .resolve_to_addrs(host, &addrs)
            .build()
            .map_err(|e| BuzzAuthorityError::Config(format!("cannot build HTTP client: {e}")))?;
        Ok((base, http))
    }

    /// Build the NIP-98 `Authorization` header value for `method`/`url`.
    ///
    /// The `u` tag carries the exact request URL (query string included), the
    /// `method` tag the HTTP method, and — when a body is present — a `payload`
    /// tag with `hex(sha256(body))`. The kind-27235 event is signed with the
    /// service key and base64-encoded. NIP-98 leaves the concrete base64
    /// variant open in the wild; this crate's own verifier
    /// (`nip98::verify_auth_header`) decodes standard base64 (padded), so we
    /// encode exactly like `nostr`'s own `HttpData::to_authorization` to keep
    /// headers produced here verifiable by it and by relays/test doubles built
    /// on the same crate.
    async fn nip98_header(
        &self,
        method: &str,
        url: &Url,
        body: Option<&[u8]>,
    ) -> Result<String, BuzzAuthorityError> {
        let mut tags = vec![
            Tag::parse(["u", url.as_str()]).map_err(|e| {
                BuzzAuthorityError::Config(format!("cannot build NIP-98 u tag: {e}"))
            })?,
            Tag::parse(["method", method]).map_err(|e| {
                BuzzAuthorityError::Config(format!("cannot build NIP-98 method tag: {e}"))
            })?,
        ];
        if let Some(body) = body {
            let digest = hex::encode(Sha256::digest(body));
            tags.push(Tag::parse(["payload", digest.as_str()]).map_err(|e| {
                BuzzAuthorityError::Config(format!("cannot build NIP-98 payload tag: {e}"))
            })?);
        }
        let event = EventBuilder::new(Kind::HttpAuth, String::new())
            .tags(tags)
            .sign_with_keys(&self.keys)
            .map_err(|e| {
                BuzzAuthorityError::Config(format!("cannot sign NIP-98 auth event: {e}"))
            })?;
        Ok(format!("Nostr {}", STANDARD.encode(event.as_json())))
    }

    /// Parse and cryptographically verify a relay response envelope.
    ///
    /// The response must be a valid Nostr event of kind 19030 whose Schnorr
    /// signature verifies and whose `pubkey` equals the pinned relay pubkey.
    /// Any mismatch is an invalid response (fail closed).
    fn verify_19030(
        &self,
        raw: &Value,
        relay_pubkey: &str,
    ) -> Result<NostrEvent, BuzzAuthorityError> {
        let event = NostrEvent::from_json(raw.to_string()).map_err(|e| {
            BuzzAuthorityError::InvalidResponse(format!("response is not a valid Nostr event: {e}"))
        })?;
        if event.kind.as_u16() != RELAY_RESPONSE_KIND {
            return Err(BuzzAuthorityError::InvalidResponse(format!(
                "response kind {} is not {RELAY_RESPONSE_KIND}",
                event.kind.as_u16()
            )));
        }
        event.verify().map_err(|e| {
            BuzzAuthorityError::InvalidResponse(format!(
                "response signature verification failed: {e}"
            ))
        })?;
        if event.pubkey.to_hex() != relay_pubkey {
            return Err(BuzzAuthorityError::InvalidResponse(
                "response pubkey does not match the pinned relay pubkey".to_string(),
            ));
        }
        Ok(event)
    }

    /// Verify a kind-19030 access-check response and map it to a read decision.
    ///
    /// Beyond [`Self::verify_19030`], the response content must parse as a
    /// [`BuzzAccessCheckResult`] whose echoed `pubkey`/`channel_id`/`message_id`
    /// match the request verbatim and whose `evaluated_at` is within
    /// [`MAX_EVALUATED_AT_AGE_SECS`] of the client clock. `allow`/`deny`/
    /// `not_found` map to [`BuzzReadDecision`]; anything else fails closed as
    /// an invalid response.
    fn decision_from_19030(
        &self,
        raw: &Value,
        relay_pubkey: &str,
        expected: &BuzzAccessCheckRequest,
    ) -> Result<BuzzReadDecision, BuzzAuthorityError> {
        let event = self.verify_19030(raw, relay_pubkey)?;
        let result: BuzzAccessCheckResult = serde_json::from_str(&event.content).map_err(|e| {
            BuzzAuthorityError::InvalidResponse(format!(
                "response content is not an access-check result: {e}"
            ))
        })?;
        if !is_fresh(result.evaluated_at) {
            let age = Utc::now()
                .timestamp()
                .saturating_sub(result.evaluated_at)
                .unsigned_abs();
            return Err(BuzzAuthorityError::InvalidResponse(format!(
                "response evaluated_at is {age}s from the client clock (max {MAX_EVALUATED_AT_AGE_SECS}s)"
            )));
        }
        Self::decision_from_result(&result, expected)
    }

    /// Map one access-check result item to a read decision, verifying the
    /// echoed values against the request.
    ///
    /// Shared by the single-check path and the batch path: `pubkey`,
    /// `channel_id`, and `message_id` must echo the request verbatim and the
    /// decision string must be `allow`/`deny`/`not_found`. Freshness is NOT
    /// checked here — the single-check path checks its own `evaluated_at`,
    /// while batch mode relies on the envelope-level `evaluated_at` (the
    /// item-level one is informational and never independently checked).
    fn decision_from_result(
        result: &BuzzAccessCheckResult,
        expected: &BuzzAccessCheckRequest,
    ) -> Result<BuzzReadDecision, BuzzAuthorityError> {
        if result.pubkey != expected.pubkey {
            return Err(BuzzAuthorityError::InvalidResponse(
                "response pubkey does not echo the requested pubkey".to_string(),
            ));
        }
        if result.channel_id != expected.channel_id {
            return Err(BuzzAuthorityError::InvalidResponse(
                "response channel_id does not echo the requested channel_id".to_string(),
            ));
        }
        if result.message_id != expected.message_id {
            return Err(BuzzAuthorityError::InvalidResponse(
                "response message_id does not echo the requested message_id".to_string(),
            ));
        }
        match result.decision.as_str() {
            "allow" => Ok(BuzzReadDecision::Allow),
            "deny" => Ok(BuzzReadDecision::Deny),
            "not_found" => Ok(BuzzReadDecision::NotFound),
            other => Err(BuzzAuthorityError::InvalidResponse(format!(
                "response decision {other:?} is not allow/deny/not_found"
            ))),
        }
    }

    /// Ask the relay for access decisions on many channel/message checks in
    /// as few round-trips as possible
    /// (`POST /api/v1/relay/access/check-batch`).
    ///
    /// Requests are grouped by `(relay_url, relay_pubkey)` — one batch
    /// round-trip per pinned relay — and a group larger than
    /// [`MAX_BATCH_CHECKS`] is split into sequential round-trips. The
    /// kind-19030 envelope is verified once per round-trip (kind, Schnorr
    /// signature, pinned pubkey, top-level `evaluated_at` freshness); an
    /// envelope failure fails every item of that round-trip closed. Per-item
    /// failures (echo mismatch, unknown decision, unparseable item) are
    /// isolated to that item. Results are aligned with the input order
    /// across groups and round-trips.
    pub async fn check_access_batch(
        &self,
        reqs: &[BuzzReadRequest],
    ) -> Vec<Result<BuzzReadDecision, BuzzAuthorityError>> {
        // One result slot per input; filled as groups and round-trips finish.
        let mut results: Vec<Option<Result<BuzzReadDecision, BuzzAuthorityError>>> =
            (0..reqs.len()).map(|_| None).collect();
        // Group indices by (relay_url, relay_pubkey), first-seen order.
        let mut groups: Vec<(String, String, Vec<usize>)> = Vec::new();
        for (index, req) in reqs.iter().enumerate() {
            let Some(relay_pubkey) = req.relay_pubkey.as_deref() else {
                results[index] = Some(Err(BuzzAuthorityError::Config(
                    "community mapping has no pinned relay_pubkey".to_string(),
                )));
                continue;
            };
            match groups
                .iter_mut()
                .find(|(url, pubkey, _)| url == &req.relay_url && pubkey == relay_pubkey)
            {
                Some((_, _, indices)) => indices.push(index),
                None => groups.push((req.relay_url.clone(), relay_pubkey.to_string(), vec![index])),
            }
        }
        for (relay_url, relay_pubkey, indices) in groups {
            let group: Vec<&BuzzReadRequest> = indices.iter().map(|&index| &reqs[index]).collect();
            let decisions = self
                .check_access_batch_for_relay(&relay_url, &relay_pubkey, &group)
                .await;
            for (index, decision) in indices.into_iter().zip(decisions) {
                results[index] = Some(decision);
            }
        }
        results
            .into_iter()
            .map(|result| result.expect("every batch index is filled"))
            .collect()
    }

    /// Drive one pinned relay's batch round-trips (≤ [`MAX_BATCH_CHECKS`] per
    /// round-trip, issued sequentially).
    async fn check_access_batch_for_relay(
        &self,
        relay_url: &str,
        relay_pubkey: &str,
        reqs: &[&BuzzReadRequest],
    ) -> Vec<Result<BuzzReadDecision, BuzzAuthorityError>> {
        if reqs.is_empty() {
            return Vec::new();
        }
        let (base, http) = match self.validated_http(relay_url).await {
            Ok(pair) => pair,
            Err(error) => return vec![Err(error); reqs.len()],
        };
        let url = match base.join("/api/v1/relay/access/check-batch") {
            Ok(url) => url,
            Err(error) => {
                return vec![
                    Err(BuzzAuthorityError::Config(format!(
                        "cannot build relay batch access-check URL: {error}"
                    )));
                    reqs.len()
                ]
            }
        };
        let mut results = Vec::with_capacity(reqs.len());
        for chunk in reqs.chunks(MAX_BATCH_CHECKS) {
            let checks: Vec<BuzzAccessCheckRequest> =
                chunk.iter().map(|req| access_check_request(req)).collect();
            let body = match serde_json::to_vec(&serde_json::json!({ "checks": checks })) {
                Ok(body) => body,
                Err(error) => {
                    let error = BuzzAuthorityError::Config(format!(
                        "cannot serialize batch access-check request: {error}"
                    ));
                    results.extend(chunk.iter().map(|_| Err(error.clone())));
                    continue;
                }
            };
            let header = match self.nip98_header("POST", &url, Some(&body)).await {
                Ok(header) => header,
                Err(error) => {
                    results.extend(chunk.iter().map(|_| Err(error.clone())));
                    continue;
                }
            };
            let response = match http
                .post(url.clone())
                .timeout(self.timeout)
                .header("Authorization", header)
                .header("Content-Type", "application/json")
                .body(body)
                .send()
                .await
            {
                Ok(response) => response,
                Err(error) => {
                    let error = BuzzAuthorityError::Transport(error.to_string());
                    results.extend(
                        chunk
                            .iter()
                            .map(|_| Err(log_relay_error(relay_url, error.clone()))),
                    );
                    continue;
                }
            };
            let raw = match read_response_json(response).await {
                Ok(raw) => raw,
                Err(error) => {
                    results.extend(
                        chunk
                            .iter()
                            .map(|_| Err(log_relay_error(relay_url, error.clone()))),
                    );
                    continue;
                }
            };
            match self.batch_decision_from_19030(&raw, relay_pubkey, chunk) {
                Ok(decisions) => results.extend(decisions),
                Err(error) => results.extend(
                    chunk
                        .iter()
                        .map(|_| Err(log_relay_error(relay_url, error.clone()))),
                ),
            }
        }
        results
    }

    /// Verify one batch round-trip's kind-19030 envelope and map every item.
    ///
    /// Envelope verification (kind, Schnorr signature, pinned pubkey,
    /// top-level `evaluated_at` freshness, `results`/`checks` length parity)
    /// applies once; any envelope failure fails EVERY item closed. Per-item
    /// failures are isolated to that item.
    fn batch_decision_from_19030(
        &self,
        raw: &Value,
        relay_pubkey: &str,
        reqs: &[&BuzzReadRequest],
    ) -> Result<Vec<Result<BuzzReadDecision, BuzzAuthorityError>>, BuzzAuthorityError> {
        let event = self.verify_19030(raw, relay_pubkey)?;
        let batch: BuzzAccessCheckBatchResponse =
            serde_json::from_str(&event.content).map_err(|e| {
                BuzzAuthorityError::InvalidResponse(format!(
                    "response content is not a batch access-check result: {e}"
                ))
            })?;
        if batch.results.len() != reqs.len() {
            return Err(BuzzAuthorityError::InvalidResponse(format!(
                "batch result count {} does not match request count {}",
                batch.results.len(),
                reqs.len()
            )));
        }
        if !is_fresh(batch.evaluated_at) {
            return Err(BuzzAuthorityError::InvalidResponse(
                "batch evaluated_at is stale".to_string(),
            ));
        }
        Ok(reqs
            .iter()
            .zip(batch.results)
            .map(|(req, result)| {
                let expected = access_check_request(req);
                Self::decision_from_result(&result, &expected)
            })
            .collect())
    }

    /// Ask the relay which channels `pubkey` may currently read — the
    /// authoritative channel registry
    /// (`GET /api/v1/relay/channels?pubkey=<64hex>`).
    ///
    /// The NIP-98 GET binds the exact request URL including the `pubkey`
    /// query string (no payload tag for GETs). The kind-19030 envelope is
    /// verified (kind, Schnorr signature, pinned pubkey) and the content's
    /// `pubkey` echo and top-level `evaluated_at` freshness are enforced. Only
    /// channels the pubkey may read are ever listed (member channels,
    /// including private ones, plus open channels), each with its `member`
    /// flag.
    pub async fn list_channels(
        &self,
        relay_url: &str,
        relay_pubkey: &str,
        pubkey: &str,
    ) -> Result<Vec<BuzzChannelInfo>, BuzzAuthorityError> {
        let (base, http) = self.validated_http(relay_url).await?;
        let mut url = base.join("/api/v1/relay/channels").map_err(|e| {
            BuzzAuthorityError::Config(format!("cannot build relay channels URL: {e}"))
        })?;
        url.query_pairs_mut().append_pair("pubkey", pubkey);
        let header = self.nip98_header("GET", &url, None).await?;
        let response = http
            .get(url)
            .timeout(self.timeout)
            .header("Authorization", header)
            .send()
            .await
            .map_err(|e| {
                log_relay_error(relay_url, BuzzAuthorityError::Transport(e.to_string()))
            })?;
        let raw = read_response_json(response)
            .await
            .map_err(|e| log_relay_error(relay_url, e))?;
        self.registry_from_19030(&raw, relay_pubkey, pubkey)
            .map_err(|e| log_relay_error(relay_url, e))
    }

    /// Verify a kind-19030 channel-registry response and parse the channel
    /// list: envelope (kind, Schnorr signature, pinned pubkey), content
    /// `pubkey` echo, and top-level `evaluated_at` freshness.
    fn registry_from_19030(
        &self,
        raw: &Value,
        relay_pubkey: &str,
        expected_pubkey: &str,
    ) -> Result<Vec<BuzzChannelInfo>, BuzzAuthorityError> {
        let event = self.verify_19030(raw, relay_pubkey)?;
        let registry: BuzzChannelRegistry = serde_json::from_str(&event.content).map_err(|e| {
            BuzzAuthorityError::InvalidResponse(format!("channel registry content is invalid: {e}"))
        })?;
        if registry.pubkey != expected_pubkey {
            return Err(BuzzAuthorityError::InvalidResponse(
                "channel registry pubkey does not echo the requested pubkey".to_string(),
            ));
        }
        if !is_fresh(registry.evaluated_at) {
            return Err(BuzzAuthorityError::InvalidResponse(
                "channel registry evaluated_at is stale".to_string(),
            ));
        }
        Ok(registry.channels)
    }
}

#[async_trait]
impl BuzzAuthority for BuzzGatewayClient {
    async fn can_read(
        &self,
        req: &BuzzReadRequest,
    ) -> Result<BuzzReadDecision, BuzzAuthorityError> {
        let relay_pubkey = req.relay_pubkey.as_deref().ok_or_else(|| {
            BuzzAuthorityError::Config("community mapping has no pinned relay_pubkey".to_string())
        })?;
        self.check_access(&req.relay_url, relay_pubkey, &access_check_request(req))
            .await
    }

    async fn can_read_batch(
        &self,
        reqs: &[BuzzReadRequest],
    ) -> Vec<Result<BuzzReadDecision, BuzzAuthorityError>> {
        self.check_access_batch(reqs).await
    }
}

/// Shared-client authority handle: the same [`Arc`] gateway instance stored
/// in `AppState`, presented as a [`BuzzAuthority`].
///
/// The orphan rule rejects `impl BuzzAuthority for Arc<BuzzGatewayClient>`
/// (and the generic `Arc<T>` form) — `Arc` is a foreign type constructor and
/// the self type must be crate-local — so the shared `Arc` is wrapped in a
/// crate-local newtype.
pub struct BuzzGatewayAuthority(pub Arc<BuzzGatewayClient>);

#[async_trait]
impl BuzzAuthority for BuzzGatewayAuthority {
    async fn can_read(
        &self,
        req: &BuzzReadRequest,
    ) -> Result<BuzzReadDecision, BuzzAuthorityError> {
        self.0.can_read(req).await
    }

    async fn can_read_batch(
        &self,
        reqs: &[BuzzReadRequest],
    ) -> Vec<Result<BuzzReadDecision, BuzzAuthorityError>> {
        self.0.can_read_batch(reqs).await
    }
}

/// Map a non-2xx relay status to a fail-closed error; `None` for success.
fn status_error(status: StatusCode) -> Option<BuzzAuthorityError> {
    match status {
        StatusCode::UNAUTHORIZED => Some(BuzzAuthorityError::Unauthorized),
        s if s.is_client_error() || s.is_server_error() || s.is_redirection() => Some(
            BuzzAuthorityError::Transport(format!("relay returned HTTP {s}")),
        ),
        _ => None,
    }
}

/// Convert a domain read request into the wire access-check shape.
fn access_check_request(req: &BuzzReadRequest) -> BuzzAccessCheckRequest {
    BuzzAccessCheckRequest {
        pubkey: req.pubkey.clone(),
        channel_id: req.channel_id.clone(),
        channel_kind: req.channel_kind,
        message_id: req.message_id.clone(),
        event_created_at: Some(req.event_created_at.timestamp()),
    }
}

/// Whether an `evaluated_at` timestamp is within the freshness window of the
/// client clock. Saturating arithmetic: `evaluated_at` is relay-controlled
/// and may be `i64::MIN`/`i64::MAX`, which would panic a plain
/// subtraction/`abs()` pair — hostile extremes must fail closed, never panic.
fn is_fresh(evaluated_at: i64) -> bool {
    Utc::now()
        .timestamp()
        .saturating_sub(evaluated_at)
        .unsigned_abs()
        <= MAX_EVALUATED_AT_AGE_SECS
}

/// Ops visibility: log a fail-closed gateway outcome with the relay URL before
/// it propagates. The request body and pubkey are deliberately not logged.
fn log_relay_error(relay_url: &str, error: BuzzAuthorityError) -> BuzzAuthorityError {
    if matches!(
        error,
        BuzzAuthorityError::Transport(_) | BuzzAuthorityError::InvalidResponse(_)
    ) {
        warn!(relay_url = %relay_url, error = %error, "Buzz gateway request failed closed");
    }
    error
}

/// Read the raw response body, failing closed on any non-2xx status.
///
/// The body is read as a bounded stream: a relay response larger than
/// [`MAX_RESPONSE_BYTES`] is treated as invalid rather than buffered without
/// limit, so a hostile relay cannot exhaust server memory. Chunk-level stream
/// failures map to [`BuzzAuthorityError::Transport`].
async fn read_response_json(response: Response) -> Result<Value, BuzzAuthorityError> {
    if let Some(error) = status_error(response.status()) {
        return Err(error);
    }
    let mut stream = response.bytes_stream();
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| BuzzAuthorityError::Transport(e.to_string()))?;
        if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
            return Err(BuzzAuthorityError::InvalidResponse(
                "response body exceeds size cap".to_string(),
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes)
        .map_err(|e| BuzzAuthorityError::InvalidResponse(format!("response body is not JSON: {e}")))
}

/// Validate the state-page envelope: an incomplete page MUST carry a
/// continuation cursor; `cursor: null` with `complete: false` is malformed and
/// fails closed.
fn validate_page(page: &BuzzStatePage) -> Result<(), BuzzAuthorityError> {
    if !page.complete && page.cursor.is_none() {
        return Err(BuzzAuthorityError::InvalidResponse(
            "state page is incomplete but carries no continuation cursor".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::nips::nip98::{verify_auth_header, HttpMethod};
    use nostr::Timestamp;
    use rustshare_core::domain::TenantId;
    use serde_json::json;
    use uuid::Uuid;

    /// A 64-char lowercase hex string (pubkey/event-id shaped).
    const HEX64: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn client(keys: Keys) -> BuzzGatewayClient {
        BuzzGatewayClient::new(keys, Client::builder()).expect("build test client")
    }

    /// Build a signed kind-19030 response event from the relay key.
    fn relay_19030(keys: &Keys, content: Value) -> NostrEvent {
        EventBuilder::new(Kind::from(RELAY_RESPONSE_KIND), content.to_string())
            .sign_with_keys(keys)
            .expect("sign the relay response")
    }

    #[tokio::test]
    async fn nip98_header_round_trips_with_verify_auth_header() {
        let keys = Keys::generate();
        let service = client(keys.clone());
        let post_url = Url::parse("https://chat.example.test/api/v1/relay/access/check").unwrap();

        // POST: the header carries a payload tag matching the body and verifies
        // as the service identity.
        let body = serde_json::to_vec(&BuzzAccessCheckRequest {
            pubkey: HEX64.to_string(),
            channel_id: "channel-1".to_string(),
            channel_kind: BuzzChannelKind::Workspace,
            message_id: Some(HEX64.to_string()),
            event_created_at: Some(1_750_000_000),
        })
        .unwrap();
        let header = service
            .nip98_header("POST", &post_url, Some(&body))
            .await
            .unwrap();
        let verified = verify_auth_header(
            &header,
            &post_url,
            HttpMethod::POST,
            Timestamp::now(),
            Some(&body),
        )
        .expect("POST header must verify");
        assert_eq!(verified, keys.public_key());

        // GET with a query string: the u tag must carry the exact URL including
        // the query, and no payload tag is required.
        let get_url = Url::parse(
            "https://chat.example.test/api/v1/relay/state/events?since=1750000000&limit=100&cursor=abc123",
        )
        .unwrap();
        let header = service.nip98_header("GET", &get_url, None).await.unwrap();
        let verified =
            verify_auth_header(&header, &get_url, HttpMethod::GET, Timestamp::now(), None)
                .expect("GET header must verify");
        assert_eq!(verified, keys.public_key());
    }

    #[test]
    fn http_base_swaps_ws_and_wss_schemes() {
        let http = BuzzGatewayClient::http_base("ws://chat.example.test:8080").unwrap();
        assert_eq!(http.as_str(), "http://chat.example.test:8080/");
        let https = BuzzGatewayClient::http_base("wss://chat.example.test:8443").unwrap();
        assert_eq!(https.as_str(), "https://chat.example.test:8443/");
        let no_port = BuzzGatewayClient::http_base("wss://chat.example.test").unwrap();
        assert_eq!(no_port.as_str(), "https://chat.example.test/");
        // Non-ws/wss schemes and unparseable URLs are configuration errors.
        assert!(matches!(
            BuzzGatewayClient::http_base("https://chat.example.test"),
            Err(BuzzAuthorityError::Config(_))
        ));
        assert!(matches!(
            BuzzGatewayClient::http_base("not a url"),
            Err(BuzzAuthorityError::Config(_))
        ));
        for relay_url in [
            "wss://user:secret@chat.example.test",
            "wss://chat.example.test?token=secret",
            "wss://chat.example.test/#secret",
        ] {
            assert!(matches!(
                BuzzGatewayClient::http_base(relay_url),
                Err(BuzzAuthorityError::Config(_))
            ));
        }
    }

    #[test]
    fn verify_19030_accepts_relay_signed_response_and_rejects_wrong_key_wrong_kind() {
        let relay = Keys::generate();
        let relay_pubkey = relay.public_key().to_hex();
        let service = client(Keys::generate());

        // A properly signed kind-19030 response verifies and yields the relay
        // pubkey.
        let raw = serde_json::to_value(relay_19030(
            &relay,
            json!({"decision": "allow", "reason": "member", "evaluated_at": 0}),
        ))
        .unwrap();
        let event = service.verify_19030(&raw, &relay_pubkey).unwrap();
        assert_eq!(event.pubkey, relay.public_key());
        assert_eq!(event.kind.as_u16(), RELAY_RESPONSE_KIND);

        // A valid kind-19030 signed by a DIFFERENT key is rejected.
        let raw = serde_json::to_value(relay_19030(
            &Keys::generate(),
            json!({"decision": "allow", "reason": "member", "evaluated_at": 0}),
        ))
        .unwrap();
        assert!(matches!(
            service.verify_19030(&raw, &relay_pubkey),
            Err(BuzzAuthorityError::InvalidResponse(_))
        ));

        // A valid kind-1 text note from the relay itself is rejected on kind.
        let note = EventBuilder::text_note("hello")
            .sign_with_keys(&relay)
            .unwrap();
        let raw = serde_json::to_value(&note).unwrap();
        assert!(matches!(
            service.verify_19030(&raw, &relay_pubkey),
            Err(BuzzAuthorityError::InvalidResponse(_))
        ));

        // Unparseable input is an invalid response.
        assert!(matches!(
            service.verify_19030(&Value::Null, &relay_pubkey),
            Err(BuzzAuthorityError::InvalidResponse(_))
        ));
    }

    #[test]
    fn decision_from_19030_maps_decisions_and_rejects_replay_and_echo_mismatch() {
        let relay = Keys::generate();
        let relay_pubkey = relay.public_key().to_hex();
        let service = client(Keys::generate());
        let expected = BuzzAccessCheckRequest {
            pubkey: HEX64.to_string(),
            channel_id: "channel-1".to_string(),
            channel_kind: BuzzChannelKind::Workspace,
            message_id: Some(HEX64.to_string()),
            event_created_at: Some(1_750_000_000),
        };
        let now = Utc::now().timestamp();

        fn response_json(
            decision: &str,
            evaluated_at: i64,
            expected: &BuzzAccessCheckRequest,
        ) -> Value {
            json!({
                "decision": decision,
                "reason": "member",
                "evaluated_at": evaluated_at,
                "pubkey": expected.pubkey,
                "channel_id": expected.channel_id,
                "message_id": expected.message_id,
            })
        }

        fn checked(
            service: &BuzzGatewayClient,
            relay: &Keys,
            relay_pubkey: &str,
            expected: &BuzzAccessCheckRequest,
            content: Value,
        ) -> Result<BuzzReadDecision, BuzzAuthorityError> {
            let raw = serde_json::to_value(relay_19030(relay, content)).unwrap();
            service.decision_from_19030(&raw, relay_pubkey, expected)
        }

        // Fresh, correctly-echoed responses map allow/deny/not_found to their
        // decisions (evaluated_at == now is the fresh control).
        for (decision, expected_decision) in [
            ("allow", BuzzReadDecision::Allow),
            ("deny", BuzzReadDecision::Deny),
            ("not_found", BuzzReadDecision::NotFound),
        ] {
            assert_eq!(
                checked(
                    &service,
                    &relay,
                    &relay_pubkey,
                    &expected,
                    response_json(decision, now, &expected)
                )
                .unwrap(),
                expected_decision
            );
        }

        // Unknown decision strings fail closed.
        let content = response_json("maybe", now, &expected);
        assert!(matches!(
            checked(&service, &relay, &relay_pubkey, &expected, content),
            Err(BuzzAuthorityError::InvalidResponse(_))
        ));

        // Echo mismatch: different pubkey.
        let mut content = response_json("allow", now, &expected);
        content["pubkey"] = Value::String("f".repeat(64));
        assert!(matches!(
            checked(&service, &relay, &relay_pubkey, &expected, content),
            Err(BuzzAuthorityError::InvalidResponse(_))
        ));

        // Echo mismatch: different channel_id.
        let mut content = response_json("allow", now, &expected);
        content["channel_id"] = Value::String("channel-2".to_string());
        assert!(matches!(
            checked(&service, &relay, &relay_pubkey, &expected, content),
            Err(BuzzAuthorityError::InvalidResponse(_))
        ));

        // Echo mismatch: message_id dropped (request had one).
        let mut content = response_json("allow", now, &expected);
        content["message_id"] = Value::Null;
        assert!(matches!(
            checked(&service, &relay, &relay_pubkey, &expected, content),
            Err(BuzzAuthorityError::InvalidResponse(_))
        ));

        // Stale response: evaluated_at 120s in the past.
        let content = response_json("allow", now - 120, &expected);
        assert!(matches!(
            checked(&service, &relay, &relay_pubkey, &expected, content),
            Err(BuzzAuthorityError::InvalidResponse(_))
        ));

        // Stale response: evaluated_at 120s in the future.
        let content = response_json("allow", now + 120, &expected);
        assert!(matches!(
            checked(&service, &relay, &relay_pubkey, &expected, content),
            Err(BuzzAuthorityError::InvalidResponse(_))
        ));

        // Hostile evaluated_at: i64::MIN (and i64::MAX) must fail closed — the
        // freshness arithmetic must not panic on relay-controlled extremes.
        let content = response_json("allow", i64::MIN, &expected);
        assert!(matches!(
            checked(&service, &relay, &relay_pubkey, &expected, content),
            Err(BuzzAuthorityError::InvalidResponse(_))
        ));
        let content = response_json("allow", i64::MAX, &expected);
        assert!(matches!(
            checked(&service, &relay, &relay_pubkey, &expected, content),
            Err(BuzzAuthorityError::InvalidResponse(_))
        ));
    }

    #[tokio::test]
    async fn can_read_without_relay_pubkey_fails_closed() {
        let service = client(Keys::generate());
        let request = BuzzReadRequest {
            tenant_id: TenantId(Uuid::new_v4()),
            community_id: "community-1".to_string(),
            relay_url: "wss://chat.example.test".to_string(),
            relay_pubkey: None,
            channel_id: "channel-1".to_string(),
            channel_kind: BuzzChannelKind::Workspace,
            message_id: Some(HEX64.to_string()),
            pubkey: HEX64.to_string(),
            event_created_at: Utc::now(),
        };
        assert!(matches!(
            service.can_read(&request).await,
            Err(BuzzAuthorityError::Config(_))
        ));
    }

    /// A domain read request pinned to `relay_pubkey` and `relay_url`.
    fn read_request(
        relay_url: &str,
        relay_pubkey: &str,
        channel_id: &str,
        pubkey: &str,
    ) -> BuzzReadRequest {
        BuzzReadRequest {
            tenant_id: TenantId(Uuid::new_v4()),
            community_id: "community-1".to_string(),
            relay_url: relay_url.to_string(),
            relay_pubkey: Some(relay_pubkey.to_string()),
            channel_id: channel_id.to_string(),
            channel_kind: BuzzChannelKind::Workspace,
            message_id: Some(HEX64.to_string()),
            pubkey: pubkey.to_string(),
            event_created_at: Utc::now(),
        }
    }

    #[test]
    fn batch_decision_from_19030_isolates_item_failures_and_fails_envelope_closed() {
        let relay = Keys::generate();
        let relay_pubkey = relay.public_key().to_hex();
        let service = client(Keys::generate());
        let now = Utc::now().timestamp();
        let req1 = read_request("wss://chat.example.test", &relay_pubkey, "channel-1", HEX64);
        let req2 = read_request("wss://chat.example.test", &relay_pubkey, "channel-2", HEX64);
        let reqs = [&req1, &req2];

        fn batch_content(now: i64, evaluated_at: i64) -> Value {
            json!({
                "results": [
                    { "decision": "allow", "reason": "member", "evaluated_at": now,
                      "pubkey": HEX64, "channel_id": "channel-1", "message_id": HEX64 },
                    { "decision": "deny", "reason": "not a member", "evaluated_at": now,
                      "pubkey": HEX64, "channel_id": "channel-2", "message_id": HEX64 },
                ],
                "evaluated_at": evaluated_at,
            })
        }

        // Fresh envelope, correctly echoed items → decisions in input order.
        let raw = serde_json::to_value(relay_19030(&relay, batch_content(now, now))).unwrap();
        let decisions = service
            .batch_decision_from_19030(&raw, &relay_pubkey, &reqs)
            .expect("a valid batch response must map");
        assert_eq!(decisions.len(), 2);
        assert_eq!(decisions[0].as_ref().unwrap(), &BuzzReadDecision::Allow);
        assert_eq!(decisions[1].as_ref().unwrap(), &BuzzReadDecision::Deny);

        // Item-level evaluated_at is informational: a stale ITEM timestamp on
        // a fresh envelope does not fail that item.
        let mut content = batch_content(now - 120, now);
        content["results"][0]["evaluated_at"] = json!(now - 120);
        let raw = serde_json::to_value(relay_19030(&relay, content)).unwrap();
        let decisions = service
            .batch_decision_from_19030(&raw, &relay_pubkey, &reqs)
            .expect("stale item evaluated_at is informational");
        assert_eq!(decisions[0].as_ref().unwrap(), &BuzzReadDecision::Allow);

        // Per-item echo mismatch fails ONLY that item.
        let mut content = batch_content(now, now);
        content["results"][1]["channel_id"] = json!("channel-3");
        let raw = serde_json::to_value(relay_19030(&relay, content)).unwrap();
        let decisions = service
            .batch_decision_from_19030(&raw, &relay_pubkey, &reqs)
            .expect("an item echo mismatch must not fail the envelope");
        assert_eq!(decisions[0].as_ref().unwrap(), &BuzzReadDecision::Allow);
        assert!(matches!(
            &decisions[1],
            Err(BuzzAuthorityError::InvalidResponse(_))
        ));

        // Envelope failures fail EVERY item closed:
        // wrong pinned pubkey
        let raw = serde_json::to_value(relay_19030(&relay, batch_content(now, now))).unwrap();
        let wrong_pin = Keys::generate().public_key().to_hex();
        assert!(matches!(
            service.batch_decision_from_19030(&raw, &wrong_pin, &reqs),
            Err(BuzzAuthorityError::InvalidResponse(_))
        ));
        // wrong kind
        let note = EventBuilder::text_note(batch_content(now, now).to_string())
            .sign_with_keys(&relay)
            .unwrap();
        let raw = serde_json::to_value(&note).unwrap();
        assert!(matches!(
            service.batch_decision_from_19030(&raw, &relay_pubkey, &reqs),
            Err(BuzzAuthorityError::InvalidResponse(_))
        ));
        // stale top-level evaluated_at (past and future)
        for stale in [now - 120, now + 120, i64::MIN, i64::MAX] {
            let raw = serde_json::to_value(relay_19030(&relay, batch_content(now, stale))).unwrap();
            assert!(
                matches!(
                    service.batch_decision_from_19030(&raw, &relay_pubkey, &reqs),
                    Err(BuzzAuthorityError::InvalidResponse(_))
                ),
                "envelope evaluated_at {stale} must fail every item"
            );
        }
        // result count mismatch
        let mut content = batch_content(now, now);
        content["results"] = json!([content["results"][0].clone()]);
        let raw = serde_json::to_value(relay_19030(&relay, content)).unwrap();
        assert!(matches!(
            service.batch_decision_from_19030(&raw, &relay_pubkey, &reqs),
            Err(BuzzAuthorityError::InvalidResponse(_))
        ));
        // unparseable content
        let raw = serde_json::to_value(relay_19030(&relay, json!({ "nope": true }))).unwrap();
        assert!(matches!(
            service.batch_decision_from_19030(&raw, &relay_pubkey, &reqs),
            Err(BuzzAuthorityError::InvalidResponse(_))
        ));
    }

    #[test]
    fn registry_from_19030_parses_channels_and_rejects_pubkey_echo_mismatch() {
        let relay = Keys::generate();
        let relay_pubkey = relay.public_key().to_hex();
        let service = client(Keys::generate());
        let now = Utc::now().timestamp();

        fn registry_content(evaluated_at: i64, pubkey: &str) -> Value {
            json!({
                "channels": [
                    { "channel_id": "channel-1", "name": "Announcements",
                      "channel_type": "stream", "visibility": "private", "member": true },
                    { "channel_id": "channel-2", "name": "Open Lounge",
                      "channel_type": "forum", "visibility": "open", "member": false },
                ],
                "evaluated_at": evaluated_at,
                "pubkey": pubkey,
            })
        }

        // Correct echo: the registry parses with the member flags intact.
        let raw = serde_json::to_value(relay_19030(&relay, registry_content(now, HEX64))).unwrap();
        let channels = service
            .registry_from_19030(&raw, &relay_pubkey, HEX64)
            .expect("a correctly-echoed registry must parse");
        assert_eq!(channels.len(), 2);
        assert_eq!(channels[0].channel_id, "channel-1");
        assert!(channels[0].member);
        assert_eq!(channels[1].channel_id, "channel-2");
        assert!(!channels[1].member);

        // The relay echoes a DIFFERENT pubkey than the one requested → the
        // response is not for this request and fails closed.
        let raw = serde_json::to_value(relay_19030(&relay, registry_content(now, &"f".repeat(64))))
            .unwrap();
        assert!(
            matches!(
                service.registry_from_19030(&raw, &relay_pubkey, HEX64),
                Err(BuzzAuthorityError::InvalidResponse(_))
            ),
            "a pubkey echo mismatch must fail closed"
        );

        // Stale registry evaluated_at fails closed too.
        let raw =
            serde_json::to_value(relay_19030(&relay, registry_content(now - 120, HEX64))).unwrap();
        assert!(matches!(
            service.registry_from_19030(&raw, &relay_pubkey, HEX64),
            Err(BuzzAuthorityError::InvalidResponse(_))
        ));
    }

    #[test]
    fn decision_from_result_maps_decisions_and_rejects_echo_mismatch() {
        let expected = BuzzAccessCheckRequest {
            pubkey: HEX64.to_string(),
            channel_id: "channel-1".to_string(),
            channel_kind: BuzzChannelKind::Workspace,
            message_id: Some(HEX64.to_string()),
            event_created_at: Some(1_750_000_000),
        };
        let result = |decision: &str| BuzzAccessCheckResult {
            decision: decision.to_string(),
            reason: "member".to_string(),
            evaluated_at: 0,
            pubkey: expected.pubkey.clone(),
            channel_id: expected.channel_id.clone(),
            message_id: expected.message_id.clone(),
        };
        for (decision, expected_decision) in [
            ("allow", BuzzReadDecision::Allow),
            ("deny", BuzzReadDecision::Deny),
            ("not_found", BuzzReadDecision::NotFound),
        ] {
            assert_eq!(
                BuzzGatewayClient::decision_from_result(&result(decision), &expected).unwrap(),
                expected_decision
            );
        }
        assert!(matches!(
            BuzzGatewayClient::decision_from_result(&result("maybe"), &expected),
            Err(BuzzAuthorityError::InvalidResponse(_))
        ));
        // Echo mismatch on any of the three echoed fields fails closed.
        for field in ["pubkey", "channel_id", "message_id"] {
            let mut bad = result("allow");
            match field {
                "pubkey" => bad.pubkey = "f".repeat(64),
                "channel_id" => bad.channel_id = "channel-2".to_string(),
                _ => bad.message_id = None,
            }
            assert!(
                matches!(
                    BuzzGatewayClient::decision_from_result(&bad, &expected),
                    Err(BuzzAuthorityError::InvalidResponse(_))
                ),
                "{field} echo mismatch must fail closed"
            );
        }
    }

    #[tokio::test]
    async fn check_access_batch_unpinned_requests_fail_closed_without_network() {
        // A request without a pinned relay pubkey fails closed as Config
        // before any network I/O; empty input yields empty results.
        let service = client(Keys::generate());
        let pinned_key = Keys::generate().public_key().to_hex();
        let unpinned = BuzzReadRequest {
            relay_pubkey: None,
            ..read_request("wss://chat.example.test", &pinned_key, "channel-1", HEX64)
        };
        let results = service.check_access_batch(&[unpinned]).await;
        assert_eq!(results.len(), 1);
        assert!(matches!(&results[0], Err(BuzzAuthorityError::Config(_))));
        assert!(service.check_access_batch(&[]).await.is_empty());
    }

    #[tokio::test]
    async fn dispatch_rejects_private_relay_targets_before_network_io() {
        let service = client(Keys::generate());
        let request = BuzzReadRequest {
            tenant_id: TenantId(Uuid::new_v4()),
            community_id: "community-1".to_string(),
            relay_url: "ws://127.0.0.1:8080".to_string(),
            relay_pubkey: Some(HEX64.to_string()),
            channel_id: "channel-1".to_string(),
            channel_kind: BuzzChannelKind::Workspace,
            message_id: Some(HEX64.to_string()),
            pubkey: HEX64.to_string(),
            event_created_at: Utc::now(),
        };
        assert!(matches!(
            service.can_read(&request).await,
            Err(BuzzAuthorityError::Config(_))
        ));
    }

    #[test]
    fn page_state_rejects_malformed_paging() {
        let page = |complete: bool, cursor: Option<&str>| BuzzStatePage {
            entries: Vec::new(),
            cursor: cursor.map(String::from),
            complete,
        };
        // `cursor: null` with `complete: false` is malformed.
        assert!(matches!(
            validate_page(&page(false, None)),
            Err(BuzzAuthorityError::InvalidResponse(_))
        ));
        // An incomplete page with a cursor is fine...
        assert!(validate_page(&page(false, Some("next"))).is_ok());
        // ...and so is any complete page, cursor or not.
        assert!(validate_page(&page(true, None)).is_ok());
        assert!(validate_page(&page(true, Some("next"))).is_ok());
    }

    #[test]
    fn access_check_request_serde_uses_snake_case_and_omits_optionals() {
        let req = BuzzAccessCheckRequest {
            pubkey: HEX64.to_string(),
            channel_id: "channel-1".to_string(),
            channel_kind: BuzzChannelKind::Dm,
            message_id: None,
            event_created_at: None,
        };
        let wire = serde_json::to_value(&req).unwrap();
        assert_eq!(wire["channel_kind"], "dm");
        assert!(wire.get("message_id").is_none());
        assert!(wire.get("event_created_at").is_none());
        let back: BuzzAccessCheckRequest = serde_json::from_value(wire).unwrap();
        assert_eq!(back.channel_kind, BuzzChannelKind::Dm);
        assert_eq!(back.message_id, None);
    }

    #[test]
    fn status_error_maps_401_to_unauthorized_and_other_statuses_to_transport() {
        // 401 is the relay's explicit "reject" signal and maps to
        // Unauthorized; other non-2xx — including redirects, which the client
        // never follows — fail closed as transport errors.
        assert!(matches!(
            status_error(StatusCode::UNAUTHORIZED),
            Some(BuzzAuthorityError::Unauthorized)
        ));
        assert!(matches!(
            status_error(StatusCode::INTERNAL_SERVER_ERROR),
            Some(BuzzAuthorityError::Transport(_))
        ));
        assert!(matches!(
            status_error(StatusCode::MOVED_PERMANENTLY),
            Some(BuzzAuthorityError::Transport(_))
        ));
        assert!(status_error(StatusCode::OK).is_none());
    }

    #[tokio::test]
    async fn read_response_json_rejects_non_json_success_body() {
        // A 2xx response carrying a non-JSON (including empty) body fails
        // closed as an invalid response — exercised without any network call.
        for body in ["", "definitely not json"] {
            let http_response = axum::http::Response::builder()
                .status(StatusCode::OK)
                .body(body)
                .unwrap();
            assert!(matches!(
                read_response_json(Response::from(http_response)).await,
                Err(BuzzAuthorityError::InvalidResponse(_))
            ));
        }
    }
}
