//! PostgreSQL-backed regressions for tenant organization-admin offboarding.
//!
//! Run only against disposable PostgreSQL and loopback S3/RustFS:
//! `cargo test -p rustshare-server --test organization_admin_offboarding_test -- --ignored --test-threads=1`

mod support;

use axum::{body::Body, http::Request, Router};
use futures_util::StreamExt;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use support::calendar_harness::{assert_local_database, create_auth_token, setup_test_env, SERIAL};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
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
    let pending_device_code = format!("offboard-device-{}", Uuid::new_v4());
    sqlx::query("INSERT INTO device_pair_requests (device_code,user_code,user_id,expires_at,approved_at) VALUES ($1,$2,$3,now()+interval '5 minutes',now())")
        .bind(&pending_device_code)
        .bind(format!("user-{}", Uuid::new_v4()))
        .bind(target)
        .execute(&pool)
        .await
        .expect("approved pending device request");

    let target_jwt = create_auth_token(&state, target, tenant);
    let mut future_iat_claims = state
        .jwt_manager
        .validate(&target_jwt)
        .expect("target JWT claims");
    future_iat_claims.iat += 60;
    let future_iat_jwt = state
        .jwt_manager
        .encode_custom_claims(&future_iat_claims)
        .expect("future-issued test token");
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
        .merge(rustshare_server::routes::device_auth_routes())
        .merge(rustshare_server::routes::device_management_routes())
        .merge(rustshare_server::routes::user_routes())
        .with_state(state.clone());
    let ws_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind websocket test listener");
    let ws_address = ws_listener.local_addr().expect("websocket address");
    let ws_app = rustshare_server::routes::sync_routes().with_state(state.clone());
    let ws_server = tokio::spawn(async move {
        axum::serve(ws_listener, ws_app)
            .await
            .expect("websocket test server")
    });
    let stale_session_user = create_user(&pool, tenant, "stale-websocket-session").await;
    let (stale_session_token, _) = rustshare_server::web_session::create_user_session(
        &state,
        stale_session_user,
        tenant,
        None,
        None,
    )
    .await
    .expect("create websocket session");
    let mut session_lifecycle = pool.begin().await.expect("begin session lifecycle");
    sqlx::query("UPDATE users SET disabled_at=now() WHERE id=$1")
        .bind(stale_session_user)
        .execute(&mut *session_lifecycle)
        .await
        .expect("disable websocket session user");
    sqlx::query("UPDATE users SET disabled_at=NULL WHERE id=$1")
        .bind(stale_session_user)
        .execute(&mut *session_lifecycle)
        .await
        .expect("reactivate websocket session user");
    session_lifecycle
        .commit()
        .await
        .expect("commit websocket session lifecycle");
    let mut stale_session_request = format!("ws://{ws_address}/api/ws")
        .into_client_request()
        .expect("stale session websocket request");
    stale_session_request.headers_mut().insert(
        axum::http::header::COOKIE,
        format!("rustshare_session={stale_session_token}")
            .parse()
            .expect("stale session cookie"),
    );
    assert!(
        tokio_tungstenite::connect_async(stale_session_request)
            .await
            .is_err(),
        "revoked session cannot establish a WebSocket after reactivation"
    );
    let mut ws_request = format!("ws://{ws_address}/api/ws")
        .into_client_request()
        .expect("websocket request");
    ws_request.headers_mut().insert(
        axum::http::header::AUTHORIZATION,
        format!("Bearer {target_jwt}")
            .parse()
            .expect("websocket authorization"),
    );
    let (mut offboarding_socket, _) = tokio_tungstenite::connect_async(ws_request)
        .await
        .expect("connect authenticated websocket");
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
    let websocket_close =
        tokio::time::timeout(std::time::Duration::from_secs(3), offboarding_socket.next())
            .await
            .expect("offboarding closes the existing websocket");
    assert!(matches!(
        websocket_close,
        None | Some(Err(_)) | Some(Ok(tokio_tungstenite::tungstenite::Message::Close(_)))
    ));
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
        rustshare_server::web_session::generate_user_jwt(
            &state,
            target,
            "test@example.com",
            tenant
        )
        .await
        .expect_err("disabled user cannot receive a new JWT"),
        "account_disabled"
    );
    assert_eq!(
        rustshare_server::web_session::create_user_session(&state, target, tenant, None, None,)
            .await
            .expect_err("disabled user cannot receive a new browser session"),
        "account_disabled"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM device_pair_requests WHERE device_code=$1",
        )
        .bind(&pending_device_code)
        .fetch_one(&pool)
        .await
        .expect("pending pair requests removed by disable"),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM device_tokens WHERE user_id=$1 AND revoked_at IS NULL"
        )
        .bind(target)
        .fetch_one(&pool)
        .await
        .expect("no late device token"),
        0
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
        request_json(&app, "GET", "/api/v1/me", &future_iat_jwt, None)
            .await
            .0,
        401,
        "credential generation invalidates tokens despite app/database clock skew"
    );
    let (device_poll_status, device_poll) = request_json(
        &app,
        "POST",
        "/api/v1/auth/device/poll",
        "",
        Some(json!({"device_code":pending_device_code})),
    )
    .await;
    assert_eq!(device_poll_status, 200);
    assert_eq!(device_poll["status"], "expired");
    assert_eq!(
        request_json(&app, "GET", "/api/v1/user/devices", &target_jwt, None)
            .await
            .0,
        401,
        "old JWT must stay invalid after reactivation"
    );
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    let fresh_jwt = rustshare_server::web_session::generate_user_jwt(
        &state,
        target,
        "test@example.com",
        tenant,
    )
    .await
    .expect("fresh JWT");
    assert_eq!(
        request_json(&app, "GET", "/api/v1/user/devices", &fresh_jwt, None)
            .await
            .0,
        200,
        "new JWT is accepted after reactivation"
    );

    let cutoff_user = create_user(&pool, tenant, "cutoff-race").await;
    let mut tenant_lock = pool.begin().await.expect("begin tenant lock");
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1::text, 0))")
        .bind(tenant)
        .execute(&mut *tenant_lock)
        .await
        .expect("hold organization admin lock");
    let offboarding_app = app.clone();
    let offboarding_token = admin_jwt.clone();
    let pending_offboarding = tokio::spawn(async move {
        request_json(
            &offboarding_app,
            "POST",
            uri,
            &offboarding_token,
            Some(json!({"dry_run":false,"user_ids":[cutoff_user]})),
        )
        .await
    });
    let mut is_waiting_for_tenant_lock = false;
    for _ in 0..100 {
        is_waiting_for_tenant_lock = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE wait_event_type='Lock' AND query LIKE '%pg_advisory_xact_lock%')",
        )
        .fetch_one(&pool)
        .await
        .expect("observe waiting offboarding");
        if is_waiting_for_tenant_lock {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(
        is_waiting_for_tenant_lock,
        "offboarding waits on the held tenant lock"
    );
    let late_jwt = rustshare_server::web_session::generate_user_jwt(
        &state,
        cutoff_user,
        "test@example.com",
        tenant,
    )
    .await
    .expect("issue JWT after offboarding transaction started");
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    tenant_lock
        .commit()
        .await
        .expect("release tenant admin lock");
    let (offboarding_status, offboarding_response) =
        pending_offboarding.await.expect("offboarding task");
    assert_eq!(offboarding_status, 200);
    assert_eq!(offboarding_response["rows"][0]["outcome"], "disabled");
    sqlx::query("UPDATE users SET disabled_at=NULL WHERE id=$1")
        .bind(cutoff_user)
        .execute(&pool)
        .await
        .expect("simulate reactivation");
    assert_eq!(
        request_json(&app, "GET", "/api/v1/user/devices", &late_jwt, None)
            .await
            .0,
        401,
        "JWT issued while offboarding waited must remain revoked after reactivation"
    );

    let stale_poll_user = create_user(&pool, tenant, "stale-device-poll").await;
    let stale_device_code = format!("stale-poll-{}", Uuid::new_v4());
    sqlx::query("INSERT INTO device_pair_requests (device_code,user_code,user_id,expires_at,approved_at) VALUES ($1,$2,$3,now()+interval '5 minutes',now())")
        .bind(&stale_device_code)
        .bind(format!("user-{}", Uuid::new_v4()))
        .bind(stale_poll_user)
        .execute(&pool)
        .await
        .expect("seed approved device poll");
    let mut user_lock = pool.begin().await.expect("begin device user lock");
    sqlx::query("SELECT id FROM users WHERE id=$1 FOR UPDATE")
        .bind(stale_poll_user)
        .execute(&mut *user_lock)
        .await
        .expect("hold device user lock");
    let poll_app = app.clone();
    let poll_device_code = stale_device_code.clone();
    let pending_poll = tokio::spawn(async move {
        request_json(
            &poll_app,
            "POST",
            "/api/v1/auth/device/poll",
            "",
            Some(json!({"device_code":poll_device_code})),
        )
        .await
    });
    let mut poll_is_waiting = false;
    for _ in 0..100 {
        poll_is_waiting = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE wait_event_type='Lock' AND query LIKE '%disabled_at IS NULL FROM users%')",
        )
        .fetch_one(&pool)
        .await
        .expect("observe blocked device poll");
        if poll_is_waiting {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(
        poll_is_waiting,
        "device poll waits for account lifecycle lock"
    );
    sqlx::query("UPDATE users SET disabled_at=now() WHERE id=$1")
        .bind(stale_poll_user)
        .execute(&mut *user_lock)
        .await
        .expect("disable during device poll");
    sqlx::query("UPDATE users SET disabled_at=NULL WHERE id=$1")
        .bind(stale_poll_user)
        .execute(&mut *user_lock)
        .await
        .expect("reactivate during device poll");
    user_lock
        .commit()
        .await
        .expect("commit device user lifecycle");
    let (poll_status, poll_response) = pending_poll.await.expect("device poll task");
    assert_eq!(poll_status, 200);
    assert_eq!(poll_response["status"], "expired");
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM device_tokens WHERE user_id=$1 AND revoked_at IS NULL",
        )
        .bind(stale_poll_user)
        .fetch_one(&pool)
        .await
        .expect("no token from stale device approval"),
        0
    );

    let racing_user = create_user(&pool, tenant, "racing-login").await;
    let mut disable_tx = pool.begin().await.expect("begin concurrent disable");
    sqlx::query("SELECT id FROM users WHERE id=$1 FOR UPDATE")
        .bind(racing_user)
        .execute(&mut *disable_tx)
        .await
        .expect("serialize account disable");
    let issue_state = state.clone();
    let pending_jwt = tokio::spawn(async move {
        rustshare_server::web_session::generate_user_jwt(
            &issue_state,
            racing_user,
            "test@example.com",
            tenant,
        )
        .await
    });
    let session_state = state.clone();
    let pending_session = tokio::spawn(async move {
        rustshare_server::web_session::create_user_session(
            &session_state,
            racing_user,
            tenant,
            None,
            None,
        )
        .await
    });
    sqlx::query("UPDATE users SET disabled_at=now() WHERE id=$1")
        .bind(racing_user)
        .execute(&mut *disable_tx)
        .await
        .expect("disable racing login user");
    disable_tx
        .commit()
        .await
        .expect("commit concurrent disable");
    assert_eq!(
        pending_jwt.await.expect("JWT issuer task").unwrap_err(),
        "account_disabled"
    );
    assert_eq!(
        pending_session
            .await
            .expect("session issuer task")
            .unwrap_err(),
        "account_disabled"
    );

    // A device approval authenticated before disable must not survive a
    // disable/reactivation cycle while waiting for the same user lock.
    let approval_user = create_user(&pool, tenant, "stale-device-approval").await;
    let approval_jwt = create_auth_token(&state, approval_user, tenant);
    let fallback_code = format!("fallback-{}", Uuid::new_v4());
    sqlx::query("INSERT INTO device_pair_requests (device_code,user_code,expires_at) VALUES ($1,$2,now()+interval '5 minutes')")
        .bind(format!("fallback-device-{}", Uuid::new_v4()))
        .bind(&fallback_code)
        .execute(&pool)
        .await
        .expect("pending fallback approval");
    let fallback_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/device/approve")
                .header("authorization", format!("Bearer {approval_jwt}"))
                .header("cookie", "rustshare_session=stale-session")
                .header("content-type", "application/json")
                .body(Body::from(json!({"user_code":fallback_code}).to_string()))
                .expect("fallback approval request"),
        )
        .await
        .expect("fallback approval response");
    assert_eq!(fallback_response.status().as_u16(), 200);
    let approval_code = format!("stale-approval-{}", Uuid::new_v4());
    sqlx::query("INSERT INTO device_pair_requests (device_code,user_code,expires_at) VALUES ($1,$2,now()+interval '5 minutes')")
        .bind(&approval_code)
        .bind(format!("user-{}", Uuid::new_v4()))
        .execute(&pool)
        .await
        .expect("pending device approval");
    let mut approval_lock = pool.begin().await.expect("begin approval lock");
    sqlx::query("SELECT id FROM users WHERE id=$1 FOR UPDATE")
        .bind(approval_user)
        .execute(&mut *approval_lock)
        .await
        .expect("hold approval user lock");
    let approval_app = app.clone();
    let approval_token = approval_jwt.clone();
    let approval_request_code = approval_code.clone();
    let pending_approval = tokio::spawn(async move {
        request_json(
            &approval_app,
            "POST",
            "/api/v1/auth/device/approve",
            &approval_token,
            Some(json!({"device_code":approval_request_code})),
        )
        .await
    });
    let mut approval_is_waiting = false;
    for _ in 0..100 {
        approval_is_waiting = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE wait_event_type='Lock' AND query LIKE '%disabled_at IS NULL AND tenant_id = $2%')",
        )
        .fetch_one(&pool)
        .await
        .expect("observe blocked device approval");
        if approval_is_waiting {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(
        approval_is_waiting,
        "device approval waits for user lifecycle lock"
    );
    sqlx::query("UPDATE users SET disabled_at=now() WHERE id=$1")
        .bind(approval_user)
        .execute(&mut *approval_lock)
        .await
        .expect("disable while approval waits");
    sqlx::query("UPDATE users SET disabled_at=NULL WHERE id=$1")
        .bind(approval_user)
        .execute(&mut *approval_lock)
        .await
        .expect("reactivate while approval waits");
    approval_lock
        .commit()
        .await
        .expect("commit approval user lifecycle");
    assert_eq!(pending_approval.await.expect("approval request").0, 401);
    assert!(
        sqlx::query_scalar::<_, bool>(
            "SELECT user_id IS NULL AND approved_at IS NULL FROM device_pair_requests WHERE device_code=$1",
        )
        .bind(&approval_code)
        .fetch_one(&pool)
        .await
        .expect("approval remains pending")
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
    ws_server.abort();
}
