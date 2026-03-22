//! AES-256-GCM encryption for sensitive config values stored in the database.
//!
//! Format: base64(12-byte-nonce || ciphertext)
//! Key: 32-byte key derived from RUSTSHARE_SECRET_ENCRYPTION_KEY env var (loaded at startup).

use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use thiserror::Error;

/// A validated 32-byte AES-256-GCM key, loaded once at startup from
/// the `RUSTSHARE_SECRET_ENCRYPTION_KEY` environment variable.
///
/// Use [`SecretEncryptionKey::from_env`] to construct at startup.
#[derive(Clone)]
pub struct SecretEncryptionKey([u8; 32]);

impl SecretEncryptionKey {
    /// Load and validate the key from `RUSTSHARE_SECRET_ENCRYPTION_KEY` (base64-encoded 32 bytes).
    /// Returns an error if the env var is missing or the decoded key is not exactly 32 bytes.
    pub fn from_env() -> Result<Self, String> {
        let raw = std::env::var("RUSTSHARE_SECRET_ENCRYPTION_KEY")
            .map_err(|_| "RUSTSHARE_SECRET_ENCRYPTION_KEY env var not set".to_string())?;
        let bytes = STANDARD
            .decode(raw.trim())
            .map_err(|_| "RUSTSHARE_SECRET_ENCRYPTION_KEY is not valid base64".to_string())?;
        let arr: [u8; 32] = bytes
            .try_into()
            .map_err(|_| "RUSTSHARE_SECRET_ENCRYPTION_KEY must decode to exactly 32 bytes".to_string())?;
        Ok(Self(arr))
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Debug, Error)]
pub enum EncryptionError {
    #[error("Encryption failed")]
    EncryptionFailed,
    #[error("Decryption failed — wrong key or corrupted ciphertext")]
    DecryptionFailed,
    #[error("Base64 decode error")]
    DecodeError,
    #[error("Ciphertext too short (must be at least 28 bytes (12-byte nonce + 16-byte GCM tag))")]
    InvalidCiphertext,
    #[error("Decrypted value is not valid UTF-8")]
    InvalidUtf8,
}

/// Encrypt `plaintext` with AES-256-GCM using `key`.
///
/// Returns a base64-encoded string containing the 12-byte random nonce
/// prepended to the ciphertext.
pub fn encrypt_secret(plaintext: &str, key: &SecretEncryptionKey) -> Result<String, EncryptionError> {
    // new_from_slice is infallible for a 32-byte key (AES-256 requires exactly 32 bytes)
    let cipher = Aes256Gcm::new(key.as_bytes().into());
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(&nonce, plaintext.as_bytes())
        .map_err(|_| EncryptionError::EncryptionFailed)?;
    let mut combined = nonce.to_vec();
    combined.extend_from_slice(&ciphertext);
    Ok(STANDARD.encode(combined))
}

/// Decrypt a base64-encoded ciphertext produced by [`encrypt_secret`].
pub fn decrypt_secret(encoded: &str, key: &SecretEncryptionKey) -> Result<String, EncryptionError> {
    let combined = STANDARD.decode(encoded).map_err(|_| EncryptionError::DecodeError)?;
    if combined.len() < 28 {  // 12-byte nonce + 16-byte GCM tag minimum
        return Err(EncryptionError::InvalidCiphertext);
    }
    let (nonce_bytes, ciphertext) = combined.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);
    // new_from_slice is infallible for a 32-byte key (AES-256 requires exactly 32 bytes)
    let cipher = Aes256Gcm::new(key.as_bytes().into());
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| EncryptionError::DecryptionFailed)?;
    String::from_utf8(plaintext).map_err(|_| EncryptionError::InvalidUtf8)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key() -> SecretEncryptionKey {
        SecretEncryptionKey([0x42u8; 32])
    }

    #[test]
    fn roundtrip_short_value() {
        let key = test_key();
        let plaintext = "super-secret-client-secret";
        let encoded = encrypt_secret(plaintext, &key).unwrap();
        let decoded = decrypt_secret(&encoded, &key).unwrap();
        assert_eq!(decoded, plaintext);
    }

    #[test]
    fn roundtrip_empty_string() {
        let key = test_key();
        let encoded = encrypt_secret("", &key).unwrap();
        let decoded = decrypt_secret(&encoded, &key).unwrap();
        assert_eq!(decoded, "");
    }

    #[test]
    fn roundtrip_unicode() {
        let key = test_key();
        let plaintext = "пароль-123-🔑";
        let encoded = encrypt_secret(plaintext, &key).unwrap();
        let decoded = decrypt_secret(&encoded, &key).unwrap();
        assert_eq!(decoded, plaintext);
    }

    #[test]
    fn different_ciphertexts_same_plaintext() {
        // Each encryption must use a fresh nonce — ciphertexts must differ
        let key = test_key();
        let plaintext = "same-input";
        let enc1 = encrypt_secret(plaintext, &key).unwrap();
        let enc2 = encrypt_secret(plaintext, &key).unwrap();
        assert_ne!(enc1, enc2, "Nonces must be random; ciphertexts must differ");
    }

    #[test]
    fn wrong_key_fails_decryption() {
        let key = test_key();
        let wrong_key = SecretEncryptionKey([0xFFu8; 32]);
        let encoded = encrypt_secret("secret", &key).unwrap();
        let result = decrypt_secret(&encoded, &wrong_key);
        assert!(result.is_err(), "Decryption with wrong key must fail");
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let key = SecretEncryptionKey([0x42u8; 32]);
        let encoded = encrypt_secret("secret", &key).unwrap();
        // Decode, flip a byte in the ciphertext portion (after the 12-byte nonce), re-encode
        let mut raw = STANDARD.decode(&encoded).unwrap();
        // Flip a byte in the middle of the payload (well past the nonce)
        let mid = raw.len() / 2;
        raw[mid] ^= 0xFF;
        let tampered = STANDARD.encode(raw);
        let result = decrypt_secret(&tampered, &key);
        assert!(result.is_err(), "Tampered ciphertext must fail authentication");
    }

    #[test]
    fn too_short_ciphertext_fails() {
        let key = test_key();
        // base64 of 5 bytes — shorter than 28-byte minimum
        let short = STANDARD.encode([0u8; 5]);
        let result = decrypt_secret(&short, &key);
        assert!(matches!(result, Err(EncryptionError::InvalidCiphertext)));
    }

    #[test]
    fn invalid_base64_fails_with_decode_error() {
        let key = SecretEncryptionKey([0x42u8; 32]);
        let result = decrypt_secret("not-valid-base64!!!", &key);
        assert!(matches!(result, Err(EncryptionError::DecodeError)));
    }
}
