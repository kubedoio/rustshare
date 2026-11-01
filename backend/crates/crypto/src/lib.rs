//! Cryptographic utilities for RustShare, including password hashing and secret encryption.

#[cfg(feature = "local-dek-key-provider")]
pub mod local_dek_key_provider;
pub mod managed_identity_vault;
pub mod password;
pub mod secret_encryption;
pub mod webhook_signature;

pub use password::{PasswordError, PasswordHasher, DUMMY_HASH};
pub use secret_encryption::{decrypt_secret, encrypt_secret, EncryptionError, SecretEncryptionKey};
pub use webhook_signature::{WebhookSignatureError, WebhookSigner};
