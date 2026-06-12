//! Authentication and authorization for RustShare.

pub mod jwt;
pub mod session;

pub use jwt::JwtManager;
pub use rustshare_crypto::{PasswordHasher, DUMMY_HASH};
pub use session::{
    generate_web_session_token, hash_web_session_token, ShareSessionClaims, WEB_SESSION_COOKIE_NAME,
};

// Implement JwtOps trait from rustshare_core for JwtManager
use rustshare_core::services::JwtOps;

impl JwtOps for JwtManager {
    fn encode_custom_claims<T: serde::Serialize>(&self, claims: &T) -> Result<String, String> {
        self.encode_custom_claims(claims).map_err(|e| e.to_string())
    }
}
