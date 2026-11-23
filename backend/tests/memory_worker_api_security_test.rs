//! HTTP-level negative tests for the workload-only Memory source API.
//!
//! Run with the disposable PostgreSQL/RustFS integration environment:
//! `cargo test -p rustshare-server --test memory_worker_api_security_test -- --ignored --test-threads=1`

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use chrono::Utc;
use rustshare_crypto::WebhookSigner;
use rustshare_server::{memory_worker_auth::MemoryWorkerRequestAuth, routes};
use serde_json::json;
use tower::ServiceExt;
use uuid::Uuid;

mod support;

const SECRET: &str = "memory-worker-http-test-secret-0123456789";
const SIGNATURE_HEADER: &str = "X-Elembra-Memory-Signature";

fn signature_at(method: &str, path: &str, body: &[u8], timestamp: i64) -> String {
    let mut payload = b"elembra-memory-worker-v1\0".to_vec();
    for part in [method.as_bytes(), path.as_bytes(), body] {
        payload.extend_from_slice(&(part.len() as u64).to_be_bytes());
        payload.extend_from_slice(part);
    }
    WebhookSigner::new(SECRET)
        .sign_with_timestamp(timestamp, payload)
        .expect("sign test request")
}

fn request(method: Method, path: &str, body: Vec<u8>, signature: Option<String>) -> Request<Body> {
    let mut builder = Request::builder().method(method).uri(path);
    if let Some(signature) = signature {
        builder = builder.header(SIGNATURE_HEADER, signature);
    }
    builder
        .header("content-type", "application/json")
        .body(Body::from(body))
        .expect("build request")
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL and RustFS"]
async fn memory_worker_http_rejects_unbounded_unauthenticated_and_malformed_requests() {
    let state = support::calendar_harness::setup_test_env().await;
    // This test binary runs with one test thread in CI. The secret is only a
    // disposable test value and is not used by any production process.
    std::env::set_var("RUSTSHARE_MEMORY_WORKER_SECRET", SECRET);
    let restart_app = routes::memory_worker_routes().with_state(state.clone());
    let app = routes::memory_worker_routes().with_state(state);

    let cases = [
        (
            "/internal/v1/memory/files/snapshot",
            json!({ "source": "files", "event_id": Uuid::new_v4(), "extra": true }),
            vec![json!({ "source": "", "event_id": Uuid::nil() })],
        ),
        (
            "/internal/v1/memory/files/reconcile/tenants",
            json!({ "after": null, "limit": 1, "extra": true }),
            vec![
                json!({ "after": Uuid::nil(), "limit": 1 }),
                json!({ "after": null, "limit": 101 }),
            ],
        ),
        (
            "/internal/v1/memory/files/reconcile/page",
            json!({ "tenant_id": Uuid::new_v4(), "after_file_id": null, "limit": 1, "extra": true }),
            vec![
                json!({ "tenant_id": Uuid::new_v4(), "after_file_id": Uuid::nil(), "limit": 1 }),
                json!({ "tenant_id": Uuid::new_v4(), "after_file_id": null, "limit": 101 }),
            ],
        ),
    ];

    let auth = MemoryWorkerRequestAuth::new(SECRET).expect("valid test secret");
    for (path, unknown_field, invalid_requests) in cases {
        let malformed = b"{".to_vec();
        let response = app
            .clone()
            .oneshot(request(Method::POST, path, malformed.clone(), None))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{path}");

        let stale = signature_at(
            "POST",
            path,
            &malformed,
            Utc::now().timestamp().saturating_sub(61),
        );
        let response = app
            .clone()
            .oneshot(request(Method::POST, path, malformed, Some(stale)))
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "stale HMAC: {path}"
        );

        let invalid_signature = format!("t={},v1={}", Utc::now().timestamp(), "00".repeat(32));
        let response = app
            .clone()
            .oneshot(request(
                Method::POST,
                path,
                b"{}".to_vec(),
                Some(invalid_signature),
            ))
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "invalid HMAC: {path}"
        );

        let unknown = serde_json::to_vec(&unknown_field).unwrap();
        let signed = auth.sign_request("POST", path, &unknown).unwrap();
        let response = app
            .clone()
            .oneshot(request(Method::POST, path, unknown, Some(signed)))
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "unknown field: {path}"
        );

        for invalid_value in invalid_requests {
            let invalid = serde_json::to_vec(&invalid_value).unwrap();
            let signed = auth.sign_request("POST", path, &invalid).unwrap();
            let response = app
                .clone()
                .oneshot(request(Method::POST, path, invalid, Some(signed)))
                .await
                .unwrap();
            assert_eq!(
                response.status(),
                StatusCode::BAD_REQUEST,
                "invalid bounds: {path}"
            );
        }

        let oversized = vec![b'x'; 4 * 1024 + 1];
        let response = app
            .clone()
            .oneshot(request(Method::POST, path, oversized, None))
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::PAYLOAD_TOO_LARGE,
            "body limit: {path}"
        );

        let body = serde_json::to_vec(&unknown_field).unwrap();
        let mismatched_path_signature = auth.sign_request("POST", path, &body).unwrap();
        let response = app
            .clone()
            .oneshot(request(
                Method::POST,
                &format!("{path}?unexpected=1"),
                body,
                Some(mismatched_path_signature),
            ))
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "path binding: {path}"
        );

        let response = app
            .clone()
            .oneshot(request(Method::GET, path, Vec::new(), None))
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::METHOD_NOT_ALLOWED,
            "method: {path}"
        );
    }

    let path = "/internal/v1/memory/files/reconcile/page";
    let body = serde_json::to_vec(&json!({
        "tenant_id": Uuid::nil(),
        "after_file_id": null,
        "limit": 1
    }))
    .unwrap();
    let signed = auth.sign_request("POST", path, &body).unwrap();
    let (first, concurrent_replay) = tokio::join!(
        app.clone().oneshot(request(
            Method::POST,
            path,
            body.clone(),
            Some(signed.clone()),
        )),
        app.oneshot(request(
            Method::POST,
            path,
            body.clone(),
            Some(signed.clone()),
        )),
    );
    let mut statuses = [first.unwrap().status(), concurrent_replay.unwrap().status()];
    statuses.sort();
    assert_eq!(statuses, [StatusCode::OK, StatusCode::UNAUTHORIZED]);

    let response = restart_app
        .oneshot(request(Method::POST, path, body, Some(signed)))
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "a claimed signature stays rejected by a fresh router sharing the database"
    );
}
