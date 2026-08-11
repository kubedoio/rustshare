//! Durable Elembra-owned state for the Chat identity/admission boundary.
//!
//! This store never contains a Buzz private key and never reads Buzz tables.

use anyhow::Result;
use chrono::{DateTime, Utc};
use rustshare_core::domain::{PrincipalId, TenantId, WorkspaceId};
use rustshare_resource_auth::{
    BindingChallenge, BindingStatus, ChatIdentityBinding, WorkspaceCommunityMapping,
};
use sqlx::{PgPool, Row};
use uuid::Uuid;

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

impl ChatIdentityStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn mapping(
        &self,
        tenant_id: TenantId,
        workspace_id: WorkspaceId,
    ) -> Result<Option<WorkspaceCommunityMapping>> {
        let row = sqlx::query(
            "SELECT tenant_id, workspace_id, community_id, relay_url, relay_pubkey, active
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
            "SELECT tenant_id, workspace_id, community_id, relay_url, relay_pubkey, active
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
        sqlx::query(
            "INSERT INTO chat_workspace_communities
                (mapping_id, tenant_id, workspace_id, community_id, relay_url, relay_pubkey, active)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(Uuid::new_v4())
        .bind(mapping.tenant_id.0)
        .bind(mapping.workspace_id.0)
        .bind(&mapping.community_id)
        .bind(&mapping.relay_url)
        .bind(&mapping.relay_pubkey)
        .bind(mapping.active)
        .execute(&self.pool)
        .await?;
        Ok(())
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
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            "UPDATE chat_workspace_communities
             SET relay_url = $1, relay_pubkey = $2
             WHERE tenant_id = $3 AND workspace_id = $4",
        )
        .bind(&relay_url)
        .bind(&relay_pubkey)
        .bind(tenant_id.0)
        .bind(workspace_id.0)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
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

    /// Record an admission and enqueue the generic Buzz bridge operation.
    /// The queue is the durable boundary; this method does not write Buzz state.
    pub async fn admit_binding(
        &self,
        tenant_id: TenantId,
        principal_id: PrincipalId,
        workspace_id: WorkspaceId,
    ) -> Result<bool> {
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query(
            "SELECT b.binding_id, b.buzz_pubkey, m.mapping_id, m.community_id, m.relay_url
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
             LIMIT 1",
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
                                                  'buzz_pubkey', $6))
            FROM outbox_event",
        )
        .bind(tenant_id.0)
        .bind(workspace_id.0)
        .bind(admission_id)
        .bind(community_id)
        .bind(relay_url)
        .bind(buzz_pubkey)
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

    /// Consume a challenge and activate its binding atomically.
    pub async fn consume_and_activate(
        &self,
        challenge: &BindingChallenge,
        binding: &ChatIdentityBinding,
        now: DateTime<Utc>,
    ) -> Result<bool> {
        let mut tx = self.pool.begin().await?;
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

        if let Some(rotation_of) = challenge.rotation_of {
            let previous_active = sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(
                    SELECT 1 FROM chat_identity_bindings
                    WHERE binding_id = $1 AND tenant_id = $2 AND principal_id = $3
                      AND status = 'active' AND revoked_at IS NULL
                )",
            )
            .bind(rotation_of)
            .bind(challenge.tenant_id.0)
            .bind(challenge.principal_id.0)
            .fetch_one(&mut *tx)
            .await?;
            if !previous_active {
                return Ok(false);
            }
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
                                                       'buzz_pubkey', a.buzz_pubkey))
                 FROM chat_buzz_admissions a
                 JOIN chat_workspace_communities m
                   ON m.tenant_id = a.tenant_id AND m.mapping_id = a.mapping_id
                 CROSS JOIN LATERAL (SELECT gen_random_uuid() AS event_id) outbox_event
                 WHERE a.tenant_id = $1 AND a.binding_id = $2 AND a.active",
            )
            .bind(challenge.tenant_id.0)
            .bind(rotation_of)
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                "UPDATE chat_buzz_admissions
                 SET active = false, revoked_at = COALESCE(revoked_at, now())
                 WHERE tenant_id = $1 AND binding_id = $2 AND active",
            )
            .bind(challenge.tenant_id.0)
            .bind(rotation_of)
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                "UPDATE chat_identity_bindings
                 SET status = 'revoked', revoked_at = COALESCE(revoked_at, now())
                 WHERE binding_id = $1 AND tenant_id = $2",
            )
            .bind(rotation_of)
            .bind(challenge.tenant_id.0)
            .execute(&mut *tx)
            .await?;
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
                                                   'buzz_pubkey', a.buzz_pubkey))
             FROM chat_buzz_admissions a
             JOIN chat_identity_bindings b
               ON b.tenant_id = a.tenant_id AND b.binding_id = a.binding_id
             JOIN chat_workspace_communities m
               ON m.tenant_id = a.tenant_id AND m.mapping_id = a.mapping_id
             CROSS JOIN LATERAL (SELECT gen_random_uuid() AS event_id) outbox_event
             WHERE a.tenant_id = $1 AND b.principal_id = $2 AND a.active",
        )
        .bind(tenant_id.0)
        .bind(principal_id.0)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE chat_buzz_admissions a
             SET active = false, revoked_at = COALESCE(revoked_at, now())
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
