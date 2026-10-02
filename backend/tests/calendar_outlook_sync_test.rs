//! DB-backed integration suite for the Calendar Application Microsoft/Outlook
//! OAuth connect flow and read-only delta sync (issue #315, Task 5).
//!
//! Covers: the single-use user-bound OAuth state lifecycle (mismatch, expiry,
//! reuse), token exchange against a local mock Microsoft identity/Graph
//! server, multi-page full sync materializing events, incremental delta
//! applying updates + cancelled tombstones + soft-deleting absent entries, an
//! invalid delta token triggering exactly one full resync, a revoked grant
//! flipping `auth_required` with further runs no-oping, lease safety for
//! concurrent same-source claims (only the lease holder refreshes tokens; a
//! rotated refresh token is written unconditionally), and the absence of
//! token plaintext from every response/assertable surface.
//!
//! DB-backed and `#[ignore]`d; run against the dev database (migrations
//! applied) with `--test-threads=1`:
//!
//!   set -a; . ./backend/.env; set +a; SQLX_OFFLINE=true \
//!     cargo test -p rustshare-server --test calendar_outlook_sync_test -- \
//!       --ignored --test-threads=1
//!
//! Every test takes the shared `SERIAL` guard and cleans up exactly the rows
//! it created under fresh tenants.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, LazyLock};

use axum::extract::State as AxumState;
use axum::routing::{get, post};
use axum::{Form, Json, Router};
use rustshare_core::domain::{CalendarEvent, CalendarSource, User};
use rustshare_crypto::SecretEncryptionKey;
use rustshare_server::services::calendar_service::{CalendarError, CalendarService};
use rustshare_server::services::google_calendar::{CalendarSyncConfig, SyncOutcome};
use rustshare_server::services::outlook_calendar::OutlookCalendarClient;
use rustshare_storage::MetadataStore;
use serde_json::{json, Value};
use sqlx::PgPool;
use tokio::sync::Mutex;
use uuid::Uuid;

/// Serializes the tests within this binary (same convention as the
/// calendar-api suite).
static SERIAL: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(|| tokio::sync::Mutex::new(()));

const WORKER_A: &str = "calendar-sync-test-a";
const TEST_REFRESH_TOKEN: &str = "test-refresh-token-value";
const TEST_ACCESS_TOKEN: &str = "test-access-token-value";
const SCOPE_READONLY: &str = "offline_access Calendars.Read";

fn sync_config() -> CalendarSyncConfig {
    CalendarSyncConfig {
        past_days: 90,
        future_days: 365,
    }
}

// ---------------------------------------------------------------------------
// Mock Microsoft identity + Graph server
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct MockResponse {
    status: axum::http::StatusCode,
    body: Value,
    retry_after: Option<u64>,
}

impl MockResponse {
    fn ok(body: Value) -> Self {
        Self {
            status: axum::http::StatusCode::OK,
            body,
            retry_after: None,
        }
    }

    fn invalid_delta_token() -> Self {
        Self {
            status: axum::http::StatusCode::BAD_REQUEST,
            body: json!({"error": {"code": "InvalidDeltaToken", "message": "delta token is invalid"}}),
            retry_after: None,
        }
    }

    /// Graph `ErrorAccessDenied` (HTTP 403), e.g. consent withdrawn.
    fn access_denied() -> Self {
        Self {
            status: axum::http::StatusCode::FORBIDDEN,
            body: json!({"error": {"code": "ErrorAccessDenied", "message": "Access is denied."}}),
            retry_after: None,
        }
    }

    fn internal_error() -> Self {
        Self {
            status: axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            body: json!({"error": {"code": "UnknownError", "message": "transient backend error"}}),
            retry_after: None,
        }
    }

    fn rate_limited(retry_after: u64) -> Self {
        Self {
            status: axum::http::StatusCode::TOO_MANY_REQUESTS,
            body: json!({"error": {"code": "TooManyRequests", "message": "rate limited"}}),
            retry_after: Some(retry_after),
        }
    }
}

#[derive(Default)]
struct MockState {
    /// Queued responses for the calendarView/delta endpoint (popped FIFO).
    delta_queue: Mutex<VecDeque<MockResponse>>,
    /// Query strings of delta requests, in arrival order.
    delta_requests: Mutex<Vec<String>>,
    /// `Prefer` header of each delta request, in arrival order.
    delta_prefer_headers: Mutex<Vec<String>>,
    /// Number of calls to the (now unused) revoke endpoint.
    revoke_hits: Mutex<u32>,
    /// Refresh-token values seen at the token endpoint (rotation tracking).
    refresh_tokens_seen: Mutex<Vec<String>>,
    token_requests: Mutex<u32>,
    /// When true, the token endpoint answers token requests with
    /// `invalid_grant` (revoked grant).
    revoke_grants: Mutex<bool>,
    /// When true, refresh responses carry a rotated refresh token.
    rotate_refresh: Mutex<bool>,
    /// When true, the authorization-code exchange omits the refresh token.
    omit_refresh_token: Mutex<bool>,
    me_email: Mutex<String>,
}

impl MockState {
    async fn token_hits(&self) -> u32 {
        *self.token_requests.lock().await
    }
}

fn spawn_mock_microsoft() -> (String, Arc<MockState>) {
    let state = Arc::new(MockState::default());
    let app = Router::new()
        .route(
            "/token",
            post(
                |AxumState(state): AxumState<Arc<MockState>>,
                 form: Form<HashMap<String, String>>| {
                    async move {
                        *state.token_requests.lock().await += 1;
                        if let Some(refresh) = form.get("refresh_token") {
                            state.refresh_tokens_seen.lock().await.push(refresh.clone());
                        }
                        if *state.revoke_grants.lock().await {
                            return (
                                axum::http::StatusCode::BAD_REQUEST,
                                Json(json!({"error": "invalid_grant", "error_description": "revoked"})),
                            );
                        }
                        let rotate = *state.rotate_refresh.lock().await;
                        let refresh_token = if rotate {
                            "rotated-refresh-token-value"
                        } else {
                            TEST_REFRESH_TOKEN
                        };
                        let grant_type = form.get("grant_type").cloned().unwrap_or_default();
                        let omit_refresh = *state.omit_refresh_token.lock().await;
                        let body = if grant_type == "refresh_token" {
                            let mut body = json!({
                                "access_token": "fresh-access-token-value",
                                "expires_in": 3600,
                                "token_type": "Bearer"
                            });
                            if rotate {
                                body["refresh_token"] = json!("rotated-refresh-token-value");
                            }
                            body
                        } else if omit_refresh {
                            json!({
                                "access_token": TEST_ACCESS_TOKEN,
                                "expires_in": 3600,
                                "token_type": "Bearer",
                                "scope": SCOPE_READONLY
                            })
                        } else {
                            json!({
                                "access_token": TEST_ACCESS_TOKEN,
                                "refresh_token": refresh_token,
                                "expires_in": 3600,
                                "token_type": "Bearer",
                                "scope": SCOPE_READONLY
                            })
                        };
                        (axum::http::StatusCode::OK, Json(body))
                    }
                },
            ),
        )
        .route(
            "/me",
            get(|AxumState(state): AxumState<Arc<MockState>>| async move {
                let email = state.me_email.lock().await.clone();
                Json(json!({"mail": email, "userPrincipalName": "upn@example.test"}))
            }),
        )
        .route(
            "/me/revokeSignInSessions",
            post(|AxumState(state): AxumState<Arc<MockState>>| async move {
                *state.revoke_hits.lock().await += 1;
                Json(json!({}))
            }),
        )
        .route(
            "/me/calendarView/delta",
            get(
                |AxumState(state): AxumState<Arc<MockState>>,
                 headers: axum::http::HeaderMap,
                 req: axum::extract::Query<HashMap<String, String>>| async move {
                    let query = req.0;
                    state
                        .delta_requests
                        .lock()
                        .await
                        .push(serde_urlencoded_params(&query));
                    state.delta_prefer_headers.lock().await.push(
                        headers
                            .get("prefer")
                            .and_then(|value| value.to_str().ok())
                            .unwrap_or_default()
                            .to_string(),
                    );
                    let queued = state.delta_queue.lock().await.pop_front();
                    let response = queued.unwrap_or_else(|| {
                        // Default: empty result; a deltaToken request advances
                        // the cursor, a window request establishes one.
                        let mut body = json!({"value": []});
                        if let Some(cursor) = query.get("$deltatoken") {
                            body["@odata.deltaLink"] = json!(format!(
                                "http://graph.example/v1.0/me/calendarView/delta?$deltatoken={cursor}-advanced"
                            ));
                        } else {
                            body["@odata.deltaLink"] = json!(
                                "http://graph.example/v1.0/me/calendarView/delta?$deltatoken=initial-delta-token"
                            );
                        }
                        MockResponse::ok(body)
                    });
                    let mut builder = axum::response::Response::builder().status(response.status);
                    if let Some(retry_after) = response.retry_after {
                        builder = builder.header("retry-after", retry_after.to_string());
                    }
                    builder
                        .header("content-type", "application/json")
                        .body(axum::body::Body::from(response.body.to_string()))
                        .unwrap()
                },
            ),
        )
        .with_state(state.clone());

    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind mock server");
    listener.set_nonblocking(true).unwrap();
    let listener = tokio::net::TcpListener::from_std(listener).unwrap();
    let address = listener.local_addr().unwrap();
    let server_state = state.clone();
    tokio::spawn(async move {
        let _ = server_state; // keep alive with the server task
        let _ = axum::serve(listener, app).await;
    });
    (format!("http://{address}"), state)
}

/// Deterministic query serialization for request assertions.
fn serde_urlencoded_params(query: &HashMap<String, String>) -> String {
    let mut pairs: Vec<(&String, &String)> = query.iter().collect();
    pairs.sort();
    pairs
        .into_iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("&")
}

fn mock_client(base: &str) -> OutlookCalendarClient {
    let mut client = OutlookCalendarClient::new(
        "mock-client-id".to_string(),
        "mock-client-secret".to_string(),
        "http://elembra.test",
    );
    client.token_url = format!("{base}/token");
    client.api_base = base.to_string();
    client.me_url = format!("{base}/me");
    client.auth_base = format!("{base}/authorize");
    client
}

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

struct Harness {
    pool: PgPool,
    store: Arc<MetadataStore>,
    secret_key: Arc<SecretEncryptionKey>,
    outbox: Arc<rustshare_storage::OutboxStore>,
    tenant_id: Uuid,
}

impl Harness {
    async fn new() -> Self {
        dotenvy::dotenv().ok();
        let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgres://rustshare:changeme@localhost:5432/rustshare".to_string()
        });
        let pool = PgPool::connect(&database_url)
            .await
            .expect("Failed to connect to database");
        let store = Arc::new(MetadataStore::new(pool.clone()));
        let outbox = Arc::new(rustshare_storage::OutboxStore::new(
            pool.clone(),
            Arc::new(rustshare_core::domain::ApplicationRegistry::first_party().unwrap()),
        ));
        let tenant_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO tenants (id, name, created_at, updated_at) VALUES ($1, $2, NOW(), NOW()) ON CONFLICT (id) DO NOTHING",
        )
        .bind(tenant_id)
        .bind(format!("Calendar Outlook Sync Test Tenant {tenant_id}"))
        .execute(&pool)
        .await
        .expect("Failed to create test tenant");
        sqlx::query(
            "INSERT INTO application_enablements (tenant_id, workspace_id, application_id, enabled)
             VALUES ($1, $1, 'io.elembra.calendar', true)",
        )
        .bind(tenant_id)
        .execute(&pool)
        .await
        .expect("Failed to enable calendar application");
        Self {
            pool,
            store,
            secret_key: Arc::new(SecretEncryptionKey::from_bytes([7u8; 32])),
            outbox,
            tenant_id,
        }
    }

    async fn create_user(&self, label: &str) -> User {
        let username = format!("{}-{}", label, Uuid::new_v4());
        let user = User::new(
            username.clone(),
            format!("{username} Display"),
            "test_password_hash".to_string(),
            format!("{username}@test.local"),
            false,
            10_737_418_240,
            self.tenant_id,
        );
        self.store
            .create_user(&user)
            .await
            .expect("Failed to create test user");
        user
    }

    /// Create an outlook source with valid unexpired tokens against the mock.
    async fn create_outlook_source(&self, user_id: Uuid) -> CalendarSource {
        self.create_outlook_source_with_access_expiry(user_id, 3600)
            .await
    }

    async fn create_outlook_source_with_access_expiry(
        &self,
        user_id: Uuid,
        access_expires_in_secs: i64,
    ) -> CalendarSource {
        let refresh_enc = rustshare_crypto::encrypt_secret(TEST_REFRESH_TOKEN, &self.secret_key)
            .expect("encrypt refresh token");
        let access_enc = rustshare_crypto::encrypt_secret(TEST_ACCESS_TOKEN, &self.secret_key)
            .expect("encrypt access token");
        self.store
            .create_oauth_calendar_source(
                self.tenant_id,
                user_id,
                "outlook",
                "Outlook (sync-user@test.local)",
                "sync-user@test.local",
                "primary",
                &refresh_enc,
                &access_enc,
                chrono::Utc::now() + chrono::Duration::seconds(access_expires_in_secs),
                SCOPE_READONLY,
            )
            .await
            .expect("create oauth calendar source")
    }

    async fn reload_source(&self, source_id: Uuid) -> CalendarSource {
        let row = sqlx::query_as!(
            rustshare_core::domain::CalendarSource,
            r#"SELECT id, tenant_id, owner_id, kind, display_name, external_account,
                external_calendar_id, refresh_token_enc, access_token_enc,
                access_token_expires_at, scopes, is_enabled, last_synced_at,
                last_error, status, deleted_at, created_at, updated_at
            FROM calendar_sources WHERE id = $1"#,
            source_id,
        )
        .fetch_one(&self.pool)
        .await
        .expect("reload source");
        row
    }

    async fn list_source_events(&self, source_id: Uuid) -> Vec<CalendarEvent> {
        sqlx::query_as!(
            CalendarEvent,
            r#"SELECT id, tenant_id, owner_id, source_id, external_uid, external_etag,
                recurrence_id, title, description, location, starts_at, ends_at,
                all_day, original_date, timezone, rrule, status, read_only, raw,
                deleted_at, created_at, updated_at
            FROM calendar_events WHERE source_id = $1 AND deleted_at IS NULL
            ORDER BY external_uid, COALESCE(recurrence_id, '')"#,
            source_id,
        )
        .fetch_all(&self.pool)
        .await
        .expect("list source events")
    }

    async fn cleanup(&self) {
        let tenant_id = self.tenant_id;
        sqlx::query(
            "DELETE FROM calendar_sync_states WHERE source_id IN
                (SELECT id FROM calendar_sources WHERE tenant_id = $1)",
        )
        .bind(tenant_id)
        .execute(&self.pool)
        .await
        .expect("cleanup calendar_sync_states");
        sqlx::query("DELETE FROM calendar_oauth_states WHERE tenant_id = $1")
            .bind(tenant_id)
            .execute(&self.pool)
            .await
            .expect("cleanup calendar_oauth_states");
        sqlx::query("DELETE FROM integration_deliveries WHERE tenant_id = $1")
            .bind(tenant_id)
            .execute(&self.pool)
            .await
            .expect("cleanup integration_deliveries");
        sqlx::query("DELETE FROM integration_outbox WHERE tenant_id = $1")
            .bind(tenant_id)
            .execute(&self.pool)
            .await
            .expect("cleanup integration_outbox");
        for table in [
            "calendar_import_jobs",
            "calendar_events",
            "calendar_sources",
        ] {
            sqlx::query(&format!("DELETE FROM {table} WHERE tenant_id = $1"))
                .bind(tenant_id)
                .execute(&self.pool)
                .await
                .unwrap_or_else(|e| panic!("cleanup {table}: {e}"));
        }
        sqlx::query("DELETE FROM users WHERE tenant_id = $1")
            .bind(tenant_id)
            .execute(&self.pool)
            .await
            .expect("cleanup users");
        sqlx::query("DELETE FROM application_enablements WHERE tenant_id = $1")
            .bind(tenant_id)
            .execute(&self.pool)
            .await
            .expect("cleanup enablements");
        sqlx::query("DELETE FROM tenants WHERE id = $1")
            .bind(tenant_id)
            .execute(&self.pool)
            .await
            .expect("cleanup tenant");
    }
}

/// Standard delta entry used across tests (Z-suffixed RFC 3339 dates).
fn graph_event(id: &str, subject: &str, start: &str, end: &str) -> Value {
    json!({
        "id": id,
        "type": "singleInstance",
        "changeKey": format!("change-{id}"),
        "subject": subject,
        "start": {"dateTime": start, "timeZone": "UTC"},
        "end": {"dateTime": end, "timeZone": "UTC"},
        "isAllDay": false,
    })
}

/// Graph's real `dateTime` shape: an offset-less wall clock with 7 fractional
/// digits, zone carried separately in `timeZone` (e.g.
/// `2017-08-29T04:00:00.0000000`).
fn graph_time(date_time: &str, time_zone: &str) -> Value {
    json!({"dateTime": date_time, "timeZone": time_zone})
}

// ---------------------------------------------------------------------------
// OAuth state + connect flow
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn connect_unconfigured_provider_returns_503_error() {
    let _guard = SERIAL.lock().await;
    let harness = Harness::new().await;
    let user = harness.create_user("cal_o_unconf").await;
    let service = CalendarService::new(harness.store.clone(), harness.secret_key.clone());

    let result = service
        .begin_connect(
            harness.tenant_id,
            user.id,
            rustshare_core::domain::CalendarSourceKind::Outlook,
        )
        .await;
    assert!(
        matches!(result, Err(CalendarError::OAuthNotConfigured(_))),
        "unconfigured provider must surface OAuthNotConfigured, got {result:?}"
    );
    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn connect_flow_uses_single_use_state_and_stores_encrypted_tokens() {
    let _guard = SERIAL.lock().await;
    let (base, mock) = spawn_mock_microsoft();
    *mock.me_email.lock().await = "sync-user@test.local".to_string();
    let harness = Harness::new().await;
    let user = harness.create_user("cal_o_connect").await;
    let mut service = CalendarService::new(harness.store.clone(), harness.secret_key.clone());
    service.configure_outlook(Some(mock_client(&base)));

    // Begin: authorize URL embeds a fresh single-use state bound to the user.
    let authorize_url = service
        .begin_connect(
            harness.tenant_id,
            user.id,
            rustshare_core::domain::CalendarSourceKind::Outlook,
        )
        .await
        .expect("begin_connect");
    assert!(authorize_url.starts_with(&format!("{base}/authorize?")));
    assert!(authorize_url.contains("response_type=code"));
    assert!(
        authorize_url.contains("scope=offline_access+Calendars.Read"),
        "authorize URL must request offline_access and Calendars.Read: {authorize_url}"
    );
    assert!(authorize_url.contains("redirect_uri="));
    let state_param = authorize_url
        .split("state=")
        .nth(1)
        .and_then(|rest| rest.split('&').next())
        .expect("authorize URL carries a state param")
        .to_string();

    // Complete: exchange + /me + encrypted token storage + due-now sync.
    let source = service
        .complete_outlook_connect(&state_param, "mock-auth-code")
        .await
        .expect("complete_outlook_connect");
    assert_eq!(source.kind, "outlook");
    assert_eq!(
        source.external_account.as_deref(),
        Some("sync-user@test.local")
    );
    assert_eq!(source.external_calendar_id.as_deref(), Some("primary"));
    let sync_state = harness
        .store
        .get_calendar_sync_state(source.id)
        .await
        .expect("sync state")
        .expect("source has sync state");
    assert!(sync_state.next_sync_at <= chrono::Utc::now() + chrono::Duration::seconds(5));

    // Reuse of the consumed state is rejected.
    let reuse = service
        .complete_outlook_connect(&state_param, "mock-auth-code")
        .await;
    assert!(
        matches!(reuse, Err(CalendarError::OAuthStateInvalid)),
        "state reuse must fail, got {reuse:?}"
    );

    // Token plaintext never appears in the response surface; stored values
    // are AES-256-GCM ciphertext that decrypts back to the mock tokens.
    let serialized = serde_json::to_value(&source).expect("serialize source");
    assert!(serialized.get("refresh_token_enc").is_none());
    assert!(serialized.get("access_token_enc").is_none());
    let stored = harness.reload_source(source.id).await;
    let stored_refresh = stored.refresh_token_enc.expect("stored refresh token");
    assert!(!stored_refresh.contains(TEST_REFRESH_TOKEN));
    let decrypted = rustshare_crypto::decrypt_secret(&stored_refresh, &harness.secret_key)
        .expect("decrypt stored refresh token");
    assert_eq!(decrypted, TEST_REFRESH_TOKEN);
    let stored_access = stored.access_token_enc.expect("stored access token");
    assert!(!stored_access.contains(TEST_ACCESS_TOKEN));

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn oauth_state_expiry_and_mismatch_are_rejected() {
    let _guard = SERIAL.lock().await;
    let harness = Harness::new().await;
    let user = harness.create_user("cal_o_state").await;

    // Unknown state: nothing to consume.
    let none = harness
        .store
        .consume_calendar_oauth_state("no-such-state")
        .await
        .expect("consume unknown state");
    assert!(none.is_none());

    // Expired state: insert directly, then consume must refuse.
    harness
        .store
        .insert_calendar_oauth_state(
            "expired-state",
            harness.tenant_id,
            user.id,
            "outlook",
            chrono::Utc::now() - chrono::Duration::minutes(1),
        )
        .await
        .expect("insert expired state");
    let expired = harness
        .store
        .consume_calendar_oauth_state("expired-state")
        .await
        .expect("consume expired state");
    assert!(expired.is_none(), "expired state must not be consumable");

    // Live state consumes exactly once and is bound to the inserting user.
    let expires = chrono::Utc::now() + chrono::Duration::minutes(10);
    harness
        .store
        .insert_calendar_oauth_state("live-state", harness.tenant_id, user.id, "outlook", expires)
        .await
        .expect("insert live state");
    let consumed = harness
        .store
        .consume_calendar_oauth_state("live-state")
        .await
        .expect("consume live state")
        .expect("live state row");
    assert_eq!(consumed.owner_id, user.id);
    assert_eq!(consumed.kind, "outlook");
    let again = harness
        .store
        .consume_calendar_oauth_state("live-state")
        .await
        .expect("re-consume");
    assert!(again.is_none(), "state is single-use");

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn connect_without_refresh_token_fails_closed() {
    let _guard = SERIAL.lock().await;
    let (base, mock) = spawn_mock_microsoft();
    *mock.me_email.lock().await = "no-refresh@test.local".to_string();
    *mock.omit_refresh_token.lock().await = true;
    let harness = Harness::new().await;
    let user = harness.create_user("cal_o_norefresh").await;
    let mut service = CalendarService::new(harness.store.clone(), harness.secret_key.clone());
    service.configure_outlook(Some(mock_client(&base)));

    let authorize_url = service
        .begin_connect(
            harness.tenant_id,
            user.id,
            rustshare_core::domain::CalendarSourceKind::Outlook,
        )
        .await
        .expect("begin_connect");
    let state_param = authorize_url
        .split("state=")
        .nth(1)
        .and_then(|rest| rest.split('&').next())
        .expect("authorize URL carries a state param")
        .to_string();

    let result = service
        .complete_outlook_connect(&state_param, "mock-auth-code")
        .await;
    assert!(
        matches!(result, Err(CalendarError::OAuthFailed(_))),
        "missing refresh token must fail closed, got {result:?}"
    );
    let sources = harness
        .store
        .list_calendar_sources(harness.tenant_id, user.id)
        .await
        .expect("list sources");
    assert!(
        sources.is_empty(),
        "no source row may be created without a refresh token"
    );

    harness.cleanup().await;
}

/// Lease acquisition helper standing in for `claim_due_calendar_source` when
/// a test drives the worker path (`run_sync`) directly.
async fn acquire_lease(harness: &Harness, source_id: Uuid, worker: &str) {
    sqlx::query(
        "UPDATE calendar_sync_states SET locked_at = NOW(), locked_by = $2
         WHERE source_id = $1",
    )
    .bind(source_id)
    .bind(worker)
    .execute(&harness.pool)
    .await
    .expect("acquire lease");
}

/// Worker-path (run_sync) coverage for a revoked grant: the first run flips
/// the source to `auth_required`; a later full worker run keeps it parked
/// and performs zero provider HTTP calls.
#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn worker_run_keeps_auth_required_parked_without_http() {
    let _guard = SERIAL.lock().await;
    let (base, mock) = spawn_mock_microsoft();
    *mock.revoke_grants.lock().await = true;
    let harness = Harness::new().await;
    let user = harness.create_user("cal_o_parked").await;
    let source = harness
        .create_outlook_source_with_access_expiry(user.id, -60)
        .await;
    let client = Arc::new(mock_client(&base));
    let worker = "calendar-sync-parked";

    let source_id = source.id;
    // Run 1: refresh attempt → invalid_grant → auth_required.
    acquire_lease(&harness, source_id, worker).await;
    rustshare_server::calendar_sync_worker::run_sync(
        harness.store.clone(),
        harness.secret_key.clone(),
        None,
        Some(client.clone()),
        harness.outbox.clone(),
        source,
        sync_config(),
        worker.to_string(),
    )
    .await;
    let parked = harness.reload_source(source_id).await;
    assert_eq!(parked.status, "auth_required");
    let token_hits_after_first = mock.token_hits().await;

    // Run 2: full worker run over the parked source — no provider traffic,
    // status untouched (the Completed no-op arm must not write `healthy`).
    acquire_lease(&harness, source_id, worker).await;
    rustshare_server::calendar_sync_worker::run_sync(
        harness.store.clone(),
        harness.secret_key.clone(),
        None,
        Some(client),
        harness.outbox.clone(),
        harness.reload_source(source_id).await,
        sync_config(),
        worker.to_string(),
    )
    .await;
    let still_parked = harness.reload_source(source_id).await;
    assert_eq!(
        still_parked.status, "auth_required",
        "a parked source must stay parked across worker runs"
    );
    assert_eq!(
        mock.token_hits().await,
        token_hits_after_first,
        "parked run must not call the token endpoint"
    );
    assert!(
        mock.delta_requests.lock().await.is_empty(),
        "parked run must not call the delta API"
    );

    harness.cleanup().await;
}

/// A transient failure mid-incremental must preserve the stored delta cursor
/// so the next run stays incremental (a forced full resync is heavier and
/// its absent-entry sweep covers only the synced window).
#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn worker_failed_run_preserves_incremental_cursor() {
    let _guard = SERIAL.lock().await;
    let (base, mock) = spawn_mock_microsoft();
    let harness = Harness::new().await;
    let user = harness.create_user("cal_o_failed").await;
    let source = harness.create_outlook_source(user.id).await;
    let client = Arc::new(mock_client(&base));
    let worker = "calendar-sync-failed";

    // Run 1: full sync establishes a cursor.
    {
        let mut queue = mock.delta_queue.lock().await;
        queue.push_back(MockResponse::ok(json!({
            "value": [graph_event("evt-f1", "First", "2026-10-05T14:00:00Z", "2026-10-05T15:00:00Z")],
            "@odata.deltaLink": "http://graph.example/v1.0/me/calendarView/delta?$deltatoken=cursor-kept"
        })));
    }
    acquire_lease(&harness, source.id, worker).await;
    let source_id = source.id;
    rustshare_server::calendar_sync_worker::run_sync(
        harness.store.clone(),
        harness.secret_key.clone(),
        None,
        Some(client.clone()),
        harness.outbox.clone(),
        source,
        sync_config(),
        worker.to_string(),
    )
    .await;
    let sync_state = harness
        .store
        .get_calendar_sync_state(source_id)
        .await
        .expect("sync state")
        .expect("sync state row");
    assert_eq!(sync_state.cursor_value.as_deref(), Some("cursor-kept"));
    let last_synced_after_success = harness.reload_source(source_id).await.last_synced_at;

    // Run 2: the provider 500s mid-incremental → Failed plan must keep the
    // cursor and leave last_synced_at at its previous watermark.
    {
        let mut queue = mock.delta_queue.lock().await;
        queue.push_back(MockResponse::internal_error());
    }
    acquire_lease(&harness, source_id, worker).await;
    rustshare_server::calendar_sync_worker::run_sync(
        harness.store.clone(),
        harness.secret_key.clone(),
        None,
        Some(client),
        harness.outbox.clone(),
        harness.reload_source(source_id).await,
        sync_config(),
        worker.to_string(),
    )
    .await;
    let sync_state = harness
        .store
        .get_calendar_sync_state(source_id)
        .await
        .expect("sync state")
        .expect("sync state row");
    assert_eq!(
        sync_state.cursor_value.as_deref(),
        Some("cursor-kept"),
        "a failed run must preserve the incremental cursor"
    );
    assert_eq!(sync_state.cursor_kind.as_deref(), Some("ms_delta_token"));
    let source_after_failure = harness.reload_source(source_id).await;
    assert_eq!(source_after_failure.status, "failed");
    assert_eq!(
        source_after_failure.last_synced_at, last_synced_after_success,
        "a failed run must not advance last_synced_at"
    );

    harness.cleanup().await;
}

/// Resync on a source whose sync-state row is missing must repair the row
/// and accept (202 at the handler), not 409.
#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn resync_repairs_missing_sync_state_row() {
    let _guard = SERIAL.lock().await;
    let harness = Harness::new().await;
    let user = harness.create_user("cal_o_resync").await;
    let source = harness.create_outlook_source(user.id).await;
    let service = CalendarService::new(harness.store.clone(), harness.secret_key.clone());

    sqlx::query("DELETE FROM calendar_sync_states WHERE source_id = $1")
        .bind(source.id)
        .execute(&harness.pool)
        .await
        .expect("delete sync state row");

    service
        .resync_source(
            harness.tenant_id,
            user.id,
            source.id,
            chrono::Duration::seconds(300),
        )
        .await
        .expect("resync with missing sync-state row must be accepted");

    let sync_state = harness
        .store
        .get_calendar_sync_state(source.id)
        .await
        .expect("sync state")
        .expect("sync-state row recreated");
    assert!(sync_state.cursor_value.is_none());
    assert!(
        sync_state.next_sync_at <= chrono::Utc::now() + chrono::Duration::seconds(5),
        "resync must force the source due now"
    );

    harness.cleanup().await;
}

// ---------------------------------------------------------------------------
// Sync semantics
// ---------------------------------------------------------------------------

/// Run outlook sync_source as `worker` after claiming the lease, then
/// release it the way the worker does.
async fn run_claimed_sync(
    harness: &Harness,
    client: &OutlookCalendarClient,
    source: &CalendarSource,
    worker: &str,
) -> SyncOutcome {
    sqlx::query(
        "UPDATE calendar_sync_states SET locked_at = NOW(), locked_by = $2
         WHERE source_id = $1",
    )
    .bind(source.id)
    .bind(worker)
    .execute(&harness.pool)
    .await
    .expect("acquire lease");
    let outcome = rustshare_server::services::outlook_calendar::sync_source(
        &harness.store,
        client,
        &harness.secret_key,
        source,
        &sync_config(),
        worker,
    )
    .await;
    match &outcome {
        SyncOutcome::Completed {
            upserted,
            next_sync_token,
            ..
        } => {
            harness
                .store
                .finish_calendar_source_sync(
                    source.id,
                    worker,
                    chrono::Utc::now() + chrono::Duration::seconds(900),
                    Some("ms_delta_token"),
                    next_sync_token.as_deref(),
                    None,
                    true,
                )
                .await
                .expect("finish sync");
            harness
                .store
                .update_calendar_source_status(source.id, "healthy", None)
                .await
                .expect("mark healthy");
            let _ = upserted;
        }
        SyncOutcome::RateLimited { .. } => {
            harness
                .store
                .update_calendar_source_status(source.id, "rate_limited", Some("rate limited"))
                .await
                .expect("mark rate limited");
        }
        SyncOutcome::AuthRequired => {
            harness
                .store
                .update_calendar_source_status(
                    source.id,
                    "auth_required",
                    Some("provider rejected the grant"),
                )
                .await
                .expect("mark auth_required");
        }
        SyncOutcome::Failed(message) => {
            harness
                .store
                .update_calendar_source_status(source.id, "failed", Some(message))
                .await
                .expect("mark failed");
        }
        // Parked and lease-lost runs write nothing (the worker returns early).
        SyncOutcome::Parked | SyncOutcome::LeaseLost => {}
    }
    outcome
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn full_sync_pages_materialize_events_and_establish_cursor() {
    let _guard = SERIAL.lock().await;
    let (base, mock) = spawn_mock_microsoft();
    let harness = Harness::new().await;
    let user = harness.create_user("cal_o_full").await;
    let source = harness.create_outlook_source(user.id).await;
    let client = mock_client(&base);

    {
        let mut queue = mock.delta_queue.lock().await;
        queue.push_back(MockResponse::ok(json!({
            "value": [
                graph_event("evt-1", "First", "2026-10-05T14:00:00Z", "2026-10-05T15:00:00Z"),
                graph_event("evt-2", "Second", "2026-10-06T09:00:00Z", "2026-10-06T09:30:00Z")
            ],
            "@odata.nextLink": "http://graph.example/v1.0/me/calendarView/delta?$skiptoken=page-2"
        })));
        queue.push_back(MockResponse::ok(json!({
            "value": [
                json!({
                    "id": "master-1",
                    "type": "seriesMaster",
                    "changeKey": "change-master-1",
                    "subject": "Weekly standup",
                    "start": {"dateTime": "2026-10-05T14:00:00Z", "timeZone": "UTC"},
                    "end": {"dateTime": "2026-10-05T15:00:00Z", "timeZone": "UTC"},
                    "recurrence": {
                        "pattern": {"type": "weekly", "interval": 1, "daysOfWeek": ["monday"]},
                        "range": {"startDate": "2026-10-05", "numberOfOccurrences": 4}
                    }
                })
            ],
            "@odata.deltaLink": "http://graph.example/v1.0/me/calendarView/delta?$deltatoken=cursor-after-full"
        })));
    }

    let outcome = run_claimed_sync(&harness, &client, &source, WORKER_A).await;
    let SyncOutcome::Completed {
        upserted,
        soft_deleted,
        next_sync_token,
    } = outcome
    else {
        panic!("expected Completed, got {outcome:?}");
    };
    assert_eq!(upserted, 3);
    assert_eq!(soft_deleted, 0);
    assert_eq!(next_sync_token.as_deref(), Some("cursor-after-full"));

    let events = harness.list_source_events(source.id).await;
    assert_eq!(events.len(), 3);
    assert!(events.iter().all(|event| event.read_only));
    assert!(events.iter().all(|event| event.status == "confirmed"));
    let master = events
        .iter()
        .find(|event| event.external_uid.as_deref() == Some("master-1"))
        .expect("recurring master materialized");
    assert_eq!(
        master.rrule.as_deref(),
        Some("FREQ=WEEKLY;BYDAY=MO;COUNT=4")
    );

    // The second page was requested with the skiptoken from the first
    // page's nextLink.
    let requests = mock.delta_requests.lock().await;
    assert!(
        requests[1].contains("$skiptoken=page-2"),
        "second page must be requested with the skiptoken: {}",
        requests[1]
    );
    drop(requests);

    // The stored cursor is what the last page returned.
    let sync_state = harness
        .store
        .get_calendar_sync_state(source.id)
        .await
        .expect("sync state")
        .expect("sync state row");
    assert_eq!(
        sync_state.cursor_value.as_deref(),
        Some("cursor-after-full")
    );
    assert_eq!(sync_state.cursor_kind.as_deref(), Some("ms_delta_token"));

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn delta_applies_updates_tombstones_and_keeps_absent_unchanged() {
    let _guard = SERIAL.lock().await;
    let (base, mock) = spawn_mock_microsoft();
    let harness = Harness::new().await;
    let user = harness.create_user("cal_o_delta").await;
    let source = harness.create_outlook_source(user.id).await;
    let client = mock_client(&base);

    // Run 1: full sync of three events.
    {
        let mut queue = mock.delta_queue.lock().await;
        queue.push_back(MockResponse::ok(json!({
            "value": [
                graph_event("evt-1", "Original title", "2026-10-05T14:00:00Z", "2026-10-05T15:00:00Z"),
                graph_event("evt-2", "To be cancelled", "2026-10-06T09:00:00Z", "2026-10-06T09:30:00Z"),
                graph_event("evt-4", "Unchanged", "2026-10-08T09:00:00Z", "2026-10-08T09:30:00Z")
            ],
            "@odata.deltaLink": "http://graph.example/v1.0/me/calendarView/delta?$deltatoken=cursor-1"
        })));
    }
    let first = run_claimed_sync(&harness, &client, &source, WORKER_A).await;
    assert!(matches!(first, SyncOutcome::Completed { upserted: 3, .. }));

    // Run 2: incremental delta — evt-1 updated, evt-2 cancelled, plus a
    // series exception override row. evt-4 did NOT change and is therefore
    // absent from the delta payload; an unchanged event absent from a delta
    // must stay intact (the absent-entry sweep runs on full runs only).
    {
        let mut queue = mock.delta_queue.lock().await;
        queue.push_back(MockResponse::ok(json!({
            "value": [
                graph_event("evt-1", "Updated title", "2026-10-05T15:00:00Z", "2026-10-05T16:00:00Z"),
                json!({
                    "id": "evt-2",
                    "type": "singleInstance",
                    "changeKey": "change-evt-2-v2",
                    "subject": "To be cancelled",
                    "start": {"dateTime": "2026-10-06T09:00:00Z", "timeZone": "UTC"},
                    "end": {"dateTime": "2026-10-06T09:30:00Z", "timeZone": "UTC"},
                    "isCancelled": true
                }),
                json!({
                    "id": "master-1",
                    "type": "exception",
                    "seriesMasterId": "master-1",
                    "changeKey": "change-exception",
                    "subject": "Moved occurrence",
                    "originalStartTime": {"dateTime": "2026-10-07T10:00:00Z", "timeZone": "UTC"},
                    "start": {"dateTime": "2026-10-07T18:00:00Z", "timeZone": "UTC"},
                    "end": {"dateTime": "2026-10-07T19:00:00Z", "timeZone": "UTC"}
                })
            ],
            "@odata.deltaLink": "http://graph.example/v1.0/me/calendarView/delta?$deltatoken=cursor-2"
        })));
    }
    let reloaded = harness.reload_source(source.id).await;
    let second = run_claimed_sync(&harness, &client, &reloaded, WORKER_A).await;
    let SyncOutcome::Completed {
        upserted,
        soft_deleted,
        ..
    } = second
    else {
        panic!("expected Completed, got {second:?}");
    };
    assert_eq!(upserted, 3);
    assert_eq!(
        soft_deleted, 0,
        "incremental deltas must never run the absent-entry sweep"
    );

    // The delta request used the stored cursor.
    let requests = mock.delta_requests.lock().await;
    assert!(
        requests[1].contains("$deltatoken=cursor-1"),
        "delta run must send the stored cursor: {}",
        requests[1]
    );
    drop(requests);

    let events = harness.list_source_events(source.id).await;
    let updated = events
        .iter()
        .find(|event| event.external_uid.as_deref() == Some("evt-1"))
        .expect("evt-1 present");
    assert_eq!(updated.title, "Updated title");
    let expected_start: chrono::DateTime<chrono::Utc> = "2026-10-05T15:00:00Z".parse().unwrap();
    assert_eq!(updated.starts_at, expected_start);
    let cancelled = events
        .iter()
        .find(|event| event.external_uid.as_deref() == Some("evt-2"))
        .expect("evt-2 tombstone kept");
    assert_eq!(cancelled.status, "cancelled");
    let unchanged = events
        .iter()
        .find(|event| event.external_uid.as_deref() == Some("evt-4"))
        .expect("unchanged evt-4 must stay intact when absent from a delta");
    assert_eq!(unchanged.title, "Unchanged");
    let occurrence = events
        .iter()
        .find(|event| event.external_uid.as_deref() == Some("master-1"))
        .expect("override row present");
    assert_eq!(
        occurrence.recurrence_id.as_deref(),
        Some("2026-10-07T10:00:00Z")
    );
    let expected_override: chrono::DateTime<chrono::Utc> = "2026-10-07T18:00:00Z".parse().unwrap();
    assert_eq!(occurrence.starts_at, expected_override);

    // Tombstones are hidden by default and visible with include_cancelled.
    let window_start: chrono::DateTime<chrono::Utc> = "2026-10-01T00:00:00Z".parse().unwrap();
    let window_end: chrono::DateTime<chrono::Utc> = "2026-10-31T00:00:00Z".parse().unwrap();
    let visible = harness
        .store
        .list_calendar_events_in_range(
            harness.tenant_id,
            user.id,
            window_start,
            window_end,
            &[],
            false,
        )
        .await
        .expect("list without cancelled");
    assert!(
        !visible
            .iter()
            .any(|event| event.external_uid.as_deref() == Some("evt-2")),
        "cancelled tombstone hidden by default"
    );
    let with_cancelled = harness
        .store
        .list_calendar_events_in_range(
            harness.tenant_id,
            user.id,
            window_start,
            window_end,
            &[],
            true,
        )
        .await
        .expect("list with cancelled");
    assert!(
        with_cancelled
            .iter()
            .any(|event| event.external_uid.as_deref() == Some("evt-2")),
        "cancelled tombstone visible with include_cancelled"
    );

    harness.cleanup().await;
}

/// A FULL run's payload is the complete live set for the synced window, so
/// mirrored rows missing from it (and starting inside the window) are
/// soft-deleted; out-of-window rows the payload cannot speak for survive.
#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn full_sync_sweep_soft_deletes_absent_in_window_events() {
    let _guard = SERIAL.lock().await;
    let (base, mock) = spawn_mock_microsoft();
    let harness = Harness::new().await;
    let user = harness.create_user("cal_o_sweep").await;
    let source = harness.create_outlook_source(user.id).await;
    let client = mock_client(&base);

    // Run 1: full sync of evt-1 and evt-2.
    {
        let mut queue = mock.delta_queue.lock().await;
        queue.push_back(MockResponse::ok(json!({
            "value": [
                graph_event("evt-1", "Kept", "2026-10-05T14:00:00Z", "2026-10-05T15:00:00Z"),
                graph_event("evt-2", "Deleted upstream", "2026-10-06T09:00:00Z", "2026-10-06T09:30:00Z")
            ],
            "@odata.deltaLink": "http://graph.example/v1.0/me/calendarView/delta?$deltatoken=cursor-sweep-1"
        })));
    }
    let first = run_claimed_sync(&harness, &client, &source, WORKER_A).await;
    assert!(matches!(first, SyncOutcome::Completed { upserted: 2, .. }));
    // Seed an out-of-window far-future row directly (the sync window is
    // 90d back / 365d forward; 2028 is well beyond it).
    let far_future_event = CalendarEvent {
        id: Uuid::new_v4(),
        tenant_id: harness.tenant_id,
        owner_id: user.id,
        source_id: source.id,
        external_uid: Some("evt-far".to_string()),
        external_etag: None,
        recurrence_id: None,
        title: "Beyond the window".to_string(),
        description: None,
        location: None,
        starts_at: "2028-01-01T09:00:00Z".parse().unwrap(),
        ends_at: "2028-01-01T09:30:00Z".parse().unwrap(),
        all_day: false,
        original_date: None,
        timezone: "UTC".to_string(),
        rrule: None,
        status: "confirmed".to_string(),
        read_only: true,
        raw: None,
        deleted_at: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    harness
        .store
        .upsert_calendar_synced_event(&far_future_event)
        .await
        .expect("seed far-future event");

    // Run 2 (forced full resync): the window payload no longer contains
    // evt-2 → swept; evt-far is outside the window → untouched.
    sqlx::query(
        "UPDATE calendar_sync_states SET cursor_value = NULL, cursor_kind = NULL
         WHERE source_id = $1",
    )
    .bind(source.id)
    .execute(&harness.pool)
    .await
    .expect("clear cursor for forced full resync");
    {
        let mut queue = mock.delta_queue.lock().await;
        queue.push_back(MockResponse::ok(json!({
            "value": [graph_event("evt-1", "Kept", "2026-10-05T14:00:00Z", "2026-10-05T15:00:00Z")],
            "@odata.deltaLink": "http://graph.example/v1.0/me/calendarView/delta?$deltatoken=cursor-sweep-2"
        })));
    }
    let reloaded = harness.reload_source(source.id).await;
    let second = run_claimed_sync(&harness, &client, &reloaded, WORKER_A).await;
    let SyncOutcome::Completed {
        upserted,
        soft_deleted,
        next_sync_token,
    } = second
    else {
        panic!("expected Completed, got {second:?}");
    };
    assert_eq!(upserted, 1);
    assert_eq!(soft_deleted, 1, "absent in-window evt-2 must be swept");
    assert_eq!(next_sync_token.as_deref(), Some("cursor-sweep-2"));

    let events = harness.list_source_events(source.id).await;
    assert!(
        events
            .iter()
            .any(|event| event.external_uid.as_deref() == Some("evt-1")),
        "evt-1 must remain"
    );
    assert!(
        !events
            .iter()
            .any(|event| event.external_uid.as_deref() == Some("evt-2")),
        "absent in-window evt-2 must be soft-deleted"
    );
    assert!(
        events
            .iter()
            .any(|event| event.external_uid.as_deref() == Some("evt-far")),
        "out-of-window evt-far must survive a window sweep"
    );

    harness.cleanup().await;
}

/// A multi-page INCREMENTAL run must page with the skiptoken from
/// `@odata.nextLink` (which Graph returns on delta runs too), terminate on
/// the final page, and persist the new delta cursor.
#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn incremental_multi_page_sync_pages_with_skiptoken_and_terminates() {
    let _guard = SERIAL.lock().await;
    let (base, mock) = spawn_mock_microsoft();
    let harness = Harness::new().await;
    let user = harness.create_user("cal_o_paged").await;
    let source = harness.create_outlook_source(user.id).await;
    let client = mock_client(&base);

    // Run 1: full sync establishes a cursor.
    {
        let mut queue = mock.delta_queue.lock().await;
        queue.push_back(MockResponse::ok(json!({
            "value": [graph_event("evt-p1", "One", "2026-10-05T14:00:00Z", "2026-10-05T15:00:00Z")],
            "@odata.deltaLink": "http://graph.example/v1.0/me/calendarView/delta?$deltatoken=cursor-paged-1"
        })));
    }
    let first = run_claimed_sync(&harness, &client, &source, WORKER_A).await;
    assert!(matches!(first, SyncOutcome::Completed { .. }));

    // Run 2: incremental delta spanning two pages. The intermediate page
    // carries `@odata.nextLink` with a $skiptoken; re-sending the
    // $deltatoken there would refetch page one forever.
    {
        let mut queue = mock.delta_queue.lock().await;
        queue.push_back(MockResponse::ok(json!({
            "value": [graph_event("evt-p1", "One updated", "2026-10-05T15:00:00Z", "2026-10-05T16:00:00Z")],
            "@odata.nextLink": "http://graph.example/v1.0/me/calendarView/delta?$skiptoken=inc-page-2"
        })));
        queue.push_back(MockResponse::ok(json!({
            "value": [],
            "@odata.deltaLink": "http://graph.example/v1.0/me/calendarView/delta?$deltatoken=cursor-paged-2"
        })));
    }
    let reloaded = harness.reload_source(source.id).await;
    let second = run_claimed_sync(&harness, &client, &reloaded, WORKER_A).await;
    let SyncOutcome::Completed {
        upserted,
        next_sync_token,
        ..
    } = second
    else {
        panic!("expected Completed, got {second:?}");
    };
    assert_eq!(upserted, 1);
    assert_eq!(next_sync_token.as_deref(), Some("cursor-paged-2"));

    // Request 1 was the full sync; request 2 opened the delta with the
    // cursor; request 3 paged with the skiptoken INSTEAD of re-sending the
    // deltatoken (the old bug re-fetched page one forever).
    let requests = mock.delta_requests.lock().await;
    assert!(
        requests[1].contains("$deltatoken=cursor-paged-1"),
        "delta run must open with the stored cursor: {}",
        requests[1]
    );
    assert!(
        requests[2].contains("$skiptoken=inc-page-2"),
        "second page must use the skiptoken: {}",
        requests[2]
    );
    assert!(
        !requests[2].contains("$deltatoken="),
        "mid-paging request must not re-send the deltatoken: {}",
        requests[2]
    );
    assert_eq!(
        requests.len(),
        3,
        "run must terminate after the final page: {requests:?}"
    );
    drop(requests);

    let sync_state = harness
        .store
        .get_calendar_sync_state(source.id)
        .await
        .expect("sync state")
        .expect("sync state row");
    assert_eq!(sync_state.cursor_value.as_deref(), Some("cursor-paged-2"));

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn invalid_delta_token_triggers_exactly_one_full_resync() {
    let _guard = SERIAL.lock().await;
    let (base, mock) = spawn_mock_microsoft();
    let harness = Harness::new().await;
    let user = harness.create_user("cal_o_gone").await;
    let source = harness.create_outlook_source(user.id).await;
    let client = mock_client(&base);

    // Establish a cursor via a full sync.
    {
        let mut queue = mock.delta_queue.lock().await;
        queue.push_back(MockResponse::ok(json!({
            "value": [graph_event("evt-a", "Alpha", "2026-10-05T14:00:00Z", "2026-10-05T15:00:00Z")],
            "@odata.deltaLink": "http://graph.example/v1.0/me/calendarView/delta?$deltatoken=cursor-doomed"
        })));
    }
    let first = run_claimed_sync(&harness, &client, &source, WORKER_A).await;
    assert!(matches!(first, SyncOutcome::Completed { .. }));

    // Next run: 400 InvalidDeltaToken on the incremental request, then
    // exactly one full window request succeeds and establishes a fresh
    // cursor.
    {
        let mut queue = mock.delta_queue.lock().await;
        queue.push_back(MockResponse::invalid_delta_token());
        queue.push_back(MockResponse::ok(json!({
            "value": [graph_event("evt-b", "Beta", "2026-10-06T14:00:00Z", "2026-10-06T15:00:00Z")],
            "@odata.deltaLink": "http://graph.example/v1.0/me/calendarView/delta?$deltatoken=cursor-reborn"
        })));
    }
    let reloaded = harness.reload_source(source.id).await;
    let outcome = run_claimed_sync(&harness, &client, &reloaded, WORKER_A).await;
    let SyncOutcome::Completed {
        upserted,
        next_sync_token,
        ..
    } = outcome
    else {
        panic!("expected Completed after one full resync, got {outcome:?}");
    };
    assert_eq!(upserted, 1);
    assert_eq!(next_sync_token.as_deref(), Some("cursor-reborn"));

    let requests = mock.delta_requests.lock().await;
    // Run 1 performed the initial full sync; run 2 must make exactly one
    // incremental attempt (the stale cursor) followed by exactly one full
    // resync — no further retries.
    let delta_index = requests
        .iter()
        .position(|query| query.contains("$deltatoken=cursor-doomed"))
        .expect("one delta attempt with the stale cursor");
    let full_after_invalid = requests[delta_index + 1..]
        .iter()
        .filter(|query| query.contains("startDateTime="))
        .count();
    assert_eq!(
        full_after_invalid, 1,
        "exactly one full resync expected after the invalid token, queries: {requests:?}"
    );
    drop(requests);

    let events = harness.list_source_events(source.id).await;
    assert!(events.iter().any(|event| event.title == "Beta"));

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn revoked_grant_flips_auth_required_and_further_runs_noop() {
    let _guard = SERIAL.lock().await;
    let (base, mock) = spawn_mock_microsoft();
    *mock.revoke_grants.lock().await = true;
    let harness = Harness::new().await;
    let user = harness.create_user("cal_o_revoked").await;
    // Access token already expired → the sync must refresh, and the provider
    // answers invalid_grant.
    let source = harness
        .create_outlook_source_with_access_expiry(user.id, -60)
        .await;
    let client = mock_client(&base);

    let outcome = rustshare_server::services::outlook_calendar::sync_source(
        &harness.store,
        &client,
        &harness.secret_key,
        &source,
        &sync_config(),
        WORKER_A,
    )
    .await;
    assert_eq!(outcome, SyncOutcome::AuthRequired);

    // The worker maps AuthRequired onto the source status.
    harness
        .store
        .update_calendar_source_status(source.id, "auth_required", Some("revoked"))
        .await
        .expect("mark auth_required");
    let token_hits_after_revoke = mock.token_hits().await;

    // Further runs are no-ops: no HTTP traffic at all.
    let reloaded = harness.reload_source(source.id).await;
    let second = rustshare_server::services::outlook_calendar::sync_source(
        &harness.store,
        &client,
        &harness.secret_key,
        &reloaded,
        &sync_config(),
        WORKER_A,
    )
    .await;
    assert_eq!(
        second,
        SyncOutcome::Parked,
        "auth_required sources must report Parked, got {second:?}"
    );
    assert_eq!(
        mock.token_hits().await,
        token_hits_after_revoke,
        "no-op run must not touch the token endpoint"
    );
    assert!(
        mock.delta_requests.lock().await.is_empty(),
        "no-op run must not call the delta API"
    );

    // Worker path: a parked source must publish no imported event and must
    // not advance the last-successful-sync watermark.
    let parked_source = harness.reload_source(source.id).await;
    let watermark_before = parked_source.last_synced_at;
    rustshare_server::calendar_sync_worker::run_sync(
        harness.store.clone(),
        harness.secret_key.clone(),
        None,
        Some(Arc::new(mock_client(&base))),
        harness.outbox.clone(),
        parked_source,
        sync_config(),
        WORKER_A.to_string(),
    )
    .await;
    let imported: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM integration_outbox
         WHERE tenant_id = $1 AND event_type = 'io.elembra.calendar.event.imported.v1'",
    )
    .bind(harness.tenant_id)
    .fetch_one(&harness.pool)
    .await
    .expect("count imported outbox rows");
    assert_eq!(
        imported, 0,
        "a parked source must publish no imported event"
    );
    let after = harness.reload_source(source.id).await;
    assert_eq!(
        after.last_synced_at, watermark_before,
        "a parked run must not advance the last-synced watermark"
    );
    assert_eq!(after.status, "auth_required", "status stays parked");

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn rate_limit_backs_off_with_retry_after() {
    let _guard = SERIAL.lock().await;
    let (base, mock) = spawn_mock_microsoft();
    let harness = Harness::new().await;
    let user = harness.create_user("cal_o_limited").await;
    let source = harness.create_outlook_source(user.id).await;
    let client = mock_client(&base);

    {
        let mut queue = mock.delta_queue.lock().await;
        queue.push_back(MockResponse::rate_limited(42));
    }
    let outcome = rustshare_server::services::outlook_calendar::sync_source(
        &harness.store,
        &client,
        &harness.secret_key,
        &source,
        &sync_config(),
        WORKER_A,
    )
    .await;
    assert!(
        matches!(outcome, SyncOutcome::RateLimited { retry_after } if retry_after == std::time::Duration::from_secs(42)),
        "expected RateLimited with 42s backoff, got {outcome:?}"
    );

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn concurrent_same_source_claims_are_safe_and_only_holder_refreshes() {
    let _guard = SERIAL.lock().await;
    let (base, mock) = spawn_mock_microsoft();
    *mock.rotate_refresh.lock().await = true;
    let harness = Harness::new().await;
    let user = harness.create_user("cal_o_concurrent").await;
    // Access token expired → the sync run refreshes and must persist the
    // rotated refresh token unconditionally.
    let source = harness
        .create_outlook_source_with_access_expiry(user.id, -60)
        .await;
    let client = mock_client(&base);
    {
        let mut queue = mock.delta_queue.lock().await;
        queue.push_back(MockResponse::ok(json!({
            "value": [graph_event("evt-c", "Concurrent", "2026-10-09T14:00:00Z", "2026-10-09T15:00:00Z")],
            "@odata.deltaLink": "http://graph.example/v1.0/me/calendarView/delta?$deltatoken=cursor-concurrent"
        })));
    }

    // Worker A claims the source; worker B gets nothing (SKIP LOCKED).
    // The claim queue is global, so sweep aside any due sources left behind
    // by crashed runs (release their lease and push them into the future).
    let mut claimed = None;
    for _ in 0..32 {
        let candidate = harness
            .store
            .claim_due_calendar_source(WORKER_A, std::time::Duration::from_secs(300))
            .await
            .expect("claim due source");
        match candidate {
            Some(candidate) if candidate.id == source.id => {
                claimed = Some(candidate);
                break;
            }
            Some(foreign) => {
                harness
                    .store
                    .finish_calendar_source_sync(
                        foreign.id,
                        WORKER_A,
                        chrono::Utc::now() + chrono::Duration::days(1),
                        None,
                        None,
                        Some("swept aside by concurrent-claim test"),
                        false,
                    )
                    .await
                    .expect("release foreign claim");
            }
            None => break,
        }
    }
    let claimed = claimed.expect("source is due and claimable");
    let second_claim = harness
        .store
        .claim_due_calendar_source("calendar-sync-test-b", std::time::Duration::from_secs(300))
        .await
        .expect("second claim");
    assert!(
        second_claim.is_none(),
        "a live lease must block concurrent claims"
    );

    // Only the lease holder runs the sync (and therefore refreshes tokens).
    let outcome = rustshare_server::services::outlook_calendar::sync_source(
        &harness.store,
        &client,
        &harness.secret_key,
        &claimed,
        &sync_config(),
        WORKER_A,
    )
    .await;
    assert!(matches!(outcome, SyncOutcome::Completed { .. }));
    harness
        .store
        .finish_calendar_source_sync(
            source.id,
            WORKER_A,
            chrono::Utc::now() + chrono::Duration::seconds(900),
            Some("ms_delta_token"),
            Some("cursor-concurrent"),
            None,
            true,
        )
        .await
        .expect("finish sync");

    // Exactly one refresh ran, with the original stored refresh token.
    let seen = mock.refresh_tokens_seen.lock().await;
    assert_eq!(
        seen.as_slice(),
        [TEST_REFRESH_TOKEN.to_string()],
        "exactly one refresh with the stored token expected"
    );
    drop(seen);

    // The rotated refresh token was written unconditionally (newer wins).
    let stored = harness.reload_source(source.id).await;
    let decrypted = rustshare_crypto::decrypt_secret(
        stored
            .refresh_token_enc
            .as_ref()
            .expect("rotated token stored"),
        &harness.secret_key,
    )
    .expect("decrypt rotated token");
    assert_eq!(decrypted, "rotated-refresh-token-value");

    // The lease is released and the source is claimable again.
    assert!(
        !harness
            .store
            .calendar_source_is_locked(source.id, std::time::Duration::from_secs(300))
            .await
            .expect("lock check"),
        "lease must be released after the run"
    );

    harness.cleanup().await;
}

// ---------------------------------------------------------------------------
// Microsoft Graph review-finding regressions
// ---------------------------------------------------------------------------

/// O2a: Graph `calendarView/delta` requests must carry
/// `Prefer: outlook.timezone="UTC"` so `dateTime` values are UTC wall clocks.
#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn delta_requests_send_prefer_utc_header() {
    let _guard = SERIAL.lock().await;
    let (base, mock) = spawn_mock_microsoft();
    let harness = Harness::new().await;
    let user = harness.create_user("cal_o_prefer").await;
    let source = harness.create_outlook_source(user.id).await;
    let client = mock_client(&base);

    {
        let mut queue = mock.delta_queue.lock().await;
        queue.push_back(MockResponse::ok(json!({
            "value": [graph_event("evt-p", "T", "2026-10-05T14:00:00Z", "2026-10-05T15:00:00Z")],
            "@odata.deltaLink": "http://graph.example/v1.0/me/calendarView/delta?$deltatoken=cursor-prefer"
        })));
    }
    let outcome = run_claimed_sync(&harness, &client, &source, WORKER_A).await;
    assert!(matches!(outcome, SyncOutcome::Completed { .. }));

    let prefer = mock.delta_prefer_headers.lock().await;
    assert_eq!(
        prefer.as_slice(),
        [r#"outlook.timezone="UTC""#.to_string()],
        "every delta request must request UTC wall clocks: {prefer:?}"
    );

    harness.cleanup().await;
}

/// O2b: the real Graph payload shape — offset-less `dateTime` with 7
/// fractional digits and the zone in `timeZone` — must parse to the correct
/// instant, not silently fall back to "now".
#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn non_z_fractional_datetime_parses_to_correct_instant() {
    let _guard = SERIAL.lock().await;
    let (base, mock) = spawn_mock_microsoft();
    let harness = Harness::new().await;
    let user = harness.create_user("cal_o_frac").await;
    let source = harness.create_outlook_source(user.id).await;
    let client = mock_client(&base);

    {
        let mut queue = mock.delta_queue.lock().await;
        queue.push_back(MockResponse::ok(json!({
            "value": [json!({
                "id": "evt-frac",
                "type": "singleInstance",
                "changeKey": "change-evt-frac",
                "subject": "Fractional",
                "start": graph_time("2017-08-29T04:00:00.0000000", "UTC"),
                "end": graph_time("2017-08-29T05:00:00.0000000", "UTC"),
                "isAllDay": false,
            })],
            "@odata.deltaLink": "http://graph.example/v1.0/me/calendarView/delta?$deltatoken=cursor-frac"
        })));
    }
    let outcome = run_claimed_sync(&harness, &client, &source, WORKER_A).await;
    assert!(matches!(
        outcome,
        SyncOutcome::Completed { upserted: 1, .. }
    ));

    let events = harness.list_source_events(source.id).await;
    let event = events
        .iter()
        .find(|event| event.external_uid.as_deref() == Some("evt-frac"))
        .expect("fractional event materialized");
    let expected: chrono::DateTime<chrono::Utc> = "2017-08-29T04:00:00Z".parse().unwrap();
    assert_eq!(
        event.starts_at, expected,
        "offset-less 7-digit fractional dateTime must parse to the correct instant"
    );
    assert_eq!(event.timezone, "UTC");

    harness.cleanup().await;
}

/// O2c: a `seriesMaster` + occurrence + exception round-trips, and on the
/// following FULL sync the occurrence is NOT swept as absent. Before the fix
/// the occurrence's `recurrence_id` (built with a `now` fallback) and the
/// `present_keys` entry (built with `unwrap_or_default`) disagreed, so the
/// sweep soft-deleted the occurrence it had just upserted.
#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn series_occurrence_not_swept_on_following_full_sync() {
    let _guard = SERIAL.lock().await;
    let (base, mock) = spawn_mock_microsoft();
    let harness = Harness::new().await;
    let user = harness.create_user("cal_o_series").await;
    let source = harness.create_outlook_source(user.id).await;
    let client = mock_client(&base);

    fn series_payload(delta: &str) -> Value {
        json!({
            "value": [
                {
                    "id": "master-1",
                    "type": "seriesMaster",
                    "changeKey": "change-master-1",
                    "subject": "Weekly standup",
                    "start": graph_time("2026-10-05T14:00:00.0000000", "UTC"),
                    "end": graph_time("2026-10-05T15:00:00.0000000", "UTC"),
                    "recurrence": {
                        "pattern": {"type": "weekly", "interval": 1, "daysOfWeek": ["monday"]},
                        "range": {"startDate": "2026-10-05", "numberOfOccurrences": 3}
                    }
                },
                {
                    "id": "master-1",
                    "type": "occurrence",
                    "seriesMasterId": "master-1",
                    "changeKey": "change-occurrence",
                    "subject": "Weekly standup",
                    "originalStartTime": graph_time("2026-10-12T14:00:00.0000000", "UTC"),
                    "start": graph_time("2026-10-12T14:00:00.0000000", "UTC"),
                    "end": graph_time("2026-10-12T15:00:00.0000000", "UTC")
                },
                {
                    "id": "master-1",
                    "type": "exception",
                    "seriesMasterId": "master-1",
                    "changeKey": "change-exception",
                    "subject": "Moved occurrence",
                    "originalStartTime": graph_time("2026-10-19T14:00:00.0000000", "UTC"),
                    "start": graph_time("2026-10-19T18:00:00.0000000", "UTC"),
                    "end": graph_time("2026-10-19T19:00:00.0000000", "UTC")
                }
            ],
            "@odata.deltaLink": format!("http://graph.example/v1.0/me/calendarView/delta?$deltatoken={delta}")
        })
    }

    {
        let mut queue = mock.delta_queue.lock().await;
        queue.push_back(MockResponse::ok(series_payload("cursor-series-1")));
    }
    let first = run_claimed_sync(&harness, &client, &source, WORKER_A).await;
    let SyncOutcome::Completed {
        upserted,
        soft_deleted,
        ..
    } = first
    else {
        panic!("expected Completed, got {first:?}");
    };
    assert_eq!(upserted, 3);
    assert_eq!(
        soft_deleted, 0,
        "the occurrence's upsert key must match its present-key on the same run"
    );

    let events = harness.list_source_events(source.id).await;
    assert_eq!(
        events.len(),
        3,
        "master + occurrence + exception all stored"
    );
    let occurrence = events
        .iter()
        .find(|event| event.recurrence_id.as_deref() == Some("2026-10-12T14:00:00Z"))
        .expect("occurrence stored under its parsed recurrence id");
    assert_eq!(occurrence.status, "confirmed");
    let exception = events
        .iter()
        .find(|event| event.recurrence_id.as_deref() == Some("2026-10-19T14:00:00Z"))
        .expect("exception stored under its parsed recurrence id");

    // Run 2: forced FULL resync with the same payload. Nothing changed, so
    // nothing may be swept.
    sqlx::query(
        "UPDATE calendar_sync_states SET cursor_value = NULL, cursor_kind = NULL
         WHERE source_id = $1",
    )
    .bind(source.id)
    .execute(&harness.pool)
    .await
    .expect("clear cursor for forced full resync");
    {
        let mut queue = mock.delta_queue.lock().await;
        queue.push_back(MockResponse::ok(series_payload("cursor-series-2")));
    }
    let reloaded = harness.reload_source(source.id).await;
    let second = run_claimed_sync(&harness, &client, &reloaded, WORKER_A).await;
    let SyncOutcome::Completed {
        upserted,
        soft_deleted,
        ..
    } = second
    else {
        panic!("expected Completed, got {second:?}");
    };
    assert_eq!(upserted, 3);
    assert_eq!(
        soft_deleted, 0,
        "an unchanged occurrence must not be swept as absent"
    );

    let events = harness.list_source_events(source.id).await;
    assert_eq!(events.len(), 3, "no occurrence was swept on the full run");
    assert!(events.iter().any(|event| event.id == occurrence.id));
    assert!(events.iter().any(|event| event.id == exception.id));

    harness.cleanup().await;
}

/// O1: a Graph `@removed` deletion tombstone must soft-delete the previously
/// mirrored event and must not upsert a bogus empty live row over it.
#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn removed_tombstone_soft_deletes_mirror_without_bogus_row() {
    let _guard = SERIAL.lock().await;
    let (base, mock) = spawn_mock_microsoft();
    let harness = Harness::new().await;
    let user = harness.create_user("cal_o_removed").await;
    let source = harness.create_outlook_source(user.id).await;
    let client = mock_client(&base);

    // Run 1: full sync mirrors the event.
    {
        let mut queue = mock.delta_queue.lock().await;
        queue.push_back(MockResponse::ok(json!({
            "value": [graph_event("evt-del", "Doomed", "2026-10-05T14:00:00Z", "2026-10-05T15:00:00Z")],
            "@odata.deltaLink": "http://graph.example/v1.0/me/calendarView/delta?$deltatoken=cursor-del-1"
        })));
    }
    let first = run_claimed_sync(&harness, &client, &source, WORKER_A).await;
    assert!(matches!(first, SyncOutcome::Completed { upserted: 1, .. }));
    let mirrored = harness.list_source_events(source.id).await;
    assert_eq!(mirrored.len(), 1);

    // Run 2: incremental delta carries the realistic minimal `@removed`
    // tombstone (no subject/start/end).
    {
        let mut queue = mock.delta_queue.lock().await;
        queue.push_back(MockResponse::ok(json!({
            "value": [{"id": "evt-del", "@removed": {"reason": "deleted"}}],
            "@odata.deltaLink": "http://graph.example/v1.0/me/calendarView/delta?$deltatoken=cursor-del-2"
        })));
    }
    let reloaded = harness.reload_source(source.id).await;
    let second = run_claimed_sync(&harness, &client, &reloaded, WORKER_A).await;
    let SyncOutcome::Completed {
        upserted,
        soft_deleted,
        ..
    } = second
    else {
        panic!("expected Completed, got {second:?}");
    };
    assert_eq!(upserted, 0, "a tombstone is not a live upsert");
    assert_eq!(soft_deleted, 1, "the mirror must be soft-deleted");

    let events = harness.list_source_events(source.id).await;
    assert!(
        events.is_empty(),
        "no live row may remain after the removal tombstone: {events:?}"
    );
    // The soft-deleted row is retained (not physically removed) and carries
    // its original title — no bogus empty confirmed row overwrote it.
    let (title, deleted_at): (String, Option<chrono::DateTime<chrono::Utc>>) = sqlx::query_as(
        "SELECT title, deleted_at FROM calendar_events WHERE source_id = $1 AND external_uid = 'evt-del'",
    )
    .bind(source.id)
    .fetch_one(&harness.pool)
    .await
    .expect("soft-deleted row retained");
    assert_eq!(title, "Doomed");
    assert!(deleted_at.is_some(), "row is soft-deleted, not live");

    harness.cleanup().await;
}

/// O4: Graph HTTP 403 `ErrorAccessDenied` (consent withdrawn) must park the
/// source as `auth_required`, not retry forever as a transient API failure.
#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn graph_403_access_denied_parks_auth_required() {
    let _guard = SERIAL.lock().await;
    let (base, mock) = spawn_mock_microsoft();
    let harness = Harness::new().await;
    let user = harness.create_user("cal_o_403").await;
    let source = harness.create_outlook_source(user.id).await;
    let client = mock_client(&base);

    {
        let mut queue = mock.delta_queue.lock().await;
        queue.push_back(MockResponse::access_denied());
    }
    let outcome = run_claimed_sync(&harness, &client, &source, WORKER_A).await;
    assert_eq!(
        outcome,
        SyncOutcome::AuthRequired,
        "403 ErrorAccessDenied must map to AuthRequired"
    );
    let parked = harness.reload_source(source.id).await;
    assert_eq!(parked.status, "auth_required");

    harness.cleanup().await;
}

/// O5: disconnecting an Outlook source performs no provider-side
/// `revokeSignInSessions` call (the least-privileged permission is not in
/// scope, so it always 403s); the local token wipe is the effective
/// revocation. The revoke endpoint must never be hit.
#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn disconnect_revocation_makes_no_provider_call() {
    let _guard = SERIAL.lock().await;
    let (base, mock) = spawn_mock_microsoft();
    let harness = Harness::new().await;
    let user = harness.create_user("cal_o_disconnect").await;
    let source = harness.create_outlook_source(user.id).await;
    let mut service = CalendarService::new(harness.store.clone(), harness.secret_key.clone());
    service.configure_outlook(Some(mock_client(&base)));

    service
        .disconnect_source(harness.tenant_id, user.id, source.id)
        .await
        .expect("disconnect with no live lease must succeed");

    assert_eq!(
        *mock.revoke_hits.lock().await,
        0,
        "no revokeSignInSessions request may be made"
    );
    let reloaded = harness.reload_source(source.id).await;
    assert_eq!(reloaded.status, "auth_required");
    assert!(reloaded.refresh_token_enc.is_none(), "tokens must be wiped");
    assert!(reloaded.access_token_enc.is_none(), "tokens must be wiped");

    harness.cleanup().await;
}
