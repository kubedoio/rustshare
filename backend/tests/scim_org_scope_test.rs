//! Organization-scoped SCIM regression (security hotfix, ADR-0040 D5).
//!
//! The SCIM v1/v2 bearer token is bound to exactly one organization, supplied
//! by `RUSTSHARE_SCIM_TENANT_ID` (defaulting to an explicitly configured
//! `RUSTSHARE_DEFAULT_TENANT_ID`). Every list, count, read, create, update,
//! disable and group-membership operation must stay inside that organization,
//! and a missing/invalid organization scope must fail closed instead of
//! silently operating installation-wide.
//!
//! Run against the explicitly configured disposable local PostgreSQL and
//! RustFS services:
//! `cargo test -p rustshare-server --test scim_org_scope_test -- --ignored --test-threads=1`

mod support;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use sqlx::PgPool;
use support::calendar_harness::{assert_local_database, setup_test_env, SERIAL};
use support::env_guard::EnvGuard;
use tower::ServiceExt;
use uuid::Uuid;

const TEST_TOKEN: &str = "scim-org-scope-test-token";
const RETRY_ATTEMPTS: usize = 3;

fn scope_env(scope: Uuid) -> EnvGuard {
    EnvGuard::set(vec![
        ("RUSTSHARE_SCIM_BEARER_TOKEN", Some(TEST_TOKEN.to_string())),
        ("RUSTSHARE_SCIM_TENANT_ID", Some(scope.to_string())),
        ("RUSTSHARE_DEFAULT_TENANT_ID", None),
    ])
}

struct Fixture {
    org_a: Uuid,
    org_b: Uuid,
    user_a1: Uuid,
    user_a2: Uuid,
    user_b1: Uuid,
    group_a: Uuid,
    group_b: Uuid,
    username_a1: String,
    user_a1_external: String,
    user_a2_external: String,
    user_b1_external: String,
    group_a_name: String,
    group_a_external: String,
    group_b_external: String,
    username_b1: String,
    display_b1: String,
    group_b_name: String,
}

/// Two synthetic organizations: `org_a` (the configured SCIM scope) and
/// `org_b` (the cross-organization target that must remain unreachable).
async fn seed_two_organizations(pool: &PgPool) -> Fixture {
    let org_a = Uuid::new_v4();
    let org_b = Uuid::new_v4();
    let user_a1 = Uuid::new_v4();
    let user_a2 = Uuid::new_v4();
    let user_b1 = Uuid::new_v4();
    let group_a = Uuid::new_v4();
    let group_b = Uuid::new_v4();
    let run = Uuid::new_v4();
    let username_a1 = format!("scim-user-a1-{run}");
    let user_a1_external = format!("scim-a1-{run}");
    let user_a2_external = format!("scim-a2-{run}");
    let user_b1_external = format!("scim-b1-{run}");
    let group_a_name = format!("scim-group-a-{run}");
    let group_a_external = format!("scim-ga-{run}");
    let group_b_external = format!("scim-gb-{run}");
    let username_b1 = format!("scim-user-b1-{run}");
    let display_b1 = format!("SCIM Org B User {run}");
    let group_b_name = format!("scim-group-b-{run}");

    for (tenant, label) in [(org_a, "A"), (org_b, "B")] {
        sqlx::query(
            "INSERT INTO tenants (id, name, created_at, updated_at)
             VALUES ($1, $2, NOW(), NOW()) ON CONFLICT (id) DO NOTHING",
        )
        .bind(tenant)
        .bind(format!("SCIM scope test org {label} {tenant}"))
        .execute(pool)
        .await
        .expect("failed to create test tenant");
    }

    let insert_user =
        |id: Uuid, username: &str, display_name: &str, tenant: Uuid, external_id: &str| {
            let username = username.to_string();
            let display_name = display_name.to_string();
            let external_id = external_id.to_string();
            async move {
                sqlx::query(
                    "INSERT INTO users
                        (id, username, email, password_hash, display_name,
                         storage_quota, tenant_id, external_id)
                     VALUES ($1, $2, $3, 'test-password-hash', $4, 10737418240, $5, $6)",
                )
                .bind(id)
                .bind(username.clone())
                .bind(format!("{username}@test.local"))
                .bind(display_name)
                .bind(tenant)
                .bind(external_id)
                .execute(pool)
                .await
                .expect("failed to create test user");
            }
        };

    insert_user(
        user_a1,
        &username_a1,
        &format!("SCIM Org A User One {run}"),
        org_a,
        &user_a1_external,
    )
    .await;
    insert_user(
        user_a2,
        &format!("scim-user-a2-{run}"),
        &format!("SCIM Org A User Two {run}"),
        org_a,
        &user_a2_external,
    )
    .await;
    insert_user(user_b1, &username_b1, &display_b1, org_b, &user_b1_external).await;

    sqlx::query(
        "INSERT INTO user_groups (id, name, external_id, tenant_id) VALUES ($1, $2, $3, $4)",
    )
    .bind(group_a)
    .bind(group_a_name.clone())
    .bind(group_a_external.clone())
    .bind(org_a)
    .execute(pool)
    .await
    .expect("failed to create test group A");
    sqlx::query(
        "INSERT INTO user_groups (id, name, external_id, tenant_id) VALUES ($1, $2, $3, $4)",
    )
    .bind(group_b)
    .bind(group_b_name.clone())
    .bind(group_b_external.clone())
    .bind(org_b)
    .execute(pool)
    .await
    .expect("failed to create test group B");

    for (group, user) in [(group_a, user_a1), (group_b, user_b1)] {
        sqlx::query("INSERT INTO group_members (group_id, user_id) VALUES ($1, $2)")
            .bind(group)
            .bind(user)
            .execute(pool)
            .await
            .expect("failed to create test group membership");
    }

    Fixture {
        org_a,
        org_b,
        user_a1,
        user_a2,
        user_b1,
        group_a,
        group_b,
        username_a1,
        user_a1_external,
        user_a2_external,
        user_b1_external,
        group_a_name,
        group_a_external,
        group_b_external,
        username_b1,
        display_b1,
        group_b_name,
    }
}

async fn seed_org_admin_grant(pool: &PgPool, tenant_id: Uuid, user_id: Uuid, actor_id: Uuid) {
    let mut tx = pool.begin().await.expect("begin grant fixture transaction");
    let history_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM organization_administrators WHERE tenant_id = $1)",
    )
    .bind(tenant_id)
    .fetch_one(&mut *tx)
    .await
    .expect("read grant fixture history");
    let active_admin_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM organization_administrators oa
         JOIN users u ON u.id = oa.user_id AND u.tenant_id = oa.tenant_id
         WHERE oa.tenant_id = $1 AND oa.revoked_at IS NULL AND u.disabled_at IS NULL)",
    )
    .bind(tenant_id)
    .fetch_one(&mut *tx)
    .await
    .expect("read active grant fixture history");
    let action_type = if !history_exists {
        "organization_admin.bootstrap"
    } else if !active_admin_exists {
        "organization_admin.recovered"
    } else {
        "organization_admin.granted"
    };
    let grant_id: Uuid = sqlx::query_scalar(
        "INSERT INTO organization_administrators (tenant_id, user_id, granted_by)
         VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(tenant_id)
    .bind(user_id)
    .bind(actor_id)
    .fetch_one(&mut *tx)
    .await
    .expect("seed organization administrator grant");
    sqlx::query(
        "INSERT INTO admin_actions
            (tenant_id, actor_id, actor_principal_id, action_type, target_type,
             target_id, detail, outcome)
         VALUES ($1, $2, $2, $4,
                 'organization_administrator', $3, $5, 'success')",
    )
    .bind(tenant_id)
    .bind(actor_id)
    .bind(user_id)
    .bind(action_type)
    .bind(json!({ "grant_id": grant_id }))
    .execute(&mut *tx)
    .await
    .expect("seed grant audit record");
    tx.commit().await.expect("commit grant fixture transaction");
}

async fn scim_request(
    app: &axum::Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {TEST_TOKEN}"),
        )
        .header("x-request-id", "scim-test-correlation");
    let request = match body {
        Some(body) => {
            builder = builder.header(axum::http::header::CONTENT_TYPE, "application/json");
            builder.body(Body::from(serde_json::to_string(&body).unwrap()))
        }
        None => builder.body(Body::empty()),
    }
    .expect("failed to build request");

    let response = app.clone().oneshot(request).await.expect("request failed");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("failed to read response body");
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, value)
}

async fn user_disabled(pool: &PgPool, user_id: Uuid) -> bool {
    sqlx::query_scalar::<_, bool>("SELECT disabled_at IS NOT NULL FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_one(pool)
        .await
        .expect("failed to read user disabled state")
}

async fn user_tenant(pool: &PgPool, username: &str) -> Option<Uuid> {
    sqlx::query_scalar::<_, Uuid>("SELECT tenant_id FROM users WHERE username = $1")
        .bind(username)
        .fetch_optional(pool)
        .await
        .expect("failed to read user tenant")
}

async fn group_member_exists(pool: &PgPool, group_id: Uuid, user_id: Uuid) -> bool {
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM group_members WHERE group_id = $1 AND user_id = $2)",
    )
    .bind(group_id)
    .bind(user_id)
    .fetch_one(pool)
    .await
    .expect("failed to read group membership")
}

fn usernames(body: &Value) -> Vec<String> {
    body["Resources"]
        .as_array()
        .map(|resources| {
            resources
                .iter()
                .filter_map(|resource| resource["userName"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Positive and negative SCIM v2 operations against two organizations.
#[tokio::test]
#[ignore = "requires explicitly configured disposable local PostgreSQL and RustFS"]
async fn scim_v2_is_scoped_to_the_configured_organization() {
    let _serial = SERIAL.lock().await;
    assert_local_database();
    let state = setup_test_env().await;
    let pool = state.db_pool.clone();
    let app = rustshare_server::routes::scim_routes().with_state(state.clone());
    let fixture = seed_two_organizations(&pool).await;
    let _env = scope_env(fixture.org_a);

    // --- Positive: same-organization operations keep working ---
    let (status, body) = scim_request(&app, "GET", "/scim/v2/Users", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["totalResults"].as_i64(),
        Some(2),
        "list must count only org A users: {body}"
    );
    let names = usernames(&body);
    assert!(
        names.iter().any(|n| n.contains("scim-user-a1")),
        "expected the first org A fixture user in the scoped list"
    );
    assert!(
        names.iter().any(|n| n.contains("scim-user-a2")),
        "expected the second org A fixture user in the scoped list"
    );

    let (status, _) = scim_request(
        &app,
        "GET",
        &format!("/scim/v2/Users/{}", fixture.user_a1),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "same-org user read must succeed");

    let (status, body) = scim_request(&app, "GET", "/scim/v2/Groups", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["totalResults"].as_i64(),
        Some(1),
        "group list must count only org A groups: {body}"
    );

    // Create resolves the organization explicitly to the configured scope.
    let create_name = format!("scim-create-{}", Uuid::new_v4());
    let (status, _) = scim_request(
        &app,
        "POST",
        "/scim/v2/Users",
        Some(json!({
            "schemas": ["urn:ietf:params:scim:schemas:core:2.0:User"],
            "userName": create_name,
            "externalId": format!("scim-create-ext-{}", Uuid::new_v4()),
            "active": true
        })),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "same-org user create must succeed"
    );
    assert_eq!(
        user_tenant(&pool, &create_name).await,
        Some(fixture.org_a),
        "created user must land in the configured organization"
    );

    // Update (PUT) and disable/re-enable (PATCH) inside the scope.
    let (status, _) = scim_request(
        &app,
        "PUT",
        &format!("/scim/v2/Users/{}", fixture.user_a2),
        Some(json!({
            "schemas": ["urn:ietf:params:scim:schemas:core:2.0:User"],
            "userName": format!("scim-user-a2-renamed-{}", Uuid::new_v4()),
            "displayName": "Updated A2",
            "active": true
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "same-org user update must succeed");

    // SCIM deprovisioning must permanently revoke an existing organization
    // administrator grant only after a successor exists; reactivation must
    // not silently restore authority.
    seed_org_admin_grant(&pool, fixture.org_a, fixture.user_a2, fixture.user_a1).await;

    let inactive_put = json!({
        "schemas": ["urn:ietf:params:scim:schemas:core:2.0:User"],
        "userName": format!("scim-user-a2-inactive-{}", Uuid::new_v4()),
        "displayName": "Updated A2",
        "active": false
    });
    let (status, _) = scim_request(
        &app,
        "PUT",
        &format!("/scim/v2/Users/{}", fixture.user_a2),
        Some(inactive_put),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(!user_disabled(&pool, fixture.user_a2).await);

    let patch_disable = json!({
        "schemas": ["urn:ietf:params:scim:api:messages:2.0:PatchOp"],
        "Operations": [{"op": "replace", "path": "active", "value": false}]
    });
    let (status, _) = scim_request(
        &app,
        "PATCH",
        &format!("/scim/v2/Users/{}", fixture.user_a2),
        Some(patch_disable.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(!user_disabled(&pool, fixture.user_a2).await);

    let (status, _) = scim_request(
        &app,
        "DELETE",
        &format!("/scim/v2/Users/{}", fixture.user_a2),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(!user_disabled(&pool, fixture.user_a2).await);

    seed_org_admin_grant(&pool, fixture.org_a, fixture.user_a1, fixture.user_a2).await;
    let (status, _) = scim_request(
        &app,
        "PATCH",
        &format!("/scim/v2/Users/{}", fixture.user_a2),
        Some(patch_disable.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "successor permits SCIM disable");
    assert!(user_disabled(&pool, fixture.user_a2).await);
    assert_eq!(
        sqlx::query_scalar::<_, Option<String>>(
            "SELECT revocation_source FROM organization_administrators
             WHERE tenant_id = $1 AND user_id = $2 AND revoked_at IS NOT NULL",
        )
        .bind(fixture.org_a)
        .bind(fixture.user_a2)
        .fetch_one(&pool)
        .await
        .expect("read SCIM-revoked grant"),
        Some("scim".to_string())
    );
    assert!(
        sqlx::query_scalar::<_, bool>(
            "SELECT actor_id IS NULL AND actor_principal_id IS NULL
                    AND detail->>'actor_kind' = 'scim'
                    AND detail->>'grant_id' IS NOT NULL
                    AND correlation_id = 'scim-test-correlation'
             FROM admin_actions
             WHERE tenant_id = $1 AND target_id = $2
               AND action_type = 'organization_admin.revoked'",
        )
        .bind(fixture.org_a)
        .bind(fixture.user_a2)
        .fetch_one(&pool)
        .await
        .expect("read SCIM grant audit"),
        "the audit record must identify the SCIM integration without inventing a user actor"
    );
    let (status, _) = scim_request(
        &app,
        "PATCH",
        &format!("/scim/v2/Users/{}", fixture.user_a2),
        Some(json!({
            "schemas": ["urn:ietf:params:scim:api:messages:2.0:PatchOp"],
            "Operations": [{"op": "replace", "path": "active", "value": true}]
        })),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "same-org user re-enable must succeed"
    );
    assert!(!user_disabled(&pool, fixture.user_a2).await);
    assert!(
        sqlx::query_scalar::<_, bool>(
            "SELECT revoked_at IS NOT NULL FROM organization_administrators
             WHERE tenant_id = $1 AND user_id = $2",
        )
        .bind(fixture.org_a)
        .bind(fixture.user_a2)
        .fetch_one(&pool)
        .await
        .expect("read grant after SCIM reactivation"),
        "SCIM reactivation must not restore the revoked administrator grant"
    );

    // Group membership inside the scope.
    let (status, _) = scim_request(
        &app,
        "PATCH",
        &format!("/scim/v2/Groups/{}", fixture.group_a),
        Some(json!({
            "schemas": ["urn:ietf:params:scim:api:messages:2.0:PatchOp"],
            "Operations": [{"op": "add", "path": "members", "value": [{"value": fixture.user_a2.to_string()}]}]
        })),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "same-org group membership add must succeed"
    );
    assert!(group_member_exists(&pool, fixture.group_a, fixture.user_a2).await);

    let new_group = format!("scim-group-new-{}", Uuid::new_v4());
    let (status, _) = scim_request(
        &app,
        "POST",
        "/scim/v2/Groups",
        Some(json!({
            "schemas": ["urn:ietf:params:scim:schemas:core:2.0:Group"],
            "displayName": new_group,
            "externalId": format!("scim-gnew-{}", Uuid::new_v4())
        })),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "same-org group create must succeed"
    );
    let new_group_tenant: Uuid =
        sqlx::query_scalar("SELECT tenant_id FROM user_groups WHERE name = $1")
            .bind(&new_group)
            .fetch_one(&pool)
            .await
            .expect("failed to read new group tenant");
    assert_eq!(
        new_group_tenant, fixture.org_a,
        "created group must land in the configured organization"
    );

    // Explicitly grant the reactivated user again; reactivation itself must not
    // restore the revoked grant. A successor then permits this deprovision.
    seed_org_admin_grant(&pool, fixture.org_a, fixture.user_a2, fixture.user_a1).await;

    // Deprovision (DELETE) inside the scope keeps the row but disables it.
    let (status, _) = scim_request(
        &app,
        "DELETE",
        &format!("/scim/v2/Users/{}", fixture.user_a1),
        None,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NO_CONTENT,
        "same-org user deprovision must succeed"
    );
    assert!(user_disabled(&pool, fixture.user_a1).await);

    // --- Negative: cross-organization targets are indistinguishable from
    // missing ones, and mutations never land ---
    for attempt in 0..RETRY_ATTEMPTS {
        let (status, _) = scim_request(
            &app,
            "GET",
            &format!("/scim/v2/Users/{}", fixture.user_b1),
            None,
        )
        .await;
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "cross-org read attempt {attempt} must 404"
        );

        let (status, _) = scim_request(
            &app,
            "PUT",
            &format!("/scim/v2/Users/{}", fixture.user_b1),
            Some(json!({
                "schemas": ["urn:ietf:params:scim:schemas:core:2.0:User"],
                "userName": format!("scim-hijack-{attempt}-{}", Uuid::new_v4()),
                "active": true
            })),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "cross-org update attempt {attempt} must 404"
        );

        let (status, _) = scim_request(
            &app,
            "PATCH",
            &format!("/scim/v2/Users/{}", fixture.user_b1),
            Some(patch_disable.clone()),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "cross-org disable attempt {attempt} must 404"
        );

        let (status, _) = scim_request(
            &app,
            "DELETE",
            &format!("/scim/v2/Users/{}", fixture.user_b1),
            None,
        )
        .await;
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "cross-org deprovision attempt {attempt} must 404"
        );
    }
    assert!(
        !user_disabled(&pool, fixture.user_b1).await,
        "cross-org user must remain enabled after denied disable attempts"
    );
    let (username, display_name): (String, String) =
        sqlx::query_as("SELECT username, display_name FROM users WHERE id = $1")
            .bind(fixture.user_b1)
            .fetch_one(&pool)
            .await
            .expect("failed to read cross-org user");
    assert_eq!(
        username, fixture.username_b1,
        "cross-org user must be unchanged by denied updates"
    );
    assert_eq!(
        display_name, fixture.display_b1,
        "cross-org user must be unchanged by denied updates"
    );

    let (status, _) = scim_request(
        &app,
        "GET",
        &format!("/scim/v2/Groups/{}", fixture.group_b),
        None,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "cross-org group read must 404"
    );

    let (status, _) = scim_request(
        &app,
        "PUT",
        &format!("/scim/v2/Groups/{}", fixture.group_b),
        Some(json!({
            "schemas": ["urn:ietf:params:scim:schemas:core:2.0:Group"],
            "displayName": format!("scim-hijacked-{}", Uuid::new_v4())
        })),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "cross-org group update must 404"
    );
    let (status, _) = scim_request(
        &app,
        "DELETE",
        &format!("/scim/v2/Groups/{}", fixture.group_b),
        None,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "cross-org group delete must 404"
    );
    let group_b_still_named: Option<String> =
        sqlx::query_scalar("SELECT name FROM user_groups WHERE id = $1")
            .bind(fixture.group_b)
            .fetch_optional(&pool)
            .await
            .expect("failed to read cross-org group");
    assert_eq!(
        group_b_still_named.as_deref(),
        Some(fixture.group_b_name.as_str()),
        "cross-org group must be unchanged by denied mutations"
    );

    // Cross-organization membership references are not resolved.
    let (status, _) = scim_request(
        &app,
        "PATCH",
        &format!("/scim/v2/Groups/{}", fixture.group_a),
        Some(json!({
            "schemas": ["urn:ietf:params:scim:api:messages:2.0:PatchOp"],
            "Operations": [{"op": "add", "path": "members", "value": [{"value": fixture.user_b1.to_string()}]}]
        })),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "in-scope group patch stays successful"
    );
    assert!(
        !group_member_exists(&pool, fixture.group_a, fixture.user_b1).await,
        "cross-organization user must never be added to an in-scope group"
    );

    // A caller-supplied tenant field in the create payload cannot redirect the
    // write: the user is forced into the configured organization.
    let plant_name = format!("scim-plant-{}", Uuid::new_v4());
    let (status, _) = scim_request(
        &app,
        "POST",
        "/scim/v2/Users",
        Some(json!({
            "schemas": ["urn:ietf:params:scim:schemas:core:2.0:User"],
            "userName": plant_name,
            "active": true,
            "tenantId": fixture.org_b.to_string(),
            "tenant_id": fixture.org_b.to_string()
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(
        user_tenant(&pool, &plant_name).await,
        Some(fixture.org_a),
        "caller-supplied tenant fields must be forced to the configured organization"
    );
}

/// Positive and negative SCIM v1 (webhook-style) operations.
#[tokio::test]
#[ignore = "requires explicitly configured disposable local PostgreSQL and RustFS"]
async fn scim_v1_is_scoped_to_the_configured_organization() {
    let _serial = SERIAL.lock().await;
    assert_local_database();
    let state = setup_test_env().await;
    let pool = state.db_pool.clone();
    let app = rustshare_server::routes::scim_routes().with_state(state.clone());
    let fixture = seed_two_organizations(&pool).await;
    let _env = scope_env(fixture.org_a);

    // Create without tenantId lands in the configured organization.
    let v1_user = format!("scim-v1-user-{}", Uuid::new_v4());
    let (status, body) = scim_request(
        &app,
        "POST",
        "/api/v1/scim/users",
        Some(json!({
            "externalId": format!("scim-v1-ext-{v1_user}"),
            "userName": v1_user,
            "active": true
        })),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "same-org v1 provision must succeed: {body}"
    );
    assert_eq!(user_tenant(&pool, &v1_user).await, Some(fixture.org_a));

    // Create with tenantId equal to the scope is accepted.
    let v1_user_scoped = format!("scim-v1-scoped-{}", Uuid::new_v4());
    let (status, _) = scim_request(
        &app,
        "POST",
        "/api/v1/scim/users",
        Some(json!({
            "externalId": format!("scim-v1-scoped-ext-{v1_user_scoped}"),
            "userName": v1_user_scoped,
            "active": true,
            "tenantId": fixture.org_a.to_string()
        })),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "tenantId matching the scope must be accepted"
    );
    assert_eq!(
        user_tenant(&pool, &v1_user_scoped).await,
        Some(fixture.org_a)
    );

    // Update of an existing same-organization user (active toggle).
    let (status, body) = scim_request(
        &app,
        "POST",
        "/api/v1/scim/users",
        Some(json!({
            "externalId": fixture.user_a1_external,
            "userName": format!("scim-a1-updated-{}", Uuid::new_v4()),
            "active": false
        })),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "same-org v1 update must succeed: {body}"
    );
    assert!(user_disabled(&pool, fixture.user_a1).await);

    // Re-enable the first user as a replacement candidate after checking the
    // ordinary SCIM disable path above.
    let (status, _) = scim_request(
        &app,
        "POST",
        "/api/v1/scim/users",
        Some(json!({
            "externalId": fixture.user_a1_external,
            "userName": format!("scim-a1-reactivated-{}", Uuid::new_v4()),
            "active": true
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(!user_disabled(&pool, fixture.user_a1).await);

    // The final SCIM deprovision is rejected until another administrator is
    // explicitly granted; successful deprovision records request correlation.
    seed_org_admin_grant(&pool, fixture.org_a, fixture.user_a2, fixture.user_a1).await;
    let (status, _) = scim_request(
        &app,
        "POST",
        "/api/v1/scim/users",
        Some(json!({
            "externalId": fixture.user_a2_external,
            "userName": format!("scim-a2-disabled-{}", Uuid::new_v4()),
            "active": false
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(!user_disabled(&pool, fixture.user_a2).await);
    let (status, _) = scim_request(
        &app,
        "DELETE",
        &format!("/api/v1/scim/users/{}", fixture.user_a2_external),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(!user_disabled(&pool, fixture.user_a2).await);

    seed_org_admin_grant(&pool, fixture.org_a, fixture.user_a1, fixture.user_a2).await;
    let (status, _) = scim_request(
        &app,
        "DELETE",
        &format!("/api/v1/scim/users/{}", fixture.user_a2_external),
        None,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NO_CONTENT,
        "same-org v1 deprovision must succeed"
    );
    assert!(user_disabled(&pool, fixture.user_a2).await);
    assert_eq!(
        sqlx::query_scalar::<_, Option<String>>(
            "SELECT revocation_source FROM organization_administrators
             WHERE tenant_id = $1 AND user_id = $2 AND revoked_at IS NOT NULL",
        )
        .bind(fixture.org_a)
        .bind(fixture.user_a2)
        .fetch_one(&pool)
        .await
        .expect("read v1 SCIM-revoked grant"),
        Some("scim".to_string())
    );
    assert!(sqlx::query_scalar::<_, bool>(
        "SELECT correlation_id = 'scim-test-correlation'
         FROM admin_actions WHERE tenant_id = $1 AND target_id = $2
           AND action_type = 'organization_admin.revoked'",
    )
    .bind(fixture.org_a)
    .bind(fixture.user_a2)
    .fetch_one(&pool)
    .await
    .expect("read v1 SCIM correlation id"));

    // Group provision with a same-organization member resolves the member.
    let v1_group = format!("scim-v1-group-{}", Uuid::new_v4());
    let (status, _) = scim_request(
        &app,
        "POST",
        "/api/v1/scim/groups",
        Some(json!({
            "externalId": format!("scim-v1-gext-{v1_group}"),
            "displayName": v1_group,
            "members": [{"value": fixture.user_a1_external}]
        })),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "same-org v1 group provision must succeed"
    );
    let v1_group_id: Uuid = sqlx::query_scalar("SELECT id FROM user_groups WHERE name = $1")
        .bind(&v1_group)
        .fetch_one(&pool)
        .await
        .expect("failed to read v1 group");
    let v1_group_tenant: Uuid =
        sqlx::query_scalar("SELECT tenant_id FROM user_groups WHERE id = $1")
            .bind(v1_group_id)
            .fetch_one(&pool)
            .await
            .expect("failed to read v1 group tenant");
    assert_eq!(v1_group_tenant, fixture.org_a);
    assert!(
        group_member_exists(&pool, v1_group_id, fixture.user_a1).await,
        "same-org member reference must resolve"
    );

    // --- Negative: cross-organization attempts are denied ---
    // tenantId in the payload pointing at another organization is rejected.
    let plant_name = format!("scim-v1-plant-{}", Uuid::new_v4());
    let (status, _) = scim_request(
        &app,
        "POST",
        "/api/v1/scim/users",
        Some(json!({
            "externalId": format!("scim-v1-plant-ext-{plant_name}"),
            "userName": plant_name,
            "active": true,
            "tenantId": fixture.org_b.to_string()
        })),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "tenantId outside the scope must be rejected"
    );
    assert_eq!(
        user_tenant(&pool, &plant_name).await,
        None,
        "rejected cross-org create must not plant a user"
    );

    // Cross-organization member references do not resolve.
    let v1_group_mixed = format!("scim-v1-group-mixed-{}", Uuid::new_v4());
    let (status, _) = scim_request(
        &app,
        "POST",
        "/api/v1/scim/groups",
        Some(json!({
            "externalId": format!("scim-v1-gmix-{v1_group_mixed}"),
            "displayName": v1_group_mixed,
            "members": [{"value": fixture.user_b1_external}]
        })),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "group itself is in scope; member is skipped"
    );
    let v1_group_mixed_id: Uuid = sqlx::query_scalar("SELECT id FROM user_groups WHERE name = $1")
        .bind(&v1_group_mixed)
        .fetch_one(&pool)
        .await
        .expect("failed to read mixed v1 group");
    assert!(
        !group_member_exists(&pool, v1_group_mixed_id, fixture.user_b1).await,
        "cross-organization member reference must not resolve"
    );

    // Deprovision of a cross-organization user keeps denying (retry-stable).
    for attempt in 0..RETRY_ATTEMPTS {
        let (status, _) = scim_request(
            &app,
            "DELETE",
            &format!("/api/v1/scim/users/{}", fixture.user_b1_external),
            None,
        )
        .await;
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "cross-org deprovision attempt {attempt} must 404"
        );
    }
    assert!(
        !user_disabled(&pool, fixture.user_b1).await,
        "cross-org user must remain enabled after denied deprovision attempts"
    );

    let (status, _) = scim_request(
        &app,
        "DELETE",
        &format!("/api/v1/scim/groups/{}", fixture.group_b_external),
        None,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "cross-org group delete must 404"
    );
    let group_b_exists: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM user_groups WHERE id = $1)")
            .bind(fixture.group_b)
            .fetch_one(&pool)
            .await
            .expect("failed to read cross-org group existence");
    assert!(group_b_exists, "cross-org group must survive denied delete");
}

/// Missing or invalid organization scope must fail closed on every SCIM
/// endpoint; an explicitly configured default tenant supplies the scope when
/// `RUSTSHARE_SCIM_TENANT_ID` is unset (ADR-0040 D5 defaulting).
#[tokio::test]
#[ignore = "requires explicitly configured disposable local PostgreSQL and RustFS"]
async fn scim_fails_closed_without_a_valid_organization_scope() {
    let _serial = SERIAL.lock().await;
    assert_local_database();
    let state = setup_test_env().await;
    let pool = state.db_pool.clone();
    let app = rustshare_server::routes::scim_routes().with_state(state.clone());
    let fixture = seed_two_organizations(&pool).await;

    let fail_closed_cases: Vec<(&str, Option<String>, Option<String>)> = vec![
        ("scope unset", None, None),
        ("scope not a uuid", Some("not-a-uuid".to_string()), None),
        ("scope empty", Some(String::new()), None),
        (
            "default tenant not a uuid",
            None,
            Some("also-not-a-uuid".to_string()),
        ),
    ];

    for (case, scim_tenant, default_tenant) in fail_closed_cases {
        let _env = EnvGuard::set(vec![
            ("RUSTSHARE_SCIM_BEARER_TOKEN", Some(TEST_TOKEN.to_string())),
            ("RUSTSHARE_SCIM_TENANT_ID", scim_tenant.clone()),
            ("RUSTSHARE_DEFAULT_TENANT_ID", default_tenant.clone()),
        ]);

        let users_before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
            .fetch_one(&pool)
            .await
            .expect("failed to count users");

        let (status, _) = scim_request(&app, "GET", "/scim/v2/Users", None).await;
        assert_eq!(
            status,
            StatusCode::INTERNAL_SERVER_ERROR,
            "case {case}: v2 list must fail closed"
        );

        let (status, _) = scim_request(
            &app,
            "GET",
            &format!("/scim/v2/Users/{}", fixture.user_a1),
            None,
        )
        .await;
        assert_eq!(
            status,
            StatusCode::INTERNAL_SERVER_ERROR,
            "case {case}: v2 read must fail closed"
        );

        let (status, _) = scim_request(&app, "GET", "/scim/v2/Groups", None).await;
        assert_eq!(
            status,
            StatusCode::INTERNAL_SERVER_ERROR,
            "case {case}: v2 group list must fail closed"
        );

        let (status, _) = scim_request(&app, "GET", "/scim/v2/ServiceProviderConfig", None).await;
        assert_eq!(
            status,
            StatusCode::INTERNAL_SERVER_ERROR,
            "case {case}: v2 discovery must fail closed"
        );

        let (status, _) = scim_request(
            &app,
            "POST",
            "/scim/v2/Users",
            Some(json!({
                "schemas": ["urn:ietf:params:scim:schemas:core:2.0:User"],
                "userName": format!("scim-failclosed-{case}-{}", Uuid::new_v4()),
                "active": true
            })),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::INTERNAL_SERVER_ERROR,
            "case {case}: v2 create must fail closed"
        );

        let (status, _) = scim_request(
            &app,
            "PATCH",
            &format!("/scim/v2/Users/{}", fixture.user_a1),
            Some(json!({
                "schemas": ["urn:ietf:params:scim:api:messages:2.0:PatchOp"],
                "Operations": [{"op": "replace", "path": "active", "value": false}]
            })),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::INTERNAL_SERVER_ERROR,
            "case {case}: v2 disable must fail closed"
        );
        assert!(
            !user_disabled(&pool, fixture.user_a1).await,
            "case {case}: fail-closed disable must not modify the user"
        );

        let (status, _) = scim_request(
            &app,
            "POST",
            "/api/v1/scim/users",
            Some(json!({
                "externalId": format!("scim-v1-failclosed-{case}-{}", Uuid::new_v4()),
                "userName": format!("scim-v1-failclosed-{case}-{}", Uuid::new_v4()),
                "active": true
            })),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::INTERNAL_SERVER_ERROR,
            "case {case}: v1 create must fail closed"
        );

        let (status, _) = scim_request(
            &app,
            "DELETE",
            &format!("/api/v1/scim/users/{}", fixture.user_a1_external),
            None,
        )
        .await;
        assert_eq!(
            status,
            StatusCode::INTERNAL_SERVER_ERROR,
            "case {case}: v1 deprovision must fail closed"
        );
        assert!(
            !user_disabled(&pool, fixture.user_a1).await,
            "case {case}: fail-closed v1 deprovision must not modify the user"
        );

        let (status, _) = scim_request(
            &app,
            "POST",
            "/api/v1/scim/groups",
            Some(json!({
                "externalId": format!("scim-v1-gfail-{case}-{}", Uuid::new_v4()),
                "displayName": format!("scim-v1-gfail-{case}-{}", Uuid::new_v4())
            })),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::INTERNAL_SERVER_ERROR,
            "case {case}: v1 group create must fail closed"
        );

        let (status, _) = scim_request(
            &app,
            "DELETE",
            &format!("/api/v1/scim/groups/{}", fixture.group_b_external),
            None,
        )
        .await;
        assert_eq!(
            status,
            StatusCode::INTERNAL_SERVER_ERROR,
            "case {case}: v1 group delete must fail closed"
        );

        let users_after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
            .fetch_one(&pool)
            .await
            .expect("failed to count users");
        assert_eq!(
            users_before, users_after,
            "case {case}: fail-closed endpoints must not create users"
        );
    }

    // Misconfiguration details (e.g. the raw invalid env value) are logged
    // server-side but must not be returned to token holders.
    let _env = EnvGuard::set(vec![
        ("RUSTSHARE_SCIM_BEARER_TOKEN", Some(TEST_TOKEN.to_string())),
        ("RUSTSHARE_SCIM_TENANT_ID", Some("not-a-uuid".to_string())),
        ("RUSTSHARE_DEFAULT_TENANT_ID", None),
    ]);
    let (status, body) = scim_request(&app, "GET", "/scim/v2/Users", None).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    let body_text = body.to_string();
    assert!(
        !body_text.contains("not-a-uuid"),
        "v2 fail-closed body must not leak the invalid configuration value: {body_text}"
    );
    let (status, body) = scim_request(
        &app,
        "POST",
        "/api/v1/scim/users",
        Some(json!({
            "externalId": format!("scim-v1-leak-{}", Uuid::new_v4()),
            "userName": format!("scim-v1-leak-{}", Uuid::new_v4()),
            "active": true
        })),
    )
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    let body_text = body.to_string();
    assert!(
        !body_text.contains("not-a-uuid"),
        "v1 fail-closed body must not leak the invalid configuration value: {body_text}"
    );

    // ADR-0040 D5 defaulting: an explicitly configured default tenant supplies
    // the scope when RUSTSHARE_SCIM_TENANT_ID is unset.
    let _env = EnvGuard::set(vec![
        ("RUSTSHARE_SCIM_BEARER_TOKEN", Some(TEST_TOKEN.to_string())),
        ("RUSTSHARE_SCIM_TENANT_ID", None),
        (
            "RUSTSHARE_DEFAULT_TENANT_ID",
            Some(fixture.org_a.to_string()),
        ),
    ]);
    let (status, body) = scim_request(&app, "GET", "/scim/v2/Users", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["totalResults"].as_i64(),
        Some(2),
        "default-tenant fallback must scope the integration to org A: {body}"
    );
    let (status, _) = scim_request(
        &app,
        "GET",
        &format!("/scim/v2/Users/{}", fixture.user_b1),
        None,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "default-tenant fallback must not expose org B"
    );
}

/// Asserts a 409 Conflict whose body does not disclose the database
/// constraint, the colliding value, or the organization owning it.
fn assert_generic_conflict(status: StatusCode, body: &Value, context: &str) {
    assert_eq!(status, StatusCode::CONFLICT, "{context}: {body}");
    let text = body.to_string().to_lowercase();
    for needle in [
        "duplicate",
        "23505",
        "unique constraint",
        "violates",
        "pkey",
        "_key",
        "users_",
        "user_groups",
    ] {
        assert!(
            !text.contains(needle),
            "{context}: conflict body leaks internals ({needle}): {body}"
        );
    }
}

fn encode_query(pairs: &[(&str, &str)]) -> String {
    url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(pairs)
        .finish()
}

/// Unique-violation collisions on the globally unique directory identifiers
/// (username, external_id, group name) must surface as a generic 409 that
/// reveals nothing about other organizations — never a 500 naming the
/// constraint, and never an update of the other organization's row.
#[tokio::test]
#[ignore = "requires explicitly configured disposable local PostgreSQL and RustFS"]
async fn scim_creates_fail_with_generic_conflict_on_unique_violation() {
    let _serial = SERIAL.lock().await;
    assert_local_database();
    let state = setup_test_env().await;
    let pool = state.db_pool.clone();
    let app = rustshare_server::routes::scim_routes().with_state(state.clone());
    let fixture = seed_two_organizations(&pool).await;
    let _env = scope_env(fixture.org_a);

    // Identifiers owned by org B (cross-org collisions).
    let b_user_external = fixture.user_b1_external.clone();
    let b_username = fixture.username_b1.clone();
    let b_group_name = fixture.group_b_name.clone();
    let b_group_external = fixture.group_b_external.clone();

    // --- SCIM v2 user creates ---
    let (status, body) = scim_request(
        &app,
        "POST",
        "/scim/v2/Users",
        Some(json!({
            "schemas": ["urn:ietf:params:scim:schemas:core:2.0:User"],
            "userName": format!("scim-v2-xext-{}", Uuid::new_v4()),
            "externalId": b_user_external,
            "active": true
        })),
    )
    .await;
    assert_generic_conflict(status, &body, "v2 create with cross-org externalId");

    let (status, body) = scim_request(
        &app,
        "POST",
        "/scim/v2/Users",
        Some(json!({
            "schemas": ["urn:ietf:params:scim:schemas:core:2.0:User"],
            "userName": b_username,
            "externalId": format!("scim-v2-xname-{}", Uuid::new_v4()),
            "active": true
        })),
    )
    .await;
    assert_generic_conflict(status, &body, "v2 create with cross-org username");

    // In-scope username collision via a different externalId (not the update path).
    let dup_name = format!("scim-v2-dup-{}", Uuid::new_v4());
    let (status, _) = scim_request(
        &app,
        "POST",
        "/scim/v2/Users",
        Some(json!({
            "schemas": ["urn:ietf:params:scim:schemas:core:2.0:User"],
            "userName": dup_name,
            "active": true
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, body) = scim_request(
        &app,
        "POST",
        "/scim/v2/Users",
        Some(json!({
            "schemas": ["urn:ietf:params:scim:schemas:core:2.0:User"],
            "userName": dup_name,
            "externalId": format!("scim-v2-dupext-{}", Uuid::new_v4()),
            "active": true
        })),
    )
    .await;
    assert_generic_conflict(status, &body, "v2 create with in-scope username duplicate");

    // --- SCIM v2 group creates ---
    let (status, body) = scim_request(
        &app,
        "POST",
        "/scim/v2/Groups",
        Some(json!({
            "schemas": ["urn:ietf:params:scim:schemas:core:2.0:Group"],
            "displayName": format!("scim-v2-gxext-{}", Uuid::new_v4()),
            "externalId": b_group_external
        })),
    )
    .await;
    assert_generic_conflict(status, &body, "v2 group create with cross-org externalId");

    let (status, body) = scim_request(
        &app,
        "POST",
        "/scim/v2/Groups",
        Some(json!({
            "schemas": ["urn:ietf:params:scim:schemas:core:2.0:Group"],
            "displayName": b_group_name,
            "externalId": format!("scim-v2-gxname-{}", Uuid::new_v4())
        })),
    )
    .await;
    assert_generic_conflict(status, &body, "v2 group create with cross-org name");

    let dup_group = format!("scim-v2-gdup-{}", Uuid::new_v4());
    let (status, _) = scim_request(
        &app,
        "POST",
        "/scim/v2/Groups",
        Some(json!({
            "schemas": ["urn:ietf:params:scim:schemas:core:2.0:Group"],
            "displayName": dup_group
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, body) = scim_request(
        &app,
        "POST",
        "/scim/v2/Groups",
        Some(json!({
            "schemas": ["urn:ietf:params:scim:schemas:core:2.0:Group"],
            "displayName": dup_group,
            "externalId": format!("scim-v2-gdupext-{}", Uuid::new_v4())
        })),
    )
    .await;
    assert_generic_conflict(
        status,
        &body,
        "v2 group create with in-scope name duplicate",
    );

    // --- SCIM v1 user creates ---
    let (status, body) = scim_request(
        &app,
        "POST",
        "/api/v1/scim/users",
        Some(json!({
            "externalId": fixture.user_b1_external,
            "userName": format!("scim-v1-xext-{}", Uuid::new_v4()),
            "active": true
        })),
    )
    .await;
    assert_generic_conflict(status, &body, "v1 create with cross-org externalId");

    let (status, body) = scim_request(
        &app,
        "POST",
        "/api/v1/scim/users",
        Some(json!({
            "externalId": format!("scim-v1-xname-{}", Uuid::new_v4()),
            "userName": fixture.username_b1,
            "active": true
        })),
    )
    .await;
    assert_generic_conflict(status, &body, "v1 create with cross-org username");

    let dup_v1_name = format!("scim-v1-dup-{}", Uuid::new_v4());
    let (status, _) = scim_request(
        &app,
        "POST",
        "/api/v1/scim/users",
        Some(json!({
            "externalId": format!("scim-v1-dupext-{}", Uuid::new_v4()),
            "userName": dup_v1_name.clone(),
            "active": true
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, body) = scim_request(
        &app,
        "POST",
        "/api/v1/scim/users",
        Some(json!({
            "externalId": format!("scim-v1-dup2-{}", Uuid::new_v4()),
            "userName": dup_v1_name,
            "active": true
        })),
    )
    .await;
    assert_generic_conflict(status, &body, "v1 create with in-scope username duplicate");

    // --- SCIM v1 group creates ---
    let (status, body) = scim_request(
        &app,
        "POST",
        "/api/v1/scim/groups",
        Some(json!({
            "externalId": fixture.group_b_external,
            "displayName": format!("scim-v1-gxext-{}", Uuid::new_v4())
        })),
    )
    .await;
    assert_generic_conflict(status, &body, "v1 group create with cross-org externalId");

    let (status, body) = scim_request(
        &app,
        "POST",
        "/api/v1/scim/groups",
        Some(json!({
            "externalId": format!("scim-v1-gxname-{}", Uuid::new_v4()),
            "displayName": fixture.group_b_name
        })),
    )
    .await;
    assert_generic_conflict(status, &body, "v1 group create with cross-org name");

    let dup_v1_group = format!("scim-v1-gdup-{}", Uuid::new_v4());
    let (status, _) = scim_request(
        &app,
        "POST",
        "/api/v1/scim/groups",
        Some(json!({
            "externalId": format!("scim-v1-gdupext-{}", Uuid::new_v4()),
            "displayName": dup_v1_group.clone()
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, body) = scim_request(
        &app,
        "POST",
        "/api/v1/scim/groups",
        Some(json!({
            "externalId": format!("scim-v1-gdup2-{}", Uuid::new_v4()),
            "displayName": dup_v1_group
        })),
    )
    .await;
    assert_generic_conflict(
        status,
        &body,
        "v1 group create with in-scope name duplicate",
    );
}

/// Filtered v2 lists must respect the configured organization even with
/// filter and pagination binds in the query (the tenant scope is bind $1;
/// filters and pagination follow). SCIM v1 webhook endpoints take no filter
/// parameter, so these are v2-only.
#[tokio::test]
#[ignore = "requires explicitly configured disposable local PostgreSQL and RustFS"]
async fn scim_v2_filtered_lists_stay_within_scope() {
    let _serial = SERIAL.lock().await;
    assert_local_database();
    let state = setup_test_env().await;
    let pool = state.db_pool.clone();
    let app = rustshare_server::routes::scim_routes().with_state(state.clone());
    let fixture = seed_two_organizations(&pool).await;
    let _env = scope_env(fixture.org_a);

    // userName filter inside the scope.
    let query = encode_query(&[(
        "filter",
        &format!("userName eq \"{}\"", fixture.username_a1),
    )]);
    let (status, body) = scim_request(&app, "GET", &format!("/scim/v2/Users?{query}"), None).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "filtered in-scope user list: {body}"
    );
    assert_eq!(
        body["totalResults"].as_i64(),
        Some(1),
        "in-scope userName filter must match exactly one row: {body}"
    );
    let names = usernames(&body);
    assert_eq!(names, vec![fixture.username_a1.clone()]);

    // userName filter targeting the other organization.
    let query = encode_query(&[(
        "filter",
        &format!("userName eq \"{}\"", fixture.username_b1),
    )]);
    let (status, body) = scim_request(&app, "GET", &format!("/scim/v2/Users?{query}"), None).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "filtered cross-org user list: {body}"
    );
    assert_eq!(
        body["totalResults"].as_i64(),
        Some(0),
        "cross-org userName filter must match nothing: {body}"
    );

    // Filter plus pagination binds (startIndex/count).
    let query = encode_query(&[
        ("filter", "userName co \"scim-user\""),
        ("startIndex", "1"),
        ("count", "5"),
    ]);
    let (status, body) = scim_request(&app, "GET", &format!("/scim/v2/Users?{query}"), None).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "filtered+paginated user list: {body}"
    );
    assert_eq!(
        body["totalResults"].as_i64(),
        Some(2),
        "scope predicate must keep the cross-org user out of filtered counts: {body}"
    );
    assert_eq!(usernames(&body).len(), 2);

    // externalId filters.
    let query = encode_query(&[(
        "filter",
        &format!("externalId eq \"{}\"", fixture.user_a1_external),
    )]);
    let (status, body) = scim_request(&app, "GET", &format!("/scim/v2/Users?{query}"), None).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "filtered in-scope externalId list: {body}"
    );
    assert_eq!(body["totalResults"].as_i64(), Some(1));

    let query = encode_query(&[(
        "filter",
        &format!("externalId eq \"{}\"", fixture.user_b1_external),
    )]);
    let (status, body) = scim_request(&app, "GET", &format!("/scim/v2/Users?{query}"), None).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "filtered cross-org externalId list: {body}"
    );
    assert_eq!(
        body["totalResults"].as_i64(),
        Some(0),
        "cross-org externalId filter must match nothing: {body}"
    );

    // Group displayName filters.
    let query = encode_query(&[(
        "filter",
        &format!("displayName eq \"{}\"", fixture.group_a_name),
    )]);
    let (status, body) = scim_request(&app, "GET", &format!("/scim/v2/Groups?{query}"), None).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "filtered in-scope group list: {body}"
    );
    assert_eq!(
        body["totalResults"].as_i64(),
        Some(1),
        "in-scope displayName filter must match exactly one group: {body}"
    );

    let query = encode_query(&[(
        "filter",
        &format!("displayName eq \"{}\"", fixture.group_b_name),
    )]);
    let (status, body) = scim_request(&app, "GET", &format!("/scim/v2/Groups?{query}"), None).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "filtered cross-org group list: {body}"
    );
    assert_eq!(
        body["totalResults"].as_i64(),
        Some(0),
        "cross-org displayName filter must match nothing: {body}"
    );

    // Group externalId filters.
    let query = encode_query(&[(
        "filter",
        &format!("externalId eq \"{}\"", fixture.group_a_external),
    )]);
    let (status, body) = scim_request(&app, "GET", &format!("/scim/v2/Groups?{query}"), None).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "filtered in-scope group externalId: {body}"
    );
    assert_eq!(body["totalResults"].as_i64(), Some(1));

    let query = encode_query(&[(
        "filter",
        &format!("externalId eq \"{}\"", fixture.group_b_external),
    )]);
    let (status, body) = scim_request(&app, "GET", &format!("/scim/v2/Groups?{query}"), None).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "filtered cross-org group externalId: {body}"
    );
    assert_eq!(
        body["totalResults"].as_i64(),
        Some(0),
        "cross-org group externalId filter must match nothing: {body}"
    );
}

/// Membership reads and membership removals must check BOTH legs of a
/// membership (group in scope AND user in scope), so cross-organization
/// membership rows that already exist cannot be observed or mutated through
/// an integration scoped to the other organization.
#[tokio::test]
#[ignore = "requires explicitly configured disposable local PostgreSQL and RustFS"]
async fn scim_membership_queries_enforce_both_scope_legs() {
    use rustshare_core::services::{ScimRepository, ScimV2Repository};

    let _serial = SERIAL.lock().await;
    assert_local_database();
    let state = setup_test_env().await;
    let pool = state.db_pool.clone();
    let fixture = seed_two_organizations(&pool).await;

    // Legacy-shaped data: cross-tenant membership rows not writable through
    // any scoped API path anymore, but still present in the table.
    for (group, user) in [
        (fixture.group_a, fixture.user_b1),
        (fixture.group_b, fixture.user_a2),
    ] {
        sqlx::query("INSERT INTO group_members (group_id, user_id) VALUES ($1, $2)")
            .bind(group)
            .bind(user)
            .execute(&pool)
            .await
            .expect("failed to seed cross-tenant membership");
    }

    let v2_repo = rustshare_server::handlers::scim_v2::ScimV2RepositoryImpl::new(pool.clone());
    let v1_repo = rustshare_server::handlers::scim::ScimRepositoryImpl::new(pool.clone());

    // v2 get_group_members: a cross-org GROUP with an in-scope user must not
    // disclose the user (group leg), and a cross-org USER in an in-scope
    // group must not be listed either (user leg).
    let members = ScimV2Repository::get_group_members(&v2_repo, fixture.org_a, fixture.group_b)
        .await
        .expect("v2 get_group_members failed");
    let member_ids: Vec<Uuid> = members.iter().map(|(id, _)| *id).collect();
    assert!(
        !member_ids.contains(&fixture.user_a2),
        "cross-org group must not disclose in-scope users: {member_ids:?}"
    );

    let members = ScimV2Repository::get_group_members(&v2_repo, fixture.org_a, fixture.group_a)
        .await
        .expect("v2 get_group_members failed");
    let member_ids: Vec<Uuid> = members.iter().map(|(id, _)| *id).collect();
    assert!(
        member_ids.contains(&fixture.user_a1),
        "in-scope membership must remain visible: {member_ids:?}"
    );
    assert!(
        !member_ids.contains(&fixture.user_b1),
        "cross-org member must not be listed in an in-scope group: {member_ids:?}"
    );

    // v2 get_user_groups: a group in scope joined to a cross-org user must
    // not be returned (user leg).
    let user_groups = ScimV2Repository::get_user_groups(&v2_repo, fixture.org_a, fixture.user_b1)
        .await
        .expect("v2 get_user_groups failed");
    assert!(
        user_groups.is_empty(),
        "cross-org user must not resolve in-scope groups: {user_groups:?}"
    );

    // v1 get_group_members: both legs.
    let v1_members = ScimRepository::get_group_members(&v1_repo, fixture.org_a, fixture.group_a)
        .await
        .expect("v1 get_group_members failed");
    assert!(
        v1_members.contains(&fixture.user_a1),
        "in-scope membership must remain visible: {v1_members:?}"
    );
    assert!(
        !v1_members.contains(&fixture.user_b1),
        "cross-org member must not be listed in an in-scope group: {v1_members:?}"
    );

    // remove_group_member: removing a cross-org user from an in-scope group
    // is a no-op on both implementations.
    ScimV2Repository::remove_group_member(
        &v2_repo,
        fixture.org_a,
        fixture.group_a,
        fixture.user_b1,
    )
    .await
    .expect("v2 remove_group_member failed");
    assert!(
        group_member_exists(&pool, fixture.group_a, fixture.user_b1).await,
        "v2 remove of a cross-org member must be a no-op"
    );
    ScimRepository::remove_group_member(&v1_repo, fixture.org_a, fixture.group_a, fixture.user_b1)
        .await
        .expect("v1 remove_group_member failed");
    assert!(
        group_member_exists(&pool, fixture.group_a, fixture.user_b1).await,
        "v1 remove of a cross-org member must be a no-op"
    );

    // clear_group_members: in-scope memberships are cleared, cross-org
    // memberships survive (per-row user leg).
    ScimV2Repository::clear_group_members(&v2_repo, fixture.org_a, fixture.group_a)
        .await
        .expect("v2 clear_group_members failed");
    assert!(
        !group_member_exists(&pool, fixture.group_a, fixture.user_a1).await,
        "v2 clear must remove in-scope memberships"
    );
    assert!(
        group_member_exists(&pool, fixture.group_a, fixture.user_b1).await,
        "v2 clear must preserve cross-org memberships"
    );

    sqlx::query("INSERT INTO group_members (group_id, user_id) VALUES ($1, $2)")
        .bind(fixture.group_a)
        .bind(fixture.user_a1)
        .execute(&pool)
        .await
        .expect("failed to re-seed in-scope membership");
    ScimRepository::clear_group_members(&v1_repo, fixture.org_a, fixture.group_a)
        .await
        .expect("v1 clear_group_members failed");
    assert!(
        !group_member_exists(&pool, fixture.group_a, fixture.user_a1).await,
        "v1 clear must remove in-scope memberships"
    );
    assert!(
        group_member_exists(&pool, fixture.group_a, fixture.user_b1).await,
        "v1 clear must preserve cross-org memberships"
    );
}
