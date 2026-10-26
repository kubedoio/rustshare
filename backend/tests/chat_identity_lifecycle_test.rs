use anyhow::Result;
use chrono::{Duration, Utc};
use rustshare_core::domain::{PrincipalId, TenantId};
use rustshare_resource_auth::{BindingChallenge, BindingStatus, ChatIdentityBinding};
use rustshare_storage::ChatIdentityStore;
use sqlx::{postgres::PgConnectOptions, PgPool};
use uuid::Uuid;

async fn issue(
    store: &ChatIdentityStore,
    tenant: Uuid,
    principal: Uuid,
    key: char,
    rotation_of: Option<Uuid>,
) -> Result<(BindingChallenge, ChatIdentityBinding)> {
    let now = Utc::now();
    let challenge = BindingChallenge::issue(
        TenantId(tenant),
        PrincipalId(principal),
        key.to_string().repeat(64),
        "wss://relay.example.test",
        rotation_of,
        now,
        Duration::minutes(5),
    );
    store.insert_challenge(&challenge).await?;
    let binding = ChatIdentityBinding {
        binding_id: Uuid::new_v4(),
        tenant_id: challenge.tenant_id,
        principal_id: challenge.principal_id,
        buzz_pubkey: challenge.buzz_pubkey.clone(),
        status: BindingStatus::Active,
        created_at: now,
        verified_at: Some(now),
        revoked_at: None,
        rotation_of,
        audit_metadata: serde_json::json!({"test": true}),
    };
    Ok((challenge, binding))
}

#[tokio::test]
#[ignore = "requires explicitly configured disposable local PostgreSQL"]
async fn revoked_keys_stay_revoked_and_competing_rotations_fail_closed() -> Result<()> {
    assert_eq!(
        std::env::var("RUSTSHARE_TEST_DISPOSABLE_DB").as_deref(),
        Ok("1")
    );
    let database_url = std::env::var("DATABASE_URL")?;
    assert!(
        !database_url.contains('?'),
        "test URLs must not contain connection parameters"
    );
    let options: PgConnectOptions = database_url.parse()?;
    assert!(matches!(
        options.get_host(),
        "127.0.0.1" | "localhost" | "::1"
    ));
    assert!(options
        .get_database()
        .is_some_and(|db| db == "rustshare_test" || db.starts_with("rustshare_test_")));
    let application_name = format!("chat-lifecycle-{}", Uuid::new_v4());
    let pool = PgPool::connect_with(options.application_name(&application_name)).await?;
    let tenant = Uuid::new_v4();
    let foreign_tenant = Uuid::new_v4();
    let owner = Uuid::new_v4();
    let other = Uuid::new_v4();
    let foreign = Uuid::new_v4();
    let attempt: Result<()> = async {
    for id in [tenant, foreign_tenant] {
        sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, $2)")
            .bind(id)
            .bind(format!("Chat identity lifecycle {id}"))
            .execute(&pool)
            .await?;
    }
    for (id, scope) in [(owner, tenant), (other, tenant), (foreign, foreign_tenant)] {
        sqlx::query("INSERT INTO users (id, username, email, password_hash, display_name, storage_quota, tenant_id)
                     VALUES ($1, $2, $3, 'test-only', $2, 1024, $4)")
            .bind(id).bind(format!("chat_lifecycle_{}", id.simple()))
            .bind(format!("{id}@test.local")).bind(scope).execute(&pool).await?;
    }
    let store = ChatIdentityStore::new(pool.clone());
    let (original, original_binding) = issue(&store, tenant, owner, '1', None).await?;
    assert!(
        store
            .consume_and_activate(&original, &original_binding, Utc::now())
            .await?
    );
    store
        .revoke_principal(TenantId(tenant), PrincipalId(owner))
        .await?;

    for (scope, principal) in [(tenant, owner), (tenant, other), (foreign_tenant, foreign)] {
        let (challenge, binding) = issue(&store, scope, principal, '1', None).await?;
        assert!(
            !store
                .consume_and_activate(&challenge, &binding, Utc::now())
                .await?,
            "a previously bound key cannot be restored, reassigned or reused across tenants"
        );
    }
    let (replacement, replacement_binding) = issue(&store, tenant, owner, '2', None).await?;
    let mut wrong_key_binding = replacement_binding.clone();
    wrong_key_binding.buzz_pubkey = '8'.to_string().repeat(64);
    assert!(
        !store
            .consume_and_activate(&replacement, &wrong_key_binding, Utc::now())
            .await?,
        "storage must reject a binding that differs from its verified challenge"
    );
    assert!(
        store
            .consume_and_activate(&replacement, &replacement_binding, Utc::now())
            .await?
    );
    assert!(
        !store
            .consume_and_activate(&replacement, &replacement_binding, Utc::now())
            .await?,
        "replay must fail"
    );
    for (scope, principal) in [(tenant, other), (foreign_tenant, foreign)] {
        let (challenge, binding) = issue(
            &store,
            scope,
            principal,
            '3',
            Some(replacement_binding.binding_id),
        )
        .await?;
        assert!(
            !store
                .consume_and_activate(&challenge, &binding, Utc::now())
                .await?,
            "rotation cannot target another Principal or tenant"
        );
    }

    let (left, left_binding) = issue(
        &store,
        tenant,
        owner,
        '3',
        Some(replacement_binding.binding_id),
    )
    .await?;
    let (right, right_binding) = issue(
        &store,
        tenant,
        owner,
        '4',
        Some(replacement_binding.binding_id),
    )
    .await?;
    // Hold the old binding so both rotations overlap before either can commit.
    let mut blocker = pool.begin().await?;
    sqlx::query("SELECT binding_id FROM chat_identity_bindings WHERE binding_id = $1 FOR UPDATE")
        .bind(replacement_binding.binding_id)
        .execute(&mut *blocker)
        .await?;
    let rotations = async {
        tokio::join!(
            store.consume_and_activate(&left, &left_binding, Utc::now()),
            store.consume_and_activate(&right, &right_binding, Utc::now()),
        )
    };
    let release = async {
        let waited = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                let waiting: i64 = sqlx::query_scalar(
                    "SELECT count(*) FROM pg_stat_activity
                     WHERE application_name = $1 AND wait_event_type = 'Lock'",
                )
                .bind(&application_name)
                .fetch_one(&pool)
                .await?;
                if waiting >= 2 {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
            Ok::<_, sqlx::Error>(())
        })
        .await;
        blocker.commit().await?;
        waited??;
        Ok::<_, anyhow::Error>(())
    };
    let ((left_result, right_result), released) = tokio::join!(rotations, release);
    released?;
    let (left_succeeded, right_succeeded) = (left_result?, right_result?);
    assert_ne!(
        left_succeeded, right_succeeded,
        "exactly one competing rotation must succeed"
    );
    let current = store
        .active_binding(TenantId(tenant), PrincipalId(owner))
        .await?
        .unwrap();
    assert_eq!(current.rotation_of, Some(replacement_binding.binding_id));

    let (owner_challenge, owner_binding) =
        issue(&store, tenant, owner, '5', Some(current.binding_id)).await?;
    let (other_challenge, other_binding) = issue(&store, tenant, other, '5', None).await?;
    let (owner_result, other_result) = tokio::join!(
        store.consume_and_activate(&owner_challenge, &owner_binding, Utc::now()),
        store.consume_and_activate(&other_challenge, &other_binding, Utc::now()),
    );
    let (owner_succeeded, other_succeeded) = (owner_result?, other_result?);
    assert_ne!(
        owner_succeeded, other_succeeded,
        "exactly one Principal can activate a public key"
    );
    let owner_current = store
        .active_binding(TenantId(tenant), PrincipalId(owner))
        .await?
        .unwrap();
    let (owner_challenge, owner_binding) =
        issue(&store, tenant, owner, '8', Some(owner_current.binding_id)).await?;
    let (foreign_challenge, foreign_binding) =
        issue(&store, foreign_tenant, foreign, '8', None).await?;
    let mut key_blocker = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(format!("chat-binding:{}", owner_challenge.buzz_pubkey))
        .execute(&mut *key_blocker)
        .await?;
    let activations = async {
        tokio::join!(
            store.consume_and_activate(&owner_challenge, &owner_binding, Utc::now()),
            store.consume_and_activate(&foreign_challenge, &foreign_binding, Utc::now()),
        )
    };
    let release = async {
        let waited = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                let waiting: i64 = sqlx::query_scalar(
                    "SELECT count(*) FROM pg_stat_activity
                     WHERE application_name = $1 AND wait_event_type = 'Lock'",
                )
                .bind(&application_name)
                .fetch_one(&pool)
                .await?;
                if waiting >= 2 {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
            Ok::<_, sqlx::Error>(())
        })
        .await;
        key_blocker.commit().await?;
        waited??;
        Ok::<_, anyhow::Error>(())
    };
    let ((owner_result, foreign_result), released) = tokio::join!(activations, release);
    released?;
    let (owner_succeeded, foreign_succeeded) = (owner_result?, foreign_result?);
    assert_ne!(
        owner_succeeded, foreign_succeeded,
        "exactly one cross-tenant activation of the same key must succeed"
    );
    let claimed: Vec<(Uuid, Uuid)> = sqlx::query_as(
        "SELECT tenant_id, principal_id FROM chat_identity_bindings WHERE buzz_pubkey = $1",
    )
    .bind(&owner_challenge.buzz_pubkey)
    .fetch_all(&pool)
    .await?;
    let expected = if owner_succeeded {
        (tenant, owner)
    } else {
        (foreign_tenant, foreign)
    };
    assert_eq!(
        claimed,
        vec![expected],
        "exactly one transaction may commit binding history for the shared key"
    );
    let current = store
        .active_binding(TenantId(tenant), PrincipalId(owner))
        .await?
        .unwrap();
    let (challenge, binding) = issue(&store, tenant, owner, '6', Some(current.binding_id)).await?;
    let (rotation, revocation) = tokio::join!(
        store.consume_and_activate(&challenge, &binding, Utc::now()),
        store.revoke_principal(TenantId(tenant), PrincipalId(owner)),
    );
    rotation?;
    revocation?;
    assert!(
        store
            .active_binding(TenantId(tenant), PrincipalId(owner))
            .await?
            .is_none(),
        "explicit revocation must also cover a competing rotation"
    );
    sqlx::query("UPDATE users SET disabled_at = now() WHERE id = $1")
        .bind(owner)
        .execute(&pool)
        .await?;
    let (challenge, binding) = issue(&store, tenant, owner, '7', None).await?;
    assert!(
        !store
            .consume_and_activate(&challenge, &binding, Utc::now())
            .await?,
        "a disabled Principal cannot activate a pending binding"
    );
    let historical: Vec<Uuid> = sqlx::query_scalar(
        "SELECT principal_id FROM chat_identity_bindings WHERE tenant_id = $1 AND buzz_pubkey = $2",
    )
    .bind(tenant)
    .bind(original.buzz_pubkey)
    .fetch_all(&pool)
    .await?;
    assert_eq!(
        historical,
        vec![owner],
        "historical attribution must be preserved"
    );

    Ok(())
    }
    .await;

    let cleanup: Result<()> = async {
        for query in [
            "DELETE FROM chat_binding_challenges WHERE tenant_id = ANY($1)",
            "DELETE FROM chat_identity_bindings WHERE tenant_id = ANY($1)",
            "DELETE FROM users WHERE tenant_id = ANY($1)",
            "DELETE FROM tenants WHERE id = ANY($1)",
        ] {
            sqlx::query(query)
                .bind(vec![tenant, foreign_tenant])
                .execute(&pool)
                .await?;
        }
        Ok(())
    }
    .await;
    attempt?;
    cleanup
}
