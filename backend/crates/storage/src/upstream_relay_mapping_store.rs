//! Default-off persistence for trusted upstream Buzz relay mappings.
//!
//! This store is not called by a route, worker, or relay connector. Only
//! `strict_public` mappings are representable until the trusted-service
//! exception has independent approval and network-enforced egress controls.

use std::net::IpAddr;
use std::str::FromStr;
use std::sync::Arc;

use chrono::{DateTime, Timelike, Utc};
use rustshare_core::domain::{TenantId, WorkspaceId};
use rustshare_resource_auth::upstream_relay_mapping::{
    verify_mapping_revision_record, verify_mapping_update, MappingError, RelayMappingRecord,
    TrustedRelayMapping,
};
use rustshare_resource_auth::upstream_relay_provisioning::{
    MappingProvisioningAction, MappingProvisioningStatement, MappingProvisioningVerifier,
    VerifiedMappingProvisioning,
};
use serde_json::{json, Value};
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error, Eq, PartialEq)]
pub enum UpstreamRelayMappingStoreError {
    #[error("upstream relay mapping storage failed")]
    Database,
    #[error("upstream relay mapping conflicts with an existing binding or generation")]
    Conflict,
    #[error("upstream relay mapping record is invalid")]
    InvalidRecord,
    #[error(transparent)]
    Policy(#[from] MappingError),
}

/// Persistence only; no production runtime caller is wired.
#[derive(Clone)]
pub struct UpstreamRelayMappingStore {
    pool: PgPool,
    verifier: Option<Arc<MappingProvisioningVerifier>>,
}

impl UpstreamRelayMappingStore {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            verifier: None,
        }
    }

    /// Construct a write-capable store with deployment-owned verifier configuration.
    pub fn with_provisioning_verifier(
        pool: PgPool,
        verifier: Arc<MappingProvisioningVerifier>,
    ) -> Self {
        Self {
            pool,
            verifier: Some(verifier),
        }
    }

    /// Apply a write only after the deployment/operator signature was verified.
    pub async fn apply(
        &self,
        statement: &MappingProvisioningStatement,
    ) -> Result<(), UpstreamRelayMappingStoreError> {
        let verifier = self
            .verifier
            .as_ref()
            .ok_or(UpstreamRelayMappingStoreError::InvalidRecord)?;
        let command = verifier
            .verify(statement, Utc::now())
            .map_err(|_| UpstreamRelayMappingStoreError::InvalidRecord)?;
        let record = command.record().clone();
        let change_reference_id = command.change_reference_id();
        let expected_generation = command.expected_generation();
        let result = match command.action() {
            MappingProvisioningAction::Create => {
                self.create_with_authority(&record, change_reference_id, Some(&command))
                    .await
            }
            MappingProvisioningAction::BindWorkspace => {
                self.bind_workspace_with_authority(
                    record.tenant_id,
                    record.workspace_id,
                    record.updated_by,
                    change_reference_id,
                    Some(&command),
                )
                .await
            }
            MappingProvisioningAction::Update => {
                let current = self.load(record.tenant_id, record.workspace_id).await?;
                self.update_with_authority(
                    &current,
                    &record,
                    expected_generation,
                    change_reference_id,
                    Some(&command),
                )
                .await
            }
            MappingProvisioningAction::Renew => {
                let current = self
                    .load_for_management(record.tenant_id, record.workspace_id)
                    .await?;
                self.renew_expired_with_authority(
                    &current,
                    record.valid_until,
                    expected_generation,
                    record.updated_by,
                    change_reference_id,
                    Some(&command),
                )
                .await
            }
            MappingProvisioningAction::Deactivate => {
                let current = self
                    .load_for_management(record.tenant_id, record.workspace_id)
                    .await?;
                self.deactivate_with_authority(
                    &current,
                    expected_generation,
                    record.updated_by,
                    change_reference_id,
                    Some(&command),
                )
                .await
            }
        };
        match result {
            Ok(()) => Ok(()),
            Err(_error) if self.operation_was_applied(&command).await? => Ok(()),
            Err(error) => Err(error),
        }
    }

    async fn operation_was_applied(
        &self,
        command: &VerifiedMappingProvisioning,
    ) -> Result<bool, UpstreamRelayMappingStoreError> {
        let mut tx = self.pool.begin().await.map_err(map_database_error)?;
        set_tenant(&mut tx, command.record().tenant_id.0).await?;
        let applied = lock_and_check_replay(&mut tx, command).await?;
        tx.commit().await.map_err(map_database_error)?;
        Ok(applied)
    }

    /// Create the tenant's sole relay mapping and bind its first workspace.
    /// Retrying the same audited record is idempotent.
    #[cfg(test)]
    async fn create(
        &self,
        record: &RelayMappingRecord,
        change_reference_id: Uuid,
    ) -> Result<(), UpstreamRelayMappingStoreError> {
        self.create_with_authority(record, change_reference_id, None)
            .await
    }

    async fn create_with_authority(
        &self,
        record: &RelayMappingRecord,
        change_reference_id: Uuid,
        command: Option<&VerifiedMappingProvisioning>,
    ) -> Result<(), UpstreamRelayMappingStoreError> {
        let record = database_precision_record(record)?;
        let now = Utc::now();
        validate_record(&record, change_reference_id, now)?;
        let generation = db_generation(record.generation)?;
        let mut tx = self.pool.begin().await.map_err(map_database_error)?;
        set_tenant(&mut tx, record.tenant_id.0).await?;
        if let Some(command) = command {
            validate_write_command(
                command,
                MappingProvisioningAction::Create,
                &record,
                0,
                change_reference_id,
            )?;
            if lock_and_check_replay(&mut tx, command).await? {
                tx.commit().await.map_err(map_database_error)?;
                return Ok(());
            }
        }

        let inserted = sqlx::query_as::<_, RelayRow>(
            "INSERT INTO upstream_buzz_tenant_relays \
             (mapping_id,tenant_id,relay_url,relay_host,community_id,relay_deployment_id,endpoint_service_identity,tls_peer_spki_sha256,relay_pubkey, \
              deployment_digest,approved_ips,generation,active,valid_until,updated_by,audit_entry_id) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16) \
             ON CONFLICT (tenant_id) DO NOTHING \
             RETURNING mapping_id,tenant_id,relay_url,relay_host,community_id,relay_deployment_id,endpoint_service_identity,tls_peer_spki_sha256,relay_pubkey, \
              deployment_digest,approved_ips,generation,active,valid_until,updated_by,audit_entry_id",
        )
        .bind(record.mapping_id)
        .bind(record.tenant_id.0)
        .bind(&record.relay_url)
        .bind(&record.relay_host)
        .bind(&record.community_id)
        .bind(record.relay_deployment_id)
        .bind(&record.endpoint_service_identity)
        .bind(&record.tls_peer_spki_sha256)
        .bind(&record.relay_pubkey)
        .bind(&record.deployment_digest)
        .bind(ip_strings(&record.approved_ips))
        .bind(generation)
        .bind(record.active)
        .bind(record.valid_until)
        .bind(record.updated_by)
        .bind(record.audit_entry_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_database_error)?;

        if inserted.is_some() {
            if let Some(command) = command {
                record_operation(&mut tx, command).await?;
            }
            let changed = mapping_snapshot(&record);
            insert_audit(
                &mut tx,
                &record,
                None,
                "mapping_created",
                change_reference_id,
                changed,
                command,
            )
            .await?;
            bind_workspace(
                &mut tx,
                &record,
                generation,
                record.updated_by,
                change_reference_id,
                command,
            )
            .await?;
        } else {
            if command.is_some() {
                return Err(UpstreamRelayMappingStoreError::Conflict);
            }
            let existing = sqlx::query_as::<_, RelayRow>(
                "SELECT mapping_id,tenant_id,relay_url,relay_host,community_id,relay_deployment_id,endpoint_service_identity,tls_peer_spki_sha256,relay_pubkey, \
                 deployment_digest,approved_ips,generation,active,valid_until,updated_by,audit_entry_id \
                 FROM upstream_buzz_tenant_relays WHERE tenant_id=$1",
            )
            .bind(record.tenant_id.0)
            .fetch_optional(&mut *tx)
            .await
            .map_err(map_database_error)?
            .ok_or(UpstreamRelayMappingStoreError::Conflict)?;
            if existing.to_record(record.workspace_id.0)? != record {
                return Err(UpstreamRelayMappingStoreError::Conflict);
            }
            bind_workspace(
                &mut tx,
                &record,
                generation,
                record.updated_by,
                change_reference_id,
                command,
            )
            .await?;
        }

        tx.commit().await.map_err(map_database_error)
    }

    /// Bind another workspace to the tenant's existing isolated relay.
    #[cfg(test)]
    async fn bind_workspace(
        &self,
        tenant_id: TenantId,
        workspace_id: WorkspaceId,
        actor_id: Uuid,
        change_reference_id: Uuid,
    ) -> Result<(), UpstreamRelayMappingStoreError> {
        self.bind_workspace_with_authority(
            tenant_id,
            workspace_id,
            actor_id,
            change_reference_id,
            None,
        )
        .await
    }

    async fn bind_workspace_with_authority(
        &self,
        tenant_id: TenantId,
        workspace_id: WorkspaceId,
        actor_id: Uuid,
        change_reference_id: Uuid,
        command: Option<&VerifiedMappingProvisioning>,
    ) -> Result<(), UpstreamRelayMappingStoreError> {
        let now = Utc::now();
        if actor_id.is_nil() || change_reference_id.is_nil() || workspace_id.0.is_nil() {
            return Err(UpstreamRelayMappingStoreError::InvalidRecord);
        }
        let mut tx = self.pool.begin().await.map_err(map_database_error)?;
        set_tenant(&mut tx, tenant_id.0).await?;
        if let Some(command) = command {
            if lock_and_check_replay(&mut tx, command).await? {
                tx.commit().await.map_err(map_database_error)?;
                return Ok(());
            }
        }
        let row = sqlx::query_as::<_, RelayRow>(
            "SELECT mapping_id,tenant_id,relay_url,relay_host,community_id,relay_deployment_id,endpoint_service_identity,tls_peer_spki_sha256,relay_pubkey, \
             deployment_digest,approved_ips,generation,active,valid_until,updated_by,audit_entry_id \
             FROM upstream_buzz_tenant_relays WHERE tenant_id=$1 FOR UPDATE",
        )
        .bind(tenant_id.0)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_database_error)?
        .ok_or(MappingError::MissingMapping)?;
        let record = row.to_record(workspace_id.0)?;
        TrustedRelayMapping::validate(record.clone(), now)?;
        if let Some(command) = command {
            validate_write_command(
                command,
                MappingProvisioningAction::BindWorkspace,
                &record,
                record.generation,
                change_reference_id,
            )?;
            let mut signed_record = record.clone();
            signed_record.workspace_id = command.record().workspace_id;
            if &signed_record != command.record() {
                return Err(UpstreamRelayMappingStoreError::Conflict);
            }
            record_operation(&mut tx, command).await?;
        }
        bind_workspace(
            &mut tx,
            &record,
            db_generation(record.generation)?,
            actor_id,
            change_reference_id,
            command,
        )
        .await?;
        tx.commit().await.map_err(map_database_error)
    }

    /// Resolve exactly one current mapping for the authenticated tenant/workspace.
    pub async fn load(
        &self,
        tenant_id: TenantId,
        workspace_id: WorkspaceId,
    ) -> Result<TrustedRelayMapping, UpstreamRelayMappingStoreError> {
        let record = self.load_record(tenant_id, workspace_id).await?;
        Ok(TrustedRelayMapping::validate(record, Utc::now())?)
    }

    /// Read a tenant-scoped record for an authorized control-plane renewal.
    /// Expired or inactive records are returned raw and are never connectable.
    pub async fn load_for_management(
        &self,
        tenant_id: TenantId,
        workspace_id: WorkspaceId,
    ) -> Result<RelayMappingRecord, UpstreamRelayMappingStoreError> {
        self.load_record(tenant_id, workspace_id).await
    }

    async fn load_record(
        &self,
        tenant_id: TenantId,
        workspace_id: WorkspaceId,
    ) -> Result<RelayMappingRecord, UpstreamRelayMappingStoreError> {
        let mut tx = self.pool.begin().await.map_err(map_database_error)?;
        set_tenant(&mut tx, tenant_id.0).await?;
        let row = sqlx::query_as::<_, RelayRow>(
            "SELECT r.mapping_id,r.tenant_id,r.relay_url,r.relay_host,r.community_id,r.relay_deployment_id,r.endpoint_service_identity,r.tls_peer_spki_sha256,r.relay_pubkey, \
             r.deployment_digest,r.approved_ips,r.generation,r.active,r.valid_until,r.updated_by,r.audit_entry_id \
             FROM upstream_buzz_tenant_relays r \
             JOIN upstream_buzz_workspace_relay_bindings b \
               ON b.tenant_id=r.tenant_id AND b.mapping_id=r.mapping_id \
             WHERE b.tenant_id=$1 AND b.workspace_id=$2",
        )
        .bind(tenant_id.0)
        .bind(workspace_id.0)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_database_error)?
        .ok_or(MappingError::MissingMapping)?;
        let record = row.to_record(workspace_id.0)?;
        tx.commit().await.map_err(map_database_error)?;
        Ok(record)
    }

    /// Compare-and-swap the one tenant relay. The generation change applies to
    /// all workspace bindings and makes existing connector snapshots stale.
    #[cfg(test)]
    async fn update(
        &self,
        current: &TrustedRelayMapping,
        next: &RelayMappingRecord,
        expected_generation: u64,
        change_reference_id: Uuid,
    ) -> Result<(), UpstreamRelayMappingStoreError> {
        self.update_with_authority(
            current,
            next,
            expected_generation,
            change_reference_id,
            None,
        )
        .await
    }

    async fn update_with_authority(
        &self,
        current: &TrustedRelayMapping,
        next: &RelayMappingRecord,
        expected_generation: u64,
        change_reference_id: Uuid,
        command: Option<&VerifiedMappingProvisioning>,
    ) -> Result<(), UpstreamRelayMappingStoreError> {
        let now = Utc::now();
        let next = database_precision_record(next)?;
        validate_record(&next, change_reference_id, now)?;
        verify_mapping_update(current, next.clone(), expected_generation, now)?;
        if let Some(command) = command {
            validate_write_command(
                command,
                MappingProvisioningAction::Update,
                &next,
                expected_generation,
                change_reference_id,
            )?;
        }
        self.persist_update(
            current.record(),
            &next,
            expected_generation,
            change_reference_id,
            command,
        )
        .await
    }

    /// Renew an expired active mapping after an authorized operator reloads it.
    /// The database row and generation are rechecked in the update transaction.
    #[cfg(test)]
    async fn renew_expired(
        &self,
        current: &RelayMappingRecord,
        valid_until: DateTime<Utc>,
        expected_generation: u64,
        actor_id: Uuid,
        change_reference_id: Uuid,
    ) -> Result<(), UpstreamRelayMappingStoreError> {
        self.renew_expired_with_authority(
            current,
            valid_until,
            expected_generation,
            actor_id,
            change_reference_id,
            None,
        )
        .await
    }

    async fn renew_expired_with_authority(
        &self,
        current: &RelayMappingRecord,
        valid_until: DateTime<Utc>,
        expected_generation: u64,
        actor_id: Uuid,
        change_reference_id: Uuid,
        command: Option<&VerifiedMappingProvisioning>,
    ) -> Result<(), UpstreamRelayMappingStoreError> {
        let now = Utc::now();
        if actor_id.is_nil()
            || change_reference_id.is_nil()
            || !current.active
            || current.valid_until >= now
        {
            return Err(UpstreamRelayMappingStoreError::InvalidRecord);
        }
        let mut next = current.clone();
        next.generation = expected_generation
            .checked_add(1)
            .ok_or(UpstreamRelayMappingStoreError::Conflict)?;
        next.active = true;
        next.valid_until = valid_until;
        next.updated_by = actor_id;
        next.audit_entry_id = command
            .map(|command| command.record().audit_entry_id)
            .unwrap_or_else(Uuid::new_v4);
        let next = database_precision_record(&next)?;
        validate_record(&next, change_reference_id, now)?;
        verify_mapping_revision_record(current, &next, expected_generation)?;
        if let Some(command) = command {
            validate_write_command(
                command,
                MappingProvisioningAction::Renew,
                &next,
                expected_generation,
                change_reference_id,
            )?;
        }
        self.persist_update(
            current,
            &next,
            expected_generation,
            change_reference_id,
            command,
        )
        .await
    }

    /// Disable the tenant mapping immediately with an audited generation bump.
    /// An inactive mapping cannot be loaded or used by a connector.
    #[cfg(test)]
    async fn deactivate(
        &self,
        current: &TrustedRelayMapping,
        expected_generation: u64,
        actor_id: Uuid,
        change_reference_id: Uuid,
    ) -> Result<(), UpstreamRelayMappingStoreError> {
        self.deactivate_with_authority(
            current.record(),
            expected_generation,
            actor_id,
            change_reference_id,
            None,
        )
        .await
    }

    async fn deactivate_with_authority(
        &self,
        current: &RelayMappingRecord,
        expected_generation: u64,
        actor_id: Uuid,
        change_reference_id: Uuid,
        command: Option<&VerifiedMappingProvisioning>,
    ) -> Result<(), UpstreamRelayMappingStoreError> {
        if actor_id.is_nil() || change_reference_id.is_nil() {
            return Err(UpstreamRelayMappingStoreError::InvalidRecord);
        }
        let mut next = current.clone();
        next.generation = expected_generation
            .checked_add(1)
            .ok_or(UpstreamRelayMappingStoreError::Conflict)?;
        next.active = false;
        next.updated_by = actor_id;
        next.audit_entry_id = command
            .map(|command| command.record().audit_entry_id)
            .unwrap_or_else(Uuid::new_v4);
        verify_mapping_revision_record(current, &next, expected_generation)?;
        if let Some(command) = command {
            validate_write_command(
                command,
                MappingProvisioningAction::Deactivate,
                &next,
                expected_generation,
                change_reference_id,
            )?;
        }
        self.persist_update(
            current,
            &next,
            expected_generation,
            change_reference_id,
            command,
        )
        .await
    }

    async fn persist_update(
        &self,
        current: &RelayMappingRecord,
        next: &RelayMappingRecord,
        expected_generation: u64,
        change_reference_id: Uuid,
        command: Option<&VerifiedMappingProvisioning>,
    ) -> Result<(), UpstreamRelayMappingStoreError> {
        let tenant_id = current.tenant_id.0;
        let workspace_id = current.workspace_id.0;
        let mapping_id = current.mapping_id;
        let generation = db_generation(next.generation)?;
        let mut tx = self.pool.begin().await.map_err(map_database_error)?;
        set_tenant(&mut tx, tenant_id).await?;
        if let Some(command) = command {
            if lock_and_check_replay(&mut tx, command).await? {
                tx.commit().await.map_err(map_database_error)?;
                return Ok(());
            }
        }
        let old = sqlx::query_as::<_, RelayRow>(
            "SELECT mapping_id,tenant_id,relay_url,relay_host,community_id,relay_deployment_id,endpoint_service_identity,tls_peer_spki_sha256,relay_pubkey, \
             deployment_digest,approved_ips,generation,active,valid_until,updated_by,audit_entry_id \
             FROM upstream_buzz_tenant_relays WHERE tenant_id=$1 AND mapping_id=$2 FOR UPDATE",
        )
        .bind(tenant_id)
        .bind(mapping_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_database_error)?
        .ok_or(UpstreamRelayMappingStoreError::Conflict)?;
        if old.to_record(workspace_id)? != *current
            || u64::try_from(old.generation).ok() != Some(expected_generation)
        {
            return Err(UpstreamRelayMappingStoreError::Conflict);
        }
        let bound: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM upstream_buzz_workspace_relay_bindings \
             WHERE tenant_id=$1 AND workspace_id=$2 AND mapping_id=$3)",
        )
        .bind(tenant_id)
        .bind(workspace_id)
        .bind(mapping_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(map_database_error)?;
        if !bound {
            return Err(UpstreamRelayMappingStoreError::Conflict);
        }
        let changed = changed_snapshot(&old, next)?;
        let result = sqlx::query(
            "UPDATE upstream_buzz_tenant_relays SET relay_url=$4,relay_host=$5,community_id=$6, \
             relay_deployment_id=$7,endpoint_service_identity=$8,tls_peer_spki_sha256=$9,relay_pubkey=$10, \
             deployment_digest=$11,approved_ips=$12,generation=$13,active=$14, \
             valid_until=$15,updated_by=$16,audit_entry_id=$17,updated_at=now() \
             WHERE tenant_id=$1 AND mapping_id=$2 AND generation=$3",
        )
        .bind(tenant_id)
        .bind(mapping_id)
        .bind(
            i64::try_from(expected_generation)
                .map_err(|_| UpstreamRelayMappingStoreError::Conflict)?,
        )
        .bind(&next.relay_url)
        .bind(&next.relay_host)
        .bind(&next.community_id)
        .bind(next.relay_deployment_id)
        .bind(&next.endpoint_service_identity)
        .bind(&next.tls_peer_spki_sha256)
        .bind(&next.relay_pubkey)
        .bind(&next.deployment_digest)
        .bind(ip_strings(&next.approved_ips))
        .bind(generation)
        .bind(next.active)
        .bind(next.valid_until)
        .bind(next.updated_by)
        .bind(next.audit_entry_id)
        .execute(&mut *tx)
        .await
        .map_err(map_database_error)?;
        if result.rows_affected() != 1 {
            return Err(UpstreamRelayMappingStoreError::Conflict);
        }
        if let Some(command) = command {
            record_operation(&mut tx, command).await?;
        }
        insert_audit(
            &mut tx,
            next,
            None,
            "mapping_updated",
            change_reference_id,
            changed,
            command,
        )
        .await?;
        tx.commit().await.map_err(map_database_error)
    }
}

async fn bind_workspace(
    tx: &mut Transaction<'_, Postgres>,
    record: &RelayMappingRecord,
    generation: i64,
    actor_id: Uuid,
    change_reference_id: Uuid,
    command: Option<&VerifiedMappingProvisioning>,
) -> Result<(), UpstreamRelayMappingStoreError> {
    let mapping_id = record.mapping_id;
    let tenant_id = record.tenant_id.0;
    let workspace_id = record.workspace_id.0;
    let audit_entry_id = Uuid::new_v4();
    let inserted = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO upstream_buzz_workspace_relay_bindings \
         (tenant_id,workspace_id,mapping_id,created_by,audit_entry_id) \
         VALUES ($1,$2,$3,$4,$5) ON CONFLICT DO NOTHING RETURNING workspace_id",
    )
    .bind(tenant_id)
    .bind(workspace_id)
    .bind(mapping_id)
    .bind(actor_id)
    .bind(audit_entry_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(map_database_error)?;
    if inserted.is_none() {
        let existing = sqlx::query_scalar::<_, Uuid>(
            "SELECT mapping_id FROM upstream_buzz_workspace_relay_bindings \
             WHERE tenant_id=$1 AND workspace_id=$2",
        )
        .bind(tenant_id)
        .bind(workspace_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(map_database_error)?;
        return if existing == Some(mapping_id) && command.is_none() {
            Ok(())
        } else {
            Err(UpstreamRelayMappingStoreError::Conflict)
        };
    }
    sqlx::query(
        "INSERT INTO upstream_buzz_relay_mapping_audit \
         (audit_entry_id,tenant_id,mapping_id,workspace_id,generation,action,actor_id,change_reference_id,operation_id,operator_key_id) \
         VALUES ($1,$2,$3,$4,$5,'workspace_bound',$6,$7,$8,$9)",
    )
    .bind(audit_entry_id)
    .bind(tenant_id)
    .bind(mapping_id)
    .bind(workspace_id)
    .bind(generation)
    .bind(actor_id)
    .bind(change_reference_id)
    .bind(command.map(VerifiedMappingProvisioning::operation_id))
    .bind(command.map(|command| command.operator_key_id()))
    .execute(&mut **tx)
    .await
    .map_err(map_database_error)?;
    Ok(())
}

async fn insert_audit(
    tx: &mut Transaction<'_, Postgres>,
    record: &RelayMappingRecord,
    workspace_id: Option<Uuid>,
    action: &str,
    change_reference_id: Uuid,
    changed_fields: Value,
    command: Option<&VerifiedMappingProvisioning>,
) -> Result<(), UpstreamRelayMappingStoreError> {
    sqlx::query(
        "INSERT INTO upstream_buzz_relay_mapping_audit \
         (audit_entry_id,tenant_id,mapping_id,workspace_id,generation,action,actor_id,change_reference_id,changed_fields,operation_id,operator_key_id) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
    )
    .bind(record.audit_entry_id)
    .bind(record.tenant_id.0)
    .bind(record.mapping_id)
    .bind(workspace_id)
    .bind(db_generation(record.generation)?)
    .bind(action)
    .bind(record.updated_by)
    .bind(change_reference_id)
    .bind(changed_fields)
    .bind(command.map(VerifiedMappingProvisioning::operation_id))
    .bind(command.map(|command| command.operator_key_id()))
    .execute(&mut **tx)
    .await
    .map_err(map_database_error)?;
    Ok(())
}

async fn check_replayed_operation(
    tx: &mut Transaction<'_, Postgres>,
    command: &VerifiedMappingProvisioning,
) -> Result<bool, UpstreamRelayMappingStoreError> {
    let existing = sqlx::query_as::<_, (Vec<u8>,)>(
        "SELECT statement_digest FROM upstream_buzz_mapping_operations \
         WHERE operation_id=$1 AND tenant_id=$2",
    )
    .bind(command.operation_id())
    .bind(command.record().tenant_id.0)
    .fetch_optional(&mut **tx)
    .await
    .map_err(map_database_error)?;
    match existing {
        Some((digest,)) if digest.as_slice() == command.statement_digest() => Ok(true),
        Some(_) => Err(UpstreamRelayMappingStoreError::Conflict),
        None => Ok(false),
    }
}

async fn lock_and_check_replay(
    tx: &mut Transaction<'_, Postgres>,
    command: &VerifiedMappingProvisioning,
) -> Result<bool, UpstreamRelayMappingStoreError> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(command.operation_id().to_string())
        .execute(&mut **tx)
        .await
        .map_err(map_database_error)?;
    check_replayed_operation(tx, command).await
}

async fn record_operation(
    tx: &mut Transaction<'_, Postgres>,
    command: &VerifiedMappingProvisioning,
) -> Result<(), UpstreamRelayMappingStoreError> {
    let record = command.record();
    let action = match command.action() {
        MappingProvisioningAction::Create => "create",
        MappingProvisioningAction::BindWorkspace => "bind_workspace",
        MappingProvisioningAction::Update => "update",
        MappingProvisioningAction::Renew => "renew",
        MappingProvisioningAction::Deactivate => "deactivate",
    };
    let inserted = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO upstream_buzz_mapping_operations \
         (operation_id,tenant_id,workspace_id,mapping_id,generation,action,operator_service_id,operator_key_id, \
          change_reference_id,statement_digest) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT DO NOTHING RETURNING operation_id",
    )
    .bind(command.operation_id())
    .bind(record.tenant_id.0)
    .bind(record.workspace_id.0)
    .bind(record.mapping_id)
    .bind(db_generation(record.generation)?)
    .bind(action)
    .bind(command.operator_service_id())
    .bind(command.operator_key_id())
    .bind(command.change_reference_id())
    .bind(command.statement_digest().as_slice())
    .fetch_optional(&mut **tx)
    .await
    .map_err(map_database_error)?;
    if inserted.is_none() {
        return Err(UpstreamRelayMappingStoreError::Conflict);
    }
    Ok(())
}

fn validate_write_command(
    command: &VerifiedMappingProvisioning,
    action: MappingProvisioningAction,
    record: &RelayMappingRecord,
    expected_generation: u64,
    change_reference_id: Uuid,
) -> Result<(), UpstreamRelayMappingStoreError> {
    if command.action() != action
        || command.record() != record
        || command.expected_generation() != expected_generation
        || command.change_reference_id() != change_reference_id
        || command.operator_service_id() != record.updated_by
    {
        return Err(UpstreamRelayMappingStoreError::Conflict);
    }
    Ok(())
}

#[derive(FromRow)]
struct RelayRow {
    mapping_id: Uuid,
    tenant_id: Uuid,
    relay_url: String,
    relay_host: String,
    community_id: String,
    relay_deployment_id: Uuid,
    endpoint_service_identity: String,
    tls_peer_spki_sha256: String,
    relay_pubkey: String,
    deployment_digest: String,
    approved_ips: Vec<String>,
    generation: i64,
    active: bool,
    valid_until: DateTime<Utc>,
    updated_by: Uuid,
    audit_entry_id: Uuid,
}

impl RelayRow {
    fn to_record(
        &self,
        workspace_id: Uuid,
    ) -> Result<RelayMappingRecord, UpstreamRelayMappingStoreError> {
        Ok(RelayMappingRecord {
            mapping_id: self.mapping_id,
            tenant_id: TenantId(self.tenant_id),
            workspace_id: WorkspaceId(workspace_id),
            community_id: self.community_id.clone(),
            relay_deployment_id: self.relay_deployment_id,
            endpoint_service_identity: self.endpoint_service_identity.clone(),
            tls_peer_spki_sha256: self.tls_peer_spki_sha256.clone(),
            relay_url: self.relay_url.clone(),
            relay_host: self.relay_host.clone(),
            relay_pubkey: self.relay_pubkey.clone(),
            deployment_digest: self.deployment_digest.clone(),
            approved_ips: self
                .approved_ips
                .iter()
                .map(|ip| {
                    IpAddr::from_str(ip).map_err(|_| UpstreamRelayMappingStoreError::InvalidRecord)
                })
                .collect::<Result<_, _>>()?,
            generation: u64::try_from(self.generation)
                .map_err(|_| UpstreamRelayMappingStoreError::InvalidRecord)?,
            active: self.active,
            valid_until: self.valid_until,
            updated_by: self.updated_by,
            audit_entry_id: self.audit_entry_id,
        })
    }
}

fn validate_record(
    record: &RelayMappingRecord,
    change_reference_id: Uuid,
    now: DateTime<Utc>,
) -> Result<(), UpstreamRelayMappingStoreError> {
    if change_reference_id.is_nil() {
        return Err(UpstreamRelayMappingStoreError::InvalidRecord);
    }
    db_generation(record.generation)?;
    TrustedRelayMapping::validate(record.clone(), now)?;
    Ok(())
}

fn database_precision_record(
    record: &RelayMappingRecord,
) -> Result<RelayMappingRecord, UpstreamRelayMappingStoreError> {
    let mut record = record.clone();
    let micros = record.valid_until.timestamp_subsec_micros();
    record.valid_until = record
        .valid_until
        .with_nanosecond(micros * 1_000)
        .ok_or(UpstreamRelayMappingStoreError::InvalidRecord)?;
    Ok(record)
}

fn db_generation(generation: u64) -> Result<i64, UpstreamRelayMappingStoreError> {
    i64::try_from(generation).map_err(|_| UpstreamRelayMappingStoreError::InvalidRecord)
}

fn ip_strings(ips: &[IpAddr]) -> Vec<String> {
    ips.iter().map(ToString::to_string).collect()
}

fn mapping_snapshot(record: &RelayMappingRecord) -> Value {
    json!({
        "relay_url": record.relay_url,
        "relay_host": record.relay_host,
        "community_id": record.community_id,
        "relay_deployment_id": record.relay_deployment_id,
        "endpoint_service_identity": record.endpoint_service_identity,
        "tls_peer_spki_sha256": record.tls_peer_spki_sha256,
        "relay_pubkey": record.relay_pubkey,
        "deployment_digest": record.deployment_digest,
        "approved_ips": ip_strings(&record.approved_ips),
        "active": record.active,
        "valid_until": record.valid_until,
    })
}

fn changed_snapshot(
    old: &RelayRow,
    next: &RelayMappingRecord,
) -> Result<Value, UpstreamRelayMappingStoreError> {
    let before = old.to_record(next.workspace_id.0)?;
    Ok(json!({
        "before": mapping_snapshot(&before),
        "after": mapping_snapshot(next),
    }))
}

async fn set_tenant(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
) -> Result<(), UpstreamRelayMappingStoreError> {
    sqlx::query("SELECT set_config('app.current_tenant_id', $1, true)")
        .bind(tenant_id.to_string())
        .execute(&mut **tx)
        .await
        .map_err(map_database_error)?;
    Ok(())
}

fn map_database_error(_: sqlx::Error) -> UpstreamRelayMappingStoreError {
    UpstreamRelayMappingStoreError::Database
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use sqlx::postgres::PgPoolOptions;
    use std::collections::HashMap;

    fn signed_create(
        tenant_id: Uuid,
        workspace_id: Uuid,
    ) -> (
        Arc<MappingProvisioningVerifier>,
        MappingProvisioningStatement,
        SigningKey,
    ) {
        let signing_key = SigningKey::from_bytes(&[21; 32]);
        let operator_service_id = Uuid::new_v4();
        let now = Utc::now();
        let now = now
            .with_nanosecond(now.timestamp_subsec_micros() * 1_000)
            .unwrap();
        let mut record = record(tenant_id, workspace_id);
        record.updated_by = operator_service_id;
        record.valid_until = now + chrono::Duration::days(1);
        let mut statement = MappingProvisioningStatement {
            operator_service_id,
            operator_key_id: "deployment-test-key-v1".into(),
            operation_id: Uuid::new_v4(),
            change_reference_id: Uuid::new_v4(),
            action: MappingProvisioningAction::Create,
            expected_generation: 0,
            issued_at: now,
            expires_at: now + chrono::Duration::minutes(2),
            record,
            signature: Vec::new(),
        };
        statement.signature = signing_key
            .sign(&statement.signing_bytes().unwrap())
            .to_bytes()
            .to_vec();
        let verifier = Arc::new(
            MappingProvisioningVerifier::new(
                operator_service_id,
                HashMap::from([(
                    statement.operator_key_id.clone(),
                    signing_key.verifying_key(),
                )]),
            )
            .unwrap(),
        );
        (verifier, statement, signing_key)
    }

    fn record(tenant_id: Uuid, workspace_id: Uuid) -> RelayMappingRecord {
        RelayMappingRecord {
            mapping_id: Uuid::new_v4(),
            tenant_id: TenantId(tenant_id),
            workspace_id: WorkspaceId(workspace_id),
            community_id: Uuid::new_v4().to_string(),
            relay_deployment_id: Uuid::new_v4(),
            endpoint_service_identity: format!(
                "cluster/test-ns/buzz-{tenant_id}#{}",
                Uuid::new_v4()
            ),
            tls_peer_spki_sha256: format!("sha256:{}", "b2".repeat(32)),
            relay_url: format!("wss://relay-{tenant_id}.example.test"),
            relay_host: format!("relay-{tenant_id}.example.test"),
            relay_pubkey: nostr::Keys::generate().public_key().to_hex(),
            deployment_digest: format!("sha256:{}", "a1".repeat(32)),
            approved_ips: vec!["8.8.8.8".parse().unwrap()],
            generation: 1,
            active: true,
            valid_until: Utc::now()
                + chrono::Duration::days(1)
                + chrono::Duration::nanoseconds(987),
            updated_by: Uuid::new_v4(),
            audit_entry_id: Uuid::new_v4(),
        }
    }

    #[tokio::test]
    async fn store_without_deployment_verifier_rejects_mapping_writes() {
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://test:test@127.0.0.1/unused")
            .unwrap();
        let (_, statement, _) = signed_create(Uuid::new_v4(), Uuid::new_v4());
        assert_eq!(
            UpstreamRelayMappingStore::new(pool)
                .apply(&statement)
                .await
                .unwrap_err(),
            UpstreamRelayMappingStoreError::InvalidRecord
        );
    }

    #[tokio::test]
    #[ignore = "requires a disposable PostgreSQL database"]
    async fn signed_mapping_write_is_atomic_idempotent_and_replay_protected() {
        let pool = PgPool::connect(
            &std::env::var("DATABASE_URL").expect("DATABASE_URL must be disposable PostgreSQL"),
        )
        .await
        .unwrap();
        sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
        let tenant_id = Uuid::new_v4();
        let tenant_b = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();
        let workspace_b = Uuid::new_v4();
        sqlx::query("INSERT INTO tenants (id,name) VALUES ($1,$2)")
            .bind(tenant_id)
            .bind(format!("signed relay mapping {tenant_id}"))
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO tenants (id,name) VALUES ($1,$2)")
            .bind(tenant_b)
            .bind(format!("signed relay mapping {tenant_b}"))
            .execute(&pool)
            .await
            .unwrap();
        let (verifier, statement, signing_key) = signed_create(tenant_id, workspace_id);
        let store = UpstreamRelayMappingStore::with_provisioning_verifier(pool.clone(), verifier);

        let (create_first, create_retry) =
            tokio::join!(store.apply(&statement), store.apply(&statement));
        create_first.unwrap();
        create_retry.unwrap();
        store.apply(&statement).await.unwrap();

        let mut update = statement.clone();
        update.operation_id = Uuid::new_v4();
        update.change_reference_id = Uuid::new_v4();
        update.action = MappingProvisioningAction::Update;
        update.expected_generation = 1;
        update.record.generation = 2;
        update.record.audit_entry_id = Uuid::new_v4();
        update.signature = signing_key
            .sign(&update.signing_bytes().unwrap())
            .to_bytes()
            .to_vec();
        let (update_first, update_retry) = tokio::join!(store.apply(&update), store.apply(&update));
        update_first.unwrap();
        update_retry.unwrap();

        let mut substituted_replay = statement.clone();
        substituted_replay.record.deployment_digest = format!("sha256:{}", "ee".repeat(32));
        substituted_replay.signature = signing_key
            .sign(&substituted_replay.signing_bytes().unwrap())
            .to_bytes()
            .to_vec();
        assert_eq!(
            store.apply(&substituted_replay).await.unwrap_err(),
            UpstreamRelayMappingStoreError::Conflict
        );

        let mut cross_tenant = statement.clone();
        cross_tenant.operation_id = Uuid::new_v4();
        cross_tenant.change_reference_id = Uuid::new_v4();
        cross_tenant.record = record(tenant_b, workspace_b);
        cross_tenant.record.updated_by = statement.operator_service_id;
        cross_tenant.record.valid_until = statement.issued_at + chrono::Duration::days(1);
        cross_tenant.record.community_id = statement.record.community_id.clone();
        cross_tenant.signature = signing_key
            .sign(&cross_tenant.signing_bytes().unwrap())
            .to_bytes()
            .to_vec();
        assert!(store.apply(&cross_tenant).await.is_err());
        assert!(matches!(
            store
                .load(TenantId(tenant_b), WorkspaceId(workspace_b))
                .await,
            Err(UpstreamRelayMappingStoreError::Policy(
                MappingError::MissingMapping
            ))
        ));

        let mut tx = pool.begin().await.unwrap();
        set_tenant(&mut tx, tenant_id).await.unwrap();
        let operation_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM upstream_buzz_mapping_operations WHERE operation_id=$1",
        )
        .bind(statement.operation_id)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        let audit_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM upstream_buzz_relay_mapping_audit WHERE operation_id=$1",
        )
        .bind(statement.operation_id)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(operation_count, 1);
        assert_eq!(audit_count, 2);
        tx.commit().await.unwrap();
    }

    #[tokio::test]
    #[ignore = "requires a disposable PostgreSQL database"]
    async fn mapping_store_is_tenant_scoped_audited_and_generation_fenced() {
        let pool = PgPool::connect(
            &std::env::var("DATABASE_URL").expect("DATABASE_URL must be disposable PostgreSQL"),
        )
        .await
        .unwrap();
        sqlx::migrate!("../../migrations").run(&pool).await.unwrap();

        let tenant_a = Uuid::new_v4();
        let tenant_b = Uuid::new_v4();
        sqlx::query("INSERT INTO tenants (id,name) VALUES ($1,$2),($3,$4)")
            .bind(tenant_a)
            .bind(format!("relay store test {tenant_a}"))
            .bind(tenant_b)
            .bind(format!("relay store test {tenant_b}"))
            .execute(&pool)
            .await
            .unwrap();

        let workspace_a = Uuid::new_v4();
        let workspace_a2 = Uuid::new_v4();
        let workspace_b = Uuid::new_v4();
        let store = UpstreamRelayMappingStore::new(pool.clone());
        let mut initial = record(tenant_a, workspace_a);
        let change_ref = Uuid::new_v4();
        let create_audit_id = initial.audit_entry_id;
        let mut unsafe_mapping = record(tenant_a, Uuid::new_v4());
        unsafe_mapping.approved_ips = vec!["10.0.0.1".parse().unwrap()];
        assert!(matches!(
            store.create(&unsafe_mapping, Uuid::new_v4()).await,
            Err(UpstreamRelayMappingStoreError::Policy(
                MappingError::UnsafeDnsTargets
            ))
        ));
        store.create(&initial, change_ref).await.unwrap();
        store.create(&initial, change_ref).await.unwrap();

        let mismatched_binding_workspace = Uuid::new_v4();
        let mismatched_binding_audit = Uuid::new_v4();
        let mut tx = pool.begin().await.unwrap();
        set_tenant(&mut tx, tenant_a).await.unwrap();
        sqlx::query(
            "INSERT INTO upstream_buzz_workspace_relay_bindings \
             (tenant_id,workspace_id,mapping_id,created_by,audit_entry_id) \
             VALUES ($1,$2,$3,$4,$5)",
        )
        .bind(tenant_a)
        .bind(mismatched_binding_workspace)
        .bind(initial.mapping_id)
        .bind(initial.updated_by)
        .bind(mismatched_binding_audit)
        .execute(&mut *tx)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO upstream_buzz_relay_mapping_audit \
             (audit_entry_id,tenant_id,mapping_id,generation,action,actor_id,change_reference_id) \
             VALUES ($1,$2,$3,2,'mapping_created',$4,$5)",
        )
        .bind(mismatched_binding_audit)
        .bind(tenant_a)
        .bind(initial.mapping_id)
        .bind(initial.updated_by)
        .bind(Uuid::new_v4())
        .execute(&mut *tx)
        .await
        .unwrap();
        let mismatched_binding_result = tx.commit().await;
        assert!(mismatched_binding_result
            .unwrap_err()
            .as_database_error()
            .unwrap()
            .message()
            .contains("workspace binding audit does not match binding"));

        store
            .bind_workspace(
                TenantId(tenant_a),
                WorkspaceId(workspace_a2),
                Uuid::new_v4(),
                Uuid::new_v4(),
            )
            .await
            .unwrap();

        let first = store
            .load(TenantId(tenant_a), WorkspaceId(workspace_a))
            .await
            .unwrap();
        let second = store
            .load(TenantId(tenant_a), WorkspaceId(workspace_a2))
            .await
            .unwrap();
        assert_eq!(first.scope().mapping_id, second.scope().mapping_id);
        assert_eq!(first.scope().generation, second.scope().generation);
        let mut tx = pool.begin().await.unwrap();
        set_tenant(&mut tx, tenant_a).await.unwrap();
        let unversioned_update =
            sqlx::query("UPDATE upstream_buzz_tenant_relays SET relay_url=$2 WHERE tenant_id=$1")
                .bind(tenant_a)
                .bind("wss://unversioned.example.test")
                .execute(&mut *tx)
                .await;
        assert!(unversioned_update
            .unwrap_err()
            .as_database_error()
            .unwrap()
            .message()
            .contains("generation and audit"));
        tx.rollback().await.unwrap();
        assert_eq!(
            store
                .load(TenantId(tenant_a), WorkspaceId(workspace_a))
                .await
                .unwrap()
                .relay_url(),
            first.relay_url()
        );
        assert!(matches!(
            store
                .load(TenantId(tenant_b), WorkspaceId(workspace_a))
                .await,
            Err(UpstreamRelayMappingStoreError::Policy(
                MappingError::MissingMapping
            ))
        ));

        let mismatched_create = record(tenant_b, workspace_b);
        let mut tx = pool.begin().await.unwrap();
        set_tenant(&mut tx, tenant_b).await.unwrap();
        sqlx::query(
            "INSERT INTO upstream_buzz_tenant_relays \
             (mapping_id,tenant_id,relay_url,relay_host,community_id,relay_deployment_id,endpoint_service_identity,tls_peer_spki_sha256,relay_pubkey, \
              deployment_digest,approved_ips,generation,active,valid_until,updated_by,audit_entry_id) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,1,true,$12,$13,$14)",
        )
        .bind(mismatched_create.mapping_id)
        .bind(tenant_b)
        .bind(&mismatched_create.relay_url)
        .bind(&mismatched_create.relay_host)
        .bind(&mismatched_create.community_id)
        .bind(mismatched_create.relay_deployment_id)
        .bind(&mismatched_create.endpoint_service_identity)
        .bind(&mismatched_create.tls_peer_spki_sha256)
        .bind(&mismatched_create.relay_pubkey)
        .bind(&mismatched_create.deployment_digest)
        .bind(ip_strings(&mismatched_create.approved_ips))
        .bind(mismatched_create.valid_until)
        .bind(mismatched_create.updated_by)
        .bind(mismatched_create.audit_entry_id)
        .execute(&mut *tx)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO upstream_buzz_relay_mapping_audit \
             (audit_entry_id,tenant_id,mapping_id,generation,action,actor_id,change_reference_id) \
             VALUES ($1,$2,$3,2,'workspace_bound',$4,$5)",
        )
        .bind(mismatched_create.audit_entry_id)
        .bind(tenant_b)
        .bind(mismatched_create.mapping_id)
        .bind(mismatched_create.updated_by)
        .bind(Uuid::new_v4())
        .execute(&mut *tx)
        .await
        .unwrap();
        let mismatched_create_result = tx.commit().await;
        assert!(mismatched_create_result
            .unwrap_err()
            .as_database_error()
            .unwrap()
            .message()
            .contains("creation audit does not match mapping"));

        let expired = record(tenant_b, workspace_b);
        store.create(&expired, Uuid::new_v4()).await.unwrap();
        let mut expired_revision = expired.clone();
        expired_revision.generation += 1;
        expired_revision.valid_until = Utc::now() - chrono::Duration::seconds(1);
        expired_revision.updated_by = Uuid::new_v4();
        expired_revision.audit_entry_id = Uuid::new_v4();
        let mut tx = pool.begin().await.unwrap();
        set_tenant(&mut tx, tenant_b).await.unwrap();
        sqlx::query(
            "UPDATE upstream_buzz_tenant_relays SET generation=$2,valid_until=$3,updated_by=$4, \
             audit_entry_id=$5,updated_at=now() WHERE tenant_id=$1",
        )
        .bind(tenant_b)
        .bind(db_generation(expired_revision.generation).unwrap())
        .bind(expired_revision.valid_until)
        .bind(expired_revision.updated_by)
        .bind(expired_revision.audit_entry_id)
        .execute(&mut *tx)
        .await
        .unwrap();
        insert_audit(
            &mut tx,
            &expired_revision,
            None,
            "mapping_updated",
            Uuid::new_v4(),
            json!({"valid_until": expired_revision.valid_until}),
            None,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        assert!(matches!(
            store
                .load(TenantId(tenant_b), WorkspaceId(workspace_b))
                .await,
            Err(UpstreamRelayMappingStoreError::Policy(
                MappingError::InactiveOrStale
            ))
        ));
        let expired_mapping = store
            .load_for_management(TenantId(tenant_b), WorkspaceId(workspace_b))
            .await
            .unwrap();
        assert!(expired_mapping.active);
        assert!(expired_mapping.valid_until < Utc::now());
        store
            .renew_expired(
                &expired_mapping,
                Utc::now() + chrono::Duration::days(1),
                expired_mapping.generation,
                Uuid::new_v4(),
                Uuid::new_v4(),
            )
            .await
            .unwrap();
        assert_eq!(
            store
                .load(TenantId(tenant_b), WorkspaceId(workspace_b))
                .await
                .unwrap()
                .scope()
                .generation,
            expired_mapping.generation + 1
        );

        let mut cross_tenant = record(tenant_b, workspace_b);
        cross_tenant.relay_url.clone_from(&initial.relay_url);
        cross_tenant.relay_host.clone_from(&initial.relay_host);
        cross_tenant.community_id.clone_from(&initial.community_id);
        assert!(store.create(&cross_tenant, Uuid::new_v4()).await.is_err());

        initial.generation = 2;
        initial.relay_url = format!("wss://next-{tenant_a}.example.test");
        initial.relay_host = format!("next-{tenant_a}.example.test");
        initial.audit_entry_id = Uuid::new_v4();
        store
            .update(&first, &initial, 1, Uuid::new_v4())
            .await
            .unwrap();
        assert_eq!(
            store
                .load(TenantId(tenant_a), WorkspaceId(workspace_a2))
                .await
                .unwrap()
                .scope()
                .generation,
            2
        );
        assert_eq!(
            store
                .update(&first, &initial, 1, Uuid::new_v4())
                .await
                .unwrap_err(),
            UpstreamRelayMappingStoreError::Conflict
        );
        let active = store
            .load(TenantId(tenant_a), WorkspaceId(workspace_a))
            .await
            .unwrap();
        store
            .deactivate(&active, 2, Uuid::new_v4(), Uuid::new_v4())
            .await
            .unwrap();
        assert!(matches!(
            store
                .load(TenantId(tenant_a), WorkspaceId(workspace_a))
                .await,
            Err(UpstreamRelayMappingStoreError::Policy(
                MappingError::InactiveOrStale
            ))
        ));

        let role = format!("upstream_relay_rls_{}", Uuid::new_v4().simple());
        sqlx::query(&format!(
            "CREATE ROLE {role} NOLOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOINHERIT NOBYPASSRLS"
        ))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(&format!(
            "GRANT SELECT ON upstream_buzz_tenant_relays, \
             upstream_buzz_workspace_relay_bindings, upstream_buzz_relay_mapping_audit TO {role}"
        ))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(&format!("GRANT {role} TO CURRENT_USER"))
            .execute(&pool)
            .await
            .unwrap();
        let mut tx = pool.begin().await.unwrap();
        sqlx::query(&format!("SET LOCAL ROLE {role}"))
            .execute(&mut *tx)
            .await
            .unwrap();
        for table in [
            "upstream_buzz_tenant_relays",
            "upstream_buzz_workspace_relay_bindings",
            "upstream_buzz_relay_mapping_audit",
        ] {
            let unscoped: i64 =
                sqlx::query_scalar(&format!("SELECT count(*) FROM {table} WHERE tenant_id=$1"))
                    .bind(tenant_a)
                    .fetch_one(&mut *tx)
                    .await
                    .unwrap();
            assert_eq!(unscoped, 0, "unscoped {table} rows are hidden");
        }
        sqlx::query("SELECT set_config('app.current_tenant_id',$1,true)")
            .bind(tenant_b.to_string())
            .execute(&mut *tx)
            .await
            .unwrap();
        for table in [
            "upstream_buzz_tenant_relays",
            "upstream_buzz_workspace_relay_bindings",
            "upstream_buzz_relay_mapping_audit",
        ] {
            let cross_tenant: i64 =
                sqlx::query_scalar(&format!("SELECT count(*) FROM {table} WHERE tenant_id=$1"))
                    .bind(tenant_a)
                    .fetch_one(&mut *tx)
                    .await
                    .unwrap();
            assert_eq!(cross_tenant, 0, "other-tenant {table} rows are hidden");
        }
        sqlx::query("SELECT set_config('app.current_tenant_id',$1,true)")
            .bind(tenant_a.to_string())
            .execute(&mut *tx)
            .await
            .unwrap();
        let scoped: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM upstream_buzz_tenant_relays WHERE tenant_id=$1",
        )
        .bind(tenant_a)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(scoped, 1);
        tx.rollback().await.unwrap();
        sqlx::query(&format!(
            "REVOKE SELECT ON upstream_buzz_tenant_relays, \
             upstream_buzz_workspace_relay_bindings, upstream_buzz_relay_mapping_audit FROM {role}"
        ))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(&format!("REVOKE {role} FROM CURRENT_USER"))
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(&format!("DROP ROLE {role}"))
            .execute(&pool)
            .await
            .unwrap();

        let mut tx = pool.begin().await.unwrap();
        set_tenant(&mut tx, tenant_a).await.unwrap();
        assert!(sqlx::query(
            "UPDATE upstream_buzz_relay_mapping_audit SET action='mapping_created' \
             WHERE tenant_id=$1 AND audit_entry_id=$2",
        )
        .bind(tenant_a)
        .bind(create_audit_id)
        .execute(&mut *tx)
        .await
        .is_err());
    }
}
