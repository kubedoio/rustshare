//! Guarded real-route regression for unified admin audit redaction.

mod support;

use axum::{
    body::Body,
    http::{header, Method, Request, StatusCode},
    Router,
};
use serde_json::Value;
use std::error::Error;
use support::calendar_harness::{setup_test_env, SERIAL};
use tower::ServiceExt;
use uuid::Uuid;

type TestResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

fn require_test(condition: bool, message: &'static str) -> TestResult<()> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}

fn is_disposable_test_database(name: &str) -> bool {
    name == "rustshare_test"
}

fn is_disposable_test_bucket(name: &str) -> bool {
    name == "rustshare-test-integration"
}

#[test]
fn disposable_target_guards_reject_production_like_prefixes() {
    assert!(is_disposable_test_database("rustshare_test"));
    assert!(!is_disposable_test_database("rustshare_test_integration"));
    assert!(!is_disposable_test_database("rustshare_test_prod"));
    assert!(!is_disposable_test_database("rustshare_testprod"));

    assert!(is_disposable_test_bucket("rustshare-test-integration"));
    assert!(!is_disposable_test_bucket("rustshare-test-prod"));
    assert!(!is_disposable_test_bucket("rustshare-testprod"));
}

async fn cleanup_statement(
    pool: &sqlx::PgPool,
    statement: &str,
    first_error: &mut Option<sqlx::Error>,
) {
    if let Err(error) = sqlx::query(statement).execute(pool).await {
        if first_error.is_none() {
            *first_error = Some(error);
        }
    }
}

async fn audit_route_status(app: &Router, bearer: Option<&str>) -> TestResult<StatusCode> {
    let mut request = Request::builder()
        .method(Method::GET)
        .uri("/api/v1/admin/audit");
    if let Some(bearer) = bearer {
        request = request.header(header::AUTHORIZATION, format!("Bearer {bearer}"));
    }
    Ok(app
        .clone()
        .oneshot(request.body(Body::empty())?)
        .await?
        .status())
}

async fn audit_route_cookie_status(app: &Router, session_token: &str) -> TestResult<StatusCode> {
    let request = Request::builder()
        .method(Method::GET)
        .uri("/api/v1/admin/audit")
        .header(
            header::COOKIE,
            format!(
                "{}={session_token}",
                rustshare_auth::WEB_SESSION_COOKIE_NAME
            ),
        )
        .body(Body::empty())?;
    Ok(app.clone().oneshot(request).await?.status())
}

#[tokio::test]
#[ignore = "requires explicitly configured disposable local PostgreSQL and RustFS services"]
async fn unified_admin_audit_enforces_admin_access_redacts_private_data_and_bounds_pagination(
) -> TestResult<()> {
    let _serial = SERIAL.lock().await;

    let database_url = std::env::var("DATABASE_URL")
        .map_err(|_| std::io::Error::other("set DATABASE_URL to a disposable local database"))?;
    let parsed_database_url = url::Url::parse(&database_url)?;
    let database_name = parsed_database_url.path().trim_start_matches('/');
    require_test(
        is_disposable_test_database(database_name),
        "refusing audit redaction test outside rustshare_test or rustshare_test_*",
    )?;
    require_test(
        matches!(
            parsed_database_url.host_str(),
            Some("localhost" | "127.0.0.1" | "::1")
        ),
        "refusing audit redaction test against a non-loopback database",
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
        matches!(
            parsed_endpoint.host_str(),
            Some("localhost" | "127.0.0.1" | "::1")
        ) && parsed_endpoint.username().is_empty()
            && parsed_endpoint.password().is_none(),
        "refusing audit redaction test against a non-loopback or credentialed object store",
    )?;
    let bucket = std::env::var("S3_BUCKET").or_else(|_| std::env::var("RUSTFS_BUCKET"))?;
    require_test(
        is_disposable_test_bucket(&bucket),
        "refusing audit redaction test outside rustshare-test or rustshare-test-*",
    )?;

    let state = setup_test_env().await;
    let pool = state.db_pool.clone();
    let tenant_id = Uuid::new_v4();
    let actor_id = Uuid::new_v4();
    let non_admin_id = Uuid::new_v4();
    let disabled_admin_id = Uuid::new_v4();
    let disabled_admin_session_id = Uuid::new_v4();
    let disabled_admin_session_token = rustshare_auth::generate_web_session_token();
    let disabled_admin_session_hash =
        rustshare_auth::hash_web_session_token(&disabled_admin_session_token);
    let admin_action_id = Uuid::new_v4();
    let template_action_id = Uuid::new_v4();
    let template_object_id = Uuid::new_v4();
    let security_event_id = Uuid::new_v4();
    let session_revoked_event_id = Uuid::new_v4();
    let other_user_security_event_id = Uuid::new_v4();
    let security_session_id = Uuid::new_v4();
    let revoked_session_id = Uuid::new_v4();
    let file_id = Uuid::new_v4();
    let share_id = Uuid::new_v4();
    let share_access_id = Uuid::new_v4();
    let share_session_id = Uuid::new_v4();
    let admin_action_id_text = admin_action_id.to_string();
    let template_action_id_text = template_action_id.to_string();
    let template_object_id_text = template_object_id.to_string();
    let security_event_id_text = security_event_id.to_string();
    let session_revoked_event_id_text = session_revoked_event_id.to_string();
    let other_user_security_event_id_text = other_user_security_event_id.to_string();
    let revoked_session_id_text = revoked_session_id.to_string();
    let share_access_id_text = share_access_id.to_string();
    let share_session_id_text = share_session_id.to_string();
    let suffix = actor_id.simple();
    let username = format!("audit_redaction_admin_{suffix}");
    let unapproved_sentinel = format!("unapproved-admin-detail-{suffix}");
    let private_path_sentinel = format!("/Workspace/private-notes-{suffix}/minutes.md");
    let description_sentinel = format!("private-security-description-{suffix}");
    let session_agent_sentinel = format!("untrusted-session-agent-{suffix}");
    let foreign_event_sentinel = format!("foreign-security-event-{suffix}");
    let share_agent_sentinel = format!("untrusted-share-agent-{suffix}");
    let share_subject_sentinel = format!("private-share-subject-{suffix}");
    let share_actor_label = format!("Pilot uploader {suffix}");
    let revoked_session_token = rustshare_auth::generate_web_session_token();
    let revoked_session_token_hash = rustshare_auth::hash_web_session_token(&revoked_session_token);
    let raw_ip = "203.0.113.77";
    let started_at = chrono::Utc::now() - chrono::Duration::seconds(1);

    let attempt: TestResult<()> = async {
        sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, $2)")
            .bind(tenant_id)
            .bind(format!("Audit redaction fixture {suffix}"))
            .execute(&pool)
            .await?;

        sqlx::query(
            "INSERT INTO users
                (id, username, email, password_hash, display_name, is_admin,
                 storage_quota, tenant_id)
             VALUES ($1, $2, $3, 'test-password-hash', $2, true, 10737418240, $4)",
        )
        .bind(actor_id)
        .bind(&username)
        .bind(format!("{username}@test.local"))
        .bind(tenant_id)
        .execute(&pool)
        .await?;

        sqlx::query(
            "INSERT INTO user_sessions
                (id, user_id, session_token_hash, expires_at, tenant_id, user_agent)
             VALUES ($1, $2, $3, NOW() + INTERVAL '1 hour', $4, $5)",
        )
        .bind(revoked_session_id)
        .bind(actor_id)
        .bind(&revoked_session_token_hash)
        .bind(tenant_id)
        .bind(&session_agent_sentinel)
        .execute(&pool)
        .await?;

        for (user_id, role, disabled_at) in [
            (non_admin_id, false, false),
            (disabled_admin_id, true, true),
        ] {
            sqlx::query(
                "INSERT INTO users
                    (id, username, email, password_hash, display_name, is_admin,
                     storage_quota, tenant_id, disabled_at)
                 VALUES ($1, $2, $3, 'test-password-hash', $2, $4, 10737418240, $5,
                         CASE WHEN $6 THEN NOW() ELSE NULL END)",
            )
            .bind(user_id)
            .bind(format!(
                "audit_boundary_{}_{suffix}",
                if role { "admin" } else { "user" }
            ))
            .bind(format!(
                "audit_boundary_{}_{suffix}@test.local",
                if role { "admin" } else { "user" }
            ))
            .bind(role)
            .bind(tenant_id)
            .bind(disabled_at)
            .execute(&pool)
            .await?;
        }

        sqlx::query(
            "INSERT INTO user_security_events
                (id, user_id, event_type, description, user_agent, session_id)
             VALUES ($1, $2, 'session_revoked', $3, $4, $5)",
        )
        .bind(session_revoked_event_id)
        .bind(actor_id)
        .bind(format!(
            "Revoked browser session ({session_agent_sentinel})"
        ))
        .bind(&session_agent_sentinel)
        .bind(security_session_id)
        .execute(&pool)
        .await?;

        sqlx::query(
            "INSERT INTO user_security_events (id, user_id, event_type, description)
             VALUES ($1, $2, 'login_failed', $3)",
        )
        .bind(other_user_security_event_id)
        .bind(non_admin_id)
        .bind(&foreign_event_sentinel)
        .execute(&pool)
        .await?;

        sqlx::query(
            "INSERT INTO user_sessions (id, user_id, session_token_hash, expires_at, tenant_id)
             VALUES ($1, $2, $3, NOW() + INTERVAL '1 hour', $4)",
        )
        .bind(disabled_admin_session_id)
        .bind(disabled_admin_id)
        .bind(&disabled_admin_session_hash)
        .bind(tenant_id)
        .execute(&pool)
        .await?;

        sqlx::query(
            "INSERT INTO admin_actions
                (id, actor_id, action_type, target_type, target_id, detail)
             VALUES ($1, $2, 'user.quota_changed', 'user', $2, $3)",
        )
        .bind(admin_action_id)
        .bind(actor_id)
        .bind(serde_json::json!({
            "old_quota": 1024,
            "new_quota": 2048,
            "unapproved": unapproved_sentinel,
        }))
        .execute(&pool)
        .await?;

        sqlx::query(
            "INSERT INTO admin_actions
                (id, actor_id, action_type, target_type, target_id, detail)
             VALUES ($1, $2, 'object.created.from_template', 'template', $3, $4)",
        )
        .bind(template_action_id)
        .bind(actor_id)
        .bind(template_object_id)
        .bind(serde_json::json!({
            "template_key": "pilot_template",
            "object_id": template_object_id_text.as_str(),
            "path": private_path_sentinel.as_str()
        }))
        .execute(&pool)
        .await?;

        sqlx::query(
            "INSERT INTO user_security_events (id, user_id, event_type, description)
             VALUES ($1, $2, 'login_failed', $3)",
        )
        .bind(security_event_id)
        .bind(actor_id)
        .bind(&description_sentinel)
        .execute(&pool)
        .await?;

        sqlx::query(
            "INSERT INTO files
                (id, name, path, size, mime_type, content_hash, storage_key, owner_id, tenant_id)
             VALUES ($1, 'audit-redaction.txt', '/audit-redaction.txt', 0, 'text/plain',
                     'sha256:audit-redaction', $2, $3, $4)",
        )
        .bind(file_id)
        .bind(format!("audit-redaction/{file_id}"))
        .bind(actor_id)
        .bind(tenant_id)
        .execute(&pool)
        .await?;

        sqlx::query(
            "INSERT INTO shares (id, file_id, share_token, created_by, permissions, tenant_id)
             VALUES ($1, $2, $3, $4, 'View', $5)",
        )
        .bind(share_id)
        .bind(file_id)
        .bind(format!("audit-redaction-{suffix}"))
        .bind(actor_id)
        .bind(tenant_id)
        .execute(&pool)
        .await?;

        sqlx::query(
            "INSERT INTO share_access_log
                (id, share_id, ip_address, user_agent, action, success, actor_type,
                 actor_label, share_session_id, share_session_subject)
             VALUES ($1, $2, $3::inet, $4, 'download', true, 'public_share_session', $5, $6, $7)",
        )
        .bind(share_access_id)
        .bind(share_id)
        .bind(raw_ip)
        .bind(&share_agent_sentinel)
        .bind(&share_actor_label)
        .bind(share_session_id)
        .bind(&share_subject_sentinel)
        .execute(&pool)
        .await?;

        let app = rustshare_server::routes::admin_routes().with_state(state.clone());
        let bearer = support::calendar_harness::create_auth_token(&state, actor_id, tenant_id);
        require_test(
            audit_route_status(&app, None).await? == StatusCode::UNAUTHORIZED,
            "anonymous callers must not read the admin audit route",
        )?;
        let non_admin_bearer =
            support::calendar_harness::create_auth_token(&state, non_admin_id, tenant_id);
        require_test(
            audit_route_status(&app, Some(&non_admin_bearer)).await? == StatusCode::FORBIDDEN,
            "authenticated non-admin callers must not read the admin audit route",
        )?;
        let disabled_admin_bearer =
            support::calendar_harness::create_auth_token(&state, disabled_admin_id, tenant_id);
        require_test(
            audit_route_status(&app, Some(&disabled_admin_bearer)).await?
                == StatusCode::UNAUTHORIZED,
            "disabled administrator accounts must be rejected during authentication",
        )?;
        require_test(
            audit_route_cookie_status(&app, &disabled_admin_session_token).await?
                == StatusCode::UNAUTHORIZED,
            "disabled administrator cookie sessions must be rejected during authentication",
        )?;
        let request = Request::builder()
            .method(Method::GET)
            .uri(format!(
                "/api/v1/admin/audit?type=all&from={}&per_page=100",
                url::form_urlencoded::byte_serialize(started_at.to_rfc3339().as_bytes())
                    .collect::<String>()
            ))
            .header(header::AUTHORIZATION, format!("Bearer {bearer}"))
            .body(Body::empty())?;
        let response = app.clone().oneshot(request).await?;
        require_test(
            response.status() == StatusCode::OK,
            "admin audit route should accept the generated administrator",
        )?;
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
        let body_text = String::from_utf8(body.to_vec())?;
        let payload: Value = serde_json::from_str(&body_text)?;
        let entries = payload
            .get("entries")
            .and_then(Value::as_array)
            .ok_or("audit response should contain an entries array")?;

        let admin_action = entries
            .iter()
            .find(|entry| {
                entry.get("id").and_then(Value::as_str) == Some(admin_action_id_text.as_str())
            })
            .ok_or("audit response should include the seeded admin action")?;
        require_test(
            admin_action.get("detail")
                == Some(&serde_json::json!({
                    "old_quota": 1024,
                    "new_quota": 2048,
                })),
            "admin action detail should contain only its allowlisted fields",
        )?;

        let template_action = entries
            .iter()
            .find(|entry| {
                entry.get("id").and_then(Value::as_str) == Some(template_action_id_text.as_str())
            })
            .ok_or("audit response should include the seeded template-object action")?;
        require_test(
            template_action.get("detail")
                == Some(&serde_json::json!({
                    "template_key": "pilot_template",
                    "object_id": template_object_id_text.as_str()
                })),
            "template-object audit response must omit its private path",
        )?;

        let security_event = entries
            .iter()
            .find(|entry| {
                entry.get("id").and_then(Value::as_str) == Some(security_event_id_text.as_str())
            })
            .ok_or("audit response should include the seeded security event")?;
        require_test(
            security_event
                .get("detail")
                .and_then(Value::as_object)
                .is_some_and(|detail| !detail.contains_key("description")),
            "security-event detail should omit its free-form description",
        )?;

        let share_access = entries
            .iter()
            .find(|entry| {
                entry.get("id").and_then(Value::as_str) == Some(share_access_id_text.as_str())
            })
            .ok_or("audit response should include the seeded share-access row")?;
        let share_detail = share_access
            .get("detail")
            .and_then(Value::as_object)
            .ok_or("share-access detail should be an object")?;
        require_test(
            !share_detail.contains_key("ip_address")
                && share_detail.get("success") == Some(&Value::Bool(true)),
            "share-access detail should omit raw IP while preserving success",
        )?;
        require_test(
            !body_text.contains(&unapproved_sentinel)
                && !body_text.contains(&private_path_sentinel)
                && !body_text.contains(&description_sentinel)
                && !body_text.contains(&session_agent_sentinel)
                && !body_text.contains(&foreign_event_sentinel)
                && !body_text.contains(&share_agent_sentinel)
                && !body_text.contains(&share_subject_sentinel)
                && !body_text.contains(raw_ip),
            "audit response must not expose unapproved detail, description, or raw IP sentinels",
        )?;

        let user_app = rustshare_server::routes::user_routes().with_state(state.clone());
        let revoke_session_request = Request::builder()
            .method(Method::DELETE)
            .uri(format!("/api/v1/me/sessions/{revoked_session_id}"))
            .header(header::AUTHORIZATION, format!("Bearer {bearer}"))
            .header(header::USER_AGENT, &session_agent_sentinel)
            .body(Body::empty())?;
        let revoke_session_response = user_app.clone().oneshot(revoke_session_request).await?;
        require_test(
            revoke_session_response.status() == StatusCode::NO_CONTENT,
            "the authenticated user should be able to revoke their browser session",
        )?;

        let user_security_request = Request::builder()
            .method(Method::GET)
            .uri("/api/v1/me/security-events")
            .header(header::AUTHORIZATION, format!("Bearer {bearer}"))
            .body(Body::empty())?;
        let user_security_response = user_app.clone().oneshot(user_security_request).await?;
        require_test(
            user_security_response.status() == StatusCode::OK,
            "authenticated users should be able to read their security events",
        )?;
        let user_security_body =
            axum::body::to_bytes(user_security_response.into_body(), usize::MAX).await?;
        let user_security_text = String::from_utf8(user_security_body.to_vec())?;
        let user_security_events: Vec<Value> = serde_json::from_str(&user_security_text)?;
        let revoked_event = user_security_events
            .iter()
            .find(|event| {
                event.get("id").and_then(Value::as_str)
                    == Some(session_revoked_event_id_text.as_str())
            })
            .ok_or("user security response should include its session-revoked event")?;
        let newly_revoked_event = user_security_events
            .iter()
            .find(|event| {
                event.get("session_id").and_then(Value::as_str)
                    == Some(revoked_session_id_text.as_str())
            })
            .ok_or("session revocation should create a security event")?;
        require_test(
            revoked_event.get("description").and_then(Value::as_str)
                == Some("Revoked browser session")
                && revoked_event.get("user_agent") == Some(&Value::Null)
                && newly_revoked_event
                    .get("description")
                    .and_then(Value::as_str)
                    == Some("Revoked browser session")
                && newly_revoked_event.get("user_agent") == Some(&Value::Null),
            "session-revoked events should omit historical and newly recorded free-form agent data",
        )?;
        require_test(
            !user_security_text.contains(&session_agent_sentinel)
                && !user_security_text.contains(&foreign_event_sentinel)
                && !user_security_text.contains(&other_user_security_event_id_text),
            "security events must not expose raw agent data or another user's events",
        )?;

        let share_app = rustshare_server::routes::share_routes().with_state(state.clone());
        let share_access_request = Request::builder()
            .method(Method::GET)
            .uri(format!("/api/v1/shares/{share_id}/access-log"))
            .header(header::AUTHORIZATION, format!("Bearer {bearer}"))
            .body(Body::empty())?;
        let share_access_response = share_app.clone().oneshot(share_access_request).await?;
        require_test(
            share_access_response.status() == StatusCode::OK,
            "share owners should be able to read their access log",
        )?;
        let share_access_body =
            axum::body::to_bytes(share_access_response.into_body(), usize::MAX).await?;
        let share_access_text = String::from_utf8(share_access_body.to_vec())?;
        let share_access_entries: Vec<Value> = serde_json::from_str(&share_access_text)?;
        let share_access_entry = share_access_entries
            .iter()
            .find(|entry| {
                entry.get("actor_label").and_then(Value::as_str) == Some(share_actor_label.as_str())
            })
            .ok_or("share owner response should retain the uploader label")?;
        require_test(
            share_access_entry.get("user_agent") == Some(&Value::Null),
            "share access response should omit raw user-agent metadata",
        )?;
        require_test(
            share_access_entry.get("share_session_id") == Some(&Value::Null)
                && share_access_entry.get("share_session_subject") == Some(&Value::Null),
            "share access response should omit internal session metadata",
        )?;
        require_test(
            matches!(
                share_access_entry.get("ip_address").and_then(Value::as_str),
                Some("203.0.113.77" | "203.0.113.77/32")
            ),
            "share owner response should retain the visitor IP field already used by the UI",
        )?;
        require_test(
            !share_access_text.contains(&share_agent_sentinel)
                && !share_access_text.contains(&share_subject_sentinel)
                && !share_access_text.contains(&share_session_id_text),
            "share access response must not expose raw user-agent or session-subject metadata",
        )?;

        let non_owner_share_request = Request::builder()
            .method(Method::GET)
            .uri(format!("/api/v1/shares/{share_id}/access-log"))
            .header(header::AUTHORIZATION, format!("Bearer {non_admin_bearer}"))
            .body(Body::empty())?;
        let non_owner_share_response = share_app.oneshot(non_owner_share_request).await?;
        require_test(
            non_owner_share_response.status() == StatusCode::OK,
            "a non-owner should receive an empty share access log, not another owner's data",
        )?;
        let non_owner_share_body =
            axum::body::to_bytes(non_owner_share_response.into_body(), usize::MAX).await?;
        let non_owner_share_entries: Vec<Value> = serde_json::from_slice(&non_owner_share_body)?;
        require_test(
            non_owner_share_entries.is_empty(),
            "share access log query must remain scoped to its owner",
        )?;

        let last_page_with_safe_offset = i64::MAX / 100 + 1;
        let far_page_request = Request::builder()
            .method(Method::GET)
            .uri(format!(
                "/api/v1/admin/audit?page={last_page_with_safe_offset}&per_page=100"
            ))
            .header(header::AUTHORIZATION, format!("Bearer {bearer}"))
            .body(Body::empty())?;
        let far_page_response = rustshare_server::routes::admin_routes()
            .with_state(state.clone())
            .oneshot(far_page_request)
            .await?;
        require_test(
            far_page_response.status() == StatusCode::OK,
            "a valid but out-of-range audit page should return an empty page",
        )?;
        let far_page_body = axum::body::to_bytes(far_page_response.into_body(), usize::MAX).await?;
        let far_page_payload: Value = serde_json::from_slice(&far_page_body)?;
        require_test(
            far_page_payload
                .get("entries")
                .and_then(Value::as_array)
                .is_some_and(Vec::is_empty),
            "an out-of-range audit page should not return entries",
        )?;

        let overflow_page_request = Request::builder()
            .method(Method::GET)
            .uri(format!(
                "/api/v1/admin/audit?page={}&per_page=100",
                last_page_with_safe_offset + 1
            ))
            .header(header::AUTHORIZATION, format!("Bearer {bearer}"))
            .body(Body::empty())?;
        let overflow_page_response = rustshare_server::routes::admin_routes()
            .with_state(state.clone())
            .oneshot(overflow_page_request)
            .await?;
        require_test(
            overflow_page_response.status() == StatusCode::BAD_REQUEST,
            "an audit page whose offset overflows must be rejected as a bad request",
        )?;
        Ok(())
    }
    .await;

    let mut cleanup_error = None;
    for statement in [
        format!("DELETE FROM share_access_log WHERE id = '{share_access_id}'"),
        format!("DELETE FROM shares WHERE id = '{share_id}'"),
        format!("DELETE FROM files WHERE id = '{file_id}'"),
        format!("DELETE FROM admin_actions WHERE id = '{template_action_id}'"),
        format!("DELETE FROM admin_actions WHERE id = '{admin_action_id}'"),
        format!("DELETE FROM user_security_events WHERE id = '{security_event_id}'"),
        format!("DELETE FROM user_security_events WHERE id = '{session_revoked_event_id}'"),
        format!("DELETE FROM user_security_events WHERE id = '{other_user_security_event_id}'"),
        format!("DELETE FROM user_sessions WHERE id = '{disabled_admin_session_id}'"),
        format!("DELETE FROM user_sessions WHERE id = '{revoked_session_id}'"),
        format!("DELETE FROM users WHERE id = '{non_admin_id}'"),
        format!("DELETE FROM users WHERE id = '{disabled_admin_id}'"),
        format!("DELETE FROM users WHERE id = '{actor_id}'"),
        format!("DELETE FROM tenants WHERE id = '{tenant_id}'"),
    ] {
        cleanup_statement(&pool, &statement, &mut cleanup_error).await;
    }

    if let Err(test_error) = attempt {
        if let Some(cleanup_error) = cleanup_error {
            return Err(format!("{test_error}; cleanup also failed: {cleanup_error}").into());
        }
        return Err(test_error);
    }
    if let Some(cleanup_error) = cleanup_error {
        return Err(Box::new(cleanup_error) as Box<dyn Error + Send + Sync>);
    }
    Ok(())
}
