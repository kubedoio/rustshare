//! PostgreSQL-backed HTTP tests for permission-safe meeting resource links.
//!
//! Run against the guarded disposable DB and object store:
//! `SQLX_OFFLINE=true cargo test -p rustshare-server --test meeting_resource_links_test -- --ignored --test-threads=1`

use std::sync::Arc;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use bytes::Bytes;
use rustshare_core::domain::ApplicationRegistry;
use rustshare_resource_auth::{ResourceOwnerRegistry, SourceAuthorizer};
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

mod support;

use support::calendar_harness::*;

fn app(state: rustshare_server::state::AppState) -> axum::Router<()> {
    rustshare_server::routes::meeting_routes()
        .merge(rustshare_server::routes::file_routes())
        .merge(rustshare_server::routes::note_routes())
        .merge(rustshare_server::routes::admin_routes())
        .with_state(state)
        .layer(axum::middleware::from_fn(
            rustshare_server::middleware::security_headers_middleware,
        ))
}

fn install_files_authorizer(state: &mut rustshare_server::state::AppState) {
    let registry = ApplicationRegistry::first_party().expect("valid first-party applications");
    let repository = Arc::new(
        rustshare_infrastructure::repositories::PermissionResolverRepository::new(
            state.db_pool.clone(),
        ),
    );
    let mut owners = ResourceOwnerRegistry::new();
    owners
        .register(
            Arc::new(rustshare_server::authz::FilesResourceOwner::new(
                state.permission_resolver.clone(),
                repository,
                state.metadata_store.clone(),
                state.object_store.clone(),
            )),
            &registry,
        )
        .expect("register Files resource owner");
    state.source_authorizer = Arc::new(SourceAuthorizer::new(owners));
}

async fn request_json(
    app: &axum::Router<()>,
    token: &str,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(body.map_or_else(Body::empty, |value| Body::from(value.to_string())))
        .expect("build HTTP request");
    let response = app.clone().oneshot(request).await.expect("HTTP response");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read HTTP response");
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("parse JSON response")
    };
    (status, body)
}

async fn create_file(
    state: &rustshare_server::state::AppState,
    owner_id: Uuid,
    tenant_id: Uuid,
    name: &str,
) -> Uuid {
    state
        .file_service
        .upload_file(
            owner_id,
            name.to_string(),
            None,
            Bytes::from_static(b"meeting resource content"),
            "text/plain".to_string(),
            tenant_id,
        )
        .await
        .expect("upload fixture file")
        .id
}

async fn grant_direct(
    pool: &PgPool,
    file_id: Uuid,
    owner_id: Uuid,
    recipient_id: Uuid,
    tenant_id: Uuid,
) {
    sqlx::query(
        "INSERT INTO shares (id, file_id, share_token, created_by, permissions, recipient_user_id, tenant_id)
         VALUES ($1, $2, NULL, $3, 'View', $4, $5)",
    )
    .bind(Uuid::new_v4())
    .bind(file_id)
    .bind(owner_id)
    .bind(recipient_id)
    .bind(tenant_id)
    .execute(pool)
    .await
    .expect("create direct view grant");
}

async fn create_meeting(
    state: &rustshare_server::state::AppState,
    user_id: Uuid,
    tenant_id: Uuid,
) -> Uuid {
    state
        .meeting_service
        .create_meeting(
            user_id,
            tenant_id,
            "Resource links test".into(),
            "Test team".into(),
            chrono::Utc::now(),
            "Agenda".into(),
        )
        .await
        .expect("create meeting")
        .id
}

fn resource_ref(file_id: Uuid) -> String {
    format!("elembra://io.elembra.files/file/{file_id}")
}

async fn cleanup_tenant_with_versions(pool: &PgPool, tenant_id: Uuid) {
    sqlx::query("DELETE FROM shares WHERE tenant_id = $1")
        .bind(tenant_id)
        .execute(pool)
        .await
        .expect("clean up fixture shares before users");
    sqlx::query("DELETE FROM user_groups WHERE tenant_id = $1")
        .bind(tenant_id)
        .execute(pool)
        .await
        .expect("clean up fixture groups before users");
    sqlx::query("DELETE FROM file_versions WHERE tenant_id = $1")
        .bind(tenant_id)
        .execute(pool)
        .await
        .expect("clean up file versions before fixture users");
    cleanup_tenant(pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "requires the guarded disposable PostgreSQL and object-store services"]
async fn direct_links_are_idempotent_permission_aware_persistent_and_non_destructive() {
    let _guard = SERIAL.lock().await;
    let mut state = setup_test_env().await;
    install_files_authorizer(&mut state);
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let owner = create_test_user(&state, "meeting_link_file_owner", tenant_id).await;
    let attendee = create_test_user(&state, "meeting_link_attendee", tenant_id).await;
    let file_id = create_file(&state, owner.id, tenant_id, "agenda.txt").await;
    grant_direct(&state.db_pool, file_id, owner.id, attendee.id, tenant_id).await;
    let meeting_id = create_meeting(&state, attendee.id, tenant_id).await;
    let token = create_auth_token(&state, attendee.id, tenant_id);
    let api = app(state.clone());
    let uri = format!("/api/v1/meetings/{meeting_id}/resources");
    let payload = json!({"resource_ref": resource_ref(file_id)});
    let uppercase_payload = json!({
        "resource_ref": format!("elembra://io.elembra.files/file/{}", file_id.to_string().to_uppercase())
    });

    let (left, right) = tokio::join!(
        request_json(&api, &token, "POST", &uri, Some(payload.clone())),
        request_json(&api, &token, "POST", &uri, Some(uppercase_payload))
    );
    let (left_status, left_body) = left;
    let (right_status, right_body) = right;
    assert_eq!(left_status, StatusCode::OK, "{left_body}");
    assert_eq!(right_status, StatusCode::OK, "{right_body}");
    let first_link_id = left_body["id"].as_str().expect("link id").to_string();
    assert_eq!(left_body["id"], first_link_id);
    assert_eq!(right_body["id"], first_link_id);

    let (status, retry) = request_json(&api, &token, "POST", &uri, Some(payload.clone())).await;
    assert_eq!(status, StatusCode::OK, "{retry}");
    assert_eq!(
        retry["id"], first_link_id,
        "retries return the canonical link"
    );

    let (status, page) = request_json(&api, &token, "GET", &uri, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page["total"], 1);
    assert_eq!(page["items"][0]["available"], true);
    assert_eq!(page["items"][0]["display_name"], "agenda.txt");

    let note = state
        .note_service
        .create_note(
            attendee.id,
            tenant_id,
            Some("Meeting note resource".into()),
            None,
            Some("Original note content".into()),
        )
        .await
        .expect("create note fixture");
    let (status, linked_note) = request_json(
        &api,
        &token,
        "POST",
        &uri,
        Some(json!({"resource_ref": resource_ref(note.id)})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{linked_note}");
    assert_eq!(linked_note["resource_kind"], "note");
    assert_eq!(linked_note["open_url"], format!("/apps/notes/{}", note.id));
    let (status, note_read) = request_json(
        &api,
        &token,
        "GET",
        &format!("/api/v1/notes/{}", note.id),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{note_read}");

    let moved_file = create_file(&state, attendee.id, tenant_id, "moved-agenda.txt").await;
    let (status, moved_link) = request_json(
        &api,
        &token,
        "POST",
        &uri,
        Some(json!({"resource_ref": resource_ref(moved_file)})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{moved_link}");
    let destination = state
        .folder_service
        .create_folder(
            "Meeting Resource Destination".into(),
            None,
            attendee.id,
            tenant_id,
        )
        .await
        .expect("create destination folder");
    sqlx::query("UPDATE files SET parent_folder_id = $1, path = $2 WHERE id = $3")
        .bind(destination.id)
        .bind(format!("{}/moved-agenda.txt", destination.path))
        .bind(moved_file)
        .execute(&state.db_pool)
        .await
        .expect("move linked resource");
    let (status, moved_resources) = request_json(&api, &token, "GET", &uri, None).await;
    assert_eq!(status, StatusCode::OK, "{moved_resources}");
    let moved_entry = moved_resources["items"]
        .as_array()
        .expect("linked resource list")
        .iter()
        .find(|item| item["id"] == moved_link["id"])
        .expect("moved resource link remains present");
    assert_eq!(moved_entry["available"], true);
    assert_eq!(moved_entry["display_name"], "moved-agenda.txt");
    assert_eq!(
        moved_entry["open_url"],
        format!("/files?preview={moved_file}")
    );

    let deleted_file = create_file(&state, attendee.id, tenant_id, "deleted-agenda.txt").await;
    let (status, deleted_link) = request_json(
        &api,
        &token,
        "POST",
        &uri,
        Some(json!({"resource_ref": resource_ref(deleted_file)})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{deleted_link}");
    let (status, _) = request_json(
        &api,
        &token,
        "DELETE",
        &format!("/api/v1/files/{deleted_file}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = request_json(
        &api,
        &token,
        "DELETE",
        &format!("/api/v1/files/{deleted_file}/permanent"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, deleted_resources) = request_json(&api, &token, "GET", &uri, None).await;
    assert_eq!(status, StatusCode::OK, "{deleted_resources}");
    let deleted_entry = deleted_resources["items"]
        .as_array()
        .expect("linked resource list")
        .iter()
        .find(|item| item["id"] == deleted_link["id"])
        .expect("deleted resource link remains for safe historical display");
    assert_eq!(deleted_entry["available"], false);
    assert!(deleted_entry.get("resource_ref").is_none());
    assert!(deleted_entry.get("display_name").is_none());
    assert!(deleted_entry.get("open_url").is_none());
    assert!(!deleted_resources.to_string().contains("deleted-agenda.txt"));

    let foreign_tenant = create_test_tenant(&state.db_pool).await;
    let foreign_owner =
        create_test_user(&state, "meeting_link_foreign_owner", foreign_tenant).await;
    let foreign_token = create_auth_token(&state, foreign_owner.id, foreign_tenant);
    let (status, concealed) = request_json(&api, &foreign_token, "GET", &uri, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (missing_status, missing_body) = request_json(
        &api,
        &foreign_token,
        "GET",
        &format!("/api/v1/meetings/{}/resources", Uuid::new_v4()),
        None,
    )
    .await;
    assert_eq!(missing_status, status);
    assert_eq!(missing_body, concealed);
    for marker in [
        "agenda.txt",
        "Meeting note resource",
        "Original note content",
    ] {
        assert!(!concealed.to_string().contains(marker));
    }
    let foreign_file = create_file(&state, foreign_owner.id, foreign_tenant, "foreign.txt").await;
    for id in [foreign_file, Uuid::new_v4()] {
        let (status, _) = request_json(
            &api,
            &token,
            "POST",
            &uri,
            Some(json!({"resource_ref": resource_ref(id)})),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
    let (status, _) = request_json(
        &api,
        &token,
        "DELETE",
        &format!("/api/v1/notes/{}", note.id),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, stale_note_resources) = request_json(&api, &token, "GET", &uri, None).await;
    assert_eq!(status, StatusCode::OK);
    let stale_note = stale_note_resources["items"]
        .as_array()
        .expect("resource list")
        .iter()
        .find(|item| item["id"] == linked_note["id"])
        .expect("stale note link remains attributable");
    assert_eq!(stale_note["available"], false);
    for field in ["display_name", "open_url", "resource_ref"] {
        assert!(stale_note.get(field).is_none());
    }
    assert!(!stale_note_resources
        .to_string()
        .contains("Original note content"));
    let private_same_tenant = create_file(&state, owner.id, tenant_id, "unshared.txt").await;
    let (status, _) = request_json(
        &api,
        &token,
        "POST",
        &uri,
        Some(json!({"resource_ref": resource_ref(private_same_tenant)})),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "same-tenant existence grants no access"
    );
    let inaccessible_rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM meeting_resource_links
         WHERE tenant_id = $1 AND meeting_folder_id = $2 AND resource_ref_uri = $3",
    )
    .bind(tenant_id)
    .bind(meeting_id)
    .bind(resource_ref(private_same_tenant))
    .fetch_one(&state.db_pool)
    .await
    .expect("count inaccessible-resource links");
    assert_eq!(inaccessible_rows, 0);

    let hidden_id = create_file(&state, attendee.id, tenant_id, "index.md").await;
    let (status, _) = request_json(
        &api,
        &token,
        "GET",
        &format!("/api/v1/files/{hidden_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = request_json(
        &api,
        &token,
        "POST",
        &uri,
        Some(json!({"resource_ref": resource_ref(hidden_id)})),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "hidden Files resources cannot be linked"
    );

    let (status, file) = request_json(
        &api,
        &token,
        "GET",
        &format!("/api/v1/files/{file_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{file}");

    let restarted = setup_test_env().await;
    let mut restarted = restarted;
    install_files_authorizer(&mut restarted);
    let restarted_api = app(restarted.clone());
    let (status, historical) = request_json(&restarted_api, &token, "GET", &uri, None).await;
    assert_eq!(status, StatusCode::OK, "{historical}");
    assert_eq!(historical["total"], 4, "links persist across restart");

    let unlink_uri = format!("{uri}/{first_link_id}");
    let (left, right) = tokio::join!(
        request_json(&restarted_api, &token, "DELETE", &unlink_uri, None),
        request_json(&restarted_api, &token, "DELETE", &unlink_uri, None)
    );
    assert_eq!(left.0, StatusCode::NO_CONTENT);
    assert_eq!(right.0, StatusCode::NO_CONTENT);
    let (status, _) = request_json(
        &restarted_api,
        &token,
        "GET",
        &format!("/api/v1/files/{file_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "unlink must not delete the target");
    let stored_file = state
        .file_service
        .get_file(file_id, attendee.id)
        .await
        .expect("unlink leaves target metadata accessible");
    let stored_content = state
        .object_store
        .get(&stored_file.storage_key())
        .await
        .expect("unlink leaves target content intact");
    assert_eq!(stored_content.as_ref(), b"meeting resource content");

    cleanup_tenant_with_versions(&state.db_pool, foreign_tenant).await;
    cleanup_tenant_with_versions(&state.db_pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "requires the guarded disposable PostgreSQL and object-store services"]
async fn revoked_direct_and_group_grants_hide_linked_metadata_immediately() {
    let _guard = SERIAL.lock().await;
    let mut state = setup_test_env().await;
    install_files_authorizer(&mut state);
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let owner = create_test_user(&state, "meeting_group_owner", tenant_id).await;
    let attendee = create_test_user(&state, "meeting_group_attendee", tenant_id).await;
    let file_id = create_file(&state, owner.id, tenant_id, "private-budget.xlsx").await;
    let group_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO user_groups (id, name, tenant_id, created_by) VALUES ($1, $2, $3, $4)",
    )
    .bind(group_id)
    .bind(format!("meeting-links-{group_id}"))
    .bind(tenant_id)
    .bind(owner.id)
    .execute(&state.db_pool)
    .await
    .expect("create fixture group");
    sqlx::query("INSERT INTO group_members (group_id, user_id, added_by) VALUES ($1, $2, $3)")
        .bind(group_id)
        .bind(attendee.id)
        .bind(owner.id)
        .execute(&state.db_pool)
        .await
        .expect("add attendee to group");
    sqlx::query(
        "INSERT INTO shares (id, file_id, share_token, created_by, permissions, recipient_group_id, tenant_id)
         VALUES ($1, $2, NULL, $3, 'View', $4, $5)",
    )
    .bind(Uuid::new_v4())
    .bind(file_id)
    .bind(owner.id)
    .bind(group_id)
    .bind(tenant_id)
    .execute(&state.db_pool)
    .await
    .expect("create group view grant");
    grant_direct(&state.db_pool, file_id, owner.id, attendee.id, tenant_id).await;

    let meeting_id = create_meeting(&state, attendee.id, tenant_id).await;
    let token = create_auth_token(&state, attendee.id, tenant_id);
    let api = app(state.clone());
    let uri = format!("/api/v1/meetings/{meeting_id}/resources");
    let (status, body) = request_json(
        &api,
        &token,
        "POST",
        &uri,
        Some(json!({"resource_ref": resource_ref(file_id)})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    sqlx::query("DELETE FROM shares WHERE file_id = $1 AND recipient_user_id = $2")
        .bind(file_id)
        .bind(attendee.id)
        .execute(&state.db_pool)
        .await
        .expect("revoke direct grant");
    let (status, still_available) = request_json(&api, &token, "GET", &uri, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(still_available["items"][0]["available"], true);

    sqlx::query("DELETE FROM group_members WHERE group_id = $1 AND user_id = $2")
        .bind(group_id)
        .bind(attendee.id)
        .execute(&state.db_pool)
        .await
        .expect("remove attendee from group");
    let (status, unavailable) = request_json(&api, &token, "GET", &uri, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(unavailable["items"][0]["available"], false);
    assert!(unavailable["items"][0].get("resource_ref").is_none());
    assert!(unavailable["items"][0].get("display_name").is_none());
    assert!(unavailable
        .to_string()
        .find("private-budget.xlsx")
        .is_none());

    let (status, _) = request_json(
        &api,
        &token,
        "GET",
        &format!("/api/v1/files/{file_id}"),
        None,
    )
    .await;
    assert_ne!(
        status,
        StatusCode::OK,
        "Files HTTP and meeting auth must agree"
    );

    cleanup_tenant_with_versions(&state.db_pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "requires the guarded disposable PostgreSQL and object-store services"]
async fn failed_link_insert_rolls_back_and_retry_succeeds() {
    let _guard = SERIAL.lock().await;
    let mut state = setup_test_env().await;
    install_files_authorizer(&mut state);
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let user = create_test_user(&state, "meeting_link_rollback", tenant_id).await;
    let file_id = create_file(&state, user.id, tenant_id, "retry.txt").await;
    let meeting_id = create_meeting(&state, user.id, tenant_id).await;
    let token = create_auth_token(&state, user.id, tenant_id);
    let api = app(state.clone());
    let uri = format!("/api/v1/meetings/{meeting_id}/resources");
    let payload = json!({"resource_ref": resource_ref(file_id)});

    sqlx::query(
        "CREATE OR REPLACE FUNCTION reject_test_meeting_resource_link() RETURNS trigger
         LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'simulated insert failure'; END $$",
    )
    .execute(&state.db_pool)
    .await
    .expect("install failure trigger function");
    sqlx::query(
        "CREATE TRIGGER reject_test_meeting_resource_link BEFORE INSERT ON meeting_resource_links
         FOR EACH ROW EXECUTE FUNCTION reject_test_meeting_resource_link()",
    )
    .execute(&state.db_pool)
    .await
    .expect("install failure trigger");
    let (failed_status, _) = request_json(&api, &token, "POST", &uri, Some(payload.clone())).await;
    sqlx::query("DROP TRIGGER reject_test_meeting_resource_link ON meeting_resource_links")
        .execute(&state.db_pool)
        .await
        .expect("remove failure trigger");
    sqlx::query("DROP FUNCTION reject_test_meeting_resource_link()")
        .execute(&state.db_pool)
        .await
        .expect("remove failure trigger function");
    assert_eq!(failed_status, StatusCode::INTERNAL_SERVER_ERROR);

    let row_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM meeting_resource_links
         WHERE tenant_id = $1 AND meeting_folder_id = $2 AND resource_ref_uri = $3",
    )
    .bind(tenant_id)
    .bind(meeting_id)
    .bind(resource_ref(file_id))
    .fetch_one(&state.db_pool)
    .await
    .expect("count rolled-back link");
    assert_eq!(row_count, 0, "failed insert must not leave a partial link");

    let (status, body) = request_json(&api, &token, "POST", &uri, Some(payload)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, page) = request_json(&api, &token, "GET", &uri, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page["total"], 1);
    cleanup_tenant_with_versions(&state.db_pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "requires the guarded disposable PostgreSQL and object-store services"]
async fn disabled_user_cannot_read_linked_meeting_resources() {
    let _guard = SERIAL.lock().await;
    let mut state = setup_test_env().await;
    install_files_authorizer(&mut state);
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let user = create_test_user(&state, "meeting_link_disabled", tenant_id).await;
    let file_id = create_file(&state, user.id, tenant_id, "disabled-user.txt").await;
    let meeting_id = create_meeting(&state, user.id, tenant_id).await;
    let token = create_auth_token(&state, user.id, tenant_id);
    let api = app(state.clone());
    let uri = format!("/api/v1/meetings/{meeting_id}/resources");
    let (status, body) = request_json(
        &api,
        &token,
        "POST",
        &uri,
        Some(json!({"resource_ref": resource_ref(file_id)})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    sqlx::query("UPDATE users SET disabled_at = now() WHERE id = $1")
        .bind(user.id)
        .execute(&state.db_pool)
        .await
        .expect("disable user");
    let (status, denied) = request_json(&api, &token, "GET", &uri, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(!denied.to_string().contains("disabled-user.txt"));
    cleanup_tenant_with_versions(&state.db_pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "requires the guarded disposable PostgreSQL and object-store services"]
async fn meeting_view_permission_does_not_allow_link_mutations() {
    let _guard = SERIAL.lock().await;
    let mut state = setup_test_env().await;
    install_files_authorizer(&mut state);
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let owner = create_test_user(&state, "meeting_link_meeting_owner", tenant_id).await;
    let viewer = create_test_user(&state, "meeting_link_meeting_viewer", tenant_id).await;
    let file_id = create_file(&state, owner.id, tenant_id, "owner-file.txt").await;
    let meeting_id = create_meeting(&state, owner.id, tenant_id).await;
    sqlx::query(
        "INSERT INTO shares (id, file_id, folder_id, share_token, created_by, permissions, recipient_user_id, tenant_id)
         VALUES ($1, NULL, $2, NULL, $3, 'View', $4, $5)",
    )
    .bind(Uuid::new_v4())
    .bind(meeting_id)
    .bind(owner.id)
    .bind(viewer.id)
    .bind(tenant_id)
    .execute(&state.db_pool)
    .await
    .expect("grant meeting view permission");

    let owner_token = create_auth_token(&state, owner.id, tenant_id);
    let viewer_token = create_auth_token(&state, viewer.id, tenant_id);
    let api = app(state.clone());
    let meeting_uri = format!("/api/v1/meetings/{meeting_id}");
    let (status, _) = request_json(&api, &viewer_token, "GET", &meeting_uri, None).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "meeting viewer can read the context"
    );

    let resources_uri = format!("{meeting_uri}/resources");
    let (status, linked) = request_json(
        &api,
        &owner_token,
        "POST",
        &resources_uri,
        Some(json!({"resource_ref": resource_ref(file_id)})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{linked}");
    let (status, _) = request_json(
        &api,
        &viewer_token,
        "POST",
        &resources_uri,
        Some(json!({"resource_ref": resource_ref(file_id)})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = request_json(
        &api,
        &viewer_token,
        "DELETE",
        &format!("{resources_uri}/{}", linked["id"].as_str().unwrap()),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, still_linked) =
        request_json(&api, &owner_token, "GET", &resources_uri, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(still_linked["total"], 1);

    cleanup_tenant_with_versions(&state.db_pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "requires the guarded disposable PostgreSQL and object-store services"]
async fn organization_admin_without_file_grant_cannot_link_or_read_private_file() {
    let _guard = SERIAL.lock().await;
    let mut state = setup_test_env().await;
    install_files_authorizer(&mut state);
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let operator = create_test_user(&state, "meeting_link_operator", tenant_id).await;
    let org_admin = create_test_user(&state, "meeting_link_org_admin", tenant_id).await;
    let owner = create_test_user(&state, "meeting_link_private_owner", tenant_id).await;
    sqlx::query("UPDATE users SET is_admin = true WHERE id = $1")
        .bind(operator.id)
        .execute(&state.db_pool)
        .await
        .expect("enable installation operator fixture");
    state.default_tenant_config = rustshare_server::config::DefaultTenantConfig::Set(tenant_id);
    let operator_token = create_auth_token(&state, operator.id, tenant_id);
    let admin_token = create_auth_token(&state, org_admin.id, tenant_id);
    let api = app(state.clone());
    let (status, body) = request_json(
        &api,
        &operator_token,
        "POST",
        &format!(
            "/api/v1/admin/organizations/{tenant_id}/administrators/{}",
            org_admin.id
        ),
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    let private_file = create_file(&state, owner.id, tenant_id, "admin-private.txt").await;
    let meeting_id = create_meeting(&state, org_admin.id, tenant_id).await;
    let resources_uri = format!("/api/v1/meetings/{meeting_id}/resources");
    let (status, _) = request_json(
        &api,
        &admin_token,
        "POST",
        &resources_uri,
        Some(json!({"resource_ref": resource_ref(private_file)})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Model an old/imported association to verify every read still consults
    // the owning Files permission resolver rather than treating the link as a grant.
    state
        .meeting_service
        .link_resource(meeting_id, tenant_id, resource_ref(private_file))
        .await
        .expect("insert historical link fixture");
    let (status, unavailable) = request_json(&api, &admin_token, "GET", &resources_uri, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(unavailable["items"][0]["available"], false);
    assert!(unavailable["items"][0].get("display_name").is_none());
    assert!(unavailable.to_string().find("admin-private.txt").is_none());
    let (status, _) = request_json(
        &api,
        &admin_token,
        "GET",
        &format!("/api/v1/files/{private_file}"),
        None,
    )
    .await;
    assert_ne!(status, StatusCode::OK);
    cleanup_tenant_with_versions(&state.db_pool, tenant_id).await;
}
