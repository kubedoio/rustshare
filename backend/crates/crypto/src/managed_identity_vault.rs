//! Inert envelope encryption primitives for managed Nostr identities.
//!
//! This module has no server, route, worker, or configuration integration.

use aes_gcm::{
    aead::{consts::U12, Aead, KeyInit, Payload},
    Aes256Gcm, Nonce,
};
use async_trait::async_trait;
use nostr::Keys;
use rand::RngExt;
use std::fmt;
use thiserror::Error;
use uuid::Uuid;
use zeroize::{Zeroize, Zeroizing};

pub const ENVELOPE_VERSION: u16 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IdentityContext {
    pub tenant_id: Uuid,
    pub principal_id: Uuid,
    pub identity_id: Uuid,
    pub generation: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeyEncryptionContext {
    pub tenant_id: Uuid,
    pub principal_id: Uuid,
    pub identity_id: Uuid,
    pub generation: i64,
    pub public_key: String,
    pub envelope_version: u16,
}

impl KeyEncryptionContext {
    fn new(context: IdentityContext, public_key: &str, envelope_version: u16) -> Self {
        Self {
            tenant_id: context.tenant_id,
            principal_id: context.principal_id,
            identity_id: context.identity_id,
            generation: context.generation,
            public_key: public_key.to_owned(),
            envelope_version,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WrappedDek {
    pub key_id: String,
    pub key_version: String,
    pub ciphertext: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManagedIdentityEnvelope {
    pub context: IdentityContext,
    pub public_key: String,
    pub version: u16,
    pub nonce: [u8; 12],
    pub ciphertext: Vec<u8>,
    pub wrapped_dek: WrappedDek,
}

/// External KMS adapter. Implementations need permission only to encrypt and
/// decrypt data keys with the configured KEK; they must not expose KEK bytes.
#[async_trait]
pub trait DekKeyManagement: Send + Sync {
    async fn wrap_dek(
        &self,
        dek: &[u8; 32],
        context: &KeyEncryptionContext,
    ) -> Result<WrappedDek, VaultError>;
    async fn unwrap_dek(
        &self,
        wrapped: &WrappedDek,
        context: &KeyEncryptionContext,
    ) -> Result<Zeroizing<[u8; 32]>, VaultError>;
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum VaultError {
    #[error("managed identity encryption failed")]
    Encryption,
    #[error("managed identity envelope is invalid")]
    InvalidEnvelope,
    #[error("managed identity recovery failed")]
    Recovery,
    #[error("key management operation failed")]
    KeyManagement,
}

/// Recovered material exposes only the public key. It cannot be formatted,
/// serialized, or used by this crate to sign events.
pub struct RecoveredIdentity {
    _secret_key: Zeroizing<[u8; 32]>,
    public_key: String,
}

impl fmt::Debug for RecoveredIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RecoveredIdentity")
            .field("public_key", &self.public_key)
            .finish_non_exhaustive()
    }
}

impl RecoveredIdentity {
    pub fn public_key(&self) -> &str {
        &self.public_key
    }
}

impl ManagedIdentityEnvelope {
    /// Generate a fresh Nostr identity, per-identity DEK, and versioned envelope.
    pub async fn generate(
        context: IdentityContext,
        kms: &impl DekKeyManagement,
    ) -> Result<Self, VaultError> {
        if context.generation <= 0 {
            return Err(VaultError::InvalidEnvelope);
        }

        let keys = Keys::generate();
        let public_key = keys.public_key().to_hex();
        let secret_key = Zeroizing::new(keys.secret_key().to_secret_bytes());
        drop(keys);
        let mut dek = Zeroizing::new([0; 32]);
        rand::rng().fill(&mut dek[..]);
        let nonce = random_nonce();
        let aad = aad(context, &public_key, ENVELOPE_VERSION);
        let cipher = Aes256Gcm::new_from_slice(&dek[..]).map_err(|_| VaultError::Encryption)?;
        let ciphertext = cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: &secret_key[..],
                    aad: &aad,
                },
            )
            .map_err(|_| VaultError::Encryption)?;
        drop(secret_key);
        let kms_context = KeyEncryptionContext::new(context, &public_key, ENVELOPE_VERSION);
        let wrapped_dek = kms.wrap_dek(&dek, &kms_context).await?;

        Ok(Self {
            context,
            public_key,
            version: ENVELOPE_VERSION,
            nonce: nonce
                .as_slice()
                .try_into()
                .map_err(|_| VaultError::Encryption)?,
            ciphertext,
            wrapped_dek,
        })
    }

    /// Recover only after KMS unwrap and AEAD authentication of the full context.
    pub async fn recover(
        &self,
        expected: IdentityContext,
        kms: &impl DekKeyManagement,
    ) -> Result<RecoveredIdentity, VaultError> {
        if self.version != ENVELOPE_VERSION
            || self.context != expected
            || self.context.generation <= 0
            || self.public_key.len() != 64
            || !self.public_key.bytes().all(|byte| byte.is_ascii_hexdigit())
            || self.ciphertext.len() != 48
        {
            return Err(VaultError::InvalidEnvelope);
        }

        let aad = aad(self.context, &self.public_key, self.version);
        let kms_context = KeyEncryptionContext::new(self.context, &self.public_key, self.version);
        let dek = kms.unwrap_dek(&self.wrapped_dek, &kms_context).await?;
        let cipher = Aes256Gcm::new_from_slice(&dek[..]).map_err(|_| VaultError::Recovery)?;
        let nonce = Nonce::<U12>::try_from(self.nonce.as_slice())
            .map_err(|_| VaultError::InvalidEnvelope)?;
        let plaintext = Zeroizing::new(
            cipher
                .decrypt(
                    &nonce,
                    Payload {
                        msg: &self.ciphertext,
                        aad: &aad,
                    },
                )
                .map_err(|_| VaultError::Recovery)?,
        );
        let secret_key: Zeroizing<[u8; 32]> = Zeroizing::new(
            plaintext
                .as_slice()
                .try_into()
                .map_err(|_| VaultError::InvalidEnvelope)?,
        );
        let mut encoded_secret = hex::encode(&secret_key[..]);
        let parsed_keys = Keys::parse(&encoded_secret);
        encoded_secret.zeroize();
        let keys = parsed_keys.map_err(|_| VaultError::Recovery)?;
        if keys.public_key().to_hex() != self.public_key {
            return Err(VaultError::Recovery);
        }

        Ok(RecoveredIdentity {
            _secret_key: secret_key,
            public_key: self.public_key.clone(),
        })
    }

    /// Rewrap only the DEK under the provider's current KEK. The signing key,
    /// identity attribution, nonce, and authenticated ciphertext remain fixed.
    pub async fn rewrap_dek(&mut self, kms: &impl DekKeyManagement) -> Result<(), VaultError> {
        let context = KeyEncryptionContext::new(self.context, &self.public_key, self.version);
        let dek = kms.unwrap_dek(&self.wrapped_dek, &context).await?;
        self.wrapped_dek = kms.wrap_dek(&dek, &context).await?;
        Ok(())
    }
}

fn aad(context: IdentityContext, public_key: &str, version: u16) -> Vec<u8> {
    let mut aad = Vec::with_capacity(16 * 3 + 8 + 2 + public_key.len());
    aad.extend_from_slice(b"rustshare-managed-nostr-identity\0");
    aad.extend_from_slice(context.tenant_id.as_bytes());
    aad.extend_from_slice(context.principal_id.as_bytes());
    aad.extend_from_slice(context.identity_id.as_bytes());
    aad.extend_from_slice(&context.generation.to_be_bytes());
    aad.extend_from_slice(&version.to_be_bytes());
    aad.extend_from_slice(public_key.as_bytes());
    aad
}

fn random_nonce() -> Nonce<U12> {
    rand::random::<[u8; 12]>().into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct TestKms {
        keys: HashMap<String, Zeroizing<[u8; 32]>>,
        current_key_version: String,
        unavailable: bool,
    }

    impl TestKms {
        fn new(byte: u8, key_version: &'static str) -> Self {
            Self {
                keys: HashMap::from([(key_version.to_owned(), Zeroizing::new([byte; 32]))]),
                current_key_version: key_version.to_owned(),
                unavailable: false,
            }
        }

        fn with_rotation(old: Self, byte: u8, key_version: &str) -> Self {
            let mut keys = old.keys;
            keys.insert(key_version.to_owned(), Zeroizing::new([byte; 32]));
            Self {
                keys,
                current_key_version: key_version.to_owned(),
                unavailable: false,
            }
        }
    }

    #[async_trait]
    impl DekKeyManagement for TestKms {
        async fn wrap_dek(
            &self,
            dek: &[u8; 32],
            context: &KeyEncryptionContext,
        ) -> Result<WrappedDek, VaultError> {
            if self.unavailable {
                return Err(VaultError::KeyManagement);
            }
            let key = self
                .keys
                .get(&self.current_key_version)
                .ok_or(VaultError::KeyManagement)?;
            let nonce = random_nonce();
            let cipher =
                Aes256Gcm::new_from_slice(&key[..]).map_err(|_| VaultError::KeyManagement)?;
            let context_aad = kms_context_aad(context);
            let ciphertext = cipher
                .encrypt(
                    &nonce,
                    Payload {
                        msg: dek,
                        aad: &context_aad,
                    },
                )
                .map_err(|_| VaultError::KeyManagement)?;
            let mut wrapped = nonce.as_slice().to_vec();
            wrapped.extend(ciphertext);
            Ok(WrappedDek {
                key_id: "test-only".to_owned(),
                key_version: self.current_key_version.clone(),
                ciphertext: wrapped,
            })
        }

        async fn unwrap_dek(
            &self,
            wrapped: &WrappedDek,
            context: &KeyEncryptionContext,
        ) -> Result<Zeroizing<[u8; 32]>, VaultError> {
            if self.unavailable || wrapped.key_id != "test-only" || wrapped.ciphertext.len() != 60 {
                return Err(VaultError::KeyManagement);
            }
            let key = self
                .keys
                .get(&wrapped.key_version)
                .ok_or(VaultError::KeyManagement)?;
            let cipher =
                Aes256Gcm::new_from_slice(&key[..]).map_err(|_| VaultError::KeyManagement)?;
            let nonce = Nonce::<U12>::try_from(&wrapped.ciphertext[..12])
                .map_err(|_| VaultError::KeyManagement)?;
            let context_aad = kms_context_aad(context);
            let plaintext = cipher
                .decrypt(
                    &nonce,
                    Payload {
                        msg: &wrapped.ciphertext[12..],
                        aad: &context_aad,
                    },
                )
                .map_err(|_| VaultError::KeyManagement)?;
            Ok(Zeroizing::new(
                plaintext
                    .try_into()
                    .map_err(|_| VaultError::KeyManagement)?,
            ))
        }
    }

    fn kms_context_aad(context: &KeyEncryptionContext) -> Vec<u8> {
        aad(
            IdentityContext {
                tenant_id: context.tenant_id,
                principal_id: context.principal_id,
                identity_id: context.identity_id,
                generation: context.generation,
            },
            &context.public_key,
            context.envelope_version,
        )
    }

    fn context() -> IdentityContext {
        IdentityContext {
            tenant_id: Uuid::new_v4(),
            principal_id: Uuid::new_v4(),
            identity_id: Uuid::new_v4(),
            generation: 1,
        }
    }

    #[tokio::test]
    async fn roundtrip_is_bound_to_tenant_principal_and_generation() {
        let context = context();
        let kms = TestKms::new(41, "v1");
        let envelope = ManagedIdentityEnvelope::generate(context, &kms)
            .await
            .unwrap();
        assert_eq!(
            envelope.recover(context, &kms).await.unwrap().public_key(),
            envelope.public_key
        );
        for changed in [
            IdentityContext {
                tenant_id: Uuid::new_v4(),
                ..context
            },
            IdentityContext {
                principal_id: Uuid::new_v4(),
                ..context
            },
            IdentityContext {
                identity_id: Uuid::new_v4(),
                ..context
            },
            IdentityContext {
                generation: 2,
                ..context
            },
        ] {
            assert_eq!(
                envelope.recover(changed, &kms).await.unwrap_err(),
                VaultError::InvalidEnvelope
            );
        }
    }

    #[tokio::test]
    async fn ciphertext_aad_and_public_key_tampering_fail_closed() {
        let context = context();
        let kms = TestKms::new(13, "v1");
        let envelope = ManagedIdentityEnvelope::generate(context, &kms)
            .await
            .unwrap();
        let mut tampered = envelope.clone();
        tampered.ciphertext[0] ^= 1;
        assert_eq!(
            tampered.recover(context, &kms).await.unwrap_err(),
            VaultError::Recovery
        );
        let mut tampered = envelope.clone();
        let replacement = if tampered.public_key.starts_with('0') {
            "1"
        } else {
            "0"
        };
        tampered.public_key.replace_range(0..1, replacement);
        assert!(tampered.recover(context, &kms).await.is_err());
        let mut tampered = envelope;
        tampered.version = 2;
        assert_eq!(
            tampered.recover(context, &kms).await.unwrap_err(),
            VaultError::InvalidEnvelope
        );
    }

    #[tokio::test]
    async fn unavailable_kms_fails_closed() {
        let context = context();
        let kms = TestKms::new(22, "v1");
        let envelope = ManagedIdentityEnvelope::generate(context, &kms)
            .await
            .unwrap();
        let unavailable = TestKms {
            unavailable: true,
            ..TestKms::new(22, "v1")
        };
        assert_eq!(
            envelope.recover(context, &unavailable).await.unwrap_err(),
            VaultError::KeyManagement
        );
    }

    #[tokio::test]
    async fn wrong_kms_key_and_debug_output_do_not_reveal_secrets() {
        let context = context();
        let kms = TestKms::new(22, "v1");
        let envelope = ManagedIdentityEnvelope::generate(context, &kms)
            .await
            .unwrap();
        let wrong_kms = TestKms::new(23, "v1");
        assert_eq!(
            envelope.recover(context, &wrong_kms).await.unwrap_err(),
            VaultError::KeyManagement
        );

        let secret = [0xab; 32];
        let recovered = RecoveredIdentity {
            _secret_key: Zeroizing::new(secret),
            public_key: "public".to_owned(),
        };
        assert!(!format!("{recovered:?}").contains(&hex::encode(secret)));
        assert_eq!(
            VaultError::KeyManagement.to_string(),
            "key management operation failed"
        );
    }

    #[tokio::test]
    async fn rewrap_changes_only_kms_metadata_and_preserves_identity() {
        let context = context();
        let old_kms = TestKms::new(22, "v1");
        let mut envelope = ManagedIdentityEnvelope::generate(context, &old_kms)
            .await
            .unwrap();
        let original = envelope.clone();
        let rotated_kms = TestKms::with_rotation(old_kms, 23, "v2");
        envelope.rewrap_dek(&rotated_kms).await.unwrap();
        assert_eq!(envelope.wrapped_dek.key_version, "v2");
        assert_eq!(envelope.context, original.context);
        assert_eq!(envelope.public_key, original.public_key);
        assert_eq!(envelope.ciphertext, original.ciphertext);
        assert_eq!(
            envelope
                .recover(context, &rotated_kms)
                .await
                .unwrap()
                .public_key(),
            original.public_key
        );
    }
}
