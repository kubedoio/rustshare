//! Durable Elembra-owned state for the Chat identity/admission boundary.
//!
//! This store never contains a Buzz private key and never reads Buzz tables.

use anyhow::Result;
use chrono::{DateTime, Utc};
use rustshare_core::domain::{PrincipalId, TenantId, WorkspaceId};
use rustshare_resource_auth::{
    BindingChallenge, BindingStatus, ChatIdentityBinding, VerifiedBindingProof,
    WorkspaceCommunityMapping,
};
use sha2::{Digest as _, Sha256};
use sqlx::{PgPool, Row};
use uuid::Uuid;

/// A chat author resolved from Elembra-owned identity state.
///
/// Returned by [`ChatIdentityStore::resolve_authors_by_pubkeys`]. Revoked
/// bindings are intentionally kept so historical messages can still show the
/// display name that was current at the time of binding.
#[derive(Debug, Clone)]
pub struct ResolvedChatAuthor {
    pub buzz_pubkey: String,
    pub principal_id: PrincipalId,
    pub display_name: String,
    pub avatar_path: Option<String>,
}

/// Errors from [`ChatIdentityStore::mapping_by_community`].
#[derive(Debug, thiserror::Error)]
pub enum CommunityMappingError {
    /// Multiple active mappings for one `community_id` is a data-integrity
    /// violation; the caller must fail closed.
    #[error("ambiguous active mapping for community_id {community_id}: {row_count} tenants")]
    Ambiguous {
        community_id: String,
        row_count: usize,
    },
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

#[derive(Clone)]
pub struct ChatIdentityStore {
    pool: PgPool,
}

#[derive(Debug, Clone)]
pub struct ReplacementStart {
    pub operation_id: Uuid,
    pub replacement_binding_id: Uuid,
    pub old_binding_id: Uuid,
    pub tenant_id: TenantId,
    pub principal_id: PrincipalId,
    pub workspace_id: WorkspaceId,
    pub actor_id: Uuid,
}

#[derive(Debug, Clone)]
pub struct ReplacementStatus {
    pub operation_id: Uuid,
    pub state: String,
    pub generation: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub failure_code: Option<String>,
    pub blocked_at: Option<DateTime<Utc>>,
    pub transient_failures: i16,
}

#[derive(Debug, Clone)]
pub struct ReplacementAuditEntry {
    pub actor_id: Uuid,
    pub from_state: Option<String>,
    pub to_state: String,
    pub reason_code: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct ReplacementLease {
    pub operation_id: Uuid,
    pub tenant_id: TenantId,
    pub principal_id: PrincipalId,
    pub workspace_id: WorkspaceId,
    pub community_id: Uuid,
    pub relay_url: String,
    pub relay_pubkey: String,
    pub old_pubkey: String,
    pub replacement_pubkey: String,
    pub state: String,
    pub generation: i64,
    pub token: Uuid,
}

fn chat_binding_history_fence(pubkey: &str) -> String {
    format!("chat-binding:{pubkey}")
}

fn replacement_challenge_mapping_fingerprint(
    mapping_id: Uuid,
    community_id: &str,
    relay_url: &str,
    relay_pubkey: Option<&str>,
    protocol: &str,
) -> Vec<u8> {
    Sha256::digest(
        format!(
            "{mapping_id}\0{community_id}\0{relay_url}\0{}\0{protocol}",
            relay_pubkey.unwrap_or_default()
        )
        .as_bytes(),
    )
    .to_vec()
}

/// Idempotent provisioning insert outcome (ADR-0036).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProvisionMappingOutcome {
    /// This call created the mapping row.
    Inserted,
    /// A concurrent call created it; the winner's row is returned and MUST be
    /// verified against the discovered community before being accepted.
    AlreadyExists(WorkspaceCommunityMapping),
}

/// Provisioning insert failure — community-level conflicts fail closed.
#[derive(Debug, thiserror::Error)]
pub enum ProvisionMappingError {
    #[error("community is already mapped to another workspace")]
    CommunityInUse,
    /// Any other storage failure, flattened to a message so the error surface
    /// does not leak raw `sqlx::Error` variants into the service layer.
    #[error("mapping storage failure: {0}")]
    Other(String),
}

impl ChatIdentityStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// `Some(true)` is an exact retry; `Some(false)` is an operation ID collision.
    pub async fn replacement_request_matches(
        &self,
        input: &ReplacementStart,
        challenge_id: Uuid,
        proof_event_id: &str,
    ) -> Result<Option<bool>> {
        let row = sqlx::query(
            "SELECT workspace_id,old_binding_id,challenge_id,proof_event_id
             FROM chat_identity_replacements
             WHERE tenant_id=$1 AND principal_id=$2 AND operation_id=$3",
        )
        .bind(input.tenant_id.0)
        .bind(input.principal_id.0)
        .bind(input.operation_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|row| {
            Ok(
                row.try_get::<Uuid, _>("workspace_id")? == input.workspace_id.0
                    && row.try_get::<Uuid, _>("old_binding_id")? == input.old_binding_id
                    && row.try_get::<Uuid, _>("challenge_id")? == challenge_id
                    && row.try_get::<String, _>("proof_event_id")? == proof_event_id,
            )
        })
        .transpose()
    }

    /// Begin replacement after the caller verified the candidate's NIP-42 proof.
    pub async fn begin_replacement(
        &self,
        input: &ReplacementStart,
        proof: &VerifiedBindingProof,
    ) -> Result<bool> {
        if input.actor_id != input.principal_id.0
            || proof.tenant_id() != input.tenant_id
            || proof.principal_id() != input.principal_id
            || proof.challenge_id().is_nil()
            || proof.event_id().len() != 64
            || input.operation_id.is_nil()
            || input.replacement_binding_id.is_nil()
        {
            return Ok(false);
        }
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL READ COMMITTED")
            .execute(&mut *tx)
            .await?;
        let principal = sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM users WHERE id=$1 AND tenant_id=$2 AND disabled_at IS NULL FOR NO KEY UPDATE",
        )
        .bind(input.principal_id.0)
        .bind(input.tenant_id.0)
        .fetch_optional(&mut *tx)
        .await?;
        if principal.is_none() {
            return Ok(false);
        }
        let duplicate = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM chat_identity_replacements
             WHERE tenant_id=$1 AND operation_id=$2 AND principal_id=$3
               AND workspace_id=$4 AND actor_id=$5 AND challenge_id=$6
               AND old_binding_id=$7 AND replacement_binding_id=$8 AND proof_event_id=$9)",
        )
        .bind(input.tenant_id.0)
        .bind(input.operation_id)
        .bind(input.principal_id.0)
        .bind(input.workspace_id.0)
        .bind(input.actor_id)
        .bind(proof.challenge_id())
        .bind(input.old_binding_id)
        .bind(input.replacement_binding_id)
        .bind(proof.event_id())
        .fetch_one(&mut *tx)
        .await?;
        if duplicate {
            tx.commit().await?;
            return Ok(true);
        }
        let challenge = sqlx::query(
            "SELECT buzz_pubkey,rotation_of,relay_url,nonce,expires_at,consumed_at,mapping_fingerprint
             FROM chat_binding_challenges WHERE challenge_id=$1 AND tenant_id=$2
               AND principal_id=$3 FOR UPDATE",
        )
        .bind(proof.challenge_id())
        .bind(input.tenant_id.0)
        .bind(input.principal_id.0)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(challenge) = challenge else {
            return Ok(false);
        };
        let candidate: String = challenge.try_get("buzz_pubkey")?;
        let nonce: String = challenge.try_get("nonce")?;
        let expires_at: DateTime<Utc> = challenge.try_get("expires_at")?;
        let consumed_at: Option<DateTime<Utc>> = challenge.try_get("consumed_at")?;
        let challenge_mapping_fingerprint: Option<Vec<u8>> =
            challenge.try_get("mapping_fingerprint")?;
        let challenge_relay: String = challenge.try_get("relay_url")?;
        let Some(old_binding_id) = challenge.try_get::<Option<Uuid>, _>("rotation_of")? else {
            return Ok(false);
        };
        if old_binding_id != input.old_binding_id {
            return Ok(false);
        }
        let now = Utc::now();
        if candidate != proof.buzz_pubkey()
            || nonce != proof.nonce()
            || challenge_relay != proof.relay_url()
            || expires_at <= now
            || consumed_at.is_some()
            || (now - proof.verified_at()).num_seconds().abs() > 60
        {
            return Ok(false);
        }
        let mapping = sqlx::query(
            "SELECT mapping_id,community_id,relay_url,relay_pubkey,membership_protocol_version FROM chat_workspace_communities
             WHERE tenant_id=$1 AND workspace_id=$2 AND active FOR SHARE",
        )
        .bind(input.tenant_id.0)
        .bind(input.workspace_id.0)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(mapping) = mapping else {
            return Ok(false);
        };
        let mapping_id: Uuid = mapping.try_get("mapping_id")?;
        let community_text: String = mapping.try_get("community_id")?;
        let Ok(community_id) = Uuid::parse_str(&community_text) else {
            return Ok(false);
        };
        let relay_url: String = mapping.try_get("relay_url")?;
        let relay_pubkey: Option<String> = mapping.try_get("relay_pubkey")?;
        let protocol: String = mapping.try_get("membership_protocol_version")?;
        let Some(relay_pubkey) = relay_pubkey else {
            return Ok(false);
        };
        if relay_url != challenge_relay
            || protocol != "v1alpha2"
            || community_id.is_nil()
            || community_text != community_id.to_string()
            || relay_pubkey.len() != 64
            || !relay_pubkey
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Ok(false);
        }
        let expected_challenge_fingerprint = replacement_challenge_mapping_fingerprint(
            mapping_id,
            &community_text,
            &relay_url,
            Some(&relay_pubkey),
            &protocol,
        );
        if challenge_mapping_fingerprint.as_deref()
            != Some(expected_challenge_fingerprint.as_slice())
        {
            return Ok(false);
        }
        let mut fences = [
            chat_binding_history_fence(&candidate),
            format!("{community_id}:{candidate}"),
            format!("{community_id}:old:{old_binding_id}"),
        ];
        fences.sort();
        for fence in fences {
            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
                .bind(fence)
                .execute(&mut *tx)
                .await?;
        }
        let old = sqlx::query(
            "SELECT buzz_pubkey FROM chat_identity_bindings
             WHERE tenant_id=$1 AND principal_id=$2 AND binding_id=$3
               AND status='active' AND revoked_at IS NULL FOR UPDATE",
        )
        .bind(input.tenant_id.0)
        .bind(input.principal_id.0)
        .bind(old_binding_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(old) = old else { return Ok(false) };
        let old_pubkey: String = old.try_get("buzz_pubkey")?;
        if old_pubkey == candidate {
            return Ok(false);
        }
        let live_operation: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM chat_identity_replacements
             WHERE tenant_id=$1 AND principal_id=$2 AND state <> 'completed')",
        )
        .bind(input.tenant_id.0)
        .bind(input.principal_id.0)
        .fetch_one(&mut *tx)
        .await?;
        if live_operation {
            return Ok(false);
        }
        let key_seen = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM chat_identity_bindings WHERE buzz_pubkey=$1)",
        )
        .bind(&candidate)
        .fetch_one(&mut *tx)
        .await?;
        if key_seen {
            return Ok(false);
        }
        let chat_enabled: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM application_enablements ae
             LEFT JOIN application_user_preferences p ON p.user_id=$3 AND p.application_id='io.elembra.chat'
             WHERE ae.tenant_id=$1 AND ae.workspace_id=$2 AND ae.application_id='io.elembra.chat'
               AND ae.enabled AND COALESCE(p.enabled,true))",
        )
        .bind(input.tenant_id.0)
        .bind(input.workspace_id.0)
        .bind(input.principal_id.0)
        .fetch_one(&mut *tx)
        .await?;
        if !chat_enabled {
            return Ok(false);
        }
        let digest = sha2::Sha256::digest(
            format!("{mapping_id}\0{community_id}\0{relay_url}\0{relay_pubkey}").as_bytes(),
        );
        // Mapping and history locks may outlive the possession proof.
        let now = Utc::now();
        if expires_at <= now || (now - proof.verified_at()).num_seconds().abs() > 60 {
            return Ok(false);
        }
        let consumed = sqlx::query(
            "UPDATE chat_binding_challenges SET consumed_at=$2 WHERE challenge_id=$1 AND consumed_at IS NULL AND expires_at>clock_timestamp()",
        )
        .bind(proof.challenge_id())
        .bind(now)
        .execute(&mut *tx)
        .await?;
        if consumed.rows_affected() != 1 {
            return Ok(false);
        }
        sqlx::query("UPDATE chat_identity_bindings SET status='revoked',revoked_at=$3 WHERE tenant_id=$1 AND binding_id=$2 AND status='active' AND revoked_at IS NULL")
            .bind(input.tenant_id.0).bind(old_binding_id).bind(now).execute(&mut *tx).await?;
        sqlx::query("UPDATE chat_buzz_admissions SET active=false,revoked_at=$3 WHERE tenant_id=$1 AND binding_id=$2 AND active")
            .bind(input.tenant_id.0).bind(old_binding_id).bind(now).execute(&mut *tx).await?;
        let binding = sqlx::query("INSERT INTO chat_identity_bindings(binding_id,tenant_id,principal_id,buzz_pubkey,status,rotation_of,audit_metadata) VALUES($1,$2,$3,$4,'pending',$5,'{\"proof\":\"nip-42\",\"operation\":\"durable-replacement\"}') ON CONFLICT (binding_id) DO NOTHING")
            .bind(input.replacement_binding_id).bind(input.tenant_id.0).bind(input.principal_id.0).bind(&candidate).bind(old_binding_id).execute(&mut *tx).await?;
        if binding.rows_affected() != 1 {
            return Ok(false);
        }
        let operation = sqlx::query("INSERT INTO chat_identity_replacements(operation_id,tenant_id,principal_id,workspace_id,mapping_id,community_id,relay_url,relay_pubkey,mapping_fingerprint,old_binding_id,old_pubkey,replacement_binding_id,replacement_pubkey,challenge_id,proof_event_id,actor_id,state) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,'pending_revocation') ON CONFLICT (operation_id) DO NOTHING")
            .bind(input.operation_id).bind(input.tenant_id.0).bind(input.principal_id.0).bind(input.workspace_id.0)
            .bind(mapping_id).bind(community_id).bind(&relay_url).bind(&relay_pubkey).bind(digest.to_vec())
            .bind(old_binding_id).bind(old_pubkey).bind(input.replacement_binding_id).bind(&candidate)
            .bind(proof.challenge_id()).bind(proof.event_id()).bind(input.actor_id).execute(&mut *tx).await?;
        if operation.rows_affected() != 1 {
            return Ok(false);
        }
        sqlx::query("INSERT INTO chat_identity_replacement_audit(audit_id,tenant_id,operation_id,actor_id,to_state) VALUES(gen_random_uuid(),$1,$2,$3,'pending_revocation')")
            .bind(input.tenant_id.0).bind(input.operation_id).bind(input.actor_id).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(true)
    }

    pub async fn claim_replacement(&self) -> Result<Option<ReplacementLease>> {
        let candidate = sqlx::query(
            "SELECT operation_id,tenant_id,principal_id,workspace_id FROM chat_identity_replacements r
             WHERE state IN ('pending_revocation','pending_revocation_verification','pending_membership_verification')
               AND blocked_at IS NULL AND retry_at<=clock_timestamp() AND (lease_expires_at IS NULL OR lease_expires_at<=clock_timestamp())
               AND EXISTS (SELECT 1 FROM users u WHERE u.id=r.principal_id AND u.tenant_id=r.tenant_id AND u.disabled_at IS NULL)
               AND NOT EXISTS (SELECT 1 FROM chat_identity_replacement_commands c WHERE c.operation_id=r.operation_id
                 AND c.outcome IN ('pending','ambiguous') AND c.phase=CASE WHEN r.state='pending_revocation' THEN 'revoke' ELSE 'admit' END)
             ORDER BY retry_at,created_at LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await?;
        let Some(candidate) = candidate else {
            return Ok(None);
        };
        let operation_id: Uuid = candidate.try_get("operation_id")?;
        let tenant_id = TenantId(candidate.try_get("tenant_id")?);
        let principal_id = PrincipalId(candidate.try_get("principal_id")?);
        let workspace_id = WorkspaceId(candidate.try_get("workspace_id")?);
        let token = Uuid::new_v4();
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL READ COMMITTED")
            .execute(&mut *tx)
            .await?;
        if sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM users WHERE id=$1 AND tenant_id=$2 AND disabled_at IS NULL FOR NO KEY UPDATE",
        )
        .bind(principal_id.0)
        .bind(tenant_id.0)
        .fetch_optional(&mut *tx)
        .await?
        .is_none()
        {
            return Ok(None);
        }
        let row = sqlx::query(
            "UPDATE chat_identity_replacements SET lease_token=$3,lease_expires_at=clock_timestamp()+interval '90 seconds',
             generation=generation+1,updated_at=clock_timestamp()
             WHERE tenant_id=$1 AND operation_id=$2 AND blocked_at IS NULL AND retry_at<=clock_timestamp()
             AND state IN ('pending_revocation','pending_revocation_verification','pending_membership_verification')
             AND (lease_expires_at IS NULL OR lease_expires_at<=clock_timestamp())
             AND NOT EXISTS (SELECT 1 FROM chat_identity_replacement_commands c
                 WHERE c.operation_id=chat_identity_replacements.operation_id
                   AND c.outcome IN ('pending','ambiguous')
                   AND c.phase=CASE WHEN chat_identity_replacements.state='pending_revocation' THEN 'revoke' ELSE 'admit' END)
             RETURNING community_id,relay_url,relay_pubkey,old_pubkey,replacement_pubkey,state,generation",
        )
        .bind(tenant_id.0)
        .bind(operation_id)
        .bind(token)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(row) = row else { return Ok(None) };
        let community_id: Uuid = row.try_get("community_id")?;
        let relay_url: String = row.try_get("relay_url")?;
        let relay_pubkey: String = row.try_get("relay_pubkey")?;
        let old_pubkey: String = row.try_get("old_pubkey")?;
        let replacement_pubkey: String = row.try_get("replacement_pubkey")?;
        let state: String = row.try_get("state")?;
        let generation: i64 = row.try_get("generation")?;
        tx.commit().await?;
        Ok(Some(ReplacementLease {
            operation_id,
            tenant_id,
            principal_id,
            workspace_id,
            community_id,
            relay_url,
            relay_pubkey,
            old_pubkey,
            replacement_pubkey,
            state,
            generation,
            token,
        }))
    }

    pub async fn replacement_status(
        &self,
        tenant_id: TenantId,
        principal_id: Uuid,
        operation_id: Uuid,
    ) -> Result<Option<ReplacementStatus>> {
        let row = sqlx::query("SELECT operation_id,state,generation,created_at,updated_at,completed_at,failure_code,blocked_at,transient_failures
            FROM chat_identity_replacements WHERE tenant_id=$1 AND principal_id=$2 AND operation_id=$3")
            .bind(tenant_id.0).bind(principal_id).bind(operation_id).fetch_optional(&self.pool).await?;
        row.map(|row| {
            Ok(ReplacementStatus {
                operation_id: row.try_get("operation_id")?,
                state: row.try_get("state")?,
                generation: row.try_get("generation")?,
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
                completed_at: row.try_get("completed_at")?,
                failure_code: row.try_get("failure_code")?,
                blocked_at: row.try_get("blocked_at")?,
                transient_failures: row.try_get("transient_failures")?,
            })
        })
        .transpose()
    }

    /// Requeue only recoverable blocked operations; the worker must gather fresh evidence.
    pub async fn recover_blocked_replacement(
        &self,
        tenant_id: TenantId,
        actor_id: Uuid,
        operation_id: Uuid,
    ) -> Result<bool> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL READ COMMITTED")
            .execute(&mut *tx)
            .await?;
        let actor = sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM users WHERE id=$1 AND tenant_id=$2 AND disabled_at IS NULL FOR NO KEY UPDATE",
        )
        .bind(actor_id)
        .bind(tenant_id.0)
        .fetch_optional(&mut *tx)
        .await?;
        if actor.is_none() {
            return Ok(false);
        }
        let operation = sqlx::query(
            "SELECT state,failure_code FROM chat_identity_replacements
             WHERE tenant_id=$1 AND operation_id=$2 AND blocked_at IS NOT NULL FOR UPDATE",
        )
        .bind(tenant_id.0)
        .bind(operation_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(operation) = operation else {
            return Ok(false);
        };
        let state: String = operation.try_get("state")?;
        let failure: String = operation.try_get("failure_code")?;
        let retry_exhausted = failure == "replacement_retries_exhausted"
            && matches!(
                state.as_str(),
                "pending_revocation"
                    | "pending_revocation_verification"
                    | "pending_membership_verification"
            );
        let rejected_admission = failure == "buzz_command_rejected"
            && state == "pending_replacement_admission"
            && sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM chat_identity_replacement_commands
                 WHERE operation_id=$1 AND phase='admit' AND outcome='rejected')",
            )
            .bind(operation_id)
            .fetch_one(&mut *tx)
            .await?;
        if !retry_exhausted && !rejected_admission {
            return Ok(false);
        }
        let next_state = if rejected_admission {
            "pending_revocation_verification"
        } else {
            state.as_str()
        };
        let changed = sqlx::query(
            "UPDATE chat_identity_replacements SET state=$3,generation=generation+1,
             transient_failures=0,blocked_at=NULL,failure_code=NULL,retry_at=clock_timestamp(),
             lease_token=NULL,lease_expires_at=NULL,updated_at=clock_timestamp()
             WHERE tenant_id=$1 AND operation_id=$2 AND blocked_at IS NOT NULL",
        )
        .bind(tenant_id.0)
        .bind(operation_id)
        .bind(next_state)
        .execute(&mut *tx)
        .await?;
        if changed.rows_affected() != 1 {
            return Ok(false);
        }
        sqlx::query(
            "INSERT INTO chat_identity_replacement_audit
             (audit_id,tenant_id,operation_id,actor_id,from_state,to_state,reason_code)
             VALUES(gen_random_uuid(),$1,$2,$3,$4,$5,'operator_recovery_fresh_verification')",
        )
        .bind(tenant_id.0)
        .bind(operation_id)
        .bind(actor_id)
        .bind(&state)
        .bind(next_state)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(true)
    }

    pub async fn replacement_audit(
        &self,
        tenant_id: TenantId,
        principal_id: Uuid,
        operation_id: Uuid,
    ) -> Result<Vec<ReplacementAuditEntry>> {
        let rows = sqlx::query("SELECT a.actor_id,a.from_state,a.to_state,a.reason_code,a.created_at
            FROM chat_identity_replacement_audit a JOIN chat_identity_replacements r USING(tenant_id,operation_id)
            WHERE a.tenant_id=$1 AND r.principal_id=$2 AND a.operation_id=$3 ORDER BY a.created_at,a.audit_id")
            .bind(tenant_id.0).bind(principal_id).bind(operation_id).fetch_all(&self.pool).await?;
        rows.into_iter()
            .map(|row| {
                Ok(ReplacementAuditEntry {
                    actor_id: row.try_get("actor_id")?,
                    from_state: row.try_get("from_state")?,
                    to_state: row.try_get("to_state")?,
                    reason_code: row.try_get("reason_code")?,
                    created_at: row.try_get("created_at")?,
                })
            })
            .collect()
    }

    pub async fn mapping(
        &self,
        tenant_id: TenantId,
        workspace_id: WorkspaceId,
    ) -> Result<Option<WorkspaceCommunityMapping>> {
        let row = sqlx::query(
            "SELECT tenant_id, workspace_id, community_id, relay_url, relay_pubkey, membership_protocol_version, active
             FROM chat_workspace_communities
             WHERE tenant_id = $1 AND workspace_id = $2",
        )
        .bind(tenant_id.0)
        .bind(workspace_id.0)
        .fetch_optional(&self.pool)
        .await?;

        row.map(|row| {
            Ok(WorkspaceCommunityMapping {
                tenant_id: TenantId(row.try_get("tenant_id")?),
                workspace_id: WorkspaceId(row.try_get("workspace_id")?),
                community_id: row.try_get("community_id")?,
                relay_url: row.try_get("relay_url")?,
                relay_pubkey: row.try_get("relay_pubkey")?,
                membership_protocol_version: row.try_get("membership_protocol_version")?,
                active: row.try_get("active")?,
            })
        })
        .transpose()
    }

    /// The workspace↔community mapping for `community_id` regardless of tenant,
    /// if active. At most one ACTIVE mapping per `community_id` is guaranteed
    /// globally by the partial unique index
    /// `chat_workspace_communities_active_community`; a deactivated mapping
    /// frees the community for another tenant.
    ///
    /// Multiple active mappings for one `community_id` is a data-integrity
    /// violation; the caller must fail closed.
    pub async fn mapping_by_community(
        &self,
        community_id: &str,
    ) -> Result<Option<WorkspaceCommunityMapping>, CommunityMappingError> {
        let rows = sqlx::query(
            "SELECT tenant_id, workspace_id, community_id, relay_url, relay_pubkey, membership_protocol_version, active
             FROM chat_workspace_communities
             WHERE community_id = $1 AND active",
        )
        .bind(community_id)
        .fetch_all(&self.pool)
        .await?;
        if rows.len() > 1 {
            return Err(CommunityMappingError::Ambiguous {
                community_id: community_id.to_string(),
                row_count: rows.len(),
            });
        }
        let Some(row) = rows.into_iter().next() else {
            return Ok(None);
        };

        Ok(Some(WorkspaceCommunityMapping {
            tenant_id: TenantId(row.try_get("tenant_id")?),
            workspace_id: WorkspaceId(row.try_get("workspace_id")?),
            community_id: row.try_get("community_id")?,
            relay_url: row.try_get("relay_url")?,
            relay_pubkey: row.try_get("relay_pubkey")?,
            membership_protocol_version: row.try_get("membership_protocol_version")?,
            active: row.try_get("active")?,
        }))
    }

    /// Current projection policy for the chat Application in this tenant/workspace,
    /// read from `application_enablements.configuration` (JSONB). Absent config
    /// ⇒ defaults (both flags false).
    ///
    /// Uses `rustshare_memory::policy::ProjectionPolicy::from_config`, which
    /// fails closed: absent or non-boolean flag values mean `false`.
    pub async fn projection_policy(
        &self,
        tenant_id: TenantId,
        workspace_id: WorkspaceId,
    ) -> Result<rustshare_memory::policy::ProjectionPolicy> {
        let configuration: Option<serde_json::Value> = sqlx::query_scalar(
            "SELECT configuration FROM application_enablements
             WHERE tenant_id = $1 AND workspace_id = $2
               AND application_id = 'io.elembra.chat' AND enabled",
        )
        .bind(tenant_id.0)
        .bind(workspace_id.0)
        .fetch_optional(&self.pool)
        .await?;
        Ok(rustshare_memory::policy::ProjectionPolicy::from_config(
            configuration.as_ref().unwrap_or(&serde_json::Value::Null),
        ))
    }

    pub async fn chat_access(
        &self,
        tenant_id: TenantId,
        workspace_id: WorkspaceId,
        principal_id: PrincipalId,
    ) -> Result<bool> {
        Ok(sqlx::query_scalar(
            "SELECT EXISTS(
                SELECT 1
                FROM application_enablements ae
                LEFT JOIN application_user_preferences aup
                  ON aup.user_id = $3 AND aup.application_id = 'io.elembra.chat'
                WHERE ae.tenant_id = $1 AND ae.workspace_id = $2
                  AND ae.application_id = 'io.elembra.chat'
                  AND ae.enabled AND COALESCE(aup.enabled, true)
            )",
        )
        .bind(tenant_id.0)
        .bind(workspace_id.0)
        .bind(principal_id.0)
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn insert_mapping(&self, mapping: &WorkspaceCommunityMapping) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        if mapping.membership_protocol_version == "v1alpha2" {
            let previous_mapping = sqlx::query_scalar::<_, Uuid>(
                "SELECT mapping_id FROM chat_workspace_communities
                 WHERE community_id=$1 FOR UPDATE",
            )
            .bind(&mapping.community_id)
            .fetch_optional(&mut *tx)
            .await?;
            if previous_mapping.is_some() {
                return Err(anyhow::anyhow!(
                    "a previously mapped community must be reconciled before v1alpha2 opt-in"
                ));
            }
            if Self::legacy_membership_commands_outstanding(&mut tx, &mapping.community_id).await? {
                return Err(anyhow::anyhow!(
                    "legacy membership commands must drain before v1alpha2 opt-in"
                ));
            }
        }
        sqlx::query(
            "INSERT INTO chat_workspace_communities
                (mapping_id, tenant_id, workspace_id, community_id, relay_url, relay_pubkey, membership_protocol_version, active)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(Uuid::new_v4())
        .bind(mapping.tenant_id.0)
        .bind(mapping.workspace_id.0)
        .bind(&mapping.community_id)
        .bind(&mapping.relay_url)
        .bind(&mapping.relay_pubkey)
        .bind(&mapping.membership_protocol_version)
        .bind(mapping.active)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Insert the mapping only if the workspace has none, resolving races on
    /// the (tenant_id, workspace_id) unique key. A concurrent insert for the
    /// same workspace returns the winner (`AlreadyExists`). Any conflict on
    /// the (tenant_id, community_id) unique key or the one-active-per-
    /// community partial index is `CommunityInUse` — a mapping can never be
    /// silently stolen from or shared with another workspace.
    pub async fn provision_mapping(
        &self,
        mapping: &WorkspaceCommunityMapping,
    ) -> Result<ProvisionMappingOutcome, ProvisionMappingError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| ProvisionMappingError::Other(error.to_string()))?;
        if mapping.membership_protocol_version == "v1alpha2" {
            let previous_mapping = sqlx::query_scalar::<_, Uuid>(
                "SELECT mapping_id FROM chat_workspace_communities
                 WHERE community_id=$1 FOR UPDATE",
            )
            .bind(&mapping.community_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|error| ProvisionMappingError::Other(error.to_string()))?;
            if previous_mapping.is_some() {
                return Err(ProvisionMappingError::Other(
                    "a previously mapped community must be reconciled before v1alpha2 opt-in"
                        .into(),
                ));
            }
            if Self::legacy_membership_commands_outstanding(&mut tx, &mapping.community_id)
                .await
                .map_err(|error| ProvisionMappingError::Other(error.to_string()))?
            {
                return Err(ProvisionMappingError::Other(
                    "legacy membership commands must drain before v1alpha2 opt-in".into(),
                ));
            }
        }
        let result = sqlx::query(
            "INSERT INTO chat_workspace_communities
                (mapping_id, tenant_id, workspace_id, community_id, relay_url, relay_pubkey, membership_protocol_version, active)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(Uuid::new_v4())
        .bind(mapping.tenant_id.0)
        .bind(mapping.workspace_id.0)
        .bind(&mapping.community_id)
        .bind(&mapping.relay_url)
        .bind(&mapping.relay_pubkey)
        .bind(&mapping.membership_protocol_version)
        .bind(mapping.active)
        .execute(&mut *tx)
        .await;
        match result {
            Ok(_) => {
                tx.commit()
                    .await
                    .map_err(|error| ProvisionMappingError::Other(error.to_string()))?;
                Ok(ProvisionMappingOutcome::Inserted)
            }
            Err(error) => {
                tx.rollback()
                    .await
                    .map_err(|rollback| ProvisionMappingError::Other(rollback.to_string()))?;
                let constraint = error
                    .as_database_error()
                    .and_then(|db| db.constraint())
                    .map(str::to_string);
                match constraint.as_deref() {
                    Some("chat_workspace_communities_tenant_id_workspace_id_key") => {
                        let existing = self
                            .mapping(mapping.tenant_id, mapping.workspace_id)
                            .await
                            .map_err(|error| {
                                ProvisionMappingError::Other(error.to_string())
                            })?
                            .ok_or_else(|| {
                                ProvisionMappingError::Other(
                                    "mapping vanished between the failed insert and the winner re-read"
                                        .to_string(),
                                )
                            })?;
                        Ok(ProvisionMappingOutcome::AlreadyExists(existing))
                    }
                    Some("chat_workspace_communities_tenant_id_community_id_key")
                    | Some("chat_workspace_communities_active_community") => {
                        Err(ProvisionMappingError::CommunityInUse)
                    }
                    _ => Err(ProvisionMappingError::Other(error.to_string())),
                }
            }
        }
    }

    /// Rotate the mapping's relay endpoint and/or pinned pubkey WITHOUT
    /// changing `community_id`/`workspace_id` (which would orphan admissions).
    /// Both columns are always written: when rotating only the pin, pass the
    /// current `relay_url` again; when rotating only the URL, pass the current
    /// (or new) pin. Returns whether a mapping row matched
    /// (`rows_affected() == 1`).
    pub async fn update_mapping_relay(
        &self,
        tenant_id: TenantId,
        workspace_id: WorkspaceId,
        relay_url: String,
        relay_pubkey: Option<String>,
        membership_protocol_version: &str,
    ) -> Result<bool, sqlx::Error> {
        if !matches!(membership_protocol_version, "v1alpha1" | "v1alpha2") {
            return Ok(false);
        }
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query(
            "SELECT mapping_id,community_id,relay_url,relay_pubkey,membership_protocol_version
             FROM chat_workspace_communities WHERE tenant_id=$1 AND workspace_id=$2 FOR UPDATE",
        )
        .bind(tenant_id.0)
        .bind(workspace_id.0)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(row) = row else {
            tx.rollback().await?;
            return Ok(false);
        };
        let current: String = row.try_get("membership_protocol_version")?;
        let community_id: String = row.try_get("community_id")?;
        let mapping_id: Uuid = row.try_get("mapping_id")?;
        let endpoint_changed = row.try_get::<String, _>("relay_url")? != relay_url
            || row.try_get::<Option<String>, _>("relay_pubkey")? != relay_pubkey;
        if current == "v1alpha2" && membership_protocol_version != current {
            tx.rollback().await?;
            return Ok(false);
        }
        if membership_protocol_version == "v1alpha2" && relay_pubkey.is_none() {
            tx.rollback().await?;
            return Ok(false);
        }
        if membership_protocol_version == "v1alpha2"
            && current != "v1alpha2"
            && Self::legacy_membership_commands_outstanding(&mut tx, &community_id).await?
        {
            tx.rollback().await?;
            return Ok(false);
        }
        if endpoint_changed
            && sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM chat_buzz_membership_operations
                 WHERE tenant_id=$1 AND mapping_id=$2 AND state='pending')",
            )
            .bind(tenant_id.0)
            .bind(mapping_id)
            .fetch_one(&mut *tx)
            .await?
        {
            tx.rollback().await?;
            return Ok(false);
        }
        sqlx::query("UPDATE chat_workspace_communities SET relay_url=$1,relay_pubkey=$2,membership_protocol_version=$3 WHERE tenant_id=$4 AND workspace_id=$5")
            .bind(&relay_url).bind(&relay_pubkey).bind(membership_protocol_version).bind(tenant_id.0).bind(workspace_id.0).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(true)
    }

    async fn legacy_membership_commands_outstanding(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        community_id: &str,
    ) -> Result<bool, sqlx::Error> {
        sqlx::query_scalar(
            "SELECT EXISTS (
           SELECT 1 FROM integration_outbox o
           LEFT JOIN integration_deliveries d
             ON d.source=o.source AND d.event_id=o.event_id
            AND d.consumer_id='io.elembra.chat.buzz-bridge.v1'
           WHERE o.event_type IN (
             'io.elembra.chat.buzz.admission.requested.v1',
             'io.elembra.chat.buzz.admission.revoked.v1')
             AND o.event_json->'data'->>'community_id'=$1
             AND COALESCE(d.state,'pending') <> 'processed'
         )",
        )
        .bind(community_id)
        .fetch_one(&mut **tx)
        .await
    }

    pub async fn insert_challenge(&self, challenge: &BindingChallenge) -> Result<()> {
        sqlx::query(
            "INSERT INTO chat_binding_challenges
                (challenge_id, tenant_id, principal_id, buzz_pubkey, relay_url, rotation_of,
                 nonce, expires_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(challenge.challenge_id)
        .bind(challenge.tenant_id.0)
        .bind(challenge.principal_id.0)
        .bind(&challenge.buzz_pubkey)
        .bind(&challenge.relay_url)
        .bind(challenge.rotation_of)
        .bind(&challenge.nonce)
        .bind(challenge.expires_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Issue a challenge bound to the exact active workspace mapping snapshot.
    pub async fn insert_challenge_for_mapping(
        &self,
        challenge: &BindingChallenge,
        workspace_id: WorkspaceId,
    ) -> Result<bool> {
        let mut tx = self.pool.begin().await?;
        let mapping = sqlx::query(
            "SELECT mapping_id,community_id,relay_url,relay_pubkey,membership_protocol_version
             FROM chat_workspace_communities
             WHERE tenant_id=$1 AND workspace_id=$2 AND active FOR SHARE",
        )
        .bind(challenge.tenant_id.0)
        .bind(workspace_id.0)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(mapping) = mapping else {
            return Ok(false);
        };
        let mapping_id: Uuid = mapping.try_get("mapping_id")?;
        let community_id: String = mapping.try_get("community_id")?;
        let relay_url: String = mapping.try_get("relay_url")?;
        let relay_pubkey: Option<String> = mapping.try_get("relay_pubkey")?;
        let protocol: String = mapping.try_get("membership_protocol_version")?;
        if relay_url != challenge.relay_url {
            return Ok(false);
        }
        let fingerprint = replacement_challenge_mapping_fingerprint(
            mapping_id,
            &community_id,
            &relay_url,
            relay_pubkey.as_deref(),
            &protocol,
        );
        sqlx::query(
            "INSERT INTO chat_binding_challenges
                (challenge_id,tenant_id,principal_id,buzz_pubkey,relay_url,rotation_of,nonce,expires_at,mapping_fingerprint)
             VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)",
        )
        .bind(challenge.challenge_id)
        .bind(challenge.tenant_id.0)
        .bind(challenge.principal_id.0)
        .bind(&challenge.buzz_pubkey)
        .bind(&challenge.relay_url)
        .bind(challenge.rotation_of)
        .bind(&challenge.nonce)
        .bind(challenge.expires_at)
        .bind(fingerprint)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(true)
    }

    /// Record an admission and enqueue the generic Buzz bridge operation.
    /// The queue is the durable boundary; this method does not write Buzz state.
    pub async fn admit_binding(
        &self,
        tenant_id: TenantId,
        principal_id: PrincipalId,
        workspace_id: WorkspaceId,
    ) -> Result<bool> {
        let mut tx = self.pool.begin().await?;
        let principal = sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM users WHERE id = $1 AND tenant_id = $2 AND disabled_at IS NULL
             FOR NO KEY UPDATE",
        )
        .bind(principal_id.0)
        .bind(tenant_id.0)
        .fetch_optional(&mut *tx)
        .await?;
        if principal.is_none() {
            return Ok(false);
        }
        let row = sqlx::query(
            "SELECT b.binding_id, b.buzz_pubkey, m.mapping_id, m.community_id, m.relay_url,
                    m.relay_pubkey,
                    m.membership_protocol_version
             FROM chat_identity_bindings b
             JOIN users u
               ON u.tenant_id = b.tenant_id AND u.id = b.principal_id
              AND u.disabled_at IS NULL
             JOIN chat_workspace_communities m
               ON m.tenant_id = b.tenant_id AND m.workspace_id = $2 AND m.active
             JOIN application_enablements ae
               ON ae.tenant_id = m.tenant_id AND ae.workspace_id = m.workspace_id
              AND ae.application_id = 'io.elembra.chat' AND ae.enabled
             LEFT JOIN application_user_preferences aup
               ON aup.user_id = $3 AND aup.application_id = 'io.elembra.chat'
             WHERE b.tenant_id = $1 AND b.principal_id = $3 AND b.status = 'active'
               AND b.revoked_at IS NULL AND COALESCE(aup.enabled, true)
             LIMIT 1 FOR UPDATE OF m",
        )
        .bind(tenant_id.0)
        .bind(workspace_id.0)
        .bind(principal_id.0)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(row) = row else {
            return Ok(false);
        };
        let admission_id = Uuid::new_v4();
        let mapping_id: Uuid = row.try_get("mapping_id")?;
        let community_id: String = row.try_get("community_id")?;
        let relay_url: String = row.try_get("relay_url")?;
        let relay_pubkey: Option<String> = row.try_get("relay_pubkey")?;
        let buzz_pubkey: String = row.try_get("buzz_pubkey")?;
        sqlx::query(
            "INSERT INTO chat_buzz_admissions
                (admission_id, tenant_id, mapping_id, binding_id, buzz_pubkey)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(admission_id)
        .bind(tenant_id.0)
        .bind(mapping_id)
        .bind(row.try_get::<Uuid, _>("binding_id")?)
        .bind(&buzz_pubkey)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "WITH outbox_event AS (SELECT gen_random_uuid() AS event_id)
             INSERT INTO integration_outbox
                (source, event_id, event_type, application_id, tenant_id, workspace_id, event_json)
            SELECT 'elembra://io.elembra.chat', outbox_event.event_id,
                   'io.elembra.chat.buzz.admission.requested.v1', 'io.elembra.chat',
                   $1, $2,
                   jsonb_build_object(
                       'specversion', '1.0', 'id', outbox_event.event_id,
                       'source', 'elembra://io.elembra.chat',
                       'type', 'io.elembra.chat.buzz.admission.requested.v1',
                       'time', now(), 'datacontenttype', 'application/json',
                       'elembraTenant', $1, 'elembraWorkspace', $2,
                       'data', jsonb_build_object('operation', 'admit', 'admission_id', $3,
                                                  'community_id', $4, 'relay_url', $5,
                                                  'buzz_pubkey', $6)
                       || CASE WHEN $7 = 'v1alpha2' THEN jsonb_build_object(
                           'command_version', 1, 'command_id', outbox_event.event_id,
                           'relay_pubkey', $8)
                           ELSE '{}'::jsonb END)
            FROM outbox_event",
        )
        .bind(tenant_id.0)
        .bind(workspace_id.0)
        .bind(admission_id)
        .bind(community_id)
        .bind(relay_url)
        .bind(buzz_pubkey)
        .bind(row.try_get::<String, _>("membership_protocol_version")?)
        .bind(relay_pubkey)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(true)
    }

    pub async fn challenge(&self, challenge_id: Uuid) -> Result<Option<BindingChallenge>> {
        let row = sqlx::query(
            "SELECT challenge_id, tenant_id, principal_id, buzz_pubkey, relay_url,
                    rotation_of, nonce, expires_at, consumed_at
             FROM chat_binding_challenges WHERE challenge_id = $1",
        )
        .bind(challenge_id)
        .fetch_optional(&self.pool)
        .await?;

        row.map(|row| {
            Ok(BindingChallenge {
                challenge_id: row.try_get("challenge_id")?,
                tenant_id: TenantId(row.try_get("tenant_id")?),
                principal_id: PrincipalId(row.try_get("principal_id")?),
                buzz_pubkey: row.try_get("buzz_pubkey")?,
                relay_url: row.try_get("relay_url")?,
                rotation_of: row.try_get("rotation_of")?,
                nonce: row.try_get("nonce")?,
                expires_at: row.try_get("expires_at")?,
                consumed_at: row.try_get("consumed_at")?,
            })
        })
        .transpose()
    }

    /// Consume a challenge and activate a Principal's first binding atomically.
    pub async fn consume_and_activate(
        &self,
        challenge: &BindingChallenge,
        binding: &ChatIdentityBinding,
        now: DateTime<Utc>,
    ) -> Result<bool> {
        // The legacy rotation path activates the replacement before Buzz
        // confirms revocation. Keep it closed until the replacement protocol
        // has durable operation state and authoritative relay read-back.
        if challenge.rotation_of.is_some() {
            return Ok(false);
        }
        let mut tx = self.pool.begin().await?;
        if binding.tenant_id != challenge.tenant_id
            || binding.principal_id != challenge.principal_id
            || binding.buzz_pubkey != challenge.buzz_pubkey
            || binding.rotation_of != challenge.rotation_of
        {
            return Ok(false);
        }
        // Serialize activation with explicit revocation and account disablement.
        let principal = sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM users WHERE id = $1 AND tenant_id = $2 AND disabled_at IS NULL
             FOR NO KEY UPDATE",
        )
        .bind(challenge.principal_id.0)
        .bind(challenge.tenant_id.0)
        .fetch_optional(&mut *tx)
        .await?;
        if principal.is_none() {
            return Ok(false);
        }
        // Principals in different tenants must not race the same key's history check.
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(chat_binding_history_fence(&challenge.buzz_pubkey))
            .execute(&mut *tx)
            .await?;
        let key_seen = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM chat_identity_bindings
                           WHERE buzz_pubkey = $1)",
        )
        .bind(&challenge.buzz_pubkey)
        .fetch_one(&mut *tx)
        .await?;
        if key_seen {
            return Ok(false);
        }
        // Omitting rotation_of after revocation must not bypass replacement verification.
        let previously_bound = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(
                SELECT 1 FROM chat_identity_bindings
                WHERE tenant_id = $1 AND principal_id = $2
            )",
        )
        .bind(challenge.tenant_id.0)
        .bind(challenge.principal_id.0)
        .fetch_one(&mut *tx)
        .await?;
        if previously_bound {
            return Ok(false);
        }
        let consumed = sqlx::query(
            "UPDATE chat_binding_challenges
             SET consumed_at = $2
             WHERE challenge_id = $1 AND consumed_at IS NULL AND expires_at > $2",
        )
        .bind(challenge.challenge_id)
        .bind(now)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if consumed != 1 {
            return Ok(false);
        }

        sqlx::query(
            "INSERT INTO chat_identity_bindings
                (binding_id, tenant_id, principal_id, buzz_pubkey, status, verified_at,
                 rotation_of, audit_metadata)
             VALUES ($1, $2, $3, $4, 'active', $5, $6, $7)",
        )
        .bind(binding.binding_id)
        .bind(binding.tenant_id.0)
        .bind(binding.principal_id.0)
        .bind(&binding.buzz_pubkey)
        .bind(now)
        .bind(binding.rotation_of)
        .bind(&binding.audit_metadata)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO user_security_events
                (id, user_id, event_type, description)
             VALUES (gen_random_uuid(), $1, 'chat.identity.bound', $2)",
        )
        .bind(binding.principal_id.0)
        .bind(format!(
            "Buzz identity binding {} activated",
            binding.binding_id
        ))
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(true)
    }

    pub async fn active_binding(
        &self,
        tenant_id: TenantId,
        principal_id: PrincipalId,
    ) -> Result<Option<ChatIdentityBinding>> {
        let row = sqlx::query(
            "SELECT binding_id, tenant_id, principal_id, buzz_pubkey, status,
                    created_at, verified_at, revoked_at, rotation_of, audit_metadata
             FROM chat_identity_bindings
             WHERE tenant_id = $1 AND principal_id = $2 AND status = 'active'
               AND revoked_at IS NULL
             ORDER BY verified_at DESC NULLS LAST LIMIT 1",
        )
        .bind(tenant_id.0)
        .bind(principal_id.0)
        .fetch_optional(&self.pool)
        .await?;

        row.map(|row| {
            let status = match row.try_get::<String, _>("status")?.as_str() {
                "active" => BindingStatus::Active,
                "pending" => BindingStatus::Pending,
                _ => BindingStatus::Revoked,
            };
            Ok(ChatIdentityBinding {
                binding_id: row.try_get("binding_id")?,
                tenant_id: TenantId(row.try_get("tenant_id")?),
                principal_id: PrincipalId(row.try_get("principal_id")?),
                buzz_pubkey: row.try_get("buzz_pubkey")?,
                status,
                created_at: row.try_get("created_at")?,
                verified_at: row.try_get("verified_at")?,
                revoked_at: row.try_get("revoked_at")?,
                rotation_of: row.try_get("rotation_of")?,
                audit_metadata: row.try_get("audit_metadata")?,
            })
        })
        .transpose()
    }

    /// Resolve display metadata for a batch of Buzz pubkeys in one query.
    ///
    /// This is a historical resolution: revoked bindings are kept so messages
    /// authored before a rotation/revocation still show the correct display
    /// name. Per pubkey the query prefers an **active** binding; if none is
    /// active it falls back to the most recently created binding. Disabled
    /// users are not filtered out because their historical display names are
    /// still needed for the timeline. Tenant isolation is enforced on the
    /// binding table (`b.tenant_id = $1`).
    pub async fn resolve_authors_by_pubkeys(
        &self,
        tenant_id: TenantId,
        pubkeys: &[String],
    ) -> Result<Vec<ResolvedChatAuthor>, sqlx::Error> {
        if pubkeys.is_empty() {
            return Ok(Vec::new());
        }

        let rows = sqlx::query(
            "SELECT DISTINCT ON (b.buzz_pubkey)
                    b.buzz_pubkey,
                    b.principal_id,
                    u.display_name,
                    u.avatar_path
             FROM chat_identity_bindings b
             JOIN users u ON u.id = b.principal_id
             WHERE b.tenant_id = $1
               AND u.tenant_id = $1
               AND b.buzz_pubkey = ANY($2)
             ORDER BY b.buzz_pubkey,
                      CASE WHEN b.status = 'active' AND b.revoked_at IS NULL THEN 0 ELSE 1 END,
                      b.created_at DESC",
        )
        .bind(tenant_id.0)
        .bind(pubkeys)
        .fetch_all(&self.pool)
        .await?;

        rows.iter()
            .map(|row| {
                Ok(ResolvedChatAuthor {
                    buzz_pubkey: row.try_get("buzz_pubkey")?,
                    principal_id: PrincipalId(row.try_get("principal_id")?),
                    display_name: row.try_get("display_name")?,
                    avatar_path: row.try_get("avatar_path")?,
                })
            })
            .collect()
    }

    /// Reverse mapping: live binding for a Buzz pubkey in this tenant, if any.
    /// The unique live index `chat_identity_bindings_live_key` guarantees at most one.
    pub async fn binding_by_pubkey(
        &self,
        tenant_id: TenantId,
        buzz_pubkey: &str,
    ) -> Result<Option<ChatIdentityBinding>> {
        let row = sqlx::query(
            "SELECT binding_id, tenant_id, principal_id, buzz_pubkey, status,
                    created_at, verified_at, revoked_at, rotation_of, audit_metadata
             FROM chat_identity_bindings
             WHERE tenant_id = $1 AND buzz_pubkey = $2 AND revoked_at IS NULL",
        )
        .bind(tenant_id.0)
        .bind(buzz_pubkey)
        .fetch_optional(&self.pool)
        .await?;

        row.map(|row| {
            let status = match row.try_get::<String, _>("status")?.as_str() {
                "active" => BindingStatus::Active,
                "pending" => BindingStatus::Pending,
                _ => BindingStatus::Revoked,
            };
            Ok(ChatIdentityBinding {
                binding_id: row.try_get("binding_id")?,
                tenant_id: TenantId(row.try_get("tenant_id")?),
                principal_id: PrincipalId(row.try_get("principal_id")?),
                buzz_pubkey: row.try_get("buzz_pubkey")?,
                status,
                created_at: row.try_get("created_at")?,
                verified_at: row.try_get("verified_at")?,
                revoked_at: row.try_get("revoked_at")?,
                rotation_of: row.try_get("rotation_of")?,
                audit_metadata: row.try_get("audit_metadata")?,
            })
        })
        .transpose()
    }

    /// Whether `buzz_pubkey` currently has an active admission in
    /// `community_id` (both the admission and the community mapping must be
    /// active).
    pub async fn active_admission(
        &self,
        tenant_id: TenantId,
        community_id: &str,
        buzz_pubkey: &str,
    ) -> Result<bool> {
        Ok(sqlx::query_scalar(
            "SELECT EXISTS(
                SELECT 1
                FROM chat_buzz_admissions a
                JOIN chat_workspace_communities m
                  ON a.tenant_id = m.tenant_id AND a.mapping_id = m.mapping_id
                WHERE a.tenant_id = $1 AND a.active AND m.active
                  AND m.community_id = $2 AND a.buzz_pubkey = $3
            )",
        )
        .bind(tenant_id.0)
        .bind(community_id)
        .bind(buzz_pubkey)
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn revoke_principal(
        &self,
        tenant_id: TenantId,
        principal_id: PrincipalId,
    ) -> Result<u64> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT id FROM users WHERE id = $1 AND tenant_id = $2 FOR NO KEY UPDATE")
            .bind(principal_id.0)
            .bind(tenant_id.0)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "INSERT INTO integration_outbox
                (source, event_id, event_type, application_id, tenant_id, workspace_id, event_json)
             SELECT 'elembra://io.elembra.chat', outbox_event.event_id,
                    'io.elembra.chat.buzz.admission.revoked.v1', 'io.elembra.chat',
                    a.tenant_id, m.workspace_id,
                    jsonb_build_object(
                        'specversion', '1.0', 'id', outbox_event.event_id,
                        'source', 'elembra://io.elembra.chat',
                        'type', 'io.elembra.chat.buzz.admission.revoked.v1',
                        'time', now(), 'datacontenttype', 'application/json',
                        'elembraTenant', a.tenant_id, 'elembraWorkspace', m.workspace_id,
                    'data', jsonb_build_object('operation', 'revoke',
                                                   'admission_id', a.admission_id,
                                                   'community_id', m.community_id,
                                                   'relay_url', m.relay_url,
                                                   'buzz_pubkey', a.buzz_pubkey)
                       || CASE WHEN m.membership_protocol_version = 'v1alpha2' THEN jsonb_build_object(
                           'command_version', 1, 'command_id', outbox_event.event_id,
                           'relay_pubkey', m.relay_pubkey)
                           ELSE '{}'::jsonb END)
             FROM chat_buzz_admissions a
             JOIN chat_identity_bindings b
               ON b.tenant_id = a.tenant_id AND b.binding_id = a.binding_id
             JOIN chat_workspace_communities m
               ON m.tenant_id = a.tenant_id AND m.mapping_id = a.mapping_id
             CROSS JOIN LATERAL (SELECT gen_random_uuid() AS event_id) outbox_event
             WHERE a.tenant_id = $1 AND b.principal_id = $2 AND a.active
             FOR UPDATE OF m",
        )
        .bind(tenant_id.0)
        .bind(principal_id.0)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE chat_buzz_admissions a
             SET active = false, revoked_at = COALESCE(a.revoked_at, now())
             FROM chat_identity_bindings b
             WHERE a.tenant_id = $1 AND a.binding_id = b.binding_id
               AND b.tenant_id = $1 AND b.principal_id = $2 AND a.active",
        )
        .bind(tenant_id.0)
        .bind(principal_id.0)
        .execute(&mut *tx)
        .await?;
        let result = sqlx::query(
            "UPDATE chat_identity_bindings
             SET status = 'revoked', revoked_at = COALESCE(revoked_at, now())
             WHERE tenant_id = $1 AND principal_id = $2 AND status <> 'revoked'",
        )
        .bind(tenant_id.0)
        .bind(principal_id.0)
        .execute(&mut *tx)
        .await?;
        if result.rows_affected() > 0 {
            sqlx::query(
                "INSERT INTO user_security_events
                    (id, user_id, event_type, description)
                 VALUES (gen_random_uuid(), $1, 'chat.identity.revoked', $2)",
            )
            .bind(principal_id.0)
            .bind("Buzz identity binding/admission explicitly revoked")
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(result.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacement_and_first_binding_share_global_history_fence() {
        let candidate = "ab".repeat(32);
        assert_eq!(
            chat_binding_history_fence(&candidate),
            format!("chat-binding:{candidate}")
        );
    }
    use sqlx::PgPool;

    #[tokio::test]
    async fn legacy_replacement_is_rejected_before_storage_changes() {
        let pool = PgPool::connect_lazy("postgres://unused:unused@127.0.0.1/unused").unwrap();
        let store = ChatIdentityStore::new(pool);
        let now = Utc::now();
        let previous_binding = Uuid::new_v4();
        let challenge = BindingChallenge::issue(
            TenantId(Uuid::new_v4()),
            PrincipalId(Uuid::new_v4()),
            "12".repeat(32),
            "wss://relay.example.test",
            Some(previous_binding),
            now,
            chrono::Duration::minutes(5),
        );
        let replacement = ChatIdentityBinding {
            binding_id: Uuid::new_v4(),
            tenant_id: challenge.tenant_id,
            principal_id: challenge.principal_id,
            buzz_pubkey: challenge.buzz_pubkey.clone(),
            status: BindingStatus::Active,
            created_at: now,
            verified_at: Some(now),
            revoked_at: None,
            rotation_of: Some(previous_binding),
            audit_metadata: serde_json::json!({}),
        };

        assert!(!store
            .consume_and_activate(&challenge, &replacement, now)
            .await
            .unwrap());
    }
}
