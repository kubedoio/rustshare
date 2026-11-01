//! Default-off software DEK wrapper backed by operator-mounted key files.
//!
//! The key files are plaintext inside the protected, read-only mount and in
//! this process's memory. This is not an HSM boundary.

use crate::managed_identity_vault::{
    DekKeyManagement, KeyEncryptionContext, VaultError, WrappedDek,
};
use aes_gcm::{
    aead::{consts::U12, Aead, KeyInit, Payload},
    Aes256Gcm, Nonce,
};
use async_trait::async_trait;
#[cfg(unix)]
use std::fs::OpenOptions;
use std::{
    collections::BTreeMap,
    fmt,
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
};
use zeroize::Zeroizing;

const WRAPPED_DEK_BYTES: usize = 12 + 32 + 16;
const WRAP_AAD_DOMAIN: &[u8] = b"rustshare-managed-identity-dek-wrap-v1\0";

/// Non-secret provider configuration. Secret bytes are read only from `key_dir`.
pub struct LocalDekKeyProviderConfig {
    pub key_dir: PathBuf,
    pub key_id: String,
    pub active_key_version: String,
}

/// Software key wrapper for the existing managed identity envelope.
///
/// All loaded master keys remain available for unwrap so persisted DEKs and
/// retained database backups can be recovered after active-version rotation.
pub struct LocalDekKeyManagement {
    key_id: String,
    active_key_version: String,
    key_versions: BTreeMap<String, Zeroizing<[u8; 32]>>,
}

impl fmt::Debug for LocalDekKeyManagement {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LocalDekKeyManagement")
            .field("key_id", &self.key_id)
            .field("active_key_version", &self.active_key_version)
            .field(
                "key_versions",
                &self.key_versions.keys().collect::<Vec<_>>(),
            )
            .finish()
    }
}

impl LocalDekKeyManagement {
    /// Load a read-only key directory. Every entry must be a protected
    /// `<version>.key` file containing exactly 32 random bytes.
    pub fn load(config: LocalDekKeyProviderConfig) -> Result<Self, VaultError> {
        if !valid_identifier(&config.key_id) || !valid_identifier(&config.active_key_version) {
            return Err(VaultError::KeyManagement);
        }
        let directory_metadata =
            fs::symlink_metadata(&config.key_dir).map_err(|_| VaultError::KeyManagement)?;
        if directory_metadata.file_type().is_symlink() || !directory_metadata.is_dir() {
            return Err(VaultError::KeyManagement);
        }
        verify_protected_metadata(&directory_metadata, true)?;

        let mut key_versions = BTreeMap::new();
        for entry in fs::read_dir(&config.key_dir).map_err(|_| VaultError::KeyManagement)? {
            let entry = entry.map_err(|_| VaultError::KeyManagement)?;
            let file_name = entry.file_name();
            let Some(file_name) = file_name.to_str() else {
                return Err(VaultError::KeyManagement);
            };
            let Some(version) = file_name.strip_suffix(".key") else {
                return Err(VaultError::KeyManagement);
            };
            if !valid_identifier(version) || key_versions.contains_key(version) {
                return Err(VaultError::KeyManagement);
            }
            let key = read_key_file(&entry.path())?;
            key_versions.insert(version.to_owned(), key);
        }
        if key_versions.is_empty() || !key_versions.contains_key(&config.active_key_version) {
            return Err(VaultError::KeyManagement);
        }

        Ok(Self {
            key_id: config.key_id,
            active_key_version: config.active_key_version,
            key_versions,
        })
    }
}

#[async_trait]
impl DekKeyManagement for LocalDekKeyManagement {
    async fn wrap_dek(
        &self,
        dek: &[u8; 32],
        context: &KeyEncryptionContext,
    ) -> Result<WrappedDek, VaultError> {
        let key = self
            .key_versions
            .get(&self.active_key_version)
            .ok_or(VaultError::KeyManagement)?;
        let cipher = Aes256Gcm::new_from_slice(&key[..]).map_err(|_| VaultError::KeyManagement)?;
        let nonce_bytes = rand::random::<[u8; 12]>();
        let nonce = Nonce::<U12>::from(nonce_bytes);
        let ciphertext = cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: dek,
                    aad: &wrap_aad(context, &self.key_id, &self.active_key_version),
                },
            )
            .map_err(|_| VaultError::KeyManagement)?;
        let mut encoded = Vec::with_capacity(WRAPPED_DEK_BYTES);
        encoded.extend_from_slice(&nonce_bytes);
        encoded.extend_from_slice(&ciphertext);

        Ok(WrappedDek {
            key_id: self.key_id.clone(),
            key_version: self.active_key_version.clone(),
            ciphertext: encoded,
        })
    }

    async fn unwrap_dek(
        &self,
        wrapped: &WrappedDek,
        context: &KeyEncryptionContext,
    ) -> Result<Zeroizing<[u8; 32]>, VaultError> {
        if wrapped.key_id != self.key_id || wrapped.ciphertext.len() != WRAPPED_DEK_BYTES {
            return Err(VaultError::KeyManagement);
        }
        let key = self
            .key_versions
            .get(&wrapped.key_version)
            .ok_or(VaultError::KeyManagement)?;
        let cipher = Aes256Gcm::new_from_slice(&key[..]).map_err(|_| VaultError::KeyManagement)?;
        let nonce = Nonce::<U12>::try_from(&wrapped.ciphertext[..12])
            .map_err(|_| VaultError::KeyManagement)?;
        let plaintext = Zeroizing::new(
            cipher
                .decrypt(
                    &nonce,
                    Payload {
                        msg: &wrapped.ciphertext[12..],
                        aad: &wrap_aad(context, &wrapped.key_id, &wrapped.key_version),
                    },
                )
                .map_err(|_| VaultError::KeyManagement)?,
        );
        let dek = plaintext
            .as_slice()
            .try_into()
            .map_err(|_| VaultError::KeyManagement)?;
        Ok(Zeroizing::new(dek))
    }
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value != "."
        && value != ".."
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn wrap_aad(context: &KeyEncryptionContext, key_id: &str, key_version: &str) -> Vec<u8> {
    let mut aad = Vec::with_capacity(WRAP_AAD_DOMAIN.len() + 128 + context.public_key.len());
    aad.extend_from_slice(WRAP_AAD_DOMAIN);
    append_field(&mut aad, key_id.as_bytes());
    append_field(&mut aad, key_version.as_bytes());
    aad.extend_from_slice(context.tenant_id.as_bytes());
    aad.extend_from_slice(context.principal_id.as_bytes());
    aad.extend_from_slice(context.identity_id.as_bytes());
    aad.extend_from_slice(&context.generation.to_be_bytes());
    aad.extend_from_slice(&context.envelope_version.to_be_bytes());
    append_field(&mut aad, context.public_key.as_bytes());
    aad
}

fn append_field(output: &mut Vec<u8>, value: &[u8]) {
    output.extend_from_slice(&(value.len() as u64).to_be_bytes());
    output.extend_from_slice(value);
}

fn read_key_file(path: &Path) -> Result<Zeroizing<[u8; 32]>, VaultError> {
    let file = protected_file(path)?;
    if file
        .metadata()
        .map_err(|_| VaultError::KeyManagement)?
        .len()
        != 32
    {
        return Err(VaultError::KeyManagement);
    }
    let mut key = Zeroizing::new([0u8; 32]);
    let mut file = file;
    file.read_exact(&mut *key)
        .map_err(|_| VaultError::KeyManagement)?;
    Ok(key)
}

#[cfg(unix)]
fn protected_file(path: &Path) -> Result<File, VaultError> {
    use std::os::unix::fs::OpenOptionsExt;

    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .map_err(|_| VaultError::KeyManagement)?;
    let metadata = file.metadata().map_err(|_| VaultError::KeyManagement)?;
    if !metadata.is_file() {
        return Err(VaultError::KeyManagement);
    }
    verify_protected_metadata(&metadata, false)?;
    Ok(file)
}

#[cfg(not(unix))]
fn protected_file(_path: &Path) -> Result<File, VaultError> {
    Err(VaultError::KeyManagement)
}

#[cfg(unix)]
fn verify_protected_metadata(metadata: &fs::Metadata, directory: bool) -> Result<(), VaultError> {
    use std::os::unix::fs::MetadataExt;

    let mode = metadata.mode();
    let owner_is_service = metadata.uid() == unsafe { libc::geteuid() };
    let is_read_only_private = mode & 0o077 == 0 && mode & 0o222 == 0;
    let owner_can_read = if directory {
        mode & 0o500 == 0o500
    } else {
        mode & 0o400 != 0 && mode & 0o111 == 0
    };
    if !owner_is_service || !is_read_only_private || !owner_can_read {
        return Err(VaultError::KeyManagement);
    }
    Ok(())
}

#[cfg(not(unix))]
fn verify_protected_metadata(_metadata: &fs::Metadata, _directory: bool) -> Result<(), VaultError> {
    Err(VaultError::KeyManagement)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::managed_identity_vault::{IdentityContext, ManagedIdentityEnvelope};
    use std::{fs, os::unix::fs::PermissionsExt};
    use uuid::Uuid;

    fn write_key(directory: &Path, version: &str, byte: u8) {
        let path = directory.join(format!("{version}.key"));
        fs::write(path, [byte; 32]).unwrap();
        fs::set_permissions(
            directory.join(format!("{version}.key")),
            fs::Permissions::from_mode(0o400),
        )
        .unwrap();
    }

    fn provider(directory: &Path, active: &str) -> LocalDekKeyManagement {
        fs::set_permissions(directory, fs::Permissions::from_mode(0o500)).unwrap();
        LocalDekKeyManagement::load(LocalDekKeyProviderConfig {
            key_dir: directory.to_path_buf(),
            key_id: "elembra-chat".into(),
            active_key_version: active.into(),
        })
        .unwrap()
    }

    fn identity_context() -> IdentityContext {
        IdentityContext {
            tenant_id: Uuid::new_v4(),
            principal_id: Uuid::new_v4(),
            identity_id: Uuid::new_v4(),
            generation: 1,
        }
    }

    #[tokio::test]
    async fn envelope_v1_survives_rotation_rewrap_and_key_bundle_restore() {
        let source = tempfile::tempdir().unwrap();
        write_key(source.path(), "v1", 0x41);
        write_key(source.path(), "v2", 0x52);
        let original_provider = provider(source.path(), "v1");
        let context = identity_context();
        let mut envelope = ManagedIdentityEnvelope::generate(context, &original_provider)
            .await
            .unwrap();
        assert_eq!(envelope.version, 1);
        assert_eq!(envelope.wrapped_dek.key_version, "v1");

        let rotated_provider = provider(source.path(), "v2");
        rotated_provider
            .unwrap_dek(
                &envelope.wrapped_dek,
                &crate::managed_identity_vault::KeyEncryptionContext {
                    tenant_id: context.tenant_id,
                    principal_id: context.principal_id,
                    identity_id: context.identity_id,
                    generation: context.generation,
                    public_key: envelope.public_key.clone(),
                    envelope_version: envelope.version,
                },
            )
            .await
            .unwrap();
        envelope.rewrap_dek(&rotated_provider).await.unwrap();
        assert_eq!(envelope.wrapped_dek.key_version, "v2");

        let restored = tempfile::tempdir().unwrap();
        for version in ["v1", "v2"] {
            fs::copy(
                source.path().join(format!("{version}.key")),
                restored.path().join(format!("{version}.key")),
            )
            .unwrap();
            fs::set_permissions(
                restored.path().join(format!("{version}.key")),
                fs::Permissions::from_mode(0o400),
            )
            .unwrap();
        }
        fs::set_permissions(restored.path(), fs::Permissions::from_mode(0o500)).unwrap();
        let recovered = envelope
            .recover(context, &provider(restored.path(), "v2"))
            .await
            .unwrap();
        assert_eq!(recovered.public_key(), envelope.public_key);
    }

    #[tokio::test]
    async fn wrapper_rejects_context_ciphertext_key_and_version_tampering() {
        let directory = tempfile::tempdir().unwrap();
        write_key(directory.path(), "v1", 0x33);
        write_key(directory.path(), "v2", 0x44);
        let provider = provider(directory.path(), "v1");
        let context = crate::managed_identity_vault::KeyEncryptionContext {
            tenant_id: Uuid::new_v4(),
            principal_id: Uuid::new_v4(),
            identity_id: Uuid::new_v4(),
            generation: 4,
            public_key: "ab".repeat(32),
            envelope_version: 1,
        };
        let mut wrapped = provider.wrap_dek(&[0x77; 32], &context).await.unwrap();
        assert_eq!(
            &provider.unwrap_dek(&wrapped, &context).await.unwrap()[..],
            &[0x77; 32]
        );

        let mut wrong_context = context.clone();
        wrong_context.generation += 1;
        assert!(provider.unwrap_dek(&wrapped, &wrong_context).await.is_err());
        wrong_context = context.clone();
        wrong_context.tenant_id = Uuid::new_v4();
        assert!(provider.unwrap_dek(&wrapped, &wrong_context).await.is_err());
        wrong_context = context.clone();
        wrong_context.principal_id = Uuid::new_v4();
        assert!(provider.unwrap_dek(&wrapped, &wrong_context).await.is_err());
        wrong_context = context.clone();
        wrong_context.identity_id = Uuid::new_v4();
        assert!(provider.unwrap_dek(&wrapped, &wrong_context).await.is_err());
        wrong_context = context.clone();
        wrong_context.public_key = "cd".repeat(32);
        assert!(provider.unwrap_dek(&wrapped, &wrong_context).await.is_err());
        wrong_context = context.clone();
        wrong_context.envelope_version += 1;
        assert!(provider.unwrap_dek(&wrapped, &wrong_context).await.is_err());

        wrapped.ciphertext[20] ^= 1;
        assert!(provider.unwrap_dek(&wrapped, &context).await.is_err());
        wrapped.ciphertext[20] ^= 1;
        wrapped.key_id = "other".into();
        assert!(provider.unwrap_dek(&wrapped, &context).await.is_err());
        wrapped.key_id = "elembra-chat".into();
        wrapped.key_version = "v2".into();
        assert!(provider.unwrap_dek(&wrapped, &context).await.is_err());
        wrapped.key_version = "unknown".into();
        assert!(provider.unwrap_dek(&wrapped, &context).await.is_err());
        assert!(!format!("{provider:?}").contains(&"33".repeat(32)));
    }

    #[test]
    fn provider_fails_closed_on_missing_symlinked_and_permissive_files() {
        let missing_parent = tempfile::tempdir().unwrap();
        assert!(LocalDekKeyManagement::load(LocalDekKeyProviderConfig {
            key_dir: missing_parent.path().join("missing"),
            key_id: "elembra-chat".into(),
            active_key_version: "v1".into(),
        })
        .is_err());

        let directory = tempfile::tempdir().unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        use std::os::unix::fs::PermissionsExt;
        fs::write(outside.path(), [0x21; 32]).unwrap();
        fs::set_permissions(outside.path(), fs::Permissions::from_mode(0o400)).unwrap();
        std::os::unix::fs::symlink(outside.path(), directory.path().join("v1.key")).unwrap();
        fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o500)).unwrap();
        assert!(LocalDekKeyManagement::load(LocalDekKeyProviderConfig {
            key_dir: directory.path().to_path_buf(),
            key_id: "elembra-chat".into(),
            active_key_version: "v1".into(),
        })
        .is_err());

        let permissive_directory = tempfile::tempdir().unwrap();
        write_key(permissive_directory.path(), "v1", 0x15);
        fs::set_permissions(
            permissive_directory.path(),
            fs::Permissions::from_mode(0o750),
        )
        .unwrap();
        assert!(LocalDekKeyManagement::load(LocalDekKeyProviderConfig {
            key_dir: permissive_directory.path().to_path_buf(),
            key_id: "elembra-chat".into(),
            active_key_version: "v1".into(),
        })
        .is_err());

        let permissive_file_directory = tempfile::tempdir().unwrap();
        fs::write(permissive_file_directory.path().join("v1.key"), [0x21; 32]).unwrap();
        fs::set_permissions(
            permissive_file_directory.path().join("v1.key"),
            fs::Permissions::from_mode(0o440),
        )
        .unwrap();
        fs::set_permissions(
            permissive_file_directory.path(),
            fs::Permissions::from_mode(0o500),
        )
        .unwrap();
        assert!(LocalDekKeyManagement::load(LocalDekKeyProviderConfig {
            key_dir: permissive_file_directory.path().to_path_buf(),
            key_id: "elembra-chat".into(),
            active_key_version: "v1".into(),
        })
        .is_err());
    }
}
