//! SQLx persistence adapter for the core SCIM repository contract.

use crate::services::note_service::{
    capture_group_share_targets, dispatch_share_target_acl_refresh,
};
use crate::services::organization_administrators::{
    lock_tenant_admin_grants, removal_would_leave_no_enabled_admin, revoke_for_scim_lifecycle,
};
use crate::user_disable::{set_user_disabled_at, DisableUserTarget};
use rustshare_core::domain::User;
use rustshare_core::services::{GroupRecord, ScimError, ScimRepository};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

/// Delete one tenant-scoped SCIM group and refresh affected share ACLs after commit.
pub async fn delete_group_and_refresh(
    pool: &PgPool,
    note_service: Arc<crate::services::note_service::NoteService>,
    tenant_id: Uuid,
    external_id: &str,
) -> Result<bool, (&'static str, sqlx::Error)> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| ("begin transaction", error))?;
    let group_id = sqlx::query_scalar!(
        "SELECT id FROM user_groups WHERE external_id = $1 AND tenant_id = $2 FOR UPDATE",
        external_id,
        tenant_id
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| ("lock group", error))?;
    let Some(group_id) = group_id else {
        return Ok(false);
    };
    let targets = capture_group_share_targets(&mut tx, group_id)
        .await
        .map_err(|error| ("capture group share targets", error))?;
    sqlx::query!(
        "DELETE FROM user_groups WHERE external_id = $1 AND tenant_id = $2",
        external_id,
        tenant_id
    )
    .execute(&mut *tx)
    .await
    .map_err(|error| ("delete group", error))?;
    tx.commit()
        .await
        .map_err(|error| ("commit group deletion", error))?;

    dispatch_share_target_acl_refresh(note_service, targets);
    Ok(true)
}

// ---------------------------------------------------------------------------
// SCIM Repository Implementation
// ---------------------------------------------------------------------------

/// SQLx-based implementation of SCIM repository.
pub struct ScimRepositoryImpl {
    pool: PgPool,
    correlation_id: Option<String>,
}

impl ScimRepositoryImpl {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            correlation_id: None,
        }
    }

    pub fn with_correlation_id(pool: PgPool, correlation_id: Option<String>) -> Self {
        Self {
            pool,
            correlation_id,
        }
    }
}

impl ScimRepository for ScimRepositoryImpl {
    async fn find_user_by_external_id(
        &self,
        tenant_id: Uuid,
        external_id: &str,
    ) -> Result<Option<User>, sqlx::Error> {
        let row = sqlx::query_as!(
            User,
            r#"
            SELECT id, username, display_name, password_hash, email, is_admin,
                   storage_quota, theme as "theme: _", created_at, updated_at, disabled_at,
                   name, surname, avatar_path, email_sharing_enabled, trash_retention_days, tenant_id,
                   dashboard_config as "dashboard_config: _"
            FROM users
            WHERE external_id = $1 AND tenant_id = $2
            "#,
            external_id,
            tenant_id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row)
    }

    async fn create_user(&self, user: &User, external_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            INSERT INTO users (
                id, username, display_name, password_hash, email, is_admin,
                storage_quota, theme, created_at, updated_at, disabled_at,
                name, surname, avatar_path, email_sharing_enabled, trash_retention_days, tenant_id, external_id
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18)
            "#,
            user.id,
            user.username,
            user.display_name,
            user.password_hash,
            user.email,
            user.is_admin,
            user.storage_quota,
            user.theme.to_string(),
            user.created_at,
            user.updated_at,
            user.disabled_at,
            user.name,
            user.surname,
            user.avatar_path,
            user.email_sharing_enabled,
            user.trash_retention_days,
            user.tenant_id,
            external_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn update_user(
        &self,
        tenant_id: Uuid,
        user_id: uuid::Uuid,
        display_name: &str,
        email: &str,
        name: Option<&str>,
        surname: Option<&str>,
        disabled_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<(), ScimError> {
        let mut tx = self.pool.begin().await?;
        lock_tenant_admin_grants(&mut tx, tenant_id).await?;
        let was_disabled = sqlx::query_scalar::<_, Option<chrono::DateTime<chrono::Utc>>>(
            "SELECT disabled_at FROM users WHERE id = $1 AND tenant_id = $2 FOR UPDATE",
        )
        .bind(user_id)
        .bind(tenant_id)
        .fetch_optional(&mut *tx)
        .await?;
        if disabled_at.is_some()
            && matches!(was_disabled.as_ref(), Some(None))
            && removal_would_leave_no_enabled_admin(&mut tx, tenant_id, user_id).await?
        {
            return Err(ScimError::Conflict(
                "Cannot disable the last enabled organization administrator".into(),
            ));
        }
        sqlx::query!(
            r#"
            UPDATE users
            SET display_name = $2,
                email = $3,
                name = $4,
                surname = $5,
                disabled_at = $6,
                updated_at = NOW()
            WHERE id = $1 AND tenant_id = $7
            "#,
            user_id,
            display_name,
            email,
            name,
            surname,
            disabled_at,
            tenant_id
        )
        .execute(&mut *tx)
        .await?;

        if was_disabled.flatten().is_some() || disabled_at.is_some() {
            revoke_for_scim_lifecycle(&mut tx, tenant_id, user_id, self.correlation_id.as_deref())
                .await?;
        }
        tx.commit().await?;

        Ok(())
    }

    async fn set_user_disabled(
        &self,
        tenant_id: Uuid,
        external_id: &str,
        disabled: bool,
    ) -> Result<(), ScimError> {
        let mut tx = self.pool.begin().await?;
        lock_tenant_admin_grants(&mut tx, tenant_id).await?;
        let target = sqlx::query_as::<_, (Uuid, Option<chrono::DateTime<chrono::Utc>>)>(
            "SELECT id, disabled_at FROM users WHERE external_id = $1 AND tenant_id = $2 FOR UPDATE",
        )
        .bind(external_id)
        .bind(tenant_id)
        .fetch_optional(&mut *tx)
        .await?;
        if disabled {
            if let Some((user_id, None)) = target.as_ref() {
                if removal_would_leave_no_enabled_admin(&mut tx, tenant_id, *user_id).await? {
                    return Err(ScimError::Conflict(
                        "Cannot disable the last enabled organization administrator".into(),
                    ));
                }
            }
        }
        set_user_disabled_at(
            &mut *tx,
            DisableUserTarget::ExternalId {
                external_id,
                tenant_id,
            },
            None,
            if disabled {
                Some(chrono::Utc::now())
            } else {
                None
            },
        )
        .await?;
        if let Some((user_id, was_disabled_at)) = target {
            if disabled || was_disabled_at.is_some() {
                revoke_for_scim_lifecycle(
                    &mut tx,
                    tenant_id,
                    user_id,
                    self.correlation_id.as_deref(),
                )
                .await?;
            }
        }
        tx.commit().await?;

        Ok(())
    }

    async fn find_group_by_external_id(
        &self,
        tenant_id: Uuid,
        external_id: &str,
    ) -> Result<Option<GroupRecord>, sqlx::Error> {
        let row = sqlx::query_as!(
            GroupRecordRow,
            r#"
            SELECT id, external_id, name
            FROM user_groups
            WHERE external_id = $1 AND tenant_id = $2
            "#,
            external_id,
            tenant_id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| GroupRecord {
            id: r.id,
            external_id: r.external_id.unwrap_or_default(),
            name: r.name,
        }))
    }

    async fn create_group(
        &self,
        tenant_id: Uuid,
        external_id: &str,
        display_name: &str,
    ) -> Result<uuid::Uuid, sqlx::Error> {
        let id = uuid::Uuid::new_v4();

        sqlx::query!(
            r#"
            INSERT INTO user_groups (id, name, external_id, created_by, tenant_id)
            VALUES ($1, $2, $3, NULL, $4)
            "#,
            id,
            display_name,
            external_id,
            tenant_id
        )
        .execute(&self.pool)
        .await?;

        Ok(id)
    }

    async fn update_group(
        &self,
        tenant_id: Uuid,
        external_id: &str,
        display_name: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE user_groups
            SET name = $2,
                updated_at = NOW()
            WHERE external_id = $1 AND tenant_id = $3
            "#,
            external_id,
            display_name,
            tenant_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn delete_group(&self, tenant_id: Uuid, external_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "DELETE FROM user_groups WHERE external_id = $1 AND tenant_id = $2",
            external_id,
            tenant_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn find_user_id_by_external_id(
        &self,
        tenant_id: Uuid,
        external_id: &str,
    ) -> Result<Option<uuid::Uuid>, sqlx::Error> {
        let id: Option<uuid::Uuid> = sqlx::query_scalar!(
            "SELECT id FROM users WHERE external_id = $1 AND tenant_id = $2",
            external_id,
            tenant_id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(id)
    }

    async fn get_group_members(
        &self,
        tenant_id: Uuid,
        group_id: uuid::Uuid,
    ) -> Result<Vec<uuid::Uuid>, sqlx::Error> {
        // Both legs of the membership must belong to the organization: the
        // group (defense in depth) and the member user.
        let members: Vec<uuid::Uuid> = sqlx::query_scalar!(
            "SELECT gm.user_id FROM group_members gm WHERE gm.group_id = $1 \
                AND EXISTS (SELECT 1 FROM user_groups g WHERE g.id = gm.group_id AND g.tenant_id = $2) \
                AND EXISTS (SELECT 1 FROM users u WHERE u.id = gm.user_id AND u.tenant_id = $2)",
            group_id,
            tenant_id
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(members)
    }

    async fn add_group_member(
        &self,
        tenant_id: Uuid,
        group_id: Uuid,
        user_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        // The membership is written only when both the group and the user
        // belong to the configured organization.
        sqlx::query!(
            r#"
            INSERT INTO group_members (group_id, user_id, added_by)
            SELECT $1, $2, NULL
            WHERE EXISTS (
                SELECT 1
                FROM user_groups g
                JOIN users u ON u.id = $2
                WHERE g.id = $1 AND g.tenant_id = $3 AND u.tenant_id = $3
            )
            ON CONFLICT (group_id, user_id) DO NOTHING
            "#,
            group_id,
            user_id,
            tenant_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn remove_group_member(
        &self,
        tenant_id: Uuid,
        group_id: Uuid,
        user_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "DELETE FROM group_members gm WHERE gm.group_id = $1 AND gm.user_id = $2 \
                AND EXISTS (SELECT 1 FROM user_groups g WHERE g.id = gm.group_id AND g.tenant_id = $3) \
                AND EXISTS (SELECT 1 FROM users u WHERE u.id = gm.user_id AND u.tenant_id = $3)",
            group_id,
            user_id,
            tenant_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn clear_group_members(
        &self,
        tenant_id: Uuid,
        group_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        // Per-row user leg: in-scope memberships are cleared, cross-tenant
        // member rows survive.
        sqlx::query!(
            "DELETE FROM group_members gm WHERE gm.group_id = $1 \
                AND EXISTS (SELECT 1 FROM user_groups g WHERE g.id = gm.group_id AND g.tenant_id = $2) \
                AND EXISTS (SELECT 1 FROM users u WHERE u.id = gm.user_id AND u.tenant_id = $2)",
            group_id,
            tenant_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}

#[derive(sqlx::FromRow)]
struct GroupRecordRow {
    id: uuid::Uuid,
    external_id: Option<String>,
    name: String,
}
