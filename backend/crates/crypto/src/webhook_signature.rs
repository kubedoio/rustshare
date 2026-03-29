//! HMAC-SHA256 webhook signature verification for secure event dispatching.
//!
//! This module provides utilities for signing and verifying webhook payloads
//! to ensure event authenticity and integrity when communicating with
//! external chat integration services.

use hmac::{Hmac, Mac};
use sha2::Sha256;
use thiserror::Error;

/// Errors that can occur during webhook signing/verification.
#[derive(Debug, Error)]
pub enum WebhookSignatureError {
    /// Invalid key length for HMAC.
    #[error("Invalid key length")]
    InvalidKeyLength,

    /// Signature verification failed.
    #[error("Signature verification failed")]
    VerificationFailed,

    /// Invalid signature format.
    #[error("Invalid signature format")]
    InvalidFormat,
}

/// HMAC-SHA256 signer for webhook events.
///
/// Uses a secret key to sign payloads and verify incoming signatures.
#[derive(Debug, Clone)]
pub struct WebhookSigner {
    secret: Vec<u8>,
}

impl WebhookSigner {
    /// Create a new webhook signer with the given secret.
    pub fn new(secret: impl AsRef<[u8]>) -> Self {
        Self {
            secret: secret.as_ref().to_vec(),
        }
    }

    /// Create a new webhook signer from a hex-encoded secret.
    pub fn from_hex(secret_hex: &str) -> Result<Self, WebhookSignatureError> {
        let secret = hex::decode(secret_hex).map_err(|_| WebhookSignatureError::InvalidFormat)?;
        Ok(Self::new(secret))
    }

    /// Sign a payload and return the signature as a hex string.
    ///
    /// The signature format is: `v1=<hex_encoded_hmac_sha256>`
    pub fn sign(&self, payload: impl AsRef<[u8]>) -> Result<String, WebhookSignatureError> {
        let signature = self.sign_raw(payload)?;
        Ok(format!("v1={}", hex::encode(signature)))
    }

    /// Sign a payload with a timestamp for additional replay protection.
    ///
    /// The signed content is: `<timestamp>.<payload>`
    /// The signature format is: `t=<timestamp>,v1=<hex_encoded_hmac_sha256>`
    pub fn sign_with_timestamp(
        &self,
        timestamp: i64,
        payload: impl AsRef<[u8]>,
    ) -> Result<String, WebhookSignatureError> {
        let signed_content = format!("{}.{}", timestamp, hex::encode(payload.as_ref()));
        let signature = self.sign_raw(&signed_content)?;
        Ok(format!("t={},v1={}", timestamp, hex::encode(signature)))
    }

    /// Verify a signature against a payload.
    ///
    /// Accepts signatures in format `v1=<hex>` or `t=<timestamp>,v1=<hex>`.
    pub fn verify(
        &self,
        signature: &str,
        payload: impl AsRef<[u8]>,
    ) -> Result<bool, WebhookSignatureError> {
        let parts: Vec<&str> = signature.split(',').collect();

        if parts.len() == 1 {
            // Simple v1 signature: "v1=<hex>"
            self.verify_simple(parts[0], payload)
        } else if parts.len() == 2 {
            // Timestamped signature: "t=<timestamp>,v1=<hex>"
            self.verify_timestamped(parts, payload)
        } else {
            Err(WebhookSignatureError::InvalidFormat)
        }
    }

    fn sign_raw(&self, data: impl AsRef<[u8]>) -> Result<Vec<u8>, WebhookSignatureError> {
        type HmacSha256 = Hmac<Sha256>;

        let mut mac =
            HmacSha256::new_from_slice(&self.secret).map_err(|_| WebhookSignatureError::InvalidKeyLength)?;
        mac.update(data.as_ref());
        Ok(mac.finalize().into_bytes().to_vec())
    }

    fn verify_simple(
        &self,
        sig_part: &str,
        payload: impl AsRef<[u8]>,
    ) -> Result<bool, WebhookSignatureError> {
        if !sig_part.starts_with("v1=") {
            return Err(WebhookSignatureError::InvalidFormat);
        }

        let expected_sig = self.sign(payload)?;
        Ok(sig_part == expected_sig)
    }

    fn verify_timestamped(
        &self,
        parts: Vec<&str>,
        payload: impl AsRef<[u8]>,
    ) -> Result<bool, WebhookSignatureError> {
        if parts.len() != 2 {
            return Err(WebhookSignatureError::InvalidFormat);
        }

        let timestamp_part = parts[0];
        let sig_part = parts[1];

        if !timestamp_part.starts_with("t=") || !sig_part.starts_with("v1=") {
            return Err(WebhookSignatureError::InvalidFormat);
        }

        let timestamp = timestamp_part[2..]
            .parse::<i64>()
            .map_err(|_| WebhookSignatureError::InvalidFormat)?;

        let signed_content = format!("{}.{}", timestamp, hex::encode(payload.as_ref()));
        let expected_sig = format!("v1={}", hex::encode(self.sign_raw(&signed_content)?));

        Ok(sig_part == expected_sig)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sign_and_verify_simple() {
        let signer = WebhookSigner::new("my_secret_key_12345");
        let payload = b"test webhook payload";

        let signature = signer.sign(payload).unwrap();
        assert!(signature.starts_with("v1="));

        let is_valid = signer.verify(&signature, payload).unwrap();
        assert!(is_valid);
    }

    #[test]
    fn test_verify_fails_with_wrong_payload() {
        let signer = WebhookSigner::new("my_secret_key_12345");
        let payload = b"test webhook payload";
        let wrong_payload = b"wrong payload";

        let signature = signer.sign(payload).unwrap();
        let is_valid = signer.verify(&signature, wrong_payload).unwrap();
        assert!(!is_valid);
    }

    #[test]
    fn test_sign_and_verify_with_timestamp() {
        let signer = WebhookSigner::new("my_secret_key_12345");
        let payload = b"test webhook payload";
        let timestamp = 1704067200i64; // 2024-01-01 00:00:00 UTC

        let signature = signer.sign_with_timestamp(timestamp, payload).unwrap();
        assert!(signature.contains("t="));
        assert!(signature.contains("v1="));

        let is_valid = signer.verify(&signature, payload).unwrap();
        assert!(is_valid);
    }

    #[test]
    fn test_from_hex() {
        let secret_hex = "deadbeef12345678";
        let signer = WebhookSigner::from_hex(secret_hex).unwrap();

        let payload = b"test";
        let signature = signer.sign(payload).unwrap();
        let is_valid = signer.verify(&signature, payload).unwrap();
        assert!(is_valid);
    }

    #[test]
    fn test_invalid_signature_format() {
        let signer = WebhookSigner::new("secret");
        let result = signer.verify("invalid_format", b"payload");
        assert!(matches!(result, Err(WebhookSignatureError::InvalidFormat)));
    }

    #[test]
    fn test_different_secrets_produce_different_signatures() {
        let signer1 = WebhookSigner::new("secret_one");
        let signer2 = WebhookSigner::new("secret_two");
        let payload = b"same payload";

        let sig1 = signer1.sign(payload).unwrap();
        let sig2 = signer2.sign(payload).unwrap();

        assert_ne!(sig1, sig2);
    }
}
