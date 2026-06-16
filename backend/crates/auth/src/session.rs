use chrono::{Duration, Utc};
use rand::{distr::Alphanumeric, RngExt};
use rustshare_core::domain::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const WEB_SESSION_COOKIE_NAME: &str = "rustshare_session";

/// Share session claims for JWT
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareSessionClaims {
    pub sub: String, // Format: "share:{share_id}"
    pub session_id: uuid::Uuid,
    pub share_id: ShareId,
    pub file_id: Option<FileId>,
    pub folder_id: Option<FolderId>,
    pub permissions: SharePermissions,
    /// Tenant that owns the shared resource. Used to enforce tenant isolation
    /// on public-share session routes.
    pub tenant_id: uuid::Uuid,
    pub iat: i64,
    pub exp: i64,
}

impl ShareSessionClaims {
    /// Create new share session claims
    pub fn new(
        share_id: ShareId,
        file_id: Option<FileId>,
        folder_id: Option<FolderId>,
        permissions: SharePermissions,
        tenant_id: uuid::Uuid,
        ttl_seconds: i64,
    ) -> Self {
        let now = Utc::now();
        let exp = now + Duration::seconds(ttl_seconds);

        Self {
            sub: format!("share:{}", share_id),
            session_id: uuid::Uuid::new_v4(),
            share_id,
            file_id,
            folder_id,
            permissions,
            tenant_id,
            iat: now.timestamp(),
            exp: exp.timestamp(),
        }
    }

    /// Check if claims are expired
    pub fn is_expired(&self) -> bool {
        Utc::now().timestamp() > self.exp
    }
}

pub fn generate_web_session_token() -> String {
    rand::rng()
        .sample_iter(&Alphanumeric)
        .take(48)
        .map(char::from)
        .collect()
}

pub fn hash_web_session_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());

    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{:02x}", byte))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_share_session_claims_creation() {
        let share_id = uuid::Uuid::new_v4();
        let file_id = uuid::Uuid::new_v4();
        let tenant_id = uuid::Uuid::new_v4();
        let claims = ShareSessionClaims::new(
            share_id,
            Some(file_id),
            None,
            SharePermissions::View,
            tenant_id,
            3600,
        );

        assert_eq!(claims.sub, format!("share:{}", share_id));
        assert!(!claims.session_id.is_nil());
        assert_eq!(claims.share_id, share_id);
        assert_eq!(claims.file_id, Some(file_id));
        assert_eq!(claims.folder_id, None);
        assert_eq!(claims.permissions, SharePermissions::View);
        assert_eq!(claims.tenant_id, tenant_id);
        assert!(claims.exp > claims.iat);
    }

    #[test]
    fn test_share_session_claims_expiration() {
        let share_id = uuid::Uuid::new_v4();
        let file_id = uuid::Uuid::new_v4();
        let tenant_id = uuid::Uuid::new_v4();
        let claims = ShareSessionClaims::new(
            share_id,
            Some(file_id),
            None,
            SharePermissions::View,
            tenant_id,
            -1,
        );

        assert!(claims.is_expired());
    }
}
