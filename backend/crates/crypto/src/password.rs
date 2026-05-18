use argon2::{
    password_hash::{
        rand_core::OsRng, PasswordHash, PasswordHasher as _, PasswordVerifier, SaltString,
    },
    Argon2,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PasswordError {
    #[error("Failed to hash password: {0}")]
    HashError(String),

    #[error("Invalid password")]
    InvalidPassword,
}

/// Password hasher using Argon2id
pub struct PasswordHasher;

impl PasswordHasher {
    /// Hash a password using Argon2id
    pub fn hash(password: &str) -> Result<String, PasswordError> {
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();

        let hash = argon2
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| PasswordError::HashError(e.to_string()))?;

        Ok(hash.to_string())
    }

    /// Verify a password against a hash
    pub fn verify(password: &str, hash: &str) -> Result<bool, PasswordError> {
        let parsed_hash =
            PasswordHash::new(hash).map_err(|e| PasswordError::HashError(e.to_string()))?;

        let argon2 = Argon2::default();

        match argon2.verify_password(password.as_bytes(), &parsed_hash) {
            Ok(()) => Ok(true),
            Err(_) => Ok(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_password_hash_and_verify() {
        let password = "secure_password_123";
        let hash = PasswordHasher::hash(password).unwrap();

        assert!(PasswordHasher::verify(password, &hash).unwrap());
        assert!(!PasswordHasher::verify("wrong_password", &hash).unwrap());
    }

    #[test]
    fn test_different_hashes_for_same_password() {
        let password = "test123";
        let hash1 = PasswordHasher::hash(password).unwrap();
        let hash2 = PasswordHasher::hash(password).unwrap();

        // Hashes should be different due to different salts
        assert_ne!(hash1, hash2);

        // But both should verify
        assert!(PasswordHasher::verify(password, &hash1).unwrap());
        assert!(PasswordHasher::verify(password, &hash2).unwrap());
    }
}
