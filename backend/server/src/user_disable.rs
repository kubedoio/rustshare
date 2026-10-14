//! Shared user disable/enable write for the admin and SCIM entry points (AR-01).
//!
//! Credential revocation on disable — deleting the principal's
//! `user_sessions` rows and revoking their `device_tokens` — is already
//! guaranteed for every writer at the database level by the
//! `users_disable_revokes_credentials` trigger (migration
//! `20261003090000_revoke_credentials_on_user_disable`, ADR-0040 D8). This
//! module therefore performs no per-caller session or device-token cleanup.
//!
//! The write is shared by the entry points whose *only* effect is toggling
//! `disabled_at`: the admin disable/enable handlers, SCIM v1
//! `set_user_disabled`, and the SCIM v2 `active` patch. The SCIM v1/v2
//! full-attribute updates (PUT) and the SCIM v2 delete (a soft-delete with an
//! additional `disabled_at IS NULL` guard) still fold `disabled_at` into
//! their own statements; the trigger covers them identically. This is a
//! coverage/parity and handler-consistency change, not a new revocation
//! mechanism.

use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Which principal the disable/enable write targets.
pub(crate) enum DisableUserTarget<'a> {
    /// Looked up by row id. The installation-administrator entry point leaves
    /// `tenant_id` as `None` (all organizations); organization-scoped entry
    /// points pin a single organization via `tenant_id`.
    Id(Uuid),
    /// SCIM v1 external-id binding. Always scoped to exactly one
    /// organization.
    ExternalId {
        external_id: &'a str,
        tenant_id: Uuid,
    },
}

/// Set or clear `users.disabled_at` through one shared statement.
///
/// `disabled_at = Some(_)` disables the principal, `None` re-enables them.
/// `tenant_id` scopes an [`DisableUserTarget::Id`] write to a single
/// organization; pass `None` for the installation-administrator scope.
///
/// Returns the number of rows written so callers can fail closed on a
/// principal that does not exist in scope.
pub(crate) async fn set_user_disabled_at<'e, E>(
    executor: E,
    target: DisableUserTarget<'_>,
    tenant_id: Option<Uuid>,
    disabled_at: Option<DateTime<Utc>>,
) -> Result<u64, sqlx::Error>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let result = match target {
        DisableUserTarget::Id(id) => match tenant_id {
            None => {
                sqlx::query("UPDATE users SET disabled_at = $1, updated_at = NOW() WHERE id = $2")
                    .bind(disabled_at)
                    .bind(id)
                    .execute(executor)
                    .await?
            }
            Some(tenant) => {
                sqlx::query(
                    "UPDATE users SET disabled_at = $1, updated_at = NOW() \
                     WHERE id = $2 AND tenant_id = $3",
                )
                .bind(disabled_at)
                .bind(id)
                .bind(tenant)
                .execute(executor)
                .await?
            }
        },
        DisableUserTarget::ExternalId {
            external_id,
            tenant_id,
        } => {
            sqlx::query(
                "UPDATE users SET disabled_at = $1, updated_at = NOW() \
                 WHERE external_id = $2 AND tenant_id = $3",
            )
            .bind(disabled_at)
            .bind(external_id)
            .bind(tenant_id)
            .execute(executor)
            .await?
        }
    };
    Ok(result.rows_affected())
}
