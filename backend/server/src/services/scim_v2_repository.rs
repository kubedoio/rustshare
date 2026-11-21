//! SQLx-backed implementation of the tenant-scoped SCIM v2 repository contract.

use crate::services::organization_administrators::{
    lock_tenant_admin_grants, removal_would_leave_no_enabled_admin, revoke_for_scim_lifecycle,
};
use crate::user_disable::{set_user_disabled_at, DisableUserTarget};
use chrono::{DateTime, Utc};
use rustshare_core::services::{
    ScimV2Error, ScimV2Group, ScimV2GroupRecord, ScimV2Repository, ScimV2User, ScimV2UserRecord,
};
use sqlx::PgPool;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Repository implementation
// ---------------------------------------------------------------------------

/// SQLx-based implementation of SCIM v2 repository.
pub struct ScimV2RepositoryImpl {
    pool: PgPool,
    correlation_id: Option<String>,
}

impl ScimV2RepositoryImpl {
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

impl ScimV2Repository for ScimV2RepositoryImpl {
    async fn list_users(
        &self,
        tenant_id: Uuid,
        filters: &[rustshare_core::services::ScimFilter],
        start_index: Option<i64>,
        count: Option<i64>,
    ) -> Result<(Vec<rustshare_core::services::ScimV2UserRecord>, i64), sqlx::Error> {
        let base_select = "SELECT id, external_id, username, display_name, email, disabled_at, name, surname, created_at, updated_at
             FROM users WHERE tenant_id = $1";
        let base_count = "SELECT COUNT(*) FROM users WHERE tenant_id = $1";

        let mut conditions: Vec<String> = Vec::new();
        let mut binds: Vec<String> = Vec::new();

        // Apply filters with parameterised binds. The tenant scope is $1, so
        // filter binds start at $2.
        for filter in filters {
            match filter.attribute.as_str() {
                "userName" => {
                    let idx = binds.len() + 2;
                    conditions.push(format!(" AND LOWER(username) LIKE ${}", idx));
                    binds.push(format!("%{}%", filter.value.to_lowercase()));
                }
                "externalId" => {
                    let idx = binds.len() + 2;
                    conditions.push(format!(" AND external_id = ${}", idx));
                    binds.push(filter.value.clone());
                }
                "active" => {
                    let is_active = filter.value.to_lowercase() == "true";
                    if is_active {
                        conditions.push(" AND disabled_at IS NULL".to_string());
                    } else {
                        conditions.push(" AND disabled_at IS NOT NULL".to_string());
                    }
                }
                _ => {}
            }
        }

        let where_clause = conditions.join("");
        let limit = count.unwrap_or(100);
        let offset = start_index.map(|s| s - 1).unwrap_or(0).max(0);

        // count query (no LIMIT/OFFSET)
        let count_query = format!("{}{}", base_count, where_clause);
        let mut count_q = sqlx::query_scalar::<_, i64>(&count_query).bind(tenant_id);
        for val in &binds {
            count_q = count_q.bind(val);
        }
        let total: i64 = count_q.fetch_one(&self.pool).await?;

        // select query with pagination binds
        let pag_idx_1 = binds.len() + 2;
        let pag_idx_2 = binds.len() + 3;
        let query = format!(
            "{} {} ORDER BY username LIMIT ${} OFFSET ${}",
            base_select, where_clause, pag_idx_1, pag_idx_2
        );
        let mut q = sqlx::query_as::<_, rustshare_core::services::ScimV2UserRecord>(&query);
        q = q.bind(tenant_id);
        for val in &binds {
            q = q.bind(val);
        }
        q = q.bind(limit).bind(offset);
        let users = q.fetch_all(&self.pool).await?;

        Ok((users, total))
    }

    async fn get_user(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<rustshare_core::services::ScimV2UserRecord>, sqlx::Error> {
        sqlx::query_as!(
            ScimV2UserRecord,
            r#"
            SELECT id, external_id, username, display_name, email, disabled_at, name, surname, created_at, updated_at
            FROM users WHERE id = $1 AND tenant_id = $2
            "#,
            id,
            tenant_id
        )
        .fetch_optional(&self.pool)
        .await
    }

    async fn get_user_by_external_id(
        &self,
        tenant_id: Uuid,
        external_id: &str,
    ) -> Result<Option<rustshare_core::services::ScimV2UserRecord>, sqlx::Error> {
        sqlx::query_as!(
            ScimV2UserRecord,
            r#"
            SELECT id, external_id, username, display_name, email, disabled_at, name, surname, created_at, updated_at
            FROM users WHERE external_id = $1 AND tenant_id = $2
            "#,
            external_id,
            tenant_id
        )
        .fetch_optional(&self.pool)
        .await
    }

    async fn create_user(
        &self,
        user: &ScimV2User,
        tenant_id: Uuid,
        storage_quota: i64,
    ) -> Result<Uuid, sqlx::Error> {
        let id = Uuid::new_v4();

        // Extract email from SCIM emails
        let email = user
            .emails
            .as_ref()
            .and_then(|emails| emails.first().map(|e| e.value.clone()))
            .unwrap_or_else(|| user.user_name.clone());

        // Extract display name
        let display_name = user
            .display_name
            .clone()
            .or_else(|| {
                user.name.as_ref().map(|n| {
                    let given = n.given_name.as_deref().unwrap_or("");
                    let family = n.family_name.as_deref().unwrap_or("");
                    format!("{} {}", given, family).trim().to_string()
                })
            })
            .unwrap_or_else(|| user.user_name.clone());

        let (name, surname) = user.name.as_ref().map_or((None, None), |n| {
            (n.given_name.clone(), n.family_name.clone())
        });

        // Generate temporary password hash
        let password_hash = generate_temporary_password_hash();

        sqlx::query!(
            r#"
            INSERT INTO users (
                id, username, display_name, password_hash, email, is_admin,
                storage_quota, theme, created_at, updated_at, disabled_at,
                name, surname, email_sharing_enabled, tenant_id, external_id
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NOW(), NOW(), $9, $10, $11, $12, $13, $14)
            "#,
            id,
            user.user_name,
            display_name,
            password_hash,
            email,
            false,
            storage_quota,
            "system",
            if user.active {
                None::<DateTime<Utc>>
            } else {
                Some(Utc::now())
            },
            name,
            surname,
            true,
            tenant_id,
            user.external_id
        )
        .execute(&self.pool)
        .await?;

        Ok(id)
    }

    async fn update_user(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        user: &ScimV2User,
    ) -> Result<(), ScimV2Error> {
        let email = user
            .emails
            .as_ref()
            .and_then(|emails| emails.first().map(|e| e.value.clone()))
            .unwrap_or_else(|| user.user_name.clone());

        let display_name = user
            .display_name
            .clone()
            .unwrap_or_else(|| user.user_name.clone());

        let (name, surname) = user.name.as_ref().map_or((None, None), |n| {
            (n.given_name.clone(), n.family_name.clone())
        });

        let mut tx = self.pool.begin().await?;
        lock_tenant_admin_grants(&mut tx, tenant_id).await?;
        let current_disabled_at = sqlx::query_scalar::<_, Option<DateTime<Utc>>>(
            "SELECT disabled_at FROM users WHERE id = $1 AND tenant_id = $2 FOR UPDATE",
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&mut *tx)
        .await?;
        if !user.active
            && matches!(current_disabled_at.as_ref(), Some(None))
            && removal_would_leave_no_enabled_admin(&mut tx, tenant_id, id).await?
        {
            return Err(ScimV2Error::Conflict(
                "Cannot deactivate the last enabled organization administrator".into(),
            ));
        }
        let was_disabled = current_disabled_at.flatten().is_some();
        sqlx::query!(
            r#"
            UPDATE users
            SET username = $2,
                display_name = $3,
                email = $4,
                disabled_at = $5,
                name = $6,
                surname = $7,
                external_id = $8,
                updated_at = NOW()
            WHERE id = $1 AND tenant_id = $9
            "#,
            id,
            user.user_name,
            display_name,
            email,
            if user.active {
                None::<DateTime<Utc>>
            } else {
                Some(Utc::now())
            },
            name,
            surname,
            user.external_id,
            tenant_id
        )
        .execute(&mut *tx)
        .await?;

        if !user.active || was_disabled {
            revoke_for_scim_lifecycle(&mut tx, tenant_id, id, self.correlation_id.as_deref())
                .await?;
        }
        tx.commit().await?;

        Ok(())
    }

    async fn patch_user(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        operations: &[rustshare_core::services::ScimPatchOperation],
    ) -> Result<(), ScimV2Error> {
        let mut tx = self.pool.begin().await?;
        lock_tenant_admin_grants(&mut tx, tenant_id).await?;
        let mut was_disabled = sqlx::query_scalar::<_, Option<DateTime<Utc>>>(
            "SELECT disabled_at FROM users WHERE id = $1 AND tenant_id = $2 FOR UPDATE",
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&mut *tx)
        .await?
        .flatten()
        .is_some();
        for op in operations {
            if op.op.to_lowercase().as_str() == "replace" {
                if let Some(ref path) = op.path {
                    if path.as_str() == "active" {
                        if let Some(ref value) = op.value {
                            let active = value.as_bool().unwrap_or(true);
                            if !active
                                && !was_disabled
                                && removal_would_leave_no_enabled_admin(&mut tx, tenant_id, id)
                                    .await?
                            {
                                return Err(ScimV2Error::Conflict(
                                    "Cannot deactivate the last enabled organization administrator"
                                        .into(),
                                ));
                            }
                            // AR-01 disable parity: shared disable/enable
                            // write (see `crate::user_disable` for which
                            // entry points share it); the
                            // `users_disable_revokes_credentials` trigger
                            // performs credential revocation.
                            set_user_disabled_at(
                                &mut *tx,
                                DisableUserTarget::Id(id),
                                Some(tenant_id),
                                if active { None } else { Some(Utc::now()) },
                            )
                            .await?;
                            if !active || was_disabled {
                                revoke_for_scim_lifecycle(
                                    &mut tx,
                                    tenant_id,
                                    id,
                                    self.correlation_id.as_deref(),
                                )
                                .await?;
                            }
                            was_disabled = !active;
                        }
                    }
                } else if let Some(ref value) = op.value {
                    // Handle value object with multiple attributes
                    if let Some(active) = value.get("active").and_then(|v| v.as_bool()) {
                        if !active
                            && !was_disabled
                            && removal_would_leave_no_enabled_admin(&mut tx, tenant_id, id).await?
                        {
                            return Err(ScimV2Error::Conflict(
                                "Cannot deactivate the last enabled organization administrator"
                                    .into(),
                            ));
                        }
                        set_user_disabled_at(
                            &mut *tx,
                            DisableUserTarget::Id(id),
                            Some(tenant_id),
                            if active { None } else { Some(Utc::now()) },
                        )
                        .await?;
                        if !active || was_disabled {
                            revoke_for_scim_lifecycle(
                                &mut tx,
                                tenant_id,
                                id,
                                self.correlation_id.as_deref(),
                            )
                            .await?;
                        }
                        was_disabled = !active;
                    }
                }
            }
        }
        tx.commit().await?;
        Ok(())
    }

    async fn delete_user(&self, tenant_id: Uuid, id: Uuid) -> Result<(), ScimV2Error> {
        let mut tx = self.pool.begin().await?;
        lock_tenant_admin_grants(&mut tx, tenant_id).await?;
        let current_disabled_at = sqlx::query_scalar::<_, Option<DateTime<Utc>>>(
            "SELECT disabled_at FROM users WHERE id = $1 AND tenant_id = $2 FOR UPDATE",
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&mut *tx)
        .await?;
        if matches!(current_disabled_at.as_ref(), Some(None))
            && removal_would_leave_no_enabled_admin(&mut tx, tenant_id, id).await?
        {
            return Err(ScimV2Error::Conflict(
                "Cannot deactivate the last enabled organization administrator".into(),
            ));
        }
        sqlx::query(
            "UPDATE users SET disabled_at = NOW(), updated_at = NOW() \
             WHERE id = $1 AND tenant_id = $2 AND disabled_at IS NULL",
        )
        .bind(id)
        .bind(tenant_id)
        .execute(&mut *tx)
        .await?;
        revoke_for_scim_lifecycle(&mut tx, tenant_id, id, self.correlation_id.as_deref()).await?;
        tx.commit().await?;
        Ok(())
    }

    async fn list_groups(
        &self,
        tenant_id: Uuid,
        filters: &[rustshare_core::services::ScimFilter],
        start_index: Option<i64>,
        count: Option<i64>,
    ) -> Result<(Vec<rustshare_core::services::ScimV2GroupRecord>, i64), sqlx::Error> {
        let base_select = "SELECT id, external_id, name, description, created_at, updated_at
             FROM user_groups WHERE tenant_id = $1";
        let base_count = "SELECT COUNT(*) FROM user_groups WHERE tenant_id = $1";

        let mut conditions: Vec<String> = Vec::new();
        let mut binds: Vec<String> = Vec::new();

        // Apply filters with parameterised binds. The tenant scope is $1, so
        // filter binds start at $2.
        for filter in filters {
            match filter.attribute.as_str() {
                "displayName" => {
                    let idx = binds.len() + 2;
                    conditions.push(format!(" AND LOWER(name) LIKE ${}", idx));
                    binds.push(format!("%{}%", filter.value.to_lowercase()));
                }
                "externalId" => {
                    let idx = binds.len() + 2;
                    conditions.push(format!(" AND external_id = ${}", idx));
                    binds.push(filter.value.clone());
                }
                _ => {}
            }
        }

        let where_clause = conditions.join("");
        let limit = count.unwrap_or(100);
        let offset = start_index.map(|s| s - 1).unwrap_or(0).max(0);

        // count query (no LIMIT/OFFSET)
        let count_query = format!("{}{}", base_count, where_clause);
        let mut count_q = sqlx::query_scalar::<_, i64>(&count_query).bind(tenant_id);
        for val in &binds {
            count_q = count_q.bind(val);
        }
        let total: i64 = count_q.fetch_one(&self.pool).await?;

        // select query with pagination binds
        let pag_idx_1 = binds.len() + 2;
        let pag_idx_2 = binds.len() + 3;
        let query = format!(
            "{} {} ORDER BY name LIMIT ${} OFFSET ${}",
            base_select, where_clause, pag_idx_1, pag_idx_2
        );
        let mut q = sqlx::query_as::<_, rustshare_core::services::ScimV2GroupRecord>(&query);
        q = q.bind(tenant_id);
        for val in &binds {
            q = q.bind(val);
        }
        q = q.bind(limit).bind(offset);
        let groups = q.fetch_all(&self.pool).await?;

        Ok((groups, total))
    }

    async fn get_group(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<rustshare_core::services::ScimV2GroupRecord>, sqlx::Error> {
        sqlx::query_as!(
            ScimV2GroupRecord,
            r#"
            SELECT id, external_id, name, description, created_at, updated_at
            FROM user_groups WHERE id = $1 AND tenant_id = $2
            "#,
            id,
            tenant_id
        )
        .fetch_optional(&self.pool)
        .await
    }

    async fn get_group_by_external_id(
        &self,
        tenant_id: Uuid,
        external_id: &str,
    ) -> Result<Option<rustshare_core::services::ScimV2GroupRecord>, sqlx::Error> {
        sqlx::query_as!(
            ScimV2GroupRecord,
            r#"
            SELECT id, external_id, name, description, created_at, updated_at
            FROM user_groups WHERE external_id = $1 AND tenant_id = $2
            "#,
            external_id,
            tenant_id
        )
        .fetch_optional(&self.pool)
        .await
    }

    async fn create_group(
        &self,
        tenant_id: Uuid,
        group: &ScimV2Group,
    ) -> Result<Uuid, sqlx::Error> {
        let id = Uuid::new_v4();

        sqlx::query!(
            "INSERT INTO user_groups (id, name, external_id, tenant_id, created_at, updated_at)
             VALUES ($1, $2, $3, $4, NOW(), NOW())",
            id,
            group.display_name,
            group.external_id,
            tenant_id
        )
        .execute(&self.pool)
        .await?;

        Ok(id)
    }

    async fn update_group(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        group: &ScimV2Group,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE user_groups SET name = $2, external_id = $3, updated_at = NOW() \
             WHERE id = $1 AND tenant_id = $4",
            id,
            group.display_name,
            group.external_id,
            tenant_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn patch_group(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        operations: &[rustshare_core::services::ScimPatchOperation],
    ) -> Result<(), sqlx::Error> {
        for op in operations {
            match op.op.to_lowercase().as_str() {
                "add" | "replace" => {
                    if let Some(ref path) = op.path {
                        if path == "members" {
                            if let Some(ref value) = op.value {
                                // Handle array of members
                                if let Some(members) = value.as_array() {
                                    for member in members {
                                        if let Some(user_id_str) =
                                            member.get("value").and_then(|v| v.as_str())
                                        {
                                            if let Ok(user_id) = Uuid::parse_str(user_id_str) {
                                                if let Err(e) = self
                                                    .add_group_member(tenant_id, id, user_id)
                                                    .await
                                                {
                                                    tracing::warn!(group_id = %id, user_id = %user_id, error = %e, "failed to add group member");
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    } else if let Some(ref value) = op.value {
                        if let Some(members) = value.get("members").and_then(|v| v.as_array()) {
                            for member in members {
                                if let Some(user_id_str) =
                                    member.get("value").and_then(|v| v.as_str())
                                {
                                    if let Ok(user_id) = Uuid::parse_str(user_id_str) {
                                        if let Err(e) =
                                            self.add_group_member(tenant_id, id, user_id).await
                                        {
                                            tracing::warn!(group_id = %id, user_id = %user_id, error = %e, "failed to add group member");
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                "remove" => {
                    if let Some(ref path) = op.path {
                        // Parse path like "members[value eq \"user-id\"]"
                        if path.starts_with("members[value eq ") {
                            if let Some(user_id_str) = path
                                .split("value eq \"")
                                .nth(1)
                                .and_then(|s| s.split("\"").next())
                            {
                                if let Ok(user_id) = Uuid::parse_str(user_id_str) {
                                    if let Err(e) =
                                        self.remove_group_member(tenant_id, id, user_id).await
                                    {
                                        tracing::warn!(group_id = %id, user_id = %user_id, error = %e, "failed to remove group member");
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    async fn delete_group(&self, tenant_id: Uuid, id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "DELETE FROM user_groups WHERE id = $1 AND tenant_id = $2",
            id,
            tenant_id
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_group_members(
        &self,
        tenant_id: Uuid,
        group_id: Uuid,
    ) -> Result<Vec<(Uuid, String)>, sqlx::Error> {
        // Both legs of the membership must belong to the organization: the
        // member user and the group (defense in depth).
        let rows = sqlx::query!(
            r#"
            SELECT u.id, u.display_name
            FROM users u
            JOIN group_members gm ON u.id = gm.user_id
            WHERE gm.group_id = $1 AND u.tenant_id = $2
              AND EXISTS (
                  SELECT 1 FROM user_groups g
                  WHERE g.id = gm.group_id AND g.tenant_id = $2
              )
            "#,
            group_id,
            tenant_id
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| (r.id, r.display_name)).collect())
    }

    async fn get_user_groups(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
    ) -> Result<Vec<(Uuid, String)>, sqlx::Error> {
        // Both legs of the membership must belong to the organization: the
        // group and the user (defense in depth).
        let rows = sqlx::query!(
            r#"
            SELECT g.id, g.name
            FROM user_groups g
            JOIN group_members gm ON g.id = gm.group_id
            WHERE gm.user_id = $1 AND g.tenant_id = $2
              AND EXISTS (
                  SELECT 1 FROM users u
                  WHERE u.id = gm.user_id AND u.tenant_id = $2
              )
            "#,
            user_id,
            tenant_id
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| (r.id, r.name)).collect())
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
            INSERT INTO group_members (group_id, user_id, added_at)
            SELECT $1, $2, NOW()
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

    async fn find_user_id_by_external_id(
        &self,
        tenant_id: Uuid,
        external_id: &str,
    ) -> Result<Option<Uuid>, sqlx::Error> {
        let row = sqlx::query_scalar!(
            "SELECT id FROM users WHERE external_id = $1 AND tenant_id = $2",
            external_id,
            tenant_id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row)
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

/// Generate a temporary password hash.
fn generate_temporary_password_hash() -> String {
    use rand::RngExt;
    let mut rng = rand::rng();
    let random_bytes: [u8; 32] = rng.random();
    format!("$scim_temp${}", base64_encode(&random_bytes))
}

/// Simple base64 encoding.
fn base64_encode(input: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();

    for chunk in input.chunks(3) {
        let b = match chunk.len() {
            1 => [chunk[0], 0, 0],
            2 => [chunk[0], chunk[1], 0],
            3 => [chunk[0], chunk[1], chunk[2]],
            _ => unreachable!(),
        };

        let idx1 = (b[0] >> 2) as usize;
        let idx2 = (((b[0] & 0b11) << 4) | (b[1] >> 4)) as usize;
        let idx3 = (((b[1] & 0b1111) << 2) | (b[2] >> 6)) as usize;
        let idx4 = (b[2] & 0b111111) as usize;

        result.push(ALPHABET[idx1] as char);
        result.push(ALPHABET[idx2] as char);

        if chunk.len() > 1 {
            result.push(ALPHABET[idx3] as char);
        } else {
            result.push('=');
        }

        if chunk.len() > 2 {
            result.push(ALPHABET[idx4] as char);
        } else {
            result.push('=');
        }
    }

    result
}
