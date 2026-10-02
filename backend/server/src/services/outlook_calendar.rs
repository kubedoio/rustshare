//! Microsoft/Outlook Calendar OAuth client and read-only delta sync (issue
//! #315).
//!
//! The provider-facing HTTP lives behind fixed, allowlisted base URLs (no
//! caller-controlled hosts, so no SSRF surface); tests point the bases at a
//! local mock server. Token material is decrypted only in-memory and never
//! logged or returned. Sync mirrors the Google path
//! (`google_calendar::SyncOutcome` taxonomy, cursor in
//! `calendar_sync_states.cursor_value` with `cursor_kind = 'ms_delta_token'`).

use std::time::Duration;

use chrono::{DateTime, Utc};
use rustshare_core::domain::{CalendarEvent, CalendarSource};
use rustshare_crypto::SecretEncryptionKey;
use rustshare_storage::MetadataStore;
use serde::Deserialize;

use crate::services::google_calendar::{CalendarSyncConfig, SyncOutcome, TOKEN_EXPIRY_MARGIN};

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
    pub revoke_url: String,
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
            http: reqwest::Client::new(),
            client_id,
            client_secret,
            redirect_url: format!("{public_url}/api/v1/calendar/oauth/outlook/callback"),
            auth_base: AUTH_BASE.to_string(),
            token_url: TOKEN_URL.to_string(),
            api_base: API_BASE.to_string(),
            me_url: format!("{API_BASE}/me"),
            revoke_url: format!("{API_BASE}/me/revokeSignInSessions"),
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

    /// Best-effort sign-in-session revocation (disconnect). **Side effect:**
    /// `revokeSignInSessions` invalidates ALL of the user's Microsoft
    /// sign-in sessions across every Entra-integrated app — not just this
    /// Elembra grant. This needs a frontend warning and an ADR note; a
    /// grant-scoped alternative would require admin-consent Graph permissions
    /// beyond `Calendars.Read`. Errors are logged by the caller and never
    /// propagated to the user.
    pub async fn revoke_token(&self, access_token: &str) -> bool {
        match self
            .http
            .post(&self.revoke_url)
            .bearer_auth(access_token)
            .send()
            .await
        {
            Ok(response) => response.status().is_success(),
            Err(_) => false,
        }
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
    fn starts_at(&self) -> Option<DateTime<Utc>> {
        // Graph returns `dateTime` (ISO, often without offset) plus
        // `timeZone`; the UTC instant follows from the wall clock in that
        // zone, mirroring the wall-clock read-time expansion. Providers send
        // Z-suffixed values for UTC-zone events, so parse RFC 3339 first and
        // only fall back to a naive parse when there is genuinely no offset.
        if let Some(dt) = &self.date_time {
            if let Ok(parsed) = DateTime::parse_from_rfc3339(dt) {
                return Some(parsed.with_timezone(&Utc));
            }
            if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(dt, "%Y-%m-%dT%H:%M:%S") {
                let tz = self
                    .time_zone
                    .as_deref()
                    .and_then(|name| name.parse::<chrono_tz::Tz>().ok())
                    .unwrap_or(chrono_tz::Tz::UTC);
                return Some(
                    naive
                        .and_local_timezone(tz)
                        .single()
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or(naive.and_utc()),
                );
            }
        }
        None
    }
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
    // id (the master itself is a separate `seriesMaster` entry).
    let (external_uid, recurrence_id) = match (&item.series_master_id, &item.original_start_time) {
        (Some(master_id), Some(original)) => (
            master_id.clone(),
            Some(
                original
                    .starts_at()
                    .unwrap_or(now)
                    .to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true),
            ),
        ),
        _ => (item.id.clone(), None),
    };
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
/// backoff, 401 → auth_required).
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
        401 => Err(OutlookError::AuthRequired),
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
    // A source whose grant was revoked stays a no-op until reconnect.
    if source.status == "auth_required" {
        tracing::debug!(source_id = %source.id, "source auth_required; skipping sync");
        return SyncOutcome::Completed {
            upserted: 0,
            soft_deleted: 0,
            next_sync_token: None,
        };
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
            if store
                .heartbeat_calendar_source_lease(source.id, worker_id)
                .await
                .is_err()
            {
                return SyncOutcome::Failed("lease heartbeat failed".to_string());
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
                present_keys.push(format!(
                    "{}|{}",
                    item.series_master_id.as_deref().unwrap_or(&item.id),
                    item.original_start_time
                        .as_ref()
                        .and_then(GraphTime::starts_at)
                        .map(|dt| dt.to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true))
                        .unwrap_or_default()
                ));
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

        // Provider deletions propagate on FULL runs only: a full window
        // payload is the complete set of live events in the window, so any
        // in-window mirrored row missing from it was deleted upstream.
        // Incremental deltas carry only CHANGED entries — an absent-key sweep
        // there would soft-delete every unchanged mirrored event (upstream
        // deletions already arrive as isCancelled tombstones).
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
    fn unlossy_pattern_maps_to_no_rrule() {
        let recurrence: GraphRecurrence = serde_json::from_value(serde_json::json!({
            "pattern": {"type": "relativeMonthly", "interval": 1, "daysOfWeek": ["tuesday"], "index": "first"},
            "range": {"startDate": "2026-01-15", "noEndDate": true}
        }))
        .unwrap();
        assert_eq!(map_rrule(&recurrence), None);
    }
}
