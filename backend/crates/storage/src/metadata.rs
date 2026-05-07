//! Metadata store for querying projection tables.
//!
//! NOTE: Currently uses runtime queries (`sqlx::query()`) instead of compile-time
//! queries (`sqlx::query!()`) because offline mode setup requires a running database.
//! This will be migrated to compile-time queries after Docker Compose is set up in Task 11.

use anyhow::Result;
use chrono::{DateTime, Utc};
use rustshare_core::domain::{
    File, FileVersion, Folder, OidcLoginState, ReplicationJob, ReplicationJobStatus,
    ReplicationState, ReplicationTarget, Share, SharePermissions, User, UserSession,
};
use sqlx::{PgPool, Row};
use uuid::Uuid;

/// Metadata store for querying projection tables
pub struct MetadataStore {
    pool: PgPool,
}

impl MetadataStore {
    /// Get access to the underlying database pool
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

#[derive(Debug, Clone)]
pub struct OwnedPublicShare {
    pub share: Share,
    pub resource_id: Uuid,
    pub resource_type: String,
    pub resource_name: String,
}

/// Folder with share information
#[derive(Debug, Clone)]
pub struct FolderWithShares {
    pub id: Uuid,
    pub name: String,
    pub path: String,
    pub parent_folder_id: Option<Uuid>,
    pub owner_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub tenant_id: Uuid,
    pub starred_at: Option<DateTime<Utc>>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub ancestor_ids: Option<Vec<Uuid>>,
    pub is_shared: bool,
    pub share_count: i64,
    pub share_expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct PublicShareAccessLogEntry {
    pub accessed_at: DateTime<Utc>,
    pub action: String,
    pub success: bool,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub actor_type: Option<String>,
    pub actor_label: Option<String>,
    pub share_session_id: Option<Uuid>,
    pub share_session_subject: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ReplicationAttemptRecord<'a> {
    pub job_id: Uuid,
    pub target_id: Uuid,
    pub attempt_number: i32,
    pub status: &'a str,
    pub error_message: Option<&'a str>,
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct ShareAccessLogEntry {
    pub share_id: Uuid,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub action: String,
    pub success: bool,
    pub actor_type: Option<String>,
    pub actor_label: Option<String>,
    pub share_session_id: Option<Uuid>,
    pub share_session_subject: Option<String>,
}

#[derive(Debug, Clone)]
pub struct UserSecurityEventRecord<'a> {
    pub user_id: Uuid,
    pub event_type: &'a str,
    pub description: &'a str,
    pub ip_address: Option<&'a str>,
    pub user_agent: Option<&'a str>,
    pub session_id: Option<Uuid>,
}

#[derive(Debug, Clone)]
pub struct UserSecurityEvent {
    pub id: Uuid,
    pub event_type: String,
    pub description: String,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub session_id: Option<Uuid>,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct SecurityConfig {
    pub login_protection_enabled: bool,
    pub max_login_attempts: i32,
    pub login_block_duration_minutes: i32,
    pub updated_at: DateTime<Utc>,
}

impl MetadataStore {
    fn permission_to_db_value(permission: SharePermissions) -> &'static str {
        match permission {
            SharePermissions::View => "View",
            SharePermissions::Edit => "Edit",
            SharePermissions::Admin => "Admin",
        }
    }

    fn permission_from_db_value(value: &str) -> SharePermissions {
        match value {
            "Edit" | "edit" => SharePermissions::Edit,
            "Admin" | "admin" => SharePermissions::Admin,
            _ => SharePermissions::View,
        }
    }

    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    fn parse_replication_state(value: &str) -> Result<ReplicationState> {
        value.parse().map_err(|error: String| {
            anyhow::anyhow!("invalid replication state `{value}`: {error}")
        })
    }

    fn parse_replication_job_status(value: &str) -> Result<ReplicationJobStatus> {
        value.parse().map_err(|error: String| {
            anyhow::anyhow!("invalid replication job status `{value}`: {error}")
        })
    }

    /// Create a new user in the projection table
    pub async fn create_user(&self, user: &User) -> Result<()> {
        // TODO: Switch to sqlx::query!() after Docker Compose setup (Task 11)
        sqlx::query(
            r#"
            INSERT INTO users (id, username, email, password_hash, display_name, is_admin, storage_quota, theme, created_at, updated_at, name, surname, avatar_path, email_sharing_enabled, trash_retention_days, tenant_id, dashboard_config)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)
            "#,
        )
        .bind(user.id)
        .bind(&user.username)
        .bind(&user.email)
        .bind(&user.password_hash)
        .bind(&user.display_name)
        .bind(user.is_admin)
        .bind(user.storage_quota)
        .bind(user.theme.to_string())
        .bind(user.created_at)
        .bind(user.updated_at)
        .bind(user.name.as_deref())
        .bind(user.surname.as_deref())
        .bind(user.avatar_path.as_deref())
        .bind(user.email_sharing_enabled)
        .bind(user.trash_retention_days)
        .bind(user.tenant_id)
        .bind(&user.dashboard_config)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Create a new opaque browser session.
    pub async fn create_user_session(&self, session: &UserSession) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO user_sessions (
                id,
                user_id,
                session_token_hash,
                expires_at,
                created_at,
                last_seen_at,
                user_agent,
                ip_address,
                tenant_id
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
        )
        .bind(session.id)
        .bind(session.user_id)
        .bind(&session.session_token_hash)
        .bind(session.expires_at)
        .bind(session.created_at)
        .bind(session.last_seen_at)
        .bind(&session.user_agent)
        .bind(&session.ip_address)
        .bind(session.tenant_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Find a browser session by hashed token.
    pub async fn find_user_session_by_token_hash(
        &self,
        token_hash: &str,
    ) -> Result<Option<UserSession>> {
        let row = sqlx::query(
            r#"
            SELECT
                id,
                user_id,
                session_token_hash,
                expires_at,
                created_at,
                last_seen_at,
                user_agent,
                ip_address,
                tenant_id
            FROM user_sessions
            WHERE session_token_hash = $1
            "#,
        )
        .bind(token_hash)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            Ok(Some(UserSession {
                id: row.try_get("id")?,
                user_id: row.try_get("user_id")?,
                session_token_hash: row.try_get("session_token_hash")?,
                expires_at: row.try_get("expires_at")?,
                created_at: row.try_get("created_at")?,
                last_seen_at: row.try_get("last_seen_at")?,
                user_agent: row.try_get("user_agent")?,
                ip_address: row.try_get("ip_address")?,
                tenant_id: row.try_get("tenant_id")?,
            }))
        } else {
            Ok(None)
        }
    }

    /// Touch session activity for active browser sessions.
    pub async fn touch_user_session(&self, session_id: Uuid) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE user_sessions
            SET last_seen_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(session_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Delete a browser session by hashed token.
    pub async fn delete_user_session_by_token_hash(&self, token_hash: &str) -> Result<()> {
        sqlx::query("DELETE FROM user_sessions WHERE session_token_hash = $1")
            .bind(token_hash)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// List active browser sessions for a user.
    pub async fn list_user_sessions(&self, user_id: Uuid) -> Result<Vec<UserSession>> {
        let rows = sqlx::query(
            r#"
            SELECT
                id,
                user_id,
                session_token_hash,
                expires_at,
                created_at,
                last_seen_at,
                user_agent,
                ip_address,
                tenant_id
            FROM user_sessions
            WHERE user_id = $1
            ORDER BY last_seen_at DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|row| {
                Ok(UserSession {
                    id: row.try_get("id")?,
                    user_id: row.try_get("user_id")?,
                    session_token_hash: row.try_get("session_token_hash")?,
                    expires_at: row.try_get("expires_at")?,
                    created_at: row.try_get("created_at")?,
                    last_seen_at: row.try_get("last_seen_at")?,
                    user_agent: row.try_get("user_agent")?,
                    ip_address: row.try_get("ip_address")?,
                    tenant_id: row.try_get("tenant_id")?,
                })
            })
            .collect()
    }

    /// Delete a browser session by session id, scoped to the owning user.
    pub async fn delete_user_session_by_id(&self, user_id: Uuid, session_id: Uuid) -> Result<()> {
        sqlx::query("DELETE FROM user_sessions WHERE user_id = $1 AND id = $2")
            .bind(user_id)
            .bind(session_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Create a security event entry for a user account.
    pub async fn create_user_security_event(
        &self,
        event: UserSecurityEventRecord<'_>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO user_security_events (
                id,
                user_id,
                event_type,
                description,
                ip_address,
                user_agent,
                session_id,
                occurred_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, NOW())
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(event.user_id)
        .bind(event.event_type)
        .bind(event.description)
        .bind(event.ip_address)
        .bind(event.user_agent)
        .bind(event.session_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// List recent security events for a user account.
    pub async fn list_user_security_events(
        &self,
        user_id: Uuid,
        limit: i64,
    ) -> Result<Vec<UserSecurityEvent>> {
        let rows = sqlx::query(
            r#"
            SELECT
                id,
                event_type,
                description,
                ip_address,
                user_agent,
                session_id,
                occurred_at
            FROM user_security_events
            WHERE user_id = $1
            ORDER BY occurred_at DESC
            LIMIT $2
            "#,
        )
        .bind(user_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|row| {
                Ok(UserSecurityEvent {
                    id: row.try_get("id")?,
                    event_type: row.try_get("event_type")?,
                    description: row.try_get("description")?,
                    ip_address: row.try_get("ip_address")?,
                    user_agent: row.try_get("user_agent")?,
                    session_id: row.try_get("session_id")?,
                    occurred_at: row.try_get("occurred_at")?,
                })
            })
            .collect()
    }

    /// Persist a short-lived OIDC login state.
    pub async fn create_oidc_login_state(&self, login_state: &OidcLoginState) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO oidc_login_states (
                state,
                pkce_verifier,
                nonce,
                redirect_to,
                expires_at,
                created_at
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(&login_state.state)
        .bind(&login_state.pkce_verifier)
        .bind(&login_state.nonce)
        .bind(&login_state.redirect_to)
        .bind(login_state.expires_at)
        .bind(login_state.created_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Load an OIDC login state by the opaque state token.
    pub async fn find_oidc_login_state(&self, state: &str) -> Result<Option<OidcLoginState>> {
        let row = sqlx::query(
            r#"
            SELECT
                state,
                pkce_verifier,
                nonce,
                redirect_to,
                expires_at,
                created_at
            FROM oidc_login_states
            WHERE state = $1
            "#,
        )
        .bind(state)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            Ok(Some(OidcLoginState {
                state: row.try_get("state")?,
                pkce_verifier: row.try_get("pkce_verifier")?,
                nonce: row.try_get("nonce")?,
                redirect_to: row.try_get("redirect_to")?,
                expires_at: row.try_get("expires_at")?,
                created_at: row.try_get("created_at")?,
            }))
        } else {
            Ok(None)
        }
    }

    /// Delete a consumed or expired OIDC login state.
    pub async fn delete_oidc_login_state(&self, state: &str) -> Result<()> {
        sqlx::query("DELETE FROM oidc_login_states WHERE state = $1")
            .bind(state)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Find user by email
    pub async fn find_user_by_email(&self, email: &str) -> Result<Option<User>> {
        // TODO: Switch to sqlx::query!() after Docker Compose setup (Task 11)
        let row = sqlx::query(
            r#"SELECT id, username, email, password_hash, display_name, is_admin, storage_quota, theme, created_at, updated_at, disabled_at, name, surname, avatar_path, email_sharing_enabled, trash_retention_days, tenant_id, dashboard_config FROM users WHERE email = $1"#,
        )
        .bind(email)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            let user = User {
                id: row.try_get("id")?,
                username: row.try_get("username")?,
                email: row.try_get("email")?,
                password_hash: row.try_get("password_hash")?,
                display_name: row.try_get("display_name")?,
                is_admin: row.try_get("is_admin")?,
                storage_quota: row.try_get("storage_quota")?,
                theme: row
                    .try_get::<String, _>("theme")?
                    .parse()
                    .unwrap_or_default(),
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
                disabled_at: row.try_get("disabled_at")?,
                name: row.try_get("name")?,
                surname: row.try_get("surname")?,
                avatar_path: row.try_get("avatar_path")?,
                email_sharing_enabled: row.try_get("email_sharing_enabled")?,
                trash_retention_days: row.try_get("trash_retention_days")?,
                tenant_id: row.try_get("tenant_id")?,
                dashboard_config: row.try_get("dashboard_config")?,
            };
            Ok(Some(user))
        } else {
            Ok(None)
        }
    }

    /// Find user by username.
    pub async fn find_user_by_username(&self, username: &str) -> Result<Option<User>> {
        let row = sqlx::query(
            r#"SELECT id, username, email, password_hash, display_name, is_admin, storage_quota, theme, created_at, updated_at, disabled_at, name, surname, avatar_path, email_sharing_enabled, trash_retention_days, tenant_id, dashboard_config FROM users WHERE username = $1"#,
        )
        .bind(username)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            Ok(Some(User {
                id: row.try_get("id")?,
                username: row.try_get("username")?,
                email: row.try_get("email")?,
                password_hash: row.try_get("password_hash")?,
                display_name: row.try_get("display_name")?,
                is_admin: row.try_get("is_admin")?,
                storage_quota: row.try_get("storage_quota")?,
                theme: row.try_get("theme")?,
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
                disabled_at: row.try_get("disabled_at")?,
                name: row.try_get("name")?,
                surname: row.try_get("surname")?,
                avatar_path: row.try_get("avatar_path")?,
                email_sharing_enabled: row.try_get("email_sharing_enabled")?,
                trash_retention_days: row.try_get("trash_retention_days")?,
                tenant_id: row.try_get("tenant_id")?,
                dashboard_config: row.try_get("dashboard_config")?,
            }))
        } else {
            Ok(None)
        }
    }

    /// Find user by ID
    pub async fn find_user_by_id(&self, id: Uuid) -> Result<Option<User>> {
        // TODO: Switch to sqlx::query!() after Docker Compose setup (Task 11)
        let row = sqlx::query(
            r#"SELECT id, username, email, password_hash, display_name, is_admin, storage_quota, theme, created_at, updated_at, disabled_at, name, surname, avatar_path, email_sharing_enabled, trash_retention_days, tenant_id, dashboard_config FROM users WHERE id = $1"#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            let user = User {
                id: row.try_get("id")?,
                username: row.try_get("username")?,
                email: row.try_get("email")?,
                password_hash: row.try_get("password_hash")?,
                display_name: row.try_get("display_name")?,
                is_admin: row.try_get("is_admin")?,
                storage_quota: row.try_get("storage_quota")?,
                theme: row
                    .try_get::<String, _>("theme")?
                    .parse()
                    .unwrap_or_default(),
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
                disabled_at: row.try_get("disabled_at")?,
                name: row.try_get("name")?,
                surname: row.try_get("surname")?,
                avatar_path: row.try_get("avatar_path")?,
                email_sharing_enabled: row.try_get("email_sharing_enabled")?,
                trash_retention_days: row.try_get("trash_retention_days")?,
                tenant_id: row.try_get("tenant_id")?,
                dashboard_config: row.try_get("dashboard_config")?,
            };
            Ok(Some(user))
        } else {
            Ok(None)
        }
    }

    /// Update a user's password hash and bump the updated timestamp.
    pub async fn update_user_password_hash(&self, id: Uuid, password_hash: &str) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE users
            SET password_hash = $2, updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(id)
        .bind(password_hash)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Check if any users exist (for admin bootstrapping)
    pub async fn has_users(&self) -> Result<bool> {
        // TODO: Switch to sqlx::query!() after Docker Compose setup (Task 11)
        let row = sqlx::query("SELECT COUNT(*) as count FROM users")
            .fetch_one(&self.pool)
            .await?;

        let count: i64 = row.try_get("count")?;
        Ok(count > 0)
    }

    /// Update user's theme preference
    pub async fn update_user_theme(&self, user_id: Uuid, theme: &str) -> Result<()> {
        sqlx::query(r#"UPDATE users SET theme = $1, updated_at = NOW() WHERE id = $2"#)
            .bind(theme)
            .bind(user_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Update user profile fields
    pub async fn update_user_profile(
        &self,
        user_id: Uuid,
        name: Option<&str>,
        surname: Option<&str>,
        display_name: Option<&str>,
        email_sharing_enabled: Option<bool>,
        theme: Option<String>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE users SET
                name = COALESCE($1, name),
                surname = COALESCE($2, surname),
                display_name = COALESCE($3, display_name),
                email_sharing_enabled = COALESCE($4, email_sharing_enabled),
                theme = COALESCE($5, theme),
                updated_at = NOW()
            WHERE id = $6
            "#,
        )
        .bind(name)
        .bind(surname)
        .bind(display_name)
        .bind(email_sharing_enabled)
        .bind(theme)
        .bind(user_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Update user's trash retention setting.
    pub async fn update_user_trash_retention(
        &self,
        user_id: Uuid,
        days: Option<i32>,
    ) -> Result<()> {
        sqlx::query(
            r#"UPDATE users SET trash_retention_days = $1, updated_at = NOW() WHERE id = $2"#,
        )
        .bind(days)
        .bind(user_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Update user's avatar path
    pub async fn update_user_avatar(&self, user_id: Uuid, avatar_path: Option<&str>) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE users
            SET avatar_path = $1,
                updated_at = NOW()
            WHERE id = $2
            "#,
        )
        .bind(avatar_path)
        .bind(user_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Update user's dashboard configuration
    pub async fn update_user_dashboard_config(
        &self,
        user_id: Uuid,
        config: &rustshare_core::domain::DashboardConfig,
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE users
            SET dashboard_config = $1,
                updated_at = NOW()
            WHERE id = $2
            "#,
        )
        .bind(sqlx::types::Json(config))
        .bind(user_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// List all users that have trash auto-clean enabled (trash_retention_days IS NOT NULL).
    pub async fn list_users_with_trash_retention(&self) -> Result<Vec<(Uuid, Uuid, i32)>> {
        let rows = sqlx::query(
            r#"SELECT id, tenant_id, trash_retention_days FROM users WHERE trash_retention_days IS NOT NULL"#
        )
        .fetch_all(&self.pool)
        .await?;

        let mut users = Vec::with_capacity(rows.len());
        for row in rows {
            let id: Uuid = row.try_get("id")?;
            let tenant_id: Uuid = row.try_get("tenant_id")?;
            let days: i32 = row.try_get("trash_retention_days")?;
            users.push((id, tenant_id, days));
        }

        Ok(users)
    }

    // -----------------------------------------------------------------
    // Login protection
    // -----------------------------------------------------------------

    /// Check if an IP address is currently blocked from logging in.
    pub async fn is_ip_blocked(&self, ip_address: &str) -> Result<bool> {
        let row = sqlx::query(
            r#"
            SELECT blocked_until
            FROM login_attempts
            WHERE ip_address = $1
            "#,
        )
        .bind(ip_address)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            let blocked_until: Option<chrono::DateTime<Utc>> = row.try_get("blocked_until")?;
            if let Some(until) = blocked_until {
                if until > Utc::now() {
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }

    /// Record a failed login attempt for an IP address.
    /// If failed_count reaches max_login_attempts, blocks the IP for login_block_duration_minutes.
    pub async fn record_login_failure(&self, ip_address: &str) -> Result<()> {
        let config = sqlx::query(
            r#"
            SELECT login_protection_enabled, max_login_attempts, login_block_duration_minutes
            FROM security_config
            WHERE id = 1
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        let enabled: bool = config.try_get("login_protection_enabled")?;
        if !enabled {
            return Ok(());
        }

        let max_attempts: i32 = config.try_get("max_login_attempts")?;
        let block_duration: i32 = config.try_get("login_block_duration_minutes")?;

        // Check if an existing block has expired — if so, reset the count
        let existing =
            sqlx::query("SELECT blocked_until FROM login_attempts WHERE ip_address = $1")
                .bind(ip_address)
                .fetch_optional(&self.pool)
                .await?;

        if let Some(row) = existing {
            let blocked_until: Option<chrono::DateTime<Utc>> = row.try_get("blocked_until")?;
            if let Some(until) = blocked_until {
                if until <= Utc::now() {
                    // Block expired — reset count so user gets a fresh start
                    sqlx::query(
                        "UPDATE login_attempts SET failed_count = 0, blocked_until = NULL WHERE ip_address = $1"
                    )
                    .bind(ip_address)
                    .execute(&self.pool)
                    .await?;
                }
            }
        }

        let row = sqlx::query(
            r#"
            INSERT INTO login_attempts (ip_address, failed_count, last_attempt_at)
            VALUES ($1, 1, NOW())
            ON CONFLICT (ip_address) DO UPDATE SET
                failed_count = login_attempts.failed_count + 1,
                last_attempt_at = NOW()
            RETURNING failed_count
            "#,
        )
        .bind(ip_address)
        .fetch_one(&self.pool)
        .await?;

        let failed_count: i32 = row.try_get("failed_count")?;

        if failed_count >= max_attempts {
            let block_until = Utc::now() + chrono::Duration::minutes(block_duration as i64);
            sqlx::query(
                r#"
                UPDATE login_attempts
                SET blocked_until = $2
                WHERE ip_address = $1
                "#,
            )
            .bind(ip_address)
            .bind(block_until)
            .execute(&self.pool)
            .await?;
        }

        Ok(())
    }

    /// Clear login attempts for an IP address after a successful login.
    pub async fn clear_login_attempts(&self, ip_address: &str) -> Result<()> {
        sqlx::query("DELETE FROM login_attempts WHERE ip_address = $1")
            .bind(ip_address)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Get the current security configuration.
    pub async fn get_security_config(&self) -> Result<SecurityConfig> {
        let row = sqlx::query(
            r#"
            SELECT login_protection_enabled, max_login_attempts, login_block_duration_minutes, updated_at
            FROM security_config
            WHERE id = 1
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(SecurityConfig {
            login_protection_enabled: row.try_get("login_protection_enabled")?,
            max_login_attempts: row.try_get("max_login_attempts")?,
            login_block_duration_minutes: row.try_get("login_block_duration_minutes")?,
            updated_at: row.try_get("updated_at")?,
        })
    }

    /// Update the security configuration.
    pub async fn update_security_config(
        &self,
        login_protection_enabled: Option<bool>,
        max_login_attempts: Option<i32>,
        login_block_duration_minutes: Option<i32>,
    ) -> Result<SecurityConfig> {
        let row = sqlx::query(
            r#"
            UPDATE security_config
            SET
                login_protection_enabled = COALESCE($1, login_protection_enabled),
                max_login_attempts = COALESCE($2, max_login_attempts),
                login_block_duration_minutes = COALESCE($3, login_block_duration_minutes),
                updated_at = NOW()
            WHERE id = 1
            RETURNING login_protection_enabled, max_login_attempts, login_block_duration_minutes, updated_at
            "#,
        )
        .bind(login_protection_enabled)
        .bind(max_login_attempts)
        .bind(login_block_duration_minutes)
        .fetch_one(&self.pool)
        .await?;

        Ok(SecurityConfig {
            login_protection_enabled: row.try_get("login_protection_enabled")?,
            max_login_attempts: row.try_get("max_login_attempts")?,
            login_block_duration_minutes: row.try_get("login_block_duration_minutes")?,
            updated_at: row.try_get("updated_at")?,
        })
    }

    /// Create a new file in the projection table
    pub async fn create_file(&self, file: &File) -> Result<()> {
        // TODO: Switch to sqlx::query!() after Docker Compose setup (Task 11)
        sqlx::query(
            r#"
            INSERT INTO files (id, name, path, size, mime_type, content_hash, storage_key, owner_id, parent_folder_id, current_version, created_at, modified_at, tenant_id, starred_at, deleted_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, NULL, NULL)
            "#,
        )
        .bind(file.id)
        .bind(&file.name)
        .bind(&file.path)
        .bind(file.size)
        .bind(&file.mime_type)
        .bind(&file.content_hash)
        .bind(file.storage_key())
        .bind(file.owner_id)
        .bind(file.parent_folder_id)
        .bind(file.current_version)
        .bind(file.created_at)
        .bind(file.modified_at)
        .bind(file.tenant_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Find file by ID
    pub async fn find_file_by_id(&self, id: Uuid, owner_id: Uuid) -> Result<Option<File>> {
        // TODO: Switch to sqlx::query!() after Docker Compose setup (Task 11)
        let row = sqlx::query(
            r#"SELECT id, name, path, size, mime_type, content_hash, owner_id, parent_folder_id, current_version, created_at, modified_at, starred_at, deleted_at, tenant_id FROM files WHERE id = $1 AND owner_id = $2 AND deleted_at IS NULL"#,
        )
        .bind(id)
        .bind(owner_id)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            let file = File {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                path: row.try_get("path")?,
                size: row.try_get("size")?,
                mime_type: row.try_get("mime_type")?,
                content_hash: row.try_get("content_hash")?,
                owner_id: row.try_get("owner_id")?,
                parent_folder_id: row.try_get("parent_folder_id")?,
                current_version: row.try_get("current_version")?,
                created_at: row.try_get("created_at")?,
                modified_at: row.try_get("modified_at")?,
                starred_at: row.try_get("starred_at")?,
                deleted_at: row.try_get("deleted_at")?,
                tenant_id: row.try_get("tenant_id")?,
            };
            Ok(Some(file))
        } else {
            Ok(None)
        }
    }

    /// Find a file by its canonical path for a specific owner.
    pub async fn find_file_by_path(&self, path: &str, owner_id: Uuid) -> Result<Option<File>> {
        let row = sqlx::query(
            r#"
            SELECT id, name, path, size, mime_type, content_hash, owner_id, parent_folder_id, current_version, created_at, modified_at, starred_at, deleted_at, tenant_id
            FROM files
            WHERE path = $1 AND owner_id = $2 AND deleted_at IS NULL
            "#,
        )
        .bind(path)
        .bind(owner_id)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            let file = File {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                path: row.try_get("path")?,
                size: row.try_get("size")?,
                mime_type: row.try_get("mime_type")?,
                content_hash: row.try_get("content_hash")?,
                owner_id: row.try_get("owner_id")?,
                parent_folder_id: row.try_get("parent_folder_id")?,
                current_version: row.try_get("current_version")?,
                created_at: row.try_get("created_at")?,
                modified_at: row.try_get("modified_at")?,
                starred_at: row.try_get("starred_at")?,
                deleted_at: row.try_get("deleted_at")?,
                tenant_id: row.try_get("tenant_id")?,
            };
            Ok(Some(file))
        } else {
            Ok(None)
        }
    }

    /// Update a file in the projection table
    pub async fn update_file(&self, file: &File) -> Result<()> {
        // TODO: Switch to sqlx::query!() after Docker Compose setup (Task 11)
        sqlx::query(
            r#"
            UPDATE files
            SET name = $2, path = $3, size = $4, mime_type = $5, content_hash = $6,
                storage_key = $7, parent_folder_id = $8, current_version = $9, modified_at = $10, tenant_id = $11
            WHERE id = $1 AND owner_id = $12
            "#,
        )
        .bind(file.id)
        .bind(&file.name)
        .bind(&file.path)
        .bind(file.size)
        .bind(&file.mime_type)
        .bind(&file.content_hash)
        .bind(file.storage_key())
        .bind(file.parent_folder_id)
        .bind(file.current_version)
        .bind(file.modified_at)
        .bind(file.tenant_id)
        .bind(file.owner_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Delete a file from the projection table
    pub async fn delete_file(&self, id: Uuid, owner_id: Uuid) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE files
            SET deleted_at = NOW(), starred_at = NULL
            WHERE id = $1 AND owner_id = $2 AND deleted_at IS NULL
            "#,
        )
        .bind(id)
        .bind(owner_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// List files with optional filters
    ///
    /// Returns files owned by the specified user, optionally filtered by parent folder.
    /// Pass `None` for parent_id to get files in the root directory (no parent).
    pub async fn list_files(
        &self,
        parent_id: Option<Uuid>,
        owner_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<File>> {
        // TODO: Switch to sqlx::query!() after Docker Compose setup (Task 11)
        let rows = sqlx::query(
            r#"
            SELECT id, name, path, size, mime_type, content_hash, owner_id, parent_folder_id, current_version, created_at, modified_at, starred_at, deleted_at, tenant_id
            FROM files
            WHERE owner_id = $1
              AND tenant_id = $2
              AND deleted_at IS NULL
              AND (parent_folder_id = $3 OR ($3 IS NULL AND parent_folder_id IS NULL))
            ORDER BY name ASC
            "#,
        )
        .bind(owner_id)
        .bind(tenant_id)
        .bind(parent_id)
        .fetch_all(&self.pool)
        .await?;

        let mut files = Vec::new();
        for row in rows {
            let file = File {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                path: row.try_get("path")?,
                size: row.try_get("size")?,
                mime_type: row.try_get("mime_type")?,
                content_hash: row.try_get("content_hash")?,
                owner_id: row.try_get("owner_id")?,
                parent_folder_id: row.try_get("parent_folder_id")?,
                current_version: row.try_get("current_version")?,
                created_at: row.try_get("created_at")?,
                modified_at: row.try_get("modified_at")?,
                starred_at: row.try_get("starred_at")?,
                deleted_at: row.try_get("deleted_at")?,
                tenant_id: row.try_get("tenant_id")?,
            };
            files.push(file);
        }

        Ok(files)
    }

    /// List files by parent folder regardless of owner.
    ///
    /// This is used for collaborative folders where children may be created by
    /// different users but still belong to the same parent folder.
    pub async fn list_files_by_parent(
        &self,
        parent_id: Option<Uuid>,
        tenant_id: Uuid,
    ) -> Result<Vec<File>> {
        let rows = sqlx::query(
            r#"
            SELECT id, name, path, size, mime_type, content_hash, owner_id, parent_folder_id, current_version, created_at, modified_at, starred_at, deleted_at, tenant_id
            FROM files
            WHERE tenant_id = $1
              AND deleted_at IS NULL
              AND (parent_folder_id = $2 OR ($2 IS NULL AND parent_folder_id IS NULL))
            ORDER BY name ASC
            "#,
        )
        .bind(tenant_id)
        .bind(parent_id)
        .fetch_all(&self.pool)
        .await?;

        let mut files = Vec::new();
        for row in rows {
            files.push(File {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                path: row.try_get("path")?,
                size: row.try_get("size")?,
                mime_type: row.try_get("mime_type")?,
                content_hash: row.try_get("content_hash")?,
                owner_id: row.try_get("owner_id")?,
                parent_folder_id: row.try_get("parent_folder_id")?,
                current_version: row.try_get("current_version")?,
                created_at: row.try_get("created_at")?,
                modified_at: row.try_get("modified_at")?,
                starred_at: row.try_get("starred_at")?,
                deleted_at: row.try_get("deleted_at")?,
                tenant_id: row.try_get("tenant_id")?,
            });
        }

        Ok(files)
    }

    pub async fn set_file_starred(&self, id: Uuid, owner_id: Uuid, starred: bool) -> Result<bool> {
        let result = sqlx::query(
            r#"
            UPDATE files
            SET starred_at = CASE WHEN $3 THEN NOW() ELSE NULL END
            WHERE id = $1 AND owner_id = $2 AND deleted_at IS NULL
            "#,
        )
        .bind(id)
        .bind(owner_id)
        .bind(starred)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn restore_file(&self, id: Uuid, owner_id: Uuid, tenant_id: Uuid) -> Result<bool> {
        let row = sqlx::query(
            r#"
            SELECT id, name, parent_folder_id
            FROM files
            WHERE id = $1 AND owner_id = $2 AND tenant_id = $3 AND deleted_at IS NOT NULL
            "#,
        )
        .bind(id)
        .bind(owner_id)
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await?;

        let Some(row) = row else {
            return Ok(false);
        };

        let name: String = row.try_get("name")?;
        let parent_folder_id: Option<Uuid> = row.try_get("parent_folder_id")?;

        let parent_path = if let Some(parent_id) = parent_folder_id {
            sqlx::query_scalar::<_, String>(
                r#"
                SELECT path
                FROM folders
                WHERE id = $1 AND owner_id = $2 AND tenant_id = $3 AND deleted_at IS NULL
                "#,
            )
            .bind(parent_id)
            .bind(owner_id)
            .bind(tenant_id)
            .fetch_optional(&self.pool)
            .await?
        } else {
            None
        };

        let restored_parent_id = if parent_path.is_some() {
            parent_folder_id
        } else {
            None
        };
        let restored_path = if let Some(parent_path) = parent_path {
            if parent_path == "/" {
                format!("/{}", name)
            } else {
                format!("{}/{}", parent_path.trim_end_matches('/'), name)
            }
        } else {
            format!("/{}", name)
        };

        let result = sqlx::query(
            r#"
            UPDATE files
            SET deleted_at = NULL, parent_folder_id = $2, path = $3
            WHERE id = $1 AND owner_id = $4 AND tenant_id = $5 AND deleted_at IS NOT NULL
            "#,
        )
        .bind(id)
        .bind(restored_parent_id)
        .bind(restored_path)
        .bind(owner_id)
        .bind(tenant_id)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn permanently_delete_file(&self, id: Uuid, owner_id: Uuid) -> Result<bool> {
        let result = sqlx::query(
            "DELETE FROM files WHERE id = $1 AND owner_id = $2 AND deleted_at IS NOT NULL",
        )
        .bind(id)
        .bind(owner_id)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Create a new file version in the projection table
    pub async fn create_file_version(&self, version: &FileVersion) -> Result<()> {
        // TODO: Switch to sqlx::query!() after Docker Compose setup (Task 11)
        sqlx::query(
            r#"
            INSERT INTO file_versions (
                id,
                file_id,
                version_number,
                content_hash,
                storage_key,
                size,
                replication_state,
                created_by,
                created_at,
                change_description,
                tenant_id
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            ON CONFLICT (file_id, version_number) DO UPDATE SET
                content_hash = EXCLUDED.content_hash,
                storage_key = EXCLUDED.storage_key,
                size = EXCLUDED.size,
                replication_state = EXCLUDED.replication_state,
                created_at = EXCLUDED.created_at,
                change_description = EXCLUDED.change_description
            "#,
        )
        .bind(version.id)
        .bind(version.file_id)
        .bind(version.version_number)
        .bind(&version.content_hash)
        .bind(version.storage_key())
        .bind(version.size)
        .bind(version.replication_state.as_str())
        .bind(version.created_by)
        .bind(version.created_at)
        .bind(&version.change_description)
        .bind(version.tenant_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// List all versions for a file, ordered by version number descending (newest first)
    pub async fn list_file_versions(&self, file_id: Uuid, owner_id: Uuid) -> Result<Vec<FileVersion>> {
        // TODO: Switch to sqlx::query!() after Docker Compose setup (Task 11)
        let rows = sqlx::query(
            r#"
            SELECT v.id, v.file_id, v.version_number, v.content_hash, v.size, v.replication_state, v.created_by, v.created_at, v.change_description, v.tenant_id
            FROM file_versions v
            JOIN files f ON v.file_id = f.id
            WHERE v.file_id = $1 AND f.owner_id = $2
            ORDER BY v.version_number DESC
            "#,
        )
        .bind(file_id)
        .bind(owner_id)
        .fetch_all(&self.pool)
        .await?;

        let mut versions = Vec::new();
        for row in rows {
            let replication_state: String = row.try_get("replication_state")?;
            let version = FileVersion {
                id: row.try_get("id")?,
                file_id: row.try_get("file_id")?,
                version_number: row.try_get("version_number")?,
                content_hash: row.try_get("content_hash")?,
                size: row.try_get("size")?,
                replication_state: Self::parse_replication_state(&replication_state)?,
                created_by: row.try_get("created_by")?,
                created_at: row.try_get("created_at")?,
                change_description: row.try_get("change_description")?,
                tenant_id: row.try_get("tenant_id")?,
            };
            versions.push(version);
        }

        Ok(versions)
    }

    /// Find a specific version of a file
    pub async fn find_file_version(
        &self,
        file_id: Uuid,
        version: i32,
        owner_id: Uuid,
    ) -> Result<Option<FileVersion>> {
        // TODO: Switch to sqlx::query!() after Docker Compose setup (Task 11)
        let row = sqlx::query(
            r#"
            SELECT v.id, v.file_id, v.version_number, v.content_hash, v.size, v.replication_state, v.created_by, v.created_at, v.change_description, v.tenant_id
            FROM file_versions v
            JOIN files f ON v.file_id = f.id
            WHERE v.file_id = $1 AND v.version_number = $2 AND f.owner_id = $3
            "#,
        )
        .bind(file_id)
        .bind(version)
        .bind(owner_id)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            let replication_state: String = row.try_get("replication_state")?;
            let version = FileVersion {
                id: row.try_get("id")?,
                file_id: row.try_get("file_id")?,
                version_number: row.try_get("version_number")?,
                content_hash: row.try_get("content_hash")?,
                size: row.try_get("size")?,
                replication_state: Self::parse_replication_state(&replication_state)?,
                created_by: row.try_get("created_by")?,
                created_at: row.try_get("created_at")?,
                change_description: row.try_get("change_description")?,
                tenant_id: row.try_get("tenant_id")?,
            };
            Ok(Some(version))
        } else {
            Ok(None)
        }
    }

    /// Count enabled replication targets.
    pub async fn count_enabled_replication_targets(&self) -> Result<i64> {
        let row =
            sqlx::query("SELECT COUNT(*) AS count FROM replication_targets WHERE enabled = TRUE")
                .fetch_one(&self.pool)
                .await?;

        row.try_get("count").map_err(Into::into)
    }

    /// Create a durable replication job for asynchronous workers.
    pub async fn create_replication_job(&self, job: &ReplicationJob) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO replication_jobs (
                id,
                file_id,
                file_version_id,
                storage_key,
                status,
                attempt_count,
                next_attempt_at,
                last_attempt_at,
                leased_at,
                lease_token,
                last_error,
                created_at,
                updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            "#,
        )
        .bind(job.id)
        .bind(job.file_id)
        .bind(job.file_version_id)
        .bind(&job.storage_key)
        .bind(job.status.as_str())
        .bind(job.attempt_count)
        .bind(job.next_attempt_at)
        .bind(job.last_attempt_at)
        .bind(job.leased_at)
        .bind(job.lease_token)
        .bind(&job.last_error)
        .bind(job.created_at)
        .bind(job.updated_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Update replication state after queueing or worker progress.
    pub async fn update_file_version_replication_state(
        &self,
        version_id: Uuid,
        state: ReplicationState,
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE file_versions
            SET replication_state = $2
            WHERE id = $1
            "#,
        )
        .bind(version_id)
        .bind(state.as_str())
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// List enabled replication targets that workers should copy into.
    pub async fn list_enabled_replication_targets(&self) -> Result<Vec<ReplicationTarget>> {
        let rows = sqlx::query(
            r#"
            SELECT
                id,
                name,
                destination_type,
                endpoint,
                bucket,
                region,
                base_path,
                is_required,
                enabled,
                auth_config,
                health_status,
                last_healthy_at,
                last_error,
                created_at,
                updated_at
            FROM replication_targets
            WHERE enabled = TRUE
            ORDER BY created_at ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let mut targets = Vec::with_capacity(rows.len());
        for row in rows {
            targets.push(ReplicationTarget {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                destination_type: row.try_get("destination_type")?,
                endpoint: row.try_get("endpoint")?,
                bucket: row.try_get("bucket")?,
                region: row.try_get("region")?,
                base_path: row.try_get("base_path")?,
                is_required: row.try_get("is_required")?,
                enabled: row.try_get("enabled")?,
                auth_config: row.try_get("auth_config")?,
                health_status: row.try_get("health_status")?,
                last_healthy_at: row.try_get("last_healthy_at")?,
                last_error: row.try_get("last_error")?,
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
            });
        }

        Ok(targets)
    }

    /// Lease due replication jobs for background processing.
    pub async fn lease_replication_jobs(
        &self,
        limit: i64,
        lease_timeout_secs: i64,
        lease_token: Uuid,
    ) -> Result<Vec<ReplicationJob>> {
        let rows = sqlx::query(
            r#"
            WITH candidates AS (
                SELECT id
                FROM replication_jobs
                WHERE status IN ('queued', 'retrying')
                  AND next_attempt_at <= NOW()
                  AND (
                    leased_at IS NULL
                    OR leased_at < NOW() - make_interval(secs => $2)
                  )
                ORDER BY next_attempt_at ASC, created_at ASC
                LIMIT $1
                FOR UPDATE SKIP LOCKED
            )
            UPDATE replication_jobs
            SET
                status = 'syncing',
                leased_at = NOW(),
                lease_token = $3,
                last_attempt_at = NOW(),
                attempt_count = attempt_count + 1,
                updated_at = NOW()
            WHERE id IN (SELECT id FROM candidates)
            RETURNING
                id,
                file_id,
                file_version_id,
                storage_key,
                status,
                attempt_count,
                next_attempt_at,
                last_attempt_at,
                leased_at,
                lease_token,
                last_error,
                created_at,
                updated_at
            "#,
        )
        .bind(limit)
        .bind(lease_timeout_secs)
        .bind(lease_token)
        .fetch_all(&self.pool)
        .await?;

        let mut jobs = Vec::with_capacity(rows.len());
        for row in rows {
            let status: String = row.try_get("status")?;
            jobs.push(ReplicationJob {
                id: row.try_get("id")?,
                file_id: row.try_get("file_id")?,
                file_version_id: row.try_get("file_version_id")?,
                storage_key: row.try_get("storage_key")?,
                status: Self::parse_replication_job_status(&status)?,
                attempt_count: row.try_get("attempt_count")?,
                next_attempt_at: row.try_get("next_attempt_at")?,
                last_attempt_at: row.try_get("last_attempt_at")?,
                leased_at: row.try_get("leased_at")?,
                lease_token: row.try_get("lease_token")?,
                last_error: row.try_get("last_error")?,
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
            });
        }

        Ok(jobs)
    }

    /// Mark a replication job as completed and release its lease.
    pub async fn mark_replication_job_completed(&self, job_id: Uuid) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE replication_jobs
            SET
                status = 'completed',
                leased_at = NULL,
                lease_token = NULL,
                last_error = NULL,
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(job_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Mark a replication job for retry after a transient failure.
    pub async fn mark_replication_job_retrying(
        &self,
        job_id: Uuid,
        last_error: &str,
        next_attempt_at: DateTime<Utc>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE replication_jobs
            SET
                status = 'retrying',
                leased_at = NULL,
                lease_token = NULL,
                last_error = $2,
                next_attempt_at = $3,
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(job_id)
        .bind(last_error)
        .bind(next_attempt_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Mark a replication job as terminally failed.
    pub async fn mark_replication_job_failed(&self, job_id: Uuid, last_error: &str) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE replication_jobs
            SET
                status = 'failed',
                leased_at = NULL,
                lease_token = NULL,
                last_error = $2,
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(job_id)
        .bind(last_error)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Record the result of a single target replication attempt.
    pub async fn create_replication_attempt(
        &self,
        attempt: ReplicationAttemptRecord<'_>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO replication_attempts (
                id,
                job_id,
                target_id,
                attempt_number,
                status,
                error_message,
                started_at,
                completed_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(attempt.job_id)
        .bind(attempt.target_id)
        .bind(attempt.attempt_number)
        .bind(attempt.status)
        .bind(attempt.error_message)
        .bind(attempt.started_at)
        .bind(attempt.completed_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Update target health after a replication attempt.
    pub async fn update_replication_target_health(
        &self,
        target_id: Uuid,
        health_status: &str,
        last_error: Option<&str>,
        last_healthy_at: Option<DateTime<Utc>>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE replication_targets
            SET
                health_status = $2,
                last_error = $3,
                last_healthy_at = COALESCE($4, last_healthy_at),
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(target_id)
        .bind(health_status)
        .bind(last_error)
        .bind(last_healthy_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Create a new folder in the projection table
    pub async fn create_folder(&self, folder: &Folder) -> Result<()> {
        // TODO: Switch to sqlx::query!() after Docker Compose setup (Task 11)
        sqlx::query(
            r#"
            INSERT INTO folders (id, name, path, parent_folder_id, owner_id, created_at, updated_at, tenant_id, starred_at, deleted_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NULL, NULL)
            "#,
        )
        .bind(folder.id)
        .bind(&folder.name)
        .bind(&folder.path)
        .bind(folder.parent_folder_id)
        .bind(folder.owner_id)
        .bind(folder.created_at)
        .bind(folder.updated_at)
        .bind(folder.tenant_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Find folder by ID
    pub async fn find_folder_by_id(&self, id: Uuid, owner_id: Uuid) -> Result<Option<Folder>> {
        // TODO: Switch to sqlx::query!() after Docker Compose setup (Task 11)
        let row = sqlx::query(
            r#"
            SELECT id, name, path, parent_folder_id, owner_id, created_at, updated_at, starred_at, deleted_at, tenant_id
            FROM folders
            WHERE id = $1 AND owner_id = $2 AND deleted_at IS NULL
            "#,
        )
        .bind(id)
        .bind(owner_id)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            let folder = Folder {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                path: row.try_get("path")?,
                parent_folder_id: row.try_get("parent_folder_id")?,
                owner_id: row.try_get("owner_id")?,
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
                starred_at: row.try_get("starred_at")?,
                deleted_at: row.try_get("deleted_at")?,
                tenant_id: row.try_get("tenant_id")?,
                ancestor_ids: None, // Will be populated from folder_documents if available
            };
            Ok(Some(folder))
        } else {
            Ok(None)
        }
    }

    /// Find a folder by its canonical path for a specific owner.
    pub async fn find_folder_by_path(&self, path: &str, owner_id: Uuid) -> Result<Option<Folder>> {
        let row = sqlx::query(
            r#"
            SELECT id, name, path, parent_folder_id, owner_id, created_at, updated_at, starred_at, deleted_at, tenant_id
            FROM folders
            WHERE path = $1 AND owner_id = $2 AND deleted_at IS NULL
            "#,
        )
        .bind(path)
        .bind(owner_id)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            let folder = Folder {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                path: row.try_get("path")?,
                parent_folder_id: row.try_get("parent_folder_id")?,
                owner_id: row.try_get("owner_id")?,
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
                starred_at: row.try_get("starred_at")?,
                deleted_at: row.try_get("deleted_at")?,
                tenant_id: row.try_get("tenant_id")?,
                ancestor_ids: None, // Will be populated from folder_documents if available
            };
            Ok(Some(folder))
        } else {
            Ok(None)
        }
    }

    /// Update a folder in the projection table
    pub async fn update_folder(&self, folder: &Folder) -> Result<()> {
        // TODO: Switch to sqlx::query!() after Docker Compose setup (Task 11)
        sqlx::query(
            r#"
            UPDATE folders
            SET name = $2, path = $3, parent_folder_id = $4, updated_at = $5, tenant_id = $6
            WHERE id = $1 AND owner_id = $7
            "#,
        )
        .bind(folder.id)
        .bind(&folder.name)
        .bind(&folder.path)
        .bind(folder.parent_folder_id)
        .bind(folder.updated_at)
        .bind(folder.tenant_id)
        .bind(folder.owner_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Delete a folder from the projection table
    pub async fn delete_folder(&self, id: Uuid, owner_id: Uuid) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE folders
            SET deleted_at = NOW(), starred_at = NULL
            WHERE id = $1 AND owner_id = $2 AND deleted_at IS NULL
            "#,
        )
        .bind(id)
        .bind(owner_id)
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            UPDATE files
            SET deleted_at = COALESCE(deleted_at, NOW()), starred_at = NULL
            WHERE parent_folder_id = $1 AND owner_id = $2 AND deleted_at IS NULL
            "#,
        )
        .bind(id)
        .bind(owner_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// List folders with optional filters
    ///
    /// Returns folders owned by the specified user, optionally filtered by parent folder.
    /// Pass `None` for parent_id to get folders in the root directory (no parent).
    pub async fn list_folders(
        &self,
        parent_id: Option<Uuid>,
        owner_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<Folder>> {
        // TODO: Switch to sqlx::query!() after Docker Compose setup (Task 11)
        let rows = sqlx::query(
            r#"
            SELECT id, name, path, parent_folder_id, owner_id, created_at, updated_at, starred_at, deleted_at, tenant_id
            FROM folders
            WHERE owner_id = $1
              AND tenant_id = $2
              AND deleted_at IS NULL
              AND (parent_folder_id = $3 OR ($3 IS NULL AND parent_folder_id IS NULL))
            ORDER BY name ASC
            "#,
        )
        .bind(owner_id)
        .bind(tenant_id)
        .bind(parent_id)
        .fetch_all(&self.pool)
        .await?;

        let mut folders = Vec::new();
        for row in rows {
            let folder = Folder {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                path: row.try_get("path")?,
                parent_folder_id: row.try_get("parent_folder_id")?,
                owner_id: row.try_get("owner_id")?,
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
                starred_at: row.try_get("starred_at")?,
                deleted_at: row.try_get("deleted_at")?,
                tenant_id: row.try_get("tenant_id")?,
                ancestor_ids: None, // Will be populated from folder_documents if available
            };
            folders.push(folder);
        }

        Ok(folders)
    }

    /// List folders by parent folder regardless of owner.
    ///
    /// This preserves collaborative folder structure when shared folders contain
    /// children created by multiple users.
    pub async fn list_folders_by_parent(
        &self,
        parent_id: Option<Uuid>,
        tenant_id: Uuid,
    ) -> Result<Vec<Folder>> {
        let rows = sqlx::query(
            r#"
            SELECT id, name, path, parent_folder_id, owner_id, created_at, updated_at, starred_at, deleted_at, tenant_id
            FROM folders
            WHERE tenant_id = $1
              AND deleted_at IS NULL
              AND (parent_folder_id = $2 OR ($2 IS NULL AND parent_folder_id IS NULL))
            ORDER BY name ASC
            "#,
        )
        .bind(tenant_id)
        .bind(parent_id)
        .fetch_all(&self.pool)
        .await?;

        let mut folders = Vec::new();
        for row in rows {
            folders.push(Folder {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                path: row.try_get("path")?,
                parent_folder_id: row.try_get("parent_folder_id")?,
                owner_id: row.try_get("owner_id")?,
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
                starred_at: row.try_get("starred_at")?,
                deleted_at: row.try_get("deleted_at")?,
                tenant_id: row.try_get("tenant_id")?,
                ancestor_ids: None,
            });
        }

        Ok(folders)
    }

    /// List folders with share counts
    ///
    /// Returns folders owned by the specified user with share information.
    pub async fn list_folders_with_shares(
        &self,
        parent_id: Option<Uuid>,
        owner_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<FolderWithShares>> {
        let rows = sqlx::query(
            r#"
            SELECT 
                f.id, f.name, f.path, f.parent_folder_id, f.owner_id, 
                f.created_at, f.updated_at, f.tenant_id,
                f.starred_at, f.deleted_at,
                EXISTS (
                    SELECT 1 FROM shares 
                    WHERE folder_id = f.id 
                    AND revoked_at IS NULL
                ) as is_shared,
                (
                    SELECT COUNT(*) FROM shares
                    WHERE folder_id = f.id
                    AND revoked_at IS NULL
                ) as share_count,
                (
                    SELECT MIN(expires_at) FROM shares
                    WHERE folder_id = f.id
                    AND revoked_at IS NULL
                ) as share_expires_at
            FROM folders f
            WHERE f.owner_id = $1
              AND f.tenant_id = $2
              AND f.deleted_at IS NULL
              AND (f.parent_folder_id = $3 OR ($3 IS NULL AND f.parent_folder_id IS NULL))
            ORDER BY f.name ASC
            "#,
        )
        .bind(owner_id)
        .bind(tenant_id)
        .bind(parent_id)
        .fetch_all(&self.pool)
        .await?;

        let mut folders = Vec::new();
        for row in rows {
            let folder = FolderWithShares {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                path: row.try_get("path")?,
                parent_folder_id: row.try_get("parent_folder_id")?,
                owner_id: row.try_get("owner_id")?,
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
                starred_at: row.try_get("starred_at")?,
                deleted_at: row.try_get("deleted_at")?,
                tenant_id: row.try_get("tenant_id")?,
                ancestor_ids: None,
                is_shared: row.try_get("is_shared")?,
                share_count: row.try_get("share_count")?,
                share_expires_at: row.try_get("share_expires_at")?,
            };
            folders.push(folder);
        }

        Ok(folders)
    }

    pub async fn set_folder_starred(
        &self,
        id: Uuid,
        owner_id: Uuid,
        starred: bool,
    ) -> Result<bool> {
        let result = sqlx::query(
            r#"
            UPDATE folders
            SET starred_at = CASE WHEN $3 THEN NOW() ELSE NULL END
            WHERE id = $1 AND owner_id = $2 AND deleted_at IS NULL
            "#,
        )
        .bind(id)
        .bind(owner_id)
        .bind(starred)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn restore_folder(&self, id: Uuid, owner_id: Uuid, tenant_id: Uuid) -> Result<bool> {
        let row = sqlx::query(
            r#"
            SELECT id, name, path, parent_folder_id
            FROM folders
            WHERE id = $1 AND owner_id = $2 AND tenant_id = $3 AND deleted_at IS NOT NULL
            "#,
        )
        .bind(id)
        .bind(owner_id)
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await?;

        let Some(row) = row else {
            return Ok(false);
        };

        let name: String = row.try_get("name")?;
        let old_path: String = row.try_get("path")?;
        let parent_folder_id: Option<Uuid> = row.try_get("parent_folder_id")?;

        let parent_row = if let Some(parent_id) = parent_folder_id {
            sqlx::query(
                r#"
                SELECT id, path
                FROM folders
                WHERE id = $1 AND owner_id = $2 AND tenant_id = $3 AND deleted_at IS NULL
                "#,
            )
            .bind(parent_id)
            .bind(owner_id)
            .bind(tenant_id)
            .fetch_optional(&self.pool)
            .await?
        } else {
            None
        };

        let restored_parent_id: Option<Uuid> = parent_row
            .as_ref()
            .and_then(|value| value.try_get("id").ok());
        let restored_path = if let Some(parent_row) = &parent_row {
            let parent_path: String = parent_row.try_get("path")?;
            if parent_path == "/" {
                format!("/{}", name)
            } else {
                format!("{}/{}", parent_path.trim_end_matches('/'), name)
            }
        } else {
            format!("/{}", name)
        };

        let duplicate = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*)
            FROM folders
            WHERE owner_id = $1
              AND tenant_id = $2
              AND deleted_at IS NULL
              AND parent_folder_id IS NOT DISTINCT FROM $3
              AND name = $4
              AND id <> $5
            "#,
        )
        .bind(owner_id)
        .bind(tenant_id)
        .bind(restored_parent_id)
        .bind(&name)
        .bind(id)
        .fetch_one(&self.pool)
        .await?;

        if duplicate > 0 {
            anyhow::bail!(
                "A folder named `{}` already exists in the restore destination",
                name
            );
        }

        sqlx::query(
            r#"
            UPDATE folders
            SET deleted_at = NULL, parent_folder_id = $2, path = $3
            WHERE id = $1 AND owner_id = $4 AND tenant_id = $5 AND deleted_at IS NOT NULL
            "#,
        )
        .bind(id)
        .bind(restored_parent_id)
        .bind(&restored_path)
        .bind(owner_id)
        .bind(tenant_id)
        .execute(&self.pool)
        .await?;

        let old_prefix = format!("{}/%", old_path.trim_end_matches('/'));
        let new_prefix = format!("{}/", restored_path.trim_end_matches('/'));

        sqlx::query(
            r#"
            UPDATE folders
            SET deleted_at = NULL,
                path = $2 || substr(path, length($3) + 1)
            WHERE owner_id = $1
              AND tenant_id = $4
              AND path LIKE $5
            "#,
        )
        .bind(owner_id)
        .bind(&new_prefix)
        .bind(&old_path)
        .bind(tenant_id)
        .bind(&old_prefix)
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            UPDATE files
            SET deleted_at = NULL,
                path = $2 || substr(path, length($3) + 1)
            WHERE owner_id = $1
              AND tenant_id = $4
              AND path LIKE $5
            "#,
        )
        .bind(owner_id)
        .bind(&new_prefix)
        .bind(&old_path)
        .bind(tenant_id)
        .bind(&old_prefix)
        .execute(&self.pool)
        .await?;

        Ok(true)
    }

    pub async fn permanently_delete_folder(&self, id: Uuid, owner_id: Uuid) -> Result<bool> {
        let result = sqlx::query(
            "DELETE FROM folders WHERE id = $1 AND owner_id = $2 AND deleted_at IS NOT NULL",
        )
        .bind(id)
        .bind(owner_id)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Get a summary of trashed items for a user.
    pub async fn get_trash_summary(
        &self,
        owner_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<(i64, i64, i64)> {
        let file_row = sqlx::query(
            r#"
            SELECT COUNT(*) as count, COALESCE(SUM(size), 0)::bigint as total_size
            FROM files
            WHERE owner_id = $1 AND tenant_id = $2 AND deleted_at IS NOT NULL
            "#,
        )
        .bind(owner_id)
        .bind(tenant_id)
        .fetch_one(&self.pool)
        .await?;

        let file_count: i64 = file_row.try_get("count")?;
        let total_size: i64 = file_row.try_get("total_size")?;

        let folder_row = sqlx::query(
            "SELECT COUNT(*) as count FROM folders WHERE owner_id = $1 AND tenant_id = $2 AND deleted_at IS NOT NULL"
        )
        .bind(owner_id)
        .bind(tenant_id)
        .fetch_one(&self.pool)
        .await?;

        let folder_count: i64 = folder_row.try_get("count")?;

        Ok((file_count, folder_count, total_size))
    }

    /// Permanently delete all trashed items for a user.
    pub async fn empty_trash(&self, owner_id: Uuid, tenant_id: Uuid) -> Result<()> {
        // Delete trashed files first (to avoid FK violations when deleting folders)
        sqlx::query(
            "DELETE FROM files WHERE owner_id = $1 AND tenant_id = $2 AND deleted_at IS NOT NULL",
        )
        .bind(owner_id)
        .bind(tenant_id)
        .execute(&self.pool)
        .await?;

        // Delete trashed folders (cascade will handle any remaining child records)
        sqlx::query(
            "DELETE FROM folders WHERE owner_id = $1 AND tenant_id = $2 AND deleted_at IS NOT NULL",
        )
        .bind(owner_id)
        .bind(tenant_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Permanently delete trashed items older than the given number of days for a user.
    pub async fn clean_old_trash(&self, owner_id: Uuid, tenant_id: Uuid, days: i32) -> Result<u64> {
        let cutoff = chrono::Utc::now() - chrono::Duration::days(days.into());

        // Delete old trashed files first
        let file_result = sqlx::query(
            "DELETE FROM files WHERE owner_id = $1 AND tenant_id = $2 AND deleted_at IS NOT NULL AND deleted_at < $3"
        )
        .bind(owner_id)
        .bind(tenant_id)
        .bind(cutoff)
        .execute(&self.pool)
        .await?;

        // Delete old trashed folders
        let folder_result = sqlx::query(
            "DELETE FROM folders WHERE owner_id = $1 AND tenant_id = $2 AND deleted_at IS NOT NULL AND deleted_at < $3"
        )
        .bind(owner_id)
        .bind(tenant_id)
        .bind(cutoff)
        .execute(&self.pool)
        .await?;

        Ok(file_result.rows_affected() + folder_result.rows_affected())
    }

    /// Find all descendant folders of a given folder using recursive CTE
    ///
    /// Returns all folders in the subtree rooted at the specified folder,
    /// including the folder itself and all its direct and indirect children.
    pub async fn find_descendant_folders(&self, folder_id: Uuid) -> Result<Vec<Folder>> {
        // TODO: Switch to sqlx::query!() after Docker Compose setup (Task 11)
        let rows = sqlx::query(
            r#"
            WITH RECURSIVE folder_tree AS (
                -- Base case: start with the specified folder
                SELECT id, name, path, parent_folder_id, owner_id, created_at, updated_at, starred_at, deleted_at, tenant_id
                FROM folders
                WHERE id = $1 AND deleted_at IS NULL

                UNION ALL

                -- Recursive case: get all direct children
                SELECT f.id, f.name, f.path, f.parent_folder_id, f.owner_id, f.created_at, f.updated_at, f.starred_at, f.deleted_at, f.tenant_id
                FROM folders f
                INNER JOIN folder_tree ft ON f.parent_folder_id = ft.id
                WHERE f.deleted_at IS NULL
            )
            SELECT id, name, path, parent_folder_id, owner_id, created_at, updated_at, starred_at, deleted_at, tenant_id
            FROM folder_tree
            ORDER BY path ASC
            "#,
        )
        .bind(folder_id)
        .fetch_all(&self.pool)
        .await?;

        let mut folders = Vec::new();
        for row in rows {
            let folder = Folder {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                path: row.try_get("path")?,
                parent_folder_id: row.try_get("parent_folder_id")?,
                owner_id: row.try_get("owner_id")?,
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
                starred_at: row.try_get("starred_at")?,
                deleted_at: row.try_get("deleted_at")?,
                tenant_id: row.try_get("tenant_id")?,
                ancestor_ids: None, // Will be populated from folder_documents if available
            };
            folders.push(folder);
        }

        Ok(folders)
    }

    /// Create a new share link for a file
    pub async fn create_share(&self, share: &Share) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO shares (id, file_id, folder_id, share_token, recipient_user_id, recipient_group_id, created_by, permissions, password_hash, expires_at, upload_only, access_count, created_at, tenant_id)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
            "#,
        )
        .bind(share.id)
        .bind(share.file_id)
        .bind(share.folder_id)
        .bind(&share.share_token)
        .bind(share.recipient_user_id)
        .bind(share.recipient_group_id)
        .bind(share.created_by)
        .bind(Self::permission_to_db_value(share.permissions))
        .bind(&share.password_hash)
        .bind(share.expires_at)
        .bind(share.upload_only)
        .bind(share.access_count)
        .bind(share.created_at)
        .bind(share.tenant_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Find a share by its token
    pub async fn get_share_by_token(&self, token: &str) -> Result<Option<Share>> {
        // TODO: Switch to sqlx::query!() after Docker Compose setup (Task 11)
        let row = sqlx::query(
            r#"
            SELECT id, file_id, folder_id, share_token, recipient_user_id, recipient_group_id, created_by, permissions, password_hash, expires_at, upload_only, access_count, created_at, revoked_at, tenant_id
            FROM shares
            WHERE share_token = $1
            "#,
        )
        .bind(token)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            let permissions_str: String = row.try_get("permissions")?;
            let permissions = Self::permission_from_db_value(&permissions_str);

            let share = Share {
                id: row.try_get("id")?,
                file_id: row.try_get("file_id")?,
                folder_id: row.try_get("folder_id")?,
                share_token: row.try_get("share_token")?,
                recipient_user_id: row.try_get("recipient_user_id")?,
                recipient_group_id: row.try_get("recipient_group_id")?,
                created_by: row.try_get("created_by")?,
                permissions,
                password_hash: row.try_get("password_hash")?,
                expires_at: row.try_get("expires_at")?,
                upload_only: row.try_get("upload_only")?,
                access_count: row.try_get("access_count")?,
                created_at: row.try_get("created_at")?,
                revoked_at: row.try_get("revoked_at")?,
                tenant_id: row.try_get("tenant_id")?,
            };
            Ok(Some(share))
        } else {
            Ok(None)
        }
    }

    /// Find a share by ID
    pub async fn get_share(&self, share_id: Uuid, actor_id: Uuid) -> Result<Option<Share>> {
        // TODO: Switch to sqlx::query!() after Docker Compose setup (Task 11)
        let row = sqlx::query(
            r#"
            SELECT id, file_id, folder_id, share_token, recipient_user_id, recipient_group_id, created_by, permissions, password_hash, expires_at, upload_only, access_count, created_at, revoked_at, tenant_id
            FROM shares
            WHERE id = $1 AND created_by = $2
            "#,
        )
        .bind(share_id)
        .bind(actor_id)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            let permissions_str: String = row.try_get("permissions")?;
            let permissions = Self::permission_from_db_value(&permissions_str);

            let share = Share {
                id: row.try_get("id")?,
                file_id: row.try_get("file_id")?,
                folder_id: row.try_get("folder_id")?,
                share_token: row.try_get("share_token")?,
                recipient_user_id: row.try_get("recipient_user_id")?,
                recipient_group_id: row.try_get("recipient_group_id")?,
                created_by: row.try_get("created_by")?,
                permissions,
                password_hash: row.try_get("password_hash")?,
                expires_at: row.try_get("expires_at")?,
                upload_only: row.try_get("upload_only")?,
                access_count: row.try_get("access_count")?,
                created_at: row.try_get("created_at")?,
                revoked_at: row.try_get("revoked_at")?,
                tenant_id: row.try_get("tenant_id")?,
            };
            Ok(Some(share))
        } else {
            Ok(None)
        }
    }

    /// Get all active (non-revoked) shares for a file
    pub async fn get_file_shares(&self, file_id: Uuid, actor_id: Uuid) -> Result<Vec<Share>> {
        // TODO: Switch to sqlx::query!() after Docker Compose setup (Task 11)
        let rows = sqlx::query(
            r#"
            SELECT id, file_id, folder_id, share_token, recipient_user_id, recipient_group_id, created_by, permissions, password_hash, expires_at, upload_only, access_count, created_at, revoked_at, tenant_id
            FROM shares
            WHERE file_id = $1 AND created_by = $2 AND revoked_at IS NULL
            ORDER BY created_at DESC
            "#,
        )
        .bind(file_id)
        .bind(actor_id)
        .fetch_all(&self.pool)
        .await?;

        let mut shares = Vec::new();
        for row in rows {
            let permissions_str: String = row.try_get("permissions")?;
            let permissions = Self::permission_from_db_value(&permissions_str);

            let share = Share {
                id: row.try_get("id")?,
                file_id: row.try_get("file_id")?,
                folder_id: row.try_get("folder_id")?,
                share_token: row.try_get("share_token")?,
                recipient_user_id: row.try_get("recipient_user_id")?,
                recipient_group_id: row.try_get("recipient_group_id")?,
                created_by: row.try_get("created_by")?,
                permissions,
                password_hash: row.try_get("password_hash")?,
                expires_at: row.try_get("expires_at")?,
                upload_only: row.try_get("upload_only")?,
                access_count: row.try_get("access_count")?,
                created_at: row.try_get("created_at")?,
                revoked_at: row.try_get("revoked_at")?,
                tenant_id: row.try_get("tenant_id")?,
            };
            shares.push(share);
        }

        Ok(shares)
    }

    /// Get all active (non-revoked) shares for a folder.
    pub async fn get_folder_shares(&self, folder_id: Uuid, actor_id: Uuid) -> Result<Vec<Share>> {
        let rows = sqlx::query(
            r#"
            SELECT id, file_id, folder_id, share_token, recipient_user_id, recipient_group_id, created_by, permissions, password_hash, expires_at, upload_only, access_count, created_at, revoked_at, tenant_id
            FROM shares
            WHERE folder_id = $1 AND created_by = $2 AND revoked_at IS NULL
            ORDER BY created_at DESC
            "#,
        )
        .bind(folder_id)
        .bind(actor_id)
        .fetch_all(&self.pool)
        .await?;

        let mut shares = Vec::new();
        for row in rows {
            let permissions_str: String = row.try_get("permissions")?;
            let permissions = Self::permission_from_db_value(&permissions_str);

            shares.push(Share {
                id: row.try_get("id")?,
                file_id: row.try_get("file_id")?,
                folder_id: row.try_get("folder_id")?,
                share_token: row.try_get("share_token")?,
                recipient_user_id: row.try_get("recipient_user_id")?,
                recipient_group_id: row.try_get("recipient_group_id")?,
                created_by: row.try_get("created_by")?,
                permissions,
                password_hash: row.try_get("password_hash")?,
                expires_at: row.try_get("expires_at")?,
                upload_only: row.try_get("upload_only")?,
                access_count: row.try_get("access_count")?,
                created_at: row.try_get("created_at")?,
                revoked_at: row.try_get("revoked_at")?,
                tenant_id: row.try_get("tenant_id")?,
            });
        }

        Ok(shares)
    }

    /// Get all active public shares created by a specific user, with file names.
    pub async fn get_user_public_shares(&self, user_id: Uuid) -> Result<Vec<OwnedPublicShare>> {
        let rows = sqlx::query(
            r#"
            SELECT
                s.id,
                s.file_id,
                s.folder_id,
                s.share_token,
                s.recipient_user_id,
                s.recipient_group_id,
                s.created_by,
                s.permissions,
                s.password_hash,
                s.expires_at,
                s.upload_only,
                s.access_count,
                s.created_at,
                s.revoked_at,
                s.tenant_id,
                COALESCE(s.file_id, s.folder_id) AS resource_id,
                CASE
                    WHEN s.file_id IS NOT NULL THEN 'file'
                    ELSE 'folder'
                END AS resource_type,
                COALESCE(f.name, fo.name) AS resource_name
            FROM shares s
            LEFT JOIN files f ON f.id = s.file_id
            LEFT JOIN folders fo ON fo.id = s.folder_id
            WHERE s.created_by = $1
              AND s.recipient_user_id IS NULL
              AND s.recipient_group_id IS NULL
              AND s.revoked_at IS NULL
            ORDER BY s.created_at DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;

        let mut shares = Vec::with_capacity(rows.len());
        for row in rows {
            let permissions_str: String = row.try_get("permissions")?;
            let permissions = Self::permission_from_db_value(&permissions_str);

            shares.push(OwnedPublicShare {
                share: Share {
                    id: row.try_get("id")?,
                    file_id: row.try_get("file_id")?,
                    folder_id: row.try_get("folder_id")?,
                    share_token: row.try_get("share_token")?,
                    recipient_user_id: row.try_get("recipient_user_id")?,
                    recipient_group_id: row.try_get("recipient_group_id")?,
                    created_by: row.try_get("created_by")?,
                    permissions,
                    password_hash: row.try_get("password_hash")?,
                    expires_at: row.try_get("expires_at")?,
                    upload_only: row.try_get("upload_only")?,
                    access_count: row.try_get("access_count")?,
                    created_at: row.try_get("created_at")?,
                    revoked_at: row.try_get("revoked_at")?,
                    tenant_id: row.try_get("tenant_id")?,
                },
                resource_id: row.try_get("resource_id")?,
                resource_type: row.try_get("resource_type")?,
                resource_name: row.try_get("resource_name")?,
            });
        }

        Ok(shares)
    }

    /// Get all active shares created by a specific user (public, user, and group shares).
    pub async fn get_user_all_shares(&self, user_id: Uuid) -> Result<Vec<OwnedPublicShare>> {
        let rows = sqlx::query(
            r#"
            SELECT
                s.id,
                s.file_id,
                s.folder_id,
                s.share_token,
                s.recipient_user_id,
                s.recipient_group_id,
                s.created_by,
                s.permissions,
                s.password_hash,
                s.expires_at,
                s.upload_only,
                s.access_count,
                s.created_at,
                s.revoked_at,
                s.tenant_id,
                COALESCE(s.file_id, s.folder_id) AS resource_id,
                CASE
                    WHEN s.file_id IS NOT NULL THEN 'file'
                    ELSE 'folder'
                END AS resource_type,
                COALESCE(f.name, fo.name) AS resource_name
            FROM shares s
            LEFT JOIN files f ON f.id = s.file_id
            LEFT JOIN folders fo ON fo.id = s.folder_id
            WHERE s.created_by = $1
              AND s.revoked_at IS NULL
            ORDER BY s.created_at DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;

        let mut shares = Vec::with_capacity(rows.len());
        for row in rows {
            let permissions_str: String = row.try_get("permissions")?;
            let permissions = Self::permission_from_db_value(&permissions_str);

            shares.push(OwnedPublicShare {
                share: Share {
                    id: row.try_get("id")?,
                    file_id: row.try_get("file_id")?,
                    folder_id: row.try_get("folder_id")?,
                    share_token: row.try_get("share_token")?,
                    recipient_user_id: row.try_get("recipient_user_id")?,
                    recipient_group_id: row.try_get("recipient_group_id")?,
                    created_by: row.try_get("created_by")?,
                    permissions,
                    password_hash: row.try_get("password_hash")?,
                    expires_at: row.try_get("expires_at")?,
                    upload_only: row.try_get("upload_only")?,
                    access_count: row.try_get("access_count")?,
                    created_at: row.try_get("created_at")?,
                    revoked_at: row.try_get("revoked_at")?,
                    tenant_id: row.try_get("tenant_id")?,
                },
                resource_id: row.try_get("resource_id")?,
                resource_type: row.try_get("resource_type")?,
                resource_name: row.try_get("resource_name")?,
            });
        }

        Ok(shares)
    }

    /// Get access-log entries for a public share owned by a specific user.
    pub async fn get_public_share_access_log(
        &self,
        share_id: Uuid,
        owner_id: Uuid,
        limit: i64,
    ) -> Result<Vec<PublicShareAccessLogEntry>> {
        let rows = sqlx::query(
            r#"
            SELECT
                sal.accessed_at,
                sal.action,
                sal.success,
                sal.ip_address,
                sal.user_agent,
                sal.actor_type,
                sal.actor_label,
                sal.share_session_id,
                sal.share_session_subject
            FROM share_access_log sal
            INNER JOIN shares s ON s.id = sal.share_id
            WHERE sal.share_id = $1
              AND s.created_by = $2
              AND s.recipient_user_id IS NULL
            ORDER BY sal.accessed_at DESC
            LIMIT $3
            "#,
        )
        .bind(share_id)
        .bind(owner_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|row| {
                Ok(PublicShareAccessLogEntry {
                    accessed_at: row.try_get("accessed_at")?,
                    action: row.try_get("action")?,
                    success: row.try_get("success")?,
                    ip_address: row.try_get("ip_address")?,
                    user_agent: row.try_get("user_agent")?,
                    actor_type: row.try_get("actor_type")?,
                    actor_label: row.try_get("actor_label")?,
                    share_session_id: row.try_get("share_session_id")?,
                    share_session_subject: row.try_get("share_session_subject")?,
                })
            })
            .collect()
    }

    /// Update a share's password and expiration
    pub async fn update_share(&self, share: &Share) -> Result<()> {
        // TODO: Switch to sqlx::query!() after Docker Compose setup (Task 11)
        sqlx::query(
            r#"
            UPDATE shares
            SET password_hash = $2, expires_at = $3, tenant_id = $4
            WHERE id = $1 AND created_by = $5
            "#,
        )
        .bind(share.id)
        .bind(&share.password_hash)
        .bind(share.expires_at)
        .bind(share.tenant_id)
        .bind(share.created_by)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Revoke a share link (soft delete)
    pub async fn revoke_share(&self, share_id: Uuid, actor_id: Uuid) -> Result<()> {
        // TODO: Switch to sqlx::query!() after Docker Compose setup (Task 11)
        sqlx::query(
            r#"
            UPDATE shares
            SET revoked_at = NOW()
            WHERE id = $1 AND created_by = $2
            "#,
        )
        .bind(share_id)
        .bind(actor_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Increment share access count and update last_accessed_at
    pub async fn increment_share_access(&self, share_id: Uuid) -> Result<()> {
        // TODO: Switch to sqlx::query!() after Docker Compose setup (Task 11)
        sqlx::query(
            r#"
            UPDATE shares
            SET access_count = access_count + 1, last_accessed_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(share_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Log a share access attempt
    pub async fn log_share_access(&self, entry: ShareAccessLogEntry) -> Result<()> {
        // TODO: Switch to sqlx::query!() after Docker Compose setup (Task 11)
        // Validate IP address format before storage
        let validated_ip = entry
            .ip_address
            .and_then(|ip| ip.parse::<std::net::IpAddr>().ok().map(|_| ip));

        sqlx::query(
            r#"
            INSERT INTO share_access_log (
                share_id, ip_address, user_agent, action, success,
                actor_type, actor_label, share_session_id, share_session_subject
            )
            VALUES ($1, $2::inet, $3, $4, $5, $6, $7, $8, $9)
            "#,
        )
        .bind(entry.share_id)
        .bind(validated_ip)
        .bind(entry.user_agent)
        .bind(entry.action)
        .bind(entry.success)
        .bind(entry.actor_type)
        .bind(entry.actor_label)
        .bind(entry.share_session_id)
        .bind(entry.share_session_subject)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// List all markdown files for a user across their entire library.
    pub async fn list_all_folders(&self, owner_id: Uuid, tenant_id: Uuid) -> Result<Vec<Folder>> {
        let rows = sqlx::query(
            r#"
            SELECT id, name, path, parent_folder_id, owner_id, created_at, updated_at, starred_at, deleted_at, tenant_id
            FROM folders
            WHERE owner_id = $1
              AND tenant_id = $2
              AND deleted_at IS NULL
            ORDER BY path ASC
            "#,
        )
        .bind(owner_id)
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await?;

        let mut folders = Vec::new();
        for row in rows {
            let folder = Folder {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                path: row.try_get("path")?,
                parent_folder_id: row.try_get("parent_folder_id")?,
                owner_id: row.try_get("owner_id")?,
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
                starred_at: row.try_get("starred_at")?,
                deleted_at: row.try_get("deleted_at")?,
                tenant_id: row.try_get("tenant_id")?,
                ancestor_ids: None,
            };
            folders.push(folder);
        }

        Ok(folders)
    }

    pub async fn list_all_markdown_files(
        &self,
        owner_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<File>> {
        let rows = sqlx::query(
            r#"
            SELECT id, name, path, size, mime_type, content_hash, owner_id, parent_folder_id, current_version, created_at, modified_at, starred_at, deleted_at, tenant_id
            FROM files
            WHERE owner_id = $1
              AND tenant_id = $2
              AND deleted_at IS NULL
              AND (mime_type = 'text/markdown' OR name ILIKE '%.md')
            ORDER BY modified_at DESC
            "#,
        )
        .bind(owner_id)
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await?;

        let mut files = Vec::new();
        for row in rows {
            let file = File {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                path: row.try_get("path")?,
                size: row.try_get("size")?,
                mime_type: row.try_get("mime_type")?,
                content_hash: row.try_get("content_hash")?,
                owner_id: row.try_get("owner_id")?,
                parent_folder_id: row.try_get("parent_folder_id")?,
                current_version: row.try_get("current_version")?,
                created_at: row.try_get("created_at")?,
                modified_at: row.try_get("modified_at")?,
                starred_at: row.try_get("starred_at")?,
                deleted_at: row.try_get("deleted_at")?,
                tenant_id: row.try_get("tenant_id")?,
            };
            files.push(file);
        }

        Ok(files)
    }

    /// Check if a user is a member of a group.
    pub async fn is_user_in_group(&self, user_id: Uuid, group_id: Uuid) -> Result<bool> {
        let exists = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM group_members
                WHERE group_id = $1 AND user_id = $2
            )
            "#,
        )
        .bind(group_id)
        .bind(user_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(exists)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustshare_core::domain::{File, FileVersion, Folder, Share, SharePermissions, User};

    const TEST_DATABASE_URL: &str = "postgres://rustshare:changeme@localhost:5432/rustshare";

    async fn setup_test_db() -> PgPool {
        let database_url =
            std::env::var("DATABASE_URL").unwrap_or_else(|_| TEST_DATABASE_URL.to_string());
        PgPool::connect(&database_url).await.unwrap()
    }

    async fn setup_metadata_store() -> (MetadataStore, PgPool) {
        let pool = setup_test_db().await;
        let store = MetadataStore::new(pool.clone());
        (store, pool)
    }

    #[tokio::test]
    #[ignore] // Requires database
    async fn test_create_and_find_user() {
        let pool = setup_test_db().await;
        let store = MetadataStore::new(pool.clone());
        let tenant_id = Uuid::new_v4();

        let user = User::new(
            "testuser".to_string(),
            "Test User".to_string(),
            "hash123".to_string(),
            "test@example.com".to_string(),
            false,
            10_737_418_240, // 10GB
            tenant_id,
        );

        store.create_user(&user).await.unwrap();

        let found = store.find_user_by_email("test@example.com").await.unwrap();
        assert!(found.is_some());
        let found_user = found.unwrap();
        assert_eq!(found_user.email, "test@example.com");
        assert_eq!(found_user.username, "testuser");

        // Cleanup
        sqlx::query("DELETE FROM users WHERE email = $1")
            .bind("test@example.com")
            .execute(&pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    #[ignore] // Requires DATABASE_URL
    async fn test_file_crud() {
        let (store, pool) = setup_metadata_store().await;
        let tenant_id = Uuid::new_v4();

        // First create a user to own the file
        let owner = User::new(
            "fileowner".to_string(),
            "File Owner".to_string(),
            "hash456".to_string(),
            "fileowner@example.com".to_string(),
            false,
            10_737_418_240,
            tenant_id,
        );
        store.create_user(&owner).await.unwrap();

        // Create a file
        let file = File::new(
            "test-document.pdf".to_string(),
            "/Documents/test-document.pdf".to_string(),
            "abc123def456hash".to_string(),
            2048,
            "application/pdf".to_string(),
            None, // No parent folder
            owner.id,
            tenant_id,
        );

        // Test: create_file
        store.create_file(&file).await.unwrap();

        // Test: find_file_by_id
        let found = store.find_file_by_id(file.id).await.unwrap();
        assert!(found.is_some());
        let found_file = found.unwrap();
        assert_eq!(found_file.id, file.id);
        assert_eq!(found_file.name, "test-document.pdf");
        assert_eq!(found_file.path, "/Documents/test-document.pdf");
        assert_eq!(found_file.content_hash, "abc123def456hash");
        assert_eq!(found_file.size, 2048);
        assert_eq!(found_file.mime_type, "application/pdf");
        assert_eq!(found_file.owner_id, owner.id);
        assert_eq!(found_file.current_version, 1);

        // Test: update_file (modify name and size)
        let mut updated_file = found_file.clone();
        updated_file.name = "renamed-document.pdf".to_string();
        updated_file.size = 4096;
        store.update_file(&updated_file).await.unwrap();

        let found_updated = store.find_file_by_id(file.id).await.unwrap().unwrap();
        assert_eq!(found_updated.name, "renamed-document.pdf");
        assert_eq!(found_updated.size, 4096);

        // Test: list_files (with no parent_id filter)
        let files = store.list_files(None, owner.id, tenant_id).await.unwrap();
        assert!(!files.is_empty());
        assert!(files.iter().any(|f| f.id == file.id));

        // Test: delete_file
        store.delete_file(file.id).await.unwrap();
        let not_found = store.find_file_by_id(file.id).await.unwrap();
        assert!(not_found.is_none());

        // Cleanup user
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(owner.id)
            .execute(&pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    #[ignore] // Requires DATABASE_URL
    async fn test_file_versions() {
        let (store, pool) = setup_metadata_store().await;
        let tenant_id = Uuid::new_v4();

        // First create a user to own the file
        let user = User::new(
            "versionuser".to_string(),
            "Version User".to_string(),
            "hash789".to_string(),
            "versionuser@example.com".to_string(),
            false,
            10_737_418_240,
            tenant_id,
        );
        store.create_user(&user).await.unwrap();

        // Create a file
        let file = File::new(
            "versioned-doc.txt".to_string(),
            "/Documents/versioned-doc.txt".to_string(),
            "hash1".to_string(),
            100,
            "text/plain".to_string(),
            None,
            user.id,
            tenant_id,
        );
        store.create_file(&file).await.unwrap();

        // Create file version 1
        let version1 = FileVersion::new(
            file.id,
            1,
            "hash1".to_string(),
            100,
            user.id,
            Some("Initial version".to_string()),
            tenant_id,
        );
        store.create_file_version(&version1).await.unwrap();

        // Create file version 2
        let version2 = FileVersion::new(
            file.id,
            2,
            "hash2".to_string(),
            200,
            user.id,
            Some("Second version".to_string()),
            tenant_id,
        );
        store.create_file_version(&version2).await.unwrap();

        // Create file version 3
        let version3 = FileVersion::new(
            file.id,
            3,
            "hash3".to_string(),
            300,
            user.id,
            None,
            tenant_id,
        );
        store.create_file_version(&version3).await.unwrap();

        // Test: list_file_versions (should be in DESC order: 3, 2, 1)
        let versions = store.list_file_versions(file.id).await.unwrap();
        assert_eq!(versions.len(), 3);
        assert_eq!(versions[0].version_number, 3);
        assert_eq!(versions[1].version_number, 2);
        assert_eq!(versions[2].version_number, 1);
        assert_eq!(versions[0].content_hash, "hash3");
        assert_eq!(versions[1].content_hash, "hash2");
        assert_eq!(versions[2].content_hash, "hash1");

        // Test: find_file_version (find version 2)
        let found_version = store.find_file_version(file.id, 2).await.unwrap();
        assert!(found_version.is_some());
        let found = found_version.unwrap();
        assert_eq!(found.version_number, 2);
        assert_eq!(found.content_hash, "hash2");
        assert_eq!(found.size, 200);
        assert_eq!(found.created_by, user.id);
        assert_eq!(found.change_description, Some("Second version".to_string()));

        // Test: find_file_version (non-existent version)
        let not_found = store.find_file_version(file.id, 99).await.unwrap();
        assert!(not_found.is_none());

        // Cleanup (file_versions will cascade delete with file)
        sqlx::query("DELETE FROM files WHERE id = $1")
            .bind(file.id)
            .execute(&pool)
            .await
            .unwrap();

        // Cleanup user
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user.id)
            .execute(&pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    #[ignore] // Requires DATABASE_URL
    async fn test_folder_crud() {
        let (store, pool) = setup_metadata_store().await;
        let tenant_id = Uuid::new_v4();

        // First create a user to own the folders
        let owner = User::new(
            "folderowner".to_string(),
            "Folder Owner".to_string(),
            "hashabc".to_string(),
            "folderowner@example.com".to_string(),
            false,
            10_737_418_240,
            tenant_id,
        );
        store.create_user(&owner).await.unwrap();

        // Test: create_folder (root folder)
        let root_folder = Folder::new_root(owner.id, tenant_id);
        store.create_folder(&root_folder).await.unwrap();

        // Test: find_folder_by_id
        let found = store.find_folder_by_id(root_folder.id).await.unwrap();
        assert!(found.is_some());
        let found_folder = found.unwrap();
        assert_eq!(found_folder.id, root_folder.id);
        assert_eq!(found_folder.name, "Root");
        assert_eq!(found_folder.path, "/Root");
        assert_eq!(found_folder.parent_folder_id, None);
        assert_eq!(found_folder.owner_id, owner.id);

        // Test: create_folder (child folder - Documents)
        let docs_folder = Folder::new_child(
            "Documents".to_string(),
            "/Documents".to_string(),
            root_folder.id,
            owner.id,
            tenant_id,
        );
        store.create_folder(&docs_folder).await.unwrap();

        // Test: create_folder (child folder - Photos)
        let photos_folder = Folder::new_child(
            "Photos".to_string(),
            "/Photos".to_string(),
            root_folder.id,
            owner.id,
            tenant_id,
        );
        store.create_folder(&photos_folder).await.unwrap();

        // Test: create_folder (nested folder - Documents/Work)
        let work_folder = Folder::new_child(
            "Work".to_string(),
            "/Documents/Work".to_string(),
            docs_folder.id,
            owner.id,
            tenant_id,
        );
        store.create_folder(&work_folder).await.unwrap();

        // Test: create_folder (deeply nested folder - Documents/Work/Projects)
        let projects_folder = Folder::new_child(
            "Projects".to_string(),
            "/Documents/Work/Projects".to_string(),
            work_folder.id,
            owner.id,
            tenant_id,
        );
        store.create_folder(&projects_folder).await.unwrap();

        // Test: list_folders (root level - should return Documents and Photos)
        let root_children = store
            .list_folders(Some(root_folder.id), owner.id, tenant_id)
            .await
            .unwrap();
        assert_eq!(root_children.len(), 2);
        assert!(root_children.iter().any(|f| f.name == "Documents"));
        assert!(root_children.iter().any(|f| f.name == "Photos"));

        // Test: list_folders (Documents children - should return Work)
        let docs_children = store
            .list_folders(Some(docs_folder.id), owner.id, tenant_id)
            .await
            .unwrap();
        assert_eq!(docs_children.len(), 1);
        assert_eq!(docs_children[0].name, "Work");

        // Test: list_folders (no parent - should return root folder)
        let root_folders = store.list_folders(None, owner.id, tenant_id).await.unwrap();
        assert_eq!(root_folders.len(), 1);
        assert_eq!(root_folders[0].name, "Root");

        // Test: find_descendant_folders (should find all descendants of Documents)
        let descendants = store.find_descendant_folders(docs_folder.id).await.unwrap();
        // Should include: Documents, Work, Projects (3 folders)
        assert_eq!(descendants.len(), 3);
        assert!(descendants.iter().any(|f| f.name == "Documents"));
        assert!(descendants.iter().any(|f| f.name == "Work"));
        assert!(descendants.iter().any(|f| f.name == "Projects"));

        // Test: find_descendant_folders (leaf folder should only return itself)
        let leaf_descendants = store
            .find_descendant_folders(projects_folder.id)
            .await
            .unwrap();
        assert_eq!(leaf_descendants.len(), 1);
        assert_eq!(leaf_descendants[0].name, "Projects");

        // Test: update_folder (rename Photos to Pictures)
        let mut updated_photos = photos_folder.clone();
        updated_photos.name = "Pictures".to_string();
        updated_photos.path = "/Pictures".to_string();
        updated_photos.updated_at = chrono::Utc::now();
        store.update_folder(&updated_photos).await.unwrap();

        let found_updated = store
            .find_folder_by_id(photos_folder.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(found_updated.name, "Pictures");
        assert_eq!(found_updated.path, "/Pictures");

        // Test: delete_folder (delete leaf folder first)
        store.delete_folder(projects_folder.id).await.unwrap();
        let not_found = store.find_folder_by_id(projects_folder.id).await.unwrap();
        assert!(not_found.is_none());

        // Verify descendants updated after deletion
        let updated_descendants = store.find_descendant_folders(docs_folder.id).await.unwrap();
        assert_eq!(updated_descendants.len(), 2); // Only Documents and Work remain
        assert!(!updated_descendants.iter().any(|f| f.name == "Projects"));

        // Cleanup: Delete folders (cascade will handle children)
        // Delete in order: leaf -> parent
        sqlx::query("DELETE FROM folders WHERE id = $1")
            .bind(work_folder.id)
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query("DELETE FROM folders WHERE id = $1")
            .bind(docs_folder.id)
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query("DELETE FROM folders WHERE id = $1")
            .bind(photos_folder.id)
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query("DELETE FROM folders WHERE id = $1")
            .bind(root_folder.id)
            .execute(&pool)
            .await
            .unwrap();

        // Cleanup user
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(owner.id)
            .execute(&pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    #[ignore] // Requires DATABASE_URL
    async fn test_share_crud() {
        let (store, pool) = setup_metadata_store().await;
        let tenant_id = Uuid::new_v4();

        // First create a user to own the file
        let owner = User::new(
            "shareowner".to_string(),
            "Share Owner".to_string(),
            "hashxyz".to_string(),
            "shareowner@example.com".to_string(),
            false,
            10_737_418_240,
            tenant_id,
        );
        store.create_user(&owner).await.unwrap();

        // Create a file to share
        let file = File::new(
            "shareable-document.pdf".to_string(),
            "/Documents/shareable-document.pdf".to_string(),
            "abcdef123456hash".to_string(),
            3072,
            "application/pdf".to_string(),
            None,
            owner.id,
            tenant_id,
        );
        store.create_file(&file).await.unwrap();

        // Test: create_share
        let share_token = Uuid::new_v4().to_string();
        let share = Share::new(
            file.id,
            share_token.clone(),
            owner.id,
            SharePermissions::View,
            Some("hashed_password".to_string()),
            None,
            tenant_id,
        );
        store.create_share(&share).await.unwrap();

        // Test: get_share_by_token
        let found_by_token = store.get_share_by_token(&share_token).await.unwrap();
        assert!(found_by_token.is_some());
        let found_share = found_by_token.unwrap();
        assert_eq!(found_share.id, share.id);
        assert_eq!(found_share.share_token, Some(share_token.clone()));
        assert_eq!(found_share.file_id, Some(file.id));
        assert_eq!(found_share.permissions, SharePermissions::View);
        assert_eq!(
            found_share.password_hash,
            Some("hashed_password".to_string())
        );
        assert_eq!(found_share.access_count, 0);

        // Test: get_share
        let found_by_id = store.get_share(share.id).await.unwrap();
        assert!(found_by_id.is_some());
        let found_share_by_id = found_by_id.unwrap();
        assert_eq!(found_share_by_id.id, share.id);
        assert_eq!(found_share_by_id.share_token, Some(share_token.clone()));

        // Create a second share for the same file
        let share_token_2 = Uuid::new_v4().to_string();
        let share2 = Share::new(
            file.id,
            share_token_2.clone(),
            owner.id,
            SharePermissions::Edit,
            None,
            None,
            tenant_id,
        );
        store.create_share(&share2).await.unwrap();

        // Test: get_file_shares
        let file_shares = store.get_file_shares(file.id).await.unwrap();
        assert_eq!(file_shares.len(), 2);
        assert!(file_shares
            .iter()
            .any(|s| s.share_token == Some(share_token.clone())));
        assert!(file_shares
            .iter()
            .any(|s| s.share_token == Some(share_token_2.clone())));

        // Test: increment_share_access
        store.increment_share_access(share.id).await.unwrap();
        let updated = store.get_share(share.id).await.unwrap().unwrap();
        assert_eq!(updated.access_count, 1);

        // Test: log_share_access
        store
            .log_share_access(ShareAccessLogEntry {
                share_id: share.id,
                ip_address: Some("192.168.1.1".to_string()),
                user_agent: Some("Mozilla/5.0".to_string()),
                action: "access".to_string(),
                success: true,
                actor_type: Some("public_share_session".to_string()),
                actor_label: Some("Uploader".to_string()),
                share_session_id: Some(Uuid::new_v4()),
                share_session_subject: Some("share:test".to_string()),
            })
            .await
            .unwrap();

        // Test: update_share
        let mut updated_share = found_share.clone();
        updated_share.password_hash = Some("new_hashed_password".to_string());
        store.update_share(&updated_share).await.unwrap();

        let after_update = store.get_share(share.id).await.unwrap().unwrap();
        assert_eq!(
            after_update.password_hash,
            Some("new_hashed_password".to_string())
        );

        // Test: revoke_share
        store.revoke_share(share.id).await.unwrap();

        // After revoke, share should not appear in get_file_shares (only active shares)
        let active_shares = store.get_file_shares(file.id).await.unwrap();
        assert_eq!(active_shares.len(), 1);
        assert!(active_shares
            .iter()
            .all(|s| s.share_token == Some(share_token_2.clone())));

        // But should still be retrievable by ID
        let revoked_share = store.get_share(share.id).await.unwrap();
        assert!(revoked_share.is_some());

        // Cleanup
        sqlx::query("DELETE FROM shares WHERE file_id = $1")
            .bind(file.id)
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query("DELETE FROM files WHERE id = $1")
            .bind(file.id)
            .execute(&pool)
            .await
            .unwrap();

        // Cleanup user
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(owner.id)
            .execute(&pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    #[ignore] // Requires database
    async fn test_get_public_shares_excludes_group_shares() {
        let (store, pool) = setup_metadata_store().await;

        // Create test user and group
        let user_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        // Create user
        let owner = User::new(
            format!("testowner_{}", user_id),
            "Test Owner".to_string(),
            "hash123".to_string(),
            format!("testowner_{}@example.com", user_id),
            false,
            10_737_418_240,
            tenant_id,
        );
        store.create_user(&owner).await.unwrap();

        // Create file
        let file = File::new(
            "test-document.pdf".to_string(),
            "/Documents/test-document.pdf".to_string(),
            "content_hash".to_string(),
            1024,
            "application/pdf".to_string(),
            None,
            owner.id,
            tenant_id,
        );
        store.create_file(&file).await.unwrap();

        // Create public share
        let public_share = Share {
            id: Uuid::new_v4(),
            file_id: Some(file.id),
            folder_id: None,
            share_token: Some(Uuid::new_v4().to_string()),
            permissions: SharePermissions::View,
            password_hash: None,
            expires_at: None,
            upload_only: false,
            access_count: 0,
            recipient_user_id: None,
            recipient_group_id: None,
            created_by: owner.id,
            created_at: Utc::now(),
            revoked_at: None,
            tenant_id,
        };
        store.create_share(&public_share).await.unwrap();

        // Create backing group row for the FK used by recipient_group_id.
        let group_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO user_groups (id, name, description, created_by)
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(group_id)
        .bind(format!("test-group-{}", group_id))
        .bind(Some("Test group".to_string()))
        .bind(owner.id)
        .execute(&pool)
        .await
        .unwrap();

        // Create group share (same file)
        let group_share = Share {
            id: Uuid::new_v4(),
            file_id: Some(file.id),
            folder_id: None,
            share_token: None,
            permissions: SharePermissions::Edit,
            password_hash: None,
            expires_at: None,
            upload_only: false,
            access_count: 0,
            recipient_user_id: None,
            recipient_group_id: Some(group_id),
            created_by: owner.id,
            created_at: Utc::now(),
            revoked_at: None,
            tenant_id,
        };
        store.create_share(&group_share).await.unwrap();

        // Query public shares
        let public_shares = store.get_user_public_shares(owner.id).await.unwrap();

        // Should only return 1 (the public share), not 2
        assert_eq!(public_shares.len(), 1);
        assert_eq!(public_shares[0].share.id, public_share.id);

        // Cleanup
        sqlx::query("DELETE FROM shares WHERE file_id = $1")
            .bind(file.id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM files WHERE id = $1")
            .bind(file.id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM user_groups WHERE id = $1")
            .bind(group_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(owner.id)
            .execute(&pool)
            .await
            .unwrap();
    }
}
