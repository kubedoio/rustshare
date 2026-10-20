//! PostgreSQL/HTTP regression tests for permission-safe meeting Decisions.
//!
//! Run only against the guarded disposable PostgreSQL and object store:
//! `SQLX_OFFLINE=true cargo test -p rustshare-server --test meeting_decisions_test -- --ignored --test-threads=1`

use std::sync::Arc;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use bytes::Bytes;
use rustshare_core::domain::ApplicationRegistry;
use rustshare_resource_auth::{ResourceOwnerRegistry, SourceAuthorizer};
use rustshare_server::state::AppState;
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

mod support;

use support::calendar_harness::*;

fn app(mut state: AppState) -> axum::Router<()> {
    let registry = ApplicationRegistry::first_party().expect("first-party applications");
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
    rustshare_server::routes::meeting_routes()
        .merge(rustshare_server::routes::decision_routes())
        .merge(rustshare_server::routes::file_routes())
        .with_state(state)
}

async fn request(
    app: &axum::Router<()>,
    token: &str,
    method: &str,
    uri: &str,
    payload: Option<Value>,
) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("Authorization", format!("Bearer {token}"))
                .header("Content-Type", "application/json")
                .body(payload.map_or_else(Body::empty, |value| Body::from(value.to_string())))
                .expect("build request"),
        )
        .await
        .expect("HTTP response");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body");
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("JSON response")
    };
    (status, body)
}

async fn meeting(state: &AppState, owner: Uuid, tenant: Uuid) -> Uuid {
    state
        .meeting_service
        .create_meeting(
            owner,
            tenant,
            "Historic planning meeting".into(),
            "Planning".into(),
            chrono::Utc::now(),
            "Discuss the release".into(),
        )
        .await
        .expect("create meeting")
        .id
}

async fn decision(state: &AppState, owner: Uuid, tenant: Uuid) -> Uuid {
    state
        .decision_service
        .create_decision(
            owner,
            tenant,
            "Private release decision".into(),
            "Planning".into(),
            "Confidential decision content".into(),
        )
        .await
        .expect("create Decision")
        .id
}

async fn direct_grant(pool: &PgPool, resource: Uuid, owner: Uuid, user: Uuid, tenant: Uuid) {
    sqlx::query(
        "INSERT INTO shares (id, file_id, share_token, created_by, permissions, recipient_user_id, tenant_id)
         VALUES ($1, $2, NULL, $3, 'View', $4, $5)",
    )
    .bind(Uuid::new_v4())
    .bind(resource)
    .bind(owner)
    .bind(user)
    .bind(tenant)
    .execute(pool)
    .await
    .expect("grant direct view");
}

async fn cleanup(pool: &PgPool, tenant: Uuid) {
    for table in ["shares", "user_groups", "file_versions"] {
        sqlx::query(&format!("DELETE FROM {table} WHERE tenant_id = $1"))
            .bind(tenant)
            .execute(pool)
            .await
            .expect("clean fixture dependencies");
    }
    cleanup_tenant(pool, tenant).await;
}

async fn decision_events(pool: &PgPool, decision_id: Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM events WHERE aggregate_id=$1 AND event_type LIKE '%DecisionModified%'",
    )
    .bind(decision_id)
    .fetch_one(pool)
    .await
    .expect("Decision modification events")
}

fn create_payload(request_id: Uuid) -> Value {
    json!({
        "request_id": request_id,
        "title": "Use the supported release",
        "category": "Planning",
        "content": "Approved after discussion"
    })
}

fn assert_unavailable(item: &Value) {
    assert_eq!(item["available"], false, "{item}");
    assert!(item.get("decision").is_none(), "{item}");
    let rendered = item.to_string();
    for protected in ["Private release", "Confidential", "open_url", "owner_id"] {
        assert!(!rendered.contains(protected), "{item}");
    }
}

#[tokio::test]
#[ignore = "requires the guarded disposable PostgreSQL and object-store services"]
async fn creation_is_idempotent_concurrent_editable_and_persistent_without_copying_provenance() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env().await;
    let tenant = create_test_tenant(&state.db_pool).await;
    let owner = create_test_user(&state, "meeting_decision_create", tenant).await;
    let meeting_id = meeting(&state, owner.id, tenant).await;
    let token = create_auth_token(&state, owner.id, tenant);
    let api = app(state.clone());
    let other_instance = app(setup_test_env().await);
    let uri = format!("/api/v1/meetings/{meeting_id}/decisions");
    let payload = create_payload(Uuid::new_v4());
    let (left, right) = tokio::join!(
        request(&api, &token, "POST", &uri, Some(payload.clone())),
        request(&other_instance, &token, "POST", &uri, Some(payload.clone()))
    );
    assert!(left.0.is_success(), "{left:?}");
    assert!(right.0.is_success(), "{right:?}");
    assert_eq!(left.1["id"], right.1["id"]);
    assert_eq!(left.1["decision"]["id"], right.1["decision"]["id"]);
    let decision_id = left.1["decision"]["id"].as_str().expect("decision ID");
    let created_id = Uuid::parse_str(decision_id).expect("created Decision UUID");
    let decision_uri = format!("/api/v1/decisions/{decision_id}");
    assert_eq!(decision_events(&state.db_pool, created_id).await, 1);
    for path in [&uri, &decision_uri] {
        let (status, opened) = request(&api, &token, "GET", path, None).await;
        assert_eq!(status, StatusCode::OK, "{opened}");
    }
    assert_eq!(
        decision_events(&state.db_pool, created_id).await,
        1,
        "opening/listing emits no modification"
    );
    let (status, edited) = request(
        &api,
        &token,
        "PUT",
        &decision_uri,
        Some(json!({"status": "Accepted", "decision_date": "2026-10-20T10:00:00Z"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{edited}");
    assert_eq!(edited["metadata"]["status"], "accepted");
    assert_eq!(edited["metadata"]["decision_date"], "2026-10-20T10:00:00Z");

    let mut changed = payload.clone();
    changed["title"] = json!("Different payload");
    let (status, _) = request(&api, &token, "POST", &uri, Some(changed)).await;
    assert_eq!(status, StatusCode::CONFLICT);
    let restarted = app(setup_test_env().await);
    let (status, retry) = request(&restarted, &token, "POST", &uri, Some(payload.clone())).await;
    assert!(status.is_success(), "{retry}");
    assert_eq!(retry["id"], left.1["id"]);
    assert_eq!(
        retry["decision"]["status"], "accepted",
        "retry preserves edits"
    );
    let (status, historical) = request(&restarted, &token, "GET", &uri, None).await;
    assert_eq!(status, StatusCode::OK, "{historical}");
    assert_eq!(historical["total"], 1);
    let (status, reverse) = request(
        &restarted,
        &token,
        "GET",
        &format!("{decision_uri}/meetings"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{reverse}");
    assert_eq!(reverse["items"][0]["id"], meeting_id.to_string());
    assert_eq!(reverse["items"][0]["is_origin"], true);

    let (status, duplicate) = request(
        &api,
        &token,
        "POST",
        &format!("{decision_uri}/duplicate"),
        None,
    )
    .await;
    assert!(status.is_success(), "{duplicate}");
    assert_ne!(duplicate["id"], decision_id);
    let (status, copied_reverse) = request(
        &api,
        &token,
        "GET",
        &format!(
            "/api/v1/decisions/{}/meetings",
            duplicate["id"].as_str().unwrap()
        ),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{copied_reverse}");
    assert_eq!(copied_reverse["items"], json!([]));
    assert_eq!(
        decision_events(&state.db_pool, created_id).await,
        2,
        "only creation and genuine edit notify"
    );
    let (status, _) = request(
        &api,
        &token,
        "DELETE",
        &format!("{uri}/{}", left.1["id"].as_str().unwrap()),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = request(&restarted, &token, "POST", &uri, Some(payload.clone())).await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "replaying creation must not undo unlink"
    );
    let (status, _) = request(
        &api,
        &token,
        "DELETE",
        &format!("/api/v1/decisions/{decision_id}"),
        None,
    )
    .await;
    assert!(status.is_success(), "delete target independently");
    let (status, _) = request(&restarted, &token, "POST", &uri, Some(payload)).await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "ready command cannot resurrect a deleted Decision"
    );
    assert!(state
        .file_service
        .get_file(created_id, owner.id)
        .await
        .is_err());
    assert_eq!(decision_events(&state.db_pool, created_id).await, 3);
    cleanup(&state.db_pool, tenant).await;
}

#[tokio::test]
#[ignore = "requires the guarded disposable PostgreSQL and object-store services"]
async fn decision_link_conflict_preserves_existing_resource_link_kind() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env().await;
    let tenant = create_test_tenant(&state.db_pool).await;
    let owner = create_test_user(&state, "meeting_decision_link_kind", tenant).await;
    let meeting_id = meeting(&state, owner.id, tenant).await;
    let decision_id = decision(&state, owner.id, tenant).await;
    let token = create_auth_token(&state, owner.id, tenant);
    let api = app(state.clone());
    let resource_ref = format!("elembra://io.elembra.files/file/{decision_id}");
    let (status, existing) = request(
        &api,
        &token,
        "POST",
        &format!("/api/v1/meetings/{meeting_id}/resources"),
        Some(json!({ "resource_ref": resource_ref.clone() })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{existing}");

    let (status, conflict) = request(
        &api,
        &token,
        "POST",
        &format!("/api/v1/meetings/{meeting_id}/decisions/links"),
        Some(json!({ "decision_id": decision_id })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{conflict}");

    let link_kind: String = sqlx::query_scalar(
        "SELECT link_kind FROM meeting_resource_links
         WHERE tenant_id=$1 AND meeting_folder_id=$2 AND resource_ref_uri=$3",
    )
    .bind(tenant)
    .bind(meeting_id)
    .bind(resource_ref)
    .fetch_one(&state.db_pool)
    .await
    .expect("original resource link remains");
    assert_eq!(link_kind, "resource");

    let (status, resources) = request(
        &api,
        &token,
        "GET",
        &format!("/api/v1/meetings/{meeting_id}/resources"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{resources}");
    assert_eq!(resources["items"].as_array().map(Vec::len), Some(1));
    cleanup(&state.db_pool, tenant).await;
}

#[tokio::test]
#[ignore = "requires the guarded disposable PostgreSQL and object-store services"]
async fn linking_existing_decisions_is_concurrent_retry_safe_and_unlink_preserves_content() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env().await;
    let tenant = create_test_tenant(&state.db_pool).await;
    let owner = create_test_user(&state, "meeting_decision_link", tenant).await;
    let meeting_id = meeting(&state, owner.id, tenant).await;
    let decision_id = decision(&state, owner.id, tenant).await;
    let token = create_auth_token(&state, owner.id, tenant);
    let api = app(state.clone());
    let uri = format!("/api/v1/meetings/{meeting_id}/decisions");
    let links_uri = format!("{uri}/links");
    let payload = json!({"decision_id": decision_id});
    let (left, right) = tokio::join!(
        request(&api, &token, "POST", &links_uri, Some(payload.clone())),
        request(&api, &token, "POST", &links_uri, Some(payload))
    );
    assert!(left.0.is_success(), "{left:?}");
    assert!(right.0.is_success(), "{right:?}");
    assert_eq!(left.1["id"], right.1["id"]);
    assert_eq!(
        left.1["decision"]["open_url"],
        format!("/apps/decisions/{decision_id}")
    );
    let (status, reverse) = request(
        &api,
        &token,
        "GET",
        &format!("/api/v1/decisions/{decision_id}/meetings"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{reverse}");
    assert_eq!(reverse["items"][0]["is_origin"], false);
    let unlink_uri = format!("{uri}/{}", left.1["id"].as_str().unwrap());
    let (left, right) = tokio::join!(
        request(&api, &token, "DELETE", &unlink_uri, None),
        request(&api, &token, "DELETE", &unlink_uri, None)
    );
    assert_eq!(left.0, StatusCode::NO_CONTENT, "{left:?}");
    assert_eq!(right.0, StatusCode::NO_CONTENT, "{right:?}");
    let (status, target) = request(
        &api,
        &token,
        "GET",
        &format!("/api/v1/decisions/{decision_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{target}");
    assert_eq!(target["content"], "Confidential decision content");
    let (status, empty) = request(&api, &token, "GET", &uri, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(empty["total"], 0);
    cleanup(&state.db_pool, tenant).await;
}

#[tokio::test]
#[ignore = "requires the guarded disposable PostgreSQL and object-store services"]
async fn overlapping_direct_and_group_permissions_are_rechecked_on_every_read() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env().await;
    let tenant = create_test_tenant(&state.db_pool).await;
    let owner = create_test_user(&state, "meeting_decision_owner", tenant).await;
    let viewer = create_test_user(&state, "meeting_decision_viewer", tenant).await;
    let decision_id = decision(&state, owner.id, tenant).await;
    let meeting_id = meeting(&state, viewer.id, tenant).await;
    direct_grant(&state.db_pool, decision_id, owner.id, viewer.id, tenant).await;
    let group = Uuid::new_v4();
    sqlx::query("INSERT INTO user_groups (id,name,tenant_id,created_by) VALUES ($1,$2,$3,$4)")
        .bind(group)
        .bind(format!("meeting-decisions-{group}"))
        .bind(tenant)
        .bind(owner.id)
        .execute(&state.db_pool)
        .await
        .expect("fixture group");
    sqlx::query("INSERT INTO group_members (group_id,user_id,added_by) VALUES ($1,$2,$3)")
        .bind(group)
        .bind(viewer.id)
        .bind(owner.id)
        .execute(&state.db_pool)
        .await
        .expect("fixture membership");
    sqlx::query(
        "INSERT INTO shares (id,file_id,share_token,created_by,permissions,recipient_group_id,tenant_id)
         VALUES ($1,$2,NULL,$3,'View',$4,$5)",
    )
    .bind(Uuid::new_v4())
    .bind(decision_id)
    .bind(owner.id)
    .bind(group)
    .bind(tenant)
    .execute(&state.db_pool)
    .await
    .expect("group view grant");
    let token = create_auth_token(&state, viewer.id, tenant);
    let api = app(state.clone());
    let uri = format!("/api/v1/meetings/{meeting_id}/decisions");
    let (status, linked) = request(
        &api,
        &token,
        "POST",
        &format!("{uri}/links"),
        Some(json!({"decision_id":decision_id})),
    )
    .await;
    assert!(status.is_success(), "{linked}");
    sqlx::query("DELETE FROM shares WHERE file_id=$1 AND recipient_user_id=$2")
        .bind(decision_id)
        .bind(viewer.id)
        .execute(&state.db_pool)
        .await
        .expect("revoke direct grant");
    let (status, group_available) = request(&api, &token, "GET", &uri, None).await;
    assert_eq!(status, StatusCode::OK, "{group_available}");
    assert_eq!(group_available["items"][0]["available"], true);
    sqlx::query("DELETE FROM group_members WHERE group_id=$1 AND user_id=$2")
        .bind(group)
        .bind(viewer.id)
        .execute(&state.db_pool)
        .await
        .expect("revoke final membership");
    let (status, unavailable) = request(&api, &token, "GET", &uri, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_unavailable(&unavailable["items"][0]);
    let (status, _) = request(
        &api,
        &token,
        "GET",
        &format!("/api/v1/decisions/{decision_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    cleanup(&state.db_pool, tenant).await;
}

#[tokio::test]
#[ignore = "requires the guarded disposable PostgreSQL and object-store services"]
async fn decision_edit_grant_updates_logical_metadata_without_granting_general_sidecar_access() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env().await;
    let tenant = create_test_tenant(&state.db_pool).await;
    let owner = create_test_user(&state, "meeting_decision_edit_owner", tenant).await;
    let recipient = create_test_user(&state, "meeting_decision_edit_recipient", tenant).await;
    let decision_id = decision(&state, owner.id, tenant).await;
    let original = state
        .decision_service
        .get_decision(decision_id, owner.id, tenant)
        .await
        .expect("original Decision");
    let sidecar_name = format!("{}.rustshare.json", original.name.trim_end_matches(".md"));
    let sidecar = state
        .metadata_store
        .list_files(original.parent_folder_id, owner.id, tenant)
        .await
        .expect("owner sibling files")
        .into_iter()
        .find(|file| file.name == sidecar_name)
        .expect("Decision sidecar");
    direct_grant(&state.db_pool, decision_id, owner.id, recipient.id, tenant).await;
    let token = create_auth_token(&state, recipient.id, tenant);
    let api = app(state.clone());
    let uri = format!("/api/v1/decisions/{decision_id}");
    let edit = json!({
        "content":"Edit only after authorization",
        "status":"Accepted",
        "decision_date":"2026-10-21T09:00:00Z"
    });
    let (status, body) = request(&api, &token, "PUT", &uri, Some(edit.clone())).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    let unchanged = state
        .decision_service
        .get_decision(decision_id, owner.id, tenant)
        .await
        .expect("View denial preserves original Decision");
    assert_eq!(
        unchanged.content, original.content,
        "no partial main-file write"
    );
    assert_eq!(unchanged.metadata, original.metadata, "no sidecar write");

    sqlx::query("UPDATE shares SET permissions='Edit' WHERE file_id=$1 AND recipient_user_id=$2")
        .bind(decision_id)
        .bind(recipient.id)
        .execute(&state.db_pool)
        .await
        .expect("upgrade main Decision grant to Edit");
    let (status, updated) = request(&api, &token, "PUT", &uri, Some(edit)).await;
    assert_eq!(status, StatusCode::OK, "{updated}");
    assert_eq!(updated["content"], "Edit only after authorization");
    assert_eq!(updated["metadata"]["status"], "accepted");
    assert_eq!(updated["metadata"]["decision_date"], "2026-10-21T09:00:00Z");
    let expected_sidecar = state
        .file_service
        .get_file(sidecar.id, owner.id)
        .await
        .expect("updated sidecar");
    let expected_bytes = state
        .object_store
        .get(&expected_sidecar.storage_key())
        .await
        .expect("sidecar bytes");
    let (status, _) = request(
        &api,
        &token,
        "POST",
        &format!("/api/v1/files/{}/edit", sidecar.id),
        Some(json!({"content":"dGFtcGVyZWQ=", "save_mode":"overwrite"})),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "main Edit grants no general sidecar edit"
    );
    let preserved = state
        .file_service
        .get_file(sidecar.id, owner.id)
        .await
        .expect("preserved sidecar");
    assert_eq!(
        state
            .object_store
            .get(&preserved.storage_key())
            .await
            .unwrap(),
        expected_bytes
    );
    state
        .file_service
        .delete_file(sidecar.id, owner.id)
        .await
        .expect("remove legacy sidecar");
    state
        .metadata_store
        .permanently_delete_file(sidecar.id, owner.id)
        .await
        .expect("remove sidecar metadata row");
    let (status, denied) = request(
        &api,
        &token,
        "PUT",
        &uri,
        Some(json!({"content":"Must not partially persist", "status":"Rejected"})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{denied}");
    let unchanged = state
        .decision_service
        .get_decision(decision_id, owner.id, tenant)
        .await
        .expect("legacy main preserved");
    assert_eq!(unchanged.content, "Edit only after authorization");
    assert!(
        !state
            .metadata_store
            .list_files(original.parent_folder_id, owner.id, tenant)
            .await
            .unwrap()
            .iter()
            .any(|file| file.name == sidecar_name),
        "denied actor cannot recreate sidecar as owner"
    );
    let owner_token = create_auth_token(&state, owner.id, tenant);
    let (status, repaired) = request(
        &api,
        &owner_token,
        "PUT",
        &uri,
        Some(json!({"status":"Accepted", "decision_date":"2026-10-22T09:00:00Z"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{repaired}");
    assert_eq!(repaired["content"], "Edit only after authorization");
    assert_eq!(repaired["metadata"]["status"], "accepted");
    assert_eq!(
        repaired["metadata"]["decision_date"],
        "2026-10-22T09:00:00Z"
    );
    let repaired_id = state
        .metadata_store
        .list_files(original.parent_folder_id, owner.id, tenant)
        .await
        .unwrap()
        .into_iter()
        .find(|file| file.name == sidecar_name)
        .expect("repaired sidecar")
        .id;
    let (status, retry) = request(
        &api,
        &owner_token,
        "PUT",
        &uri,
        Some(json!({"status":"Accepted"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{retry}");
    let matching: Vec<_> = state
        .metadata_store
        .list_files(original.parent_folder_id, owner.id, tenant)
        .await
        .unwrap()
        .into_iter()
        .filter(|file| file.name == sidecar_name)
        .collect();
    assert_eq!(matching.len(), 1);
    assert_eq!(
        matching[0].id, repaired_id,
        "repair retries preserve sidecar identity"
    );
    cleanup(&state.db_pool, tenant).await;
}

#[tokio::test]
#[ignore = "requires the guarded disposable PostgreSQL and object-store services"]
async fn reverse_pagination_can_reach_authorized_meetings_after_inaccessible_candidates() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env().await;
    let tenant = create_test_tenant(&state.db_pool).await;
    let viewer = create_test_user(&state, "meeting_decision_page_viewer", tenant).await;
    let other = create_test_user(&state, "meeting_decision_page_other", tenant).await;
    let decision_id = decision(&state, viewer.id, tenant).await;
    let resource_ref = format!("elembra://io.elembra.files/file/{decision_id}");
    for index in 0..100 {
        let folder = rustshare_core::domain::Folder::new_root_with_name(
            format!("Private candidate {index}"),
            other.id,
            tenant,
        );
        state
            .metadata_store
            .create_folder(&folder)
            .await
            .expect("lean inaccessible candidate");
        sqlx::query("INSERT INTO meeting_resource_links (tenant_id,meeting_folder_id,resource_ref_uri,link_kind,created_at) VALUES ($1,$2,$3,'decision','2026-01-01T00:00:00Z')")
            .bind(tenant).bind(folder.id).bind(&resource_ref).execute(&state.db_pool).await.expect("seed inaccessible historical association");
    }
    let visible_id = meeting(&state, viewer.id, tenant).await;
    sqlx::query("INSERT INTO meeting_resource_links (tenant_id,meeting_folder_id,resource_ref_uri,link_kind,created_at) VALUES ($1,$2,$3,'decision','2026-02-01T00:00:00Z')")
        .bind(tenant).bind(visible_id).bind(&resource_ref).execute(&state.db_pool).await.expect("seed later accessible association");
    let later_visible_id = meeting(&state, viewer.id, tenant).await;
    sqlx::query("INSERT INTO meeting_resource_links (tenant_id,meeting_folder_id,resource_ref_uri,link_kind,created_at) VALUES ($1,$2,$3,'decision','2026-03-01T00:00:00Z')")
        .bind(tenant).bind(later_visible_id).bind(&resource_ref).execute(&state.db_pool).await.expect("seed second accessible association");
    let token = create_auth_token(&state, viewer.id, tenant);
    let api = app(state.clone());
    let uri = format!("/api/v1/decisions/{decision_id}/meetings?per_page=1");
    let (status, first) = request(&api, &token, "GET", &uri, None).await;
    assert_eq!(status, StatusCode::OK, "{first}");
    assert_eq!(first["items"].as_array().unwrap().len(), 1);
    assert_eq!(first["items"][0]["id"], visible_id.to_string());
    assert_eq!(first["next_page"], 2);
    assert!(!first.to_string().contains("Private candidate"));
    let (status, second) = request(&api, &token, "GET", &format!("{uri}&page=2"), None).await;
    assert_eq!(status, StatusCode::OK, "{second}");
    assert_eq!(second["items"].as_array().unwrap().len(), 1);
    assert_eq!(second["items"][0]["id"], later_visible_id.to_string());
    assert!(second.get("next_page").is_none_or(Value::is_null));
    let (status, all) = request(
        &api,
        &token,
        "GET",
        &format!("/api/v1/decisions/{decision_id}/meetings?per_page=100"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{all}");
    assert_eq!(all["items"].as_array().unwrap().len(), 2);
    assert!(
        all.get("next_page").is_none_or(Value::is_null),
        "inaccessible candidates never affect continuation metadata"
    );
    cleanup(&state.db_pool, tenant).await;
}

#[tokio::test]
#[ignore = "requires the guarded disposable PostgreSQL and object-store services"]
async fn inaccessible_foreign_non_decision_and_disabled_requests_never_create_links() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env().await;
    let tenant = create_test_tenant(&state.db_pool).await;
    let foreign_tenant = create_test_tenant(&state.db_pool).await;
    let user = create_test_user(&state, "meeting_decision_actor", tenant).await;
    let other = create_test_user(&state, "meeting_decision_private", tenant).await;
    let foreign = create_test_user(&state, "meeting_decision_foreign", foreign_tenant).await;
    let private_id = decision(&state, other.id, tenant).await;
    let foreign_id = decision(&state, foreign.id, foreign_tenant).await;
    let ordinary = state
        .file_service
        .upload_file(
            user.id,
            "ordinary.md".into(),
            None,
            Bytes::from_static(b"ordinary"),
            "text/markdown".into(),
            tenant,
        )
        .await
        .expect("ordinary Markdown file");
    let meeting_id = meeting(&state, user.id, tenant).await;
    let token = create_auth_token(&state, user.id, tenant);
    let api = app(state.clone());
    let uri = format!("/api/v1/meetings/{meeting_id}/decisions");
    let mut missing_body = None;
    for id in [Uuid::new_v4(), private_id, foreign_id, ordinary.id] {
        let (status, body) = request(
            &api,
            &token,
            "POST",
            &format!("{uri}/links"),
            Some(json!({"decision_id":id})),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
        if let Some(expected) = &missing_body {
            assert_eq!(&body, expected, "resource existence must not affect errors");
        } else {
            missing_body = Some(body);
        }
    }
    let (status, empty) = request(&api, &token, "GET", &uri, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(empty["total"], 0);
    sqlx::query("UPDATE users SET disabled_at=NOW() WHERE id=$1")
        .bind(user.id)
        .execute(&state.db_pool)
        .await
        .expect("disable actor");
    for (method, path, payload) in [
        ("GET", uri.clone(), None),
        ("POST", uri.clone(), Some(create_payload(Uuid::new_v4()))),
        (
            "GET",
            format!("/api/v1/decisions/{private_id}/meetings"),
            None,
        ),
    ] {
        let (status, _) = request(&api, &token, method, &path, payload).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
    cleanup(&state.db_pool, foreign_tenant).await;
    cleanup(&state.db_pool, tenant).await;
}

#[tokio::test]
#[ignore = "requires the guarded disposable PostgreSQL and object-store services"]
async fn meeting_view_cannot_mutate_and_revocation_removes_backlink_metadata() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env().await;
    let tenant = create_test_tenant(&state.db_pool).await;
    let owner = create_test_user(&state, "meeting_decision_meeting_owner", tenant).await;
    let viewer = create_test_user(&state, "meeting_decision_meeting_viewer", tenant).await;
    let meeting_id = meeting(&state, owner.id, tenant).await;
    let decision_id = decision(&state, viewer.id, tenant).await;
    direct_grant(&state.db_pool, decision_id, viewer.id, owner.id, tenant).await;
    sqlx::query(
        "INSERT INTO shares (id,folder_id,share_token,created_by,permissions,recipient_user_id,tenant_id)
         VALUES ($1,$2,NULL,$3,'View',$4,$5)",
    )
    .bind(Uuid::new_v4())
    .bind(meeting_id)
    .bind(owner.id)
    .bind(viewer.id)
    .bind(tenant)
    .execute(&state.db_pool)
    .await
    .expect("grant meeting View");
    let owner_token = create_auth_token(&state, owner.id, tenant);
    let viewer_token = create_auth_token(&state, viewer.id, tenant);
    let api = app(state.clone());
    let uri = format!("/api/v1/meetings/{meeting_id}/decisions");
    let (status, link) = request(
        &api,
        &owner_token,
        "POST",
        &format!("{uri}/links"),
        Some(json!({"decision_id":decision_id})),
    )
    .await;
    assert!(status.is_success(), "{link}");
    for (method, path, payload) in [
        ("POST", uri.clone(), Some(create_payload(Uuid::new_v4()))),
        (
            "POST",
            format!("{uri}/links"),
            Some(json!({"decision_id":decision_id})),
        ),
        (
            "DELETE",
            format!("{uri}/{}", link["id"].as_str().unwrap()),
            None,
        ),
    ] {
        let (status, body) = request(&api, &viewer_token, method, &path, payload).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    }
    let reverse_uri = format!("/api/v1/decisions/{decision_id}/meetings");
    let (status, visible) = request(&api, &viewer_token, "GET", &reverse_uri, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(visible["items"][0]["id"], meeting_id.to_string());
    sqlx::query("DELETE FROM shares WHERE folder_id=$1 AND recipient_user_id=$2")
        .bind(meeting_id)
        .bind(viewer.id)
        .execute(&state.db_pool)
        .await
        .expect("revoke meeting access");
    let (status, hidden) = request(&api, &viewer_token, "GET", &reverse_uri, None).await;
    assert_eq!(status, StatusCode::OK, "{hidden}");
    assert_eq!(hidden["items"], json!([]));
    assert!(!hidden.to_string().contains("Historic planning"));
    let (status, _) = request(&api, &viewer_token, "GET", &uri, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    cleanup(&state.db_pool, tenant).await;
}

#[tokio::test]
#[ignore = "requires the guarded disposable PostgreSQL and object-store services"]
async fn interrupted_finalization_recovers_same_objects_after_restart_without_deleting_edits() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env().await;
    let tenant = create_test_tenant(&state.db_pool).await;
    let owner = create_test_user(&state, "meeting_decision_recovery", tenant).await;
    let meeting_id = meeting(&state, owner.id, tenant).await;
    let token = create_auth_token(&state, owner.id, tenant);
    let api = app(state.clone());
    let uri = format!("/api/v1/meetings/{meeting_id}/decisions");
    let request_id = Uuid::new_v4();
    let payload = create_payload(request_id);
    sqlx::query(
        "CREATE OR REPLACE FUNCTION reject_test_meeting_decision_link() RETURNS trigger
         LANGUAGE plpgsql AS $$ BEGIN IF NEW.link_kind = 'decision' THEN
         RAISE EXCEPTION 'simulated finalization failure'; END IF; RETURN NEW; END $$",
    )
    .execute(&state.db_pool)
    .await
    .expect("failure function");
    sqlx::query(
        "CREATE TRIGGER reject_test_meeting_decision_link BEFORE INSERT ON meeting_resource_links
         FOR EACH ROW EXECUTE FUNCTION reject_test_meeting_decision_link()",
    )
    .execute(&state.db_pool)
    .await
    .expect("failure trigger");
    let (status, failure) = request(&api, &token, "POST", &uri, Some(payload.clone())).await;
    sqlx::query("DROP TRIGGER reject_test_meeting_decision_link ON meeting_resource_links")
        .execute(&state.db_pool)
        .await
        .expect("remove failure trigger");
    sqlx::query("DROP FUNCTION reject_test_meeting_decision_link()")
        .execute(&state.db_pool)
        .await
        .expect("remove failure function");
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{failure}");
    assert!(!failure.to_string().contains("simulated finalization"));
    let (id, sidecar_id, ready): (Uuid, Uuid, bool) = sqlx::query_as(
        "SELECT decision_file_id,sidecar_file_id,ready FROM meeting_decision_creations
         WHERE tenant_id=$1 AND actor_id=$2 AND request_id=$3",
    )
    .bind(tenant)
    .bind(owner.id)
    .bind(request_id)
    .fetch_one(&state.db_pool)
    .await
    .expect("durable pending creation");
    assert!(!ready);
    // Model a process interruption after the main file was written but before
    // the reserved sidecar was persisted. Editing metadata repairs that sidecar
    // and must use the command's reserved identity so the original retry can finish.
    state
        .file_service
        .delete_file(sidecar_id, owner.id)
        .await
        .expect("remove interrupted sidecar");
    state
        .metadata_store
        .permanently_delete_file(sidecar_id, owner.id)
        .await
        .expect("remove interrupted sidecar metadata row");
    let decision_uri = format!("/api/v1/decisions/{id}");
    let (status, repaired) = request(
        &api,
        &token,
        "PUT",
        &decision_uri,
        Some(json!({"status":"Accepted", "decision_date":"2026-10-22T09:00:00Z"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{repaired}");
    let primary = state
        .file_service
        .get_file(id, owner.id)
        .await
        .expect("pending Decision");
    let repaired_sidecars: Vec<_> = state
        .metadata_store
        .list_files(primary.parent_folder_id, owner.id, tenant)
        .await
        .expect("list repaired sidecar")
        .into_iter()
        .filter(|file| {
            file.name == format!("{}.rustshare.json", primary.name.trim_end_matches(".md"))
        })
        .collect();
    assert_eq!(repaired_sidecars.len(), 1, "repair creates one sidecar");
    assert_eq!(
        repaired_sidecars[0].id, sidecar_id,
        "repair uses reserved identity"
    );
    assert_eq!(repaired["metadata"]["status"], "accepted");
    assert_eq!(
        repaired["metadata"]["decision_date"],
        "2026-10-22T09:00:00Z"
    );
    state
        .file_service
        .edit_file(
            id,
            owner.id,
            Bytes::from_static(b"Edits after interrupted creation"),
            "overwrite",
            None,
        )
        .await
        .expect("edit surviving pending Decision");
    let (status, renamed) = request(
        &api,
        &token,
        "POST",
        &format!("{decision_uri}/rename"),
        Some(json!({"title":"Renamed after interrupted creation"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{renamed}");
    let destination = state
        .folder_service
        .create_folder("Recovered Decisions".into(), None, owner.id, tenant)
        .await
        .expect("recovery destination");
    let (status, moved) = request(
        &api,
        &token,
        "POST",
        &format!("{decision_uri}/move"),
        Some(json!({"target_folder_id":destination.id})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{moved}");
    let original = state
        .file_service
        .get_file(id, owner.id)
        .await
        .expect("edited pending object");
    let sidecar = state
        .file_service
        .get_file(sidecar_id, owner.id)
        .await
        .expect("pending sidecar remains valid after move");
    assert_eq!(original.parent_folder_id, Some(destination.id));
    assert_eq!(sidecar.parent_folder_id, Some(destination.id));
    let original_bytes = state
        .object_store
        .get(&original.storage_key())
        .await
        .expect("pending content");
    let sidecar_bytes = state
        .object_store
        .get(&sidecar.storage_key())
        .await
        .expect("pending sidecar");
    let restarted = app(setup_test_env().await);
    let (status, recovered) =
        request(&restarted, &token, "POST", &uri, Some(payload.clone())).await;
    assert!(status.is_success(), "{recovered}");
    assert_eq!(recovered["decision"]["id"], id.to_string());
    assert_eq!(
        recovered["decision"]["title"],
        "Renamed after interrupted creation"
    );
    assert_eq!(
        state
            .object_store
            .get(&original.storage_key())
            .await
            .unwrap(),
        original_bytes
    );
    assert_eq!(
        state
            .object_store
            .get(&sidecar.storage_key())
            .await
            .unwrap(),
        sidecar_bytes
    );
    let (status, page) = request(&restarted, &token, "GET", &uri, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page["total"], 1, "retry finalizes exactly once");
    let (status, retry) = request(&restarted, &token, "POST", &uri, Some(payload)).await;
    assert!(status.is_success(), "{retry}");
    assert_eq!(retry["id"], recovered["id"]);
    assert_eq!(retry["decision"]["status"], "accepted");
    assert_eq!(retry["decision"]["decision_date"], "2026-10-22T09:00:00Z");
    cleanup(&state.db_pool, tenant).await;
}
