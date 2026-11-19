use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool, Row};
use uuid::Uuid;

#[derive(Debug, FromRow)]
pub struct DeviceRecord {
    pub id: Uuid,
    pub device_name: String,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}

/// Tenant- and owner-scoped persistence for user device tokens.
#[derive(Clone)]
pub struct DeviceRepository {
    pool: PgPool,
}

#[derive(Debug, Clone)]
pub enum DeviceApprovalLookup {
    UserCode(String),
    DeviceCode(String),
}

#[derive(Debug, Clone)]
pub enum DeviceApprovalBearer {
    Jwt {
        user_id: Uuid,
        issued_at: i64,
        credential_version: i64,
    },
    DeviceTokenHash(String),
    InvalidJwt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceApprovalOutcome {
    Approved,
    NotFound,
    AlreadyApproved,
    AccountDisabled,
    Unauthorized,
}

#[derive(Debug, FromRow)]
pub struct DevicePairPollRecord {
    pub user_id: Option<Uuid>,
    pub is_approved: Option<bool>,
    pub is_expired: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceTokenIssueOutcome {
    Issued,
    Expired,
}

impl DeviceRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create_pair_request(
        &self,
        device_code: &str,
        user_code: &str,
        ttl_seconds: f64,
    ) -> sqlx::Result<()> {
        sqlx::query(
            "INSERT INTO device_pair_requests (id, device_code, user_code, expires_at)
             VALUES (gen_random_uuid(), $1, $2, NOW() + INTERVAL '1 second' * $3)",
        )
        .bind(device_code)
        .bind(user_code)
        .bind(ttl_seconds)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn approve_pair_request(
        &self,
        user_id: Uuid,
        tenant_id: Uuid,
        session_token_hash: Option<&str>,
        bearer: Option<&DeviceApprovalBearer>,
        lookup: &DeviceApprovalLookup,
    ) -> sqlx::Result<DeviceApprovalOutcome> {
        let mut tx = self.pool.begin().await?;
        let user_is_active: Option<bool> = sqlx::query_scalar(
            "SELECT disabled_at IS NULL AND tenant_id = $2 FROM users WHERE id = $1 FOR UPDATE",
        )
        .bind(user_id)
        .bind(tenant_id)
        .fetch_optional(&mut *tx)
        .await?;
        if user_is_active != Some(true) {
            return Ok(DeviceApprovalOutcome::AccountDisabled);
        }

        let session_is_current = if let Some(token_hash) = session_token_hash {
            Some(
                sqlx::query_scalar::<_, bool>(
                    "SELECT EXISTS(SELECT 1 FROM user_sessions WHERE user_id=$1 AND tenant_id=$2 AND session_token_hash=$3 AND expires_at > clock_timestamp())",
                )
                .bind(user_id)
                .bind(tenant_id)
                .bind(token_hash)
                .fetch_one(&mut *tx)
                .await?,
            )
        } else {
            None
        };
        let credential_is_current = if session_is_current == Some(true) {
            true
        } else {
            match bearer {
                Some(DeviceApprovalBearer::Jwt {
                    user_id: token_user_id,
                    issued_at,
                    credential_version,
                }) if *token_user_id == user_id => {
                    sqlx::query_scalar::<_, bool>(
                        "SELECT credential_version=$3 AND (credentials_revoked_at IS NULL OR to_timestamp($2) > credentials_revoked_at) FROM users WHERE id=$1 AND tenant_id=$4",
                    )
                    .bind(user_id)
                    .bind(issued_at)
                    .bind(credential_version)
                    .bind(tenant_id)
                    .fetch_optional(&mut *tx)
                    .await?
                    .unwrap_or(false)
                }
                Some(DeviceApprovalBearer::DeviceTokenHash(token_hash)) => {
                    sqlx::query_scalar::<_, bool>(
                        "SELECT EXISTS(SELECT 1 FROM device_tokens WHERE user_id=$1 AND token_hash=$2 AND revoked_at IS NULL)",
                    )
                    .bind(user_id)
                    .bind(token_hash)
                    .fetch_one(&mut *tx)
                    .await?
                }
                Some(DeviceApprovalBearer::InvalidJwt) => false,
                _ => false,
            }
        };
        if !credential_is_current {
            return Ok(DeviceApprovalOutcome::Unauthorized);
        }

        let (query, value) = match lookup {
            DeviceApprovalLookup::UserCode(value) => (
                "SELECT id, approved_at IS NOT NULL AS is_approved, expires_at < clock_timestamp() AS is_expired FROM device_pair_requests WHERE UPPER(user_code) = UPPER($1) FOR UPDATE",
                value,
            ),
            DeviceApprovalLookup::DeviceCode(value) => (
                "SELECT id, approved_at IS NOT NULL AS is_approved, expires_at < clock_timestamp() AS is_expired FROM device_pair_requests WHERE device_code = $1 FOR UPDATE",
                value,
            ),
        };
        let row = sqlx::query(query)
            .bind(value)
            .fetch_optional(&mut *tx)
            .await?;
        let Some(row) = row else {
            return Ok(DeviceApprovalOutcome::NotFound);
        };
        let id: Uuid = row.try_get("id")?;
        let is_approved: bool = row.try_get("is_approved").unwrap_or(false);
        let is_expired: bool = row.try_get("is_expired").unwrap_or(true);
        if is_expired {
            return Ok(DeviceApprovalOutcome::NotFound);
        }
        if is_approved {
            return Ok(DeviceApprovalOutcome::AlreadyApproved);
        }

        let update = sqlx::query(
            "UPDATE device_pair_requests SET user_id=$1, approved_at=NOW() WHERE id=$2 AND approved_at IS NULL AND expires_at > clock_timestamp()",
        )
        .bind(user_id)
        .bind(id)
        .execute(&mut *tx)
        .await?;
        if update.rows_affected() != 1 {
            return Ok(DeviceApprovalOutcome::AlreadyApproved);
        }
        tx.commit().await?;
        Ok(DeviceApprovalOutcome::Approved)
    }

    pub async fn pair_request_for_poll(
        &self,
        device_code: &str,
    ) -> sqlx::Result<Option<DevicePairPollRecord>> {
        sqlx::query_as(
            "SELECT user_id, approved_at IS NOT NULL AS is_approved, expires_at < NOW() AS is_expired
             FROM device_pair_requests WHERE device_code = $1",
        )
        .bind(device_code)
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn delete_pair_request(&self, device_code: &str) -> sqlx::Result<()> {
        sqlx::query("DELETE FROM device_pair_requests WHERE device_code = $1")
            .bind(device_code)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn issue_token_for_approved_pair(
        &self,
        device_code: &str,
        user_id: Uuid,
        token_hash: &str,
        device_name: &str,
    ) -> sqlx::Result<DeviceTokenIssueOutcome> {
        let mut tx = self.pool.begin().await?;
        let user_is_active: Option<bool> =
            sqlx::query_scalar("SELECT disabled_at IS NULL FROM users WHERE id = $1 FOR UPDATE")
                .bind(user_id)
                .fetch_optional(&mut *tx)
                .await?;
        if user_is_active != Some(true) {
            sqlx::query("DELETE FROM device_pair_requests WHERE device_code = $1")
                .bind(device_code)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
            return Ok(DeviceTokenIssueOutcome::Expired);
        }

        let pair_request_id: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM device_pair_requests WHERE device_code=$1 AND user_id=$2 AND approved_at IS NOT NULL AND expires_at > clock_timestamp() FOR UPDATE",
        )
        .bind(device_code)
        .bind(user_id)
        .fetch_optional(&mut *tx)
        .await?;
        if pair_request_id.is_none() {
            tx.commit().await?;
            return Ok(DeviceTokenIssueOutcome::Expired);
        }

        sqlx::query(
            "INSERT INTO device_tokens (id, user_id, token_hash, device_name)
             VALUES (gen_random_uuid(), $1, $2, $3)",
        )
        .bind(user_id)
        .bind(token_hash)
        .bind(device_name)
        .execute(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM device_pair_requests WHERE device_code = $1")
            .bind(device_code)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(DeviceTokenIssueOutcome::Issued)
    }

    pub async fn list_for_user(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
    ) -> anyhow::Result<Vec<DeviceRecord>> {
        Ok(sqlx::query_as(
            "SELECT d.id, d.device_name, d.created_at, d.last_used_at
             FROM device_tokens d
             JOIN users u ON u.id = d.user_id
             WHERE u.tenant_id = $1 AND d.user_id = $2 AND d.revoked_at IS NULL
             ORDER BY d.created_at DESC",
        )
        .bind(tenant_id)
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn revoke_for_user(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
        device_id: Uuid,
    ) -> anyhow::Result<bool> {
        let result = sqlx::query(
            "UPDATE device_tokens d
             SET revoked_at = NOW()
             WHERE d.id = $1 AND d.user_id = $2 AND d.revoked_at IS NULL
               AND EXISTS (
                   SELECT 1 FROM users u WHERE u.id = d.user_id AND u.tenant_id = $3
               )",
        )
        .bind(device_id)
        .bind(user_id)
        .bind(tenant_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }
}
