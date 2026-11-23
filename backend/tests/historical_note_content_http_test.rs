//! Historical Note content stays exact and follows current Files authorization.
//!
//! Run with the disposable PostgreSQL/RustFS integration environment and
//! `--ignored --test-threads=1`.

mod support;

use axum::{
    body::{to_bytes, Body},
    http::{header, Method, Request, StatusCode},
    Router,
};
use bytes::Bytes;
use chrono::Utc;
use rustshare_core::domain::{Share, SharePermissions};
use rustshare_server::{routes, state::AppState};
use support::calendar_harness::{
    cleanup_tenant, create_auth_token, create_test_tenant, create_test_user, setup_test_env, SERIAL,
};
use tower::ServiceExt;
use uuid::Uuid;

async fn content(
    app: &Router,
    file_id: Uuid,
    version: Option<&str>,
    bearer: &str,
) -> (StatusCode, Bytes) {
    let version = version.map_or_else(String::new, |value| format!("?version=sha256%3A{value}"));
    let request = Request::builder()
        .method(Method::GET)
        .uri(format!("/api/v1/files/{file_id}/content{version}"))
        .header(header::AUTHORIZATION, format!("Bearer {bearer}"))
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    (status, body)
}

fn share(file_id: Uuid, owner_id: Uuid, recipient_id: Uuid, tenant_id: Uuid) -> Share {
    Share {
        id: Uuid::new_v4(),
        file_id: Some(file_id),
        folder_id: None,
        share_token: None,
        permissions: SharePermissions::View,
        password_hash: None,
        expires_at: None,
        upload_only: false,
        access_count: 0,
        recipient_user_id: Some(recipient_id),
        recipient_group_id: None,
        created_by: owner_id,
        created_at: Utc::now(),
        revoked_at: None,
        tenant_id,
    }
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL and RustFS"]
async fn historical_note_content_rechecks_authorization_and_never_falls_back() {
    let _serial = SERIAL.lock().await;
    let state: AppState = setup_test_env().await;
    let tenant = create_test_tenant(&state.db_pool).await;
    let other_tenant = create_test_tenant(&state.db_pool).await;
    let owner = create_test_user(&state, "historical_owner", tenant).await;
    let recipient = create_test_user(&state, "historical_recipient", tenant).await;
    let outsider = create_test_user(&state, "historical_outsider", other_tenant).await;
    let file = state
        .file_service
        .upload_file(
            owner.id,
            "citation.md".into(),
            None,
            Bytes::from_static(b"# Historical Note\n\nExact v1 body"),
            "text/markdown".into(),
            tenant,
        )
        .await
        .unwrap();
    let historical_hash = file.content_hash.clone();
    state
        .file_service
        .update_file(
            file.id,
            owner.id,
            file.current_version,
            Bytes::from_static(b"# Current Note\n\nCurrent v2 body"),
        )
        .await
        .unwrap();

    let share = share(file.id, owner.id, recipient.id, tenant);
    state.metadata_store.create_share(&share).await.unwrap();
    let app = routes::file_routes().with_state(state.clone());
    let owner_token = create_auth_token(&state, owner.id, tenant);
    let recipient_token = create_auth_token(&state, recipient.id, tenant);
    let outsider_token = create_auth_token(&state, outsider.id, other_tenant);

    let (status, body) = content(&app, file.id, Some(&historical_hash), &recipient_token).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_ref(), b"# Historical Note\n\nExact v1 body");
    let (status, body) = content(&app, file.id, None, &owner_token).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_ref(), b"# Current Note\n\nCurrent v2 body");

    state
        .metadata_store
        .revoke_share(share.id, owner.id)
        .await
        .unwrap();
    let (status, _) = content(&app, file.id, Some(&historical_hash), &recipient_token).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "revocation applies to old versions"
    );

    let (status, _) = content(&app, file.id, Some(&historical_hash), &outsider_token).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "cross-tenant reads stay denied"
    );

    let unavailable = "deadbeef".repeat(8);
    let (status, body) = content(&app, file.id, Some(&unavailable), &owner_token).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_ne!(body.as_ref(), b"# Current Note\n\nCurrent v2 body");

    cleanup_tenant(&state.db_pool, tenant).await;
    cleanup_tenant(&state.db_pool, other_tenant).await;
}
