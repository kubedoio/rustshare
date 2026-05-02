use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum JwtError {
    #[error("Failed to encode token: {0}")]
    EncodeError(String),

    #[error("Failed to decode token: {0}")]
    DecodeError(String),

    #[error("Token expired")]
    TokenExpired,
}

/// JWT claims
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: String, // Subject (user ID)
    pub email: String,
    pub tenant_id: Uuid,
    pub exp: i64,    // Expiration time
    pub iat: i64,    // Issued at
    pub iss: String, // Issuer
}

/// JWT token manager
pub struct JwtManager {
    secret: String,
}

impl JwtManager {
    /// Create a new JWT manager with the given secret.
    ///
    /// The secret can be any type that converts into a String,
    /// such as `&str` or `String`.
    pub fn new(secret: impl Into<String>) -> Self {
        Self {
            secret: secret.into(),
        }
    }

    /// Generate a JWT token for a user.
    ///
    /// The email parameter accepts any string type (`&str` or `String`)
    /// to avoid unnecessary allocations.
    pub fn generate(
        &self,
        user_id: Uuid,
        email: impl AsRef<str>,
        tenant_id: Uuid,
    ) -> Result<String, JwtError> {
        let now = Utc::now();
        let expiration = now + Duration::hours(24);
        let email = email.as_ref();

        let claims = Claims {
            sub: user_id.to_string(),
            email: email.to_string(),
            tenant_id,
            exp: expiration.timestamp(),
            iat: now.timestamp(),
            iss: "rustshare".to_string(),
        };

        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(self.secret.as_bytes()),
        )
        .map_err(|e| JwtError::EncodeError(e.to_string()))
    }

    /// Validate and decode a JWT token
    pub fn validate(&self, token: &str) -> Result<Claims, JwtError> {
        let validation = Validation::default();

        let token_data = decode::<Claims>(
            token,
            &DecodingKey::from_secret(self.secret.as_bytes()),
            &validation,
        )
        .map_err(|e| JwtError::DecodeError(e.to_string()))?;

        Ok(token_data.claims)
    }

    /// Encode custom claims to JWT
    pub fn encode_custom_claims<T: Serialize>(&self, claims: &T) -> Result<String, JwtError> {
        encode(
            &Header::default(),
            claims,
            &EncodingKey::from_secret(self.secret.as_bytes()),
        )
        .map_err(|e| JwtError::EncodeError(e.to_string()))
    }

    /// Decode custom claims from JWT
    pub fn decode_custom<T: for<'de> Deserialize<'de>>(&self, token: &str) -> Result<T, JwtError> {
        let token_data = decode::<T>(
            token,
            &DecodingKey::from_secret(self.secret.as_bytes()),
            &Validation::default(),
        )
        .map_err(|e| JwtError::DecodeError(e.to_string()))?;

        Ok(token_data.claims)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_and_validate_token() {
        let secret = "test_secret_key_at_least_32_chars_long_for_security";
        let manager = JwtManager::new(secret.to_string());

        let user_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let email = "test@example.com".to_string();

        let token = manager.generate(user_id, email.clone(), tenant_id).unwrap();
        let claims = manager.validate(&token).unwrap();

        assert_eq!(claims.sub, user_id.to_string());
        assert_eq!(claims.email, email);
        assert_eq!(claims.tenant_id, tenant_id);
        assert_eq!(claims.iss, "rustshare");
    }

    #[test]
    fn test_invalid_token_fails_validation() {
        let secret = "test_secret_key_at_least_32_chars_long_for_security";
        let manager = JwtManager::new(secret.to_string());

        let result = manager.validate("invalid.token.here");
        assert!(result.is_err());
    }

    #[test]
    fn test_encode_decode_custom_claims() {
        use crate::session::ShareSessionClaims;
        use rustshare_core::domain::SharePermissions;

        let manager = JwtManager::new("test_secret".to_string());
        let share_id = uuid::Uuid::new_v4();
        let file_id = uuid::Uuid::new_v4();

        let claims =
            ShareSessionClaims::new(share_id, Some(file_id), None, SharePermissions::View, 3600);

        let token = manager.encode_custom_claims(&claims).unwrap();
        let decoded: ShareSessionClaims = manager.decode_custom(&token).unwrap();

        assert_eq!(decoded.share_id, share_id);
        assert_eq!(decoded.file_id, Some(file_id));
    }
}
