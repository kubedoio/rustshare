//! Read-only queries used by the installation-admin user endpoints.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(sqlx::FromRow)]
pub struct AdminUserRow {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub display_name: String,
    pub is_admin: bool,
    pub storage_quota: i64,
    pub disabled_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct AdminUserDetailRow {
    pub user: AdminUserRow,
    pub storage_used_bytes: i64,
}

pub struct AdminUserQueries {
    pool: PgPool,
}

impl AdminUserQueries {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn list(
        &self,
        search: Option<&str>,
        status: Option<&str>,
        page: i64,
        per_page: i64,
    ) -> Result<(Vec<AdminUserRow>, i64), sqlx::Error> {
        let offset = (page - 1) * per_page;

        let cols = "id, username, email, display_name, is_admin, storage_quota, disabled_at, created_at, updated_at";
        let order = "ORDER BY created_at DESC";

        let (rows, total): (Vec<AdminUserRow>, i64) = match (search, status) {
            (None, None) | (None, Some("all")) => {
                let total = sqlx::query_scalar!("SELECT COUNT(*) FROM users")
                    .fetch_one(&self.pool)
                    .await?
                    .unwrap_or(0);
                let rows = sqlx::query_as::<_, AdminUserRow>(&format!(
                    "SELECT {cols} FROM users {order} LIMIT $1 OFFSET $2"
                ))
                .bind(per_page)
                .bind(offset)
                .fetch_all(&self.pool)
                .await?;
                (rows, total)
            }
            (None, Some("active")) => {
                let total =
                    sqlx::query_scalar!("SELECT COUNT(*) FROM users WHERE disabled_at IS NULL")
                        .fetch_one(&self.pool)
                        .await?
                        .unwrap_or(0);
                let rows = sqlx::query_as::<_, AdminUserRow>(&format!(
                    "SELECT {cols} FROM users WHERE disabled_at IS NULL {order} LIMIT $1 OFFSET $2"
                ))
                .bind(per_page)
                .bind(offset)
                .fetch_all(&self.pool)
                .await?;
                (rows, total)
            }
            (None, Some("disabled")) => {
                let total =
                    sqlx::query_scalar!("SELECT COUNT(*) FROM users WHERE disabled_at IS NOT NULL")
                        .fetch_one(&self.pool)
                        .await?
                        .unwrap_or(0);
                let rows = sqlx::query_as::<_, AdminUserRow>(&format!(
                "SELECT {cols} FROM users WHERE disabled_at IS NOT NULL {order} LIMIT $1 OFFSET $2"
            ))
                .bind(per_page)
                .bind(offset)
                .fetch_all(&self.pool)
                .await?;
                (rows, total)
            }
            (Some(search), None) | (Some(search), Some("all")) => {
                let pattern = format!("%{}%", search);
                let total = sqlx::query_scalar!(
                    "SELECT COUNT(*) FROM users WHERE username ILIKE $1 OR email ILIKE $1",
                    &pattern
                )
                .fetch_one(&self.pool)
                .await?
                .unwrap_or(0);
                let rows = sqlx::query_as::<_, AdminUserRow>(&format!(
                    "SELECT {cols} FROM users WHERE username ILIKE $1 OR email ILIKE $1 {order} LIMIT $2 OFFSET $3"
                ))
                .bind(&pattern)
                .bind(per_page)
                .bind(offset)
                .fetch_all(&self.pool)
                .await
                ?;
                (rows, total)
            }
            (Some(search), Some("active")) => {
                let pattern = format!("%{}%", search);
                let total = sqlx::query_scalar!(
                    "SELECT COUNT(*) FROM users WHERE (username ILIKE $1 OR email ILIKE $1) AND disabled_at IS NULL",
                    &pattern
                )
                .fetch_one(&self.pool)
                .await
                ?
                .unwrap_or(0);
                let rows = sqlx::query_as::<_, AdminUserRow>(&format!(
                    "SELECT {cols} FROM users WHERE (username ILIKE $1 OR email ILIKE $1) AND disabled_at IS NULL {order} LIMIT $2 OFFSET $3"
                ))
                .bind(&pattern)
                .bind(per_page)
                .bind(offset)
                .fetch_all(&self.pool)
                .await
                ?;
                (rows, total)
            }
            (Some(search), Some("disabled")) => {
                let pattern = format!("%{}%", search);
                let total = sqlx::query_scalar!(
                    "SELECT COUNT(*) FROM users WHERE (username ILIKE $1 OR email ILIKE $1) AND disabled_at IS NOT NULL",
                    &pattern
                )
                .fetch_one(&self.pool)
                .await
                ?
                .unwrap_or(0);
                let rows = sqlx::query_as::<_, AdminUserRow>(&format!(
                    "SELECT {cols} FROM users WHERE (username ILIKE $1 OR email ILIKE $1) AND disabled_at IS NOT NULL {order} LIMIT $2 OFFSET $3"
                ))
                .bind(&pattern)
                .bind(per_page)
                .bind(offset)
                .fetch_all(&self.pool)
                .await
                ?;
                (rows, total)
            }
            _ => {
                // Fallback: all users
                let total = sqlx::query_scalar!("SELECT COUNT(*) FROM users")
                    .fetch_one(&self.pool)
                    .await?
                    .unwrap_or(0);
                let rows = sqlx::query_as::<_, AdminUserRow>(&format!(
                    "SELECT {cols} FROM users {order} LIMIT $1 OFFSET $2"
                ))
                .bind(per_page)
                .bind(offset)
                .fetch_all(&self.pool)
                .await?;
                (rows, total)
            }
        };

        Ok((rows, total))
    }

    pub async fn get(&self, user_id: Uuid) -> Result<Option<AdminUserDetailRow>, sqlx::Error> {
        // User IDs are globally unique; installation admins may inspect users
        // across organizations, matching the existing endpoint contract.
        let user = sqlx::query_as::<_, AdminUserRow>(
            "SELECT id, username, email, display_name, is_admin, storage_quota, disabled_at, created_at, updated_at FROM users WHERE id = $1",
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;
        let Some(user) = user else {
            return Ok(None);
        };

        let storage_used_bytes = sqlx::query_scalar::<_, i64>(
            "SELECT COALESCE(SUM(size), 0)::BIGINT FROM (
                 SELECT DISTINCT ON (fv.storage_key) fv.size
                 FROM file_versions fv
                 JOIN files f ON f.id = fv.file_id
                 WHERE f.owner_id = $1
             ) sub",
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(Some(AdminUserDetailRow {
            user,
            storage_used_bytes,
        }))
    }
}
