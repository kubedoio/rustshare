//! Shared validation and hashing utilities.

use bytes::Bytes;
use sha2::{Digest, Sha256};

/// Validate a file or folder name.
/// Returns Ok(()) if valid, or Err with a descriptive message.
pub fn validate_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Name cannot be empty".to_string());
    }
    if name.contains('/') {
        return Err("Name cannot contain forward slash (/)".to_string());
    }
    if name.contains('\0') {
        return Err("Name cannot contain null character".to_string());
    }
    Ok(())
}

/// Compute SHA-256 hash of byte content.
pub fn calculate_sha256(content: &Bytes) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content);
    hex::encode(hasher.finalize())
}
