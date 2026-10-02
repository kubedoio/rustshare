//! Microsoft/Outlook Calendar OAuth client and read-only delta sync (issue
//! #315).
//!
//! The provider-facing HTTP lives behind fixed, allowlisted base URLs (no
//! caller-controlled hosts, so no SSRF surface); tests point the bases at a
//! local mock server. Token material is decrypted only in-memory and never
//! logged or returned. Sync mirrors the Google path
//! (`google_calendar::SyncOutcome` taxonomy, cursor in
//! `calendar_sync_states.cursor_value` with `cursor_kind = 'ms_delta_token'`).
//!
//! Two Graph behaviours the read-only delta sync must handle explicitly:
//!
//! * Date/time fidelity: Graph sends `dateTime` as a wall clock with up to 7
//!   fractional digits and the zone in the separate `timeZone` field, so every
//!   `calendarView` request carries `Prefer: outlook.timezone="UTC"` and the
//!   parser accepts fractional, offset-less values (see `GraphTime::starts_at`).
//! * Deletions: `calendarView/delta` reports removed events as minimal objects
//!   under `@removed` (`reason: deleted`), not as `isCancelled` tombstones.
//!   These soft-delete the mirrored row by `(external_uid, recurrence_id)` and
//!   are never mapped/upserted (see `@removed` handling in `sync_source`).
//!
//! Disconnect performs **no** provider-side revocation call: Microsoft exposes
//! no grant-scoped revoke endpoint within the `Calendars.Read` scope (the
//! `revokeSignInSessions` API needs `User.RevokeSessions.All`, which this app
//! does not request and cannot obtain without admin consent), so the effective
//! revocation is the local token wipe and the user removes the Elembra grant
//! from their Microsoft account.

use std::time::Duration;

use chrono::{DateTime, Utc};
use rustshare_core::domain::{CalendarEvent, CalendarSource};
use rustshare_crypto::SecretEncryptionKey;
use rustshare_storage::MetadataStore;
use serde::Deserialize;

use crate::services::google_calendar::{
    build_http_client, CalendarSyncConfig, SyncOutcome, TOKEN_EXPIRY_MARGIN,
};

const AUTH_BASE: &str = "https://login.microsoftonline.com/common/oauth2/v2.0/authorize";
const TOKEN_URL: &str = "https://login.microsoftonline.com/common/oauth2/v2.0/token";
const API_BASE: &str = "https://graph.microsoft.com/v1.0";
const READONLY_SCOPE: &str = "offline_access Calendars.Read";
const DEFAULT_RETRY_AFTER: Duration = Duration::from_secs(60);

/// OAuth client + Microsoft Graph endpoints for one Entra app registration.
pub struct OutlookCalendarClient {
    pub http: reqwest::Client,
    pub client_id: String,
    pub client_secret: String,
    pub redirect_url: String,
    pub auth_base: String,
    pub token_url: String,
    pub api_base: String,
    pub me_url: String,
}

#[derive(Debug, thiserror::Error)]
pub enum OutlookError {
    #[error("Token exchange failed: {0}")]
    TokenExchange(String),
    #[error("Access token refresh failed: {0}")]
    TokenRefresh(String),
    #[error("User info request failed: {0}")]
    UserInfo(String),
    #[error("Calendar API request failed: {0}")]
    Api(String),
    /// The provider rejected the stored delta token (400 InvalidDeltaToken /
    /// 410 syncStateNotFound); the run retries exactly once as a full window
    /// sync, mirroring the Google 410 handling.
    #[error("Delta token invalidated")]
    Gone,
    #[error("Rate limited by provider")]
    RateLimited { retry_after: Duration },
    #[error("Provider rejected the grant")]
    AuthRequired,
}

impl OutlookCalendarClient {
    /// Build a client from env config; `None` when the deployment has no
    /// Microsoft OAuth client id/secret (provider unconfigured, not an error).
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
            redirect_url: format!("{public_url}/api/v1/calendar/oauth/outlook/callback"),
            auth_base: AUTH_BASE.to_string(),
            token_url: TOKEN_URL.to_string(),
            api_base: API_BASE.to_string(),
            me_url: format!("{API_BASE}/me"),
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
                ("state", state),
            ])
            .finish();
        format!("{}?{query}", self.auth_base)
    }

    async fn post_token_request(
        &self,
        params: &[(&str, &str)],
        context: &str,
    ) -> Result<crate::services::google_calendar::TokenResponse, OutlookError> {
        let response = self
            .http
            .post(&self.token_url)
            .form(params)
            .send()
            .await
            .map_err(|e| {
                if context == "refresh" {
                    OutlookError::TokenRefresh(e.to_string())
                } else {
                    OutlookError::TokenExchange(e.to_string())
                }
            })?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            // `invalid_grant` marks a revoked/expired refresh token.
            if body.contains("invalid_grant") {
                return Err(OutlookError::AuthRequired);
            }
            return Err(if context == "refresh" {
                OutlookError::TokenRefresh(format!("HTTP {status}"))
            } else {
                OutlookError::TokenExchange(format!("HTTP {status}"))
            });
        }
        let parse_err = |e: reqwest::Error| {
            if context == "refresh" {
                OutlookError::TokenRefresh(e.to_string())
            } else {
                OutlookError::TokenExchange(e.to_string())
            }
        };
        response.json().await.map_err(parse_err)
    }

    /// Exchange an authorization code for tokens (OAuth callback).
    pub async fn exchange_code(
        &self,
        code: &str,
    ) -> Result<crate::services::google_calendar::TokenResponse, OutlookError> {
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
    ) -> Result<crate::services::google_calendar::TokenResponse, OutlookError> {
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

    /// The connected account's email address (from `/me`), used as
    /// `calendar_sources.external_account`.
    pub async fn user_email(&self, access_token: &str) -> Result<String, OutlookError> {
        let response = self
            .http
            .get(&self.me_url)
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|e| OutlookError::UserInfo(e.to_string()))?;
        if !response.status().is_success() {
            return Err(OutlookError::UserInfo(format!(
                "HTTP {}",
                response.status()
            )));
        }
        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|e| OutlookError::UserInfo(e.to_string()))?;
        body.get("mail")
            .or_else(|| body.get("userPrincipalName"))
            .and_then(|value| value.as_str())
            .map(str::to_string)
            .ok_or_else(|| OutlookError::UserInfo("response had no email".to_string()))
    }

    /// Best-effort revocation reported to the disconnect caller. **No
    /// provider HTTP call is made.** Microsoft Graph has no grant-scoped
    /// revoke endpoint usable with the requested `offline_access
    /// Calendars.Read` scope: `POST /me/revokeSignInSessions` requires
    /// `User.RevokeSessions.All`, which this app registration neither requests
    /// nor can obtain without admin consent, so the call always failed 403 in
    /// real deployments while the UI promised a Microsoft-wide sign-out. The
    /// effective revocation of this app's access is the local wipe of the
    /// encrypted access/refresh tokens (performed by the caller), after which
    /// the refresh token can no longer be exchanged; the user removes the
    /// Elembra grant from their Microsoft account to invalidate it upstream.
    ///
    /// Returns `true` so the caller does not log a spurious "revocation was
    /// not accepted" warning for a step that is intentionally a no-op.
    ///
    /// NOTE: kept only as the signature the (separately owned)
    /// `CalendarService::disconnect_source` call site compiles against; that
    /// call site can drop it entirely.
    pub async fn revoke_token(&self, access_token: &str) -> bool {
        let _ = access_token;
        tracing::debug!(
            "outlook disconnect: no provider-side revocation is available within \
             Calendars.Read; local token wipe is the effective revocation"
        );
        true
    }
}

// ---------------------------------------------------------------------------
// Read-only delta sync
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct DeltaResponse {
    value: Option<Vec<GraphEvent>>,
    #[serde(rename = "@odata.nextLink")]
    next_link: Option<String>,
    #[serde(rename = "@odata.deltaLink")]
    delta_link: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GraphEvent {
    id: String,
    #[serde(rename = "type")]
    kind: Option<String>,
    #[serde(rename = "seriesMasterId")]
    series_master_id: Option<String>,
    #[serde(rename = "isCancelled")]
    is_cancelled: Option<bool>,
    subject: Option<String>,
    location: Option<GraphLocation>,
    start: Option<GraphTime>,
    end: Option<GraphTime>,
    #[serde(rename = "originalStartTime")]
    original_start_time: Option<GraphTime>,
    recurrence: Option<GraphRecurrence>,
    #[serde(rename = "changeKey")]
    change_key: Option<String>,
    #[serde(rename = "isAllDay")]
    is_all_day: Option<bool>,
    /// Present on `calendarView/delta` deletion tombstones. Graph reports
    /// removed events as minimal objects (`{"id": "...", "@removed":
    /// {"reason": "deleted"}}`) with no `subject`/`start`/`end`; they must be
    /// treated as deletions, never mapped onto a live row.
    #[serde(rename = "@removed")]
    removed: Option<GraphRemoved>,
}

#[derive(Debug, Deserialize)]
struct GraphRemoved {
    reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GraphLocation {
    #[serde(rename = "displayName")]
    display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GraphTime {
    #[serde(rename = "dateTime")]
    date_time: Option<String>,
    #[serde(rename = "timeZone")]
    time_zone: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GraphRecurrence {
    pattern: Option<GraphPattern>,
    range: Option<GraphRange>,
}

#[derive(Debug, Deserialize)]
struct GraphPattern {
    #[serde(rename = "type")]
    pattern_type: Option<String>,
    interval: Option<u32>,
    #[serde(rename = "daysOfWeek")]
    days_of_week: Option<Vec<String>>,
    #[serde(rename = "dayOfMonth")]
    day_of_month: Option<u32>,
    month: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct GraphRange {
    #[serde(rename = "endDate")]
    end_date: Option<String>,
    #[serde(rename = "numberOfOccurrences")]
    number_of_occurrences: Option<u32>,
}

impl GraphTime {
    /// The UTC instant for a Graph `dateTime`/`timeZone` pair.
    ///
    /// Graph sends `dateTime` either as an offset-qualified RFC 3339 value
    /// (`...Z`, rare in calendar payloads) or, far more often, as an
    /// offset-less wall clock with up to 7 fractional digits — e.g.
    /// `2017-08-29T04:00:00.0000000` — with the zone carried separately in
    /// `timeZone`. Both shapes must parse; the old RFC 3339 + whole-second
    /// parse failed on the common shape and silently substituted "now".
    fn starts_at(&self) -> Option<DateTime<Utc>> {
        let raw = self.date_time.as_deref()?;
        // Explicit offset (Z or ±hh:mm): the instant is unambiguous.
        if let Ok(parsed) = DateTime::parse_from_rfc3339(raw) {
            return Some(parsed.with_timezone(&Utc));
        }
        // Offset-less wall clock, fractional or whole-second.
        let naive = chrono::NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%S%.f")
            .or_else(|_| chrono::NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%S"))
            .ok()?;
        Some(self.wall_clock_to_utc(naive))
    }

    /// Interpret an offset-less wall clock in `timeZone`.
    ///
    /// All calendarView requests send `Prefer: outlook.timezone="UTC"`, so
    /// Graph returns UTC wall clocks and a UTC instant is exact. If a
    /// non-IANA zone name is nevertheless present (e.g. the Windows name
    /// `"Pacific Standard Time"`), there is no safe conversion available: the
    /// raw name is preserved on the row for display, but the wall clock is
    /// taken as-is rather than silently shifted by a guessed zone. Recurrence
    /// expansion (`calendar_service.rs::expand_master`) then falls back to UTC
    /// for such rows, which matches the value stored here.
    fn wall_clock_to_utc(&self, naive: chrono::NaiveDateTime) -> DateTime<Utc> {
        match self
            .time_zone
            .as_deref()
            .and_then(|name| name.parse::<chrono_tz::Tz>().ok())
        {
            Some(tz) => naive
                .and_local_timezone(tz)
                .single()
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|| naive.and_utc()),
            None => naive.and_utc(),
        }
    }
}

/// The mirrored-event identity `(external_uid, recurrence_id)` for a Graph
/// entry. Occurrences and exceptions of a recurring master ride on the
/// master's id with the original start as the recurrence id; the master and
/// single instances use their own id with a null recurrence id.
///
/// `map_event` and the full-run `present_keys` sweep both derive identity
/// through this one function so the upserted key and the sweep key can never
/// diverge (a mismatch would sweep a just-synced occurrence as "absent").
fn event_identity(item: &GraphEvent) -> (String, Option<String>) {
    match (&item.series_master_id, &item.original_start_time) {
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

/// Map a Graph recurrence pattern to a verbatim RRULE. Only patterns that
/// map losslessly are converted (daily / weekly / absolute monthly /
/// absolute yearly, with interval and count/until); anything else yields
/// `None` (the master is stored as a single row, per the spec's nullable
/// `rrule`).
fn map_rrule(recurrence: &GraphRecurrence) -> Option<String> {
    let pattern = recurrence.pattern.as_ref()?;
    let freq = match pattern.pattern_type.as_deref()? {
        "daily" => "DAILY",
        "weekly" => "WEEKLY",
        "absoluteMonthly" => "MONTHLY",
        "absoluteYearly" => "YEARLY",
        _ => return None,
    };
    let mut parts = vec![format!("FREQ={freq}")];
    if let Some(interval) = pattern.interval.filter(|interval| *interval > 1) {
        parts.push(format!("INTERVAL={interval}"));
    }
    if freq == "WEEKLY" {
        let days = pattern.days_of_week.as_ref()?;
        if days.is_empty() {
            return None;
        }
        let byday = days
            .iter()
            .map(|day| match day.to_ascii_lowercase().as_str() {
                "monday" => Some("MO"),
                "tuesday" => Some("TU"),
                "wednesday" => Some("WE"),
                "thursday" => Some("TH"),
                "friday" => Some("FR"),
                "saturday" => Some("SA"),
                "sunday" => Some("SU"),
                _ => None,
            })
            .collect::<Option<Vec<_>>>()?;
        parts.push(format!("BYDAY={}", byday.join(",")));
    }
    if matches!(freq, "MONTHLY" | "YEARLY") {
        parts.push(format!("BYMONTHDAY={}", pattern.day_of_month?));
    }
    if freq == "YEARLY" {
        parts.push(format!("BYMONTH={}", pattern.month?));
    }
    let range = recurrence.range.as_ref()?;
    if let Some(count) = range.number_of_occurrences {
        parts.push(format!("COUNT={count}"));
    } else if let Some(end_date) = range.end_date.as_deref() {
        if let Ok(date) = end_date.parse::<chrono::NaiveDate>() {
            let until = date
                .and_hms_opt(23, 59, 59)
                .unwrap()
                .and_utc()
                .format("%Y%m%dT%H%M%SZ");
            parts.push(format!("UNTIL={until}"));
        }
    }
    Some(parts.join(";"))
}

fn map_event(source: &CalendarSource, item: GraphEvent, now: DateTime<Utc>) -> CalendarEvent {
    // Instances/exceptions of a recurring master carry `seriesMasterId` and
    // `originalStartTime`; they become override rows on the master's external
    // id (the master itself is a separate `seriesMaster` entry). Identity is
    // derived by the shared `event_identity` helper so it always matches the
    // `present_keys` used by the absent-entry sweep.
    let (external_uid, recurrence_id) = event_identity(&item);
    let fallback_start = item
        .original_start_time
        .as_ref()
        .and_then(GraphTime::starts_at)
        .unwrap_or(now);
    let starts_at = item
        .start
        .as_ref()
        .and_then(GraphTime::starts_at)
        .unwrap_or(fallback_start);
    let ends_at = item
        .end
        .as_ref()
        .and_then(GraphTime::starts_at)
        .unwrap_or(starts_at + chrono::Duration::hours(1));
    let all_day = item.is_all_day.unwrap_or(false);
    let original_date = item
        .start
        .as_ref()
        .and_then(|start| start.date_time.as_deref())
        .and_then(|dt| dt.get(..10))
        .and_then(|date| date.parse::<chrono::NaiveDate>().ok())
        .filter(|_| all_day);
    let timezone = item
        .start
        .as_ref()
        .and_then(|start| start.time_zone.clone())
        .unwrap_or_else(|| "UTC".to_string());
    let rrule = if item.kind.as_deref() == Some("seriesMaster") {
        item.recurrence.as_ref().and_then(map_rrule)
    } else {
        None
    };
    let status = if item.is_cancelled.unwrap_or(false) {
        "cancelled"
    } else {
        "confirmed"
    };
    CalendarEvent {
        id: uuid::Uuid::new_v4(),
        tenant_id: source.tenant_id,
        owner_id: source.owner_id,
        source_id: source.id,
        external_uid: Some(external_uid),
        external_etag: item.change_key,
        recurrence_id,
        title: item.subject.clone().unwrap_or_default(),
        description: None,
        location: item.location.and_then(|loc| loc.display_name),
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

/// Soft-delete the mirrored row for a Graph `@removed` tombstone, keyed by the
/// same `(source_id, external_uid, recurrence_id)` identity used by the
/// upsert. Returns the number of rows removed (0 when the mirror was never
/// seen or is already deleted). A store failure is logged, not fatal: the next
/// full sync's absent-entry sweep is the backstop.
async fn soft_delete_removed_event(
    store: &MetadataStore,
    source: &CalendarSource,
    external_uid: &str,
    recurrence_id: Option<&str>,
) -> u64 {
    match sqlx::query(
        r#"
        UPDATE calendar_events
        SET deleted_at = now(), updated_at = now()
        WHERE source_id = $1
          AND external_uid = $2
          AND COALESCE(recurrence_id, '') = $3
          AND deleted_at IS NULL
        "#,
    )
    .bind(source.id)
    .bind(external_uid)
    .bind(recurrence_id.unwrap_or_default())
    .execute(store.pool())
    .await
    {
        Ok(result) => result.rows_affected(),
        Err(e) => {
            tracing::warn!(source_id = %source.id, "removed-tombstone soft delete failed: {e}");
            0
        }
    }
}

/// Extract the opaque paging/delta token from a provider next/delta link.
/// Only the token is kept and re-sent to the fixed API base (a provider
/// answer must never redirect fetches to a caller-controlled host).
fn token_from_link(link: &str, names: &[&str]) -> Option<String> {
    let query = link.split('?').nth(1)?;
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        let decoded_key = url::form_urlencoded::parse(key.as_bytes()).next()?.0;
        if names.iter().any(|name| decoded_key == *name) {
            return url::form_urlencoded::parse(value.as_bytes())
                .next()
                .map(|(value, _)| value.into_owned());
        }
    }
    None
}

/// One `calendarView/delta` request; classifies provider status codes so the
/// caller can react (400/410 delta-token errors → one full resync, 429 →
/// backoff, 401/403 → auth_required). Sends `Prefer: outlook.timezone="UTC"`
/// so Graph returns UTC `dateTime` wall clocks regardless of the user's
/// mailbox timezone (see `GraphTime::starts_at`).
async fn fetch_delta_page(
    client: &OutlookCalendarClient,
    access_token: &str,
    incremental: bool,
    cursor: Option<&str>,
    page_token: Option<&str>,
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
) -> Result<reqwest::Response, OutlookError> {
    let mut request = client
        .http
        .get(format!("{}/me/calendarView/delta", client.api_base))
        .header("Prefer", "outlook.timezone=\"UTC\"")
        .bearer_auth(access_token);
    // Mid-paging requests (a `$skiptoken` from `@odata.nextLink`) take
    // priority over the mode: Graph returns nextLinks on incremental delta
    // runs too, and re-sending the `$deltatoken` there would refetch page one
    // forever while holding the lease.
    if let Some(page_token) = page_token {
        request = request.query(&[("$skiptoken", page_token)]);
    } else if incremental {
        request = request.query(&[("$deltatoken", cursor.unwrap_or_default())]);
    } else {
        request = request
            .query(&[("startDateTime", window_start.to_rfc3339())])
            .query(&[("endDateTime", window_end.to_rfc3339())]);
    }
    let response = request
        .send()
        .await
        .map_err(|e| OutlookError::Api(e.to_string()))?;
    match response.status().as_u16() {
        200 => Ok(response),
        410 => Err(OutlookError::Gone),
        400 => {
            let body = response.text().await.unwrap_or_default();
            if body.contains("InvalidDeltaToken") || body.contains("syncStateNotFound") {
                Err(OutlookError::Gone)
            } else {
                Err(OutlookError::Api(format!("HTTP 400: {body}")))
            }
        }
        429 => Err(OutlookError::RateLimited {
            retry_after: parse_retry_after(response.headers()),
        }),
        401 | 403 => Err(OutlookError::AuthRequired),
        status => Err(OutlookError::Api(format!("HTTP {status}"))),
    }
}

fn parse_retry_after(headers: &reqwest::header::HeaderMap) -> Duration {
    headers
        .get("retry-after")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or(DEFAULT_RETRY_AFTER)
}

/// Run one delta sync pass for an Outlook source. Callers must hold the sync
/// lease (`worker_id`) — only the lease holder refreshes access tokens here,
/// and a rotated refresh token is written unconditionally (newer wins).
pub async fn sync_source(
    store: &MetadataStore,
    client: &OutlookCalendarClient,
    secret_key: &SecretEncryptionKey,
    source: &CalendarSource,
    config: &CalendarSyncConfig,
    worker_id: &str,
) -> SyncOutcome {
    // A source whose grant was revoked stays parked until reconnect: no work,
    // no publish, no watermark/status write (the worker treats `Parked` as
    // neither success nor failure).
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
        match client.refresh_access_token(&refresh_token).await {
            Ok(tokens) => {
                let rotated = match tokens.rotated_refresh_token() {
                    Some(rotated) => rustshare_crypto::encrypt_secret(rotated, secret_key).ok(),
                    None => None,
                };
                let access_enc =
                    match rustshare_crypto::encrypt_secret(tokens.access_token(), secret_key) {
                        Ok(enc) => enc,
                        Err(_) => {
                            return SyncOutcome::Failed("token encryption failed".to_string())
                        }
                    };
                if let Err(e) = store
                    .update_calendar_source_tokens(
                        source.id,
                        rotated.as_deref(),
                        &access_enc,
                        tokens.expires_at(),
                    )
                    .await
                {
                    return SyncOutcome::Failed(format!("failed to persist tokens: {e}"));
                }
                access_token = Some(tokens.access_token().to_string());
            }
            Err(OutlookError::AuthRequired) => return SyncOutcome::AuthRequired,
            Err(e) => return SyncOutcome::Failed(e.to_string()),
        }
    }
    let access_token = match access_token {
        Some(token) => token,
        None => return SyncOutcome::Failed("no access token available".to_string()),
    };

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
    // `sync_expired` tracks a rejected delta token mid-paging: the cursor is
    // nulled and the run retries exactly once as a full window sync.
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
        let mut removed_deleted = 0u64;
        let mut present_keys: Vec<String> = Vec::new();

        loop {
            let response = match fetch_delta_page(
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
                Err(OutlookError::Gone) if incremental && !sync_expired => {
                    sync_expired = true;
                    continue 'full_retry;
                }
                Err(OutlookError::Gone) => {
                    return SyncOutcome::Failed("delta token expired twice".to_string());
                }
                Err(OutlookError::RateLimited { retry_after }) => {
                    return SyncOutcome::RateLimited { retry_after };
                }
                Err(OutlookError::AuthRequired) => return SyncOutcome::AuthRequired,
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
            let page = match serde_json::from_str::<DeltaResponse>(&raw) {
                Ok(page) => page,
                Err(e) => return SyncOutcome::Failed(e.to_string()),
            };
            let now = Utc::now();
            for item in page.value.unwrap_or_default() {
                let (external_uid, recurrence_id) = event_identity(&item);
                // `calendarView/delta` deletion tombstones arrive as minimal
                // `@removed` objects with no subject/times. Mapping one would
                // upsert a bogus confirmed row over the real mirrored event;
                // instead soft-delete the mirror by identity and never treat
                // the tombstone as a live entry.
                if let Some(removed) = item.removed.as_ref() {
                    tracing::debug!(
                        source_id = %source.id,
                        external_uid = %external_uid,
                        reason = removed.reason.as_deref().unwrap_or("unknown"),
                        "graph removed tombstone; soft-deleting mirror"
                    );
                    removed_deleted += soft_delete_removed_event(
                        store,
                        source,
                        &external_uid,
                        recurrence_id.as_deref(),
                    )
                    .await;
                    continue;
                }
                present_keys.push(identity_key(&external_uid, recurrence_id.as_deref()));
                let event = map_event(source, item, now);
                match store.upsert_calendar_synced_event(&event).await {
                    Ok(_) => upserted += 1,
                    Err(e) => {
                        tracing::warn!(source_id = %source.id, "synced event upsert failed: {e}");
                    }
                }
            }
            if let Some(next_link) = page.next_link {
                match token_from_link(&next_link, &["skiptoken", "$skiptoken"]) {
                    Some(token) => {
                        page_token = Some(token);
                        continue;
                    }
                    None => {
                        return SyncOutcome::Failed(
                            "provider next link carried no skiptoken".to_string(),
                        )
                    }
                }
            }
            // Final page: the delta link is the next incremental cursor.
            next_sync_token = page
                .delta_link
                .as_deref()
                .and_then(|link| token_from_link(link, &["deltatoken", "$deltatoken"]));
            break;
        }

        // The absent-key sweep propagates deletions on FULL runs only: a full
        // window payload is the complete set of live events in the window, so
        // any in-window mirrored row missing from it was deleted upstream.
        // Incremental deltas carry only CHANGED entries, so an absent-key
        // sweep there would soft-delete every unchanged mirrored event;
        // upstream deletions on incremental runs are carried explicitly as
        // `@removed` tombstones (`removed_deleted` above), not inferred from
        // absence.
        let mut soft_deleted = removed_deleted;
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
    fn parses_delta_token_from_final_page_link() {
        let body = serde_json::json!({
            "value": [],
            "@odata.deltaLink": "https://graph.example/v1.0/me/calendarView/delta?$deltatoken=cursor-after-full"
        });
        let page: DeltaResponse = serde_json::from_value(body).unwrap();
        assert!(page.next_link.is_none());
        assert_eq!(
            token_from_link(
                page.delta_link.as_deref().unwrap(),
                &["deltatoken", "$deltatoken"]
            )
            .as_deref(),
            Some("cursor-after-full")
        );
    }

    #[test]
    fn maps_absolute_monthly_pattern_to_rrule() {
        let recurrence: GraphRecurrence = serde_json::from_value(serde_json::json!({
            "pattern": {"type": "absoluteMonthly", "interval": 1, "dayOfMonth": 15},
            "range": {"startDate": "2026-01-15", "numberOfOccurrences": 6}
        }))
        .unwrap();
        assert_eq!(
            map_rrule(&recurrence).as_deref(),
            Some("FREQ=MONTHLY;BYMONTHDAY=15;COUNT=6")
        );
    }

    #[test]
    fn parses_graph_fractional_datetime_without_offset() {
        let time: GraphTime = serde_json::from_value(serde_json::json!({
            "dateTime": "2017-08-29T04:00:00.0000000",
            "timeZone": "UTC"
        }))
        .unwrap();
        assert_eq!(
            time.starts_at(),
            Some("2017-08-29T04:00:00Z".parse().unwrap())
        );
    }

    #[test]
    fn parses_offset_suffixed_datetime() {
        let time: GraphTime = serde_json::from_value(serde_json::json!({
            "dateTime": "2026-10-05T14:00:00Z",
            "timeZone": "UTC"
        }))
        .unwrap();
        assert_eq!(
            time.starts_at(),
            Some("2026-10-05T14:00:00Z".parse().unwrap())
        );
    }

    #[test]
    fn event_identity_matches_between_map_and_present_keys() {
        let item: GraphEvent = serde_json::from_value(serde_json::json!({
            "id": "master-1",
            "type": "occurrence",
            "seriesMasterId": "master-1",
            "originalStartTime": {
                "dateTime": "2026-10-12T14:00:00.0000000",
                "timeZone": "UTC"
            }
        }))
        .unwrap();
        let (external_uid, recurrence_id) = event_identity(&item);
        assert_eq!(external_uid, "master-1");
        assert_eq!(recurrence_id.as_deref(), Some("2026-10-12T14:00:00Z"));
        assert_eq!(
            identity_key(&external_uid, recurrence_id.as_deref()),
            "master-1|2026-10-12T14:00:00Z"
        );
    }

    #[test]
    fn unlossy_pattern_maps_to_no_rrule() {
        let recurrence: GraphRecurrence = serde_json::from_value(serde_json::json!({
            "pattern": {"type": "relativeMonthly", "interval": 1, "daysOfWeek": ["tuesday"], "index": "first"},
            "range": {"startDate": "2026-01-15", "noEndDate": true}
        }))
        .unwrap();
        assert_eq!(map_rrule(&recurrence), None);
    }
}
