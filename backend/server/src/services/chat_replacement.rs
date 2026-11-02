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
            "command_version":1,"command_id":command_id,"expected_revision":expected_revision,
            "staged":phase == "admit"}))
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
    let result = async {
        match lease.state.as_str() {
            "pending_revocation" => {
                let old = read_membership(pool, client, &lease, &lease.old_pubkey, None).await?;
                queue_command(pool, outbox, &lease, bridge_pubkey, "revoke", &[&old]).await
            }
            "pending_revocation_verification" => {
                let old = read_membership(pool, client, &lease, &lease.old_pubkey, Some("revoked"))
                    .await?;
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
                let old = read_membership(pool, client, &lease, &lease.old_pubkey, Some("revoked"))
                    .await?;
                let candidate =
                    read_membership(pool, client, &lease, &lease.replacement_pubkey, None).await?;
                if old.observation.membership != BuzzMembership::Revoked {
                    return Err(BuzzAuthorityError::InvalidResponse(
                        "old replacement key is not revoked".into(),
                    ));
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
                    BuzzMembership::Active => {
                        complete(pool, outbox, &lease, &old, &candidate).await
                    }
                }
            }
            _ => Ok(false),
        }
    }
    .await;
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
    outbox: &OutboxStore,
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
        || candidate.observation.send_enabled
        || candidate.observation.staged_operation_id.is_none()
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
    let staged_command = candidate
        .observation
        .staged_operation_id
        .ok_or_else(|| BuzzAuthorityError::InvalidResponse("candidate is not staged".into()))?;
    let add_expected_revision = candidate
        .observation
        .membership_revision
        .checked_sub(1)
        .ok_or_else(|| {
            BuzzAuthorityError::InvalidResponse("invalid staged membership revision".into())
        })?;
    let staged_command_matches: bool =
        sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM chat_identity_replacement_commands
         WHERE operation_id=$1 AND phase='admit' AND command_id=$2 AND community_id=$3
           AND pubkey=$4 AND expected_revision=$5)",
        )
        .bind(lease.operation_id)
        .bind(staged_command)
        .bind(community)
        .bind(&lease.replacement_pubkey)
        .bind(i64::try_from(add_expected_revision).map_err(|_| {
            BuzzAuthorityError::InvalidResponse("membership revision overflow".into())
        })?)
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| {
            BuzzAuthorityError::Transport("cannot validate staged admission command".into())
        })?;
    if !staged_command_matches {
        tx.rollback().await.map_err(|_| {
            BuzzAuthorityError::Transport("cannot reject unbound staged admission".into())
        })?;
        record_blocked_failure(pool, lease, "replacement_staged_evidence_rejected").await?;
        return Ok(false);
    }
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
    let admission_id = Uuid::new_v4();
    sqlx::query("INSERT INTO chat_buzz_admissions(admission_id,tenant_id,mapping_id,binding_id,buzz_pubkey,active) VALUES($1,$2,$3,$4,$5,true)")
        .bind(admission_id).bind(lease.tenant_id.0).bind(mapping_id).bind(binding).bind(&lease.replacement_pubkey).execute(&mut *tx).await
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
    let release = IntegrationEvent::builder()
        .source("elembra://io.elembra.chat")
        .r#type("io.elembra.chat.buzz.admission.released.v1")
        .tenant_id(TenantId(lease.tenant_id.0))
        .workspace_id(WorkspaceId(lease.workspace_id.0))
        .actor(ActorRef::Principal(PrincipalId(lease.principal_id.0)))
        .data(serde_json::json!({
            "operation":"admit",
            "admission_id":admission_id,
            "community_id":community.to_string(),
            "relay_url":relay_url,
            "relay_pubkey":relay_pubkey,
            "buzz_pubkey":lease.replacement_pubkey,
            "command_version":1,
            "command_id":Uuid::new_v4(),
            "expected_revision":candidate.observation.membership_revision,
            "release_of":staged_command
        }))
        .build()
        .map_err(|_| {
            BuzzAuthorityError::InvalidResponse("cannot create staged admission release".into())
        })?;
    outbox.insert_in_tx(&mut tx, &release).await.map_err(|_| {
        BuzzAuthorityError::Transport("cannot persist staged admission release".into())
    })?;
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
        let disabled_principal = Uuid::new_v4();
        let disabled_old_binding = Uuid::new_v4();
        let disabled_new_binding = Uuid::new_v4();
        let disabled_operation = Uuid::new_v4();
        let disabled_challenge = Uuid::new_v4();
        let request_id = Uuid::new_v4();
        let old_key = Keys::generate().public_key().to_hex();
        let new_key = Keys::generate().public_key().to_hex();
        let disabled_old_key = Keys::generate().public_key().to_hex();
        let disabled_new_key = Keys::generate().public_key().to_hex();
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
        sqlx::query("INSERT INTO chat_workspace_communities(mapping_id,tenant_id,workspace_id,community_id,relay_url,relay_pubkey,membership_protocol_version,active)
            VALUES($1,$2,$3,$4,$5,$6,'v1alpha2',true)")
            .bind(mapping).bind(tenant).bind(workspace).bind(community.to_string()).bind("wss://relay.example.test").bind(&relay_pubkey).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO chat_identity_bindings(binding_id,tenant_id,principal_id,buzz_pubkey,status,rotation_of,revoked_at)
            VALUES($1,$2,$3,$4,'revoked',NULL,clock_timestamp()),($5,$2,$3,$6,'pending',$1,NULL)")
            .bind(old_binding).bind(tenant).bind(principal).bind(&old_key).bind(new_binding).bind(&new_key).execute(&pool).await.unwrap();
        let candidate_keys = Keys::generate();
        let expiring = rustshare_resource_auth::BindingChallenge::issue(
            TenantId(tenant),
            PrincipalId(principal),
            candidate_keys.public_key().to_hex(),
            "wss://relay.example.test",
            Some(old_binding),
            chrono::Utc::now(),
            chrono::Duration::seconds(2),
        );
        let proof_event = EventBuilder::new(Kind::Authentication, "")
            .tags([
                nostr::Tag::parse(["relay", "wss://relay.example.test"]).unwrap(),
                nostr::Tag::parse(["challenge", expiring.nonce.as_str()]).unwrap(),
            ])
            .sign_with_keys(&candidate_keys)
            .unwrap();
        let proof = expiring
            .verify_proof(
                TenantId(tenant),
                PrincipalId(principal),
                &proof_event,
                chrono::Utc::now(),
            )
            .unwrap();
        let start_store = ChatIdentityStore::new(pool.clone());
        assert!(start_store
            .insert_challenge_for_mapping(&expiring, WorkspaceId(workspace))
            .await
            .unwrap());
        sqlx::query(
            "UPDATE chat_identity_bindings SET status='active',revoked_at=NULL WHERE binding_id=$1",
        )
        .bind(old_binding)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO application_enablements(tenant_id,workspace_id,application_id,enabled) VALUES($1,$1,'io.elembra.chat',true)")
            .bind(tenant).execute(&pool).await.unwrap();
        let valid_principal = Uuid::new_v4();
        let valid_old_binding = Uuid::new_v4();
        let valid_operation = Uuid::new_v4();
        let valid_keys = Keys::generate();
        let valid_old_key = Keys::generate().public_key().to_hex();
        sqlx::query("INSERT INTO users(id,username,email,password_hash,tenant_id,display_name,storage_quota)
            VALUES($1,$2,$3,'test-hash',$4,'valid replacement test',1048576)")
            .bind(valid_principal)
            .bind(format!("replacement-valid-{valid_principal}"))
            .bind(format!("replacement-valid-{valid_principal}@example.test"))
            .bind(tenant)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO chat_identity_bindings(binding_id,tenant_id,principal_id,buzz_pubkey,status)
            VALUES($1,$2,$3,$4,'active')")
            .bind(valid_old_binding)
            .bind(tenant)
            .bind(valid_principal)
            .bind(valid_old_key)
            .execute(&pool)
            .await
            .unwrap();
        let valid_challenge = rustshare_resource_auth::BindingChallenge::issue(
            TenantId(tenant),
            PrincipalId(valid_principal),
            valid_keys.public_key().to_hex(),
            "wss://relay.example.test",
            Some(valid_old_binding),
            chrono::Utc::now(),
            chrono::Duration::minutes(5),
        );
        let valid_event = EventBuilder::new(Kind::Authentication, "")
            .tags([
                nostr::Tag::parse(["relay", "wss://relay.example.test"]).unwrap(),
                nostr::Tag::parse(["challenge", valid_challenge.nonce.as_str()]).unwrap(),
            ])
            .sign_with_keys(&valid_keys)
            .unwrap();
        let valid_proof = valid_challenge
            .verify_proof(
                TenantId(tenant),
                PrincipalId(valid_principal),
                &valid_event,
                chrono::Utc::now(),
            )
            .unwrap();
        assert!(start_store
            .insert_challenge_for_mapping(&valid_challenge, WorkspaceId(workspace))
            .await
            .unwrap());
        let valid_start = rustshare_storage::ReplacementStart {
            operation_id: valid_operation,
            replacement_binding_id: valid_operation,
            old_binding_id: valid_old_binding,
            tenant_id: TenantId(tenant),
            principal_id: PrincipalId(valid_principal),
            workspace_id: WorkspaceId(workspace),
            actor_id: valid_principal,
        };
        assert!(start_store
            .begin_replacement(&valid_start, &valid_proof)
            .await
            .unwrap());
        assert!(start_store
            .begin_replacement(&valid_start, &valid_proof)
            .await
            .unwrap());
        assert_eq!(
            start_store
                .replacement_request_matches(
                    &valid_start,
                    valid_challenge.challenge_id,
                    valid_proof.event_id(),
                )
                .await
                .unwrap(),
            Some(true)
        );
        let mut conflict_start = valid_start.clone();
        conflict_start.workspace_id = WorkspaceId(Uuid::new_v4());
        assert_eq!(
            start_store
                .replacement_request_matches(
                    &conflict_start,
                    valid_challenge.challenge_id,
                    valid_proof.event_id(),
                )
                .await
                .unwrap(),
            Some(false)
        );
        let started: (bool, String, String, String, i64) = sqlx::query_as(
            "SELECT c.consumed_at IS NOT NULL,old.status,new.status,r.state,
                (SELECT count(*) FROM chat_identity_replacement_audit WHERE operation_id=r.operation_id)
             FROM chat_identity_replacements r
             JOIN chat_binding_challenges c ON c.challenge_id=r.challenge_id
             JOIN chat_identity_bindings old ON old.binding_id=r.old_binding_id
             JOIN chat_identity_bindings new ON new.binding_id=r.replacement_binding_id
             WHERE r.operation_id=$1",
        )
        .bind(valid_operation)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            started,
            (
                true,
                "revoked".into(),
                "pending".into(),
                "pending_revocation".into(),
                1
            )
        );
        let visible = start_store
            .replacement_status(TenantId(tenant), valid_principal, valid_operation)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(visible.state, "pending_revocation");
        assert!(start_store
            .replacement_status(TenantId(tenant), principal, valid_operation)
            .await
            .unwrap()
            .is_none());
        assert!(start_store
            .replacement_status(TenantId(Uuid::new_v4()), valid_principal, valid_operation)
            .await
            .unwrap()
            .is_none());
        assert_eq!(
            start_store
                .replacement_audit(TenantId(tenant), valid_principal, valid_operation)
                .await
                .unwrap()
                .len(),
            1
        );
        assert!(start_store
            .replacement_audit(TenantId(tenant), principal, valid_operation)
            .await
            .unwrap()
            .is_empty());
        sqlx::query("DELETE FROM chat_identity_replacement_audit WHERE operation_id=$1")
            .bind(valid_operation)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM chat_identity_replacements WHERE operation_id=$1")
            .bind(valid_operation)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM chat_binding_challenges WHERE challenge_id=$1")
            .bind(valid_challenge.challenge_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM chat_identity_bindings WHERE binding_id=ANY($1)")
            .bind(vec![valid_operation, valid_old_binding])
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM users WHERE id=$1 AND tenant_id=$2")
            .bind(valid_principal)
            .bind(tenant)
            .execute(&pool)
            .await
            .unwrap();
        let start = rustshare_storage::ReplacementStart {
            operation_id: Uuid::new_v4(),
            replacement_binding_id: Uuid::new_v4(),
            old_binding_id: old_binding,
            tenant_id: TenantId(tenant),
            principal_id: PrincipalId(principal),
            workspace_id: WorkspaceId(workspace),
            actor_id: principal,
        };
        let mut wrong_old = start.clone();
        wrong_old.old_binding_id = Uuid::new_v4();
        assert!(!start_store
            .begin_replacement(&wrong_old, &proof)
            .await
            .unwrap());
        sqlx::query("UPDATE chat_workspace_communities SET relay_pubkey=$2 WHERE mapping_id=$1")
            .bind(mapping)
            .bind("a".repeat(64))
            .execute(&pool)
            .await
            .unwrap();
        assert!(!start_store.begin_replacement(&start, &proof).await.unwrap());
        sqlx::query("UPDATE chat_workspace_communities SET relay_pubkey=$2 WHERE mapping_id=$1")
            .bind(mapping)
            .bind(&relay_pubkey)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE chat_workspace_communities SET membership_protocol_version='v1alpha1' WHERE mapping_id=$1")
            .bind(mapping).execute(&pool).await.unwrap();
        assert!(!start_store.begin_replacement(&start, &proof).await.unwrap());
        sqlx::query("UPDATE chat_workspace_communities SET membership_protocol_version='v1alpha2' WHERE mapping_id=$1")
            .bind(mapping).execute(&pool).await.unwrap();
        let untouched: (bool, String, i64) = sqlx::query_as(
            "SELECT c.consumed_at IS NULL,b.status,
                (SELECT count(*) FROM chat_identity_replacements WHERE operation_id=$2)
             FROM chat_binding_challenges c JOIN chat_identity_bindings b ON b.binding_id=$3
             WHERE c.challenge_id=$1",
        )
        .bind(expiring.challenge_id)
        .bind(start.operation_id)
        .bind(old_binding)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(untouched, (true, "active".into(), 0));
        let mut mapping_lock = pool.begin().await.unwrap();
        sqlx::query(
            "SELECT mapping_id FROM chat_workspace_communities WHERE mapping_id=$1 FOR UPDATE",
        )
        .bind(mapping)
        .execute(&mut *mapping_lock)
        .await
        .unwrap();
        let start_operation_id = start.operation_id;
        let starter =
            tokio::spawn(async move { start_store.begin_replacement(&start, &proof).await });
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let waiting: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE 'SELECT mapping_id,community_id%')")
                    .fetch_one(&pool).await.unwrap();
                if waiting { break; }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        }).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        mapping_lock.commit().await.unwrap();
        assert!(
            !starter.await.unwrap().unwrap(),
            "a challenge expiring during the mapping lock wait must not start replacement"
        );
        let rolled_back: (bool, String, i64, i64, i64) = sqlx::query_as(
            "SELECT c.consumed_at IS NULL,b.status,
                (SELECT count(*) FROM chat_identity_replacements WHERE operation_id=$2),
                (SELECT count(*) FROM chat_identity_bindings WHERE buzz_pubkey=c.buzz_pubkey),
                (SELECT count(*) FROM chat_identity_replacement_audit WHERE operation_id=$2)
             FROM chat_binding_challenges c JOIN chat_identity_bindings b ON b.binding_id=$3
             WHERE c.challenge_id=$1",
        )
        .bind(expiring.challenge_id)
        .bind(start_operation_id)
        .bind(old_binding)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(rolled_back, (true, "active".into(), 0, 0, 0));
        sqlx::query("DELETE FROM chat_binding_challenges WHERE challenge_id=$1")
            .bind(expiring.challenge_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM application_enablements WHERE tenant_id=$1")
            .bind(tenant)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE chat_identity_bindings SET status='revoked',revoked_at=clock_timestamp() WHERE binding_id=$1")
            .bind(old_binding).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO chat_binding_challenges(challenge_id,tenant_id,principal_id,buzz_pubkey,rotation_of,relay_url,nonce,expires_at)
            VALUES($1,$2,$3,$4,$5,$6,$7,clock_timestamp()+interval '5 minutes')")
            .bind(challenge).bind(tenant).bind(principal).bind(&new_key).bind(old_binding).bind("wss://relay.example.test").bind(Uuid::new_v4().to_string()).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO chat_identity_replacements(operation_id,tenant_id,principal_id,workspace_id,mapping_id,community_id,relay_url,relay_pubkey,mapping_fingerprint,old_binding_id,old_pubkey,replacement_binding_id,replacement_pubkey,challenge_id,proof_event_id,actor_id,state)
            VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$3,'pending_revocation')")
            .bind(operation).bind(tenant).bind(principal).bind(workspace).bind(mapping).bind(community)
            .bind("wss://relay.example.test").bind(&relay_pubkey).bind(digest(mapping,community,"wss://relay.example.test",&relay_pubkey))
            .bind(old_binding).bind(&old_key).bind(new_binding).bind(&new_key).bind(challenge).bind("ab".repeat(32)).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO users(id,username,email,password_hash,tenant_id,display_name,storage_quota,disabled_at)
            VALUES($1,$2,$3,'test-hash',$4,'disabled replacement test',1048576,clock_timestamp())")
            .bind(disabled_principal)
            .bind(format!("replacement-disabled-{disabled_principal}"))
            .bind(format!("replacement-disabled-{disabled_principal}@example.test"))
            .bind(tenant)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO chat_identity_bindings(binding_id,tenant_id,principal_id,buzz_pubkey,status,rotation_of,revoked_at)
            VALUES($1,$2,$3,$4,'revoked',NULL,clock_timestamp()),($5,$2,$3,$6,'pending',$1,NULL)")
            .bind(disabled_old_binding).bind(tenant).bind(disabled_principal).bind(&disabled_old_key)
            .bind(disabled_new_binding).bind(&disabled_new_key).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO chat_binding_challenges(challenge_id,tenant_id,principal_id,buzz_pubkey,rotation_of,relay_url,nonce,expires_at)
            VALUES($1,$2,$3,$4,$5,$6,$7,clock_timestamp()+interval '5 minutes')")
            .bind(disabled_challenge).bind(tenant).bind(disabled_principal).bind(&disabled_new_key)
            .bind(disabled_old_binding).bind("wss://relay.example.test").bind(Uuid::new_v4().to_string())
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO chat_identity_replacements(operation_id,tenant_id,principal_id,workspace_id,mapping_id,community_id,relay_url,relay_pubkey,mapping_fingerprint,old_binding_id,old_pubkey,replacement_binding_id,replacement_pubkey,challenge_id,proof_event_id,actor_id,state,created_at,retry_at)
            VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$3,'pending_revocation',clock_timestamp()-interval '1 hour',clock_timestamp()-interval '1 hour')")
            .bind(disabled_operation).bind(tenant).bind(disabled_principal).bind(workspace).bind(mapping).bind(community)
            .bind("wss://relay.example.test").bind(&relay_pubkey).bind(digest(mapping,community,"wss://relay.example.test",&relay_pubkey))
            .bind(disabled_old_binding).bind(&disabled_old_key).bind(disabled_new_binding).bind(&disabled_new_key)
            .bind(disabled_challenge).bind("cd".repeat(32)).execute(&pool).await.unwrap();

        let request = BuzzMembershipRequest {
            request_id,
            community_id: community,
            pubkey: &old_key,
        };
        let response = serde_json::json!({
            "request_id": request_id.to_string(), "community_id": community.to_string(),
            "pubkey": old_key.clone(), "membership": "revoked", "membership_revision": 1,
            "send_enabled": false, "staged_operation_id": null,
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
        let mut command_tx = pool.begin().await.unwrap();
        sqlx::query("SELECT id FROM users WHERE id=$1 FOR NO KEY UPDATE")
            .bind(principal)
            .execute(&mut *command_tx)
            .await
            .unwrap();
        let waiting_store = store.clone();
        let waiter = tokio::spawn(async move { waiting_store.claim_replacement().await });
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let waiting: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE 'SELECT id FROM users%')")
                    .fetch_one(&pool).await.unwrap();
                if waiting { break; }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        }).await.unwrap();
        let racing_command = Uuid::new_v4();
        sqlx::query("INSERT INTO chat_identity_replacement_commands(command_id,operation_id,phase,attempt,operation_generation,community_id,pubkey,kind,expected_revision,actor_pubkey,outbox_event_id) VALUES($1,$2,'admit',1,1,$3,$4,9030,0,$5,$6)")
            .bind(racing_command).bind(operation).bind(community).bind(&new_key).bind(&relay_pubkey).bind(Uuid::new_v4())
            .execute(&mut *command_tx).await.unwrap();
        command_tx.commit().await.unwrap();
        assert!(
            waiter.await.unwrap().unwrap().is_none(),
            "a command queued during the Principal lock wait must fence the claim"
        );
        sqlx::query("DELETE FROM chat_identity_replacement_commands WHERE command_id=$1")
            .bind(racing_command)
            .execute(&pool)
            .await
            .unwrap();
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
        assert_eq!(claimed.operation_id, operation);
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
        let admission_data = crate::buzz_bridge::AdmissionData {
            operation: BuzzAdmissionOperation::Admit,
            admission_id: new_binding,
            community_id: community.to_string(),
            relay_url: "wss://relay.example.test".into(),
            relay_pubkey: None,
            buzz_pubkey: new_key.clone(),
            command_version: Some(1),
            command_id: Some(stale_command),
            expected_revision: Some(0),
            staged: true,
            release_of: None,
        };
        assert!(crate::buzz_bridge::validate_conditional_command(
            &pool,
            &ack_event,
            &admission_data,
            &relay_pubkey,
        )
        .await
        .unwrap());
        crate::buzz_bridge::record_conditional_outcome(
            &pool,
            &ack_event,
            &admission_data,
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
        assert!(!store
            .recover_blocked_replacement(TenantId(Uuid::new_v4()), Uuid::new_v4(), operation)
            .await
            .unwrap());
        assert!(store
            .recover_blocked_replacement(TenantId(tenant), principal, operation)
            .await
            .unwrap());
        let recovered: (String, i16, Option<chrono::DateTime<chrono::Utc>>) = sqlx::query_as(
            "SELECT state,transient_failures,blocked_at FROM chat_identity_replacements WHERE operation_id=$1",
        )
        .bind(operation)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(recovered.0, "pending_revocation_verification");
        assert_eq!(recovered.1, 0);
        assert!(recovered.2.is_none());
        let binding_states: Vec<String> = sqlx::query_scalar(
            "SELECT status FROM chat_identity_bindings WHERE tenant_id=$1 AND binding_id IN ($2,$3) ORDER BY status",
        )
        .bind(tenant)
        .bind(old_binding)
        .bind(new_binding)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(binding_states, vec!["pending", "revoked"]);
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM chat_identity_replacement_audit WHERE operation_id=$1 AND actor_id=$2 AND reason_code='operator_recovery_fresh_verification'")
                .bind(operation).bind(principal).fetch_one(&pool).await.unwrap(),
            1
        );
        assert!(store.claim_replacement().await.unwrap().is_some());
        sqlx::query("UPDATE chat_identity_replacements SET state='pending_membership_verification',blocked_at=clock_timestamp(),failure_code='replacement_revoked',lease_token=NULL,lease_expires_at=NULL WHERE operation_id=$1")
            .bind(operation).execute(&pool).await.unwrap();
        assert!(!store
            .recover_blocked_replacement(TenantId(tenant), principal, operation)
            .await
            .unwrap());
        let rejected_command = Uuid::new_v4();
        let rejected_event = Uuid::new_v4();
        sqlx::query("UPDATE chat_identity_replacements SET state='pending_replacement_admission',blocked_at=clock_timestamp(),failure_code='buzz_command_rejected',lease_token=NULL,lease_expires_at=NULL WHERE operation_id=$1")
            .bind(operation).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO chat_identity_replacement_commands(command_id,operation_id,phase,attempt,operation_generation,community_id,pubkey,kind,expected_revision,actor_pubkey,outbox_event_id,outcome)
            VALUES($1,$2,'admit',2,7,$3,$4,9030,1,$5,$6,'rejected')")
            .bind(rejected_command).bind(operation).bind(community).bind(&new_key).bind(&relay_pubkey).bind(rejected_event)
            .execute(&pool).await.unwrap();
        assert!(store
            .recover_blocked_replacement(TenantId(tenant), principal, operation)
            .await
            .unwrap());
        let recovered_rejection: String = sqlx::query_scalar(
            "SELECT state FROM chat_identity_replacements WHERE operation_id=$1",
        )
        .bind(operation)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(recovered_rejection, "pending_revocation_verification");
        let unauthorized = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let app = axum::Router::new().route(
            "/api/v1/relay/membership/check",
            axum::routing::post({
                let unauthorized = unauthorized.clone();
                move || async move {
                    if unauthorized.load(std::sync::atomic::Ordering::SeqCst) {
                        axum::http::StatusCode::UNAUTHORIZED
                    } else {
                        axum::http::StatusCode::SERVICE_UNAVAILABLE
                    }
                }
            }),
        );
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let local_relay = format!("ws://{address}");
        sqlx::query("UPDATE chat_identity_replacements SET state='pending_revocation',retry_at=clock_timestamp(),blocked_at=NULL,transient_failures=0,failure_code=NULL,lease_token=NULL,lease_expires_at=NULL,relay_url=$2 WHERE operation_id=$1")
            .bind(operation).bind(&local_relay).execute(&pool).await.unwrap();
        let outbox = OutboxStore::new(
            pool.clone(),
            std::sync::Arc::new(
                rustshare_core::domain::ApplicationRegistry::first_party().unwrap(),
            ),
        );
        let client =
            BuzzGatewayClient::new_for_test(Keys::generate(), reqwest::Client::builder()).unwrap();
        assert!(matches!(
            process_next_replacement(&pool, &store, &outbox, &client, &relay_pubkey).await,
            Err(BuzzAuthorityError::Transport(_))
        ));
        let retry: (i16, Option<chrono::DateTime<chrono::Utc>>, String) = sqlx::query_as(
            "SELECT transient_failures,blocked_at,failure_code FROM chat_identity_replacements WHERE operation_id=$1",
        )
        .bind(operation)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(retry.0, 1);
        assert!(retry.1.is_none());
        assert_eq!(retry.2, "replacement_transport_retry");
        unauthorized.store(true, std::sync::atomic::Ordering::SeqCst);
        sqlx::query("UPDATE chat_identity_replacements SET retry_at=clock_timestamp() WHERE operation_id=$1")
            .bind(operation).execute(&pool).await.unwrap();
        assert!(matches!(
            process_next_replacement(&pool, &store, &outbox, &client, &relay_pubkey).await,
            Err(BuzzAuthorityError::Unauthorized)
        ));
        let rejected: (i16, Option<chrono::DateTime<chrono::Utc>>, String) = sqlx::query_as(
            "SELECT transient_failures,blocked_at,failure_code FROM chat_identity_replacements WHERE operation_id=$1",
        )
        .bind(operation)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(rejected.0, 1);
        assert!(rejected.1.is_some());
        assert_eq!(rejected.2, "replacement_upstream_unauthorized");
        server.abort();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let app = axum::Router::new().route(
            "/api/v1/relay/membership/check",
            axum::routing::post(move |axum::Json(mut body): axum::Json<serde_json::Value>| {
                let relay = relay.clone();
                async move {
                    body["membership"] = serde_json::json!("active");
                    body["membership_revision"] = serde_json::json!(100);
                    body["evaluated_at"] = serde_json::json!(chrono::Utc::now().timestamp());
                    let signed = EventBuilder::new(Kind::Custom(19_030), body.to_string())
                        .sign_with_keys(&relay)
                        .unwrap();
                    axum::Json(serde_json::to_value(signed).unwrap())
                }
            }),
        );
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        sqlx::query("UPDATE chat_identity_replacements SET state='pending_membership_verification',retry_at=clock_timestamp(),blocked_at=NULL,failure_code=NULL,lease_token=NULL,lease_expires_at=NULL,relay_url=$2 WHERE operation_id=$1")
            .bind(operation).bind(format!("ws://{address}")).execute(&pool).await.unwrap();
        assert!(matches!(
            process_next_replacement(&pool, &store, &outbox, &client, &relay_pubkey).await,
            Err(BuzzAuthorityError::InvalidResponse(_))
        ));
        let blocked: (bool, String) = sqlx::query_as("SELECT blocked_at IS NOT NULL,failure_code FROM chat_identity_replacements WHERE operation_id=$1")
            .bind(operation).fetch_one(&pool).await.unwrap();
        assert_eq!(blocked, (true, "replacement_evidence_rejected".into()));
        let bindings: Vec<String> = sqlx::query_scalar(
            "SELECT status FROM chat_identity_bindings WHERE binding_id=ANY($1) ORDER BY status",
        )
        .bind(vec![old_binding, new_binding])
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(bindings, vec!["pending", "revoked"]);
        assert!(store.claim_replacement().await.unwrap().is_none());
        server.abort();
        sqlx::query("DELETE FROM chat_buzz_membership_verifications WHERE operation_id=$1")
            .bind(operation)
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
        sqlx::query("DELETE FROM chat_identity_replacements WHERE operation_id=$1")
            .bind(disabled_operation)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM chat_binding_challenges WHERE challenge_id=$1")
            .bind(challenge)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM chat_binding_challenges WHERE challenge_id=$1")
            .bind(disabled_challenge)
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
        sqlx::query(
            "DELETE FROM chat_identity_bindings WHERE tenant_id=$1 AND binding_id IN ($2,$3)",
        )
        .bind(tenant)
        .bind(disabled_old_binding)
        .bind(disabled_new_binding)
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
        sqlx::query("DELETE FROM users WHERE id=$1 AND tenant_id=$2")
            .bind(disabled_principal)
            .bind(tenant)
            .execute(&pool)
            .await
            .unwrap();
    }
}
