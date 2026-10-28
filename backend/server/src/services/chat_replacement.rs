//! Disabled durable state machine for Buzz identity replacement.

use crate::{
    buzz_gateway::{
        BuzzGatewayClient, BuzzMembership, BuzzMembershipObservation, BuzzMembershipRequest,
    },
    services::chat_membership::accept_buzz_membership_in_tx,
};
use rustshare_core::domain::{PrincipalId, TenantId, WorkspaceId};
use rustshare_integration_events::{ActorRef, IntegrationEvent};
use rustshare_resource_auth::{BuzzAdmissionOperation, BuzzAuthorityError};
use rustshare_storage::{ChatIdentityStore, OutboxStore, ReplacementLease};
use sha2::Digest as _;
use sqlx::{PgPool, Row, Transaction};
use uuid::Uuid;

const MAX_TRANSIENT_FAILURES: i16 = 5;

pub struct MembershipRead {
    pub request_id: Uuid,
    pub observation: BuzzMembershipObservation,
}

async fn revision_floor(
    pool: &PgPool,
    community: Uuid,
    pubkey: &str,
) -> Result<u64, BuzzAuthorityError> {
    let revision: Option<i64> = sqlx::query_scalar(
        "SELECT MAX(accepted_revision) FROM chat_buzz_membership_verifications
         WHERE community_id=$1 AND pubkey=$2 AND accepted_at IS NOT NULL",
    )
    .bind(community)
    .bind(pubkey)
    .fetch_one(pool)
    .await
    .map_err(|_| BuzzAuthorityError::Transport("cannot read membership revision floor".into()))?;
    revision.unwrap_or(0).try_into().map_err(|_| {
        BuzzAuthorityError::InvalidResponse("invalid membership revision floor".into())
    })
}

async fn read_membership(
    pool: &PgPool,
    client: &BuzzGatewayClient,
    lease: &ReplacementLease,
    pubkey: &str,
    expected_state: Option<&str>,
) -> Result<MembershipRead, BuzzAuthorityError> {
    let key = nostr::PublicKey::from_hex(pubkey)
        .map_err(|_| BuzzAuthorityError::Config("invalid replacement public key".into()))?;
    if key.to_hex() != pubkey {
        return Err(BuzzAuthorityError::Config(
            "noncanonical replacement public key".into(),
        ));
    }
    let minimum_revision = revision_floor(pool, lease.community_id, pubkey).await?;
    let minimum_db = i64::try_from(minimum_revision)
        .map_err(|_| BuzzAuthorityError::Config("membership revision overflow".into()))?;
    let request = BuzzMembershipRequest {
        request_id: Uuid::new_v4(),
        community_id: lease.community_id,
        pubkey,
    };
    sqlx::query("INSERT INTO chat_buzz_membership_verifications
        (request_id,community_id,pubkey,relay_pubkey,minimum_revision,tenant_id,operation_id,expected_state)
        VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
        .bind(request.request_id).bind(lease.community_id).bind(pubkey).bind(&lease.relay_pubkey)
        .bind(minimum_db).bind(lease.tenant_id.0).bind(lease.operation_id).bind(expected_state)
        .execute(pool).await.map_err(|_| BuzzAuthorityError::Transport("cannot persist replacement membership request".into()))?;
    let observation = client
        .check_membership(
            &lease.relay_url,
            &lease.relay_pubkey,
            &request,
            minimum_revision,
        )
        .await?;
    Ok(MembershipRead {
        request_id: request.request_id,
        observation,
    })
}

fn digest(mapping_id: Uuid, community_id: Uuid, relay_url: &str, relay_pubkey: &str) -> Vec<u8> {
    sha2::Sha256::digest(
        format!("{mapping_id}\0{community_id}\0{relay_url}\0{relay_pubkey}").as_bytes(),
    )
    .to_vec()
}

async fn lock_principal(
    tx: &mut Transaction<'_, sqlx::Postgres>,
    lease: &ReplacementLease,
) -> Result<bool, BuzzAuthorityError> {
    let row = sqlx::query_scalar::<_, Uuid>("SELECT id FROM users WHERE id=$1 AND tenant_id=$2 AND disabled_at IS NULL FOR NO KEY UPDATE")
        .bind(lease.principal_id.0).bind(lease.tenant_id.0).fetch_optional(&mut **tx).await
        .map_err(|_| BuzzAuthorityError::Transport("cannot lock replacement Principal".into()))?;
    Ok(row.is_some())
}

async fn chat_enabled(
    tx: &mut Transaction<'_, sqlx::Postgres>,
    lease: &ReplacementLease,
) -> Result<bool, BuzzAuthorityError> {
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM application_enablements ae
         LEFT JOIN application_user_preferences p ON p.user_id=$3 AND p.application_id='io.elembra.chat'
         WHERE ae.tenant_id=$1 AND ae.workspace_id=$2 AND ae.application_id='io.elembra.chat'
           AND ae.enabled AND COALESCE(p.enabled,true))",
    )
    .bind(lease.tenant_id.0)
    .bind(lease.workspace_id.0)
    .bind(lease.principal_id.0)
    .fetch_one(&mut **tx)
    .await
    .map_err(|_| BuzzAuthorityError::Transport("cannot verify Chat authorization".into()))
}

async fn accept_read(
    tx: &mut Transaction<'_, sqlx::Postgres>,
    lease: &ReplacementLease,
    read: &MembershipRead,
) -> Result<(), BuzzAuthorityError> {
    let owner = sqlx::query(
        "SELECT tenant_id,operation_id FROM chat_buzz_membership_verifications WHERE request_id=$1",
    )
    .bind(read.request_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|_| BuzzAuthorityError::Transport("cannot verify evidence owner".into()))?;
    let Some(owner) = owner else {
        return Err(BuzzAuthorityError::InvalidResponse(
            "membership evidence is missing".into(),
        ));
    };
    if owner.try_get::<Option<Uuid>, _>("tenant_id").ok().flatten() != Some(lease.tenant_id.0)
        || owner
            .try_get::<Option<Uuid>, _>("operation_id")
            .ok()
            .flatten()
            != Some(lease.operation_id)
    {
        return Err(BuzzAuthorityError::InvalidResponse(
            "membership evidence scope mismatch".into(),
        ));
    }
    accept_buzz_membership_in_tx(tx, read.request_id, &read.observation).await
}

async fn queue_command(
    pool: &PgPool,
    outbox: &OutboxStore,
    lease: &ReplacementLease,
    bridge_pubkey: &str,
    phase: &str,
    reads: &[&MembershipRead],
) -> Result<bool, BuzzAuthorityError> {
    let (expected_state, operation, target) = match phase {
        "revoke" if reads.len() == 1 && reads[0].observation.pubkey == lease.old_pubkey => (
            "pending_revocation",
            BuzzAdmissionOperation::Revoke,
            &reads[0].observation,
        ),
        "admit"
            if reads.len() == 2
                && reads[0].observation.pubkey == lease.old_pubkey
                && reads[1].observation.pubkey == lease.replacement_pubkey =>
        {
            (
                lease.state.as_str(),
                BuzzAdmissionOperation::Admit,
                &reads[1].observation,
            )
        }
        _ => {
            return Err(BuzzAuthorityError::Config(
                "invalid replacement command phase".into(),
            ))
        }
    };
    let actor = nostr::PublicKey::from_hex(bridge_pubkey)
        .map_err(|_| BuzzAuthorityError::Config("invalid bridge identity".into()))?;
    if actor.to_hex() != bridge_pubkey {
        return Err(BuzzAuthorityError::Config(
            "noncanonical bridge identity".into(),
        ));
    }
    if phase == "admit"
        && (reads[0].observation.membership != BuzzMembership::Revoked
            || reads[1].observation.membership != BuzzMembership::Absent)
    {
        record_blocked_failure(pool, lease, "replacement_precondition_failed").await?;
        return Ok(false);
    }
    if phase == "revoke" && reads[0].observation.community_id != lease.community_id.to_string() {
        return Ok(false);
    }
    let mut tx = pool
        .begin()
        .await
        .map_err(|_| BuzzAuthorityError::Transport("cannot begin replacement command".into()))?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL READ COMMITTED")
        .execute(&mut *tx)
        .await
        .map_err(|_| BuzzAuthorityError::Transport("cannot set replacement isolation".into()))?;
    if !lock_principal(&mut tx, lease).await? {
        return Ok(false);
    }
    if !chat_enabled(&mut tx, lease).await? {
        return Ok(false);
    }
    let op=sqlx::query("SELECT mapping_id,community_id,relay_url,relay_pubkey,mapping_fingerprint,state,generation,
        lease_token,lease_expires_at,replacement_binding_id FROM chat_identity_replacements
        WHERE tenant_id=$1 AND operation_id=$2 AND principal_id=$3 FOR UPDATE")
        .bind(lease.tenant_id.0).bind(lease.operation_id).bind(lease.principal_id.0).fetch_optional(&mut *tx).await
        .map_err(|_| BuzzAuthorityError::Transport("cannot lock replacement operation".into()))?;
    let Some(op) = op else { return Ok(false) };
    let state: String = op
        .try_get("state")
        .map_err(|_| BuzzAuthorityError::InvalidResponse("malformed replacement".into()))?;
    let generation: i64 = op
        .try_get("generation")
        .map_err(|_| BuzzAuthorityError::InvalidResponse("malformed replacement".into()))?;
    let token: Option<Uuid> = op
        .try_get("lease_token")
        .map_err(|_| BuzzAuthorityError::InvalidResponse("malformed replacement".into()))?;
    let expires: Option<chrono::DateTime<chrono::Utc>> = op
        .try_get("lease_expires_at")
        .map_err(|_| BuzzAuthorityError::InvalidResponse("malformed replacement".into()))?;
    if state != expected_state
        || generation != lease.generation
        || token != Some(lease.token)
        || expires.is_none_or(|t| t <= chrono::Utc::now())
    {
        return Ok(false);
    }
    let mapping_id: Uuid = op
        .try_get("mapping_id")
        .map_err(|_| BuzzAuthorityError::InvalidResponse("malformed mapping".into()))?;
    let community_id: Uuid = op
        .try_get("community_id")
        .map_err(|_| BuzzAuthorityError::InvalidResponse("malformed mapping".into()))?;
    let relay_url: String = op
        .try_get("relay_url")
        .map_err(|_| BuzzAuthorityError::InvalidResponse("malformed mapping".into()))?;
    let relay_pubkey: String = op
        .try_get("relay_pubkey")
        .map_err(|_| BuzzAuthorityError::InvalidResponse("malformed mapping".into()))?;
    let stored: Vec<u8> = op
        .try_get("mapping_fingerprint")
        .map_err(|_| BuzzAuthorityError::InvalidResponse("malformed mapping".into()))?;
    let mapped = sqlx::query(
        "SELECT community_id,relay_url,relay_pubkey FROM chat_workspace_communities
        WHERE tenant_id=$1 AND workspace_id=$2 AND mapping_id=$3 AND active FOR SHARE",
    )
    .bind(lease.tenant_id.0)
    .bind(lease.workspace_id.0)
    .bind(mapping_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| BuzzAuthorityError::Transport("cannot verify replacement mapping".into()))?;
    let Some(mapped) = mapped else {
        tx.rollback().await.map_err(|_| {
            BuzzAuthorityError::Transport("cannot release invalid replacement mapping".into())
        })?;
        record_blocked_failure(pool, lease, "replacement_mapping_changed").await?;
        return Ok(false);
    };
    if mapped.try_get::<String, _>("community_id").ok().as_deref()
        != Some(community_id.to_string().as_str())
        || mapped.try_get::<String, _>("relay_url").ok().as_deref() != Some(relay_url.as_str())
        || mapped
            .try_get::<Option<String>, _>("relay_pubkey")
            .ok()
            .flatten()
            .as_deref()
            != Some(relay_pubkey.as_str())
        || stored != digest(mapping_id, community_id, &relay_url, &relay_pubkey)
    {
        tx.rollback().await.map_err(|_| {
            BuzzAuthorityError::Transport("cannot release invalid replacement mapping".into())
        })?;
        record_blocked_failure(pool, lease, "replacement_mapping_changed").await?;
        return Ok(false);
    }
    let mut fences: Vec<_> = reads
        .iter()
        .map(|r| format!("{}:{}", community_id, r.observation.pubkey))
        .collect();
    fences.sort();
    fences.dedup();
    for fence in fences {
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
            .bind(fence)
            .execute(&mut *tx)
            .await
            .map_err(|_| {
                BuzzAuthorityError::Transport("cannot lock membership revision fence".into())
            })?;
    }
    for read in reads {
        accept_read(&mut tx, lease, read).await?;
    }
    let expected_revision = i64::try_from(target.membership_revision)
        .map_err(|_| BuzzAuthorityError::InvalidResponse("membership revision overflow".into()))?;
    let phase_attempt:Option<i32>=sqlx::query_scalar("SELECT max(attempt) FROM chat_identity_replacement_commands WHERE operation_id=$1 AND phase=$2")
        .bind(lease.operation_id).bind(phase).fetch_one(&mut *tx).await.map_err(|_|BuzzAuthorityError::Transport("cannot read command attempts".into()))?;
    let attempt = phase_attempt.unwrap_or(0).checked_add(1).ok_or_else(|| {
        BuzzAuthorityError::InvalidResponse("replacement attempt counter overflow".into())
    })?;
    let command_id = Uuid::new_v4();
    let outbox_event_id = Uuid::new_v4();
    let event_type = if phase == "revoke" {
        "io.elembra.chat.buzz.admission.revoked.v1"
    } else {
        "io.elembra.chat.buzz.admission.requested.v1"
    };
    let event=IntegrationEvent::builder().source("elembra://io.elembra.chat").r#type(event_type)
        .tenant_id(TenantId(lease.tenant_id.0)).workspace_id(WorkspaceId(lease.workspace_id.0))
        .actor(ActorRef::Principal(PrincipalId(lease.principal_id.0)))
        .data(serde_json::json!({"operation":if phase=="revoke"{"revoke"}else{"admit"},
            "admission_id":op.try_get::<Uuid,_>("replacement_binding_id").map_err(|_|BuzzAuthorityError::InvalidResponse("malformed replacement".into()))?,
            "community_id":community_id.to_string(),"relay_url":relay_url,"buzz_pubkey":target.pubkey,
            "command_version":1,"command_id":command_id,"expected_revision":expected_revision}))
        .build().map_err(|_|BuzzAuthorityError::InvalidResponse("cannot create replacement event".into()))?;
    let mut event = event;
    event.id = outbox_event_id;
    sqlx::query("INSERT INTO chat_identity_replacement_commands(command_id,operation_id,phase,attempt,operation_generation,community_id,pubkey,kind,expected_revision,actor_pubkey,outbox_event_id)
        VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)")
        .bind(command_id).bind(lease.operation_id).bind(phase).bind(attempt).bind(lease.generation).bind(community_id).bind(&target.pubkey)
        .bind(operation.buzz_kind() as i32).bind(expected_revision).bind(bridge_pubkey).bind(outbox_event_id)
        .execute(&mut *tx).await.map_err(|_|BuzzAuthorityError::Transport("cannot persist replacement command".into()))?;
    outbox.insert_in_tx(&mut tx, &event).await.map_err(|_| {
        BuzzAuthorityError::Transport("cannot persist replacement outbox event".into())
    })?;
    let next = if phase == "admit" {
        "pending_replacement_admission"
    } else {
        "pending_revocation"
    };
    let changed=sqlx::query("UPDATE chat_identity_replacements SET state=$3,generation=generation+1,lease_token=NULL,lease_expires_at=NULL,
        updated_at=clock_timestamp(),failure_code=NULL WHERE tenant_id=$1 AND operation_id=$2 AND state=$4 AND generation=$5
        AND lease_token=$6 AND lease_expires_at>clock_timestamp()")
        .bind(lease.tenant_id.0).bind(lease.operation_id).bind(next).bind(state.as_str()).bind(lease.generation).bind(lease.token)
        .execute(&mut *tx).await.map_err(|_|BuzzAuthorityError::Transport("cannot advance replacement".into()))?;
    if changed.rows_affected() != 1 {
        return Ok(false);
    }
    sqlx::query("INSERT INTO chat_identity_replacement_audit(audit_id,tenant_id,operation_id,actor_id,from_state,to_state)
        VALUES(gen_random_uuid(),$1,$2,$3,$4,$5)")
        .bind(lease.tenant_id.0).bind(lease.operation_id).bind(lease.principal_id.0).bind(state).bind(next)
        .execute(&mut *tx).await.map_err(|_|BuzzAuthorityError::Transport("cannot audit replacement".into()))?;
    tx.commit()
        .await
        .map_err(|_| BuzzAuthorityError::Transport("cannot commit replacement command".into()))?;
    Ok(true)
}

pub async fn process_next_replacement(
    pool: &PgPool,
    store: &ChatIdentityStore,
    outbox: &OutboxStore,
    client: &BuzzGatewayClient,
    bridge_pubkey: &str,
) -> Result<bool, BuzzAuthorityError> {
    let Some(lease) = store
        .claim_replacement()
        .await
        .map_err(|_| BuzzAuthorityError::Transport("cannot lease replacement".into()))?
    else {
        return Ok(false);
    };
    let result = match lease.state.as_str() {
        "pending_revocation" => {
            let old = read_membership(pool, client, &lease, &lease.old_pubkey, None).await?;
            queue_command(pool, outbox, &lease, bridge_pubkey, "revoke", &[&old]).await
        }
        "pending_revocation_verification" => {
            let old =
                read_membership(pool, client, &lease, &lease.old_pubkey, Some("revoked")).await?;
            let candidate = read_membership(
                pool,
                client,
                &lease,
                &lease.replacement_pubkey,
                Some("absent"),
            )
            .await?;
            queue_command(
                pool,
                outbox,
                &lease,
                bridge_pubkey,
                "admit",
                &[&old, &candidate],
            )
            .await
        }
        "pending_membership_verification" => {
            let old =
                read_membership(pool, client, &lease, &lease.old_pubkey, Some("revoked")).await?;
            let candidate =
                read_membership(pool, client, &lease, &lease.replacement_pubkey, None).await?;
            if old.observation.membership != BuzzMembership::Revoked {
                return Ok(false);
            }
            match candidate.observation.membership {
                BuzzMembership::Absent => {
                    queue_command(
                        pool,
                        outbox,
                        &lease,
                        bridge_pubkey,
                        "admit",
                        &[&old, &candidate],
                    )
                    .await
                }
                BuzzMembership::Revoked => {
                    record_blocked_failure(pool, &lease, "replacement_revoked").await?;
                    Ok(false)
                }
                BuzzMembership::Active => complete(pool, &lease, &old, &candidate).await,
            }
        }
        _ => Ok(false),
    };
    if let Err(error) = &result {
        if matches!(error, BuzzAuthorityError::Transport(_)) {
            record_transient_failure(pool, &lease).await?;
        } else {
            let code = match error {
                BuzzAuthorityError::Unauthorized => "replacement_upstream_unauthorized",
                BuzzAuthorityError::InvalidResponse(_) => "replacement_evidence_rejected",
                BuzzAuthorityError::Config(_) => "replacement_configuration_invalid",
                BuzzAuthorityError::Transport(_) => unreachable!(),
            };
            record_blocked_failure(pool, &lease, code).await?;
        }
    }
    result
}

pub fn spawn_replacement_worker(
    pool: PgPool,
    store: ChatIdentityStore,
    outbox: std::sync::Arc<OutboxStore>,
    gateway: std::sync::Arc<BuzzGatewayClient>,
    bridge_pubkey: String,
    mut shutdown: tokio::sync::broadcast::Receiver<()>,
) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
        loop {
            tokio::select! {
                _ = shutdown.recv() => break,
                _ = interval.tick() => {
                    if let Err(error) = process_next_replacement(
                        &pool, &store, &outbox, &gateway, &bridge_pubkey,
                    ).await {
                        tracing::warn!(error = %error, "Chat identity replacement attempt failed");
                    }
                }
            }
        }
    });
}

async fn record_blocked_failure(
    pool: &PgPool,
    lease: &ReplacementLease,
    code: &str,
) -> Result<(), BuzzAuthorityError> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|_| BuzzAuthorityError::Transport("cannot begin failure record".into()))?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL READ COMMITTED")
        .execute(&mut *tx)
        .await
        .map_err(|_| BuzzAuthorityError::Transport("cannot set failure isolation".into()))?;
    if !lock_principal(&mut tx, lease).await? {
        return Ok(());
    }
    let row = sqlx::query("UPDATE chat_identity_replacements SET failure_code=$4,blocked_at=clock_timestamp(),retry_at=clock_timestamp(),lease_token=NULL,lease_expires_at=NULL,updated_at=clock_timestamp()
        WHERE tenant_id=$1 AND operation_id=$2 AND lease_token=$3 AND generation=$5 AND lease_expires_at>clock_timestamp()
        RETURNING actor_id,state")
        .bind(lease.tenant_id.0)
        .bind(lease.operation_id)
        .bind(lease.token)
        .bind(code)
        .bind(lease.generation)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| BuzzAuthorityError::Transport("cannot record replacement failure".into()))?;
    if let Some(row) = row {
        let actor: Uuid = row
            .try_get("actor_id")
            .map_err(|_| BuzzAuthorityError::InvalidResponse("malformed failure audit".into()))?;
        let state: String = row
            .try_get("state")
            .map_err(|_| BuzzAuthorityError::InvalidResponse("malformed failure audit".into()))?;
        sqlx::query("INSERT INTO chat_identity_replacement_audit(audit_id,tenant_id,operation_id,actor_id,from_state,to_state,reason_code)
            VALUES(gen_random_uuid(),$1,$2,$3,$4,$4,$5)")
            .bind(lease.tenant_id.0).bind(lease.operation_id).bind(actor).bind(state).bind(code)
            .execute(&mut *tx).await.map_err(|_| BuzzAuthorityError::Transport("cannot audit replacement failure".into()))?;
    }
    tx.commit()
        .await
        .map_err(|_| BuzzAuthorityError::Transport("cannot commit replacement failure".into()))?;
    Ok(())
}

async fn record_transient_failure(
    pool: &PgPool,
    lease: &ReplacementLease,
) -> Result<(), BuzzAuthorityError> {
    let mut tx = pool.begin().await.map_err(|_| {
        BuzzAuthorityError::Transport("cannot begin replacement retry record".into())
    })?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL READ COMMITTED")
        .execute(&mut *tx)
        .await
        .map_err(|_| BuzzAuthorityError::Transport("cannot set retry isolation".into()))?;
    if !lock_principal(&mut tx, lease).await? {
        return Ok(());
    }
    let row = sqlx::query("UPDATE chat_identity_replacements SET
        transient_failures=transient_failures+1,
        blocked_at=CASE WHEN transient_failures+1 >= $5 THEN clock_timestamp() ELSE NULL END,
        retry_at=CASE WHEN transient_failures+1 >= $5 THEN clock_timestamp()
                      ELSE clock_timestamp()+LEAST(300,5*power(2,transient_failures)::integer)*interval '1 second' END,
        failure_code=CASE WHEN transient_failures+1 >= $5 THEN 'replacement_retries_exhausted' ELSE 'replacement_transport_retry' END,
        lease_token=NULL,lease_expires_at=NULL,updated_at=clock_timestamp()
        WHERE tenant_id=$1 AND operation_id=$2 AND lease_token=$3 AND generation=$4
          AND lease_expires_at>clock_timestamp()
        RETURNING actor_id,state,failure_code")
        .bind(lease.tenant_id.0)
        .bind(lease.operation_id)
        .bind(lease.token)
        .bind(lease.generation)
        .bind(MAX_TRANSIENT_FAILURES)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| BuzzAuthorityError::Transport("cannot record replacement retry".into()))?;
    if let Some(row) = row {
        sqlx::query("INSERT INTO chat_identity_replacement_audit(audit_id,tenant_id,operation_id,actor_id,from_state,to_state,reason_code)
            VALUES(gen_random_uuid(),$1,$2,$3,$4,$4,$5)")
            .bind(lease.tenant_id.0)
            .bind(lease.operation_id)
            .bind(row.try_get::<Uuid, _>("actor_id").map_err(|_| {
                BuzzAuthorityError::InvalidResponse("malformed retry audit".into())
            })?)
            .bind(row.try_get::<String, _>("state").map_err(|_| {
                BuzzAuthorityError::InvalidResponse("malformed retry audit".into())
            })?)
            .bind(row.try_get::<String, _>("failure_code").map_err(|_| {
                BuzzAuthorityError::InvalidResponse("malformed retry audit".into())
            })?)
            .execute(&mut *tx)
            .await
            .map_err(|_| BuzzAuthorityError::Transport("cannot audit replacement retry".into()))?;
    }
    tx.commit()
        .await
        .map_err(|_| BuzzAuthorityError::Transport("cannot commit replacement retry".into()))
}

async fn complete(
    pool: &PgPool,
    lease: &ReplacementLease,
    old: &MembershipRead,
    candidate: &MembershipRead,
) -> Result<bool, BuzzAuthorityError> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|_| BuzzAuthorityError::Transport("cannot begin replacement completion".into()))?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL READ COMMITTED")
        .execute(&mut *tx)
        .await
        .map_err(|_| BuzzAuthorityError::Transport("cannot set completion isolation".into()))?;
    if !lock_principal(&mut tx, lease).await? {
        return Ok(false);
    }
    if !chat_enabled(&mut tx, lease).await? {
        return Ok(false);
    }
    let op=sqlx::query("SELECT mapping_id,community_id,relay_url,relay_pubkey,mapping_fingerprint,replacement_binding_id,state,generation,lease_token,lease_expires_at
        FROM chat_identity_replacements WHERE tenant_id=$1 AND operation_id=$2 AND principal_id=$3 FOR UPDATE")
        .bind(lease.tenant_id.0).bind(lease.operation_id).bind(lease.principal_id.0).fetch_optional(&mut *tx).await
        .map_err(|_|BuzzAuthorityError::Transport("cannot lock completion operation".into()))?;
    let Some(op) = op else { return Ok(false) };
    let state: String = op
        .try_get("state")
        .map_err(|_| BuzzAuthorityError::InvalidResponse("malformed operation".into()))?;
    let generation: i64 = op
        .try_get("generation")
        .map_err(|_| BuzzAuthorityError::InvalidResponse("malformed operation".into()))?;
    let token: Option<Uuid> = op
        .try_get("lease_token")
        .map_err(|_| BuzzAuthorityError::InvalidResponse("malformed operation".into()))?;
    let expires: Option<chrono::DateTime<chrono::Utc>> = op
        .try_get("lease_expires_at")
        .map_err(|_| BuzzAuthorityError::InvalidResponse("malformed operation".into()))?;
    if state != "pending_membership_verification"
        || generation != lease.generation
        || token != Some(lease.token)
        || expires.is_none_or(|t| t <= chrono::Utc::now())
    {
        return Ok(false);
    }
    let community: Uuid = op
        .try_get("community_id")
        .map_err(|_| BuzzAuthorityError::InvalidResponse("malformed operation".into()))?;
    let mapping_id: Uuid = op
        .try_get("mapping_id")
        .map_err(|_| BuzzAuthorityError::InvalidResponse("malformed operation".into()))?;
    let relay_url: String = op
        .try_get("relay_url")
        .map_err(|_| BuzzAuthorityError::InvalidResponse("malformed operation".into()))?;
    let relay_pubkey: String = op
        .try_get("relay_pubkey")
        .map_err(|_| BuzzAuthorityError::InvalidResponse("malformed operation".into()))?;
    if old.observation.membership != BuzzMembership::Revoked
        || candidate.observation.membership != BuzzMembership::Active
        || old.observation.community_id != community.to_string()
        || candidate.observation.community_id != community.to_string()
        || old.observation.pubkey != lease.old_pubkey
        || candidate.observation.pubkey != lease.replacement_pubkey
    {
        tx.rollback().await.map_err(|_| {
            BuzzAuthorityError::Transport("cannot release invalid replacement evidence".into())
        })?;
        record_blocked_failure(pool, lease, "replacement_evidence_rejected").await?;
        return Ok(false);
    }
    let mapped=sqlx::query("SELECT community_id,relay_url,relay_pubkey FROM chat_workspace_communities WHERE tenant_id=$1 AND workspace_id=$2 AND mapping_id=$3 AND active FOR SHARE")
        .bind(lease.tenant_id.0).bind(lease.workspace_id.0).bind(mapping_id).fetch_optional(&mut *tx).await
        .map_err(|_|BuzzAuthorityError::Transport("cannot check mapping".into()))?;
    let Some(mapped) = mapped else {
        tx.rollback().await.map_err(|_| {
            BuzzAuthorityError::Transport("cannot release invalid replacement mapping".into())
        })?;
        record_blocked_failure(pool, lease, "replacement_mapping_changed").await?;
        return Ok(false);
    };
    if mapped.try_get::<String, _>("community_id").ok().as_deref()
        != Some(community.to_string().as_str())
        || mapped.try_get::<String, _>("relay_url").ok().as_deref() != Some(relay_url.as_str())
        || mapped
            .try_get::<Option<String>, _>("relay_pubkey")
            .ok()
            .flatten()
            .as_deref()
            != Some(relay_pubkey.as_str())
        || op
            .try_get::<Vec<u8>, _>("mapping_fingerprint")
            .ok()
            .as_deref()
            != Some(digest(mapping_id, community, &relay_url, &relay_pubkey).as_slice())
    {
        tx.rollback().await.map_err(|_| {
            BuzzAuthorityError::Transport("cannot release invalid replacement mapping".into())
        })?;
        record_blocked_failure(pool, lease, "replacement_mapping_changed").await?;
        return Ok(false);
    }
    let mut keys = [
        format!("{community}:{}", lease.old_pubkey),
        format!("{community}:{}", lease.replacement_pubkey),
    ];
    keys.sort();
    for key in keys {
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
            .bind(key)
            .execute(&mut *tx)
            .await
            .map_err(|_| BuzzAuthorityError::Transport("cannot lock membership history".into()))?;
    }
    accept_read(&mut tx, lease, old).await?;
    accept_read(&mut tx, lease, candidate).await?;
    let binding: Uuid = op
        .try_get("replacement_binding_id")
        .map_err(|_| BuzzAuthorityError::InvalidResponse("malformed replacement binding".into()))?;
    let activated=sqlx::query("UPDATE chat_identity_bindings SET status='active',verified_at=clock_timestamp()
        WHERE tenant_id=$1 AND principal_id=$2 AND binding_id=$3 AND buzz_pubkey=$4 AND status='pending' AND revoked_at IS NULL")
        .bind(lease.tenant_id.0).bind(lease.principal_id.0).bind(binding).bind(&lease.replacement_pubkey).execute(&mut *tx).await
        .map_err(|_|BuzzAuthorityError::Transport("cannot activate replacement".into()))?;
    if activated.rows_affected() != 1 {
        return Ok(false);
    }
    sqlx::query("INSERT INTO chat_buzz_admissions(admission_id,tenant_id,mapping_id,binding_id,buzz_pubkey,active) VALUES($1,$2,$3,$4,$5,true)")
        .bind(Uuid::new_v4()).bind(lease.tenant_id.0).bind(mapping_id).bind(binding).bind(&lease.replacement_pubkey).execute(&mut *tx).await
        .map_err(|_|BuzzAuthorityError::Transport("cannot activate local admission".into()))?;
    let done=sqlx::query("UPDATE chat_identity_replacements SET state='completed',generation=generation+1,completed_at=clock_timestamp(),updated_at=clock_timestamp(),lease_token=NULL,lease_expires_at=NULL,failure_code=NULL
        WHERE tenant_id=$1 AND operation_id=$2 AND state=$3 AND generation=$4 AND lease_token=$5 AND lease_expires_at>clock_timestamp()")
        .bind(lease.tenant_id.0).bind(lease.operation_id).bind(state).bind(lease.generation).bind(lease.token).execute(&mut *tx).await
        .map_err(|_|BuzzAuthorityError::Transport("cannot complete operation".into()))?;
    if done.rows_affected() != 1 {
        return Ok(false);
    }
    let actor: Uuid =
        sqlx::query_scalar("SELECT actor_id FROM chat_identity_replacements WHERE operation_id=$1")
            .bind(lease.operation_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|_| BuzzAuthorityError::Transport("cannot read audit actor".into()))?;
    sqlx::query("INSERT INTO chat_identity_replacement_audit(audit_id,tenant_id,operation_id,actor_id,from_state,to_state) VALUES(gen_random_uuid(),$1,$2,$3,'pending_membership_verification','completed')")
        .bind(lease.tenant_id.0).bind(lease.operation_id).bind(actor).execute(&mut *tx).await
        .map_err(|_|BuzzAuthorityError::Transport("cannot write completion audit".into()))?;
    tx.commit().await.map_err(|_| {
        BuzzAuthorityError::Transport("cannot commit replacement completion".into())
    })?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buzz_gateway::BuzzMembershipRequest;
    use nostr::{EventBuilder, JsonUtil, Keys, Kind};

    #[tokio::test]
    #[ignore = "requires migrated disposable PostgreSQL"]
    async fn signed_evidence_and_replacement_transition_commit_or_rollback_together() {
        assert_eq!(
            std::env::var("RUSTSHARE_TEST_DISPOSABLE_DB").as_deref(),
            Ok("1")
        );
        let url = crate::test_guard::disposable_database_url_from_env().unwrap();
        let pool = PgPool::connect(&url).await.unwrap();
        let tenant = Uuid::new_v4();
        let principal = Uuid::new_v4();
        let workspace = tenant;
        let mapping = Uuid::new_v4();
        let community = Uuid::new_v4();
        let old_binding = Uuid::new_v4();
        let new_binding = Uuid::new_v4();
        let operation = Uuid::new_v4();
        let challenge = Uuid::new_v4();
        let request_id = Uuid::new_v4();
        let old_key = Keys::generate().public_key().to_hex();
        let new_key = Keys::generate().public_key().to_hex();
        let relay = Keys::generate();
        let relay_pubkey = relay.public_key().to_hex();
        sqlx::query("INSERT INTO users(id,username,email,password_hash,tenant_id,display_name,storage_quota)
            VALUES($1,$2,$3,'test-hash',$4,'replacement test',1048576)")
            .bind(principal)
            .bind(format!("replacement-{principal}"))
            .bind(format!("replacement-{principal}@example.test"))
            .bind(tenant)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO chat_workspace_communities(mapping_id,tenant_id,workspace_id,community_id,relay_url,relay_pubkey,active)
            VALUES($1,$2,$3,$4,$5,$6,true)")
            .bind(mapping).bind(tenant).bind(workspace).bind(community.to_string()).bind("wss://relay.example.test").bind(&relay_pubkey).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO chat_identity_bindings(binding_id,tenant_id,principal_id,buzz_pubkey,status,rotation_of)
            VALUES($1,$2,$3,$4,'active',NULL),($5,$2,$3,$6,'pending',$1)")
            .bind(old_binding).bind(tenant).bind(principal).bind(&old_key).bind(new_binding).bind(&new_key).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO chat_binding_challenges(challenge_id,tenant_id,principal_id,buzz_pubkey,rotation_of,relay_url,nonce,expires_at)
            VALUES($1,$2,$3,$4,$5,$6,$7,clock_timestamp()+interval '5 minutes')")
            .bind(challenge).bind(tenant).bind(principal).bind(&new_key).bind(old_binding).bind("wss://relay.example.test").bind(Uuid::new_v4().to_string()).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO chat_identity_replacements(operation_id,tenant_id,principal_id,workspace_id,mapping_id,community_id,relay_url,relay_pubkey,mapping_fingerprint,old_binding_id,old_pubkey,replacement_binding_id,replacement_pubkey,challenge_id,proof_event_id,actor_id,state)
            VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$3,'pending_revocation')")
            .bind(operation).bind(tenant).bind(principal).bind(workspace).bind(mapping).bind(community)
            .bind("wss://relay.example.test").bind(&relay_pubkey).bind(digest(mapping,community,"wss://relay.example.test",&relay_pubkey))
            .bind(old_binding).bind(&old_key).bind(new_binding).bind(&new_key).bind(challenge).bind("ab".repeat(32)).execute(&pool).await.unwrap();

        let request = BuzzMembershipRequest {
            request_id,
            community_id: community,
            pubkey: &old_key,
        };
        let response = serde_json::json!({
            "request_id": request_id.to_string(), "community_id": community.to_string(),
            "pubkey": old_key.clone(), "membership": "revoked", "membership_revision": 1,
            "evaluated_at": chrono::Utc::now().timestamp()
        });
        let signed = EventBuilder::new(Kind::Custom(19_030), response.to_string())
            .sign_with_keys(&relay)
            .unwrap();
        let client =
            BuzzGatewayClient::new_for_test(Keys::generate(), reqwest::Client::builder()).unwrap();
        let observation = client
            .membership_from_19030(
                &serde_json::from_str(&signed.as_json()).unwrap(),
                &relay_pubkey,
                &request,
                0,
            )
            .unwrap();
        sqlx::query("INSERT INTO chat_buzz_membership_verifications(request_id,community_id,pubkey,relay_pubkey,minimum_revision,tenant_id,operation_id,expected_state)
            VALUES($1,$2,$3,$4,0,$5,$6,'revoked')")
            .bind(request_id).bind(community).bind(&old_key).bind(&relay_pubkey).bind(tenant).bind(operation).execute(&pool).await.unwrap();
        let read = MembershipRead {
            request_id,
            observation,
        };
        let lease = ReplacementLease {
            operation_id: operation,
            tenant_id: TenantId(tenant),
            principal_id: PrincipalId(principal),
            workspace_id: WorkspaceId(workspace),
            community_id: community,
            relay_url: "wss://relay.example.test".into(),
            relay_pubkey: relay_pubkey.clone(),
            old_pubkey: old_key,
            replacement_pubkey: new_key.clone(),
            state: "pending_revocation".into(),
            generation: 1,
            token: Uuid::new_v4(),
        };

        let mut rollback = pool.begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL READ COMMITTED")
            .execute(&mut *rollback)
            .await
            .unwrap();
        accept_read(&mut rollback, &lease, &read).await.unwrap();
        sqlx::query("UPDATE chat_identity_replacements SET state='pending_revocation_verification' WHERE operation_id=$1")
            .bind(operation).execute(&mut *rollback).await.unwrap();
        rollback.rollback().await.unwrap();
        let unchanged: (Option<chrono::DateTime<chrono::Utc>>, String) = sqlx::query_as(
            "SELECT v.accepted_at,r.state FROM chat_buzz_membership_verifications v JOIN chat_identity_replacements r ON r.operation_id=v.operation_id WHERE v.request_id=$1",
        ).bind(request_id).fetch_one(&pool).await.unwrap();
        assert!(unchanged.0.is_none());
        assert_eq!(unchanged.1, "pending_revocation");

        let mut commit = pool.begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL READ COMMITTED")
            .execute(&mut *commit)
            .await
            .unwrap();
        accept_read(&mut commit, &lease, &read).await.unwrap();
        sqlx::query("UPDATE chat_identity_replacements SET state='pending_revocation_verification' WHERE operation_id=$1")
            .bind(operation).execute(&mut *commit).await.unwrap();
        commit.commit().await.unwrap();
        let committed: (Option<chrono::DateTime<chrono::Utc>>, String) = sqlx::query_as(
            "SELECT v.accepted_at,r.state FROM chat_buzz_membership_verifications v JOIN chat_identity_replacements r ON r.operation_id=v.operation_id WHERE v.request_id=$1",
        ).bind(request_id).fetch_one(&pool).await.unwrap();
        assert!(committed.0.is_some());
        assert_eq!(committed.1, "pending_revocation_verification");
        let store = ChatIdentityStore::new(pool.clone());
        let (first, second) = tokio::join!(store.claim_replacement(), store.claim_replacement());
        let leases: Vec<_> = [first.unwrap(), second.unwrap()]
            .into_iter()
            .flatten()
            .collect();
        assert_eq!(
            leases.len(),
            1,
            "only one concurrent claimant owns the operation"
        );
        let claimed = &leases[0];
        sqlx::query("UPDATE chat_identity_replacements SET lease_expires_at=clock_timestamp()-interval '1 second'
            WHERE tenant_id=$1 AND operation_id=$2 AND lease_token=$3")
            .bind(tenant)
            .bind(operation)
            .bind(claimed.token)
            .execute(&pool)
            .await
            .unwrap();
        let reclaimed = store.claim_replacement().await.unwrap().unwrap();
        assert_eq!(reclaimed.operation_id, operation);
        assert!(reclaimed.generation > claimed.generation);
        let stale_command = Uuid::new_v4();
        let stale_event = Uuid::new_v4();
        sqlx::query("UPDATE chat_identity_replacements SET state='pending_replacement_admission',generation=4,transient_failures=0,lease_token=NULL,lease_expires_at=NULL WHERE operation_id=$1")
            .bind(operation).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO chat_identity_replacement_commands(command_id,operation_id,phase,attempt,operation_generation,community_id,pubkey,kind,expected_revision,actor_pubkey,outbox_event_id)
            VALUES($1,$2,'admit',1,3,$3,$4,9030,0,$5,$6)")
            .bind(stale_command).bind(operation).bind(community).bind(&new_key).bind(&relay_pubkey).bind(stale_event)
            .execute(&pool).await.unwrap();
        let mut ack_event = IntegrationEvent::builder()
            .source("elembra://io.elembra.chat")
            .r#type("io.elembra.chat.buzz.admission.requested.v1")
            .tenant_id(TenantId(tenant))
            .workspace_id(WorkspaceId(workspace))
            .build()
            .unwrap();
        ack_event.id = stale_event;
        crate::buzz_bridge::record_conditional_outcome(
            &pool,
            &ack_event,
            &crate::buzz_bridge::AdmissionData {
                operation: BuzzAdmissionOperation::Admit,
                admission_id: new_binding,
                community_id: community.to_string(),
                relay_url: "wss://relay.example.test".into(),
                buzz_pubkey: new_key.clone(),
                command_version: Some(1),
                command_id: Some(stale_command),
                expected_revision: Some(0),
            },
            Some(false),
        )
        .await
        .unwrap();
        let stale_state: (String, i16) = sqlx::query_as(
            "SELECT state,transient_failures FROM chat_identity_replacements WHERE operation_id=$1",
        )
        .bind(operation)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(stale_state, ("pending_revocation_verification".into(), 1));
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT outcome FROM chat_identity_replacement_commands WHERE command_id=$1"
            )
            .bind(stale_command)
            .fetch_one(&pool)
            .await
            .unwrap(),
            "stale"
        );
        sqlx::query("UPDATE chat_identity_replacements SET retry_at=clock_timestamp() WHERE operation_id=$1")
            .bind(operation).execute(&pool).await.unwrap();
        let mut retry_lease = store.claim_replacement().await.unwrap().unwrap();
        for failure in 2..=MAX_TRANSIENT_FAILURES {
            record_transient_failure(&pool, &retry_lease).await.unwrap();
            if failure < MAX_TRANSIENT_FAILURES {
                sqlx::query("UPDATE chat_identity_replacements SET retry_at=clock_timestamp() WHERE operation_id=$1")
                    .bind(operation).execute(&pool).await.unwrap();
                retry_lease = store.claim_replacement().await.unwrap().unwrap();
            }
        }
        let blocked: (i16, Option<chrono::DateTime<chrono::Utc>>, String) = sqlx::query_as(
            "SELECT transient_failures,blocked_at,failure_code FROM chat_identity_replacements WHERE operation_id=$1",
        )
        .bind(operation)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(blocked.0, MAX_TRANSIENT_FAILURES);
        assert!(blocked.1.is_some());
        assert_eq!(blocked.2, "replacement_retries_exhausted");
        assert!(store.claim_replacement().await.unwrap().is_none());
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM chat_identity_replacement_audit WHERE operation_id=$1 AND reason_code='replacement_retries_exhausted'")
                .bind(operation).fetch_one(&pool).await.unwrap(),
            1
        );
        sqlx::query("DELETE FROM chat_buzz_membership_verifications WHERE request_id=$1")
            .bind(request_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM chat_identity_replacement_audit WHERE operation_id=$1")
            .bind(operation)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM chat_identity_replacement_commands WHERE operation_id=$1")
            .bind(operation)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM chat_identity_replacements WHERE operation_id=$1")
            .bind(operation)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM chat_binding_challenges WHERE challenge_id=$1")
            .bind(challenge)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "DELETE FROM chat_identity_bindings WHERE tenant_id=$1 AND binding_id IN ($2,$3)",
        )
        .bind(tenant)
        .bind(old_binding)
        .bind(new_binding)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("DELETE FROM chat_workspace_communities WHERE mapping_id=$1")
            .bind(mapping)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM users WHERE id=$1 AND tenant_id=$2")
            .bind(principal)
            .bind(tenant)
            .execute(&pool)
            .await
            .unwrap();
    }
}
