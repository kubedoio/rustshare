//! Google Calendar OAuth client and read-only sync (issue #315).
//!
//! The provider-facing HTTP lives behind fixed, allowlisted base URLs (no
//! caller-controlled hosts, so no SSRF surface); tests point the bases at a
//! local mock server. Token material is decrypted only in-memory and never
//! logged or returned.

use std::time::Duration;

use chrono::{DateTime, Utc};
use rustshare_core::domain::{CalendarEvent, CalendarSource};
use rustshare_crypto::SecretEncryptionKey;
use rustshare_storage::MetadataStore;
use serde::Deserialize;

const AUTH_BASE: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const API_BASE: &str = "https://www.googleapis.com/calendar/v3";
const USERINFO_URL: &str = "https://www.googleapis.com/oauth2/v3/userinfo";
const REVOKE_URL: &str = "https://oauth2.googleapis.com/revoke";
const READONLY_SCOPE: &str = "https://www.googleapis.com/auth/calendar.readonly";
const EVENTS_PAGE_SIZE: u32 = 2500;

/// Base delay between successful sync runs when the scheduler is free-running.
pub const DEFAULT_SYNC_INTERVAL: Duration = Duration::from_secs(900);
/// Access tokens are refreshed this long before their advertised expiry.
pub const TOKEN_EXPIRY_MARGIN: Duration = Duration::from_secs(60);
const DEFAULT_RETRY_AFTER: Duration = Duration::from_secs(60);
/// Provider HTTP bounds: a stalled connection must not hang a sync run (and
/// thereby hold its lease) indefinitely. The generous total timeout covers a
/// large multi-page events.list.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

/// Build a provider HTTP client with connect and total timeouts. The `http`
/// field of each provider client stays public so tests can swap in a client
/// with different bounds against a slow mock.
pub(crate) fn build_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

/// OAuth client + Calendar API endpoints for one Google app registration.
pub struct GoogleCalendarClient {
    pub http: reqwest::Client,
    pub client_id: String,
    pub client_secret: String,
    pub redirect_url: String,
    pub auth_base: String,
    pub token_url: String,
    pub api_base: String,
    pub userinfo_url: String,
    pub revoke_url: String,
}

#[derive(Debug, thiserror::Error)]
pub enum GoogleError {
    #[error("Token exchange failed: {0}")]
    TokenExchange(String),
    #[error("Access token refresh failed: {0}")]
    TokenRefresh(String),
    #[error("User info request failed: {0}")]
    UserInfo(String),
    #[error("Calendar API request failed: {0}")]
    Api(String),
    #[error("Sync token invalidated (410)")]
    Gone,
    #[error("Rate limited by provider")]
    RateLimited { retry_after: Duration },
    /// HTTP 401: the cached access token was rejected. Distinct from
    /// `AuthRequired` so the sync can force one refresh + retry before
    /// parking the source.
    #[error("Access token rejected (401)")]
    Unauthorized,
    #[error("Provider rejected the grant")]
    AuthRequired,
}

#[derive(Debug, Deserialize)]
pub struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
    #[allow(dead_code)]
    token_type: Option<String>,
    #[allow(dead_code)]
    scope: Option<String>,
}

impl TokenResponse {
    pub fn access_token(&self) -> &str {
        &self.access_token
    }

    /// A rotated refresh token, when the provider returned one.
    pub fn rotated_refresh_token(&self) -> Option<&str> {
        self.refresh_token.as_deref()
    }

    pub fn expires_at(&self) -> DateTime<Utc> {
        Utc::now() + chrono::Duration::seconds(self.expires_in.unwrap_or(3600))
    }
}

impl GoogleCalendarClient {
    /// Build a client from env config; `None` when the deployment has no
    /// Google OAuth client id/secret (provider unconfigured, not an error).
    pub fn from_config(
        client_id: Option<String>,
        client_secret: Option<String>,
        public_url: &str,
    ) -> Option<Self> {
        let (client_id, client_secret) = (client_id?, client_secret?);
        Some(Self::new(client_id, client_secret, public_url))
    }

    pub fn new(client_id: String, client_secret: String, public_url: &str) -> Self {
        Self {
            http: build_http_client(),
            client_id,
            client_secret,
            redirect_url: format!("{public_url}/api/v1/calendar/oauth/google/callback"),
            auth_base: AUTH_BASE.to_string(),
            token_url: TOKEN_URL.to_string(),
            api_base: API_BASE.to_string(),
            userinfo_url: USERINFO_URL.to_string(),
            revoke_url: REVOKE_URL.to_string(),
        }
    }

    /// The provider consent URL the frontend navigates to.
    pub fn authorize_url(&self, state: &str) -> String {
        let query = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs([
                ("client_id", self.client_id.as_str()),
                ("redirect_uri", self.redirect_url.as_str()),
                ("response_type", "code"),
                ("scope", READONLY_SCOPE),
                ("access_type", "offline"),
                ("prompt", "consent"),
                ("state", state),
            ])
            .finish();
        format!("{}?{query}", self.auth_base)
    }

    async fn post_token_request(
        &self,
        params: &[(&str, &str)],
        context: &str,
    ) -> Result<TokenResponse, GoogleError> {
        let response = self
            .http
            .post(&self.token_url)
            .form(params)
            .send()
            .await
            .map_err(|e| {
                if context == "refresh" {
                    GoogleError::TokenRefresh(e.to_string())
                } else {
                    GoogleError::TokenExchange(e.to_string())
                }
            })?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            // `invalid_grant` marks a revoked/expired refresh token.
            if body.contains("invalid_grant") {
                return Err(GoogleError::AuthRequired);
            }
            return Err(if context == "refresh" {
                GoogleError::TokenRefresh(format!("HTTP {status}"))
            } else {
                GoogleError::TokenExchange(format!("HTTP {status}"))
            });
        }
        let parse_err = |e: reqwest::Error| {
            if context == "refresh" {
                GoogleError::TokenRefresh(e.to_string())
            } else {
                GoogleError::TokenExchange(e.to_string())
            }
        };
        response.json::<TokenResponse>().await.map_err(parse_err)
    }

    /// Exchange an authorization code for tokens (OAuth callback).
    pub async fn exchange_code(&self, code: &str) -> Result<TokenResponse, GoogleError> {
        self.post_token_request(
            &[
                ("code", code),
                ("client_id", &self.client_id),
                ("client_secret", &self.client_secret),
                ("redirect_uri", &self.redirect_url),
                ("grant_type", "authorization_code"),
            ],
            "exchange",
        )
        .await
    }

    /// Refresh an access token. A rotated refresh token, when returned, wins
    /// over the stored one.
    pub async fn refresh_access_token(
        &self,
        refresh_token: &str,
    ) -> Result<TokenResponse, GoogleError> {
        self.post_token_request(
            &[
                ("refresh_token", refresh_token),
                ("client_id", &self.client_id),
                ("client_secret", &self.client_secret),
                ("grant_type", "refresh_token"),
            ],
            "refresh",
        )
        .await
    }

    /// The connected account's email address (from userinfo), used as
    /// `calendar_sources.external_account`.
    pub async fn user_email(&self, access_token: &str) -> Result<String, GoogleError> {
        let response = self
            .http
            .get(&self.userinfo_url)
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|e| GoogleError::UserInfo(e.to_string()))?;
        if !response.status().is_success() {
            return Err(GoogleError::UserInfo(format!("HTTP {}", response.status())));
        }
        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|e| GoogleError::UserInfo(e.to_string()))?;
        body.get("email")
            .and_then(|value| value.as_str())
            .map(str::to_string)
            .ok_or_else(|| GoogleError::UserInfo("response had no email".to_string()))
    }

    /// Best-effort token revocation (disconnect). Returns whether the
    /// provider accepted the revocation; errors are logged by the caller and
    /// never propagated to the user.
    pub async fn revoke_token(&self, token: &str) -> bool {
        match self
            .http
            .post(&self.revoke_url)
            .query(&[("token", token)])
            .send()
            .await
        {
            Ok(response) => response.status().is_success(),
            Err(_) => false,
        }
    }
}

// ---------------------------------------------------------------------------
// Read-only sync
// ---------------------------------------------------------------------------

pub struct CalendarSyncConfig {
    pub past_days: i64,
    pub future_days: i64,
}

impl CalendarSyncConfig {
    pub fn from_config(config: &crate::config::AppConfig) -> Self {
        Self {
            past_days: config.calendar_sync_past_days,
            future_days: config.calendar_sync_future_days,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum SyncOutcome {
    /// A run completed. `next_sync_token` is the new incremental cursor
    /// (`None` until the provider supplies one). `soft_deleted` counts
    /// provider deletions propagated; on a full window run it is the
    /// absent-entry sweep count (plus, for Outlook, explicit `@removed`
    /// tombstones), and on an incremental run only the explicit tombstones.
    ///
    /// `upserted` is the payload size (entries the run attempted to
    /// materialize), not a changed-row count: the upsert's `IS DISTINCT FROM`
    /// guard suppresses the `updated_at` bump for unchanged rows, but the
    /// counter still includes them. Both providers count it identically.
    Completed {
        upserted: usize,
        soft_deleted: u64,
        next_sync_token: Option<String>,
    },
    /// The provider rejected the grant; the source is marked `auth_required`
    /// and later runs no-op until reconnect.
    AuthRequired,
    /// The source is parked (`auth_required`): the run did no work and must
    /// not be treated as a success or a failure. The worker publishes no
    /// event, advances no watermark, writes no status, and records no error.
    /// It still releases the lease and reschedules the source so a parked
    /// source is not stuck "in progress" until the stale takeover.
    Parked,
    /// The sync lease was lost mid-run (stale takeover by another worker).
    /// Like `Parked`, the run must not publish or write status/watermark —
    /// the new lease holder owns the source now.
    LeaseLost,
    /// HTTP 429 / Retry-After; the caller backs off `next_sync_at`.
    RateLimited { retry_after: Duration },
    /// Transient failure; safe to retry on schedule.
    Failed(String),
}

#[derive(Debug, Deserialize)]
struct EventsListResponse {
    items: Option<Vec<GoogleEvent>>,
    #[serde(rename = "nextPageToken")]
    next_page_token: Option<String>,
    #[serde(rename = "nextSyncToken")]
    next_sync_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GoogleEvent {
    id: String,
    #[serde(rename = "recurringEventId")]
    recurring_event_id: Option<String>,
    #[serde(rename = "originalStartTime")]
    original_start_time: Option<GoogleTime>,
    #[serde(rename = "start")]
    start: Option<GoogleTime>,
    #[serde(rename = "end")]
    end: Option<GoogleTime>,
    summary: Option<String>,
    description: Option<String>,
    location: Option<String>,
    status: Option<String>,
    recurrence: Option<Vec<String>>,
    etag: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GoogleTime {
    #[serde(rename = "dateTime")]
    date_time: Option<String>,
    date: Option<String>,
    #[serde(rename = "timeZone")]
    time_zone: Option<String>,
}

impl GoogleTime {
    fn starts_at(&self) -> Option<DateTime<Utc>> {
        if let Some(dt) = &self.date_time {
            return DateTime::parse_from_rfc3339(dt)
                .ok()
                .map(|parsed| parsed.with_timezone(&Utc));
        }
        self.date
            .as_deref()
            .and_then(|date| date.parse::<chrono::NaiveDate>().ok())
            .map(|date| date.and_hms_opt(0, 0, 0).unwrap().and_utc())
    }
}

/// The mirrored-event identity `(external_uid, recurrence_id)` for a Google
/// entry. Occurrences and overrides of a recurring master ride on the master's
/// id with the original start as the recurrence id; the master and single
/// instances use their own id with a null recurrence id.
///
/// `map_event` and the full-run `present_keys` sweep both derive identity
/// through this one function so the upserted key and the sweep key can never
/// diverge. A missing or unparseable `originalStartTime` yields a null
/// recurrence id, which keeps the row on its own `id` instead of fabricating
/// an unstable key (the previous `unwrap_or(now)` could soft-delete the live
/// occurrence and insert a duplicate).
fn event_identity(item: &GoogleEvent) -> (String, Option<String>) {
    match (&item.recurring_event_id, &item.original_start_time) {
        (Some(master_id), Some(original)) => (
            master_id.clone(),
            original
                .starts_at()
                .map(|dt| dt.to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true)),
        ),
        _ => (item.id.clone(), None),
    }
}

/// The sweep/upsert key format `external_uid|recurrence_id`, matching the
/// `(external_uid || '|' || COALESCE(recurrence_id, ''))` expression used by
/// `soft_delete_calendar_events_absent`.
fn identity_key(external_uid: &str, recurrence_id: Option<&str>) -> String {
    format!("{external_uid}|{}", recurrence_id.unwrap_or_default())
}

fn map_event(source: &CalendarSource, item: GoogleEvent, now: DateTime<Utc>) -> CalendarEvent {
    // Instances of a recurring master (including cancelled ones) carry
    // `recurringEventId` + `originalStartTime`; they become override rows on
    // the master's external id. Identity is derived by the shared
    // `event_identity` helper so it always matches the `present_keys` used by
    // the absent-entry sweep.
    let (external_uid, recurrence_id) = event_identity(&item);
    let fallback_start = item
        .original_start_time
        .as_ref()
        .and_then(GoogleTime::starts_at)
        .unwrap_or(now);
    let starts_at = item
        .start
        .as_ref()
        .and_then(GoogleTime::starts_at)
        .unwrap_or(fallback_start);
    let ends_at = item
        .end
        .as_ref()
        .and_then(GoogleTime::starts_at)
        .unwrap_or(starts_at + chrono::Duration::hours(1));
    let all_day = item
        .start
        .as_ref()
        .and_then(|start| start.date.as_ref())
        .is_some();
    let original_date = item
        .start
        .as_ref()
        .and_then(|start| start.date.as_deref())
        .and_then(|date| date.parse::<chrono::NaiveDate>().ok());
    let timezone = item
        .start
        .as_ref()
        .and_then(|start| start.time_zone.clone())
        .unwrap_or_else(|| "UTC".to_string());
    let rrule = item.recurrence.as_ref().and_then(|rules| {
        rules
            .iter()
            .find_map(|rule| rule.strip_prefix("RRULE:").map(str::to_string))
    });
    let status = match item.status.as_deref() {
        Some("cancelled") => "cancelled",
        Some("tentative") => "tentative",
        _ => "confirmed",
    };
    CalendarEvent {
        id: uuid::Uuid::new_v4(),
        tenant_id: source.tenant_id,
        owner_id: source.owner_id,
        source_id: source.id,
        external_uid: Some(external_uid),
        external_etag: item.etag,
        recurrence_id,
        title: item.summary.clone().unwrap_or_default(),
        description: item.description,
        location: item.location,
        starts_at,
        ends_at,
        all_day,
        original_date,
        timezone,
        rrule,
        status: status.to_string(),
        read_only: true,
        raw: None,
        deleted_at: None,
        created_at: now,
        updated_at: now,
    }
}

/// One `events.list` request; classifies provider status codes so the caller
/// can react (410 → one full resync, 429 → backoff, 401/403 → auth_required).
async fn fetch_events_page(
    client: &GoogleCalendarClient,
    access_token: &str,
    incremental: bool,
    cursor: Option<&str>,
    page_token: Option<&str>,
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
) -> Result<reqwest::Response, GoogleError> {
    let mut request = client
        .http
        .get(format!("{}/calendars/primary/events", client.api_base))
        .bearer_auth(access_token)
        .query(&[("maxResults", EVENTS_PAGE_SIZE.to_string())]);
    if incremental {
        request = request
            .query(&[("showDeleted", "true")])
            .query(&[("syncToken", cursor.unwrap_or_default())]);
    } else {
        request = request
            .query(&[("timeMin", window_start.to_rfc3339())])
            .query(&[("timeMax", window_end.to_rfc3339())])
            .query(&[("singleEvents", "false")]);
    }
    // Mid-paging requests carry the page token in addition to the mode
    // parameters (Google combines pageToken with syncToken/window filters),
    // so a multi-page response advances pages instead of restarting.
    if let Some(page_token) = page_token {
        request = request.query(&[("pageToken", page_token)]);
    }
    let response = request
        .send()
        .await
        .map_err(|e| GoogleError::Api(e.to_string()))?;
    let response = response;
    match response.status().as_u16() {
        200 => Ok(response),
        410 => Err(GoogleError::Gone),
        429 => Err(GoogleError::RateLimited {
            retry_after: parse_retry_after(response.headers()),
        }),
        401 => Err(GoogleError::Unauthorized),
        403 => {
            // Google uses 403 both for genuine auth rejections and for quota
            // throttling (`rateLimitExceeded`/`userRateLimitExceeded`). Only
            // classify a 403 as auth when the error body names no rate-limit
            // reason, so a throttle backs off instead of parking the source.
            let retry_after = parse_retry_after(response.headers());
            let body = response.text().await.unwrap_or_default();
            if google_error_reasons(&body)
                .iter()
                .any(|reason| is_rate_limit_reason(reason))
            {
                Err(GoogleError::RateLimited { retry_after })
            } else {
                Err(GoogleError::AuthRequired)
            }
        }
        status => Err(GoogleError::Api(format!("HTTP {status}"))),
    }
}

/// The `error.errors[].reason` values from a Google API error body.
fn google_error_reasons(body: &str) -> Vec<String> {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| {
            value
                .pointer("/error/errors")
                .and_then(|e| e.as_array())
                .cloned()
        })
        .map(|errors| {
            errors
                .iter()
                .filter_map(|error| {
                    error
                        .get("reason")
                        .and_then(|r| r.as_str())
                        .map(str::to_string)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Google error reasons that mean "too many requests" rather than a rejected
/// grant, mapped to a backoff instead of `auth_required`.
fn is_rate_limit_reason(reason: &str) -> bool {
    matches!(
        reason,
        "rateLimitExceeded" | "userRateLimitExceeded" | "quotaExceeded" | "dailyLimitExceeded"
    )
}

fn parse_retry_after(headers: &reqwest::header::HeaderMap) -> Duration {
    headers
        .get("retry-after")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or(DEFAULT_RETRY_AFTER)
}

/// Refresh the access token and persist the (possibly rotated) token pair.
/// Returns the new access token, or the `SyncOutcome` that should end the run
/// (`AuthRequired` only when the refresh itself is rejected with
/// `invalid_grant`; `LeaseLost` when the worker no longer holds the lease).
///
/// The lease is re-checked before the provider refresh and the token write is
/// lease-guarded, so a stale worker that lost its lease (stale takeover) can
/// neither rotate the grant at the provider nor overwrite the new holder's
/// rotated refresh token.
async fn refresh_and_persist_access_token(
    store: &MetadataStore,
    client: &GoogleCalendarClient,
    secret_key: &SecretEncryptionKey,
    source_id: uuid::Uuid,
    worker_id: &str,
    refresh_token: &str,
) -> Result<String, SyncOutcome> {
    match store
        .heartbeat_calendar_source_lease(source_id, worker_id)
        .await
    {
        Ok(true) => {}
        Ok(false) => return Err(SyncOutcome::LeaseLost),
        Err(e) => {
            tracing::warn!(source_id = %source_id, "lease heartbeat failed before token refresh: {e}");
            return Err(SyncOutcome::LeaseLost);
        }
    }
    match client.refresh_access_token(refresh_token).await {
        Ok(tokens) => {
            let rotated = tokens
                .rotated_refresh_token()
                .and_then(|rotated| rustshare_crypto::encrypt_secret(rotated, secret_key).ok());
            let access_enc =
                match rustshare_crypto::encrypt_secret(tokens.access_token(), secret_key) {
                    Ok(enc) => enc,
                    Err(_) => {
                        return Err(SyncOutcome::Failed("token encryption failed".to_string()))
                    }
                };
            match store
                .update_calendar_source_tokens(
                    source_id,
                    worker_id,
                    rotated.as_deref(),
                    &access_enc,
                    tokens.expires_at(),
                )
                .await
            {
                Ok(true) => {}
                Ok(false) => return Err(SyncOutcome::LeaseLost),
                Err(e) => {
                    return Err(SyncOutcome::Failed(format!(
                        "failed to persist tokens: {e}"
                    )))
                }
            }
            Ok(tokens.access_token().to_string())
        }
        Err(GoogleError::AuthRequired) => Err(SyncOutcome::AuthRequired),
        Err(e) => Err(SyncOutcome::Failed(e.to_string())),
    }
}

/// Run one sync pass for a Google source. Callers must hold the sync lease
/// (`worker_id`) — only the lease holder refreshes access tokens here, and a
/// rotated refresh token is written unconditionally (newer wins).
pub async fn sync_source(
    store: &MetadataStore,
    client: &GoogleCalendarClient,
    secret_key: &SecretEncryptionKey,
    source: &CalendarSource,
    config: &CalendarSyncConfig,
    worker_id: &str,
) -> SyncOutcome {
    // A source whose grant was revoked stays parked until reconnect: no
    // work, no publish, no watermark/status write (the worker treats `Parked`
    // as neither success nor failure).
    if source.status == "auth_required" {
        tracing::debug!(source_id = %source.id, "source auth_required; skipping sync");
        return SyncOutcome::Parked;
    }
    let Some(refresh_enc) = source.refresh_token_enc.as_deref() else {
        return SyncOutcome::Failed("source has no refresh token".to_string());
    };
    let Ok(refresh_token) = rustshare_crypto::decrypt_secret(refresh_enc, secret_key) else {
        return SyncOutcome::Failed("stored refresh token could not be decrypted".to_string());
    };

    // Ensure a usable access token, refreshing (and persisting the rotation)
    // when missing or near expiry.
    let mut access_token = match source.access_token_enc.as_deref() {
        Some(enc) => rustshare_crypto::decrypt_secret(enc, secret_key).ok(),
        None => None,
    };
    let token_expired = source
        .access_token_expires_at
        .map(|expires_at| expires_at - TOKEN_EXPIRY_MARGIN <= Utc::now())
        .unwrap_or(true);
    if access_token.is_none() || token_expired {
        match refresh_and_persist_access_token(
            store,
            client,
            secret_key,
            source.id,
            worker_id,
            &refresh_token,
        )
        .await
        {
            Ok(token) => access_token = Some(token),
            Err(outcome) => return outcome,
        }
    }
    let mut access_token = match access_token {
        Some(token) => token,
        None => return SyncOutcome::Failed("no access token available".to_string()),
    };
    // A 401 from an otherwise valid-looking token forces exactly one refresh
    // and retry before parking: the cached token can be revoked or clock-skew
    // expired even though `access_token_expires_at` said otherwise.
    let mut refreshed_after_401 = false;

    let sync_state = match store.get_calendar_sync_state(source.id).await {
        Ok(state) => state,
        Err(e) => return SyncOutcome::Failed(format!("failed to load sync state: {e}")),
    };
    let cursor = sync_state
        .as_ref()
        .and_then(|state| state.cursor_value.clone());
    let window_start = Utc::now() - chrono::Duration::days(config.past_days.max(0));
    let window_end = Utc::now() + chrono::Duration::days(config.future_days.max(0));

    let mut sync_expired = false;
    // `sync_expired` tracks a 410 mid-paging: the cursor is nulled and the
    // run retries exactly once as a full window sync.
    'full_retry: loop {
        let incremental = !sync_expired && cursor.is_some();
        let effective_cursor = if sync_expired {
            None
        } else {
            cursor.as_deref()
        };
        let mut page_token: Option<String> = None;
        // Assigned on every exit path of the paging loop below (each path
        // sets it before `break`).
        let next_sync_token: Option<String>;
        let mut upserted = 0usize;
        let mut present_keys: Vec<String> = Vec::new();

        loop {
            let response = match fetch_events_page(
                client,
                &access_token,
                incremental,
                effective_cursor,
                page_token.as_deref(),
                window_start,
                window_end,
            )
            .await
            {
                Ok(response) => response,
                Err(GoogleError::Gone) if incremental && !sync_expired => {
                    sync_expired = true;
                    continue 'full_retry;
                }
                Err(GoogleError::Gone) => {
                    return SyncOutcome::Failed("sync token expired twice".to_string());
                }
                Err(GoogleError::RateLimited { retry_after }) => {
                    return SyncOutcome::RateLimited { retry_after };
                }
                Err(GoogleError::Unauthorized) if !refreshed_after_401 => {
                    // Attempt exactly one forced refresh + retry before
                    // parking the source.
                    match refresh_and_persist_access_token(
                        store,
                        client,
                        secret_key,
                        source.id,
                        worker_id,
                        &refresh_token,
                    )
                    .await
                    {
                        Ok(token) => {
                            access_token = token;
                            refreshed_after_401 = true;
                            continue;
                        }
                        Err(outcome) => return outcome,
                    }
                }
                Err(GoogleError::Unauthorized | GoogleError::AuthRequired) => {
                    return SyncOutcome::AuthRequired
                }
                Err(e) => return SyncOutcome::Failed(e.to_string()),
            };
            match store
                .heartbeat_calendar_source_lease(source.id, worker_id)
                .await
            {
                Ok(true) => {}
                Ok(false) => return SyncOutcome::LeaseLost,
                Err(e) => {
                    tracing::warn!(source_id = %source.id, "lease heartbeat failed: {e}");
                    return SyncOutcome::LeaseLost;
                }
            }
            let raw = match response.text().await {
                Ok(text) => text,
                Err(e) => return SyncOutcome::Failed(e.to_string()),
            };
            let page = match serde_json::from_str::<EventsListResponse>(&raw) {
                Ok(page) => page,
                Err(e) => return SyncOutcome::Failed(e.to_string()),
            };
            let now = Utc::now();
            for item in page.items.unwrap_or_default() {
                let (external_uid, recurrence_id) = event_identity(&item);
                present_keys.push(identity_key(&external_uid, recurrence_id.as_deref()));
                let event = map_event(source, item, now);
                match store.upsert_calendar_synced_event(&event).await {
                    Ok(_) => upserted += 1,
                    Err(e) => {
                        tracing::warn!(source_id = %source.id, "synced event upsert failed: {e}");
                    }
                }
            }
            if page.next_page_token.is_none() {
                next_sync_token = page.next_sync_token;
                break;
            }
            page_token = page.next_page_token;
        }

        // Provider deletions propagate on FULL runs only: a full window
        // payload is the complete set of live events in the window, so any
        // in-window mirrored row missing from it was deleted upstream.
        // Incremental deltas carry only CHANGED entries — an absent-key sweep
        // there would soft-delete every unchanged mirrored event (upstream
        // deletions already arrive as cancelled/isCancelled tombstones).
        let mut soft_deleted = 0u64;
        if !incremental {
            match store
                .soft_delete_calendar_events_absent(
                    source.id,
                    &present_keys,
                    window_start,
                    window_end,
                )
                .await
            {
                Ok(count) => soft_deleted = count,
                Err(e) => tracing::warn!(source_id = %source.id, "absent-event sweep failed: {e}"),
            }
        }
        return SyncOutcome::Completed {
            upserted,
            soft_deleted,
            next_sync_token,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_next_sync_token_from_final_page() {
        let body = serde_json::json!({
            "items": [],
            "nextSyncToken": "cursor-after-full"
        });
        let page: EventsListResponse = serde_json::from_value(body).unwrap();
        assert_eq!(page.next_sync_token.as_deref(), Some("cursor-after-full"));
        assert!(page.next_page_token.is_none());
    }

    #[test]
    fn classifies_google_403_rate_limit_reasons() {
        let body = serde_json::json!({
            "error": {
                "code": 403,
                "message": "Quota exceeded",
                "errors": [{"domain": "usageLimits", "reason": "rateLimitExceeded"}]
            }
        })
        .to_string();
        let reasons = google_error_reasons(&body);
        assert_eq!(reasons, vec!["rateLimitExceeded".to_string()]);
        assert!(reasons.iter().any(|reason| is_rate_limit_reason(reason)));

        // A genuine auth 403 (no rate-limit reason) stays AuthRequired.
        let auth_body = serde_json::json!({
            "error": {"code": 403, "errors": [{"reason": "insufficientPermissions"}]}
        })
        .to_string();
        assert!(!google_error_reasons(&auth_body)
            .iter()
            .any(|reason| is_rate_limit_reason(reason)));
        assert!(google_error_reasons("not json").is_empty());
    }

    #[test]
    fn google_identity_requires_a_parseable_original_start() {
        // Occurrence with a parseable originalStartTime → master key + stamp.
        let item: GoogleEvent = serde_json::from_value(serde_json::json!({
            "id": "master_20261012T140000Z",
            "recurringEventId": "master",
            "originalStartTime": {"dateTime": "2026-10-12T14:00:00Z"}
        }))
        .unwrap();
        assert_eq!(
            event_identity(&item),
            (
                "master".to_string(),
                Some("2026-10-12T14:00:00Z".to_string())
            )
        );

        // Missing/unparseable originalStartTime must NOT fabricate an
        // unstable `now`-based key (which diverged from the sweep key). The
        // recurrence id stays null and the identity remains the master id,
        // matching the sweep key produced by the same helper.
        let broken: GoogleEvent = serde_json::from_value(serde_json::json!({
            "id": "master_20261012T140000Z",
            "recurringEventId": "master",
            "originalStartTime": {"dateTime": "not-a-date"}
        }))
        .unwrap();
        assert_eq!(event_identity(&broken), ("master".to_string(), None));
        assert_eq!(
            identity_key(&event_identity(&broken).0, None),
            "master|",
            "upsert and sweep keys must agree for an unparseable original start"
        );
        assert_eq!(
            identity_key("external", None),
            "external|",
            "sweep key format must match the SQL COALESCE expression"
        );
    }
}
