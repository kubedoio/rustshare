//! Cryptographic utilities for RustShare, including password hashing.

pub mod password;
pub mod secret_encryption;

pub use password::{PasswordHasher, PasswordError};
pub use secret_encryption::{encrypt_secret, decrypt_secret, EncryptionError};
