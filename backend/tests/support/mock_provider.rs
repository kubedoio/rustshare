//! Pluggable mock OAuth + calendar provider server shared by the calendar
//! Google and Outlook sync suites.
//!
//! One `MockProvider` serves the token, identity, data-list, and revoke
//! endpoints a calendar provider client talks to. `MockProvider::google()`
//! and `MockProvider::microsoft()` pick the provider-specific paths and default
//! response shapes; queued responses override the default for the data
//! endpoint. Requests, token/revoke hits, identity email, and failure-injection
//! flags are all observable and mutable for assertions.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use axum::extract::State as AxumState;
use axum::routing::{get, post};
use axum::{Form, Json, Router};
use serde_json::{json, Value};
use tokio::sync::Mutex;

/// Values the mock token endpoint issues (also the values the harness stores
/// on a created source, so refresh rotation compares against them).
pub const TEST_REFRESH_TOKEN: &str = "test-refresh-token-value";
pub const TEST_ACCESS_TOKEN: &str = "test-access-token-value";

/// Read-only scopes echoed by the token endpoint, per provider.
const GOOGLE_SCOPE_READONLY: &str = "https://www.googleapis.com/auth/calendar.readonly";
const MICROSOFT_SCOPE_READONLY: &str = "offline_access Calendars.Read";

#[derive(Debug, Clone)]
pub struct MockResponse {
    pub status: axum::http::StatusCode,
    pub body: Value,
    pub retry_after: Option<u64>,
}

impl MockResponse {
    pub fn ok(body: Value) -> Self {
        Self {
            status: axum::http::StatusCode::OK,
            body,
            retry_after: None,
        }
    }

    /// Google `410 GONE`: the delta cursor expired and a full resync is due.
    pub fn gone() -> Self {
        Self {
            status: axum::http::StatusCode::GONE,
            body: json!({"error": {"code": 410, "message": "sync token expired"}}),
            retry_after: None,
        }
    }

    /// Microsoft Graph `400 InvalidDeltaToken`: the delta token is stale.
    pub fn invalid_delta_token() -> Self {
        Self {
            status: axum::http::StatusCode::BAD_REQUEST,
            body: json!({"error": {"code": "InvalidDeltaToken", "message": "delta token is invalid"}}),
            retry_after: None,
        }
    }

    /// Graph `ErrorAccessDenied` (HTTP 403), e.g. consent withdrawn.
    pub fn access_denied() -> Self {
        Self {
            status: axum::http::StatusCode::FORBIDDEN,
            body: json!({"error": {"code": "ErrorAccessDenied", "message": "Access is denied."}}),
            retry_after: None,
        }
    }

    pub fn internal_error(kind: MockProviderKind) -> Self {
        let body = match kind {
            MockProviderKind::Google => {
                json!({"error": {"code": 500, "message": "transient backend error"}})
            }
            MockProviderKind::Microsoft => {
                json!({"error": {"code": "UnknownError", "message": "transient backend error"}})
            }
        };
        Self {
            status: axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            body,
            retry_after: None,
        }
    }

    pub fn rate_limited(kind: MockProviderKind, retry_after: u64) -> Self {
        let body = match kind {
            MockProviderKind::Google => json!({"error": {"code": 429, "message": "rate limited"}}),
            MockProviderKind::Microsoft => {
                json!({"error": {"code": "TooManyRequests", "message": "rate limited"}})
            }
        };
        Self {
            status: axum::http::StatusCode::TOO_MANY_REQUESTS,
            body,
            retry_after: Some(retry_after),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MockProviderKind {
    Google,
    Microsoft,
}

struct MockInner {
    kind: MockProviderKind,
    /// Queued responses for the data endpoint (popped FIFO).
    data_queue: Mutex<VecDeque<MockResponse>>,
    /// Query strings of data requests, in arrival order.
    data_requests: Mutex<Vec<String>>,
    /// `Prefer` header of each data request, in arrival order.
    data_prefer_headers: Mutex<Vec<String>>,
    /// Refresh-token values seen at the token endpoint (rotation tracking).
    refresh_tokens_seen: Mutex<Vec<String>>,
    token_hits: Mutex<u32>,
    /// Authorization/revocation hits (Google revokes grant-scoped tokens).
    revoke_hits: Mutex<u32>,
    /// When true, the token endpoint answers token requests with
    /// `invalid_grant` (revoked grant).
    revoke_grants: Mutex<bool>,
    /// When true, the token endpoint rejects the token request as a
    /// redirect-URI / client-registration mismatch (Google
    /// `redirect_uri_mismatch`, Microsoft `AADSTS50011`). Takes effect only
    /// when `revoke_grants` is off, so `invalid_grant` keeps precedence.
    redirect_uri_mismatch: Mutex<bool>,
    /// When true, refresh responses carry a rotated refresh token.
    rotate_refresh: Mutex<bool>,
    /// When true, the authorization-code exchange omits the refresh token
    /// (Google does this when the user has not re-consented).
    omit_refresh_token: Mutex<bool>,
    identity_email: Mutex<String>,
}

impl MockInner {
    async fn token_hits(&self) -> u32 {
        *self.token_hits.lock().await
    }

    async fn revoke_hits(&self) -> u32 {
        *self.revoke_hits.lock().await
    }

    async fn data_requests(&self) -> Vec<String> {
        self.data_requests.lock().await.clone()
    }

    async fn data_prefer_headers(&self) -> Vec<String> {
        self.data_prefer_headers.lock().await.clone()
    }

    async fn refresh_tokens_seen(&self) -> Vec<String> {
        self.refresh_tokens_seen.lock().await.clone()
    }

    async fn identity_email(&self) -> String {
        self.identity_email.lock().await.clone()
    }

    async fn push_data_response(&self, response: MockResponse) {
        self.data_queue.lock().await.push_back(response);
    }
}

/// Handle for pushing responses onto the data endpoint's FIFO queue.
pub struct MockDataQueue<'a> {
    provider: &'a MockProvider,
}

impl MockDataQueue<'_> {
    pub async fn push_back(&self, response: MockResponse) {
        self.provider.inner.push_data_response(response).await;
    }
}

/// A running mock provider server.
#[derive(Clone)]
pub struct MockProvider {
    base_url: String,
    inner: Arc<MockInner>,
}

impl MockProvider {
    pub fn google() -> Self {
        Self::spawn(MockProviderKind::Google)
    }

    pub fn microsoft() -> Self {
        Self::spawn(MockProviderKind::Microsoft)
    }

    fn spawn(kind: MockProviderKind) -> Self {
        let inner = Arc::new(MockInner {
            kind,
            data_queue: Mutex::new(VecDeque::new()),
            data_requests: Mutex::new(Vec::new()),
            data_prefer_headers: Mutex::new(Vec::new()),
            refresh_tokens_seen: Mutex::new(Vec::new()),
            token_hits: Mutex::new(0),
            revoke_hits: Mutex::new(0),
            revoke_grants: Mutex::new(false),
            redirect_uri_mismatch: Mutex::new(false),
            rotate_refresh: Mutex::new(false),
            omit_refresh_token: Mutex::new(false),
            identity_email: Mutex::new(String::new()),
        });

        let data_path = match kind {
            MockProviderKind::Google => "/calendar/v3/calendars/primary/events",
            MockProviderKind::Microsoft => "/me/calendarView/delta",
        };
        let identity_path = match kind {
            MockProviderKind::Google => "/userinfo",
            MockProviderKind::Microsoft => "/me",
        };

        let app = Router::new()
            .route("/token", post(token_endpoint))
            .route(identity_path, get(identity_endpoint))
            .route(data_path, get(data_endpoint))
            .route("/revoke", post(revoke_endpoint))
            .with_state(inner.clone());

        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind mock server");
        listener.set_nonblocking(true).unwrap();
        let listener = tokio::net::TcpListener::from_std(listener).unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        Self {
            base_url: format!("http://{address}"),
            inner,
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn data_queue(&self) -> MockDataQueue<'_> {
        MockDataQueue { provider: self }
    }

    /// A 500 whose body matches this provider's error shape.
    pub fn internal_error(&self) -> MockResponse {
        MockResponse::internal_error(self.inner.kind)
    }

    /// A 429 with `Retry-After`, body shaped per provider.
    pub fn rate_limited(&self, retry_after: u64) -> MockResponse {
        MockResponse::rate_limited(self.inner.kind, retry_after)
    }

    pub async fn data_requests(&self) -> Vec<String> {
        self.inner.data_requests().await
    }

    pub async fn data_prefer_headers(&self) -> Vec<String> {
        self.inner.data_prefer_headers().await
    }

    pub async fn refresh_tokens_seen(&self) -> Vec<String> {
        self.inner.refresh_tokens_seen().await
    }

    pub async fn token_hits(&self) -> u32 {
        self.inner.token_hits().await
    }

    pub async fn revoke_hits(&self) -> u32 {
        self.inner.revoke_hits().await
    }

    pub async fn identity_email(&self) -> String {
        self.inner.identity_email().await
    }

    pub async fn set_identity_email(&self, email: impl Into<String>) {
        *self.inner.identity_email.lock().await = email.into();
    }

    pub async fn set_revoke_grants(&self, revoke: bool) {
        *self.inner.revoke_grants.lock().await = revoke;
    }

    /// Make the token endpoint reject the request as a redirect-URI/client
    /// registration mismatch. `invalid_grant` (revoke) keeps precedence.
    pub async fn set_redirect_uri_mismatch(&self, mismatch: bool) {
        *self.inner.redirect_uri_mismatch.lock().await = mismatch;
    }

    pub async fn set_rotate_refresh(&self, rotate: bool) {
        *self.inner.rotate_refresh.lock().await = rotate;
    }

    pub async fn set_omit_refresh_token(&self, omit: bool) {
        *self.inner.omit_refresh_token.lock().await = omit;
    }
}

async fn token_endpoint(
    AxumState(inner): AxumState<Arc<MockInner>>,
    form: Form<HashMap<String, String>>,
) -> (axum::http::StatusCode, Json<Value>) {
    *inner.token_hits.lock().await += 1;
    if let Some(refresh) = form.get("refresh_token") {
        inner.refresh_tokens_seen.lock().await.push(refresh.clone());
    }
    if *inner.revoke_grants.lock().await {
        let body = match inner.kind {
            MockProviderKind::Google => json!({"error": "invalid_grant"}),
            MockProviderKind::Microsoft => {
                json!({"error": "invalid_grant", "error_description": "revoked"})
            }
        };
        return (axum::http::StatusCode::BAD_REQUEST, Json(body));
    }
    if *inner.redirect_uri_mismatch.lock().await {
        let body = match inner.kind {
            MockProviderKind::Google => json!({"error": "redirect_uri_mismatch"}),
            MockProviderKind::Microsoft => json!({
                "error": "invalid_client",
                "error_description": "AADSTS50011: the redirect URI specified in the request does not match"
            }),
        };
        return (axum::http::StatusCode::BAD_REQUEST, Json(body));
    }
    let rotate = *inner.rotate_refresh.lock().await;
    let refresh_token = if rotate {
        "rotated-refresh-token-value"
    } else {
        TEST_REFRESH_TOKEN
    };
    let grant_type = form.get("grant_type").cloned().unwrap_or_default();
    let omit_refresh = *inner.omit_refresh_token.lock().await;
    let scope = match inner.kind {
        MockProviderKind::Google => GOOGLE_SCOPE_READONLY,
        MockProviderKind::Microsoft => MICROSOFT_SCOPE_READONLY,
    };
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
            "scope": scope
        })
    } else {
        json!({
            "access_token": TEST_ACCESS_TOKEN,
            "refresh_token": refresh_token,
            "expires_in": 3600,
            "token_type": "Bearer",
            "scope": scope
        })
    };
    (axum::http::StatusCode::OK, Json(body))
}

async fn identity_endpoint(AxumState(inner): AxumState<Arc<MockInner>>) -> Json<Value> {
    let email = inner.identity_email().await;
    match inner.kind {
        MockProviderKind::Google => Json(json!({"email": email, "sub": "mock-subject"})),
        MockProviderKind::Microsoft => {
            Json(json!({"mail": email, "userPrincipalName": "upn@example.test"}))
        }
    }
}

async fn data_endpoint(
    AxumState(inner): AxumState<Arc<MockInner>>,
    headers: axum::http::HeaderMap,
    query: axum::extract::Query<HashMap<String, String>>,
) -> axum::response::Response {
    let query = query.0;
    inner
        .data_requests
        .lock()
        .await
        .push(serde_urlencoded_params(&query));
    inner.data_prefer_headers.lock().await.push(
        headers
            .get("prefer")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string(),
    );
    let queued = inner.data_queue.lock().await.pop_front();
    let response = queued.unwrap_or_else(|| default_data_response(inner.kind, &query));
    let mut builder = axum::response::Response::builder().status(response.status);
    if let Some(retry_after) = response.retry_after {
        builder = builder.header("retry-after", retry_after.to_string());
    }
    builder
        .header("content-type", "application/json")
        .body(axum::body::Body::from(response.body.to_string()))
        .unwrap()
}

async fn revoke_endpoint(
    AxumState(inner): AxumState<Arc<MockInner>>,
    _form: Form<HashMap<String, String>>,
) -> axum::http::StatusCode {
    *inner.revoke_hits.lock().await += 1;
    axum::http::StatusCode::OK
}

/// Default data response: empty page; a cursor request advances the cursor, a
/// window request establishes one.
fn default_data_response(kind: MockProviderKind, query: &HashMap<String, String>) -> MockResponse {
    match kind {
        MockProviderKind::Google => {
            let mut body = json!({"items": []});
            if let Some(cursor) = query.get("syncToken") {
                body["nextSyncToken"] = json!(format!("{cursor}-advanced"));
            } else {
                body["nextSyncToken"] = json!("initial-sync-token");
            }
            MockResponse::ok(body)
        }
        MockProviderKind::Microsoft => {
            let mut body = json!({"value": []});
            if let Some(cursor) = query.get("$deltatoken") {
                body["@odata.deltaLink"] = json!(format!(
                    "http://graph.example/v1.0/me/calendarView/delta?$deltatoken={cursor}-advanced"
                ));
            } else {
                body["@odata.deltaLink"] =
                    json!("http://graph.example/v1.0/me/calendarView/delta?$deltatoken=initial-delta-token");
            }
            MockResponse::ok(body)
        }
    }
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
