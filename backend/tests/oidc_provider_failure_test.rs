//! Real-route coverage for OIDC discovery outage and recovery.
//!
//! Run this ignored test by itself with `--test-threads=1`. It temporarily
//! updates the singleton OIDC configuration and must not run concurrently with
//! `admin_config_oidc_test` or another test that mutates `oidc_config`.

mod support;

use axum::{
    extract::State as AxumState,
    http::{Request, StatusCode, Uri},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use futures_util::FutureExt;
use rustshare_crypto::encrypt_secret;
use serde_json::Value;
use std::{
    error::Error,
    net::SocketAddr,
    panic::AssertUnwindSafe,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
};
use support::calendar_harness::{setup_test_env, SERIAL};
use tokio::net::TcpListener;
use tower::ServiceExt;
use uuid::Uuid;

type TestResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

const OIDC_CONFIG_ID: Uuid = uuid::uuid!("00000000-0000-0000-0000-000000000001");
const UPSTREAM_ERROR_SENTINEL: &str = "upstream-provider-error-secret-sentinel";
const CALLBACK_ERROR_SENTINEL: &str = "provider-error-description-private-sentinel";
const TOKEN_ERROR_SENTINEL: &str = "provider-token-response-private-sentinel";
const CORRUPTED_SECRET_SENTINEL: &str = "corrupted-client-secret-ciphertext-sentinel";
const PUBLIC_OIDC_FAILURE_MESSAGE: &str = "OIDC authentication is temporarily unavailable";

#[derive(Clone)]
struct MockIssuer {
    available: Arc<AtomicBool>,
    discovery_requests: Arc<AtomicUsize>,
    jwks_requests: Arc<AtomicUsize>,
    issuer_url: String,
    last_discovery_path: Arc<Mutex<String>>,
}

async fn discovery(AxumState(issuer): AxumState<MockIssuer>, uri: Uri) -> axum::response::Response {
    issuer.discovery_requests.fetch_add(1, Ordering::SeqCst);
    *issuer
        .last_discovery_path
        .lock()
        .expect("discovery path lock") = uri.path().to_string();
    if !issuer.available.load(Ordering::SeqCst) {
        return (StatusCode::SERVICE_UNAVAILABLE, UPSTREAM_ERROR_SENTINEL).into_response();
    }

    Json(serde_json::json!({
        "issuer": issuer.issuer_url,
        "authorization_endpoint": format!("{}/authorize", issuer.issuer_url),
        "token_endpoint": format!("{}/token", issuer.issuer_url),
        "jwks_uri": format!("{}/jwks", issuer.issuer_url),
        "response_types_supported": ["code"],
        "subject_types_supported": ["public"],
        "id_token_signing_alg_values_supported": ["RS256"],
    }))
    .into_response()
}

async fn jwks(AxumState(issuer): AxumState<MockIssuer>) -> Json<Value> {
    issuer.jwks_requests.fetch_add(1, Ordering::SeqCst);
    Json(serde_json::json!({ "keys": [] }))
}

async fn token_error() -> axum::response::Response {
    (StatusCode::BAD_GATEWAY, TOKEN_ERROR_SENTINEL).into_response()
}

fn require_test(condition: bool, message: &'static str) -> TestResult<()> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}

fn has_approved_object_store_port(endpoint: &url::Url) -> bool {
    matches!(endpoint.port(), Some(9000 | 19000))
}

fn assert_disposable_local_environment() -> TestResult<()> {
    let database_url = std::env::var("DATABASE_URL")?;
    let parsed_database_url = url::Url::parse(&database_url)?;
    let database_name = parsed_database_url.path().trim_start_matches('/');
    require_test(
        database_name == "rustshare_test" || database_name.starts_with("rustshare_test_"),
        "refusing OIDC test outside a rustshare_test database",
    )?;
    require_test(
        matches!(
            parsed_database_url.host_str(),
            Some("localhost" | "127.0.0.1")
        ),
        "refusing OIDC test against a non-local database",
    )?;
    require_test(
        std::env::var("RUSTSHARE_TEST_DISPOSABLE_DB").as_deref() == Ok("1"),
        "set RUSTSHARE_TEST_DISPOSABLE_DB=1 only for a disposable database",
    )?;
    require_test(
        std::env::var("RUSTSHARE_TEST_DISPOSABLE_OBJECT_STORE").as_deref() == Ok("1"),
        "set RUSTSHARE_TEST_DISPOSABLE_OBJECT_STORE=1 only for a disposable object store",
    )?;

    let endpoint = std::env::var("S3_ENDPOINT").or_else(|_| std::env::var("RUSTFS_ENDPOINT"))?;
    let parsed_endpoint = url::Url::parse(&endpoint)?;
    require_test(
        matches!(parsed_endpoint.scheme(), "http" | "https")
            && matches!(parsed_endpoint.host_str(), Some("localhost" | "127.0.0.1"))
            && parsed_endpoint.username().is_empty()
            && parsed_endpoint.password().is_none()
            && has_approved_object_store_port(&parsed_endpoint),
        "refusing OIDC test against a non-local or credentialed object store",
    )?;
    let bucket = std::env::var("S3_BUCKET").or_else(|_| std::env::var("RUSTFS_BUCKET"))?;
    require_test(
        bucket == "rustshare-test"
            || bucket.starts_with("rustshare-test-")
            || bucket.starts_with("rustshare-test_"),
        "refusing OIDC test outside a rustshare-test bucket",
    )
}

#[cfg(test)]
mod endpoint_guard_tests {
    use super::has_approved_object_store_port;

    #[test]
    fn accepts_harness_approved_object_store_ports() {
        for endpoint in ["http://127.0.0.1:9000", "https://localhost:19000"] {
            let endpoint = url::Url::parse(endpoint).expect("valid test endpoint");
            assert!(has_approved_object_store_port(&endpoint));
        }
    }

    #[test]
    fn rejects_unapproved_and_implicit_object_store_ports() {
        for endpoint in [
            "http://127.0.0.1:9001",
            "https://localhost:443",
            "http://localhost",
        ] {
            let endpoint = url::Url::parse(endpoint).expect("valid test endpoint");
            assert!(!has_approved_object_store_port(&endpoint));
        }
    }
}

async fn restore_oidc_config(
    pool: &sqlx::PgPool,
    state: &rustshare_server::state::AppState,
    snapshot: &Value,
    redirect_to: &str,
) -> TestResult<()> {
    let mut cleanup_error = None;
    if let Err(error) = sqlx::query("DELETE FROM oidc_login_states WHERE redirect_to = $1")
        .bind(redirect_to)
        .execute(pool)
        .await
    {
        cleanup_error = Some(error);
    }

    let restore = sqlx::query(
        "INSERT INTO oidc_config
            (id, enabled, provider_name, client_id, client_secret_enc, issuer_url,
             scopes, auto_provision_users, updated_by, updated_at,
             device_pair_code_ttl_seconds, redirect_url, login_label)
         SELECT id, enabled, provider_name, client_id, client_secret_enc, issuer_url,
                scopes, auto_provision_users, updated_by, updated_at,
                device_pair_code_ttl_seconds, redirect_url, login_label
         FROM jsonb_populate_record(NULL::oidc_config, $1)
         ON CONFLICT (id) DO UPDATE SET
            enabled = EXCLUDED.enabled,
            provider_name = EXCLUDED.provider_name,
            client_id = EXCLUDED.client_id,
            client_secret_enc = EXCLUDED.client_secret_enc,
            issuer_url = EXCLUDED.issuer_url,
            scopes = EXCLUDED.scopes,
            auto_provision_users = EXCLUDED.auto_provision_users,
            updated_by = EXCLUDED.updated_by,
            updated_at = EXCLUDED.updated_at,
            device_pair_code_ttl_seconds = EXCLUDED.device_pair_code_ttl_seconds,
            redirect_url = EXCLUDED.redirect_url,
            login_label = EXCLUDED.login_label",
    )
    .bind(snapshot)
    .execute(pool)
    .await
    .and_then(|result| {
        if result.rows_affected() == 1 {
            Ok(())
        } else {
            Err(sqlx::Error::Protocol(
                "restoring the OIDC singleton affected an unexpected row count".into(),
            ))
        }
    });

    state.oidc_runtime_cache.invalidate().await;

    let restore = match restore {
        Ok(()) => {
            let restored: Result<Value, sqlx::Error> =
                sqlx::query_scalar("SELECT to_jsonb(oidc_config) FROM oidc_config WHERE id = $1")
                    .bind(OIDC_CONFIG_ID)
                    .fetch_one(pool)
                    .await;
            restored.and_then(|restored| {
                if &restored == snapshot {
                    Ok(())
                } else {
                    Err(sqlx::Error::Protocol(
                        "restored OIDC configuration differs from its exact snapshot".into(),
                    ))
                }
            })
        }
        Err(error) => Err(error),
    };

    match (cleanup_error, restore) {
        (Some(cleanup_error), Err(restore_error)) => Err(format!(
            "failed to remove test OIDC login states ({cleanup_error}) and restore original OIDC configuration ({restore_error})"
        )
        .into()),
        (Some(error), Ok(_)) | (None, Err(error)) => Err(error.into()),
        (None, Ok(())) => Ok(()),
    }
}

#[tokio::test]
#[ignore = "requires explicitly configured disposable local PostgreSQL and RustFS services"]
async fn oidc_discovery_failure_is_visible_and_login_recovers() -> TestResult<()> {
    let _serial = SERIAL.lock().await;
    assert_disposable_local_environment()?;

    let state = setup_test_env().await;
    let pool = state.db_pool.clone();
    let oidc_id = OIDC_CONFIG_ID;
    let snapshot: Value =
        sqlx::query_scalar("SELECT to_jsonb(oidc_config) FROM oidc_config WHERE id = $1")
            .bind(oidc_id)
            .fetch_one(&pool)
            .await?;

    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address: SocketAddr = listener.local_addr()?;
    let issuer_url = format!("http://{address}");
    let available = Arc::new(AtomicBool::new(false));
    let discovery_requests = Arc::new(AtomicUsize::new(0));
    let jwks_requests = Arc::new(AtomicUsize::new(0));
    let last_discovery_path = Arc::new(Mutex::new(String::new()));
    let mock_issuer = MockIssuer {
        available: Arc::clone(&available),
        discovery_requests: Arc::clone(&discovery_requests),
        jwks_requests: Arc::clone(&jwks_requests),
        issuer_url: issuer_url.clone(),
        last_discovery_path: Arc::clone(&last_discovery_path),
    };
    let issuer_app = Router::new()
        .route("/.well-known/openid-configuration", get(discovery))
        .route("/token", post(token_error))
        .route("/jwks", get(jwks))
        .fallback(discovery)
        .with_state(mock_issuer);
    let issuer_task = tokio::spawn(async move {
        if let Err(error) = axum::serve(listener, issuer_app).await {
            tracing::error!(%error, "mock OIDC issuer stopped unexpectedly");
        }
    });

    let redirect_to = format!("/oidc-discovery-test/{}", Uuid::new_v4());
    let redirect_query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("redirect_to", &redirect_to)
        .finish();
    let app = rustshare_server::routes::auth_routes().with_state(state.clone());
    let attempt = AssertUnwindSafe(async {
        let client_secret_enc = encrypt_secret("test-only-client-secret", &state.secret_key)?;
        sqlx::query(
            "UPDATE oidc_config
             SET enabled = true, provider_name = 'Loopback Test Issuer',
                 client_id = 'rustshare-oidc-test', client_secret_enc = $2,
                 issuer_url = $3, scopes = ARRAY['openid', 'email', 'profile'],
                 auto_provision_users = false, updated_by = NULL, updated_at = NOW(),
                 redirect_url = 'http://localhost/api/v1/auth/oidc/callback',
                 login_label = 'Test issuer'
             WHERE id = $1",
        )
        .bind(oidc_id)
        .bind(client_secret_enc)
        .bind(&issuer_url)
        .execute(&pool)
        .await?;
        state.oidc_runtime_cache.invalidate().await;

        let request = || {
            Request::builder()
                .uri(format!("/api/v1/auth/oidc/login?{redirect_query}"))
                .body(axum::body::Body::empty())
                .expect("build OIDC login request")
        };

        let failed = app.clone().oneshot(request()).await?;
        require_test(
            failed.status() == StatusCode::BAD_GATEWAY,
            "OIDC discovery outage should return a visible 502 response",
        )?;
        require_test(
            !failed
                .headers()
                .contains_key(axum::http::header::SET_COOKIE),
            "OIDC discovery outage must not issue a session cookie",
        )?;
        require_test(
            !failed.headers().contains_key(axum::http::header::LOCATION),
            "OIDC discovery outage must not return a redirect",
        )?;
        let failure_body = axum::body::to_bytes(failed.into_body(), usize::MAX).await?;
        let failure_body = String::from_utf8_lossy(&failure_body);
        require_test(
            failure_body == PUBLIC_OIDC_FAILURE_MESSAGE,
            "OIDC discovery outage should return the stable generic failure message",
        )?;
        require_test(
            !failure_body.contains(UPSTREAM_ERROR_SENTINEL),
            "OIDC discovery outage response must not expose the upstream response body",
        )?;
        require_test(
            discovery_requests.load(Ordering::SeqCst) == 1,
            "cold-cache outage request did not reach the loopback issuer exactly once",
        )?;
        require_test(
            jwks_requests.load(Ordering::SeqCst) == 0,
            "JWKS should not be fetched when discovery fails",
        )?;
        let failed_login_state_count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM oidc_login_states WHERE redirect_to = $1")
                .bind(&redirect_to)
                .fetch_one(&pool)
                .await?;
        require_test(
            failed_login_state_count == 0,
            "discovery failure must not persist an OIDC login-state row",
        )?;

        available.store(true, Ordering::SeqCst);
        let recovered = app.clone().oneshot(request()).await?;
        if recovered.status() != StatusCode::TEMPORARY_REDIRECT {
            let status = recovered.status();
            let body = axum::body::to_bytes(recovered.into_body(), usize::MAX).await?;
            return Err(format!(
                "OIDC login after issuer recovery returned {status}: {}",
                String::from_utf8_lossy(&body)
            )
            .into());
        }
        let location = recovered
            .headers()
            .get(axum::http::header::LOCATION)
            .ok_or("successful OIDC login is missing Location")?
            .to_str()?;
        let authorization_url = url::Url::parse(location)?;
        require_test(
            authorization_url.origin().ascii_serialization() == issuer_url,
            "successful OIDC login should redirect to the recovered issuer",
        )?;
        let state_token = authorization_url
            .query_pairs()
            .find_map(|(key, value)| (key == "state").then(|| value.into_owned()))
            .ok_or("OIDC authorization redirect is missing state")?;

        let provider_error_query = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("error", "access_denied")
            .append_pair("error_description", CALLBACK_ERROR_SENTINEL)
            .finish();
        let provider_error_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/auth/oidc/callback?{provider_error_query}"))
                    .body(axum::body::Body::empty())?,
            )
            .await?;
        require_test(
            provider_error_response.status() == StatusCode::BAD_REQUEST,
            "provider-rejected login should retain its client-error status",
        )?;
        require_test(
            !provider_error_response
                .headers()
                .contains_key(axum::http::header::SET_COOKIE)
                && !provider_error_response
                    .headers()
                    .contains_key(axum::http::header::LOCATION),
            "provider-rejected login must not set cookies or redirect",
        )?;
        let provider_error_body =
            axum::body::to_bytes(provider_error_response.into_body(), usize::MAX).await?;
        let provider_error_body = String::from_utf8_lossy(&provider_error_body);
        require_test(
            provider_error_body == "OIDC login was rejected by the identity provider"
                && !provider_error_body.contains(CALLBACK_ERROR_SENTINEL),
            "callback must not reflect provider error or description values",
        )?;

        let (login_state_count, stored_state): (i64, Option<String>) = sqlx::query_as(
            "SELECT count(*), min(state) FROM oidc_login_states WHERE redirect_to = $1",
        )
        .bind(&redirect_to)
        .fetch_one(&pool)
        .await?;
        require_test(
            login_state_count == 1 && stored_state.as_deref() == Some(&state_token),
            "successful OIDC login should persist exactly one matching single-use state",
        )?;
        require_test(
            discovery_requests.load(Ordering::SeqCst) == 2,
            "recovery request should rediscover provider metadata after the cold-cache failure",
        )?;
        require_test(
            jwks_requests.load(Ordering::SeqCst) == 1,
            "successful discovery should fetch the issuer JWKS exactly once",
        )?;
        require_test(
            last_discovery_path
                .lock()
                .expect("discovery path lock")
                .as_str()
                == "/.well-known/openid-configuration",
            "OIDC runtime used an unexpected provider discovery path",
        )?;

        let token_error_query = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("code", "test-code")
            .append_pair("state", &state_token)
            .finish();
        let token_error_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/auth/oidc/callback?{token_error_query}"))
                    .body(axum::body::Body::empty())?,
            )
            .await?;
        require_test(
            token_error_response.status() == StatusCode::BAD_GATEWAY,
            "token-endpoint failure should retain its gateway-error status",
        )?;
        require_test(
            !token_error_response
                .headers()
                .contains_key(axum::http::header::SET_COOKIE)
                && !token_error_response
                    .headers()
                    .contains_key(axum::http::header::LOCATION),
            "token-endpoint failure must not set cookies or redirect",
        )?;
        let token_error_body =
            axum::body::to_bytes(token_error_response.into_body(), usize::MAX).await?;
        let token_error_body = String::from_utf8_lossy(&token_error_body);
        require_test(
            token_error_body == PUBLIC_OIDC_FAILURE_MESSAGE
                && !token_error_body.contains(TOKEN_ERROR_SENTINEL),
            "callback must not expose the token-endpoint response body",
        )?;

        let corrupted_secret_rows =
            sqlx::query("UPDATE oidc_config SET client_secret_enc = $2 WHERE id = $1")
                .bind(oidc_id)
                .bind(CORRUPTED_SECRET_SENTINEL)
                .execute(&pool)
                .await?
                .rows_affected();
        require_test(
            corrupted_secret_rows == 1,
            "corrupting the test OIDC secret should update exactly the singleton row",
        )?;
        state.oidc_runtime_cache.invalidate().await;
        let discoveries_before_corrupt_secret = discovery_requests.load(Ordering::SeqCst);
        let corrupt_secret_response = app.oneshot(request()).await?;
        require_test(
            corrupt_secret_response.status() == StatusCode::BAD_GATEWAY,
            "invalid OIDC client secret should return a visible 502 response",
        )?;
        require_test(
            !corrupt_secret_response
                .headers()
                .contains_key(axum::http::header::SET_COOKIE),
            "invalid OIDC client secret must not issue a session cookie",
        )?;
        require_test(
            !corrupt_secret_response
                .headers()
                .contains_key(axum::http::header::LOCATION),
            "invalid OIDC client secret must not return a redirect",
        )?;
        let corrupt_secret_body =
            axum::body::to_bytes(corrupt_secret_response.into_body(), usize::MAX).await?;
        let corrupt_secret_body = String::from_utf8_lossy(&corrupt_secret_body);
        require_test(
            corrupt_secret_body == PUBLIC_OIDC_FAILURE_MESSAGE,
            "invalid OIDC client secret should return the stable generic failure message",
        )?;
        require_test(
            !corrupt_secret_body.contains(CORRUPTED_SECRET_SENTINEL),
            "invalid OIDC client secret response must not expose stored ciphertext",
        )?;
        require_test(
            discovery_requests.load(Ordering::SeqCst) == discoveries_before_corrupt_secret,
            "invalid OIDC client secret should fail before provider discovery",
        )?;
        Ok::<(), Box<dyn Error + Send + Sync>>(())
    })
    .catch_unwind()
    .await;

    issuer_task.abort();
    let _ = issuer_task.await;
    let cleanup = restore_oidc_config(&pool, &state, &snapshot, &redirect_to).await;
    match (attempt, cleanup) {
        (Ok(Ok(())), Ok(())) => Ok(()),
        (Ok(Err(test_error)), Ok(())) => Err(test_error),
        (Ok(Ok(())), Err(cleanup_error)) => Err(cleanup_error),
        (Ok(Err(test_error)), Err(cleanup_error)) => Err(format!(
            "OIDC outage/recovery test failed ({test_error}); cleanup also failed ({cleanup_error})"
        )
        .into()),
        (Err(payload), Ok(())) => std::panic::resume_unwind(payload),
        (Err(payload), Err(cleanup_error)) => {
            eprintln!("OIDC outage/recovery cleanup failed while unwinding: {cleanup_error}");
            std::panic::resume_unwind(payload)
        }
    }
}
