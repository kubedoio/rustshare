//! SCIM-lite provisioning service.
//!
//! Provides webhook-style SCIM operations for enterprise IdP integration.
//! This is a lightweight implementation, not full RFC 7644 compliance.

use crate::domain::User;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Errors that can occur during SCIM operations.
#[derive(Debug, Error)]
pub enum ScimError {
    #[error("User not found: {0}")]
    UserNotFound(String),
    #[error("Group not found: {0}")]
    GroupNotFound(String),
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("Invalid request: {0}")]
    InvalidRequest(String),
    #[error("Conflict: {0}")]
    Conflict(String),
}

/// A SCIM group write failure tagged with whether any statement committed
/// before the error was raised.
///
/// Group writes commit per statement, so a failure can follow committed
/// changes (e.g. the group INSERT succeeded and a later member lookup
/// failed). Callers that act after a failure — such as the best-effort index
/// ACL refresh — must only run when `writes_committed` is true; a rejection
/// that committed nothing must not dispatch follow-up work.
#[derive(Debug)]
pub struct ScimWriteError {
    pub error: ScimError,
    pub writes_committed: bool,
}

impl ScimWriteError {
    fn pre_write(error: ScimError) -> Self {
        Self {
            error,
            writes_committed: false,
        }
    }

    fn post_commit(error: ScimError) -> Self {
        Self {
            error,
            writes_committed: true,
        }
    }
}

impl std::fmt::Display for ScimWriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.error.fmt(f)
    }
}

impl std::error::Error for ScimWriteError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

/// Converts an error through the blanket `From` impls only via `?` in
/// **pre-write** positions — before the first statement that can commit.
/// Once any write has committed, errors must be constructed explicitly with
/// `writes_committed: true` (see [`ScimWriteError::post_commit`]); a `?`
/// added after a committing statement would silently default to
/// `writes_committed: false` and wrongly skip post-failure refresh.
impl From<ScimError> for ScimWriteError {
    // Conservative default: an unclassified error is treated as pre-write.
    // Only apply `?` via this conversion BEFORE the first committing
    // statement; errors after any committed write must be constructed
    // explicitly with writes_committed: true.
    fn from(error: ScimError) -> Self {
        Self::pre_write(error)
    }
}

/// Converts an error through the blanket `From` impls only via `?` in
/// **pre-write** positions — before the first statement that can commit.
/// Once any write has committed, errors must be constructed explicitly with
/// `writes_committed: true` (see [`ScimWriteError::post_commit`]); a `?`
/// added after a committing statement would silently default to
/// `writes_committed: false` and wrongly skip post-failure refresh.
impl From<sqlx::Error> for ScimWriteError {
    // Conservative default: an unclassified error is treated as pre-write.
    // Only apply `?` via this conversion BEFORE the first committing
    // statement; errors after any committed write must be constructed
    // explicitly with writes_committed: true.
    fn from(error: sqlx::Error) -> Self {
        Self::pre_write(error.into())
    }
}

/// SCIM Name complex type.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScimName {
    #[serde(rename = "givenName")]
    pub given_name: Option<String>,
    #[serde(rename = "familyName")]
    pub family_name: Option<String>,
}

/// SCIM Email complex type.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScimEmail {
    pub value: String,
    #[serde(default)]
    pub primary: bool,
}

/// SCIM User resource.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScimUser {
    #[serde(rename = "externalId")]
    pub external_id: String,
    #[serde(rename = "userName")]
    pub user_name: String,
    pub name: Option<ScimName>,
    pub emails: Option<Vec<ScimEmail>>,
    pub active: bool,
    #[serde(rename = "tenantId")]
    pub tenant_id: Option<Uuid>,
}

/// SCIM Group member reference.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScimMember {
    pub value: String, // external_id of the user
}

/// SCIM Group resource.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScimGroup {
    #[serde(rename = "externalId")]
    pub external_id: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
    pub members: Option<Vec<ScimMember>>,
}

/// Result of a SCIM user provisioning operation.
#[derive(Debug, Serialize)]
pub struct ScimUserResult {
    pub id: Uuid,
    pub external_id: String,
    pub action: ScimAction,
}

/// Result of a SCIM group provisioning operation.
#[derive(Debug, Serialize)]
pub struct ScimGroupResult {
    pub id: Uuid,
    pub external_id: String,
    pub action: ScimAction,
}

/// Action taken during SCIM provisioning.
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScimAction {
    Created,
    Updated,
}

/// Repository operations required for SCIM service.
///
/// Every operation is scoped to a single organization (`tenant_id`): the
/// SCIM integration is bound to exactly one organization and must never
/// reach across it.
#[allow(async_fn_in_trait)]
pub trait ScimRepository: Send + Sync {
    /// Find user by external_id within the organization.
    async fn find_user_by_external_id(
        &self,
        tenant_id: Uuid,
        external_id: &str,
    ) -> Result<Option<User>, sqlx::Error>;

    /// Create a new user in the organization.
    async fn create_user(&self, user: &User, external_id: &str) -> Result<(), sqlx::Error>;

    /// Update an existing user within the organization.
    #[allow(clippy::too_many_arguments)]
    async fn update_user(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
        display_name: &str,
        email: &str,
        name: Option<&str>,
        surname: Option<&str>,
        disabled_at: Option<DateTime<Utc>>,
    ) -> Result<(), sqlx::Error>;

    /// Set user's disabled_at timestamp within the organization.
    async fn set_user_disabled(
        &self,
        tenant_id: Uuid,
        external_id: &str,
        disabled: bool,
    ) -> Result<(), sqlx::Error>;

    /// Find group by external_id within the organization.
    async fn find_group_by_external_id(
        &self,
        tenant_id: Uuid,
        external_id: &str,
    ) -> Result<Option<GroupRecord>, sqlx::Error>;

    /// Create a new group in the organization.
    async fn create_group(
        &self,
        tenant_id: Uuid,
        external_id: &str,
        display_name: &str,
    ) -> Result<Uuid, sqlx::Error>;

    /// Update an existing group within the organization.
    async fn update_group(
        &self,
        tenant_id: Uuid,
        external_id: &str,
        display_name: &str,
    ) -> Result<(), sqlx::Error>;

    /// Delete a group within the organization.
    async fn delete_group(&self, tenant_id: Uuid, external_id: &str) -> Result<(), sqlx::Error>;

    /// Find user ID by external_id within the organization.
    async fn find_user_id_by_external_id(
        &self,
        tenant_id: Uuid,
        external_id: &str,
    ) -> Result<Option<Uuid>, sqlx::Error>;

    /// Get current group members within the organization.
    async fn get_group_members(
        &self,
        tenant_id: Uuid,
        group_id: Uuid,
    ) -> Result<Vec<Uuid>, sqlx::Error>;

    /// Add member to group; both must belong to the organization.
    async fn add_group_member(
        &self,
        tenant_id: Uuid,
        group_id: Uuid,
        user_id: Uuid,
    ) -> Result<(), sqlx::Error>;

    /// Remove member from group within the organization.
    async fn remove_group_member(
        &self,
        tenant_id: Uuid,
        group_id: Uuid,
        user_id: Uuid,
    ) -> Result<(), sqlx::Error>;

    /// Clear all members from a group within the organization.
    async fn clear_group_members(&self, tenant_id: Uuid, group_id: Uuid)
        -> Result<(), sqlx::Error>;
}

/// Map a collision with the installation-wide unique directory identifiers
/// (SQLSTATE 23505) to a generic conflict that does not name the database
/// constraint or reveal which organization owns the existing row.
fn directory_conflict(resource: &str, e: sqlx::Error) -> ScimError {
    if let sqlx::Error::Database(db) = &e {
        if db.code().as_deref() == Some("23505") {
            return ScimError::Conflict(format!(
                "a {resource} with the same identifying value already exists in the directory"
            ));
        }
    }
    ScimError::Database(e)
}

/// Group record for SCIM operations.
#[derive(Debug, Clone)]
pub struct GroupRecord {
    pub id: Uuid,
    pub external_id: String,
    pub name: String,
}

/// SCIM provisioning service.
///
/// `tenant_scope` is the single organization this integration is bound to
/// (ADR-0040 D5). It is resolved from server configuration by the HTTP layer;
/// request input can never widen or redirect it.
pub struct ScimService<R: ScimRepository> {
    repository: Arc<R>,
    tenant_scope: Uuid,
    default_storage_quota: i64,
}

impl<R: ScimRepository> ScimService<R> {
    /// Create a new SCIM service scoped to one organization.
    pub fn new(repository: Arc<R>, tenant_scope: Uuid, default_storage_quota: i64) -> Self {
        Self {
            repository,
            tenant_scope,
            default_storage_quota,
        }
    }

    /// Provision or update a user from SCIM data.
    pub async fn provision_user(&self, scim_user: ScimUser) -> Result<ScimUserResult, ScimError> {
        debug!("Provisioning SCIM user: {}", scim_user.external_id);

        // The integration is bound to exactly one organization. A
        // request-supplied tenantId must equal that scope or the request is
        // rejected; it can never widen or redirect the write.
        let tenant_id = self.tenant_scope;
        if let Some(requested) = scim_user.tenant_id {
            if requested != tenant_id {
                warn!(
                    external_id = %scim_user.external_id,
                    requested = %requested,
                    scope = %tenant_id,
                    "SCIM provision rejected: tenantId outside the configured organization scope"
                );
                return Err(ScimError::InvalidRequest(
                    "tenantId does not match the configured SCIM organization scope".to_string(),
                ));
            }
        }

        // Extract email from SCIM emails (prefer primary, fallback to first)
        let email = scim_user
            .emails
            .as_ref()
            .and_then(|emails| {
                emails
                    .iter()
                    .find(|e| e.primary)
                    .or_else(|| emails.first())
                    .map(|e| e.value.clone())
            })
            .unwrap_or_else(|| scim_user.user_name.clone());

        // Extract display name from SCIM name components or userName
        let display_name = scim_user
            .name
            .as_ref()
            .map(|n| {
                let given = n.given_name.as_deref().unwrap_or("");
                let family = n.family_name.as_deref().unwrap_or("");
                let full = format!("{} {}", given, family).trim().to_string();
                if full.is_empty() {
                    scim_user.user_name.clone()
                } else {
                    full
                }
            })
            .unwrap_or_else(|| scim_user.user_name.clone());

        // Extract first/last name
        let name = scim_user.name.as_ref().and_then(|n| n.given_name.clone());
        let surname = scim_user.name.as_ref().and_then(|n| n.family_name.clone());

        // Check if user already exists by external_id within the organization
        let existing = self
            .repository
            .find_user_by_external_id(tenant_id, &scim_user.external_id)
            .await?;

        match existing {
            Some(user) => {
                // Update existing user
                info!(
                    "Updating existing SCIM user: {} (id: {})",
                    scim_user.external_id, user.id
                );

                let disabled_at = if scim_user.active {
                    None
                } else {
                    Some(Utc::now())
                };

                self.repository
                    .update_user(
                        tenant_id,
                        user.id,
                        &display_name,
                        &email,
                        name.as_deref(),
                        surname.as_deref(),
                        disabled_at,
                    )
                    .await?;

                Ok(ScimUserResult {
                    id: user.id,
                    external_id: scim_user.external_id,
                    action: ScimAction::Updated,
                })
            }
            None => {
                // Create new user
                info!("Creating new SCIM user: {}", scim_user.external_id);

                // Generate a random password (user will need to use password reset or SSO)
                let password_hash = generate_temporary_password_hash();

                let user = User::new(
                    scim_user.user_name.clone(),
                    display_name,
                    password_hash,
                    email,
                    false, // Not admin by default
                    self.default_storage_quota,
                    tenant_id,
                );

                let user_id = user.id;

                self.repository
                    .create_user(&user, &scim_user.external_id)
                    .await
                    .map_err(|e| directory_conflict("user", e))?;

                // If user should be disabled, set that now
                if !scim_user.active {
                    self.repository
                        .set_user_disabled(tenant_id, &scim_user.external_id, true)
                        .await?;
                }

                Ok(ScimUserResult {
                    id: user_id,
                    external_id: scim_user.external_id,
                    action: ScimAction::Created,
                })
            }
        }
    }

    /// Deprovision (disable) a user.
    pub async fn deprovision_user(&self, external_id: &str) -> Result<(), ScimError> {
        info!("Deprovisioning SCIM user: {}", external_id);

        let existing = self
            .repository
            .find_user_by_external_id(self.tenant_scope, external_id)
            .await?;

        if existing.is_none() {
            warn!(
                "Attempted to deprovision non-existent user: {}",
                external_id
            );
            return Err(ScimError::UserNotFound(external_id.to_string()));
        }

        self.repository
            .set_user_disabled(self.tenant_scope, external_id, true)
            .await?;

        Ok(())
    }

    /// Provision or update a group from SCIM data.
    pub async fn provision_group(
        &self,
        scim_group: ScimGroup,
    ) -> Result<ScimGroupResult, ScimWriteError> {
        debug!("Provisioning SCIM group: {}", scim_group.external_id);

        let existing = self
            .repository
            .find_group_by_external_id(self.tenant_scope, &scim_group.external_id)
            .await?;

        match existing {
            Some(group) => {
                // Update existing group
                info!(
                    "Updating existing SCIM group: {} (id: {})",
                    scim_group.external_id, group.id
                );

                // The rename is the first write; its failure commits nothing.
                self.repository
                    .update_group(
                        self.tenant_scope,
                        &scim_group.external_id,
                        &scim_group.display_name,
                    )
                    .await?;

                // The rename is committed from here on.
                if let Some(members) = scim_group.members {
                    self.sync_group_members(group.id, members).await?;
                }

                Ok(ScimGroupResult {
                    id: group.id,
                    external_id: scim_group.external_id,
                    action: ScimAction::Updated,
                })
            }
            None => {
                // Create new group
                info!("Creating new SCIM group: {}", scim_group.external_id);

                let group_id = self
                    .repository
                    .create_group(
                        self.tenant_scope,
                        &scim_group.external_id,
                        &scim_group.display_name,
                    )
                    .await
                    .map_err(|e| ScimWriteError::pre_write(directory_conflict("group", e)))?;

                // The group INSERT is committed from here on.
                if let Some(members) = scim_group.members {
                    for member in members {
                        match self
                            .repository
                            .find_user_id_by_external_id(self.tenant_scope, &member.value)
                            .await
                        {
                            Ok(Some(user_id)) => {
                                if let Err(e) = self
                                    .repository
                                    .add_group_member(self.tenant_scope, group_id, user_id)
                                    .await
                                {
                                    warn!(
                                        "Failed to add member {} to group {}: {}",
                                        member.value, scim_group.external_id, e
                                    );
                                }
                            }
                            Ok(None) => {
                                warn!(
                                    "Member {} not found when creating group {}",
                                    member.value, scim_group.external_id
                                );
                            }
                            Err(e) => return Err(ScimWriteError::post_commit(e.into())),
                        }
                    }
                }

                Ok(ScimGroupResult {
                    id: group_id,
                    external_id: scim_group.external_id,
                    action: ScimAction::Created,
                })
            }
        }
    }

    /// Delete a group.
    pub async fn delete_group(&self, external_id: &str) -> Result<(), ScimError> {
        info!("Deleting SCIM group: {}", external_id);

        let existing = self
            .repository
            .find_group_by_external_id(self.tenant_scope, external_id)
            .await?;

        if existing.is_none() {
            warn!("Attempted to delete non-existent group: {}", external_id);
            return Err(ScimError::GroupNotFound(external_id.to_string()));
        }

        self.repository
            .delete_group(self.tenant_scope, external_id)
            .await?;

        Ok(())
    }

    /// Sync group members.
    ///
    /// Callers must have committed an earlier write in the same request (the
    /// group rename); every error raised here is therefore tagged
    /// `writes_committed`.
    async fn sync_group_members(
        &self,
        group_id: Uuid,
        members: Vec<ScimMember>,
    ) -> Result<(), ScimWriteError> {
        // Get current members
        let current_members = self
            .repository
            .get_group_members(self.tenant_scope, group_id)
            .await
            .map_err(|e| ScimWriteError::post_commit(e.into()))?;
        let current_set: std::collections::HashSet<_> = current_members.into_iter().collect();

        // Resolve new member IDs within the organization
        let mut new_members = Vec::new();
        for member in members {
            match self
                .repository
                .find_user_id_by_external_id(self.tenant_scope, &member.value)
                .await
            {
                Ok(Some(user_id)) => new_members.push(user_id),
                Ok(None) => {
                    warn!("Member {} not found during group sync", member.value);
                }
                Err(e) => return Err(ScimWriteError::post_commit(e.into())),
            }
        }
        let new_set: std::collections::HashSet<_> = new_members.into_iter().collect();

        // Calculate diffs
        let to_add: Vec<_> = new_set.difference(&current_set).copied().collect();
        let to_remove: Vec<_> = current_set.difference(&new_set).copied().collect();

        // Store counts for logging
        let added_count = to_add.len();
        let removed_count = to_remove.len();

        // Apply changes
        for user_id in to_add {
            if let Err(e) = self
                .repository
                .add_group_member(self.tenant_scope, group_id, user_id)
                .await
            {
                error!(
                    "Failed to add member {} to group {}: {}",
                    user_id, group_id, e
                );
            }
        }

        for user_id in to_remove {
            if let Err(e) = self
                .repository
                .remove_group_member(self.tenant_scope, group_id, user_id)
                .await
            {
                error!(
                    "Failed to remove member {} from group {}: {}",
                    user_id, group_id, e
                );
            }
        }

        debug!(
            "Group {} sync complete: added {}, removed {}",
            group_id, added_count, removed_count
        );

        Ok(())
    }
}

/// Generate a temporary password hash for SCIM-provisioned users.
/// These users should typically use SSO or password reset.
fn generate_temporary_password_hash() -> String {
    use rand::RngExt;
    let mut rng = rand::rng();
    let random_bytes: [u8; 32] = rng.random();
    format!("$scim_temp${}", base64::encode(random_bytes))
}

// Simple base64 encoding for temporary passwords
mod base64 {
    pub fn encode(input: impl AsRef<[u8]>) -> String {
        const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let input = input.as_ref();
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base64_encode() {
        assert_eq!(base64::encode("hello"), "aGVsbG8=");
        assert_eq!(base64::encode("A"), "QQ==");
    }
}
