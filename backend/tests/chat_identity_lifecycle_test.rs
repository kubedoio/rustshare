use anyhow::{anyhow, Result};
use chrono::{Duration, Utc};
use rustshare_core::domain::{PrincipalId, TenantId, WorkspaceId};
use rustshare_integration_events::{ConsumerOutcome, IntegrationEvent, OutboxConsumer};
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
async fn revoked_keys_stay_revoked_and_replacement_stays_fail_closed() -> Result<()> {
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
    let revoked_owner = Uuid::new_v4();
    let other = Uuid::new_v4();
    let same_tenant_local = Uuid::new_v4();
    let foreign = Uuid::new_v4();
    let attempt: Result<()> = async {
    for id in [tenant, foreign_tenant] {
        sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, $2)")
            .bind(id)
            .bind(format!("Chat identity lifecycle {id}"))
            .execute(&pool)
            .await?;
    }
    for (id, scope) in [
        (owner, tenant),
        (revoked_owner, tenant),
        (other, tenant),
        (same_tenant_local, tenant),
        (foreign, foreign_tenant),
    ] {
        sqlx::query("INSERT INTO users (id, username, email, password_hash, display_name, storage_quota, tenant_id)
                     VALUES ($1, $2, $3, 'test-only', $2, 1024, $4)")
            .bind(id).bind(format!("chat_lifecycle_{}", id.simple()))
            .bind(format!("{id}@test.local")).bind(scope).execute(&pool).await?;
    }
    let store = ChatIdentityStore::new(pool.clone());
    let (original, original_binding) = issue(&store, tenant, revoked_owner, '1', None).await?;
    assert!(
        store
            .consume_and_activate(&original, &original_binding, Utc::now())
            .await?
    );
    store
        .revoke_principal(TenantId(tenant), PrincipalId(revoked_owner))
        .await?;

    for (scope, principal) in [(tenant, revoked_owner), (tenant, other), (foreign_tenant, foreign)] {
        let (challenge, binding) = issue(&store, scope, principal, '1', None).await?;
        assert!(
            !store
                .consume_and_activate(&challenge, &binding, Utc::now())
                .await?,
            "a previously bound key cannot be restored, reassigned or reused across tenants"
        );
    }
    let (implicit_replacement, implicit_binding) =
        issue(&store, tenant, revoked_owner, '2', None).await?;
    assert!(
        !store
            .consume_and_activate(&implicit_replacement, &implicit_binding, Utc::now())
            .await?,
        "a revoked Principal cannot bypass replacement verification by omitting rotation_of"
    );
    let consumed: bool = sqlx::query_scalar(
        "SELECT consumed_at IS NOT NULL FROM chat_binding_challenges WHERE challenge_id = $1",
    )
    .bind(implicit_replacement.challenge_id)
    .fetch_one(&pool)
    .await?;
    assert!(!consumed, "implicit replacement must not consume the challenge");
    assert!(store
        .active_binding(TenantId(tenant), PrincipalId(revoked_owner))
        .await?
        .is_none());
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
    let (second_key, second_binding) = issue(&store, tenant, owner, '9', None).await?;
    assert!(
        !store
            .consume_and_activate(&second_key, &second_binding, Utc::now())
            .await?,
        "a distinct key without rotation_of is ineligible while another key is active"
    );
    let inserted: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM chat_identity_bindings WHERE buzz_pubkey = $1",
    )
    .bind(&second_key.buzz_pubkey)
    .fetch_one(&pool)
    .await?;
    assert_eq!(inserted, 0, "ineligible activation must not insert a binding");
    let consumed: bool = sqlx::query_scalar(
        "SELECT consumed_at IS NOT NULL FROM chat_binding_challenges WHERE challenge_id = $1",
    )
    .bind(second_key.challenge_id)
    .fetch_one(&pool)
    .await?;
    assert!(!consumed, "ineligible activation must not consume the challenge");
    let current = store
        .active_binding(TenantId(tenant), PrincipalId(owner))
        .await?
        .unwrap();
    assert_eq!(current.binding_id, replacement_binding.binding_id);

    // Keep a real existing admission present while proving a rejected
    // replacement leaves every durable authorization surface unchanged.
    let mapping_id = Uuid::new_v4();
    let workspace_id = Uuid::new_v4();
    let community_id = format!("chat-lifecycle-{tenant}");
    sqlx::query(
        "INSERT INTO chat_workspace_communities
            (mapping_id, tenant_id, workspace_id, community_id, relay_url)
         VALUES ($1, $2, $3, $4, 'wss://relay.example.test')",
    )
    .bind(mapping_id)
    .bind(tenant)
    .bind(workspace_id)
    .bind(&community_id)
    .execute(&pool)
    .await?;
    sqlx::query(
        "INSERT INTO chat_buzz_admissions
            (admission_id, tenant_id, mapping_id, binding_id, buzz_pubkey)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(Uuid::new_v4())
    .bind(tenant)
    .bind(mapping_id)
    .bind(current.binding_id)
    .bind(&current.buzz_pubkey)
    .execute(&pool)
    .await?;
    assert!(store
        .active_admission(TenantId(tenant), &community_id, &current.buzz_pubkey)
        .await?);
    let durable_state_before: (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT count(*) FROM chat_identity_bindings
             WHERE tenant_id = $1 AND principal_id = $2),
            (SELECT count(*) FROM chat_buzz_admissions WHERE tenant_id = $1),
            (SELECT count(*) FROM integration_outbox WHERE tenant_id = $1
             AND source = 'elembra://io.elembra.chat'),
            (SELECT count(*) FROM user_security_events
             WHERE user_id = $2 AND event_type LIKE 'chat.identity.%')",
    )
    .bind(tenant)
    .bind(owner)
    .fetch_one(&pool)
    .await?;

    let (mismatched_challenge, mismatched_binding) = issue(
        &store,
        tenant,
        owner,
        '7',
        Some(current.binding_id),
    )
    .await?;
    let mut wrong_tenant_binding = mismatched_binding.clone();
    wrong_tenant_binding.tenant_id = TenantId(foreign_tenant);
    let mut wrong_principal_binding = mismatched_binding.clone();
    wrong_principal_binding.principal_id = PrincipalId(other);
    let mut wrong_key_binding = mismatched_binding.clone();
    wrong_key_binding.buzz_pubkey = '8'.to_string().repeat(64);
    let mut missing_rotation_binding = mismatched_binding.clone();
    missing_rotation_binding.rotation_of = None;
    for binding in [
        wrong_tenant_binding,
        wrong_principal_binding,
        wrong_key_binding,
        missing_rotation_binding,
    ] {
        assert!(!store
            .consume_and_activate(&mismatched_challenge, &binding, Utc::now())
            .await?);
    }

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
        let consumed: bool = sqlx::query_scalar(
            "SELECT consumed_at IS NOT NULL FROM chat_binding_challenges WHERE challenge_id = $1",
        )
        .bind(challenge.challenge_id)
        .fetch_one(&pool)
        .await?;
        assert!(!consumed, "out-of-scope rotation challenges remain unconsumed");
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
    let (left_result, right_result) = tokio::join!(
        store.consume_and_activate(&left, &left_binding, Utc::now()),
        store.consume_and_activate(&right, &right_binding, Utc::now()),
    );
    assert!(
        !left_result? && !right_result?,
        "replacement stays disabled until Buzz confirmation exists"
    );
    let current = store
        .active_binding(TenantId(tenant), PrincipalId(owner))
        .await?
        .unwrap();
    assert_eq!(current.binding_id, replacement_binding.binding_id);
    for challenge in [&left, &right] {
        let consumed: bool = sqlx::query_scalar(
            "SELECT consumed_at IS NOT NULL FROM chat_binding_challenges WHERE challenge_id = $1",
        )
        .bind(challenge.challenge_id)
        .fetch_one(&pool)
        .await?;
        assert!(!consumed, "rejected replacement challenges remain unconsumed");
    }
    let consumed: bool = sqlx::query_scalar(
        "SELECT consumed_at IS NOT NULL FROM chat_binding_challenges WHERE challenge_id = $1",
    )
    .bind(mismatched_challenge.challenge_id)
    .fetch_one(&pool)
    .await?;
    assert!(!consumed, "binding-scope mismatches cannot consume a rotation challenge");
    let durable_state_after: (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT count(*) FROM chat_identity_bindings
             WHERE tenant_id = $1 AND principal_id = $2),
            (SELECT count(*) FROM chat_buzz_admissions WHERE tenant_id = $1),
            (SELECT count(*) FROM integration_outbox WHERE tenant_id = $1
             AND source = 'elembra://io.elembra.chat'),
            (SELECT count(*) FROM user_security_events
             WHERE user_id = $2 AND event_type LIKE 'chat.identity.%')",
    )
    .bind(tenant)
    .bind(owner)
    .fetch_one(&pool)
    .await?;
    assert_eq!(durable_state_after, durable_state_before);
    assert!(store
        .active_admission(TenantId(tenant), &community_id, &current.buzz_pubkey)
        .await?);
    let replacement_admissions: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM chat_buzz_admissions
         WHERE tenant_id = $1 AND buzz_pubkey = ANY($2)",
    )
    .bind(tenant)
    .bind(vec![
        mismatched_challenge.buzz_pubkey.clone(),
        left.buzz_pubkey.clone(),
        right.buzz_pubkey.clone(),
    ])
    .fetch_one(&pool)
    .await?;
    assert_eq!(replacement_admissions, 0);
    let replacement_outbox_events: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM integration_outbox
         WHERE tenant_id = $1
           AND event_type = 'io.elembra.chat.buzz.admission.requested.v1'
           AND event_json #>> '{data,buzz_pubkey}' = ANY($2)",
    )
    .bind(tenant)
    .bind(vec![
        mismatched_challenge.buzz_pubkey.clone(),
        left.buzz_pubkey.clone(),
        right.buzz_pubkey.clone(),
    ])
    .fetch_one(&pool)
    .await?;
    assert_eq!(replacement_outbox_events, 0);

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
    let (local_challenge, local_binding) =
        issue(&store, tenant, same_tenant_local, '8', None).await?;
    let (foreign_challenge, foreign_binding) =
        issue(&store, foreign_tenant, foreign, '8', None).await?;
    let mut key_blocker = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(format!("chat-binding:{}", local_challenge.buzz_pubkey))
        .execute(&mut *key_blocker)
        .await?;
    let activations = async {
        tokio::join!(
            store.consume_and_activate(&local_challenge, &local_binding, Utc::now()),
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
    let (local_succeeded, foreign_succeeded) = (owner_result?, foreign_result?);
    assert_ne!(
        local_succeeded, foreign_succeeded,
        "exactly one cross-tenant activation of the same key must succeed"
    );
    let claimed: Vec<(Uuid, Uuid)> = sqlx::query_as(
        "SELECT tenant_id, principal_id FROM chat_identity_bindings WHERE buzz_pubkey = $1",
    )
    .bind(&local_challenge.buzz_pubkey)
    .fetch_all(&pool)
    .await?;
    let expected = if local_succeeded {
        (tenant, same_tenant_local)
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
    let (implicit_challenge, implicit_binding) = issue(&store, tenant, owner, '6', None).await?;
    let (rotation, implicit_rotation, revocation) = tokio::join!(
        store.consume_and_activate(&challenge, &binding, Utc::now()),
        store.consume_and_activate(&implicit_challenge, &implicit_binding, Utc::now()),
        store.revoke_principal(TenantId(tenant), PrincipalId(owner)),
    );
    assert!(!rotation?, "replacement stays unavailable during revocation");
    assert!(!implicit_rotation?, "omitted rotation intent cannot race revocation");
    revocation?;
    assert!(
        store
            .active_binding(TenantId(tenant), PrincipalId(owner))
            .await?
            .is_none(),
        "explicit revocation must also cover a competing rotation"
    );
    let durable_state_before: (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT count(*) FROM chat_identity_bindings WHERE tenant_id = $1),
            (SELECT count(*) FROM chat_buzz_admissions WHERE tenant_id = $1),
            (SELECT count(*) FROM integration_outbox WHERE tenant_id = $1),
            (SELECT count(*) FROM user_security_events WHERE user_id = $2)",
    )
    .bind(tenant)
    .bind(owner)
    .fetch_one(&pool)
    .await?;
    assert!(
        !store
            .consume_and_activate(&implicit_challenge, &implicit_binding, Utc::now())
            .await?,
        "a challenge issued before revocation cannot activate a replacement afterward"
    );
    let consumed: bool = sqlx::query_scalar(
        "SELECT consumed_at IS NOT NULL FROM chat_binding_challenges WHERE challenge_id = $1",
    )
    .bind(implicit_challenge.challenge_id)
    .fetch_one(&pool)
    .await?;
    assert!(!consumed);
    let durable_state_after: (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT count(*) FROM chat_identity_bindings WHERE tenant_id = $1),
            (SELECT count(*) FROM chat_buzz_admissions WHERE tenant_id = $1),
            (SELECT count(*) FROM integration_outbox WHERE tenant_id = $1),
            (SELECT count(*) FROM user_security_events WHERE user_id = $2)",
    )
    .bind(tenant)
    .bind(owner)
    .fetch_one(&pool)
    .await?;
    assert_eq!(durable_state_after, durable_state_before);
    assert!(!store
        .active_admission(TenantId(tenant), &community_id, &current.buzz_pubkey)
        .await?);
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
        vec![revoked_owner],
        "historical attribution must be preserved"
    );

    Ok(())
    }
    .await;

    let mut cleanup_errors = Vec::new();
    for statement in [
        "DELETE FROM chat_binding_challenges WHERE tenant_id = ANY($1)",
        "DELETE FROM chat_buzz_admissions WHERE tenant_id = ANY($1)",
        "DELETE FROM chat_workspace_communities WHERE tenant_id = ANY($1)",
        "DELETE FROM chat_identity_bindings WHERE tenant_id = ANY($1)",
        "DELETE FROM users WHERE tenant_id = ANY($1)",
        "DELETE FROM tenants WHERE id = ANY($1)",
    ] {
        if let Err(error) = sqlx::query(statement)
            .bind(vec![tenant, foreign_tenant])
            .execute(&pool)
            .await
        {
            cleanup_errors.push(format!("{statement}: {error}"));
        }
    }
    if let Err(error) = attempt {
        anyhow::ensure!(
            cleanup_errors.is_empty(),
            "attempt failed: {error}; cleanup errors: {}",
            cleanup_errors.join("; ")
        );
        return Err(error);
    }
    anyhow::ensure!(
        cleanup_errors.is_empty(),
        "failed to clean up Chat identity fixtures: {}",
        cleanup_errors.join("; ")
    );
    Ok(())
}

#[derive(Clone, Copy)]
enum AdmissionRace {
    ExplicitRevocation,
    Disablement,
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires explicitly configured disposable local PostgreSQL"]
async fn admission_is_serialized_with_principal_revocation_and_disablement() -> Result<()> {
    assert_eq!(
        std::env::var("RUSTSHARE_TEST_DISPOSABLE_DB").as_deref(),
        Ok("1")
    );
    let database_url = std::env::var("DATABASE_URL")?;
    let options: PgConnectOptions = database_url.parse()?;
    assert!(matches!(
        options.get_host(),
        "127.0.0.1" | "localhost" | "::1"
    ));
    assert!(options
        .get_database()
        .is_some_and(|db| db == "rustshare_test" || db.starts_with("rustshare_test_")));
    let application_name = format!("chat-admission-race-{}", Uuid::new_v4());
    let pool = PgPool::connect_with(options.application_name(&application_name)).await?;

    for (race, original_key) in [
        (AdmissionRace::ExplicitRevocation, 'a'),
        (AdmissionRace::Disablement, 'b'),
    ] {
        let tenant = Uuid::new_v4();
        let principal = Uuid::new_v4();
        let attempt: Result<()> = async {
            sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, $2)")
                .bind(tenant)
                .bind(format!("Chat admission race {tenant}"))
                .execute(&pool)
                .await?;
            sqlx::query(
                "INSERT INTO users (id, username, email, password_hash, display_name, storage_quota, tenant_id)
                 VALUES ($1, $2, $3, 'test-only', $2, 1024, $4)",
            )
            .bind(principal)
            .bind(format!("chat_admission_{}", principal.simple()))
            .bind(format!("{principal}@test.local"))
            .bind(tenant)
            .execute(&pool)
            .await?;
            let store = ChatIdentityStore::new(pool.clone());
            let (challenge, binding) = issue(&store, tenant, principal, original_key, None).await?;
            anyhow::ensure!(
                store
                    .consume_and_activate(&challenge, &binding, Utc::now())
                    .await?
            );

            let seed_workspace = Uuid::new_v4();
            let race_workspace = Uuid::new_v4();
            let seed_mapping = Uuid::new_v4();
            let race_mapping = Uuid::new_v4();
            for (mapping, workspace, community) in [
                (seed_mapping, seed_workspace, format!("seed-{tenant}")),
                (race_mapping, race_workspace, format!("race-{tenant}")),
            ] {
                sqlx::query(
                    "INSERT INTO chat_workspace_communities
                        (mapping_id, tenant_id, workspace_id, community_id, relay_url)
                     VALUES ($1, $2, $3, $4, 'wss://relay.example.test')",
                )
                .bind(mapping)
                .bind(tenant)
                .bind(workspace)
                .bind(community)
                .execute(&pool)
                .await?;
                sqlx::query(
                    "INSERT INTO application_enablements
                        (tenant_id, workspace_id, application_id, enabled)
                     VALUES ($1, $2, 'io.elembra.chat', true)",
                )
                .bind(tenant)
                .bind(workspace)
                .execute(&pool)
                .await?;
            }
            anyhow::ensure!(
                store
                    .admit_binding(
                        TenantId(tenant),
                        PrincipalId(principal),
                        WorkspaceId(seed_workspace),
                    )
                    .await?
            );
            sqlx::query(
                "CREATE FUNCTION chat_lifecycle_admission_barrier() RETURNS trigger
                 LANGUAGE plpgsql AS $$
                 BEGIN
                     PERFORM pg_advisory_xact_lock(hashtextextended(TG_ARGV[0], 0));
                     RETURN NEW;
                 END $$",
            )
            .execute(&pool)
            .await?;
            let barrier_key = format!("chat-admission-test-{tenant}");
            sqlx::query(&format!(
                "CREATE TRIGGER chat_lifecycle_admission_barrier
                 BEFORE INSERT ON chat_buzz_admissions FOR EACH ROW
                 EXECUTE FUNCTION chat_lifecycle_admission_barrier('{barrier_key}')"
            ))
            .execute(&pool)
            .await?;
            let mut blocker = pool.begin().await?;
            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
                .bind(&barrier_key)
                .execute(&mut *blocker)
                .await?;

            let admission = tokio::spawn({
                let store = store.clone();
                async move {
                    store
                        .admit_binding(
                            TenantId(tenant),
                            PrincipalId(principal),
                            WorkspaceId(race_workspace),
                        )
                        .await
                }
            });
            let admission_blocked = tokio::time::timeout(
                std::time::Duration::from_secs(10),
                async {
                    loop {
                        let waiting: i64 = sqlx::query_scalar(
                            "SELECT count(*) FROM pg_stat_activity
                             WHERE application_name = $1 AND wait_event_type = 'Lock'
                               AND query ILIKE '%chat_buzz_admissions%'",
                        )
                        .bind(&application_name)
                        .fetch_one(&pool)
                        .await?;
                        if waiting == 1 {
                            break;
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                    }
                    Ok::<_, sqlx::Error>(())
                },
            )
            .await;
            if let Err(error) = admission_blocked {
                blocker.commit().await?;
                admission.await??;
                return Err(anyhow!("admission did not reach the barrier: {error}"));
            }
            admission_blocked??;

            let counter = tokio::spawn({
                let store = store.clone();
                let pool = pool.clone();
                async move {
                    match race {
                        AdmissionRace::ExplicitRevocation => {
                            store
                                .revoke_principal(TenantId(tenant), PrincipalId(principal))
                                .await?;
                        }
                        AdmissionRace::Disablement => {
                            sqlx::query("UPDATE users SET disabled_at = now() WHERE id = $1")
                                .bind(principal)
                                .execute(&pool)
                                .await?;
                        }
                    }
                    Ok::<_, anyhow::Error>(())
                }
            });
            let counter_ready = tokio::time::timeout(
                std::time::Duration::from_secs(10),
                async {
                    loop {
                        if counter.is_finished() {
                            break;
                        }
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
                },
            )
            .await;
            if let Err(error) = counter_ready {
                blocker.commit().await?;
                admission.await??;
                counter.await??;
                return Err(anyhow!("competing operation did not reach its barrier: {error}"));
            }
            counter_ready??;
            blocker.commit().await?;

            anyhow::ensure!(admission.await??);
            counter.await??;
            let active: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM chat_buzz_admissions a
                 JOIN chat_identity_bindings b ON b.tenant_id = a.tenant_id AND b.binding_id = a.binding_id
                 WHERE a.tenant_id = $1 AND b.principal_id = $2 AND a.active",
            )
            .bind(tenant)
            .bind(principal)
            .fetch_one(&pool)
            .await?;
            anyhow::ensure!(active == 0, "a revoked Principal retains {active} active admissions");
            let historical: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM chat_buzz_admissions WHERE tenant_id = $1",
            )
            .bind(tenant)
            .fetch_one(&pool)
            .await?;
            anyhow::ensure!(historical == 2, "admission history must be preserved");
            let requested: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM integration_outbox
                 WHERE tenant_id = $1 AND event_type = 'io.elembra.chat.buzz.admission.requested.v1'",
            )
            .bind(tenant)
            .fetch_one(&pool)
            .await?;
            let revoked: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM integration_outbox
                 WHERE tenant_id = $1 AND event_type = 'io.elembra.chat.buzz.admission.revoked.v1'",
            )
            .bind(tenant)
            .fetch_one(&pool)
            .await?;
            anyhow::ensure!(
                requested == 2 && revoked == 2,
                "outbox must contain both admissions followed by both revocations, got {requested} admissions and {revoked} revocations"
            );
            Ok(())
        }
        .await;

        let mut cleanup_errors = Vec::new();
        for statement in [
            "DROP TRIGGER IF EXISTS chat_lifecycle_admission_barrier ON chat_buzz_admissions",
            "DROP FUNCTION IF EXISTS chat_lifecycle_admission_barrier",
            "DELETE FROM integration_outbox WHERE tenant_id = $1",
            "DELETE FROM chat_buzz_admissions WHERE tenant_id = $1",
            "DELETE FROM chat_binding_challenges WHERE tenant_id = $1",
            "DELETE FROM chat_identity_bindings WHERE tenant_id = $1",
            "DELETE FROM chat_workspace_communities WHERE tenant_id = $1",
            "DELETE FROM application_enablements WHERE tenant_id = $1",
            "DELETE FROM users WHERE tenant_id = $1",
            "DELETE FROM tenants WHERE id = $1",
        ] {
            let result = if statement.contains("$1") {
                sqlx::query(statement).bind(tenant).execute(&pool).await
            } else {
                sqlx::query(statement).execute(&pool).await
            };
            if let Err(error) = result {
                cleanup_errors.push(format!("{statement}: {error}"));
            }
        }
        if let Err(error) = attempt {
            return Err(anyhow!(
                "admission race attempt failed: {error}; cleanup errors: {}",
                cleanup_errors.join("; ")
            ));
        }
        anyhow::ensure!(cleanup_errors.is_empty(), "{}", cleanup_errors.join("; "));
    }
    Ok(())
}

#[tokio::test]
#[ignore = "requires explicitly configured disposable local PostgreSQL"]
async fn membership_protocol_transition_and_writers_are_versioned() -> Result<()> {
    assert_eq!(
        std::env::var("RUSTSHARE_TEST_DISPOSABLE_DB").as_deref(),
        Ok("1")
    );
    let options: PgConnectOptions = std::env::var("DATABASE_URL")?.parse()?;
    assert!(matches!(
        options.get_host(),
        "127.0.0.1" | "localhost" | "::1"
    ));
    assert!(options
        .get_database()
        .is_some_and(|db| db == "rustshare_test" || db.starts_with("rustshare_test_")));
    let pool = PgPool::connect_with(
        options.application_name(&format!("chat-protocol-{}", Uuid::new_v4())),
    )
    .await?;
    let tenant = Uuid::new_v4();
    let principal_v1 = Uuid::new_v4();
    let principal_v2 = Uuid::new_v4();
    let principal_deleted = Uuid::new_v4();
    let workspace = Uuid::new_v4();
    let community = Uuid::new_v4();
    let result: Result<()> = async {
        sqlx::query("INSERT INTO tenants (id,name) VALUES ($1,$2)").bind(tenant).bind(format!("membership protocol {tenant}")).execute(&pool).await?;
        for principal in [principal_v1, principal_v2, principal_deleted] {
            sqlx::query("INSERT INTO users (id,username,email,password_hash,display_name,storage_quota,tenant_id) VALUES ($1,$2,$3,'test-only',$2,1024,$4)")
                .bind(principal).bind(format!("chat_protocol_{}",principal.simple())).bind(format!("{principal}@test.local")).bind(tenant).execute(&pool).await?;
        }
        sqlx::query("INSERT INTO chat_workspace_communities (mapping_id,tenant_id,workspace_id,community_id,relay_url,relay_pubkey) VALUES ($1,$2,$3,$4,'wss://relay.example.test',$5)")
            .bind(Uuid::new_v4()).bind(tenant).bind(workspace).bind(community.to_string()).bind("1".repeat(64)).execute(&pool).await?;
        sqlx::query("INSERT INTO application_enablements (tenant_id,workspace_id,application_id,enabled) VALUES ($1,$2,'io.elembra.chat',true)")
            .bind(tenant).bind(workspace).execute(&pool).await?;
        let store = ChatIdentityStore::new(pool.clone());
        let (v1_challenge,v1_binding)=issue(&store,tenant,principal_v1,'1',None).await?;
        anyhow::ensure!(store.consume_and_activate(&v1_challenge,&v1_binding,Utc::now()).await?);
        anyhow::ensure!(store.admit_binding(TenantId(tenant),PrincipalId(principal_v1),WorkspaceId(workspace)).await?);
        let v1_event: (Uuid,serde_json::Value,serde_json::Value)=sqlx::query_as("SELECT event_id,event_json->'data',event_json FROM integration_outbox WHERE tenant_id=$1 AND event_type='io.elembra.chat.buzz.admission.requested.v1'")
            .bind(tenant).fetch_one(&pool).await?;
        anyhow::ensure!(v1_event.1.get("command_version").is_none(),"default mapping must retain the v1alpha1 payload");
        anyhow::ensure!(!store.update_mapping_relay(TenantId(tenant),WorkspaceId(workspace),"wss://changed.example.test".into(),Some("1".repeat(64)),"v1alpha2").await?,"unprocessed legacy event must prevent opt-in");
        let unchanged: (String,String)=sqlx::query_as("SELECT relay_url,membership_protocol_version FROM chat_workspace_communities WHERE tenant_id=$1 AND workspace_id=$2").bind(tenant).bind(workspace).fetch_one(&pool).await?;
        anyhow::ensure!(unchanged == ("wss://relay.example.test".to_string(),"v1alpha1".to_string()),"rejected opt-in must not partially update the mapping");
        sqlx::query("INSERT INTO integration_deliveries (consumer_id,source,event_id,event_type,tenant_id,workspace_id,state,processed_at) SELECT 'io.elembra.chat.buzz-bridge.v1',source,event_id,event_type,tenant_id,workspace_id,'processed',now() FROM integration_outbox WHERE event_id=$1")
            .bind(v1_event.0).execute(&pool).await?;
        anyhow::ensure!(store.update_mapping_relay(TenantId(tenant),WorkspaceId(workspace),"wss://relay.example.test".into(),Some("1".repeat(64)),"v1alpha2").await?,"drained legacy event should allow explicit opt-in");
        let delayed_legacy: IntegrationEvent = serde_json::from_value(v1_event.2.clone())?;
        let service_keys = nostr::Keys::generate();
        let previous_bridge_secret = std::env::var_os("RUSTSHARE_CHAT_BRIDGE_SECRET_KEY");
        std::env::set_var("RUSTSHARE_CHAT_BRIDGE_SECRET_KEY", service_keys.secret_key().to_secret_hex());
        let bridge = rustshare_server::buzz_bridge::BuzzAdmissionBridge::from_env_with_pool(pool.clone()).unwrap();
        let delayed_result = bridge.process(&delayed_legacy).await;
        if let Some(previous) = previous_bridge_secret {
            std::env::set_var("RUSTSHARE_CHAT_BRIDGE_SECRET_KEY", previous);
        } else {
            std::env::remove_var("RUSTSHARE_CHAT_BRIDGE_SECRET_KEY");
        }
        anyhow::ensure!(matches!(delayed_result, ConsumerOutcome::Permanent { .. }),"delayed unversioned event must be rejected after v1alpha2 opt-in");
        let (v2_challenge,v2_binding)=issue(&store,tenant,principal_v2,'2',None).await?;
        anyhow::ensure!(store.consume_and_activate(&v2_challenge,&v2_binding,Utc::now()).await?);
        anyhow::ensure!(store.admit_binding(TenantId(tenant),PrincipalId(principal_v2),WorkspaceId(workspace)).await?);
        let v2_event: (Uuid,serde_json::Value)=sqlx::query_as("SELECT event_id,event_json->'data' FROM integration_outbox WHERE tenant_id=$1 AND event_type='io.elembra.chat.buzz.admission.requested.v1' AND event_id<>$2")
            .bind(tenant).bind(v1_event.0).fetch_one(&pool).await?;
        anyhow::ensure!(v2_event.1.get("command_version").and_then(|v|v.as_i64())==Some(1));
        anyhow::ensure!(v2_event.1.get("command_id").and_then(|v|v.as_str())==Some(v2_event.0.to_string().as_str()));
        store.revoke_principal(TenantId(tenant),PrincipalId(principal_v2)).await?;
        let revoked: serde_json::Value=sqlx::query_scalar("SELECT event_json->'data' FROM integration_outbox WHERE tenant_id=$1 AND event_type='io.elembra.chat.buzz.admission.revoked.v1' AND event_json->'data'->>'admission_id'=(SELECT admission_id::text FROM chat_buzz_admissions WHERE tenant_id=$1 AND binding_id=$2)")
            .bind(tenant).bind(v2_binding.binding_id).fetch_one(&pool).await?;
        anyhow::ensure!(revoked.get("command_version").and_then(|v|v.as_i64())==Some(1),"admin revocation uses the selected v1alpha2 contract");
        sqlx::query("UPDATE users SET disabled_at=now() WHERE id=$1 AND tenant_id=$2")
            .bind(principal_v1).bind(tenant).execute(&pool).await?;
        let offboarded: serde_json::Value=sqlx::query_scalar("SELECT event_json->'data' FROM integration_outbox WHERE tenant_id=$1 AND event_type='io.elembra.chat.buzz.admission.revoked.v1' AND event_json->'data'->>'admission_id'=(SELECT admission_id::text FROM chat_buzz_admissions WHERE tenant_id=$1 AND binding_id=(SELECT binding_id FROM chat_identity_bindings WHERE principal_id=$2))")
            .bind(tenant).bind(principal_v1).fetch_one(&pool).await?;
        anyhow::ensure!(offboarded.get("command_version").and_then(|v|v.as_i64())==Some(1),"Principal offboarding uses the selected v1alpha2 contract");
        let (deleted_challenge,deleted_binding)=issue(&store,tenant,principal_deleted,'3',None).await?;
        anyhow::ensure!(store.consume_and_activate(&deleted_challenge,&deleted_binding,Utc::now()).await?);
        anyhow::ensure!(store.admit_binding(TenantId(tenant),PrincipalId(principal_deleted),WorkspaceId(workspace)).await?);
        sqlx::query("DELETE FROM users WHERE id=$1 AND tenant_id=$2").bind(principal_deleted).bind(tenant).execute(&pool).await?;
        let deleted_events:i64=sqlx::query_scalar("SELECT count(*) FROM integration_outbox WHERE tenant_id=$1 AND event_type='io.elembra.chat.buzz.admission.revoked.v1' AND event_json->'data'->>'command_version'='1'").bind(tenant).fetch_one(&pool).await?;
        anyhow::ensure!(deleted_events==3,"disable, delete, and admin revoke all retain v1alpha2 operation metadata");
        Ok(())
    }.await;
    let cleanup = async {
        sqlx::query("DELETE FROM integration_outbox WHERE tenant_id=$1")
            .bind(tenant)
            .execute(&pool)
            .await?;
        sqlx::query("DELETE FROM chat_buzz_admissions WHERE tenant_id=$1")
            .bind(tenant)
            .execute(&pool)
            .await?;
        sqlx::query("DELETE FROM chat_binding_challenges WHERE tenant_id=$1")
            .bind(tenant)
            .execute(&pool)
            .await?;
        sqlx::query("DELETE FROM chat_identity_bindings WHERE tenant_id=$1")
            .bind(tenant)
            .execute(&pool)
            .await?;
        sqlx::query("DELETE FROM chat_workspace_communities WHERE tenant_id=$1")
            .bind(tenant)
            .execute(&pool)
            .await?;
        sqlx::query("DELETE FROM application_enablements WHERE tenant_id=$1")
            .bind(tenant)
            .execute(&pool)
            .await?;
        sqlx::query("DELETE FROM users WHERE tenant_id=$1")
            .bind(tenant)
            .execute(&pool)
            .await?;
        sqlx::query("DELETE FROM tenants WHERE id=$1")
            .bind(tenant)
            .execute(&pool)
            .await?;
        Ok::<(), sqlx::Error>(())
    }
    .await;
    result?;
    cleanup?;
    Ok(())
}
