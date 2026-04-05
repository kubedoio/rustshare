//! Session manager implementation

use super::*;
use chrono::TimeDelta;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, TokenData, Validation};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};


/// JWT claims structure
#[derive(Debug, Serialize, Deserialize)]
struct JwtClaims {
    /// Subject (user ID)
    sub: String,
    /// User email
    email: String,
    /// Session ID
    sid: String,
    /// Session type
    stype: String,
    /// Issued at
    iat: i64,
    /// Expiration
    exp: i64,
}

impl From<SessionClaims> for JwtClaims {
    fn from(claims: SessionClaims) -> Self {
        Self {
            sub: claims.user_id.to_string(),
            email: claims.email,
            sid: claims.session_id,
            stype: claims.session_type.as_str().to_string(),
            iat: claims.issued_at.timestamp(),
            exp: claims.expires_at.timestamp(),
        }
    }
}

impl TryFrom<JwtClaims> for SessionClaims {
    type Error = SessionError;
    
    fn try_from(claims: JwtClaims) -> Result<Self, Self::Error> {
        let session_type = match claims.stype.as_str() {
            "web" => SessionType::Web,
            "api" => SessionType::Api,
            "device" => SessionType::Device,
            "share" => SessionType::Share,
            _ => return Err(SessionError::InvalidToken),
        };
        
        Ok(Self {
            user_id: claims.sub.parse().map_err(|_| SessionError::InvalidToken)?,
            email: claims.email,
            session_id: claims.sid,
            issued_at: DateTime::from_timestamp(claims.iat, 0)
                .ok_or(SessionError::InvalidToken)?,
            expires_at: DateTime::from_timestamp(claims.exp, 0)
                .ok_or(SessionError::InvalidToken)?,
            session_type,
        })
    }
}

/// Session manager implementation
pub struct SessionManager {
    config: SessionConfig,
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
}

impl SessionManager {
    /// Create a new session manager
    pub fn new(config: SessionConfig) -> Self {
        let encoding_key = EncodingKey::from_secret(config.jwt_secret.as_bytes());
        let decoding_key = DecodingKey::from_secret(config.jwt_secret.as_bytes());
        
        Self {
            config,
            encoding_key,
            decoding_key,
        }
    }
    
    /// Create a new session for a user
    pub fn create_session(
        &self,
        user_id: Uuid,
        email: String,
        session_type: SessionType,
    ) -> Result<SessionInfo, SessionError> {
        let session_id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let expires_at = now + TimeDelta::from_std(self.config.session_ttl)
            .map_err(|e| SessionError::BackendError(e.to_string()))?;
        
        let claims = SessionClaims {
            user_id,
            email: email.clone(),
            session_id: session_id.clone(),
            issued_at: now,
            expires_at,
            session_type,
        };
        
        let jwt_claims: JwtClaims = claims.into();
        let token = encode(&Header::default(), &jwt_claims, &self.encoding_key)
            .map_err(|e| SessionError::BackendError(e.to_string()))?;
        
        let cookie_value = if session_type == SessionType::Web {
            Some(self.build_cookie(&token, &expires_at))
        } else {
            None
        };
        
        Ok(SessionInfo {
            token,
            session_id,
            expires_at,
            cookie_value,
        })
    }
    
    /// Validate a session token
    pub fn validate_token(&self, token: &str) -> ValidationResult {
        // First validate the JWT signature and expiration
        let token_data = match self.decode_token(token) {
            Ok(data) => data,
            Err(e) => return ValidationResult::Invalid(e),
        };
        
        let claims = match SessionClaims::try_from(token_data.claims) {
            Ok(c) => c,
            Err(e) => return ValidationResult::Invalid(e),
        };
        
        // Check if token is expired
        if claims.expires_at < Utc::now() {
            return ValidationResult::Invalid(SessionError::Expired);
        }
        
        // Check revocation cache if enabled
        if self.config.use_revocation_cache {
            let session_hash = self.hash_session(token);
            // In a real implementation, we'd check the coordination store here
            // For now, we assume not revoked (stateless validation)
            tracing::debug!("Checking revocation for session hash: {}", session_hash);
        }
        
        ValidationResult::Valid(claims)
    }
    
    /// Validate a session from cookie
    pub fn validate_cookie(&self, cookie_value: &str) -> ValidationResult {
        // Extract token from cookie format
        let token = self.extract_token_from_cookie(cookie_value);
        self.validate_token(&token)
    }
    
    /// Revoke a session
    pub fn revoke_session(&self, token: &str) -> Result<(), SessionError> {
        if !self.config.use_revocation_cache {
            // Stateless mode - we can't revoke without persistence
            return Ok(());
        }
        
        let session_hash = self.hash_session(token);
        tracing::info!("Revoking session: {}", session_hash);
        
        // In a real implementation, we'd store this in the coordination store
        // For now, this is a no-op in the stateless implementation
        
        Ok(())
    }
    
    /// Refresh a session (issue new token with extended expiry)
    pub fn refresh_session(&self, token: &str) -> Result<SessionInfo, SessionError> {
        match self.validate_token(token) {
            ValidationResult::Valid(claims) => {
                self.create_session(claims.user_id, claims.email, claims.session_type)
            }
            ValidationResult::Invalid(e) => Err(e),
        }
    }
    
    /// Build a cookie string
    fn build_cookie(&self, token: &str, expires_at: &DateTime<Utc>) -> String {
        let max_age = (*expires_at - Utc::now()).num_seconds().max(0);
        let secure = if self.config.cookie_secure { "Secure; " } else { "" };
        let same_site = &self.config.cookie_same_site;
        
        format!(
            "{}={}; Max-Age={}; HttpOnly; {}SameSite={}; Path=/",
            self.config.cookie_name, token, max_age, secure, same_site
        )
    }
    
    /// Extract token from cookie header value
    fn extract_token_from_cookie(&self, cookie_value: &str) -> String {
        // Parse cookie header value like "name=value; name2=value2"
        for part in cookie_value.split(';') {
            let part = part.trim();
            if let Some(pos) = part.find('=') {
                let (name, value) = part.split_at(pos);
                if name == self.config.cookie_name {
                    return value[1..].to_string(); // Skip the '='
                }
            }
        }
        cookie_value.to_string() // Fallback
    }
    
    /// Decode and validate a JWT token
    fn decode_token(&self, token: &str) -> Result<TokenData<JwtClaims>, SessionError> {
        let validation = Validation::default();
        decode::<JwtClaims>(token, &self.decoding_key, &validation)
            .map_err(|e| match e.kind() {
                jsonwebtoken::errors::ErrorKind::ExpiredSignature => SessionError::Expired,
                _ => SessionError::InvalidToken,
            })
    }
    
    /// Hash a session token for storage/revocation lookup
    fn hash_session(&self, token: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(token.as_bytes());
        hex::encode(hasher.finalize())
    }
    
    /// Build an expired cookie (for logout)
    pub fn build_logout_cookie(&self) -> String {
        format!(
            "{}=; Max-Age=0; HttpOnly; SameSite={}; Path=/",
            self.config.cookie_name, self.config.cookie_same_site
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_manager() -> SessionManager {
        let config = SessionConfig {
            jwt_secret: "test-secret-key-that-is-long-enough".to_string(),
            session_ttl: Duration::from_secs(3600),
            use_revocation_cache: false,
            revocation_cache_ttl: Duration::from_secs(3600),
            cookie_name: "test_session".to_string(),
            cookie_secure: false,
            cookie_same_site: "lax".to_string(),
        };
        SessionManager::new(config)
    }

    #[test]
    fn test_create_and_validate_session() {
        let manager = create_test_manager();
        let user_id = Uuid::new_v4();
        
        let session = manager
            .create_session(user_id, "test@example.com".to_string(), SessionType::Web)
            .unwrap();
        
        assert!(!session.token.is_empty());
        assert!(session.cookie_value.is_some());
        
        // Validate the token
        match manager.validate_token(&session.token) {
            ValidationResult::Valid(claims) => {
                assert_eq!(claims.user_id, user_id);
                assert_eq!(claims.email, "test@example.com");
                assert_eq!(claims.session_type, SessionType::Web);
            }
            ValidationResult::Invalid(e) => panic!("Expected valid session: {:?}", e),
        }
    }

    #[test]
    fn test_validate_invalid_token() {
        let manager = create_test_manager();
        
        match manager.validate_token("invalid.token.here") {
            ValidationResult::Valid(_) => panic!("Expected invalid token"),
            ValidationResult::Invalid(e) => {
                assert!(matches!(e, SessionError::InvalidToken));
            }
        }
    }

    #[test]
    fn test_cookie_parsing() {
        let manager = create_test_manager();
        let user_id = Uuid::new_v4();
        
        let session = manager
            .create_session(user_id, "test@example.com".to_string(), SessionType::Web)
            .unwrap();
        
        let cookie = session.cookie_value.unwrap();
        
        // Validate from cookie
        match manager.validate_cookie(&cookie) {
            ValidationResult::Valid(claims) => {
                assert_eq!(claims.user_id, user_id);
            }
            ValidationResult::Invalid(e) => panic!("Expected valid session: {:?}", e),
        }
    }

    #[test]
    fn test_logout_cookie() {
        let manager = create_test_manager();
        let cookie = manager.build_logout_cookie();
        
        assert!(cookie.contains("Max-Age=0"));
        assert!(cookie.contains("test_session="));
    }

    #[test]
    fn test_different_session_types() {
        let manager = create_test_manager();
        let user_id = Uuid::new_v4();
        
        let web_session = manager
            .create_session(user_id, "test@example.com".to_string(), SessionType::Web)
            .unwrap();
        let api_session = manager
            .create_session(user_id, "test@example.com".to_string(), SessionType::Api)
            .unwrap();
        
        // Web session should have cookie
        assert!(web_session.cookie_value.is_some());
        // API session should not have cookie
        assert!(api_session.cookie_value.is_none());
    }
}
