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

#[derive(Debug, Error)]
pub enum EncryptionError {
    #[error("Invalid key length")]
    InvalidKey,
    #[error("Encryption failed")]
    EncryptionFailed,
    #[error("Decryption failed — wrong key or corrupted ciphertext")]
    DecryptionFailed,
    #[error("Base64 decode error")]
    DecodeError,
    #[error("Ciphertext too short (must be at least 12 bytes for nonce)")]
    InvalidCiphertext,
    #[error("Decrypted value is not valid UTF-8")]
    InvalidUtf8,
}

/// Encrypt `plaintext` with AES-256-GCM using `key`.
///
/// Returns a base64-encoded string containing the 12-byte random nonce
/// prepended to the ciphertext.
pub fn encrypt_secret(plaintext: &str, key: &[u8; 32]) -> Result<String, EncryptionError> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| EncryptionError::InvalidKey)?;
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(&nonce, plaintext.as_bytes())
        .map_err(|_| EncryptionError::EncryptionFailed)?;
    let mut combined = nonce.to_vec();
    combined.extend_from_slice(&ciphertext);
    Ok(STANDARD.encode(combined))
}

/// Decrypt a base64-encoded ciphertext produced by [`encrypt_secret`].
pub fn decrypt_secret(encoded: &str, key: &[u8; 32]) -> Result<String, EncryptionError> {
    let combined = STANDARD.decode(encoded).map_err(|_| EncryptionError::DecodeError)?;
    if combined.len() < 12 {
        return Err(EncryptionError::InvalidCiphertext);
    }
    let (nonce_bytes, ciphertext) = combined.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| EncryptionError::InvalidKey)?;
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| EncryptionError::DecryptionFailed)?;
    String::from_utf8(plaintext).map_err(|_| EncryptionError::InvalidUtf8)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key() -> [u8; 32] {
        [0x42u8; 32]
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
        let wrong_key = [0xFFu8; 32];
        let encoded = encrypt_secret("secret", &key).unwrap();
        let result = decrypt_secret(&encoded, &wrong_key);
        assert!(result.is_err(), "Decryption with wrong key must fail");
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let key = test_key();
        let mut encoded = encrypt_secret("secret", &key).unwrap();
        // Flip last character
        let last = encoded.pop().unwrap();
        encoded.push(if last == 'A' { 'B' } else { 'A' });
        let result = decrypt_secret(&encoded, &key);
        assert!(result.is_err(), "Tampered ciphertext must fail authentication");
    }

    #[test]
    fn too_short_ciphertext_fails() {
        let key = test_key();
        // base64 of 5 bytes — shorter than 12-byte nonce minimum
        let short = base64::engine::general_purpose::STANDARD.encode([0u8; 5]);
        let result = decrypt_secret(&short, &key);
        assert!(matches!(result, Err(EncryptionError::InvalidCiphertext)));
    }
}
