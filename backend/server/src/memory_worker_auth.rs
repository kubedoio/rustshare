//! Request authentication shared by the Memory worker and its Files API.

use chrono::Utc;
use rustshare_crypto::WebhookSigner;
use thiserror::Error;

const SECRET_ENV: &str = "RUSTSHARE_MEMORY_WORKER_SECRET";
const MIN_SECRET_CHARS: usize = 32;
const MAX_AGE_SECONDS: i64 = 60;
const FUTURE_SKEW_SECONDS: i64 = 30;

/// Signs and verifies timestamped Memory worker requests.
///
/// Signatures cover the HTTP method, origin-form path (including query), and
/// exact body bytes. The secret is never exposed by this type's formatting.
#[derive(Clone)]
pub struct MemoryWorkerRequestAuth {
    signer: WebhookSigner,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MemoryWorkerAuthError {
    #[error("{SECRET_ENV} is not configured")]
    MissingSecret,
    #[error("{SECRET_ENV} must contain at least {MIN_SECRET_CHARS} characters")]
    WeakSecret,
    #[error("Memory worker request method or path is invalid")]
    InvalidRequest,
    #[error("Memory worker signature is malformed")]
    MalformedSignature,
    #[error("Memory worker signature is invalid")]
    InvalidSignature,
    #[error("Memory worker signature is outside its validity window")]
    ExpiredSignature,
}

impl MemoryWorkerRequestAuth {
    /// Creates request authentication from a secret of at least 32 characters.
    pub fn new(secret: &str) -> Result<Self, MemoryWorkerAuthError> {
        if secret.chars().count() < MIN_SECRET_CHARS {
            return Err(MemoryWorkerAuthError::WeakSecret);
        }
        Ok(Self {
            signer: WebhookSigner::new(secret),
        })
    }

    /// Loads the shared worker secret from `RUSTSHARE_MEMORY_WORKER_SECRET`.
    pub fn from_env() -> Result<Self, MemoryWorkerAuthError> {
        let secret = std::env::var(SECRET_ENV).map_err(|_| MemoryWorkerAuthError::MissingSecret)?;
        Self::new(&secret)
    }

    /// Signs a request using the current UTC timestamp.
    pub fn sign_request(
        &self,
        method: &str,
        path: &str,
        body: &[u8],
    ) -> Result<String, MemoryWorkerAuthError> {
        self.sign_request_at(method, path, body, Utc::now().timestamp())
    }

    /// Verifies a request using the current UTC timestamp.
    pub fn verify_request(
        &self,
        signature: &str,
        method: &str,
        path: &str,
        body: &[u8],
    ) -> Result<(), MemoryWorkerAuthError> {
        self.verify_request_at(signature, method, path, body, Utc::now().timestamp())
    }

    fn sign_request_at(
        &self,
        method: &str,
        path: &str,
        body: &[u8],
        timestamp: i64,
    ) -> Result<String, MemoryWorkerAuthError> {
        let payload = canonical_request(method, path, body)?;
        self.signer
            .sign_with_timestamp(timestamp, payload)
            .map_err(|_| MemoryWorkerAuthError::MalformedSignature)
    }

    fn verify_request_at(
        &self,
        signature: &str,
        method: &str,
        path: &str,
        body: &[u8],
        now: i64,
    ) -> Result<(), MemoryWorkerAuthError> {
        let timestamp = signature_timestamp(signature)?;
        if timestamp > now.saturating_add(FUTURE_SKEW_SECONDS)
            || timestamp < now.saturating_sub(MAX_AGE_SECONDS)
        {
            return Err(MemoryWorkerAuthError::ExpiredSignature);
        }

        let payload = canonical_request(method, path, body)?;
        if self
            .signer
            .verify(signature, payload)
            .map_err(|_| MemoryWorkerAuthError::MalformedSignature)?
        {
            Ok(())
        } else {
            Err(MemoryWorkerAuthError::InvalidSignature)
        }
    }
}

fn canonical_request(
    method: &str,
    path: &str,
    body: &[u8],
) -> Result<Vec<u8>, MemoryWorkerAuthError> {
    if method.is_empty()
        || !method.bytes().all(|byte| byte.is_ascii_alphabetic())
        || !path.starts_with('/')
        || path.contains(['\r', '\n', '#'])
    {
        return Err(MemoryWorkerAuthError::InvalidRequest);
    }

    let method = method.to_ascii_uppercase();
    let mut payload = b"elembra-memory-worker-v1\0".to_vec();
    for part in [method.as_bytes(), path.as_bytes(), body] {
        payload.extend_from_slice(&(part.len() as u64).to_be_bytes());
        payload.extend_from_slice(part);
    }
    Ok(payload)
}

fn signature_timestamp(signature: &str) -> Result<i64, MemoryWorkerAuthError> {
    let (timestamp, versioned_signature) = signature
        .split_once(',')
        .ok_or(MemoryWorkerAuthError::MalformedSignature)?;
    if !timestamp.starts_with("t=") || !versioned_signature.starts_with("v1=") {
        return Err(MemoryWorkerAuthError::MalformedSignature);
    }
    timestamp[2..]
        .parse()
        .map_err(|_| MemoryWorkerAuthError::MalformedSignature)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_SECRET: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn secret_must_contain_at_least_32_characters() {
        assert!(matches!(
            MemoryWorkerRequestAuth::new("x".repeat(31).as_str()),
            Err(MemoryWorkerAuthError::WeakSecret)
        ));
        assert!(MemoryWorkerRequestAuth::new(TEST_SECRET).is_ok());
    }

    #[test]
    fn signature_binds_method_path_and_exact_body() {
        let auth = MemoryWorkerRequestAuth::new(TEST_SECRET).unwrap();
        let signature = auth
            .sign_request_at(
                "POST",
                "/internal/memory/files/source?cursor=1",
                b"payload",
                100,
            )
            .unwrap();

        assert!(auth
            .verify_request_at(
                &signature,
                "POST",
                "/internal/memory/files/source?cursor=1",
                b"payload",
                101,
            )
            .is_ok());
        assert_eq!(
            auth.verify_request_at(
                &signature,
                "GET",
                "/internal/memory/files/source?cursor=1",
                b"payload",
                101,
            ),
            Err(MemoryWorkerAuthError::InvalidSignature)
        );
        assert_eq!(
            auth.verify_request_at(
                &signature,
                "POST",
                "/internal/memory/files/source?cursor=2",
                b"payload",
                101,
            ),
            Err(MemoryWorkerAuthError::InvalidSignature)
        );
        assert_eq!(
            auth.verify_request_at(
                &signature,
                "POST",
                "/internal/memory/files/source?cursor=1",
                b"changed",
                101,
            ),
            Err(MemoryWorkerAuthError::InvalidSignature)
        );
    }

    #[test]
    fn signatures_expire_and_reject_far_future_timestamps() {
        let auth = MemoryWorkerRequestAuth::new(TEST_SECRET).unwrap();
        let signature = auth.sign_request_at("POST", "/source", b"", 100).unwrap();

        assert_eq!(
            auth.verify_request_at(&signature, "POST", "/source", b"", 161),
            Err(MemoryWorkerAuthError::ExpiredSignature)
        );
        assert!(auth
            .verify_request_at(&signature, "POST", "/source", b"", 160)
            .is_ok());
        assert_eq!(
            auth.verify_request_at(&signature, "POST", "/source", b"", 69),
            Err(MemoryWorkerAuthError::ExpiredSignature)
        );
        assert!(auth
            .verify_request_at(&signature, "POST", "/source", b"", 70)
            .is_ok());
        assert!(auth
            .verify_request_at(&signature, "POST", "/source", b"", 130)
            .is_ok());
    }

    #[test]
    fn malformed_signatures_are_rejected() {
        let auth = MemoryWorkerRequestAuth::new(TEST_SECRET).unwrap();
        assert_eq!(
            auth.verify_request_at("t=100,v1=bad", "POST", "/source", b"", 100),
            Err(MemoryWorkerAuthError::MalformedSignature)
        );
    }

    #[test]
    fn canonical_request_rejects_non_origin_paths_and_ambiguous_headers() {
        assert_eq!(
            canonical_request("POST", "https://example.test/source", b""),
            Err(MemoryWorkerAuthError::InvalidRequest)
        );
        assert_eq!(
            canonical_request("POST", "/source\nforged", b""),
            Err(MemoryWorkerAuthError::InvalidRequest)
        );
        assert_eq!(
            canonical_request("GE T", "/source", b""),
            Err(MemoryWorkerAuthError::InvalidRequest)
        );
    }
}
