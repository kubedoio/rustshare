//! HTTP/PostgreSQL coverage for tenant-safe bulk invitation onboarding.
//!
//! Run against disposable local PostgreSQL/RustFS with the same explicit
//! guards used by the other organization-administration integration tests.

mod support;

use axum::{body::Body, http::Request, Router};
use serde_json::{json, Value};
use sqlx::PgPool;
use support::calendar_harness::{assert_local_database, create_auth_token, setup_test_env, SERIAL};
use tower::ServiceExt;
use uuid::Uuid;

async fn send_json(app: &Router, uri: &str, token: &str, body: Value) -> (u16, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::post(uri)
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .expect("request"),
        )
        .await
        .expect("response");
    let status = response.status().as_u16();
    let body = axum::body::to_bytes(response.into_body(), 1_000_000)
        .await
        .expect("response body");
    (
        status,
        if body.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&body).expect("json response")
        },
    )
}

async fn send_body(app: &Router, uri: &str, token: &str, body: String) -> (u16, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::post(uri)
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(body))
                .expect("request"),
        )
        .await
        .expect("response");
    let status = response.status().as_u16();
    let body = axum::body::to_bytes(response.into_body(), 1_000_000)
        .await
        .expect("response body");
    (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
}

fn escape_ascii_json(value: &str) -> String {
    use std::fmt::Write as _;
    let mut escaped = String::with_capacity(value.len() * 6);
    for byte in value.bytes() {
        write!(escaped, "\\u{byte:04x}").expect("write escaped byte");
    }
    escaped
}

async fn create_user(pool: &PgPool, tenant_id: Uuid, suffix: &str, is_admin: bool) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id,tenant_id,username,email,password_hash,display_name,is_admin,storage_quota) VALUES ($1,$2,$3,$4,'test-hash',$3,$5,10737418240)",
    )
    .bind(id)
    .bind(tenant_id)
    .bind(format!("bulk_{suffix}_{}", id.simple()))
    .bind(format!("bulk_{}@test.local", id.simple()))
    .bind(is_admin)
    .execute(pool)
    .await
    .expect("test user");
    id
}

async fn grant_organization_admin(pool: &PgPool, tenant_id: Uuid, user_id: Uuid) {
    let grant_id = Uuid::new_v4();
    let granted_at: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT now()")
        .fetch_one(pool)
        .await
        .expect("database time");
    let mut tx = pool.begin().await.expect("grant transaction");
    sqlx::query(
        "INSERT INTO admin_actions (tenant_id,actor_id,actor_principal_id,action_type,target_type,target_id,detail,performed_at,outcome) VALUES ($1,$2,$2,'organization_admin.bootstrap','user',$2,$3,$4,'success')",
    )
    .bind(tenant_id)
    .bind(user_id)
    .bind(json!({"grant_id":grant_id}))
    .bind(granted_at)
    .execute(&mut *tx)
    .await
    .expect("grant audit");
    sqlx::query(
        "INSERT INTO organization_administrators (id,tenant_id,user_id,granted_by,granted_at) VALUES ($1,$2,$3,$3,$4)",
    )
    .bind(grant_id)
    .bind(tenant_id)
    .bind(user_id)
    .bind(granted_at)
    .execute(&mut *tx)
    .await
    .expect("organization admin grant");
    tx.commit().await.expect("grant commit");
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable local PostgreSQL database and S3-compatible storage"]
async fn bulk_invites_validate_apply_retry_and_isolate_tenants() {
    let _serial = SERIAL.lock().await;
    let database_url = std::env::var("DATABASE_URL").expect("disposable DATABASE_URL");
    let parsed = url::Url::parse(&database_url).expect("DATABASE_URL URL");
    assert!(
        parsed
            .path()
            .trim_start_matches('/')
            .starts_with("rustshare_test"),
        "refusing writes outside rustshare_test"
    );
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
        .expect("loopback S3_ENDPOINT");
    let endpoint = url::Url::parse(&endpoint).expect("S3 endpoint URL");
    assert!(matches!(
        endpoint.host_str(),
        Some("localhost" | "127.0.0.1")
    ));

    let state = setup_test_env().await;
    let pool = state.db_pool.clone();
    sqlx::query("UPDATE workflows SET status='active' WHERE key='invite_email'")
        .execute(&pool)
        .await
        .expect("activate invite workflow for HTTP apply");
    let tenant = Uuid::new_v4();
    let foreign_tenant = Uuid::new_v4();
    let admin = create_user(&pool, tenant, "admin", false).await;
    let local_existing = create_user(&pool, tenant, "existing", false).await;
    let foreign_existing = create_user(&pool, foreign_tenant, "foreign", false).await;
    let foreign_username_collision = format!("collision_{}@test.local", Uuid::new_v4().simple());
    let foreign_username_user = create_user(&pool, foreign_tenant, "username", false).await;
    sqlx::query("UPDATE users SET username=$1 WHERE id=$2")
        .bind(&foreign_username_collision)
        .bind(foreign_username_user)
        .execute(&pool)
        .await
        .expect("username collision fixture");
    grant_organization_admin(&pool, tenant, admin).await;
    let own_group = Uuid::new_v4();
    let foreign_group = Uuid::new_v4();
    for (id, group_tenant, name) in [
        (own_group, tenant, "bulk own"),
        (foreign_group, foreign_tenant, "bulk foreign"),
    ] {
        sqlx::query("INSERT INTO user_groups (id,name,created_by,tenant_id) VALUES ($1,$2,$3,$4)")
            .bind(id)
            .bind(format!("{name} {}", id.simple()))
            .bind(admin)
            .bind(group_tenant)
            .execute(&pool)
            .await
            .expect("test group");
    }
    let max_batch_groups = (0..20).map(|_| Uuid::new_v4()).collect::<Vec<_>>();
    for id in &max_batch_groups {
        sqlx::query("INSERT INTO user_groups (id,name,created_by,tenant_id) VALUES ($1,$2,$3,$4)")
            .bind(id)
            .bind(format!("bulk maximum {id}"))
            .bind(admin)
            .bind(tenant)
            .execute(&pool)
            .await
            .expect("maximum batch group");
    }
    let expired_email = format!("expired_{}@test.local", Uuid::new_v4().simple());
    let expired_invite = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO invite_tokens (id,tenant_id,token,sender_id,recipient_email,recipient_display_name,workflow_id,expires_at,bulk_delivery_status,bulk_delivery_attempted_at) SELECT $1,$2,$3,$4,$5,'Expired User',id,now()-interval '1 day','failed',now()-interval '1 day' FROM workflows WHERE key='invite_email' LIMIT 1",
    )
    .bind(expired_invite)
    .bind(tenant)
    .bind(format!("{}", Uuid::new_v4().simple()))
    .bind(admin)
    .bind(&expired_email)
    .execute(&pool)
    .await
    .expect("expired invite fixture");
    sqlx::query(
        "INSERT INTO invite_token_group_assignments (invite_token_id,group_id) VALUES ($1,$2)",
    )
    .bind(expired_invite)
    .bind(own_group)
    .execute(&pool)
    .await
    .expect("expired assignment fixture");
    let legacy_pending_email = format!("legacy_pending_{}@test.local", Uuid::new_v4().simple());
    sqlx::query(
        "INSERT INTO invite_tokens (tenant_id,token,sender_id,recipient_email,workflow_id,expires_at) SELECT $1,$2,$3,$4,id,now()+interval '7 days' FROM workflows WHERE key='invite_email' LIMIT 1",
    )
    .bind(tenant)
    .bind(Uuid::new_v4().simple().to_string())
    .bind(admin)
    .bind(&legacy_pending_email)
    .execute(&pool)
    .await
    .expect("legacy invite fixture");
    let token = create_auth_token(&state, admin, tenant);
    let app = rustshare_server::routes::admin_routes()
        .merge(rustshare_server::routes::invite_routes())
        .with_state(state);
    let bulk_uri = "/api/v1/organization-admin/users/bulk";
    let oversized = format!(
        "{{\"dry_run\":true,\"users\":[],\"ignored\":\"{}\"}}",
        "x".repeat(1024 * 1024)
    );
    assert_eq!(send_body(&app, bulk_uri, &token, oversized).await.0, 413);
    let escaped_groups = max_batch_groups
        .iter()
        .map(|id| format!("\"{}\"", escape_ascii_json(&id.to_string())))
        .collect::<Vec<_>>()
        .join(",");
    let mut max_valid_batch = String::from("{\"dry_run\":true,\"users\":[");
    for index in 0..100 {
        if index > 0 {
            max_valid_batch.push(',');
        }
        let email = format!(
            "u{index:03}{}@{}.{}.{}",
            "a".repeat(60),
            "b".repeat(63),
            "c".repeat(63),
            "d".repeat(61)
        );
        let display_name = format!("Member {index:03} {}", "x".repeat(244));
        max_valid_batch.push_str(&format!(
            "{{\"email\":\"{}\",\"display_name\":\"{}\",\"group_ids\":[{}]}}",
            escape_ascii_json(&email),
            escape_ascii_json(&display_name),
            escaped_groups
        ));
    }
    max_valid_batch.push_str("]}");
    assert!(max_valid_batch.len() < 1024 * 1024);
    let (status, max_batch_preview) = send_body(&app, bulk_uri, &token, max_valid_batch).await;
    assert_eq!(status, 200);
    assert_eq!(max_batch_preview["rows"].as_array().unwrap().len(), 100);
    assert!(max_batch_preview["rows"]
        .as_array()
        .unwrap()
        .iter()
        .all(|row| row["outcome"] == "ready"));
    let valid_email = format!("onboard_{}@test.local", Uuid::new_v4().simple());
    let foreign_existing_email: String = sqlx::query_scalar("SELECT email FROM users WHERE id=$1")
        .bind(foreign_existing)
        .fetch_one(&pool)
        .await
        .expect("foreign email");
    let local_existing_email: String = sqlx::query_scalar("SELECT email FROM users WHERE id=$1")
        .bind(local_existing)
        .fetch_one(&pool)
        .await
        .expect("local email");
    let before: (i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM invite_tokens),(SELECT count(*) FROM admin_actions WHERE action_type='organization_admin.user_invited')",
    )
    .fetch_one(&pool)
    .await
    .expect("initial counts");
    let rows = json!([
        {"email":valid_email,"display_name":"New member","group_ids":[own_group]},
        {"email":"bad address","display_name":"Bad"},
        {"email":valid_email.to_ascii_uppercase(),"display_name":"Duplicate"},
        {"email":foreign_existing_email,"display_name":"Existing elsewhere"},
        {"email":local_existing_email,"display_name":"Existing here"},
        {"email":foreign_username_collision,"display_name":"Username collision"},
        {"email":format!("group_{}@test.local",Uuid::new_v4().simple()),"display_name":"Foreign group","group_ids":[foreign_group]},
        {"email":format!("unknown_{}@test.local",Uuid::new_v4().simple()),"display_name":"Unknown group","group_ids":[Uuid::new_v4()]},
        {"email":expired_email,"display_name":"Expired User","group_ids":[own_group]},
        {"email":format!("control_{}@test.local",Uuid::new_v4().simple()),"display_name":"Invalid\nName"}
    ]);
    let (status, preview) =
        send_json(&app, bulk_uri, &token, json!({"dry_run":true,"users":rows})).await;
    assert_eq!(status, 200);
    let outcomes: Vec<&str> = preview["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["outcome"].as_str().unwrap())
        .collect();
    assert_eq!(
        outcomes,
        [
            "ready",
            "invalid_email",
            "duplicate_in_request",
            "identifier_unavailable",
            "already_exists",
            "identifier_unavailable",
            "group_unavailable",
            "group_unavailable",
            "ready",
            "invalid_display_name"
        ]
    );
    let after_preview: (i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM invite_tokens),(SELECT count(*) FROM admin_actions WHERE action_type='organization_admin.user_invited')",
    )
    .fetch_one(&pool)
    .await
    .expect("preview counts");
    assert_eq!(before, after_preview, "dry-run is read-only");

    let legacy_row = json!({"email":legacy_pending_email,"display_name":"Legacy member","group_ids":[own_group]});
    let (status, legacy_preview) = send_json(
        &app,
        bulk_uri,
        &token,
        json!({"dry_run":true,"users":[legacy_row.clone()]}),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(
        legacy_preview["rows"][0]["outcome"],
        "pending_invite_mismatch"
    );
    let (status, legacy_apply) = send_json(
        &app,
        bulk_uri,
        &token,
        json!({"dry_run":false,"users":[legacy_row]}),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(
        legacy_apply["rows"][0]["outcome"],
        "pending_invite_mismatch"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM invite_token_group_assignments a JOIN invite_tokens i ON i.id=a.invite_token_id WHERE i.tenant_id=$1 AND i.recipient_email=$2",
        )
        .bind(tenant)
        .bind(&legacy_pending_email)
        .fetch_one(&pool)
        .await
        .expect("legacy invite has no assumed group assignment"),
        0
    );

    let apply_rows = json!([
        {"email":valid_email,"display_name":"New member","group_ids":[own_group]},
        {"email":"still not an email","display_name":"Bad"},
        {"email":format!("foreign_group_{}@test.local",Uuid::new_v4().simple()),"display_name":"Foreign group","group_ids":[foreign_group]},
        {"email":expired_email,"display_name":"Expired User","group_ids":[own_group]}
    ]);
    let (status, applied) = send_json(
        &app,
        bulk_uri,
        &token,
        json!({"dry_run":false,"users":apply_rows}),
    )
    .await;
    assert_eq!(status, 200);
    assert!(matches!(
        applied["rows"][0]["outcome"].as_str(),
        Some("invited" | "delivery_failed")
    ));
    assert_eq!(applied["rows"][1]["outcome"], "invalid_email");
    assert_eq!(applied["rows"][2]["outcome"], "group_unavailable");
    assert!(matches!(
        applied["rows"][3]["outcome"].as_str(),
        Some("invited" | "delivery_failed")
    ));
    assert!(sqlx::query_scalar::<_, bool>(
        "SELECT revoked_at IS NOT NULL FROM invite_tokens WHERE id=$1"
    )
    .bind(expired_invite)
    .fetch_one(&pool)
    .await
    .expect("expired invitation replaced"),);
    let replacement_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM invite_tokens WHERE tenant_id=$1 AND lower(recipient_email)=lower($2) AND revoked_at IS NULL",
    )
    .bind(tenant)
    .bind(&expired_email)
    .fetch_one(&pool)
    .await
    .expect("replacement invitation");
    let replaced_audit: Value = sqlx::query_scalar(
        "SELECT detail FROM admin_actions WHERE target_id=$1 AND action_type='organization_admin.expired_invites_replaced'",
    )
    .bind(replacement_id)
    .fetch_one(&pool)
    .await
    .expect("expired invitation audit");
    assert_eq!(
        replaced_audit["expired_invite_ids"],
        json!([expired_invite])
    );
    assert_eq!(replaced_audit["count"], 1);
    assert!(applied.to_string().find("/invite/").is_none());
    assert!(!applied.to_string().contains(&valid_email));
    let (invite_id, invite_token): (Uuid, String) = sqlx::query_as(
        "SELECT id,token FROM invite_tokens WHERE tenant_id=$1 AND recipient_email=$2 AND used_at IS NULL AND revoked_at IS NULL",
    )
    .bind(tenant)
    .bind(&valid_email)
    .fetch_one(&pool)
    .await
    .expect("persisted invite");
    let attribution: (Uuid, Uuid, String) = sqlx::query_as(
        "SELECT tenant_id,actor_principal_id,action_type FROM admin_actions WHERE target_id=$1 AND action_type='organization_admin.user_invited'",
    )
    .bind(invite_id)
    .fetch_one(&pool)
    .await
    .expect("invitation audit");
    assert_eq!(
        attribution,
        (tenant, admin, "organization_admin.user_invited".into())
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM invite_token_group_assignments WHERE invite_token_id=$1 AND group_id=$2")
            .bind(invite_id)
            .bind(own_group)
            .fetch_one(&pool)
            .await
            .expect("pending group assignment"),
        1
    );

    let (status, retry) = send_json(
        &app,
        bulk_uri,
        &token,
        json!({"dry_run":false,"users":[{"email":valid_email,"display_name":"New member","group_ids":[own_group]}]}),
    )
    .await;
    assert_eq!(status, 200);
    assert!(matches!(
        retry["rows"][0]["outcome"].as_str(),
        Some("already_pending" | "delivery_pending" | "delivery_failed" | "delivery_retried")
    ));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM invite_tokens WHERE tenant_id=$1 AND lower(recipient_email)=lower($2) AND used_at IS NULL AND revoked_at IS NULL")
            .bind(tenant)
            .bind(&valid_email)
            .fetch_one(&pool)
            .await
            .expect("one pending invite"),
        1
    );

    let concurrent_email = format!("concurrent_{}@test.local", Uuid::new_v4().simple());
    let concurrent_body = json!({"dry_run":false,"users":[{"email":concurrent_email,"display_name":"Concurrent User"}]});
    let (first, second) = tokio::join!(
        send_json(&app, bulk_uri, &token, concurrent_body.clone()),
        send_json(&app, bulk_uri, &token, concurrent_body),
    );
    assert_eq!(first.0, 200);
    assert_eq!(second.0, 200);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM invite_tokens WHERE tenant_id=$1 AND lower(recipient_email)=lower($2) AND used_at IS NULL AND revoked_at IS NULL")
            .bind(tenant)
            .bind(concurrent_email)
            .fetch_one(&pool)
            .await
            .expect("concurrent invite count"),
        1
    );

    let accept_uri = format!("/api/v1/invites/{invite_token}/accept");
    let (status, accepted) = send_json(
        &app,
        &accept_uri,
        "",
        json!({"email":valid_email.to_ascii_uppercase(),"display_name":"New member","password":"correct-horse-battery","terms_accepted":true}),
    )
    .await;
    assert_eq!(status, 200, "{accepted}");
    let accepted_id: Uuid = accepted["id"].as_str().unwrap().parse().unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM group_members WHERE user_id=$1 AND group_id=$2"
        )
        .bind(accepted_id)
        .bind(own_group)
        .fetch_one(&pool)
        .await
        .expect("accepted group membership"),
        1
    );
    let acceptance_audit: (Uuid, Uuid, Value) = sqlx::query_as(
        "SELECT tenant_id,actor_principal_id,detail FROM admin_actions WHERE action_type='organization_admin.invite_accepted' AND target_id=$1",
    )
    .bind(accepted_id)
    .fetch_one(&pool)
    .await
    .expect("acceptance audit");
    assert_eq!(acceptance_audit.0, tenant);
    assert_eq!(acceptance_audit.1, accepted_id);
    assert_eq!(acceptance_audit.2["group_ids"], json!([own_group]));

    let deleted_group = Uuid::new_v4();
    sqlx::query("INSERT INTO user_groups (id,name,created_by,tenant_id) VALUES ($1,$2,$3,$4)")
        .bind(deleted_group)
        .bind(format!("deleted pending group {}", deleted_group.simple()))
        .bind(admin)
        .bind(tenant)
        .execute(&pool)
        .await
        .expect("pending group fixture");
    let stranded_email = format!("stranded_{}@test.local", Uuid::new_v4().simple());
    let (status, _) = send_json(
        &app,
        bulk_uri,
        &token,
        json!({"dry_run":false,"users":[{"email":stranded_email,"display_name":"Pending user","group_ids":[deleted_group]}]}),
    )
    .await;
    assert_eq!(status, 200);
    let (stranded_invite, stranded_token): (Uuid, String) = sqlx::query_as(
        "SELECT id,token FROM invite_tokens WHERE tenant_id=$1 AND recipient_email=$2",
    )
    .bind(tenant)
    .bind(&stranded_email)
    .fetch_one(&pool)
    .await
    .expect("pending invite to deleted group");
    sqlx::query("DELETE FROM user_groups WHERE id=$1")
        .bind(deleted_group)
        .execute(&pool)
        .await
        .expect("delete pending group");
    let (status, _) = send_json(
        &app,
        &format!("/api/v1/invites/{stranded_token}/accept"),
        "",
        json!({"email":stranded_email,"display_name":"Pending user","password":"correct-horse-battery","terms_accepted":true}),
    )
    .await;
    assert_eq!(status, 409);
    let failed_acceptance_state: (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM users WHERE lower(email)=lower($1)), (SELECT count(*) FROM user_sessions s JOIN users u ON u.id=s.user_id WHERE lower(u.email)=lower($1)), (SELECT count(*) FROM application_user_preferences p JOIN users u ON u.id=p.user_id WHERE lower(u.email)=lower($1)), (SELECT count(*) FROM group_members m JOIN users u ON u.id=m.user_id WHERE lower(u.email)=lower($1))",
    )
    .bind(&stranded_email)
    .fetch_one(&pool)
    .await
    .expect("failed acceptance has no partial account state");
    assert_eq!(failed_acceptance_state, (0, 0, 0, 0));
    let unavailable_audit: (Uuid, Uuid, Value) = sqlx::query_as(
        "SELECT tenant_id,actor_principal_id,detail FROM admin_actions WHERE action_type='organization_admin.invite_revoked_group_unavailable' AND target_id=$1",
    )
    .bind(stranded_invite)
    .fetch_one(&pool)
    .await
    .expect("unavailable group audit");
    assert_eq!(unavailable_audit.0, tenant);
    assert_eq!(unavailable_audit.1, admin);
    assert_eq!(unavailable_audit.2["group_ids"], json!([deleted_group]));
    assert!(sqlx::query_scalar::<_, bool>(
        "SELECT revoked_at IS NOT NULL FROM invite_tokens WHERE id=$1"
    )
    .bind(stranded_invite)
    .fetch_one(&pool)
    .await
    .expect("unusable invitation revoked"),);

    sqlx::query("DROP TRIGGER IF EXISTS fail_bulk_invite_audit ON admin_actions")
        .execute(&pool)
        .await
        .expect("clear audit failure trigger");
    sqlx::query("DROP FUNCTION IF EXISTS fail_bulk_invite_audit()")
        .execute(&pool)
        .await
        .expect("clear audit failure function");
    sqlx::query(
        r#"CREATE FUNCTION fail_bulk_invite_audit() RETURNS trigger LANGUAGE plpgsql AS $$
        BEGIN
            IF NEW.action_type='organization_admin.user_invited' AND EXISTS (
                SELECT 1 FROM invite_tokens WHERE id=NEW.target_id
                    AND recipient_email LIKE 'bulk_audit_failure_%@test.local'
            ) THEN
                RAISE EXCEPTION 'injected audit failure';
            END IF;
            RETURN NEW;
        END;
        $$"#,
    )
    .execute(&pool)
    .await
    .expect("create audit failure function");
    sqlx::query(
        "CREATE TRIGGER fail_bulk_invite_audit BEFORE INSERT ON admin_actions FOR EACH ROW EXECUTE FUNCTION fail_bulk_invite_audit()",
    )
    .execute(&pool)
    .await
    .expect("create audit failure trigger");
    let failed_email = format!("bulk_audit_failure_{}@test.local", Uuid::new_v4().simple());
    let left_email = format!("failure_left_{}@test.local", Uuid::new_v4().simple());
    let right_email = format!("failure_right_{}@test.local", Uuid::new_v4().simple());
    let (status, partial) = send_json(
        &app,
        bulk_uri,
        &token,
        json!({"dry_run":false,"users":[
            {"email":left_email,"display_name":"Left"},
            {"email":failed_email,"display_name":"Injected failure"},
            {"email":right_email,"display_name":"Right"}
        ]}),
    )
    .await;
    sqlx::query("DROP TRIGGER fail_bulk_invite_audit ON admin_actions")
        .execute(&pool)
        .await
        .expect("drop audit failure trigger");
    sqlx::query("DROP FUNCTION fail_bulk_invite_audit()")
        .execute(&pool)
        .await
        .expect("drop audit failure function");
    assert_eq!(status, 200, "{partial}");
    assert!(matches!(
        partial["rows"][0]["outcome"].as_str(),
        Some("invited" | "delivery_failed")
    ));
    assert_eq!(partial["rows"][1]["outcome"], "apply_failed");
    assert!(matches!(
        partial["rows"][2]["outcome"].as_str(),
        Some("invited" | "delivery_failed")
    ));
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM invite_tokens WHERE tenant_id=$1 AND lower(recipient_email)=lower($2)",
        )
        .bind(tenant)
        .bind(&failed_email)
        .fetch_one(&pool)
        .await
        .expect("failed row rolled back"),
        0
    );

    let hundred = (0..100)
        .map(|_| json!({"email":format!("load_{}@test.local",Uuid::new_v4().simple()),"display_name":"Load User"}))
        .collect::<Vec<_>>();
    let (status, preview) = send_json(
        &app,
        bulk_uri,
        &token,
        json!({"dry_run":true,"users":hundred}),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(preview["rows"].as_array().unwrap().len(), 100);
    assert!(preview["rows"]
        .as_array()
        .unwrap()
        .iter()
        .all(|row| row["outcome"] == "ready"));
    let (status, applied_hundred) = send_json(
        &app,
        bulk_uri,
        &token,
        json!({"dry_run":false,"users":hundred}),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(applied_hundred["rows"].as_array().unwrap().len(), 100);
    assert!(applied_hundred["rows"]
        .as_array()
        .unwrap()
        .iter()
        .all(|row| { matches!(row["outcome"].as_str(), Some("invited" | "delivery_failed")) }));
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM invite_tokens WHERE tenant_id=$1 AND recipient_email LIKE 'load_%@test.local'",
        )
        .bind(tenant)
        .fetch_one(&pool)
        .await
        .expect("100 invitations applied"),
        100
    );

    let _ = sqlx::query("DELETE FROM users WHERE id = ANY($1)")
        .bind(vec![
            admin,
            local_existing,
            foreign_existing,
            foreign_username_user,
        ])
        .execute(&pool)
        .await;
    let _ = sqlx::query("DELETE FROM user_groups WHERE id = ANY($1)")
        .bind(vec![own_group, foreign_group])
        .execute(&pool)
        .await;
}
