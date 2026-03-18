use chrono::{Duration, Utc};
use rustshare_core::domain::*;
use serde::{Deserialize, Serialize};

/// Share session claims for JWT
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareSessionClaims {
    pub sub: String, // Format: "share:{share_id}"
    pub share_id: ShareId,
    pub file_id: FileId,
    pub permissions: SharePermissions,
    pub iat: i64,
    pub exp: i64,
}

impl ShareSessionClaims {
    /// Create new share session claims
    pub fn new(
        share_id: ShareId,
        file_id: FileId,
        permissions: SharePermissions,
        ttl_seconds: i64,
    ) -> Self {
        let now = Utc::now();
        let exp = now + Duration::seconds(ttl_seconds);

        Self {
            sub: format!("share:{}", share_id),
            share_id,
            file_id,
            permissions,
            iat: now.timestamp(),
            exp: exp.timestamp(),
        }
    }

    /// Check if claims are expired
    pub fn is_expired(&self) -> bool {
        Utc::now().timestamp() > self.exp
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_share_session_claims_creation() {
        let share_id = uuid::Uuid::new_v4();
        let file_id = uuid::Uuid::new_v4();
        let claims = ShareSessionClaims::new(share_id, file_id, SharePermissions::View, 3600);

        assert_eq!(claims.sub, format!("share:{}", share_id));
        assert_eq!(claims.share_id, share_id);
        assert_eq!(claims.file_id, file_id);
        assert_eq!(claims.permissions, SharePermissions::View);
        assert!(claims.exp > claims.iat);
    }

    #[test]
    fn test_share_session_claims_expiration() {
        let share_id = uuid::Uuid::new_v4();
        let file_id = uuid::Uuid::new_v4();
        let claims = ShareSessionClaims::new(share_id, file_id, SharePermissions::View, -1);

        assert!(claims.is_expired());
    }
}

