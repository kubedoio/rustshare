//! PostgreSQL-backed regressions for tenant organization-admin offboarding.
//!
//! Run only against disposable PostgreSQL and loopback S3/RustFS:
//! `cargo test -p rustshare-server --test organization_admin_offboarding_test -- --ignored --test-threads=1`

mod support;

use axum::{body::Body, http::Request, Router};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use support::calendar_harness::{assert_local_database, create_auth_token, setup_test_env, SERIAL};
use tower::ServiceExt;
use uuid::Uuid;

async fn request_json(
    app: &Router,
    method: &str,
    uri: &str,
    token: &str,
    body: Option<Value>,
) -> (u16, Value) {
    let mut request = Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {token}"));
    if body.is_some() {
        request = request.header("content-type", "application/json");
    }
    let response = app
        .clone()
        .oneshot(
            request
                .body(body.map_or_else(Body::empty, |b| Body::from(b.to_string())))
                .expect("request"),
        )
        .await
        .expect("response");
    let status = response.status().as_u16();
    let body = axum::body::to_bytes(response.into_body(), 1_000_000)
        .await
        .expect("body");
    (
        status,
        if body.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&body).expect("JSON")
        },
    )
}

async fn create_user(pool: &sqlx::PgPool, tenant_id: Uuid, suffix: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO users (id,tenant_id,username,email,password_hash,display_name,is_admin,storage_quota) VALUES ($1,$2,$3,$4,'test-hash',$3,false,10737418240)")
        .bind(id).bind(tenant_id).bind(format!("offboard_{suffix}_{}", id.simple()))
        .bind(format!("offboard_{}@test.local", id.simple())).execute(pool).await.expect("test user");
    id
}

async fn grant_org_admin(pool: &sqlx::PgPool, tenant_id: Uuid, user_id: Uuid) {
    let mut tx = pool.begin().await.expect("grant transaction");
    let grant_id = Uuid::new_v4();
    let granted_at = chrono::Utc::now();
    sqlx::query("INSERT INTO admin_actions (tenant_id,actor_id,actor_principal_id,action_type,target_type,target_id,detail,outcome,performed_at) VALUES ($1,$2,$2,'organization_admin.bootstrap','user',$3,$4,'success',$5)")
        .bind(tenant_id)
        .bind(user_id)
        .bind(user_id)
        .bind(json!({"grant_id": grant_id}))
        .bind(granted_at)
        .execute(&mut *tx)
        .await
        .expect("grant audit");
    sqlx::query("INSERT INTO organization_administrators (id,tenant_id,user_id,granted_by,granted_at) VALUES ($1,$2,$3,$3,$4)")
        .bind(grant_id)
        .bind(tenant_id)
        .bind(user_id)
        .bind(granted_at)
        .execute(&mut *tx)
        .await
        .expect("admin grant");
    tx.commit().await.expect("grant commit");
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable local PostgreSQL database and S3-compatible storage"]
async fn organization_admin_offboarding_is_transactional_tenant_scoped_and_retry_safe() {
    let _serial = SERIAL.lock().await;
    let database_url = std::env::var("DATABASE_URL").expect("disposable DATABASE_URL");
    let parsed = url::Url::parse(&database_url).expect("DATABASE_URL URL");
    assert!(parsed
        .path()
        .trim_start_matches('/')
        .starts_with("rustshare_test"));
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
        .expect("loopback object store");
    assert!(matches!(
        url::Url::parse(&endpoint).expect("S3 URL").host_str(),
        Some("localhost" | "127.0.0.1")
    ));

    let state = setup_test_env().await;
    let pool = state.db_pool.clone();
    let tenant = Uuid::new_v4();
    let foreign_tenant = Uuid::new_v4();
    for (id, name) in [
        (tenant, "offboarding tenant"),
        (foreign_tenant, "foreign offboarding tenant"),
    ] {
        sqlx::query(
            "INSERT INTO tenants (id,name,created_at,updated_at) VALUES ($1,$2,NOW(),NOW())",
        )
        .bind(id)
        .bind(format!("{name} {id}"))
        .execute(&pool)
        .await
        .expect("tenant");
    }
    let admin = create_user(&pool, tenant, "admin").await;
    let target = create_user(&pool, tenant, "target").await;
    let foreign_user = create_user(&pool, foreign_tenant, "foreign").await;
    grant_org_admin(&pool, tenant, admin).await;
    let group = Uuid::new_v4();
    sqlx::query("INSERT INTO user_groups (id,name,created_by,tenant_id) VALUES ($1,'offboarding group',$2,$3)")
        .bind(group).bind(admin).bind(tenant).execute(&pool).await.expect("group");
    sqlx::query("INSERT INTO group_members (group_id,user_id,added_by) VALUES ($1,$2,$3)")
        .bind(group)
        .bind(target)
        .bind(admin)
        .execute(&pool)
        .await
        .expect("membership");
    let file = Uuid::new_v4();
    let bytes = b"authored content survives offboarding";
    let storage_key = format!("rustshare-test/offboarding/{file}");
    sqlx::query("INSERT INTO files (id,name,path,size,mime_type,content_hash,storage_key,owner_id,tenant_id) VALUES ($1,'owned.md',$2,$3,'text/markdown','offboarding-hash',$4,$5,$6)")
        .bind(file).bind(format!("/owned-{file}.md")).bind(bytes.len() as i64).bind(&storage_key).bind(target).bind(tenant)
        .execute(&pool).await.expect("authored file");
    let public_share = Uuid::new_v4();
    let direct_share = Uuid::new_v4();
    let surviving_group_share = Uuid::new_v4();
    for (id, token, creator, recipient, recipient_group) in [
        (
            public_share,
            Some(format!("public-{}", Uuid::new_v4().simple())),
            target,
            None,
            None,
        ),
        (direct_share, None, admin, Some(target), None),
        (surviving_group_share, None, admin, None, Some(group)),
    ] {
        sqlx::query("INSERT INTO shares (id,file_id,share_token,created_by,permissions,recipient_user_id,recipient_group_id,tenant_id) VALUES ($1,$2,$3,$4,'View',$5,$6,$7)")
            .bind(id).bind(file).bind(token).bind(creator).bind(recipient).bind(recipient_group).bind(tenant)
            .execute(&pool).await.expect("share");
    }
    assert!(rustshare_server::handlers::ws_auth::share_is_active(
        &state,
        surviving_group_share,
        chrono::Utc::now().timestamp() + 60,
    )
    .await
    .expect("active share check"));
    assert!(!rustshare_server::handlers::ws_auth::share_is_active(
        &state,
        surviving_group_share,
        chrono::Utc::now().timestamp() - 1,
    )
    .await
    .expect("expired share session check"));
    sqlx::query("UPDATE shares SET expires_at=NOW()-INTERVAL '1 second' WHERE id=$1")
        .bind(surviving_group_share)
        .execute(&pool)
        .await
        .expect("expire share");
    assert!(!rustshare_server::handlers::ws_auth::share_is_active(
        &state,
        surviving_group_share,
        chrono::Utc::now().timestamp() + 60,
    )
    .await
    .expect("expired share check"));
    sqlx::query("UPDATE shares SET expires_at=NULL WHERE id=$1")
        .bind(surviving_group_share)
        .execute(&pool)
        .await
        .expect("restore share expiry");
    let session_token = rustshare_auth::generate_web_session_token();
    let session_hash = rustshare_auth::hash_web_session_token(&session_token);
    let session_id = Uuid::new_v4();
    sqlx::query("INSERT INTO user_sessions (id,user_id,session_token_hash,expires_at,tenant_id) VALUES ($1,$2,$3,NOW()+INTERVAL '1 hour',$4)")
        .bind(session_id).bind(target).bind(session_hash).bind(tenant).execute(&pool).await.expect("session");
    let device_token = format!("offboard-device-{}", Uuid::new_v4());
    let device_hash = hex::encode(Sha256::digest(device_token.as_bytes()));
    sqlx::query("INSERT INTO device_tokens (id,user_id,token_hash,device_name) VALUES ($1,$2,$3,'offboarding test')")
        .bind(Uuid::new_v4()).bind(target).bind(&device_hash).execute(&pool).await.expect("device token");

    let target_jwt = create_auth_token(&state, target, tenant);
    let admin_jwt = create_auth_token(&state, admin, tenant);
    let share_jwt = state
        .jwt_manager
        .encode_custom_claims(&rustshare_auth::ShareSessionClaims::new(
            public_share,
            Some(file),
            None,
            rustshare_core::domain::SharePermissions::View,
            tenant,
            3600,
        ))
        .expect("share session token");
    let app = rustshare_server::routes::admin_routes()
        .merge(rustshare_server::routes::device_management_routes())
        .merge(rustshare_server::routes::user_routes())
        .with_state(state.clone());
    let uri = "/api/v1/organization-admin/users/offboard";
    let (status, dry_run) = request_json(
        &app,
        "POST",
        uri,
        &admin_jwt,
        Some(json!({"dry_run":true,"user_ids":[target,foreign_user,target]})),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(dry_run["rows"][0]["outcome"], "ready");
    assert_eq!(dry_run["rows"][1]["outcome"], "not_found");
    assert_eq!(dry_run["rows"][2]["outcome"], "duplicate");
    assert!(
        sqlx::query_scalar::<_, bool>("SELECT disabled_at IS NULL FROM users WHERE id=$1")
            .bind(target)
            .fetch_one(&pool)
            .await
            .expect("dry run untouched")
    );
    assert_eq!(
        request_json(&app, "GET", "/api/v1/me", &target_jwt, None)
            .await
            .0,
        200
    );
    assert_eq!(
        request_json(&app, "GET", "/api/v1/me", &device_token, None)
            .await
            .0,
        200
    );

    sqlx::query("CREATE FUNCTION fail_offboarding_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.action_type='organization_admin.user_offboarded' THEN RAISE EXCEPTION 'injected audit failure'; END IF; RETURN NEW; END $$")
        .execute(&pool).await.expect("audit failure function");
    sqlx::query("CREATE TRIGGER fail_offboarding_audit BEFORE INSERT ON admin_actions FOR EACH ROW EXECUTE FUNCTION fail_offboarding_audit()")
        .execute(&pool).await.expect("audit failure trigger");
    let (status, rollback) = request_json(
        &app,
        "POST",
        uri,
        &admin_jwt,
        Some(json!({"dry_run":false,"user_ids":[target]})),
    )
    .await;
    sqlx::query("DROP TRIGGER fail_offboarding_audit ON admin_actions")
        .execute(&pool)
        .await
        .expect("drop audit trigger");
    sqlx::query("DROP FUNCTION fail_offboarding_audit()")
        .execute(&pool)
        .await
        .expect("drop audit function");
    assert_eq!(status, 200);
    assert_eq!(rollback["rows"][0]["outcome"], "apply_failed");
    assert!(
        sqlx::query_scalar::<_, bool>("SELECT disabled_at IS NULL FROM users WHERE id=$1")
            .bind(target)
            .fetch_one(&pool)
            .await
            .expect("rollback user")
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM user_sessions WHERE id=$1")
            .bind(session_id)
            .fetch_one(&pool)
            .await
            .expect("rollback session"),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM shares WHERE id=ANY($1) AND revoked_at IS NULL"
        )
        .bind(&[public_share, direct_share, surviving_group_share][..])
        .fetch_one(&pool)
        .await
        .expect("rollback shares"),
        3
    );

    let (status, applied) = request_json(
        &app,
        "POST",
        uri,
        &admin_jwt,
        Some(json!({"dry_run":false,"user_ids":[target,foreign_user,target,admin]})),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(applied["rows"][0]["outcome"], "disabled");
    assert_eq!(applied["rows"][1]["outcome"], "not_found");
    assert_eq!(applied["rows"][2]["outcome"], "duplicate");
    assert_eq!(applied["rows"][3]["outcome"], "self_offboarding_rejected");
    assert!(sqlx::query_scalar::<_, bool>("SELECT disabled_at IS NOT NULL AND credentials_revoked_at IS NOT NULL FROM users WHERE id=$1").bind(target).fetch_one(&pool).await.expect("disabled and revocation timestamp"));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM user_sessions WHERE id=$1")
            .bind(session_id)
            .fetch_one(&pool)
            .await
            .expect("session revoked"),
        0
    );
    assert!(sqlx::query_scalar::<_, bool>(
        "SELECT revoked_at IS NOT NULL FROM device_tokens WHERE user_id=$1 AND token_hash=$2"
    )
    .bind(target)
    .bind(&device_hash)
    .fetch_one(&pool)
    .await
    .expect("device token revoked"));
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM shares WHERE id=ANY($1) AND revoked_at IS NOT NULL"
        )
        .bind(&[public_share, direct_share][..])
        .fetch_one(&pool)
        .await
        .expect("user capability revocation"),
        2
    );
    assert!(
        sqlx::query_scalar::<_, bool>("SELECT revoked_at IS NULL FROM shares WHERE id=$1")
            .bind(surviving_group_share)
            .fetch_one(&pool)
            .await
            .expect("other group members retain group grant")
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM group_members WHERE group_id=$1 AND user_id=$2"
        )
        .bind(group)
        .bind(target)
        .fetch_one(&pool)
        .await
        .expect("membership removed"),
        0
    );
    let preserved: (Uuid, String, String) =
        sqlx::query_as("SELECT owner_id,storage_key,name FROM files WHERE id=$1")
            .bind(file)
            .fetch_one(&pool)
            .await
            .expect("owned content preserved");
    assert_eq!(preserved, (target, storage_key, "owned.md".into()));
    assert_eq!(sqlx::query_scalar::<_, i64>("SELECT count(*) FROM admin_actions WHERE tenant_id=$1 AND action_type='organization_admin.user_offboarded' AND target_id=$2").bind(tenant).bind(target).fetch_one(&pool).await.expect("mandatory audit"), 1);
    assert_eq!(
        request_json(&app, "GET", "/api/v1/me", &target_jwt, None)
            .await
            .0,
        401
    );
    assert_eq!(
        request_json(&app, "GET", "/api/v1/me", &device_token, None)
            .await
            .0,
        401
    );
    assert!(
        rustshare_server::handlers::ws_auth::validate_client_token(&share_jwt, &state)
            .await
            .is_err(),
        "revoked share JWT must not establish a WebSocket"
    );

    sqlx::query("UPDATE users SET disabled_at=NULL WHERE id=$1")
        .bind(target)
        .execute(&pool)
        .await
        .expect("simulate authorized reactivation");
    assert_eq!(
        request_json(&app, "GET", "/api/v1/user/devices", &target_jwt, None)
            .await
            .0,
        401,
        "old JWT must stay invalid after reactivation"
    );
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    let fresh_jwt = state
        .jwt_manager
        .generate(target, "test@example.com", tenant)
        .expect("fresh JWT");
    assert_eq!(
        request_json(&app, "GET", "/api/v1/user/devices", &fresh_jwt, None)
            .await
            .0,
        200,
        "new JWT is accepted after reactivation"
    );

    // Concurrent retries serialize on the tenant grant lock and remain idempotent.
    let concurrent = create_user(&pool, tenant, "concurrent").await;
    let body = Some(json!({"dry_run":false,"user_ids":[concurrent]}));
    let (one, two) = tokio::join!(
        request_json(&app, "POST", uri, &admin_jwt, body.clone()),
        request_json(&app, "POST", uri, &admin_jwt, body),
    );
    assert_eq!(one.0, 200);
    assert_eq!(two.0, 200);
    let outcomes = [
        one.1["rows"][0]["outcome"].as_str().unwrap(),
        two.1["rows"][0]["outcome"].as_str().unwrap(),
    ];
    assert!(outcomes.contains(&"disabled") && outcomes.contains(&"already_disabled"));
    assert_eq!(sqlx::query_scalar::<_, i64>("SELECT count(*) FROM admin_actions WHERE tenant_id=$1 AND target_id=$2 AND action_type='organization_admin.user_offboarded'").bind(tenant).bind(concurrent).fetch_one(&pool).await.expect("one offboarding event"), 1);

    sqlx::query("DELETE FROM shares WHERE tenant_id=$1")
        .bind(tenant)
        .execute(&pool)
        .await
        .expect("cleanup shares");
    sqlx::query("DELETE FROM files WHERE tenant_id=$1")
        .bind(tenant)
        .execute(&pool)
        .await
        .expect("cleanup files");
    sqlx::query("DELETE FROM user_groups WHERE tenant_id=$1")
        .bind(tenant)
        .execute(&pool)
        .await
        .expect("cleanup groups");
    support::calendar_harness::cleanup_tenant(&pool, tenant).await;
    support::calendar_harness::cleanup_tenant(&pool, foreign_tenant).await;
}
