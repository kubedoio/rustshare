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
            post(|| async { Json(json!({})) }),
        )
        .route(
            "/me/calendarView/delta",
            get(
                |AxumState(state): AxumState<Arc<MockState>>,
                 req: axum::extract::Query<HashMap<String, String>>| async move {
                    let query = req.0;
                    state
                        .delta_requests
                        .lock()
                        .await
                        .push(serde_urlencoded_params(&query));
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
    client.revoke_url = format!("{base}/me/revokeSignInSessions");
    client
}

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

struct Harness {
    pool: PgPool,
    store: Arc<MetadataStore>,
    secret_key: Arc<SecretEncryptionKey>,
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

/// Standard delta entry used across tests.
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
/// so the next run stays incremental (a forced full resync would not
/// propagate provider deletions and could resurrect deleted events).
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
async fn delta_applies_updates_tombstones_cancelled_and_soft_deletes_absent() {
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
                graph_event("evt-4", "To vanish", "2026-10-08T09:00:00Z", "2026-10-08T09:30:00Z")
            ],
            "@odata.deltaLink": "http://graph.example/v1.0/me/calendarView/delta?$deltatoken=cursor-1"
        })));
    }
    let first = run_claimed_sync(&harness, &client, &source, WORKER_A).await;
    assert!(matches!(first, SyncOutcome::Completed { upserted: 3, .. }));

    // Run 2: incremental delta — evt-1 updated, evt-2 cancelled, evt-4 absent
    // (soft-delete), plus a series exception override row.
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
    assert_eq!(soft_deleted, 1, "absent evt-4 must be soft-deleted");

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
    assert!(
        !events
            .iter()
            .any(|event| event.external_uid.as_deref() == Some("evt-4")),
        "absent evt-4 must be gone from active rows"
    );
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
    assert!(
        matches!(
            second,
            SyncOutcome::Completed {
                upserted: 0,
                soft_deleted: 0,
                ..
            }
        ),
        "auth_required sources must no-op, got {second:?}"
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
