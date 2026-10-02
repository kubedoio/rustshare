//! HTTP-level suite for the Calendar OAuth connect flow (issue #315, Task 6).
//!
//! Covers the operator-facing surface added by Stage 1 auth: the 503 when a
//! provider has no credentials, the authorize URL's derived redirect URI, the
//! full callback branch matrix (state/denied/kind-mismatch/exchange/success)
//! with its `?error=oauth_*&reason=*` redirects, the guarantee that no token
//! material ever leaves in the redirect, and the read-only provider-status
//! endpoint.
//!
//! DB-backed and `#[ignore]`d; run against the dev database (migrations
//! applied) with `--test-threads=1`:
//!
//!   set -a; . ./backend/.env; set +a; SQLX_OFFLINE=true \
//!     cargo test -p rustshare-server --test calendar_connect_test -- \
//!       --ignored --test-threads=1
//!
//! Every test takes the shared `SERIAL` guard and cleans up exactly the rows
//! it created under fresh tenants.

use std::collections::HashMap;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use serde_json::Value;
use tower::ServiceExt;

use rustshare_server::services::google_calendar::GoogleCalendarClient;
use rustshare_server::services::outlook_calendar::OutlookCalendarClient;
use rustshare_server::state::AppState;

mod support;

use support::calendar_harness::*;
use support::mock_provider::MockProvider;

const TEST_PUBLIC_URL: &str = "https://cal.example.test";
const GOOGLE_CALLBACK: &str = "/api/v1/calendar/oauth/google/callback";
const OUTLOOK_CALLBACK: &str = "/api/v1/calendar/oauth/outlook/callback";

/// A plain configured Google client (real provider endpoints). Used where no
/// HTTP exchange happens or where only the authorize URL is inspected.
fn configured_google_client(public_url: &str) -> GoogleCalendarClient {
    GoogleCalendarClient::new(
        "test-google-client-id".to_string(),
        "test-google-client-secret".to_string(),
        public_url,
    )
}

/// A Google client whose token/identity endpoints point at the mock provider,
/// so the callback can complete an exchange without touching google.com.
fn mock_google_client(mock: &MockProvider, public_url: &str) -> GoogleCalendarClient {
    let mut client = configured_google_client(public_url);
    client.auth_base = format!("{}/authorize", mock.base_url());
    client.token_url = format!("{}/token", mock.base_url());
    client.userinfo_url = format!("{}/userinfo", mock.base_url());
    client.api_base = format!("{}/calendar/v3", mock.base_url());
    client.revoke_url = format!("{}/revoke", mock.base_url());
    client
}

fn configured_outlook_client(public_url: &str) -> OutlookCalendarClient {
    OutlookCalendarClient::new(
        "test-outlook-client-id".to_string(),
        "test-outlook-client-secret".to_string(),
        public_url,
    )
}

/// Fresh tenant with Calendar enabled plus an auth token and app router.
async fn connect_harness(
    providers: TestProviders,
) -> (AppState, axum::Router<()>, uuid::Uuid, uuid::Uuid, String) {
    let state = setup_test_env_with_providers(providers).await;
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let user = create_test_user(&state, "calendar_connect", tenant_id).await;
    configure_calendar(&state, tenant_id, user.id, true).await;
    let token = create_auth_token(&state, user.id, tenant_id);
    let app = build_app(state.clone());
    (state, app, tenant_id, user.id, token)
}

async fn get_authed(app: &axum::Router<()>, uri: &str, token: &str) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(uri)
                .method("GET")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    response_json(response).await
}

async fn get_raw(app: &axum::Router<()>, uri: &str) -> (StatusCode, Option<String>, Vec<u8>) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(uri)
                .method("GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let location = response
        .headers()
        .get(header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap()
        .to_vec();
    (status, location, body)
}

fn query_params(url: &str) -> HashMap<String, String> {
    url.split_once('?')
        .map(|(_, query)| {
            url::form_urlencoded::parse(query.as_bytes())
                .into_owned()
                .collect()
        })
        .unwrap_or_default()
}

fn redirect_params(location: &str) -> HashMap<String, String> {
    query_params(location)
}

fn assert_redirects_with(location: &str, error: &str, reason: &str) {
    assert!(
        location.starts_with("/settings/apps/calendar?"),
        "unexpected redirect target: {location}"
    );
    let params = redirect_params(location);
    assert_eq!(
        params.get("error").map(String::as_str),
        Some(error),
        "{location}"
    );
    assert_eq!(
        params.get("reason").map(String::as_str),
        Some(reason),
        "{location}"
    );
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn connect_unconfigured_returns_503_via_http() {
    let _guard = SERIAL.lock().await;
    let (state, app, tenant_id, _user_id, token) = connect_harness(TestProviders::default()).await;

    let (status, body) = get_authed(&app, "/api/v1/calendar/sources/google/connect", &token).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "body: {body}");
    let error = body["error"].as_str().unwrap_or_default();
    assert!(
        error.to_lowercase().contains("not configured"),
        "error must name the missing configuration: {error}"
    );

    cleanup_tenant(&state.db_pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn connect_configured_returns_authorize_url_with_derived_redirect_uri() {
    let _guard = SERIAL.lock().await;
    let providers = TestProviders {
        public_url: TEST_PUBLIC_URL.to_string(),
        google: Some(configured_google_client(TEST_PUBLIC_URL)),
        outlook: None,
    };
    let (state, app, tenant_id, _user_id, token) = connect_harness(providers).await;

    let (status, body) = get_authed(&app, "/api/v1/calendar/sources/google/connect", &token).await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    let authorize_url = body["authorize_url"].as_str().expect("authorize_url");
    assert!(
        authorize_url.contains("accounts.google.com"),
        "google authorize URL must target the real provider: {authorize_url}"
    );
    let params = query_params(authorize_url);
    assert_eq!(
        params.get("redirect_uri").map(String::as_str),
        Some("https://cal.example.test/api/v1/calendar/oauth/google/callback"),
        "redirect_uri must derive from the harness public_url"
    );
    assert!(
        params
            .get("scope")
            .is_some_and(|scope| scope.contains("calendar.readonly")),
        "scope: {:?}",
        params.get("scope")
    );
    assert_eq!(
        params.get("access_type").map(String::as_str),
        Some("offline")
    );
    assert_eq!(params.get("prompt").map(String::as_str), Some("consent"));

    cleanup_tenant(&state.db_pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn callback_branch_matrix_redirects_with_reasons() {
    let _guard = SERIAL.lock().await;
    let mock = MockProvider::google();
    let providers = TestProviders {
        public_url: TEST_PUBLIC_URL.to_string(),
        google: Some(mock_google_client(&mock, TEST_PUBLIC_URL)),
        outlook: None,
    };
    let (state, app, tenant_id, user_id, _token) = connect_harness(providers).await;

    // a) no state/code: the connect session is unusable.
    let (status, location, body) = get_raw(&app, GOOGLE_CALLBACK).await;
    assert_eq!(status, StatusCode::FOUND);
    assert_redirects_with(location.as_deref().unwrap(), "oauth_state", "state");
    assert!(body.is_empty(), "redirect body must be empty");

    // b) provider-side denial (user clicked "deny").
    let (_, location, body) = get_raw(
        &app,
        &format!("{GOOGLE_CALLBACK}?error=access_denied&state=any-state"),
    )
    .await;
    assert_redirects_with(location.as_deref().unwrap(), "oauth_denied", "denied");
    assert!(body.is_empty(), "redirect body must be empty");

    // c) kind mismatch: a google state presented to the outlook callback.
    insert_oauth_state(
        &state,
        "google-state-mismatch",
        tenant_id,
        user_id,
        "google",
    )
    .await;
    let (_, location, body) = get_raw(
        &app,
        &format!("{OUTLOOK_CALLBACK}?state=google-state-mismatch&code=any-code"),
    )
    .await;
    assert_redirects_with(location.as_deref().unwrap(), "oauth_state", "state");
    assert!(body.is_empty(), "redirect body must be empty");

    // d) exchange failure: the token endpoint rejects the grant.
    mock.set_revoke_grants(true).await;
    insert_oauth_state(
        &state,
        "google-state-exchange",
        tenant_id,
        user_id,
        "google",
    )
    .await;
    let (_, location, body) = get_raw(
        &app,
        &format!("{GOOGLE_CALLBACK}?state=google-state-exchange&code=bad-code"),
    )
    .await;
    assert_redirects_with(location.as_deref().unwrap(), "oauth_exchange", "exchange");
    assert!(body.is_empty(), "redirect body must be empty");

    // e) success: tokens returned, identity resolved, source created.
    mock.set_revoke_grants(false).await;
    mock.set_identity_email("connected@example.test").await;
    insert_oauth_state(&state, "google-state-success", tenant_id, user_id, "google").await;
    let (_, location, body) = get_raw(
        &app,
        &format!("{GOOGLE_CALLBACK}?state=google-state-success&code=good-code"),
    )
    .await;
    assert_eq!(
        redirect_params(location.as_deref().unwrap())
            .get("connected")
            .map(String::as_str),
        Some("google")
    );
    assert!(body.is_empty());

    cleanup_tenant(&state.db_pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn callback_redirect_uri_mismatch_redirects_with_reason() {
    let _guard = SERIAL.lock().await;
    let mock = MockProvider::google();
    let providers = TestProviders {
        public_url: TEST_PUBLIC_URL.to_string(),
        google: Some(mock_google_client(&mock, TEST_PUBLIC_URL)),
        outlook: None,
    };
    let (state, app, tenant_id, user_id, _token) = connect_harness(providers).await;

    // The token endpoint rejects the exchange as a redirect-URI mismatch.
    mock.set_redirect_uri_mismatch(true).await;
    insert_oauth_state(
        &state,
        "google-state-redirect-uri",
        tenant_id,
        user_id,
        "google",
    )
    .await;
    let code = "code-that-must-not-leak";
    let (status, location, body) = get_raw(
        &app,
        &format!("{GOOGLE_CALLBACK}?state=google-state-redirect-uri&code={code}"),
    )
    .await;
    assert_eq!(status, StatusCode::FOUND);
    let location = location.expect("Location header");
    assert_redirects_with(&location, "oauth_exchange", "redirect_uri");
    assert!(body.is_empty(), "redirect body must be empty");
    for forbidden in [code, "test-refresh-token-value", "test-access-token-value"] {
        assert!(
            !location.contains(forbidden),
            "Location leaked {forbidden:?}: {location}"
        );
    }

    // `invalid_grant` keeps precedence over the redirect-URI reason: a revoked
    // grant must stay `exchange`, not be misclassified as a configuration bug.
    mock.set_revoke_grants(true).await;
    insert_oauth_state(
        &state,
        "google-state-precedence",
        tenant_id,
        user_id,
        "google",
    )
    .await;
    let (_, location, _) = get_raw(
        &app,
        &format!("{GOOGLE_CALLBACK}?state=google-state-precedence&code=bad-code"),
    )
    .await;
    assert_redirects_with(location.as_deref().unwrap(), "oauth_exchange", "exchange");

    cleanup_tenant(&state.db_pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn callback_response_never_contains_token_material() {
    let _guard = SERIAL.lock().await;
    let mock = MockProvider::google();
    mock.set_identity_email("secret-holder@example.test").await;
    let providers = TestProviders {
        public_url: TEST_PUBLIC_URL.to_string(),
        google: Some(mock_google_client(&mock, TEST_PUBLIC_URL)),
        outlook: None,
    };
    let (state, app, tenant_id, user_id, _token) = connect_harness(providers).await;

    let code = "authorization-code-that-must-not-leak";
    insert_oauth_state(&state, "google-state-secret", tenant_id, user_id, "google").await;
    let (status, location, body) = get_raw(
        &app,
        &format!("{GOOGLE_CALLBACK}?state=google-state-secret&code={code}"),
    )
    .await;
    assert_eq!(status, StatusCode::FOUND);
    assert!(body.is_empty(), "success redirect body must be empty");

    let location = location.expect("Location header");
    for forbidden in [
        code,
        "test-refresh-token-value",
        "test-access-token-value",
        "secret-holder@example.test",
    ] {
        assert!(
            !location.contains(forbidden),
            "Location leaked {forbidden:?}: {location}"
        );
    }

    cleanup_tenant(&state.db_pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn provider_status_reports_configuration_and_redirect_uris() {
    let _guard = SERIAL.lock().await;
    let providers = TestProviders {
        public_url: TEST_PUBLIC_URL.to_string(),
        google: Some(configured_google_client(TEST_PUBLIC_URL)),
        outlook: None,
    };
    let (state, app, tenant_id, _user_id, token) = connect_harness(providers).await;

    let (status, body) = get_authed(&app, "/api/v1/calendar/providers", &token).await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["public_url"].as_str(), Some(TEST_PUBLIC_URL));

    let providers_json = body["providers"].as_array().expect("providers array");
    assert_eq!(providers_json.len(), 2, "one entry per external provider");
    let by_kind: HashMap<&str, &Value> = providers_json
        .iter()
        .map(|entry| (entry["kind"].as_str().unwrap(), entry))
        .collect();
    let google = by_kind["google"];
    assert_eq!(google["configured"], Value::Bool(true));
    assert_eq!(
        google["redirect_uri"].as_str(),
        Some("https://cal.example.test/api/v1/calendar/oauth/google/callback")
    );
    let outlook = by_kind["outlook"];
    assert_eq!(outlook["configured"], Value::Bool(false));
    assert_eq!(
        outlook["redirect_uri"].as_str(),
        Some("https://cal.example.test/api/v1/calendar/oauth/outlook/callback")
    );

    // No client id/secret may appear anywhere in the response.
    let rendered = body.to_string();
    assert!(!rendered.contains("test-google-client-id"), "{rendered}");
    assert!(
        !rendered.contains("test-google-client-secret"),
        "{rendered}"
    );
    assert!(
        !rendered.contains("test-outlook-client-secret"),
        "{rendered}"
    );

    // The endpoint is authenticated.
    let unauth = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/calendar/providers")
                .method("GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauth.status(), StatusCode::UNAUTHORIZED);

    // The endpoint is also gated on the tenant's Calendar enablement.
    let disabled_tenant = create_test_tenant(&state.db_pool).await;
    let disabled_user = create_test_user(&state, "calendar_disabled", disabled_tenant).await;
    configure_calendar(&state, disabled_tenant, disabled_user.id, false).await;
    let disabled_token = create_auth_token(&state, disabled_user.id, disabled_tenant);
    let (disabled_status, disabled_body) =
        get_authed(&app, "/api/v1/calendar/providers", &disabled_token).await;
    assert_eq!(
        disabled_status,
        StatusCode::FORBIDDEN,
        "provider status must be gated on Calendar enablement: {disabled_body}"
    );
    cleanup_tenant(&state.db_pool, disabled_tenant).await;

    cleanup_tenant(&state.db_pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn outlook_authorize_url_requests_consent() {
    let _guard = SERIAL.lock().await;
    let providers = TestProviders {
        public_url: TEST_PUBLIC_URL.to_string(),
        google: None,
        outlook: Some(configured_outlook_client(TEST_PUBLIC_URL)),
    };
    let (state, app, tenant_id, _user_id, token) = connect_harness(providers).await;

    let (status, body) = get_authed(&app, "/api/v1/calendar/sources/outlook/connect", &token).await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    let authorize_url = body["authorize_url"].as_str().expect("authorize_url");
    let params = query_params(authorize_url);
    assert_eq!(
        params.get("prompt").map(String::as_str),
        Some("consent"),
        "Outlook must request consent so a refresh token is issued: {authorize_url}"
    );
    let scope = params.get("scope").cloned().unwrap_or_default();
    assert!(scope.contains("Calendars.Read"), "scope: {scope}");
    assert!(scope.contains("offline_access"), "scope: {scope}");
    assert_eq!(
        params.get("redirect_uri").map(String::as_str),
        Some("https://cal.example.test/api/v1/calendar/oauth/outlook/callback")
    );

    cleanup_tenant(&state.db_pool, tenant_id).await;
}
