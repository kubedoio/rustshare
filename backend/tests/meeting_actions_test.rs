//! Guarded PostgreSQL/S3 HTTP acceptance for meeting-owned action items.

use std::sync::Arc;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
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
    let registry = ApplicationRegistry::first_party().expect("applications");
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
        .expect("Files owner");
    state.source_authorizer = Arc::new(SourceAuthorizer::new(owners));
    state.unified_search_service = Arc::new(
        rustshare_server::services::unified_search::UnifiedSearchService::new(
            state.source_authorizer.clone(),
            state.metadata_store.clone(),
            state.ai_service.clone(),
            state.memory_catalog_store.clone(),
        ),
    );
    rustshare_server::routes::meeting_routes()
        .merge(rustshare_server::routes::calendar_routes())
        .merge(rustshare_server::routes::decision_routes())
        .merge(rustshare_server::routes::notification_routes())
        .merge(rustshare_server::routes::search_routes())
        .with_state(state)
}

async fn request(
    api: &axum::Router<()>,
    token: &str,
    method: &str,
    uri: &str,
    payload: Option<Value>,
) -> (StatusCode, Value) {
    let response = api
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("Authorization", format!("Bearer {token}"))
                .header("Content-Type", "application/json")
                .body(payload.map_or_else(Body::empty, |v| Body::from(v.to_string())))
                .expect("request"),
        )
        .await
        .expect("response");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
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
            "Historic private planning".into(),
            "Planning".into(),
            chrono::Utc::now(),
            "Private agenda".into(),
        )
        .await
        .expect("meeting")
        .id
}

fn payload(request_id: Uuid) -> Value {
    json!({"request_id":request_id,"title":"Prepare the release","description":"Private action details",
        "assignee_id":null,"due_date":"2026-10-25","related_decision_id":null})
}

fn edit(action: &Value) -> Value {
    json!({"request_id":Uuid::new_v4(),"expected_version":action["version"],"title":action["title"],
        "description":action["description"],"assignee_id":action["assignee_id"],"due_date":action["due_date"],"state":action["state"]})
}

async fn create(api: &axum::Router<()>, token: &str, meeting: Uuid, body: Value) -> Value {
    let (status, action) = request(
        api,
        token,
        "POST",
        &format!("/api/v1/meetings/{meeting}/actions"),
        Some(body),
    )
    .await;
    assert!(
        status == StatusCode::CREATED || status == StatusCode::OK,
        "{status}: {action}"
    );
    action
}

async fn grant(
    pool: &PgPool,
    folder: Uuid,
    owner: Uuid,
    recipient: Uuid,
    tenant: Uuid,
    permission: &str,
) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO shares (id,folder_id,share_token,created_by,permissions,recipient_user_id,tenant_id) VALUES ($1,$2,NULL,$3,$4,$5,$6)")
        .bind(id).bind(folder).bind(owner).bind(permission).bind(recipient).bind(tenant).execute(pool).await.expect("share");
    id
}

async fn event_count(pool: &PgPool, action: Uuid) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM events WHERE payload->>'action_id'=$1::text AND event_type LIKE '%MeetingActionModified%'")
        .bind(action.to_string()).fetch_one(pool).await.expect("events")
}

async fn cleanup(pool: &PgPool, tenant: Uuid) {
    for table in [
        "meeting_action_commands",
        "meeting_actions",
        "shares",
        "user_groups",
        "file_versions",
    ] {
        sqlx::query(&format!("DELETE FROM {table} WHERE tenant_id=$1"))
            .bind(tenant)
            .execute(pool)
            .await
            .expect("cleanup");
    }
    cleanup_tenant(pool, tenant).await;
}

#[tokio::test]
#[ignore = "requires guarded disposable PostgreSQL and object storage"]
async fn search_reopens_a_standalone_historical_meeting() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env().await;
    let tenant = create_test_tenant(&state.db_pool).await;
    let owner = create_test_user(&state, "standalone_meeting_search", tenant).await;
    let token = create_auth_token(&state, owner.id, tenant);
    let meeting_id = meeting(&state, owner.id, tenant).await;
    let mut attachments = Vec::new();
    for name in ["Historic agenda.txt", "Historic report.txt"] {
        attachments.push(
            state
                .file_service
                .upload_file(
                    owner.id,
                    name.to_string(),
                    Some(meeting_id),
                    bytes::Bytes::from_static(b"Authorized meeting attachment"),
                    "text/plain".to_string(),
                    tenant,
                )
                .await
                .expect("upload meeting attachment")
                .id,
        );
    }
    let candidates = state
        .metadata_store
        .search_files_by_name_path(tenant, "Historic", 10)
        .await
        .expect("meeting file candidate lookup");
    assert!(
        !candidates.is_empty(),
        "meeting file was not a search candidate"
    );
    assert!(
        candidates
            .iter()
            .any(|file| !rustshare_core::services::is_hidden_file_for_path(&file.name, &file.path)),
        "meeting file path was filtered: {:?}",
        candidates.iter().map(|file| &file.path).collect::<Vec<_>>()
    );
    let api = app(state.clone());

    let (status, search) = request(
        &api,
        &token,
        "POST",
        "/api/v1/search",
        Some(json!({"query":"Historic","sources":["files"],"limit":10})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{search}");
    assert!(
        search["results"].as_array().is_some_and(|items| {
            items.iter().any(|item| {
                item["title"] == "Historic private planning"
                    && item["source_type"] == "meeting"
                    && item["provenance"]["meeting_id"] == meeting_id.to_string()
                    && item["provenance"]["calendar_event_id"].is_null()
            })
        }),
        "{search}"
    );
    for file_id in attachments {
        assert!(
            search["results"].as_array().is_some_and(|items| {
                items.iter().any(|item| {
                    item["source_type"] == "file"
                        && item["provenance"]["file_id"] == file_id.to_string()
                        && item["provenance"]["meeting_id"] == meeting_id.to_string()
                })
            }),
            "meeting attachment was collapsed into a meeting result: {search}"
        );
    }

    cleanup(&state.db_pool, tenant).await;
}

#[tokio::test]
#[ignore = "requires guarded disposable PostgreSQL and object storage"]
async fn event_meeting_decision_action_completion_and_historical_restart_journey() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env().await;
    let tenant = create_test_tenant(&state.db_pool).await;
    let owner = create_test_user(&state, "actions_journey", tenant).await;
    let responsible = create_test_user(&state, "actions_responsible", tenant).await;
    configure_calendar(&state, tenant, owner.id, true).await;
    let token = create_auth_token(&state, owner.id, tenant);
    let api = app(state.clone());
    let (status, event) = request(
        &api,
        &token,
        "POST",
        "/api/v1/calendar/events",
        Some(json!({
        "title":"Past planning","description":"Discuss release","starts_at":"2026-03-28T10:00:00Z",
        "ends_at":"2026-03-28T11:00:00Z","timezone":"Europe/Berlin"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{event}");
    let event_id = event["id"].as_str().expect("event ID");
    let event_meeting = format!("/api/v1/calendar/events/{event_id}/meeting");
    let (status, note) = request(&api, &token, "POST", &event_meeting, None).await;
    assert_eq!(status, StatusCode::OK, "{note}");
    let meeting_id = Uuid::parse_str(note["id"].as_str().expect("meeting ID")).expect("UUID");
    let (status,decision) = request(&api,&token,"POST",&format!("/api/v1/meetings/{meeting_id}/decisions"),
        Some(json!({"request_id":Uuid::new_v4(),"title":"Ship release","category":"Planning","content":"Approved"}))).await;
    assert!(status.is_success(), "{status}: {decision}");
    let decision_id = decision["decision"]["id"].as_str().expect("Decision ID");
    let mut body = payload(Uuid::new_v4());
    body["assignee_id"] = json!(responsible.id);
    body["related_decision_id"] = json!(decision_id);
    let action = create(&api, &token, meeting_id, body).await;
    assert_eq!(action["calendar_event_id"], event_id);
    assert_eq!(action["assignee_id"], responsible.id.to_string());
    assert_eq!(
        action["due_date"], "2026-10-25",
        "date-only due date survives DST boundaries"
    );
    assert_eq!(action["created_by"], owner.id.to_string());
    let (status, search) = request(
        &api,
        &token,
        "POST",
        "/api/v1/search",
        Some(json!({"query":"Prepare the release","sources":["files"],"limit":10})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{search}");
    assert!(search["results"]
        .as_array()
        .is_some_and(|items| items.iter().any(|item| {
            item["provenance"]["meeting_id"] == meeting_id.to_string()
                && item["provenance"]["action_id"] == action["id"]
        })));
    let denied_user = create_test_user(&state, "actions_search_denied", tenant).await;
    let denied_token = create_auth_token(&state, denied_user.id, tenant);
    let (status, denied_search) = request(
        &api,
        &denied_token,
        "POST",
        "/api/v1/search",
        Some(json!({"query":"Prepare the release","sources":["files"],"limit":10})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{denied_search}");
    assert_eq!(denied_search["total"], 0);
    let direct = grant(
        &state.db_pool,
        meeting_id,
        owner.id,
        denied_user.id,
        tenant,
        "View",
    )
    .await;
    let (status, granted_search) = request(
        &api,
        &denied_token,
        "POST",
        "/api/v1/search",
        Some(json!({"query":"Prepare the release","sources":["files"],"limit":10})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{granted_search}");
    assert!(granted_search["results"].as_array().is_some_and(|items| {
        items.iter().any(|item| {
            item["provenance"]["meeting_id"] == meeting_id.to_string()
                && item["provenance"]["action_id"] == action["id"]
        })
    }));
    sqlx::query("DELETE FROM shares WHERE id=$1")
        .bind(direct)
        .execute(&state.db_pool)
        .await
        .expect("revoke final search grant");
    let (status, revoked_search) = request(
        &api,
        &denied_token,
        "POST",
        "/api/v1/search",
        Some(json!({"query":"Prepare the release","sources":["files"],"limit":10})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{revoked_search}");
    assert_eq!(revoked_search["total"], 0);
    let uri = format!(
        "/api/v1/meetings/{meeting_id}/actions/{}",
        action["id"].as_str().expect("action ID")
    );
    let mut completion = edit(&action);
    completion["state"] = json!("completed");
    let (status, completed) = request(&api, &token, "PUT", &uri, Some(completion)).await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    assert_eq!(completed["state"], "completed");
    assert_eq!(completed["completed_by"], owner.id.to_string());
    assert!(completed["completed_at"].is_string());
    let restarted = app(setup_test_env().await);
    let (status, reopened_meeting) =
        request(&restarted, &token, "POST", &event_meeting, None).await;
    assert_eq!(status, StatusCode::OK, "{reopened_meeting}");
    assert_eq!(reopened_meeting["id"], note["id"]);
    let (status, page) = request(
        &restarted,
        &token,
        "GET",
        &format!("/api/v1/meetings/{meeting_id}/actions"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert_eq!(page["total"], 1);
    assert_eq!(page["items"][0]["state"], "completed");
    assert_eq!(page["items"][0]["related_decision"]["id"], decision_id);
    let mut reopen = edit(&completed);
    reopen["state"] = json!("open");
    let (status, reopened) = request(&restarted, &token, "PUT", &uri, Some(reopen)).await;
    assert_eq!(status, StatusCode::OK, "{reopened}");
    assert_eq!(reopened["state"], "open");
    assert!(reopened["completed_at"].is_null());
    let id = Uuid::parse_str(action["id"].as_str().expect("action")).expect("UUID");
    assert_eq!(
        event_count(&state.db_pool, id).await,
        3,
        "one durable event per real mutation"
    );
    cleanup(&state.db_pool, tenant).await;
}

#[tokio::test]
#[ignore = "requires guarded disposable PostgreSQL and object storage"]
async fn concurrent_create_and_retry_after_restart_produce_one_action_and_event() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env().await;
    let tenant = create_test_tenant(&state.db_pool).await;
    let owner = create_test_user(&state, "actions_create_race", tenant).await;
    let id = meeting(&state, owner.id, tenant).await;
    let token = create_auth_token(&state, owner.id, tenant);
    let api = app(state.clone());
    let other = app(setup_test_env().await);
    let uri = format!("/api/v1/meetings/{id}/actions");
    let body = payload(Uuid::new_v4());
    let (a, b) = tokio::join!(
        request(&api, &token, "POST", &uri, Some(body.clone())),
        request(&other, &token, "POST", &uri, Some(body.clone()))
    );
    assert!(a.0.is_success(), "{:?}", a);
    assert!(b.0.is_success(), "{:?}", b);
    assert_eq!(a.1["id"], b.1["id"]);
    let (status, retry) = request(&other, &token, "POST", &uri, Some(body.clone())).await;
    assert!(status.is_success(), "{retry}");
    assert_eq!(retry["id"], a.1["id"]);
    let mut changed = body.clone();
    changed["title"] = json!("Conflicting payload");
    assert_eq!(
        request(&api, &token, "POST", &uri, Some(changed)).await.0,
        StatusCode::CONFLICT
    );
    let action_id = Uuid::parse_str(a.1["id"].as_str().expect("id")).expect("UUID");
    assert_eq!(event_count(&state.db_pool, action_id).await, 1);
    let (_, page) = request(&api, &token, "GET", &uri, None).await;
    assert_eq!(page["total"], 1);
    cleanup(&state.db_pool, tenant).await;
}

#[tokio::test]
#[ignore = "requires guarded disposable PostgreSQL and object storage"]
async fn concurrent_updates_compare_versions_and_response_loss_retries_do_not_reemit() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env().await;
    let tenant = create_test_tenant(&state.db_pool).await;
    let owner = create_test_user(&state, "actions_update_race", tenant).await;
    let meeting_id = meeting(&state, owner.id, tenant).await;
    let token = create_auth_token(&state, owner.id, tenant);
    let api = app(state.clone());
    let other = app(setup_test_env().await);
    let creation = payload(Uuid::new_v4());
    let action = create(&api, &token, meeting_id, creation.clone()).await;
    let uri = format!(
        "/api/v1/meetings/{meeting_id}/actions/{}",
        action["id"].as_str().expect("id")
    );
    let mut first = edit(&action);
    first["title"] = json!("First update");
    let mut second = edit(&action);
    second["title"] = json!("Second update");
    let (a, b) = tokio::join!(
        request(&api, &token, "PUT", &uri, Some(first.clone())),
        request(&other, &token, "PUT", &uri, Some(second.clone()))
    );
    assert!(
        (a.0 == StatusCode::OK && b.0 == StatusCode::CONFLICT)
            || (b.0 == StatusCode::OK && a.0 == StatusCode::CONFLICT),
        "{a:?} {b:?}"
    );
    let winning = if a.0 == StatusCode::OK {
        first.clone()
    } else {
        second
    };
    let winner = if a.0 == StatusCode::OK { a.1 } else { b.1 };
    let (status, retry) = request(&other, &token, "PUT", &uri, Some(winning.clone())).await;
    assert_eq!(status, StatusCode::OK, "{retry}");
    assert_eq!(retry["version"], winner["version"]);
    let mut changed = winning.clone();
    changed["description"] = json!("Changed retry");
    assert_eq!(
        request(&api, &token, "PUT", &uri, Some(changed)).await.0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        request(&api, &token, "PUT", &uri, Some(first)).await.0,
        if winner["title"] == "First update" {
            StatusCode::OK
        } else {
            StatusCode::CONFLICT
        }
    );
    let action_id = Uuid::parse_str(action["id"].as_str().expect("id")).expect("UUID");
    assert_eq!(event_count(&state.db_pool, action_id).await, 2);
    let mut completion = edit(&winner);
    completion["state"] = json!("completed");
    let (status, latest) = request(&api, &token, "PUT", &uri, Some(completion)).await;
    assert_eq!(status, StatusCode::OK, "{latest}");
    assert_eq!(latest["version"], 3);
    let (status, old_update_retry) = request(&other, &token, "PUT", &uri, Some(winning)).await;
    assert_eq!(status, StatusCode::OK, "{old_update_retry}");
    assert_eq!(
        old_update_retry, latest,
        "old successful mutation retry returns latest authorized state"
    );
    let old_create_retry = create(&other, &token, meeting_id, creation).await;
    assert_eq!(
        old_create_retry, latest,
        "old creation retry returns latest state without undoing completion"
    );
    assert_eq!(event_count(&state.db_pool, action_id).await, 3);
    cleanup(&state.db_pool, tenant).await;
}

#[tokio::test]
#[ignore = "requires guarded disposable PostgreSQL and object storage"]
async fn assignment_is_validated_and_does_not_grant_private_meeting_access() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env().await;
    let tenant = create_test_tenant(&state.db_pool).await;
    let foreign = create_test_tenant(&state.db_pool).await;
    let owner = create_test_user(&state, "actions_assign_owner", tenant).await;
    let user = create_test_user(&state, "actions_assignee", tenant).await;
    let outsider = create_test_user(&state, "actions_foreign", foreign).await;
    let disabled = create_test_user(&state, "actions_disabled", tenant).await;
    sqlx::query("UPDATE users SET disabled_at=now() WHERE id=$1")
        .bind(disabled.id)
        .execute(&state.db_pool)
        .await
        .expect("disable");
    let meeting_id = meeting(&state, owner.id, tenant).await;
    let token = create_auth_token(&state, owner.id, tenant);
    let api = app(state.clone());
    let uri = format!("/api/v1/meetings/{meeting_id}/actions");
    let mut denials = Vec::new();
    for assignee in [outsider.id, disabled.id, Uuid::new_v4()] {
        let mut body = payload(Uuid::new_v4());
        body["assignee_id"] = json!(assignee);
        let response = request(&api, &token, "POST", &uri, Some(body)).await;
        assert!(!response.0.is_success(), "{response:?}");
        denials.push(response);
    }
    assert_eq!(denials[0], denials[1]);
    assert_eq!(denials[1], denials[2]);
    let mut body = payload(Uuid::new_v4());
    body["assignee_id"] = json!(user.id);
    let action = create(&api, &token, meeting_id, body).await;
    let user_token = create_auth_token(&state, user.id, tenant);
    assert_eq!(
        request(&api, &user_token, "GET", &uri, None).await.0,
        StatusCode::NOT_FOUND
    );
    let (_, roster) = request(
        &api,
        &token,
        "GET",
        &format!("/api/v1/meetings/{meeting_id}/action-assignees?per_page=1000"),
        None,
    )
    .await;
    let rendered = roster.to_string();
    assert!(!rendered.contains(&outsider.id.to_string()));
    assert!(!rendered.contains(&disabled.id.to_string()));
    assert!(rendered.contains(&user.id.to_string()));
    sqlx::query("UPDATE users SET disabled_at=now() WHERE id=$1")
        .bind(user.id)
        .execute(&state.db_pool)
        .await
        .expect("disable responsible user");
    let mut completion = edit(&action);
    completion["state"] = json!("completed");
    let (status, updated) = request(
        &api,
        &token,
        "PUT",
        &format!("{uri}/{}", action["id"].as_str().expect("id")),
        Some(completion),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "historical disabled attribution must not block completion: {updated}"
    );
    assert_eq!(updated["assignee_id"], user.id.to_string());
    cleanup(&state.db_pool, foreign).await;
    cleanup(&state.db_pool, tenant).await;
}

#[tokio::test]
#[ignore = "requires guarded disposable PostgreSQL and object storage"]
async fn private_foreign_missing_meetings_and_nested_action_ids_are_concealed() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env().await;
    let tenant = create_test_tenant(&state.db_pool).await;
    let foreign = create_test_tenant(&state.db_pool).await;
    let owner = create_test_user(&state, "actions_private_owner", tenant).await;
    let other = create_test_user(&state, "actions_other", tenant).await;
    let outsider = create_test_user(&state, "actions_outsider", foreign).await;
    let private = meeting(&state, owner.id, tenant).await;
    let own = meeting(&state, other.id, tenant).await;
    let cross = meeting(&state, outsider.id, foreign).await;
    let token = create_auth_token(&state, owner.id, tenant);
    let api = app(state.clone());
    let action = create(&api, &token, private, payload(Uuid::new_v4())).await;
    let other_token = create_auth_token(&state, other.id, tenant);
    let mut denials = Vec::new();
    for id in [private, cross, Uuid::new_v4()] {
        let uri = format!("/api/v1/meetings/{id}/actions");
        let denied = request(&api, &other_token, "GET", &uri, None).await;
        assert_eq!(denied.0, StatusCode::NOT_FOUND);
        denials.push(denied);
        assert_eq!(
            request(
                &api,
                &other_token,
                "POST",
                &uri,
                Some(payload(Uuid::new_v4()))
            )
            .await
            .0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            request(
                &api,
                &other_token,
                "GET",
                &format!("/api/v1/meetings/{id}/action-assignees"),
                None
            )
            .await
            .0,
            StatusCode::NOT_FOUND
        );
    }
    assert_eq!(denials[0], denials[1]);
    assert_eq!(denials[1], denials[2]);
    let nested = format!(
        "/api/v1/meetings/{own}/actions/{}",
        action["id"].as_str().expect("id")
    );
    assert_eq!(
        request(&api, &other_token, "GET", &nested, None).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        request(&api, &other_token, "PUT", &nested, Some(edit(&action)))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    sqlx::query("UPDATE users SET is_admin=true WHERE id=$1")
        .bind(other.id)
        .execute(&state.db_pool)
        .await
        .expect("operator flag");
    assert_eq!(
        request(
            &api,
            &other_token,
            "GET",
            &format!("/api/v1/meetings/{private}/actions"),
            None
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    cleanup(&state.db_pool, foreign).await;
    cleanup(&state.db_pool, tenant).await;
}

#[tokio::test]
#[ignore = "requires guarded disposable PostgreSQL and object storage"]
async fn direct_and_group_overlap_revocation_and_disabled_caller_are_live() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env().await;
    let tenant = create_test_tenant(&state.db_pool).await;
    let owner = create_test_user(&state, "actions_grant_owner", tenant).await;
    let viewer = create_test_user(&state, "actions_grant_viewer", tenant).await;
    let meeting_id = meeting(&state, owner.id, tenant).await;
    let token = create_auth_token(&state, owner.id, tenant);
    let viewer_token = create_auth_token(&state, viewer.id, tenant);
    let api = app(state.clone());
    let action = create(&api, &token, meeting_id, payload(Uuid::new_v4())).await;
    let uri = format!("/api/v1/meetings/{meeting_id}/actions");
    let action_uri = format!("{uri}/{}", action["id"].as_str().expect("id"));
    let direct = grant(
        &state.db_pool,
        meeting_id,
        owner.id,
        viewer.id,
        tenant,
        "View",
    )
    .await;
    assert_eq!(
        request(&api, &viewer_token, "GET", &uri, None).await.0,
        StatusCode::OK
    );
    let assignees_uri = format!("/api/v1/meetings/{meeting_id}/action-assignees");
    assert_eq!(
        request(&api, &viewer_token, "GET", &assignees_uri, None)
            .await
            .0,
        StatusCode::NOT_FOUND,
        "view access must not disclose the tenant user directory"
    );
    assert!(
        !request(&api, &viewer_token, "PUT", &action_uri, Some(edit(&action)))
            .await
            .0
            .is_success()
    );
    let group = Uuid::new_v4();
    sqlx::query("INSERT INTO user_groups(id,name,tenant_id,created_by) VALUES($1,$2,$3,$4)")
        .bind(group)
        .bind(format!("actions-{group}"))
        .bind(tenant)
        .bind(owner.id)
        .execute(&state.db_pool)
        .await
        .expect("group");
    sqlx::query("INSERT INTO group_members(group_id,user_id,added_by) VALUES($1,$2,$3)")
        .bind(group)
        .bind(viewer.id)
        .bind(owner.id)
        .execute(&state.db_pool)
        .await
        .expect("member");
    sqlx::query("INSERT INTO shares(id,folder_id,share_token,created_by,permissions,recipient_group_id,tenant_id) VALUES($1,$2,NULL,$3,'Edit',$4,$5)")
        .bind(Uuid::new_v4()).bind(meeting_id).bind(owner.id).bind(group).bind(tenant).execute(&state.db_pool).await.expect("group grant");
    let mut change = edit(&action);
    change["title"] = json!("Group edit");
    let (status, updated) = request(&api, &viewer_token, "PUT", &action_uri, Some(change)).await;
    assert_eq!(status, StatusCode::OK, "{updated}");
    assert_eq!(
        request(&api, &viewer_token, "GET", &assignees_uri, None)
            .await
            .0,
        StatusCode::OK,
        "edit access can load assignee choices"
    );
    sqlx::query("DELETE FROM shares WHERE id=$1")
        .bind(direct)
        .execute(&state.db_pool)
        .await
        .expect("revoke direct");
    assert_eq!(
        request(&api, &viewer_token, "GET", &uri, None).await.0,
        StatusCode::OK,
        "group remains"
    );
    sqlx::query("DELETE FROM group_members WHERE group_id=$1 AND user_id=$2")
        .bind(group)
        .bind(viewer.id)
        .execute(&state.db_pool)
        .await
        .expect("remove member");
    assert_eq!(
        request(&api, &viewer_token, "GET", &uri, None).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        request(&api, &viewer_token, "GET", &assignees_uri, None)
            .await
            .0,
        StatusCode::NOT_FOUND,
        "removing the group grant must immediately hide assignee choices"
    );
    assert_eq!(
        request(
            &api,
            &viewer_token,
            "PUT",
            &action_uri,
            Some(edit(&updated))
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    let (status, activity) = request(
        &api,
        &viewer_token,
        "GET",
        "/api/v1/activity?per_page=100",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{activity}");
    assert!(
        !activity.to_string().contains(&meeting_id.to_string()),
        "revoked folder authority must hide action activity: {activity}"
    );
    grant(
        &state.db_pool,
        meeting_id,
        owner.id,
        viewer.id,
        tenant,
        "Edit",
    )
    .await;
    sqlx::query("UPDATE users SET disabled_at=now() WHERE id=$1")
        .bind(viewer.id)
        .execute(&state.db_pool)
        .await
        .expect("disable caller");
    assert_eq!(
        request(&api, &viewer_token, "GET", &uri, None).await.0,
        StatusCode::UNAUTHORIZED
    );
    cleanup(&state.db_pool, tenant).await;
}

#[tokio::test]
#[ignore = "requires guarded disposable PostgreSQL and object storage"]
async fn pagination_is_bounded_deterministic_and_old_meetings_have_empty_actions() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env().await;
    let tenant = create_test_tenant(&state.db_pool).await;
    let owner = create_test_user(&state, "actions_pages", tenant).await;
    let id = meeting(&state, owner.id, tenant).await;
    let token = create_auth_token(&state, owner.id, tenant);
    let api = app(state.clone());
    let uri = format!("/api/v1/meetings/{id}/actions");
    let (status, empty) = request(&api, &token, "GET", &uri, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(empty["items"], json!([]));
    let mut ids = Vec::new();
    for _ in 0..3 {
        ids.push(create(&api, &token, id, payload(Uuid::new_v4())).await["id"].clone());
    }
    let (_, first) = request(
        &api,
        &token,
        "GET",
        &format!("{uri}?per_page=2&page=1"),
        None,
    )
    .await;
    let (_, second) = request(
        &api,
        &token,
        "GET",
        &format!("{uri}?per_page=2&page=2"),
        None,
    )
    .await;
    assert_eq!(first["total"], 3);
    assert_eq!(first["items"][0]["id"], ids[0]);
    assert_eq!(first["items"][1]["id"], ids[1]);
    assert_eq!(second["items"][0]["id"], ids[2]);
    let (_, bounded) = request(&api, &token, "GET", &format!("{uri}?per_page=1000"), None).await;
    assert_eq!(bounded["per_page"], 100);
    cleanup(&state.db_pool, tenant).await;
}

#[tokio::test]
#[ignore = "requires guarded disposable PostgreSQL and object storage"]
async fn related_decision_requires_its_own_grant_and_preserves_hidden_provenance() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env().await;
    let tenant = create_test_tenant(&state.db_pool).await;
    let foreign = create_test_tenant(&state.db_pool).await;
    let owner = create_test_user(&state, "actions_decision_owner", tenant).await;
    let author = create_test_user(&state, "actions_decision_author", tenant).await;
    let outsider = create_test_user(&state, "actions_decision_foreign", foreign).await;
    let meeting_id = meeting(&state, owner.id, tenant).await;
    let decision = state
        .decision_service
        .create_decision(
            author.id,
            tenant,
            "Protected decision title".into(),
            "Planning".into(),
            "Protected decision body".into(),
        )
        .await
        .expect("private Decision");
    let foreign_decision = state
        .decision_service
        .create_decision(
            outsider.id,
            foreign,
            "Foreign title".into(),
            "Planning".into(),
            "Foreign body".into(),
        )
        .await
        .expect("foreign Decision");
    let token = create_auth_token(&state, owner.id, tenant);
    let api = app(state.clone());
    let uri = format!("/api/v1/meetings/{meeting_id}/actions");
    let mut denied = Vec::new();
    for id in [decision.id, foreign_decision.id, Uuid::new_v4()] {
        let mut body = payload(Uuid::new_v4());
        body["related_decision_id"] = json!(id);
        let response = request(&api, &token, "POST", &uri, Some(body)).await;
        assert_eq!(response.0, StatusCode::NOT_FOUND, "{response:?}");
        denied.push(response);
    }
    assert_eq!(denied[0], denied[1]);
    assert_eq!(denied[1], denied[2]);
    let share_id = Uuid::new_v4();
    sqlx::query("INSERT INTO shares (id,file_id,share_token,created_by,permissions,recipient_user_id,tenant_id) VALUES ($1,$2,NULL,$3,'View',$4,$5)")
        .bind(share_id).bind(decision.id).bind(author.id).bind(owner.id).bind(tenant).execute(&state.db_pool).await.expect("Decision direct grant");
    let mut body = payload(Uuid::new_v4());
    body["related_decision_id"] = json!(decision.id);
    let action = create(&api, &token, meeting_id, body).await;
    assert_eq!(action["related_decision"]["id"], decision.id.to_string());
    assert_eq!(
        action["related_decision"]["title"],
        "Protected decision title"
    );
    let action_id = Uuid::parse_str(action["id"].as_str().expect("action ID")).expect("UUID");
    let action_uri = format!("{uri}/{action_id}");
    sqlx::query("DELETE FROM shares WHERE id=$1")
        .bind(share_id)
        .execute(&state.db_pool)
        .await
        .expect("revoke Decision grant");
    let (status, hidden) = request(&api, &token, "GET", &action_uri, None).await;
    assert_eq!(status, StatusCode::OK, "{hidden}");
    assert_eq!(hidden["related_decision_available"], false);
    assert!(hidden["related_decision"].is_null());
    let rendered = hidden.to_string();
    assert!(!rendered.contains(&decision.id.to_string()));
    assert!(!rendered.contains("Protected decision"));
    assert!(!rendered.contains("open_url"));
    let mut complete = edit(&hidden);
    complete["state"] = json!("completed");
    let (status, completed) = request(&api, &token, "PUT", &action_uri, Some(complete)).await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    let stored: Option<Uuid> = sqlx::query_scalar(
        "SELECT related_decision_id FROM meeting_actions WHERE tenant_id=$1 AND id=$2",
    )
    .bind(tenant)
    .bind(action_id)
    .fetch_one(&state.db_pool)
    .await
    .expect("retained provenance");
    assert_eq!(
        stored,
        Some(decision.id),
        "omitted hidden reference survives unrelated edits"
    );
    state
        .decision_service
        .delete_decision(decision.id, author.id, tenant)
        .await
        .expect("delete Decision");
    let (status, after_delete) = request(&api, &token, "GET", &action_uri, None).await;
    assert_eq!(status, StatusCode::OK, "{after_delete}");
    assert_eq!(after_delete["state"], "completed");
    assert!(after_delete["related_decision"].is_null());
    let mut clear = edit(&after_delete);
    clear["related_decision_id"] = Value::Null;
    let (status, cleared) = request(&api, &token, "PUT", &action_uri, Some(clear)).await;
    assert_eq!(status, StatusCode::OK, "{cleared}");
    let stored: Option<Uuid> = sqlx::query_scalar(
        "SELECT related_decision_id FROM meeting_actions WHERE tenant_id=$1 AND id=$2",
    )
    .bind(tenant)
    .bind(action_id)
    .fetch_one(&state.db_pool)
    .await
    .expect("cleared provenance");
    assert_eq!(stored, None, "explicit null clears the relation");
    cleanup(&state.db_pool, foreign).await;
    cleanup(&state.db_pool, tenant).await;
}

async fn action_event_failure_trigger(pool: &PgPool, meeting_id: Uuid, install: bool) {
    let name = format!("reject_action_event_{}", meeting_id.simple());
    if install {
        sqlx::query(&format!("CREATE FUNCTION {name}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'sensitive PostgreSQL storage failure'; RETURN NEW; END $$"))
            .execute(pool).await.expect("failure function");
        sqlx::query(&format!("CREATE TRIGGER {name} BEFORE INSERT ON events FOR EACH ROW WHEN (NEW.aggregate_id='{meeting_id}'::uuid AND NEW.event_type LIKE '%MeetingActionModified%') EXECUTE FUNCTION {name}()"))
            .execute(pool).await.expect("scoped event failure trigger");
    } else {
        sqlx::query(&format!("DROP TRIGGER {name} ON events"))
            .execute(pool)
            .await
            .expect("remove scoped trigger");
        sqlx::query(&format!("DROP FUNCTION {name}()"))
            .execute(pool)
            .await
            .expect("remove failure function");
    }
}

#[tokio::test]
#[ignore = "requires guarded disposable PostgreSQL and object storage"]
async fn action_and_command_receipts_roll_back_with_events_then_retry_after_restart() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env().await;
    let tenant = create_test_tenant(&state.db_pool).await;
    let owner = create_test_user(&state, "actions_atomic_owner", tenant).await;
    let meeting_id = meeting(&state, owner.id, tenant).await;
    let token = create_auth_token(&state, owner.id, tenant);
    let api = app(state.clone());
    let uri = format!("/api/v1/meetings/{meeting_id}/actions");
    let body = payload(Uuid::new_v4());
    action_event_failure_trigger(&state.db_pool, meeting_id, true).await;
    let failed = request(&api, &token, "POST", &uri, Some(body.clone())).await;
    action_event_failure_trigger(&state.db_pool, meeting_id, false).await;
    assert_eq!(failed.0, StatusCode::INTERNAL_SERVER_ERROR, "{failed:?}");
    assert!(!failed.1.to_string().contains("sensitive"));
    assert!(!failed.1.to_string().contains("PostgreSQL"));
    let actions: i64 =
        sqlx::query_scalar("SELECT count(*) FROM meeting_actions WHERE tenant_id=$1")
            .bind(tenant)
            .fetch_one(&state.db_pool)
            .await
            .expect("actions after rollback");
    let commands: i64 =
        sqlx::query_scalar("SELECT count(*) FROM meeting_action_commands WHERE tenant_id=$1")
            .bind(tenant)
            .fetch_one(&state.db_pool)
            .await
            .expect("commands after rollback");
    let events: i64 = sqlx::query_scalar("SELECT count(*) FROM events WHERE aggregate_id=$1 AND event_type LIKE '%MeetingActionModified%'").bind(meeting_id).fetch_one(&state.db_pool).await.expect("events after rollback");
    assert_eq!((actions, commands, events), (0, 0, 0));
    let restarted = app(setup_test_env().await);
    let action = create(&restarted, &token, meeting_id, body).await;
    let action_id = Uuid::parse_str(action["id"].as_str().expect("action ID")).expect("UUID");
    let action_uri = format!("{uri}/{action_id}");
    let mut complete = edit(&action);
    complete["state"] = json!("completed");
    let request_id =
        Uuid::parse_str(complete["request_id"].as_str().expect("request ID")).expect("UUID");
    action_event_failure_trigger(&state.db_pool, meeting_id, true).await;
    let failed = request(
        &restarted,
        &token,
        "PUT",
        &action_uri,
        Some(complete.clone()),
    )
    .await;
    action_event_failure_trigger(&state.db_pool, meeting_id, false).await;
    assert_eq!(failed.0, StatusCode::INTERNAL_SERVER_ERROR, "{failed:?}");
    assert!(!failed.1.to_string().contains("sensitive"));
    let (status, unchanged) = request(&restarted, &token, "GET", &action_uri, None).await;
    assert_eq!(status, StatusCode::OK, "{unchanged}");
    assert_eq!(
        unchanged, action,
        "failed commit cannot change action timestamps, version or state"
    );
    let commands: i64 = sqlx::query_scalar("SELECT count(*) FROM meeting_action_commands WHERE tenant_id=$1 AND actor_id=$2 AND request_id=$3")
        .bind(tenant).bind(owner.id).bind(request_id).fetch_one(&state.db_pool).await.expect("update receipt rollback");
    assert_eq!(commands, 0);
    assert_eq!(event_count(&state.db_pool, action_id).await, 1);
    let restarted_again = app(setup_test_env().await);
    let (status, completed) = request(
        &restarted_again,
        &token,
        "PUT",
        &action_uri,
        Some(complete.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    assert_eq!(completed["state"], "completed");
    assert_eq!(completed["version"], 2);
    let (status, retried) =
        request(&restarted_again, &token, "PUT", &action_uri, Some(complete)).await;
    assert_eq!(status, StatusCode::OK, "{retried}");
    assert_eq!(retried, completed);
    assert_eq!(event_count(&state.db_pool, action_id).await, 2);
    cleanup(&state.db_pool, tenant).await;
}

#[tokio::test]
#[ignore = "requires guarded disposable PostgreSQL and object storage"]
async fn deleting_source_meeting_retains_action_provenance_without_reassigning_it() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env().await;
    let tenant = create_test_tenant(&state.db_pool).await;
    let owner = create_test_user(&state, "actions_deleted_meeting", tenant).await;
    let id = meeting(&state, owner.id, tenant).await;
    let token = create_auth_token(&state, owner.id, tenant);
    let api = app(state.clone());
    let action = create(&api, &token, id, payload(Uuid::new_v4())).await;
    let action_id = Uuid::parse_str(action["id"].as_str().expect("action ID")).expect("UUID");
    let (status, deleted) = request(
        &api,
        &token,
        "DELETE",
        &format!("/api/v1/meetings/{id}"),
        None,
    )
    .await;
    assert!(status.is_success(), "{status}: {deleted}");
    let stored: (Uuid, Uuid, String) = sqlx::query_as(
        "SELECT meeting_id,created_by,title FROM meeting_actions WHERE tenant_id=$1 AND id=$2",
    )
    .bind(tenant)
    .bind(action_id)
    .fetch_one(&state.db_pool)
    .await
    .expect("retained action");
    assert_eq!(stored, (id, owner.id, "Prepare the release".into()));
    assert_eq!(event_count(&state.db_pool, action_id).await, 1);
    let restarted = app(setup_test_env().await);
    let (status, denied) = request(
        &restarted,
        &token,
        "GET",
        &format!("/api/v1/meetings/{id}/actions/{action_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{denied}");
    assert!(!denied.to_string().contains("Prepare the release"));
    let replacement = meeting(&state, owner.id, tenant).await;
    assert_ne!(replacement, id);
    let (status, page) = request(
        &restarted,
        &token,
        "GET",
        &format!("/api/v1/meetings/{replacement}/actions"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert_eq!(page["total"], 0);
    assert_eq!(page["items"], json!([]));
    cleanup(&state.db_pool, tenant).await;
}

#[tokio::test]
#[ignore = "requires guarded disposable PostgreSQL and object storage"]
async fn inherited_edit_grant_allows_action_mutation_and_revocation_denies_it() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env().await;
    let tenant = create_test_tenant(&state.db_pool).await;
    let owner = create_test_user(&state, "actions_inherit_owner", tenant).await;
    let editor = create_test_user(&state, "actions_inherit_editor", tenant).await;
    let id = meeting(&state, owner.id, tenant).await;
    let parent = state
        .folder_service
        .create_folder(
            format!("Actions parent {}", Uuid::new_v4()),
            None,
            owner.id,
            tenant,
        )
        .await
        .expect("parent");
    state
        .folder_service
        .move_folder(id, Some(parent.id), owner.id)
        .await
        .expect("move meeting under parent");
    sqlx::query("UPDATE users SET is_admin=true WHERE id=$1")
        .bind(owner.id)
        .execute(&state.db_pool)
        .await
        .expect("installation operator");
    let operator_token = create_auth_token(&state, owner.id, tenant);
    let admin_api = axum::Router::new().route("/api/v1/admin/organizations/{tenant_id}/administrators/{user_id}",
        axum::routing::post(rustshare_server::handlers::admin::organization_administrators::grant_organization_administrator)).with_state(state.clone());
    let (status, granted) = request(
        &admin_api,
        &operator_token,
        "POST",
        &format!(
            "/api/v1/admin/organizations/{tenant}/administrators/{}",
            editor.id
        ),
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{granted}");
    let token = create_auth_token(&state, editor.id, tenant);
    let api = app(state.clone());
    let private_uri = format!("/api/v1/meetings/{id}/actions");
    assert_eq!(
        request(&api, &token, "GET", &private_uri, None).await.0,
        StatusCode::NOT_FOUND,
        "organization administration is not private meeting authority"
    );
    assert_eq!(
        request(
            &api,
            &token,
            "POST",
            &private_uri,
            Some(payload(Uuid::new_v4()))
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    let share = grant(
        &state.db_pool,
        parent.id,
        owner.id,
        editor.id,
        tenant,
        "Edit",
    )
    .await;
    let action = create(&api, &token, id, payload(Uuid::new_v4())).await;
    assert_eq!(action["created_by"], editor.id.to_string());
    let uri = format!(
        "/api/v1/meetings/{id}/actions/{}",
        action["id"].as_str().expect("ID")
    );
    let mut complete = edit(&action);
    complete["state"] = json!("completed");
    let (status, completed) = request(&api, &token, "PUT", &uri, Some(complete)).await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    assert_eq!(completed["completed_by"], editor.id.to_string());
    sqlx::query("DELETE FROM shares WHERE id=$1")
        .bind(share)
        .execute(&state.db_pool)
        .await
        .expect("revoke parent grant");
    assert_eq!(
        request(&api, &token, "GET", &uri, None).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        request(&api, &token, "PUT", &uri, Some(edit(&completed)))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    cleanup(&state.db_pool, tenant).await;
}

#[tokio::test]
#[ignore = "requires guarded disposable PostgreSQL and object storage"]
async fn committed_command_retries_ignore_obsolete_grants_but_reauthorize_current_meeting() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env().await;
    let tenant = create_test_tenant(&state.db_pool).await;
    let owner = create_test_user(&state, "actions_receipt_owner", tenant).await;
    let editor = create_test_user(&state, "actions_receipt_editor", tenant).await;
    let responsible = create_test_user(&state, "actions_receipt_assignee", tenant).await;
    let meeting_id = meeting(&state, owner.id, tenant).await;
    let meeting_grant = grant(
        &state.db_pool,
        meeting_id,
        owner.id,
        editor.id,
        tenant,
        "Edit",
    )
    .await;
    let decision = state
        .decision_service
        .create_decision(
            owner.id,
            tenant,
            "Revoked receipt Decision".into(),
            "Planning".into(),
            "Private Decision body".into(),
        )
        .await
        .expect("Decision");
    let decision_grant = Uuid::new_v4();
    sqlx::query("INSERT INTO shares (id,file_id,share_token,created_by,permissions,recipient_user_id,tenant_id) VALUES ($1,$2,NULL,$3,'View',$4,$5)")
        .bind(decision_grant).bind(decision.id).bind(owner.id).bind(editor.id).bind(tenant).execute(&state.db_pool).await.expect("Decision grant");
    let token = create_auth_token(&state, editor.id, tenant);
    let api = app(state.clone());
    let uri = format!("/api/v1/meetings/{meeting_id}/actions");
    let mut creation = payload(Uuid::new_v4());
    creation["assignee_id"] = json!(responsible.id);
    creation["related_decision_id"] = json!(decision.id);
    let action = create(&api, &token, meeting_id, creation.clone()).await;
    let action_id = Uuid::parse_str(action["id"].as_str().expect("action ID")).expect("UUID");
    let action_uri = format!("{uri}/{action_id}");
    let mut completion = edit(&action);
    completion["state"] = json!("completed");
    completion["related_decision_id"] = json!(decision.id);
    let (status, completed) =
        request(&api, &token, "PUT", &action_uri, Some(completion.clone())).await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    sqlx::query("UPDATE users SET disabled_at=now() WHERE id=$1")
        .bind(responsible.id)
        .execute(&state.db_pool)
        .await
        .expect("disable obsolete assignee");
    let restarted = app(setup_test_env().await);
    let retried = create(&restarted, &token, meeting_id, creation.clone()).await;
    assert_eq!(retried["version"], completed["version"]);
    assert_eq!(retried["state"], "completed");
    assert!(
        retried["assignee"].is_null(),
        "current response redacts disabled assignee metadata"
    );
    let mut clear_assignment = edit(&retried);
    clear_assignment["assignee_id"] = Value::Null;
    let (status, latest) = request(
        &restarted,
        &token,
        "PUT",
        &action_uri,
        Some(clear_assignment),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{latest}");
    let (status, old_completion) = request(
        &restarted,
        &token,
        "PUT",
        &action_uri,
        Some(completion.clone()),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "obsolete disabled assignment must not block committed PUT recovery: {old_completion}"
    );
    assert_eq!(old_completion, latest);
    sqlx::query("DELETE FROM shares WHERE id=$1")
        .bind(decision_grant)
        .execute(&state.db_pool)
        .await
        .expect("revoke obsolete Decision grant");
    let (status, current) = request(&restarted, &token, "GET", &action_uri, None).await;
    assert_eq!(status, StatusCode::OK, "{current}");
    assert!(current["related_decision"].is_null());
    assert!(!current.to_string().contains(&decision.id.to_string()));
    assert!(!current.to_string().contains("Revoked receipt Decision"));
    let recovered_create = create(&restarted, &token, meeting_id, creation.clone()).await;
    assert_eq!(
        recovered_create, current,
        "receipt replay does not revalidate obsolete write references"
    );
    let (status, recovered_update) = request(
        &restarted,
        &token,
        "PUT",
        &action_uri,
        Some(completion.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{recovered_update}");
    assert_eq!(recovered_update, current);
    assert_eq!(event_count(&state.db_pool, action_id).await, 3);
    let mut fresh_create = creation.clone();
    fresh_create["request_id"] = json!(Uuid::new_v4());
    assert_eq!(
        request(&restarted, &token, "POST", &uri, Some(fresh_create))
            .await
            .0,
        StatusCode::BAD_REQUEST,
        "fresh commands still validate disabled assignments"
    );
    let mut fresh_update = edit(&current);
    fresh_update["related_decision_id"] = json!(decision.id);
    assert_eq!(
        request(&restarted, &token, "PUT", &action_uri, Some(fresh_update))
            .await
            .0,
        StatusCode::NOT_FOUND,
        "fresh commands still authorize the Decision"
    );
    let mut conflicting = creation.clone();
    conflicting["title"] = json!("Changed obsolete draft");
    assert_eq!(
        request(&restarted, &token, "POST", &uri, Some(conflicting))
            .await
            .0,
        StatusCode::CONFLICT
    );
    sqlx::query("DELETE FROM shares WHERE id=$1")
        .bind(meeting_grant)
        .execute(&state.db_pool)
        .await
        .expect("revoke meeting authority");
    assert_eq!(
        request(&restarted, &token, "POST", &uri, Some(creation))
            .await
            .0,
        StatusCode::NOT_FOUND,
        "receipts never bypass current meeting authority"
    );
    assert_eq!(
        request(&restarted, &token, "PUT", &action_uri, Some(completion))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(event_count(&state.db_pool, action_id).await, 3);
    cleanup(&state.db_pool, tenant).await;
}
