//! Real-service API coverage for owner boundaries across meeting-context
//! resources. These checks characterize existing access rules; they do not
//! grant workspace or cross-user visibility.

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
    Router,
};
use chrono::Utc;
use rustshare_server::state::AppState;
use serde_json::{json, Value};
use std::error::Error;
use tower::ServiceExt;
use uuid::Uuid;

mod support;

use support::calendar_harness::{
    cleanup_tenant, create_auth_token, create_test_tenant, create_test_user, setup_test_env, SERIAL,
};

type TestResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

async fn cleanup_user(state: &AppState, user_id: Uuid) {
    let object_keys = sqlx::query_scalar::<_, String>(
        r#"
        SELECT storage_key FROM files WHERE owner_id = $1
        UNION
        SELECT versions.storage_key
        FROM file_versions AS versions
        JOIN files ON files.id = versions.file_id
        WHERE files.owner_id = $1
        "#,
    )
    .bind(user_id)
    .fetch_all(&state.db_pool)
    .await
    .expect("load test user's object keys");

    sqlx::query("DELETE FROM file_versions WHERE created_by = $1")
        .bind(user_id)
        .execute(&state.db_pool)
        .await
        .expect("delete test user's file versions");
    sqlx::query("DELETE FROM files WHERE owner_id = $1")
        .bind(user_id)
        .execute(&state.db_pool)
        .await
        .expect("delete test user's files");
    sqlx::query("DELETE FROM folders WHERE owner_id = $1")
        .bind(user_id)
        .execute(&state.db_pool)
        .await
        .expect("delete test user's folders");
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(&state.db_pool)
        .await
        .expect("delete test user");

    for object_key in object_keys {
        let _blob_lock = state
            .object_store
            .acquire_blob_lock(&object_key)
            .await
            .expect("lock test object during cleanup");
        let references = state
            .metadata_store
            .count_blob_references(&object_key)
            .await
            .expect("count test object references")
            .total();
        if references == 0
            && state
                .object_store
                .exists(&object_key)
                .await
                .expect("check test object existence")
        {
            let references = state
                .metadata_store
                .count_blob_references(&object_key)
                .await
                .expect("recount test object references under lock")
                .total();
            if references == 0 {
                state
                    .object_store
                    .delete(&object_key)
                    .await
                    .expect("delete unreferenced test object");
                assert!(
                    !state
                        .object_store
                        .exists(&object_key)
                        .await
                        .expect("verify test object removal"),
                    "test object still exists after cleanup"
                );
            }
        }
    }
}

async fn json_request(
    app: &Router,
    method: Method,
    uri: String,
    token: &str,
    body: Option<Value>,
) -> TestResult<(StatusCode, Value)> {
    let mut request = Request::builder()
        .method(method)
        .uri(uri)
        .header("Authorization", format!("Bearer {token}"));
    let body = if let Some(body) = body {
        request = request.header("Content-Type", "application/json");
        Body::from(body.to_string())
    } else {
        Body::empty()
    };
    let response = app.clone().oneshot(request.body(body)?).await?;
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024).await?;
    Ok((status, serde_json::from_slice(&body)?))
}

#[tokio::test]
#[ignore = "requires the guarded disposable PostgreSQL and object-store services"]
async fn same_tenant_non_owner_cannot_read_change_or_delete_meeting_context_resources() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env().await;
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let owner = create_test_user(&state, "context_owner", tenant_id).await;
    let other = create_test_user(&state, "context_other", tenant_id).await;
    let owner_token = create_auth_token(&state, owner.id, tenant_id);
    let other_token = create_auth_token(&state, other.id, tenant_id);

    let meeting_date = Utc::now();
    let app = rustshare_server::routes::meeting_routes()
        .merge(rustshare_server::routes::decision_routes())
        .merge(rustshare_server::routes::standup_routes())
        .with_state(state.clone())
        .layer(axum::middleware::from_fn(
            rustshare_server::middleware::security_headers_middleware,
        ));

    let attempt: TestResult<_> = async {
        let meeting = state
            .meeting_service
            .create_meeting(
                owner.id,
                tenant_id,
                "Owner meeting".to_string(),
                "Pilot team".to_string(),
                meeting_date,
                "Owner meeting content".to_string(),
            )
            .await?;
        let decision = state
            .decision_service
            .create_decision(
                owner.id,
                tenant_id,
                "Owner decision".to_string(),
                "Pilot".to_string(),
                "Owner decision content".to_string(),
            )
            .await?;
        let standup = state
            .standup_service
            .create_standup(
                owner.id,
                tenant_id,
                "Owner standup".to_string(),
                Utc::now(),
                "Owner standup content".to_string(),
            )
            .await?;

        let resources = [
            (
                "meetings",
                meeting.id,
                "Owner meeting",
                "Owner meeting content",
            ),
            (
                "decisions",
                decision.id,
                "Owner decision",
                "Owner decision content",
            ),
            (
                "standups",
                standup.id,
                "Owner standup",
                "Owner standup content",
            ),
        ];
        let mut results = Vec::new();

        for (resource, id, title, content) in resources {
            let uri = format!("/api/v1/{resource}/{id}");
            let (read_status, read_body) =
                json_request(&app, Method::GET, uri.clone(), &other_token, None).await?;
            let (update_status, _) = json_request(
                &app,
                Method::PUT,
                uri.clone(),
                &other_token,
                Some(json!({
                    "title": "Unauthorized replacement",
                    "content": "Unauthorized content"
                })),
            )
            .await?;
            let (delete_status, _) =
                json_request(&app, Method::DELETE, uri.clone(), &other_token, None).await?;
            let (owner_status, owner_body) =
                json_request(&app, Method::GET, uri, &owner_token, None).await?;

            results.push((
                read_status,
                read_body,
                update_status,
                delete_status,
                owner_status,
                owner_body,
                title,
                content,
            ));
        }
        Ok(results)
    }
    .await;

    cleanup_user(&state, owner.id).await;
    cleanup_user(&state, other.id).await;
    cleanup_tenant(&state.db_pool, tenant_id).await;

    let results = attempt.expect("create and exercise meeting-context resources");
    for (
        read_status,
        read_body,
        update_status,
        delete_status,
        owner_status,
        owner_body,
        title,
        content,
    ) in results
    {
        assert_eq!(read_status, StatusCode::FORBIDDEN);
        let denied_body = read_body.to_string();
        assert!(!denied_body.contains(title));
        assert!(!denied_body.contains(content));
        assert_eq!(update_status, StatusCode::FORBIDDEN);
        assert_eq!(delete_status, StatusCode::FORBIDDEN);
        assert_eq!(owner_status, StatusCode::OK);
        assert_eq!(owner_body["metadata"]["title"], title);
        assert_eq!(owner_body["content"], content);
    }
}
