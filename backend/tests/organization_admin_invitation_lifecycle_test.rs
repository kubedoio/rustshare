//! HTTP/PostgreSQL tests for tenant-safe organization invitation lifecycle.

mod support;

use axum::{
    body::Body,
    http::{HeaderMap, HeaderValue, Method, Request, StatusCode},
    Router,
};
use serde_json::{json, Value};
use sqlx::PgPool;
use support::calendar_harness::{assert_local_database, create_auth_token, setup_test_env, SERIAL};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::TcpListener,
};
use tower::ServiceExt;
use uuid::Uuid;

async fn request(
    app: &Router,
    method: Method,
    uri: &str,
    bearer: Option<&str>,
    headers: HeaderMap,
    body: Option<Value>,
) -> (StatusCode, Vec<u8>, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(bearer) = bearer {
        builder = builder.header("authorization", format!("Bearer {bearer}"));
    }
    for (name, value) in &headers {
        builder = builder.header(name, value);
    }
    let mut builder = builder;
    if body.is_some() {
        builder = builder.header("content-type", "application/json");
    }
    let response = app
        .clone()
        .oneshot(
            builder
                .body(
                    body.map(|value| Body::from(value.to_string()))
                        .unwrap_or_else(Body::empty),
                )
                .expect("request"),
        )
        .await
        .expect("response");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1_000_000)
        .await
        .expect("response body")
        .to_vec();
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, bytes, value)
}

async fn create_user(pool: &PgPool, tenant_id: Uuid, prefix: &str) -> Uuid {
    let user_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users
            (id,tenant_id,username,email,password_hash,display_name,is_admin,storage_quota)
         VALUES ($1,$2,$3,$4,'test-hash',$3,false,10737418240)",
    )
    .bind(user_id)
    .bind(tenant_id)
    .bind(format!("{prefix}_{}", user_id.simple()))
    .bind(format!("{prefix}_{}@test.local", user_id.simple()))
    .execute(pool)
    .await
    .expect("insert user");
    user_id
}

async fn grant_organization_admin(pool: &PgPool, tenant_id: Uuid, user_id: Uuid) {
    let grant_id = Uuid::new_v4();
    let granted_at: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(pool)
        .await
        .expect("database time");
    let mut tx = pool.begin().await.expect("grant transaction");
    sqlx::query(
        "INSERT INTO admin_actions
            (tenant_id,actor_id,actor_principal_id,action_type,target_type,target_id,detail,performed_at,outcome)
         VALUES ($1,$2,$2,'organization_admin.bootstrap','user',$2,$3,$4,'success')",
    )
    .bind(tenant_id)
    .bind(user_id)
    .bind(json!({"grant_id":grant_id}))
    .bind(granted_at)
    .execute(&mut *tx)
    .await
    .expect("grant audit");
    sqlx::query(
        "INSERT INTO organization_administrators (id,tenant_id,user_id,granted_by,granted_at)
         VALUES ($1,$2,$3,$3,$4)",
    )
    .bind(grant_id)
    .bind(tenant_id)
    .bind(user_id)
    .bind(granted_at)
    .execute(&mut *tx)
    .await
    .expect("organization admin grant");
    tx.commit().await.expect("commit grant");
}

#[allow(clippy::too_many_arguments)]
async fn insert_invitation(
    pool: &PgPool,
    tenant_id: Uuid,
    sender_id: Uuid,
    email: &str,
    token: &str,
    display_name: Option<&str>,
    expires_at: chrono::DateTime<chrono::Utc>,
    delivery_status: &str,
    accepted: bool,
    cancelled: bool,
) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO invite_tokens
            (id,tenant_id,token,sender_id,recipient_email,recipient_display_name,
             workflow_id,expires_at,used_at,revoked_at,cancelled_at,
             bulk_delivery_status,bulk_delivery_attempted_at)
         SELECT $1,$2,$3,$4,$5,$6,id,$7,
                CASE WHEN $9 THEN clock_timestamp() ELSE NULL END,
                CASE WHEN $10 THEN clock_timestamp() ELSE NULL END,
                CASE WHEN $10 THEN clock_timestamp() ELSE NULL END,
                $8,CASE WHEN $8='sent' THEN NULL ELSE clock_timestamp() END
         FROM workflows WHERE key='invite_email' LIMIT 1",
    )
    .bind(id)
    .bind(tenant_id)
    .bind(token)
    .bind(sender_id)
    .bind(email)
    .bind(display_name)
    .bind(expires_at)
    .bind(delivery_status)
    .bind(accepted)
    .bind(cancelled)
    .execute(pool)
    .await
    .expect("insert invitation");
    id
}

fn app(state: &rustshare_server::AppState) -> Router {
    rustshare_server::routes::admin_routes()
        .merge(rustshare_server::routes::invite_routes())
        .with_state(state.clone())
}

fn assert_disposable() {
    assert_local_database();
    assert_eq!(
        std::env::var("RUSTSHARE_TEST_DISPOSABLE_DB")
            .ok()
            .as_deref(),
        Some("1")
    );
    assert_eq!(
        std::env::var("RUSTSHARE_TEST_DISPOSABLE_OBJECT_STORE")
            .ok()
            .as_deref(),
        Some("1")
    );
    let endpoint = std::env::var("S3_ENDPOINT")
        .or_else(|_| std::env::var("RUSTFS_ENDPOINT"))
        .expect("loopback object-store endpoint");
    let endpoint = url::Url::parse(&endpoint).expect("S3 endpoint URL");
    assert!(matches!(
        endpoint.host_str(),
        Some("localhost" | "127.0.0.1")
    ));
}

fn enable_local_test_smtp() {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        std::env::set_var("RUSTSHARE_ALLOW_INTERNAL_SMTP_FOR_TESTS", "true");
        std::env::set_var("RUSTSHARE_ALLOW_INTERNAL_MAIL_SERVERS", "true");
    });
}

async fn mock_smtp_once(listener: TcpListener) -> String {
    let (stream, _) = listener.accept().await.expect("accept SMTP connection");
    let (read, mut write) = tokio::io::split(stream);
    let mut read = BufReader::new(read);
    write
        .write_all(b"220 localhost ESMTP\r\n")
        .await
        .expect("SMTP greeting");
    let mut message = String::new();
    loop {
        let mut line = String::new();
        if read.read_line(&mut line).await.expect("read SMTP command") == 0 {
            break;
        }
        let command = line.to_ascii_uppercase();
        if command.starts_with("DATA") {
            write
                .write_all(b"354 end with dot\r\n")
                .await
                .expect("SMTP data");
            loop {
                let mut line = String::new();
                if read.read_line(&mut line).await.expect("read SMTP body") == 0 {
                    break;
                }
                if line == ".\r\n" || line == ".\n" {
                    break;
                }
                message.push_str(&line);
            }
            write
                .write_all(b"250 queued\r\n")
                .await
                .expect("SMTP accepted");
        } else if command.starts_with("QUIT") {
            write.write_all(b"221 bye\r\n").await.expect("SMTP quit");
            break;
        } else {
            write
                .write_all(b"250 localhost\r\n")
                .await
                .expect("SMTP response");
        }
    }
    message
}

#[tokio::test]
#[ignore = "requires explicitly configured disposable local PostgreSQL and RustFS"]
async fn invitation_list_resend_cancel_and_retry_are_tenant_safe_and_redacted() {
    let _serial = SERIAL.lock().await;
    assert_disposable();
    let state = setup_test_env().await;
    let pool = state.db_pool.clone();
    sqlx::query("UPDATE workflows SET status='active' WHERE key='invite_email'")
        .execute(&pool)
        .await
        .expect("enable invite workflow");
    sqlx::query(
        "UPDATE smtp_config SET enabled=false WHERE id='00000000-0000-0000-0000-000000000002'",
    )
    .execute(&pool)
    .await
    .expect("force deterministic SMTP failure");

    let tenant_id = Uuid::new_v4();
    let foreign_tenant_id = Uuid::new_v4();
    let admin_id = create_user(&pool, tenant_id, "inv_admin").await;
    let foreign_admin_id = create_user(&pool, foreign_tenant_id, "foreign_inv_admin").await;
    grant_organization_admin(&pool, tenant_id, admin_id).await;
    grant_organization_admin(&pool, foreign_tenant_id, foreign_admin_id).await;
    let admin_token = create_auth_token(&state, admin_id, tenant_id);
    let api = app(&state);

    let now: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&pool)
        .await
        .expect("database time");
    let sentinel_token = format!("invite-secret-{}", Uuid::new_v4().simple());
    let sent_id = insert_invitation(
        &pool,
        tenant_id,
        admin_id,
        "legacy@test.local",
        &sentinel_token,
        None,
        now + chrono::Duration::days(7),
        "sent",
        false,
        false,
    )
    .await;
    let pending_id = insert_invitation(
        &pool,
        tenant_id,
        admin_id,
        "pending@test.local",
        &Uuid::new_v4().simple().to_string(),
        Some("Pending User"),
        now + chrono::Duration::days(7),
        "sending",
        false,
        false,
    )
    .await;
    let failed_id = insert_invitation(
        &pool,
        tenant_id,
        admin_id,
        "failed@test.local",
        &Uuid::new_v4().simple().to_string(),
        Some("Failed User"),
        now + chrono::Duration::days(7),
        "failed",
        false,
        false,
    )
    .await;
    let expired_id = insert_invitation(
        &pool,
        tenant_id,
        admin_id,
        "expired@test.local",
        &Uuid::new_v4().simple().to_string(),
        Some("Expired User"),
        now - chrono::Duration::seconds(1),
        "failed",
        false,
        false,
    )
    .await;
    let accepted_id = insert_invitation(
        &pool,
        tenant_id,
        admin_id,
        "accepted@test.local",
        &Uuid::new_v4().simple().to_string(),
        Some("Accepted User"),
        now + chrono::Duration::days(7),
        "sent",
        true,
        false,
    )
    .await;
    let cancelled_id = insert_invitation(
        &pool,
        tenant_id,
        admin_id,
        "cancelled@test.local",
        &Uuid::new_v4().simple().to_string(),
        Some("Cancelled User"),
        now + chrono::Duration::days(7),
        "sent",
        false,
        true,
    )
    .await;
    let foreign_id = insert_invitation(
        &pool,
        foreign_tenant_id,
        foreign_admin_id,
        "foreign@test.local",
        &Uuid::new_v4().simple().to_string(),
        Some("Foreign User"),
        now + chrono::Duration::days(7),
        "sent",
        false,
        false,
    )
    .await;

    let (status, _, first_page) = request(
        &api,
        Method::GET,
        "/api/v1/organization-admin/invitations?page=1&per_page=2",
        Some(&admin_token),
        HeaderMap::new(),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(first_page["total"], 6);
    assert_eq!(first_page["invitations"].as_array().unwrap().len(), 2);
    assert_eq!(first_page["per_page"], 2);

    let (status, bytes, listing) = request(
        &api,
        Method::GET,
        "/api/v1/organization-admin/invitations?per_page=100",
        Some(&admin_token),
        HeaderMap::new(),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let rows = listing["invitations"].as_array().unwrap();
    let states = rows
        .iter()
        .map(|row| row["status"].as_str().unwrap())
        .collect::<std::collections::HashSet<_>>();
    let expected_states: std::collections::HashSet<_> = [
        "sent",
        "pending",
        "failed",
        "expired",
        "accepted",
        "cancelled",
    ]
    .into_iter()
    .collect();
    assert_eq!(states, expected_states);
    let serialized = String::from_utf8(bytes).expect("listing UTF-8");
    assert!(!serialized.contains(&sentinel_token));
    assert!(!serialized.contains("invite_link"));
    assert!(!serialized.contains("password"));
    assert!(rows.iter().all(|row| row.get("token").is_none()));

    let foreign_cancel = request(
        &api,
        Method::POST,
        &format!("/api/v1/organization-admin/invitations/{foreign_id}/cancel"),
        Some(&admin_token),
        HeaderMap::new(),
        None,
    )
    .await;
    let missing_cancel = request(
        &api,
        Method::POST,
        &format!(
            "/api/v1/organization-admin/invitations/{}/cancel",
            Uuid::new_v4()
        ),
        Some(&admin_token),
        HeaderMap::new(),
        None,
    )
    .await;
    assert_eq!(foreign_cancel.0, StatusCode::NOT_FOUND);
    assert_eq!(foreign_cancel.2, missing_cancel.2);
    let mut foreign_resend_headers = HeaderMap::new();
    foreign_resend_headers.insert(
        "idempotency-key",
        HeaderValue::from_static("foreign-invitation-resend"),
    );
    let foreign_resend = request(
        &api,
        Method::POST,
        &format!("/api/v1/organization-admin/invitations/{foreign_id}/resend"),
        Some(&admin_token),
        foreign_resend_headers,
        None,
    )
    .await;
    assert_eq!(foreign_resend.0, StatusCode::NOT_FOUND);
    assert_eq!(foreign_resend.2, missing_cancel.2);

    let mut no_key = HeaderMap::new();
    let no_idempotency = request(
        &api,
        Method::POST,
        &format!("/api/v1/organization-admin/invitations/{sent_id}/resend"),
        Some(&admin_token),
        no_key.clone(),
        None,
    )
    .await;
    assert_eq!(no_idempotency.0, StatusCode::BAD_REQUEST);

    no_key.insert(
        "idempotency-key",
        HeaderValue::from_static("resend-attempt-1"),
    );
    let resend_path = format!("/api/v1/organization-admin/invitations/{sent_id}/resend");
    let (first, second) = tokio::join!(
        request(
            &api,
            Method::POST,
            &resend_path,
            Some(&admin_token),
            no_key.clone(),
            None,
        ),
        request(
            &api,
            Method::POST,
            &resend_path,
            Some(&admin_token),
            no_key.clone(),
            None,
        )
    );
    assert_eq!(first.0, StatusCode::OK);
    assert_eq!(second.0, StatusCode::OK);
    assert!(first.2["replayed"] == true || second.2["replayed"] == true);
    let (status, body, resent) = if first.2["replayed"] == false {
        (first.0, first.1, first.2)
    } else {
        (second.0, second.1, second.2)
    };
    assert_eq!(status, StatusCode::OK);
    assert_eq!(resent["delivery_status"], "failed");
    assert!(!String::from_utf8(body).unwrap().contains(&sentinel_token));
    let repeated = request(
        &api,
        Method::POST,
        &format!("/api/v1/organization-admin/invitations/{sent_id}/resend"),
        Some(&admin_token),
        no_key.clone(),
        None,
    )
    .await;
    assert_eq!(repeated.2["replayed"], true);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM organization_invite_resend_attempts
             WHERE tenant_id=$1 AND invite_token_id=$2",
        )
        .bind(tenant_id)
        .bind(sent_id)
        .fetch_one(&pool)
        .await
        .expect("count idempotent attempts"),
        1
    );
    no_key.insert(
        "idempotency-key",
        HeaderValue::from_static("resend-attempt-2"),
    );
    assert_eq!(
        request(
            &api,
            Method::POST,
            &format!("/api/v1/organization-admin/invitations/{sent_id}/resend"),
            Some(&admin_token),
            no_key,
            None,
        )
        .await
        .2["delivery_status"],
        "failed"
    );

    enable_local_test_smtp();
    let smtp_listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind disposable SMTP listener");
    let smtp_port = smtp_listener.local_addr().expect("SMTP address").port();
    let smtp_task = tokio::spawn(mock_smtp_once(smtp_listener));
    sqlx::query(
        "UPDATE smtp_config
         SET enabled=true,host='127.0.0.1',port=$1,username=NULL,password_enc=NULL,
             from_address='noreply@test.local',from_name='RustShare',tls_mode='none'
         WHERE id='00000000-0000-0000-0000-000000000002'",
    )
    .bind(i32::from(smtp_port))
    .execute(&pool)
    .await
    .expect("configure loopback SMTP");
    let mut success_key = HeaderMap::new();
    success_key.insert(
        "idempotency-key",
        HeaderValue::from_static("resend-success"),
    );
    let success = request(
        &api,
        Method::POST,
        &format!("/api/v1/organization-admin/invitations/{sent_id}/resend"),
        Some(&admin_token),
        success_key,
        None,
    )
    .await;
    assert_eq!(success.0, StatusCode::OK);
    assert_eq!(success.2["delivery_status"], "sent");
    let smtp_message = smtp_task.await.expect("SMTP mock task");
    assert!(smtp_message.contains(&format!("/invite/{sentinel_token}")));
    assert!(!String::from_utf8(success.1)
        .unwrap()
        .contains(&sentinel_token));

    let group_id = Uuid::new_v4();
    sqlx::query("INSERT INTO user_groups (id,name,created_by,tenant_id) VALUES ($1,$2,$3,$4)")
        .bind(group_id)
        .bind(format!("cancel test {}", group_id.simple()))
        .bind(admin_id)
        .bind(tenant_id)
        .execute(&pool)
        .await
        .expect("create assigned group");
    sqlx::query(
        "INSERT INTO invite_token_group_assignments (invite_token_id,group_id) VALUES ($1,$2)",
    )
    .bind(sent_id)
    .bind(group_id)
    .execute(&pool)
    .await
    .expect("assign group to invite");
    let cancel = request(
        &api,
        Method::POST,
        &format!("/api/v1/organization-admin/invitations/{sent_id}/cancel"),
        Some(&admin_token),
        HeaderMap::new(),
        None,
    )
    .await;
    assert_eq!(cancel.0, StatusCode::OK);
    assert_eq!(cancel.2["group_assignments_removed"], 1);
    assert_eq!(cancel.2["status"], "cancelled");
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM invite_token_group_assignments WHERE invite_token_id=$1",
        )
        .bind(sent_id)
        .fetch_one(&pool)
        .await
        .expect("check assignments after cancellation"),
        0
    );
    let accepted_after_cancel = request(
        &api,
        Method::POST,
        &format!("/api/v1/invites/{sentinel_token}/accept"),
        None,
        HeaderMap::new(),
        Some(json!({
            "email":"legacy@test.local",
            "display_name":"Legacy User",
            "password":"test-password-123"
        })),
    )
    .await;
    assert_eq!(accepted_after_cancel.0, StatusCode::GONE);
    let repeated_cancel = request(
        &api,
        Method::POST,
        &format!("/api/v1/organization-admin/invitations/{sent_id}/cancel"),
        Some(&admin_token),
        HeaderMap::new(),
        None,
    )
    .await;
    assert_eq!(repeated_cancel.2["changed"], false);

    let audit: Vec<(String, Value)> = sqlx::query_as(
        "SELECT action_type, detail FROM admin_actions
         WHERE tenant_id=$1 AND target_id=$2
           AND action_type LIKE 'organization_admin.invitation_%'",
    )
    .bind(tenant_id)
    .bind(sent_id)
    .fetch_all(&pool)
    .await
    .expect("query lifecycle audit");
    let audit_json = serde_json::to_string(&audit).expect("serialize audit");
    assert!(!audit_json.contains(&sentinel_token));
    assert!(audit
        .iter()
        .any(|(action, _)| action == "organization_admin.invitation_cancelled"));
    assert!(audit
        .iter()
        .any(|(action, _)| action == "organization_admin.invitation_resend_failed"));

    let _ = (
        pending_id,
        failed_id,
        expired_id,
        accepted_id,
        cancelled_id,
        foreign_id,
    );
}

#[tokio::test]
#[ignore = "requires explicitly configured disposable local PostgreSQL and RustFS"]
async fn invitation_cancellation_and_acceptance_serialize_on_the_same_tenant_invite() {
    let _serial = SERIAL.lock().await;
    assert_disposable();
    let state = setup_test_env().await;
    let pool = state.db_pool.clone();
    sqlx::query("UPDATE workflows SET status='active' WHERE key='invite_email'")
        .execute(&pool)
        .await
        .expect("enable invite workflow");
    let tenant_id = Uuid::new_v4();
    let admin_id = create_user(&pool, tenant_id, "race_admin").await;
    grant_organization_admin(&pool, tenant_id, admin_id).await;
    let admin_token = create_auth_token(&state, admin_id, tenant_id);
    let email = format!("race_{}@test.local", Uuid::new_v4().simple());
    let invite_token = Uuid::new_v4().simple().to_string();
    let invitation_id = insert_invitation(
        &pool,
        tenant_id,
        admin_id,
        &email,
        &invite_token,
        Some("Race User"),
        chrono::Utc::now() + chrono::Duration::days(7),
        "sent",
        false,
        false,
    )
    .await;
    let api = app(&state);
    let cancel_path = format!("/api/v1/organization-admin/invitations/{invitation_id}/cancel");
    let accept_path = format!("/api/v1/invites/{invite_token}/accept");

    let (cancel, accept) = tokio::join!(
        request(
            &api,
            Method::POST,
            &cancel_path,
            Some(&admin_token),
            HeaderMap::new(),
            None,
        ),
        request(
            &api,
            Method::POST,
            &accept_path,
            None,
            HeaderMap::new(),
            Some(json!({
                "email":email,
                "display_name":"Race User",
                "password":"test-password-123"
            })),
        )
    );

    if cancel.0 == StatusCode::OK {
        assert_eq!(accept.0, StatusCode::GONE);
        assert_eq!(cancel.2["status"], "cancelled");
        assert!(sqlx::query_scalar::<_, bool>(
            "SELECT used_at IS NULL FROM invite_tokens WHERE id=$1"
        )
        .bind(invitation_id)
        .fetch_one(&pool)
        .await
        .expect("check canceled invitation"));
    } else {
        assert_eq!(cancel.0, StatusCode::CONFLICT);
        assert_eq!(accept.0, StatusCode::OK);
        assert!(sqlx::query_scalar::<_, bool>(
            "SELECT used_at IS NOT NULL FROM invite_tokens WHERE id=$1"
        )
        .bind(invitation_id)
        .fetch_one(&pool)
        .await
        .expect("check accepted invitation"));
    }
}

#[tokio::test]
#[ignore = "requires explicitly configured disposable local PostgreSQL and RustFS"]
async fn invitation_mutation_rolls_back_when_mandatory_audit_fails() {
    let _serial = SERIAL.lock().await;
    assert_disposable();
    let state = setup_test_env().await;
    let pool = state.db_pool.clone();
    sqlx::query("UPDATE workflows SET status='active' WHERE key='invite_email'")
        .execute(&pool)
        .await
        .expect("enable invite workflow");
    sqlx::query(
        "UPDATE smtp_config SET enabled=false WHERE id='00000000-0000-0000-0000-000000000002'",
    )
    .execute(&pool)
    .await
    .expect("disable SMTP");
    let tenant_id = Uuid::new_v4();
    let admin_id = create_user(&pool, tenant_id, "audit_admin").await;
    grant_organization_admin(&pool, tenant_id, admin_id).await;
    let admin_token = create_auth_token(&state, admin_id, tenant_id);
    let invitation_id = insert_invitation(
        &pool,
        tenant_id,
        admin_id,
        "audit@test.local",
        &Uuid::new_v4().simple().to_string(),
        Some("Audit User"),
        chrono::Utc::now() + chrono::Duration::days(7),
        "sent",
        false,
        false,
    )
    .await;
    sqlx::query(
        "CREATE FUNCTION reject_invitation_lifecycle_audit() RETURNS trigger AS $$
         BEGIN
             IF NEW.action_type LIKE 'organization_admin.invitation_%' THEN
                 RAISE EXCEPTION 'injected invitation audit failure';
             END IF;
             RETURN NEW;
         END;
         $$ LANGUAGE plpgsql",
    )
    .execute(&pool)
    .await
    .expect("create audit failure trigger function");
    sqlx::query(
        "CREATE TRIGGER reject_invitation_lifecycle_audit
         BEFORE INSERT ON admin_actions
         FOR EACH ROW EXECUTE FUNCTION reject_invitation_lifecycle_audit()",
    )
    .execute(&pool)
    .await
    .expect("create audit failure trigger");

    let api = app(&state);
    let cancel = request(
        &api,
        Method::POST,
        &format!("/api/v1/organization-admin/invitations/{invitation_id}/cancel"),
        Some(&admin_token),
        HeaderMap::new(),
        None,
    )
    .await;
    assert_eq!(cancel.0, StatusCode::INTERNAL_SERVER_ERROR);
    let invitation_state: (
        Option<chrono::DateTime<chrono::Utc>>,
        Option<chrono::DateTime<chrono::Utc>>,
        String,
    ) = sqlx::query_as(
        "SELECT cancelled_at, revoked_at, bulk_delivery_status FROM invite_tokens WHERE id=$1",
    )
    .bind(invitation_id)
    .fetch_one(&pool)
    .await
    .expect("inspect rollback");
    assert!(invitation_state.0.is_none());
    assert!(invitation_state.1.is_none());
    assert_eq!(invitation_state.2, "sent");

    let mut headers = HeaderMap::new();
    headers.insert(
        "idempotency-key",
        HeaderValue::from_static("audit-failure-resend"),
    );
    let resend = request(
        &api,
        Method::POST,
        &format!("/api/v1/organization-admin/invitations/{invitation_id}/resend"),
        Some(&admin_token),
        headers,
        None,
    )
    .await;
    assert_eq!(resend.0, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM organization_invite_resend_attempts WHERE invite_token_id=$1",
        )
        .bind(invitation_id)
        .fetch_one(&pool)
        .await
        .expect("check rolled back resend attempt"),
        0
    );
    let status: String =
        sqlx::query_scalar("SELECT bulk_delivery_status FROM invite_tokens WHERE id=$1")
            .bind(invitation_id)
            .fetch_one(&pool)
            .await
            .expect("check rolled back delivery state");
    assert_eq!(status, "sent");

    sqlx::query("DROP TRIGGER reject_invitation_lifecycle_audit ON admin_actions")
        .execute(&pool)
        .await
        .expect("drop audit failure trigger");
    sqlx::query("DROP FUNCTION reject_invitation_lifecycle_audit()")
        .execute(&pool)
        .await
        .expect("drop audit failure trigger function");
}
