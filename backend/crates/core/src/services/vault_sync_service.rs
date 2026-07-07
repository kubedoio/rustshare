//! VaultSyncService for RustShare Vault Sync operations.
//!
//! Obsidian is a trademark of Dynalist Inc. RustShare is not affiliated with,
//! endorsed by, or sponsored by Obsidian.
//!
//! This service implements the core business logic for vault sync, including:
//! - Vault creation and listing
//! - File upload with optimistic revision conflict detection
//! - File download, deletion (tombstoning), and renaming
//! - Manifest generation
//! - Device registration and revocation

use std::sync::Arc;

use bytes::Bytes;
use chrono::Utc;
use uuid::Uuid;

use crate::domain::{
    CreateVaultRequest, DeleteVaultFileRequest, RenameVaultFileRequest,
    SaveVaultFileContentRequest, UploadVaultFileRequest, Vault, VaultDevice, VaultFile,
    VaultFileContentResponse, VaultFileContentSavedResponse, VaultManifest, VaultManifestEntry,
    VaultManifestResult, VaultWritePolicy,
};
use crate::services::{ObjectStoreOps, VaultStore, VaultSyncError};

/// Service for vault sync operations.
///
/// Generic over the metadata store (`VaultStore`) and object store
/// (`ObjectStoreOps`) to enable unit testing without real dependencies.
pub struct VaultSyncService<S: VaultStore, O: ObjectStoreOps> {
    store: Arc<S>,
    object_store: Arc<O>,
}

impl<S: VaultStore, O: ObjectStoreOps> VaultSyncService<S, O> {
    /// Create a new `VaultSyncService`.
    pub fn new(store: Arc<S>, object_store: Arc<O>) -> Self {
        Self {
            store,
            object_store,
        }
    }

    // ─────────────────────────────────────────────
    // Vault management
    // ─────────────────────────────────────────────

    /// Create a new vault.
    ///
    /// The vault is created with `server_rev = 0` and a rooted path under
    /// `My Files/Vaults/Obsidian/`.
    pub async fn create_vault(
        &self,
        req: CreateVaultRequest,
        tenant_id: Uuid,
        owner_user_id: Uuid,
    ) -> Result<Vault, VaultSyncError> {
        let name = self.validate_vault_name(&req.name)?;
        let create_device = self
            .get_bindable_create_device(&req.device_id, tenant_id, owner_user_id)
            .await?;

        // Enforce unique vault names per user.
        let existing = self.store.list_vaults(tenant_id, owner_user_id).await?;
        if existing.iter().any(|v| v.name.eq_ignore_ascii_case(&name)) {
            return Err(VaultSyncError::VaultAlreadyExists(name));
        }

        let root_path = format!("My Files/Vaults/Obsidian/{}", name);
        let vault = Vault {
            id: Uuid::new_v4(),
            tenant_id,
            owner_user_id,
            name,
            adapter: req.adapter,
            root_path: Some(root_path),
            write_policy: VaultWritePolicy::ReadOnly,
            server_rev: 0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let vault = self.store.create_vault(&vault).await?;

        if create_device.is_some() {
            self.store
                .bind_device_to_vault(&req.device_id, tenant_id, vault.id)
                .await?;
            self.store
                .update_device_last_seen(&req.device_id, tenant_id)
                .await?;
        }

        Ok(vault)
    }

    /// Get a vault by ID, verifying the requesting user is the owner.
    pub async fn get_vault(
        &self,
        vault_id: Uuid,
        tenant_id: Uuid,
        user_id: Uuid,
    ) -> Result<Vault, VaultSyncError> {
        let vault = self.store.get_vault(vault_id, tenant_id).await?;
        if vault.owner_user_id != user_id {
            return Err(VaultSyncError::Unauthorized);
        }
        Ok(vault)
    }

    /// List vaults owned by the user within a tenant.
    pub async fn list_vaults(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
    ) -> Result<Vec<Vault>, VaultSyncError> {
        self.store.list_vaults(tenant_id, user_id).await
    }

    const MAX_WEBUI_EDIT_SIZE: i64 = 1024 * 1024; // 1 MiB
    const WEBUI_DEVICE_TYPE: &str = "web_ui";
    const WEBUI_DEVICE_NAME: &str = "RustShare Web UI";

    /// Determine whether a vault file is eligible for WebUI editing.
    fn is_editable_file(file: &VaultFile) -> bool {
        if file.deleted {
            return false;
        }
        if let Some(size) = file.size {
            if size > Self::MAX_WEBUI_EDIT_SIZE {
                return false;
            }
        }
        let path_lower = file.relative_path.to_lowercase();
        if path_lower.ends_with(".md") || path_lower.ends_with(".markdown") {
            return true;
        }
        if path_lower.ends_with(".txt") {
            if let Some(ct) = &file.content_type {
                return ct.starts_with("text/");
            }
            return true;
        }
        false
    }

    /// Build a synthetic WebUI device for a vault.
    fn webui_device(tenant_id: Uuid, user_id: Uuid, vault_id: Uuid) -> VaultDevice {
        VaultDevice {
            id: Uuid::new_v4(),
            tenant_id,
            user_id,
            vault_id: Some(vault_id),
            device_name: Self::WEBUI_DEVICE_NAME.to_string(),
            client_type: Self::WEBUI_DEVICE_TYPE.to_string(),
            client_version: None,
            last_sync_rev: None,
            revoked_at: None,
            created_at: Utc::now(),
            last_seen_at: Utc::now(),
        }
    }

    async fn get_or_create_webui_device(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
        vault_id: Uuid,
    ) -> Result<VaultDevice, VaultSyncError> {
        if let Some(mut device) = self
            .store
            .get_webui_device(tenant_id, user_id, vault_id)
            .await?
        {
            device.last_seen_at = Utc::now();
            self.store
                .update_vault_device_last_seen_at(device.id, device.last_seen_at)
                .await?;
            return Ok(device);
        }
        let device = Self::webui_device(tenant_id, user_id, vault_id);
        self.store.create_webui_device(&device).await?;
        self.store
            .get_webui_device(tenant_id, user_id, vault_id)
            .await?
            .ok_or_else(|| {
                VaultSyncError::Storage("WebUI device not found after creation".to_string())
            })
    }

    // ─────────────────────────────────────────────
    // File operations
    // ─────────────────────────────────────────────

    /// Upload a file into a vault.
    ///
    /// Uses optimistic revision locking: the client supplies the revision it
    /// believes the file is at (`base_server_rev`). If the server revision has
    /// moved on, a [`VaultSyncError::Conflict`] is returned. Uploading over a
    /// tombstoned file returns [`VaultSyncError::Conflict`].
    pub async fn upload_file(
        &self,
        req: UploadVaultFileRequest,
        tenant_id: Uuid,
        user_id: Uuid,
    ) -> Result<VaultFile, VaultSyncError> {
        self.validate_relative_path(&req.relative_path)?;

        if !req.sha256.chars().all(|c| c.is_ascii_hexdigit()) || req.sha256.len() != 64 {
            return Err(VaultSyncError::InvalidName(
                "SHA256 must be 64 hex characters".to_string(),
            ));
        }

        // Verify vault exists and user is the owner.
        let vault = self.store.get_vault(req.vault_id, tenant_id).await?;
        if vault.owner_user_id != user_id {
            return Err(VaultSyncError::Unauthorized);
        }

        self.authorize_write_device(&req.device_id, tenant_id, user_id, req.vault_id)
            .await?;

        let existing = self
            .store
            .get_file_including_deleted(req.vault_id, &req.relative_path, tenant_id)
            .await;

        if let Ok(file) = &existing {
            if file.deleted {
                return Err(VaultSyncError::Conflict {
                    client_rev: req.base_server_rev,
                    current_rev: file.server_rev,
                    server_sha256: file.sha256.clone(),
                });
            }
        }

        // Blob is written to object store before the conditional DB update.
        // This ordering is intentional: a dangling blob is harmless because
        // content-addressed storage deduplicates by SHA-256, but a missing
        // blob would break file references and violate data integrity.
        // Under high contention orphaned blobs may accumulate because a
        // conflict after the write leaves the blob unreferenced.
        // TODO: Implement a background GC worker to reclaim orphaned blobs.
        // The recommended long-term fix is a periodic garbage-collection
        // task that scans for blobs with no referencing DB rows.
        let storage_key = format!("blobs/{}", req.sha256);
        self.object_store
            .put(&storage_key, req.content)
            .await
            .map_err(|e| VaultSyncError::Storage(e.to_string()))?;

        let file = match existing {
            Ok(file) => {
                // Existing file: attempt atomic conditional update guarded by server_rev.
                // The revision is incremented inside the same transaction so a
                // conflict does not leak a skipped revision number.
                let updated = self
                    .store
                    .update_file_conditional_atomic(
                        &VaultFile {
                            id: file.id,
                            tenant_id,
                            vault_id: req.vault_id,
                            relative_path: req.relative_path.clone(),
                            content_type: req.content_type.clone(),
                            sha256: Some(req.sha256.clone()),
                            size: Some(req.size),
                            server_rev: 0, // ignored — set inside transaction
                            mtime_client: None,
                            mtime_server: Utc::now(),
                            deleted: false,
                            deleted_at: None,
                            last_writer_device_id: Some(req.device_id.clone()),
                            created_at: file.created_at,
                            updated_at: Utc::now(),
                        },
                        req.base_server_rev,
                    )
                    .await?;
                match updated {
                    Some(f) => f,
                    None => {
                        return Err(VaultSyncError::Conflict {
                            client_rev: req.base_server_rev,
                            current_rev: file.server_rev,
                            server_sha256: file.sha256.clone(),
                        });
                    }
                }
            }
            Err(VaultSyncError::FileNotFound(_)) => {
                // New file: base revision must indicate the client thinks
                // the file does not yet exist (0 or 1).
                if req.base_server_rev != 0 && req.base_server_rev != 1 {
                    return Err(VaultSyncError::Conflict {
                        client_rev: req.base_server_rev,
                        current_rev: 0,
                        server_sha256: None,
                    });
                }
                let file = VaultFile {
                    id: Uuid::new_v4(),
                    tenant_id,
                    vault_id: req.vault_id,
                    relative_path: req.relative_path,
                    content_type: req.content_type,
                    sha256: Some(req.sha256),
                    size: Some(req.size),
                    server_rev: 0, // ignored — set inside transaction
                    mtime_client: None,
                    mtime_server: Utc::now(),
                    deleted: false,
                    deleted_at: None,
                    last_writer_device_id: Some(req.device_id),
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                };
                match self.store.insert_file_atomic(&file).await {
                    Ok(f) => f,
                    Err(VaultSyncError::Database(ref msg))
                        if msg.to_lowercase().contains("duplicate")
                            || msg.to_lowercase().contains("unique constraint") =>
                    {
                        // Unique violation → file was created concurrently.
                        return Err(VaultSyncError::Conflict {
                            client_rev: req.base_server_rev,
                            current_rev: 0,
                            server_sha256: None,
                        });
                    }
                    Err(e) => return Err(e),
                }
            }
            Err(e) => return Err(e),
        };

        Ok(file)
    }

    /// Download a file from a vault.
    ///
    /// Verifies vault ownership and that the file has not been tombstoned.
    pub async fn download_file(
        &self,
        vault_id: Uuid,
        relative_path: &str,
        tenant_id: Uuid,
        user_id: Uuid,
    ) -> Result<(Bytes, Option<String>), VaultSyncError> {
        self.validate_relative_path(relative_path)?;

        let vault = self.store.get_vault(vault_id, tenant_id).await?;
        if vault.owner_user_id != user_id {
            return Err(VaultSyncError::Unauthorized);
        }

        let file = self
            .store
            .get_file(vault_id, relative_path, tenant_id)
            .await?;
        if file.deleted {
            return Err(VaultSyncError::FileNotFound(relative_path.to_string()));
        }

        let sha256 = file
            .sha256
            .ok_or_else(|| VaultSyncError::Storage("File missing sha256 hash".to_string()))?;

        let storage_key = format!("blobs/{}", sha256);
        let bytes = self
            .object_store
            .get(&storage_key)
            .await
            .map_err(|e| VaultSyncError::Storage(e.to_string()))?;

        Ok((bytes, file.content_type))
    }

    /// Load file content for WebUI editing.
    pub async fn get_file_content_for_webui(
        &self,
        vault_id: Uuid,
        relative_path: &str,
        tenant_id: Uuid,
        user_id: Uuid,
    ) -> Result<VaultFileContentResponse, VaultSyncError> {
        self.validate_relative_path(relative_path)?;

        let vault = self.store.get_vault(vault_id, tenant_id).await?;
        if vault.owner_user_id != user_id {
            return Err(VaultSyncError::Unauthorized);
        }

        let file = self
            .store
            .get_file(vault_id, relative_path, tenant_id)
            .await?;
        if file.deleted {
            return Err(VaultSyncError::FileNotFound(relative_path.to_string()));
        }

        if !Self::is_editable_file(&file) {
            return Err(VaultSyncError::NotEditable(relative_path.to_string()));
        }

        let sha256 = file
            .sha256
            .ok_or_else(|| VaultSyncError::Storage("File missing sha256 hash".to_string()))?;
        let storage_key = format!("blobs/{}", sha256);
        let bytes = self
            .object_store
            .get(&storage_key)
            .await
            .map_err(|e| VaultSyncError::Storage(e.to_string()))?;

        let content = String::from_utf8(bytes.to_vec())
            .map_err(|_| VaultSyncError::NotEditable("file is not valid UTF-8".to_string()))?;

        Ok(VaultFileContentResponse {
            path: file.relative_path,
            content,
            server_rev: file.server_rev,
            content_type: file.content_type,
            size: file.size.unwrap_or(0),
        })
    }

    /// Save file content from the WebUI.
    pub async fn save_file_content_for_webui(
        &self,
        vault_id: Uuid,
        relative_path: &str,
        req: SaveVaultFileContentRequest,
        tenant_id: Uuid,
        user_id: Uuid,
    ) -> Result<VaultFileContentSavedResponse, VaultSyncError> {
        self.validate_relative_path(relative_path)?;

        let vault = self.store.get_vault(vault_id, tenant_id).await?;
        if vault.owner_user_id != user_id {
            return Err(VaultSyncError::Unauthorized);
        }

        match vault.write_policy {
            VaultWritePolicy::ReadOnly | VaultWritePolicy::SyncClientOnly => {
                return Err(VaultSyncError::WritePolicyDenied {
                    policy: vault.write_policy.to_string(),
                });
            }
            VaultWritePolicy::WebEditingEnabled => {}
        }

        let existing = self
            .store
            .get_file_including_deleted(vault_id, relative_path, tenant_id)
            .await;

        let file = match existing {
            Ok(f) if f.deleted => {
                return Err(VaultSyncError::TombstoneConflict);
            }
            Ok(f) => f,
            Err(VaultSyncError::FileNotFound(_)) => {
                return Err(VaultSyncError::FileNotFound(relative_path.to_string()));
            }
            Err(e) => return Err(e),
        };

        if !Self::is_editable_file(&file) {
            return Err(VaultSyncError::NotEditable(relative_path.to_string()));
        }

        let expected_revision = req.expected_revision;
        let content_bytes = Bytes::from(req.content.into_bytes());
        let content_len = content_bytes.len() as i64;
        if content_len > Self::MAX_WEBUI_EDIT_SIZE {
            return Err(VaultSyncError::NotEditable(format!(
                "file exceeds {} bytes",
                Self::MAX_WEBUI_EDIT_SIZE
            )));
        }

        let sha256 = {
            use sha2::{Digest, Sha256};
            hex::encode(Sha256::digest(&content_bytes))
        };

        let device = self
            .get_or_create_webui_device(tenant_id, user_id, vault_id)
            .await?;

        // Write blob before conditional DB update (same rationale as upload_file).
        let storage_key = format!("blobs/{}", sha256);
        self.object_store
            .put(&storage_key, content_bytes)
            .await
            .map_err(|e| VaultSyncError::Storage(e.to_string()))?;

        let updated = self
            .store
            .update_file_conditional_atomic(
                &VaultFile {
                    id: file.id,
                    tenant_id,
                    vault_id,
                    relative_path: relative_path.to_string(),
                    content_type: file
                        .content_type
                        .clone()
                        .or_else(|| Some("text/plain".to_string())),
                    sha256: Some(sha256),
                    size: Some(content_len),
                    server_rev: 0, // set inside transaction
                    mtime_client: None,
                    mtime_server: Utc::now(),
                    deleted: false,
                    deleted_at: None,
                    last_writer_device_id: Some(device.id.to_string()),
                    created_at: file.created_at,
                    updated_at: Utc::now(),
                },
                expected_revision,
            )
            .await?;

        match updated {
            Some(f) => Ok(VaultFileContentSavedResponse {
                path: f.relative_path,
                server_rev: f.server_rev,
                updated_at: f.updated_at,
            }),
            None => {
                let current = self
                    .store
                    .get_file_including_deleted(vault_id, relative_path, tenant_id)
                    .await?;
                Err(VaultSyncError::Conflict {
                    client_rev: expected_revision,
                    current_rev: current.server_rev,
                    server_sha256: current.sha256.clone(),
                })
            }
        }
    }

    /// Delete (tombstone) a file in a vault.
    ///
    /// Checks `base_server_rev` atomically before applying the tombstone to
    /// prevent silently overwriting concurrent changes.
    pub async fn delete_file(
        &self,
        req: DeleteVaultFileRequest,
        tenant_id: Uuid,
        user_id: Uuid,
    ) -> Result<VaultFile, VaultSyncError> {
        self.validate_relative_path(&req.relative_path)?;

        let vault = self.store.get_vault(req.vault_id, tenant_id).await?;
        if vault.owner_user_id != user_id {
            return Err(VaultSyncError::Unauthorized);
        }

        self.authorize_write_device(&req.device_id, tenant_id, user_id, req.vault_id)
            .await?;

        let file = self
            .store
            .get_file(req.vault_id, &req.relative_path, tenant_id)
            .await?;

        // Revision is incremented atomically inside the transaction so a
        // conflict does not leak a skipped revision number.
        let tombstoned = self
            .store
            .tombstone_file_conditional_atomic(
                req.vault_id,
                &req.relative_path,
                tenant_id,
                req.base_server_rev,
                &req.device_id,
            )
            .await?;
        match tombstoned {
            Some(f) => Ok(f),
            None => Err(VaultSyncError::Conflict {
                client_rev: req.base_server_rev,
                current_rev: file.server_rev,
                server_sha256: file.sha256.clone(),
            }),
        }
    }

    /// Rename a file within a vault.
    ///
    /// Checks `base_server_rev` atomically before applying the rename to
    /// prevent silently overwriting concurrent changes.
    pub async fn rename_file(
        &self,
        req: RenameVaultFileRequest,
        tenant_id: Uuid,
        user_id: Uuid,
    ) -> Result<VaultFile, VaultSyncError> {
        self.validate_relative_path(&req.old_path)?;
        self.validate_relative_path(&req.new_path)?;

        let vault = self.store.get_vault(req.vault_id, tenant_id).await?;
        if vault.owner_user_id != user_id {
            return Err(VaultSyncError::Unauthorized);
        }

        self.authorize_write_device(&req.device_id, tenant_id, user_id, req.vault_id)
            .await?;

        let file = self
            .store
            .get_file(req.vault_id, &req.old_path, tenant_id)
            .await?;

        // Revision is incremented atomically inside the transaction so a
        // conflict does not leak a skipped revision number.
        let renamed = self
            .store
            .rename_file_conditional_atomic(
                req.vault_id,
                &req.old_path,
                &req.new_path,
                tenant_id,
                req.base_server_rev,
                &req.device_id,
            )
            .await?;
        match renamed {
            Some(f) => Ok(f),
            None => Err(VaultSyncError::Conflict {
                client_rev: req.base_server_rev,
                current_rev: file.server_rev,
                server_sha256: file.sha256.clone(),
            }),
        }
    }

    // ─────────────────────────────────────────────
    // Manifest
    // ─────────────────────────────────────────────

    /// Get the current manifest for a vault.
    ///
    /// The manifest includes **all** files, including tombstones, so that
    /// clients can correctly reconcile their local state.
    ///
    /// NOTE: This is NOT snapshot-isolated. The vault.server_rev and the file list
    /// are fetched in separate queries. Under high concurrency, the manifest may
    /// reflect a server_rev newer than some of its entries. Clients must handle
    /// this by using the returned server_rev as an upper bound and reconciling
    /// via subsequent incremental syncs.
    pub async fn get_manifest(
        &self,
        vault_id: Uuid,
        tenant_id: Uuid,
        user_id: Uuid,
    ) -> Result<VaultManifestResult, VaultSyncError> {
        let vault = self.store.get_vault(vault_id, tenant_id).await?;
        if vault.owner_user_id != user_id {
            return Err(VaultSyncError::Unauthorized);
        }

        let files = self
            .store
            .list_files(vault_id, tenant_id, Some(10_001))
            .await?;

        const MAX_MANIFEST_ENTRIES: usize = 10_000;
        if files.len() > MAX_MANIFEST_ENTRIES {
            tracing::warn!(
                "Manifest for vault {} exceeds {} entries; refusing partial manifest",
                vault_id,
                MAX_MANIFEST_ENTRIES,
            );
            return Err(VaultSyncError::ManifestTooLarge {
                limit: MAX_MANIFEST_ENTRIES,
            });
        }

        let entries = files
            .iter()
            .map(|f| VaultManifestEntry {
                path: f.relative_path.clone(),
                sha256: f.sha256.clone(),
                size: f.size,
                content_type: f.content_type.clone(),
                server_rev: f.server_rev,
                mtime_server: f.mtime_server,
                deleted: f.deleted,
                deleted_at: f.deleted_at,
            })
            .collect();

        Ok(VaultManifestResult {
            manifest: VaultManifest {
                vault_id,
                adapter: vault.adapter,
                server_rev: vault.server_rev,
                generated_at: Utc::now(),
                files: entries,
            },
            truncated: false,
        })
    }

    // ─────────────────────────────────────────────
    // Device management
    // ─────────────────────────────────────────────

    /// Register a device for vault sync.
    ///
    /// If the device is associated with a vault, verifies that the device's
    /// user is the vault owner.
    pub async fn register_device(
        &self,
        device: VaultDevice,
        caller_user_id: Uuid,
    ) -> Result<VaultDevice, VaultSyncError> {
        if device.user_id != caller_user_id {
            return Err(VaultSyncError::Unauthorized);
        }
        if let Some(vault_id) = device.vault_id {
            let vault = self.store.get_vault(vault_id, device.tenant_id).await?;
            if vault.owner_user_id != device.user_id {
                return Err(VaultSyncError::Unauthorized);
            }
        }
        self.store.register_device(&device).await
    }

    /// Revoke a device.
    ///
    /// Verifies that the requesting user owns the vault the device is
    /// associated with (or owns the device itself if unbound).
    pub async fn revoke_device(
        &self,
        device_id: Uuid,
        tenant_id: Uuid,
        user_id: Uuid,
    ) -> Result<(), VaultSyncError> {
        let device = self
            .store
            .get_device(&device_id.to_string(), tenant_id)
            .await?;

        if device.revoked_at.is_some() {
            return Err(VaultSyncError::DeviceRevoked);
        }

        if let Some(vault_id) = device.vault_id {
            let vault = self.store.get_vault(vault_id, tenant_id).await?;
            if vault.owner_user_id != user_id {
                return Err(VaultSyncError::Unauthorized);
            }
        } else if device.user_id != user_id {
            return Err(VaultSyncError::Unauthorized);
        }

        self.store.revoke_device(device_id, tenant_id).await
    }

    // ─────────────────────────────────────────────
    // Helpers
    // ─────────────────────────────────────────────

    fn validate_vault_name(&self, name: &str) -> Result<String, VaultSyncError> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(VaultSyncError::InvalidName(
                "vault name is empty".to_string(),
            ));
        }
        if trimmed.len() > 128 {
            return Err(VaultSyncError::InvalidName(
                "vault name exceeds 128 characters".to_string(),
            ));
        }
        if trimmed == "." || trimmed == ".." {
            return Err(VaultSyncError::InvalidName(
                "vault name is reserved".to_string(),
            ));
        }
        if trimmed.contains("..") {
            return Err(VaultSyncError::InvalidName(
                "vault name contains parent directory reference".to_string(),
            ));
        }
        if trimmed.contains('/') || trimmed.contains('\\') {
            return Err(VaultSyncError::InvalidName(
                "vault name contains path separator".to_string(),
            ));
        }
        if trimmed.contains('\0') {
            return Err(VaultSyncError::InvalidName(
                "vault name contains null byte".to_string(),
            ));
        }
        if trimmed.bytes().any(|b| b < 0x20) {
            return Err(VaultSyncError::InvalidName(
                "vault name contains control characters".to_string(),
            ));
        }
        Ok(trimmed.to_string())
    }

    async fn authorize_write_device(
        &self,
        device_id: &str,
        tenant_id: Uuid,
        user_id: Uuid,
        vault_id: Uuid,
    ) -> Result<VaultDevice, VaultSyncError> {
        let device = self.store.get_device(device_id, tenant_id).await?;
        if device.user_id != user_id {
            return Err(VaultSyncError::Unauthorized);
        }
        if device.revoked_at.is_some() {
            return Err(VaultSyncError::DeviceRevoked);
        }

        let device = match device.vault_id {
            Some(bound_vault_id) if bound_vault_id == vault_id => device,
            Some(_) => return Err(VaultSyncError::Unauthorized),
            None => {
                self.store
                    .bind_device_to_vault(device_id, tenant_id, vault_id)
                    .await?
            }
        };

        self.store
            .update_device_last_seen(device_id, tenant_id)
            .await?;
        Ok(device)
    }

    async fn get_bindable_create_device(
        &self,
        device_id: &str,
        tenant_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<VaultDevice>, VaultSyncError> {
        // Device IDs in the database are UUIDs; a non-UUID string can never
        // match a registered device, so treat it as "not found".
        if Uuid::parse_str(device_id).is_err() {
            return Ok(None);
        }

        let device = match self.store.get_device(device_id, tenant_id).await {
            Ok(device) => device,
            Err(VaultSyncError::DeviceNotFound(_)) => return Ok(None),
            Err(err) => return Err(err),
        };

        if device.user_id != user_id {
            return Err(VaultSyncError::Unauthorized);
        }
        if device.revoked_at.is_some() {
            return Err(VaultSyncError::DeviceRevoked);
        }
        if device.vault_id.is_some() {
            return Err(VaultSyncError::Unauthorized);
        }

        Ok(Some(device))
    }

    fn validate_relative_path(&self, path: &str) -> Result<(), VaultSyncError> {
        if path.is_empty() {
            return Err(VaultSyncError::InvalidPath("path is empty".to_string()));
        }
        if path.len() > 4096 {
            let truncated = if path.len() > 100 {
                format!("{}...", &path[..100])
            } else {
                path.to_string()
            };
            return Err(VaultSyncError::InvalidPath(format!(
                "Path exceeds 4096 bytes: {}",
                truncated
            )));
        }
        if path.starts_with('/') {
            return Err(VaultSyncError::InvalidPath(
                "path has leading slash".to_string(),
            ));
        }
        if path.ends_with('/') {
            return Err(VaultSyncError::InvalidPath(
                "path has trailing slash".to_string(),
            ));
        }
        if path.contains('\0') {
            return Err(VaultSyncError::InvalidPath(
                "path contains null byte".to_string(),
            ));
        }
        if path.contains('\\') {
            return Err(VaultSyncError::InvalidPath(
                "path contains backslash".to_string(),
            ));
        }
        for component in path.split('/') {
            if component.is_empty() {
                return Err(VaultSyncError::InvalidPath(
                    "path contains empty component".to_string(),
                ));
            }
            if component == ".." || component == "." {
                return Err(VaultSyncError::InvalidPath(
                    "path contains parent directory reference".to_string(),
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        CreateVaultRequest, DeleteVaultFileRequest, RenameVaultFileRequest,
        SaveVaultFileContentRequest, UploadVaultFileRequest, Vault, VaultAdapter, VaultDevice,
        VaultFile, VaultWritePolicy,
    };
    use crate::services::{ObjectStoreOps, VaultStore, VaultSyncError};
    use bytes::Bytes;
    use chrono::Utc;
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio::sync::Mutex;
    use uuid::Uuid;

    struct MockVaultStore {
        vaults: Mutex<HashMap<Uuid, Vault>>,
        files: Mutex<HashMap<(Uuid, String), VaultFile>>,
        devices: Mutex<HashMap<String, VaultDevice>>,
    }

    impl MockVaultStore {
        fn new() -> Self {
            Self {
                vaults: Mutex::new(HashMap::new()),
                files: Mutex::new(HashMap::new()),
                devices: Mutex::new(HashMap::new()),
            }
        }
    }

    #[allow(async_fn_in_trait)]
    impl VaultStore for MockVaultStore {
        async fn create_vault(&self, vault: &Vault) -> Result<Vault, VaultSyncError> {
            let mut vaults = self.vaults.lock().await;
            if vaults.contains_key(&vault.id) {
                return Err(VaultSyncError::VaultAlreadyExists(vault.name.clone()));
            }
            vaults.insert(vault.id, vault.clone());
            Ok(vault.clone())
        }

        async fn get_vault(
            &self,
            vault_id: Uuid,
            tenant_id: Uuid,
        ) -> Result<Vault, VaultSyncError> {
            let vaults = self.vaults.lock().await;
            let vault = vaults
                .get(&vault_id)
                .ok_or(VaultSyncError::VaultNotFound(vault_id))?;
            if vault.tenant_id != tenant_id {
                return Err(VaultSyncError::VaultNotFound(vault_id));
            }
            Ok(vault.clone())
        }

        async fn list_vaults(
            &self,
            tenant_id: Uuid,
            owner_id: Uuid,
        ) -> Result<Vec<Vault>, VaultSyncError> {
            let vaults = self.vaults.lock().await;
            Ok(vaults
                .values()
                .filter(|v| v.tenant_id == tenant_id && v.owner_user_id == owner_id)
                .cloned()
                .collect())
        }

        async fn get_file(
            &self,
            vault_id: Uuid,
            relative_path: &str,
            tenant_id: Uuid,
        ) -> Result<VaultFile, VaultSyncError> {
            let files = self.files.lock().await;
            let file = files
                .get(&(vault_id, relative_path.to_string()))
                .ok_or_else(|| VaultSyncError::FileNotFound(relative_path.to_string()))?;
            if file.tenant_id != tenant_id || file.deleted {
                return Err(VaultSyncError::FileNotFound(relative_path.to_string()));
            }
            Ok(file.clone())
        }

        async fn get_file_including_deleted(
            &self,
            vault_id: Uuid,
            relative_path: &str,
            tenant_id: Uuid,
        ) -> Result<VaultFile, VaultSyncError> {
            let files = self.files.lock().await;
            let file = files
                .get(&(vault_id, relative_path.to_string()))
                .ok_or_else(|| VaultSyncError::FileNotFound(relative_path.to_string()))?;
            if file.tenant_id != tenant_id {
                return Err(VaultSyncError::FileNotFound(relative_path.to_string()));
            }
            Ok(file.clone())
        }

        async fn list_files(
            &self,
            vault_id: Uuid,
            tenant_id: Uuid,
            _limit: Option<i64>,
        ) -> Result<Vec<VaultFile>, VaultSyncError> {
            let files = self.files.lock().await;
            Ok(files
                .values()
                .filter(|f| f.vault_id == vault_id && f.tenant_id == tenant_id)
                .cloned()
                .collect())
        }

        // Atomic methods: lock order is vaults → files to avoid deadlock.
        // Always acquire vaults first, then files. Never reverse this order.
        async fn insert_file_atomic(&self, file: &VaultFile) -> Result<VaultFile, VaultSyncError> {
            let mut vaults = self.vaults.lock().await;
            let mut files = self.files.lock().await;
            if files.contains_key(&(file.vault_id, file.relative_path.clone())) {
                return Err(VaultSyncError::Database(
                    "duplicate key value violates unique constraint".to_string(),
                ));
            }
            let vault = vaults
                .get_mut(&file.vault_id)
                .ok_or(VaultSyncError::VaultNotFound(file.vault_id))?;
            vault.server_rev += 1;
            let mut inserted = file.clone();
            inserted.server_rev = vault.server_rev;
            files.insert(
                (file.vault_id, file.relative_path.clone()),
                inserted.clone(),
            );
            Ok(inserted)
        }

        async fn update_file_conditional_atomic(
            &self,
            file: &VaultFile,
            base_server_rev: i64,
        ) -> Result<Option<VaultFile>, VaultSyncError> {
            let mut vaults = self.vaults.lock().await;
            let mut files = self.files.lock().await;
            let vault = vaults
                .get_mut(&file.vault_id)
                .ok_or(VaultSyncError::VaultNotFound(file.vault_id))?;
            let entry = files
                .get_mut(&(file.vault_id, file.relative_path.clone()))
                .ok_or_else(|| VaultSyncError::FileNotFound(file.relative_path.clone()))?;
            if entry.server_rev != base_server_rev || entry.deleted {
                return Ok(None);
            }
            vault.server_rev += 1;
            entry.sha256 = file.sha256.clone();
            entry.size = file.size;
            entry.server_rev = vault.server_rev;
            entry.mtime_server = file.mtime_server;
            entry.updated_at = file.updated_at;
            entry.last_writer_device_id = file.last_writer_device_id.clone();
            entry.content_type = file.content_type.clone();
            Ok(Some(entry.clone()))
        }

        async fn tombstone_file_conditional_atomic(
            &self,
            vault_id: Uuid,
            relative_path: &str,
            tenant_id: Uuid,
            base_server_rev: i64,
            device_id: &str,
        ) -> Result<Option<VaultFile>, VaultSyncError> {
            let mut vaults = self.vaults.lock().await;
            let mut files = self.files.lock().await;
            let vault = vaults
                .get_mut(&vault_id)
                .ok_or(VaultSyncError::VaultNotFound(vault_id))?;
            let file = files
                .get_mut(&(vault_id, relative_path.to_string()))
                .ok_or_else(|| VaultSyncError::FileNotFound(relative_path.to_string()))?;
            if file.tenant_id != tenant_id {
                return Err(VaultSyncError::FileNotFound(relative_path.to_string()));
            }
            if file.server_rev != base_server_rev || file.deleted {
                return Ok(None);
            }
            vault.server_rev += 1;
            file.deleted = true;
            file.deleted_at = Some(Utc::now());
            file.server_rev = vault.server_rev;
            file.last_writer_device_id = Some(device_id.to_string());
            file.updated_at = Utc::now();
            Ok(Some(file.clone()))
        }

        async fn rename_file_conditional_atomic(
            &self,
            vault_id: Uuid,
            old_path: &str,
            new_path: &str,
            tenant_id: Uuid,
            base_server_rev: i64,
            device_id: &str,
        ) -> Result<Option<VaultFile>, VaultSyncError> {
            let mut vaults = self.vaults.lock().await;
            let mut files = self.files.lock().await;
            let vault = vaults
                .get_mut(&vault_id)
                .ok_or(VaultSyncError::VaultNotFound(vault_id))?;
            if files.contains_key(&(vault_id, new_path.to_string())) {
                let dest = files.get(&(vault_id, new_path.to_string())).unwrap();
                if !dest.deleted {
                    return Err(VaultSyncError::FileAlreadyExists(new_path.to_string()));
                }
            }
            let file = files
                .get_mut(&(vault_id, old_path.to_string()))
                .ok_or_else(|| VaultSyncError::FileNotFound(old_path.to_string()))?;
            if file.tenant_id != tenant_id {
                return Err(VaultSyncError::FileNotFound(old_path.to_string()));
            }
            if file.server_rev != base_server_rev || file.deleted {
                return Ok(None);
            }
            vault.server_rev += 1;
            let mut new_file = file.clone();
            new_file.relative_path = new_path.to_string();
            new_file.server_rev = vault.server_rev;
            new_file.last_writer_device_id = Some(device_id.to_string());
            new_file.updated_at = Utc::now();
            files.remove(&(vault_id, old_path.to_string()));
            files.insert((vault_id, new_path.to_string()), new_file.clone());
            Ok(Some(new_file))
        }

        async fn register_device(
            &self,
            device: &VaultDevice,
        ) -> Result<VaultDevice, VaultSyncError> {
            let mut devices = self.devices.lock().await;
            devices.insert(device.id.to_string(), device.clone());
            Ok(device.clone())
        }

        async fn get_device(
            &self,
            device_id: &str,
            tenant_id: Uuid,
        ) -> Result<VaultDevice, VaultSyncError> {
            let devices = self.devices.lock().await;
            let device = devices
                .get(device_id)
                .ok_or_else(|| VaultSyncError::DeviceNotFound(device_id.to_string()))?;
            if device.tenant_id != tenant_id {
                return Err(VaultSyncError::DeviceNotFound(device_id.to_string()));
            }
            Ok(device.clone())
        }

        async fn bind_device_to_vault(
            &self,
            device_id: &str,
            tenant_id: Uuid,
            vault_id: Uuid,
        ) -> Result<VaultDevice, VaultSyncError> {
            let mut devices = self.devices.lock().await;
            let device = devices
                .get_mut(device_id)
                .ok_or_else(|| VaultSyncError::DeviceNotFound(device_id.to_string()))?;
            if device.tenant_id != tenant_id {
                return Err(VaultSyncError::DeviceNotFound(device_id.to_string()));
            }
            if device.revoked_at.is_some() {
                return Err(VaultSyncError::DeviceRevoked);
            }
            if matches!(device.vault_id, Some(bound_vault_id) if bound_vault_id != vault_id) {
                return Err(VaultSyncError::Unauthorized);
            }
            device.vault_id = Some(vault_id);
            device.last_seen_at = Utc::now();
            Ok(device.clone())
        }

        async fn update_vault(&self, vault: &Vault) -> Result<Vault, VaultSyncError> {
            let mut vaults = self.vaults.lock().await;
            let entry = vaults
                .get_mut(&vault.id)
                .ok_or(VaultSyncError::VaultNotFound(vault.id))?;
            if entry.tenant_id != vault.tenant_id {
                return Err(VaultSyncError::VaultNotFound(vault.id));
            }
            *entry = vault.clone();
            Ok(entry.clone())
        }

        async fn get_webui_device(
            &self,
            tenant_id: Uuid,
            user_id: Uuid,
            vault_id: Uuid,
        ) -> Result<Option<VaultDevice>, VaultSyncError> {
            let devices = self.devices.lock().await;
            Ok(devices
                .values()
                .find(|d| {
                    d.tenant_id == tenant_id
                        && d.user_id == user_id
                        && d.vault_id == Some(vault_id)
                        && d.client_type == "web_ui"
                        && d.revoked_at.is_none()
                })
                .cloned())
        }

        async fn create_webui_device(
            &self,
            device: &VaultDevice,
        ) -> Result<VaultDevice, VaultSyncError> {
            let mut devices = self.devices.lock().await;
            devices.insert(device.id.to_string(), device.clone());
            Ok(device.clone())
        }

        async fn revoke_device(
            &self,
            device_id: Uuid,
            tenant_id: Uuid,
        ) -> Result<(), VaultSyncError> {
            let mut devices = self.devices.lock().await;
            let device = devices
                .get_mut(&device_id.to_string())
                .ok_or_else(|| VaultSyncError::DeviceNotFound(device_id.to_string()))?;
            if device.tenant_id != tenant_id {
                return Err(VaultSyncError::DeviceNotFound(device_id.to_string()));
            }
            device.revoked_at = Some(Utc::now());
            Ok(())
        }

        async fn update_device_last_seen(
            &self,
            device_id: &str,
            tenant_id: Uuid,
        ) -> Result<(), VaultSyncError> {
            let mut devices = self.devices.lock().await;
            let device = devices
                .get_mut(device_id)
                .ok_or_else(|| VaultSyncError::DeviceNotFound(device_id.to_string()))?;
            if device.tenant_id != tenant_id {
                return Err(VaultSyncError::DeviceNotFound(device_id.to_string()));
            }
            device.last_seen_at = Utc::now();
            Ok(())
        }

        async fn update_vault_device_last_seen_at(
            &self,
            device_id: Uuid,
            last_seen_at: chrono::DateTime<chrono::Utc>,
        ) -> Result<(), VaultSyncError> {
            let mut devices = self.devices.lock().await;
            let device = devices
                .get_mut(&device_id.to_string())
                .ok_or_else(|| VaultSyncError::DeviceNotFound(device_id.to_string()))?;
            device.last_seen_at = last_seen_at;
            Ok(())
        }
    }

    struct MockObjectStore {
        blobs: Mutex<HashMap<String, Bytes>>,
    }

    impl MockObjectStore {
        fn new() -> Self {
            Self {
                blobs: Mutex::new(HashMap::new()),
            }
        }
    }

    impl ObjectStoreOps for MockObjectStore {
        async fn put(&self, key: &str, data: Bytes) -> anyhow::Result<()> {
            let mut blobs = self.blobs.lock().await;
            blobs.insert(key.to_string(), data);
            Ok(())
        }

        async fn put_from_path(&self, key: &str, path: &std::path::Path) -> anyhow::Result<()> {
            let data = tokio::fs::read(path).await?;
            let mut blobs = self.blobs.lock().await;
            blobs.insert(key.to_string(), Bytes::from(data));
            Ok(())
        }

        async fn exists(&self, key: &str) -> anyhow::Result<bool> {
            let blobs = self.blobs.lock().await;
            Ok(blobs.contains_key(key))
        }

        async fn get(&self, key: &str) -> anyhow::Result<Bytes> {
            let blobs = self.blobs.lock().await;
            blobs
                .get(key)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Object not found: {}", key))
        }

        async fn delete(&self, key: &str) -> anyhow::Result<()> {
            let mut blobs = self.blobs.lock().await;
            blobs.remove(key);
            Ok(())
        }
    }

    fn setup() -> (
        Arc<MockVaultStore>,
        Arc<MockObjectStore>,
        VaultSyncService<MockVaultStore, MockObjectStore>,
    ) {
        let store = Arc::new(MockVaultStore::new());
        let object_store = Arc::new(MockObjectStore::new());
        let service = VaultSyncService::new(store.clone(), object_store.clone());
        (store, object_store, service)
    }

    fn test_device(user_id: Uuid, tenant_id: Uuid) -> VaultDevice {
        VaultDevice {
            id: Uuid::new_v4(),
            tenant_id,
            user_id,
            vault_id: None,
            device_name: "Test Device".to_string(),
            client_type: "test".to_string(),
            client_version: None,
            last_sync_rev: None,
            revoked_at: None,
            created_at: Utc::now(),
            last_seen_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn test_create_vault() {
        let (_, _, service) = setup();
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let req = CreateVaultRequest {
            name: "TestVault".to_string(),
            adapter: VaultAdapter::ObsidianVault,
            client_vault_id: None,
            device_id: "device-1".to_string(),
        };

        let vault = service
            .create_vault(req, tenant_id, owner_id)
            .await
            .unwrap();

        assert_eq!(vault.name, "TestVault");
        assert_eq!(vault.adapter, VaultAdapter::ObsidianVault);
        assert_eq!(vault.server_rev, 0);
        assert_eq!(
            vault.root_path,
            Some("My Files/Vaults/Obsidian/TestVault".to_string())
        );
        assert_eq!(vault.tenant_id, tenant_id);
        assert_eq!(vault.owner_user_id, owner_id);
    }

    #[tokio::test]
    async fn test_create_vault_binds_registered_device() {
        let (store, _, service) = setup();
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let device = test_device(owner_id, tenant_id);
        service
            .register_device(device.clone(), owner_id)
            .await
            .unwrap();

        let vault = service
            .create_vault(
                CreateVaultRequest {
                    name: "TestVault".to_string(),
                    adapter: VaultAdapter::ObsidianVault,
                    client_vault_id: None,
                    device_id: device.id.to_string(),
                },
                tenant_id,
                owner_id,
            )
            .await
            .unwrap();

        let bound_device = store
            .get_device(&device.id.to_string(), tenant_id)
            .await
            .unwrap();
        assert_eq!(bound_device.vault_id, Some(vault.id));
    }

    #[tokio::test]
    async fn test_upload_file_new_file() {
        let (_, object_store, service) = setup();
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let vault = service
            .create_vault(
                CreateVaultRequest {
                    name: "TestVault".to_string(),
                    adapter: VaultAdapter::ObsidianVault,
                    client_vault_id: None,
                    device_id: "device-1".to_string(),
                },
                tenant_id,
                owner_id,
            )
            .await
            .unwrap();

        let device = test_device(owner_id, tenant_id);
        service
            .register_device(device.clone(), owner_id)
            .await
            .unwrap();

        let content = Bytes::from_static(b"hello world");
        let req = UploadVaultFileRequest {
            vault_id: vault.id,
            relative_path: "notes/hello.md".to_string(),
            content_type: Some("text/markdown".to_string()),
            sha256: "aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899".to_string(),
            size: content.len() as i64,
            base_server_rev: 0,
            device_id: device.id.to_string(),
            content: content.clone(),
        };

        let file = service.upload_file(req, tenant_id, owner_id).await.unwrap();
        assert_eq!(file.relative_path, "notes/hello.md");
        assert_eq!(file.server_rev, 1);
        assert_eq!(
            file.sha256,
            Some("aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899".to_string())
        );
        assert_eq!(file.size, Some(11));
        assert!(!file.deleted);

        // Blob stored in object store
        let stored = object_store
            .get("blobs/aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899")
            .await
            .unwrap();
        assert_eq!(stored, content);

        // Upload a second file to verify vault rev increment
        let content2 = Bytes::from_static(b"second file");
        let req2 = UploadVaultFileRequest {
            vault_id: vault.id,
            relative_path: "notes/bye.md".to_string(),
            content_type: Some("text/markdown".to_string()),
            sha256: "bbaaccddeeff00112233445566778899aabbccddeeff00112233445566778899".to_string(),
            size: content2.len() as i64,
            base_server_rev: 1,
            device_id: device.id.to_string(),
            content: content2.clone(),
        };
        let file2 = service
            .upload_file(req2, tenant_id, owner_id)
            .await
            .unwrap();
        assert_eq!(file2.server_rev, 2);

        let vault = service
            .get_vault(vault.id, tenant_id, owner_id)
            .await
            .unwrap();
        assert_eq!(vault.server_rev, 2);
    }

    #[tokio::test]
    async fn test_upload_file_update_existing() {
        let (_, _, service) = setup();
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let vault = service
            .create_vault(
                CreateVaultRequest {
                    name: "TestVault".to_string(),
                    adapter: VaultAdapter::ObsidianVault,
                    client_vault_id: None,
                    device_id: "device-1".to_string(),
                },
                tenant_id,
                owner_id,
            )
            .await
            .unwrap();

        let device = test_device(owner_id, tenant_id);
        service
            .register_device(device.clone(), owner_id)
            .await
            .unwrap();

        let content = Bytes::from_static(b"hello world");
        let req = UploadVaultFileRequest {
            vault_id: vault.id,
            relative_path: "notes/hello.md".to_string(),
            content_type: Some("text/markdown".to_string()),
            sha256: "aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899".to_string(),
            size: content.len() as i64,
            base_server_rev: 0,
            device_id: device.id.to_string(),
            content: content.clone(),
        };
        let file = service.upload_file(req, tenant_id, owner_id).await.unwrap();
        assert_eq!(file.server_rev, 1);

        // Update with correct base_rev
        let content2 = Bytes::from_static(b"hello world updated");
        let req2 = UploadVaultFileRequest {
            vault_id: vault.id,
            relative_path: "notes/hello.md".to_string(),
            content_type: Some("text/markdown".to_string()),
            sha256: "ccaaccddeeff00112233445566778899aabbccddeeff00112233445566778899".to_string(),
            size: content2.len() as i64,
            base_server_rev: 1,
            device_id: device.id.to_string(),
            content: content2.clone(),
        };
        let file2 = service
            .upload_file(req2, tenant_id, owner_id)
            .await
            .unwrap();
        assert_eq!(file2.server_rev, 2);
        assert_eq!(
            file2.sha256,
            Some("ccaaccddeeff00112233445566778899aabbccddeeff00112233445566778899".to_string())
        );

        // Update with stale base_rev → Conflict
        let content3 = Bytes::from_static(b"hello world v3");
        let req3 = UploadVaultFileRequest {
            vault_id: vault.id,
            relative_path: "notes/hello.md".to_string(),
            content_type: Some("text/markdown".to_string()),
            sha256: "ddaaccddeeff00112233445566778899aabbccddeeff00112233445566778899".to_string(),
            size: content3.len() as i64,
            base_server_rev: 1, // stale
            device_id: device.id.to_string(),
            content: content3,
        };
        let err = service
            .upload_file(req3, tenant_id, owner_id)
            .await
            .unwrap_err();
        match err {
            VaultSyncError::Conflict {
                client_rev,
                current_rev,
                ..
            } => {
                assert_eq!(client_rev, 1);
                assert_eq!(current_rev, 2);
            }
            _ => panic!("Expected Conflict, got {:?}", err),
        }
    }

    #[tokio::test]
    async fn test_upload_file_tombstone_conflict() {
        let (_, _, service) = setup();
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let vault = service
            .create_vault(
                CreateVaultRequest {
                    name: "TestVault".to_string(),
                    adapter: VaultAdapter::ObsidianVault,
                    client_vault_id: None,
                    device_id: "device-1".to_string(),
                },
                tenant_id,
                owner_id,
            )
            .await
            .unwrap();

        let device = test_device(owner_id, tenant_id);
        service
            .register_device(device.clone(), owner_id)
            .await
            .unwrap();

        let content = Bytes::from_static(b"hello world");
        let req = UploadVaultFileRequest {
            vault_id: vault.id,
            relative_path: "notes/hello.md".to_string(),
            content_type: Some("text/markdown".to_string()),
            sha256: "aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899".to_string(),
            size: content.len() as i64,
            base_server_rev: 0,
            device_id: device.id.to_string(),
            content: content.clone(),
        };
        service.upload_file(req, tenant_id, owner_id).await.unwrap();

        // Tombstone the file
        let del_req = DeleteVaultFileRequest {
            vault_id: vault.id,
            relative_path: "notes/hello.md".to_string(),
            base_server_rev: 1,
            device_id: device.id.to_string(),
        };
        service
            .delete_file(del_req, tenant_id, owner_id)
            .await
            .unwrap();

        // Try to upload over the tombstone with matching rev
        let content2 = Bytes::from_static(b"hello again");
        let req2 = UploadVaultFileRequest {
            vault_id: vault.id,
            relative_path: "notes/hello.md".to_string(),
            content_type: Some("text/markdown".to_string()),
            sha256: "ccaaccddeeff00112233445566778899aabbccddeeff00112233445566778899".to_string(),
            size: content2.len() as i64,
            base_server_rev: 2, // matches tombstoned file rev
            device_id: device.id.to_string(),
            content: content2,
        };
        let err = service
            .upload_file(req2, tenant_id, owner_id)
            .await
            .unwrap_err();
        assert!(matches!(err, VaultSyncError::Conflict { .. }));
    }

    #[tokio::test]
    async fn test_device_binds_to_first_vault_and_cannot_write_another() {
        let (store, _, service) = setup();
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let vault_a = service
            .create_vault(
                CreateVaultRequest {
                    name: "VaultA".to_string(),
                    adapter: VaultAdapter::ObsidianVault,
                    client_vault_id: None,
                    device_id: "device-1".to_string(),
                },
                tenant_id,
                owner_id,
            )
            .await
            .unwrap();
        let vault_b = service
            .create_vault(
                CreateVaultRequest {
                    name: "VaultB".to_string(),
                    adapter: VaultAdapter::ObsidianVault,
                    client_vault_id: None,
                    device_id: "device-1".to_string(),
                },
                tenant_id,
                owner_id,
            )
            .await
            .unwrap();

        let device = test_device(owner_id, tenant_id);
        service
            .register_device(device.clone(), owner_id)
            .await
            .unwrap();

        let req_a = UploadVaultFileRequest {
            vault_id: vault_a.id,
            relative_path: "notes/a.md".to_string(),
            content_type: Some("text/markdown".to_string()),
            sha256: "aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899".to_string(),
            size: 1,
            base_server_rev: 0,
            device_id: device.id.to_string(),
            content: Bytes::from_static(b"a"),
        };
        service
            .upload_file(req_a, tenant_id, owner_id)
            .await
            .unwrap();

        let bound = store
            .get_device(&device.id.to_string(), tenant_id)
            .await
            .unwrap();
        assert_eq!(bound.vault_id, Some(vault_a.id));

        let req_b = UploadVaultFileRequest {
            vault_id: vault_b.id,
            relative_path: "notes/b.md".to_string(),
            content_type: Some("text/markdown".to_string()),
            sha256: "bbaaccddeeff00112233445566778899aabbccddeeff00112233445566778899".to_string(),
            size: 1,
            base_server_rev: 0,
            device_id: device.id.to_string(),
            content: Bytes::from_static(b"b"),
        };
        let err = service
            .upload_file(req_b, tenant_id, owner_id)
            .await
            .unwrap_err();
        assert!(matches!(err, VaultSyncError::Unauthorized));
    }

    #[tokio::test]
    async fn test_manifest_refuses_partial_results() {
        let (store, _, service) = setup();
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let vault = service
            .create_vault(
                CreateVaultRequest {
                    name: "TestVault".to_string(),
                    adapter: VaultAdapter::ObsidianVault,
                    client_vault_id: None,
                    device_id: "device-1".to_string(),
                },
                tenant_id,
                owner_id,
            )
            .await
            .unwrap();

        let mut files = store.files.lock().await;
        for i in 0..10_001 {
            let path = format!("notes/{i}.md");
            files.insert(
                (vault.id, path.clone()),
                VaultFile {
                    id: Uuid::new_v4(),
                    tenant_id,
                    vault_id: vault.id,
                    relative_path: path,
                    content_type: Some("text/markdown".to_string()),
                    sha256: None,
                    size: Some(0),
                    server_rev: i,
                    mtime_client: None,
                    mtime_server: Utc::now(),
                    deleted: false,
                    deleted_at: None,
                    last_writer_device_id: None,
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                },
            );
        }
        drop(files);

        let err = service
            .get_manifest(vault.id, tenant_id, owner_id)
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            VaultSyncError::ManifestTooLarge { limit: 10_000 }
        ));
    }

    #[tokio::test]
    async fn test_download_file() {
        let (_, _, service) = setup();
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let vault = service
            .create_vault(
                CreateVaultRequest {
                    name: "TestVault".to_string(),
                    adapter: VaultAdapter::ObsidianVault,
                    client_vault_id: None,
                    device_id: "device-1".to_string(),
                },
                tenant_id,
                owner_id,
            )
            .await
            .unwrap();

        let device = test_device(owner_id, tenant_id);
        service
            .register_device(device.clone(), owner_id)
            .await
            .unwrap();

        let content = Bytes::from_static(b"hello world");
        let req = UploadVaultFileRequest {
            vault_id: vault.id,
            relative_path: "notes/hello.md".to_string(),
            content_type: Some("text/markdown".to_string()),
            sha256: "aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899".to_string(),
            size: content.len() as i64,
            base_server_rev: 0,
            device_id: device.id.to_string(),
            content: content.clone(),
        };
        service.upload_file(req, tenant_id, owner_id).await.unwrap();

        // Download existing file
        let (bytes, content_type) = service
            .download_file(vault.id, "notes/hello.md", tenant_id, owner_id)
            .await
            .unwrap();
        assert_eq!(bytes, content);
        assert_eq!(content_type, Some("text/markdown".to_string()));

        // Tombstone the file
        let del_req = DeleteVaultFileRequest {
            vault_id: vault.id,
            relative_path: "notes/hello.md".to_string(),
            base_server_rev: 1,
            device_id: device.id.to_string(),
        };
        service
            .delete_file(del_req, tenant_id, owner_id)
            .await
            .unwrap();

        // Download tombstoned file → FileNotFound
        let err = service
            .download_file(vault.id, "notes/hello.md", tenant_id, owner_id)
            .await
            .unwrap_err();
        match err {
            VaultSyncError::FileNotFound(path) => assert_eq!(path, "notes/hello.md"),
            _ => panic!("Expected FileNotFound, got {:?}", err),
        }

        // Download non-existent file → FileNotFound
        let err = service
            .download_file(vault.id, "notes/missing.md", tenant_id, owner_id)
            .await
            .unwrap_err();
        match err {
            VaultSyncError::FileNotFound(path) => assert_eq!(path, "notes/missing.md"),
            _ => panic!("Expected FileNotFound, got {:?}", err),
        }
    }

    #[tokio::test]
    async fn test_delete_file() {
        let (_, _, service) = setup();
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let vault = service
            .create_vault(
                CreateVaultRequest {
                    name: "TestVault".to_string(),
                    adapter: VaultAdapter::ObsidianVault,
                    client_vault_id: None,
                    device_id: "device-1".to_string(),
                },
                tenant_id,
                owner_id,
            )
            .await
            .unwrap();

        let device = test_device(owner_id, tenant_id);
        service
            .register_device(device.clone(), owner_id)
            .await
            .unwrap();

        let content = Bytes::from_static(b"hello world");
        let req = UploadVaultFileRequest {
            vault_id: vault.id,
            relative_path: "notes/hello.md".to_string(),
            content_type: Some("text/markdown".to_string()),
            sha256: "aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899".to_string(),
            size: content.len() as i64,
            base_server_rev: 0,
            device_id: device.id.to_string(),
            content,
        };
        service.upload_file(req, tenant_id, owner_id).await.unwrap();

        // Delete with wrong base_rev → Conflict
        let del_req = DeleteVaultFileRequest {
            vault_id: vault.id,
            relative_path: "notes/hello.md".to_string(),
            base_server_rev: 0,
            device_id: device.id.to_string(),
        };
        let err = service
            .delete_file(del_req, tenant_id, owner_id)
            .await
            .unwrap_err();
        match err {
            VaultSyncError::Conflict {
                client_rev,
                current_rev,
                ..
            } => {
                assert_eq!(client_rev, 0);
                assert_eq!(current_rev, 1);
            }
            _ => panic!("Expected Conflict, got {:?}", err),
        }

        // Delete with correct base_rev → tombstoned
        let del_req = DeleteVaultFileRequest {
            vault_id: vault.id,
            relative_path: "notes/hello.md".to_string(),
            base_server_rev: 1,
            device_id: device.id.to_string(),
        };
        let tombstoned = service
            .delete_file(del_req, tenant_id, owner_id)
            .await
            .unwrap();
        assert!(tombstoned.deleted);
        assert_eq!(tombstoned.server_rev, 2);

        // Vault rev is 2 because the failed conflict attempt did NOT leak a revision
        let vault = service
            .get_vault(vault.id, tenant_id, owner_id)
            .await
            .unwrap();
        assert_eq!(vault.server_rev, 2);
    }

    #[tokio::test]
    async fn test_rename_file() {
        let (_, _, service) = setup();
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let vault = service
            .create_vault(
                CreateVaultRequest {
                    name: "TestVault".to_string(),
                    adapter: VaultAdapter::ObsidianVault,
                    client_vault_id: None,
                    device_id: "device-1".to_string(),
                },
                tenant_id,
                owner_id,
            )
            .await
            .unwrap();

        let device = test_device(owner_id, tenant_id);
        service
            .register_device(device.clone(), owner_id)
            .await
            .unwrap();

        let content = Bytes::from_static(b"hello world");
        let req = UploadVaultFileRequest {
            vault_id: vault.id,
            relative_path: "notes/hello.md".to_string(),
            content_type: Some("text/markdown".to_string()),
            sha256: "aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899".to_string(),
            size: content.len() as i64,
            base_server_rev: 0,
            device_id: device.id.to_string(),
            content,
        };
        service.upload_file(req, tenant_id, owner_id).await.unwrap();

        // Rename with wrong base_rev → Conflict
        let rename_req = RenameVaultFileRequest {
            vault_id: vault.id,
            old_path: "notes/hello.md".to_string(),
            new_path: "notes/renamed.md".to_string(),
            base_server_rev: 0,
            device_id: device.id.to_string(),
        };
        let err = service
            .rename_file(rename_req, tenant_id, owner_id)
            .await
            .unwrap_err();
        match err {
            VaultSyncError::Conflict {
                client_rev,
                current_rev,
                ..
            } => {
                assert_eq!(client_rev, 0);
                assert_eq!(current_rev, 1);
            }
            _ => panic!("Expected Conflict, got {:?}", err),
        }

        // Rename with correct base_rev
        let rename_req = RenameVaultFileRequest {
            vault_id: vault.id,
            old_path: "notes/hello.md".to_string(),
            new_path: "notes/renamed.md".to_string(),
            base_server_rev: 1,
            device_id: device.id.to_string(),
        };
        let renamed = service
            .rename_file(rename_req, tenant_id, owner_id)
            .await
            .unwrap();
        assert_eq!(renamed.relative_path, "notes/renamed.md");
        assert_eq!(renamed.server_rev, 2);

        // Old path no longer exists
        let err = service
            .download_file(vault.id, "notes/hello.md", tenant_id, owner_id)
            .await
            .unwrap_err();
        assert!(matches!(err, VaultSyncError::FileNotFound(_)));
    }

    #[tokio::test]
    async fn test_get_manifest() {
        let (_, _, service) = setup();
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let vault = service
            .create_vault(
                CreateVaultRequest {
                    name: "TestVault".to_string(),
                    adapter: VaultAdapter::ObsidianVault,
                    client_vault_id: None,
                    device_id: "device-1".to_string(),
                },
                tenant_id,
                owner_id,
            )
            .await
            .unwrap();

        let device = test_device(owner_id, tenant_id);
        service
            .register_device(device.clone(), owner_id)
            .await
            .unwrap();

        let content1 = Bytes::from_static(b"file one");
        let req1 = UploadVaultFileRequest {
            vault_id: vault.id,
            relative_path: "notes/active.md".to_string(),
            content_type: Some("text/markdown".to_string()),
            sha256: "1111111111111111111111111111111111111111111111111111111111111111".to_string(),
            size: content1.len() as i64,
            base_server_rev: 0,
            device_id: device.id.to_string(),
            content: content1,
        };
        service
            .upload_file(req1, tenant_id, owner_id)
            .await
            .unwrap();

        let content2 = Bytes::from_static(b"file two");
        let req2 = UploadVaultFileRequest {
            vault_id: vault.id,
            relative_path: "notes/todelete.md".to_string(),
            content_type: Some("text/markdown".to_string()),
            sha256: "2222222222222222222222222222222222222222222222222222222222222222".to_string(),
            size: content2.len() as i64,
            base_server_rev: 1,
            device_id: device.id.to_string(),
            content: content2,
        };
        service
            .upload_file(req2, tenant_id, owner_id)
            .await
            .unwrap();

        // Tombstone one file
        let del_req = DeleteVaultFileRequest {
            vault_id: vault.id,
            relative_path: "notes/todelete.md".to_string(),
            base_server_rev: 2,
            device_id: device.id.to_string(),
        };
        service
            .delete_file(del_req, tenant_id, owner_id)
            .await
            .unwrap();

        let manifest = service
            .get_manifest(vault.id, tenant_id, owner_id)
            .await
            .unwrap()
            .manifest;
        assert_eq!(manifest.vault_id, vault.id);
        assert_eq!(manifest.adapter, VaultAdapter::ObsidianVault);
        assert_eq!(manifest.files.len(), 2);

        let active = manifest
            .files
            .iter()
            .find(|f| f.path == "notes/active.md")
            .unwrap();
        assert!(!active.deleted);
        assert_eq!(active.server_rev, 1);

        let deleted = manifest
            .files
            .iter()
            .find(|f| f.path == "notes/todelete.md")
            .unwrap();
        assert!(deleted.deleted);
        assert_eq!(deleted.server_rev, 3);
        assert!(deleted.deleted_at.is_some());

        // Vault server_rev matches manifest
        let vault = service
            .get_vault(vault.id, tenant_id, owner_id)
            .await
            .unwrap();
        assert_eq!(manifest.server_rev, vault.server_rev);
    }

    #[tokio::test]
    async fn test_path_validation() {
        let (_, _, service) = setup();
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let vault = service
            .create_vault(
                CreateVaultRequest {
                    name: "TestVault".to_string(),
                    adapter: VaultAdapter::ObsidianVault,
                    client_vault_id: None,
                    device_id: "device-1".to_string(),
                },
                tenant_id,
                owner_id,
            )
            .await
            .unwrap();

        let device = test_device(owner_id, tenant_id);
        service
            .register_device(device.clone(), owner_id)
            .await
            .unwrap();

        let test_cases = vec![
            ("../escape.md", "parent directory reference"),
            ("/absolute.md", "leading slash"),
            ("", "path is empty"),
            ("null\0byte.md", "null byte"),
        ];

        for (path, expected_msg) in test_cases {
            let content = Bytes::from_static(b"test");
            let req = UploadVaultFileRequest {
                vault_id: vault.id,
                relative_path: path.to_string(),
                content_type: None,
                sha256: "0000000000000000000000000000000000000000000000000000000000000000"
                    .to_string(),
                size: 4,
                base_server_rev: 0,
                device_id: device.id.to_string(),
                content: content.clone(),
            };
            let err = service
                .upload_file(req, tenant_id, owner_id)
                .await
                .unwrap_err();
            match err {
                VaultSyncError::InvalidPath(msg) => {
                    assert!(
                        msg.contains(expected_msg),
                        "path '{}': expected message to contain '{}', got: {}",
                        path,
                        expected_msg,
                        msg
                    );
                }
                _ => panic!("path '{}': Expected InvalidPath, got {:?}", path, err),
            }
        }
    }

    #[tokio::test]
    async fn test_sha256_validation() {
        let (_, _, service) = setup();
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let vault = service
            .create_vault(
                CreateVaultRequest {
                    name: "TestVault".to_string(),
                    adapter: VaultAdapter::ObsidianVault,
                    client_vault_id: None,
                    device_id: "device-1".to_string(),
                },
                tenant_id,
                owner_id,
            )
            .await
            .unwrap();

        let device = test_device(owner_id, tenant_id);
        service
            .register_device(device.clone(), owner_id)
            .await
            .unwrap();

        // Too short
        let req = UploadVaultFileRequest {
            vault_id: vault.id,
            relative_path: "notes/hello.md".to_string(),
            content_type: None,
            sha256: "tooshort".to_string(),
            size: 4,
            base_server_rev: 0,
            device_id: device.id.to_string(),
            content: Bytes::from_static(b"test"),
        };
        let err = service
            .upload_file(req, tenant_id, owner_id)
            .await
            .unwrap_err();
        assert!(matches!(err, VaultSyncError::InvalidName(_)));

        // Non-hex characters
        let req = UploadVaultFileRequest {
            vault_id: vault.id,
            relative_path: "notes/hello.md".to_string(),
            content_type: None,
            sha256: "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz".to_string(),
            size: 4,
            base_server_rev: 0,
            device_id: device.id.to_string(),
            content: Bytes::from_static(b"test"),
        };
        let err = service
            .upload_file(req, tenant_id, owner_id)
            .await
            .unwrap_err();
        assert!(matches!(err, VaultSyncError::InvalidName(_)));
    }

    #[tokio::test]
    async fn test_max_path_length() {
        let (_, _, service) = setup();
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let vault = service
            .create_vault(
                CreateVaultRequest {
                    name: "TestVault".to_string(),
                    adapter: VaultAdapter::ObsidianVault,
                    client_vault_id: None,
                    device_id: "device-1".to_string(),
                },
                tenant_id,
                owner_id,
            )
            .await
            .unwrap();

        let device = test_device(owner_id, tenant_id);
        service
            .register_device(device.clone(), owner_id)
            .await
            .unwrap();

        let long_path = "a/".repeat(2049);
        let req = UploadVaultFileRequest {
            vault_id: vault.id,
            relative_path: long_path,
            content_type: None,
            sha256: "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
            size: 4,
            base_server_rev: 0,
            device_id: device.id.to_string(),
            content: Bytes::from_static(b"test"),
        };
        let err = service
            .upload_file(req, tenant_id, owner_id)
            .await
            .unwrap_err();
        assert!(matches!(err, VaultSyncError::InvalidPath(_)));
    }

    #[tokio::test]
    async fn test_vault_name_dot_rejection() {
        let (_, _, service) = setup();
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();

        let err = service
            .create_vault(
                CreateVaultRequest {
                    name: ".".to_string(),
                    adapter: VaultAdapter::ObsidianVault,
                    client_vault_id: None,
                    device_id: "device-1".to_string(),
                },
                tenant_id,
                owner_id,
            )
            .await
            .unwrap_err();
        assert!(matches!(err, VaultSyncError::InvalidName(_)));
    }

    #[tokio::test]
    async fn test_register_and_revoke_device() {
        let (_, _, service) = setup();
        let tenant_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();

        let device = VaultDevice {
            id: Uuid::new_v4(),
            tenant_id,
            user_id,
            vault_id: None,
            device_name: "My Device".to_string(),
            client_type: "desktop".to_string(),
            client_version: Some("1.0.0".to_string()),
            last_sync_rev: None,
            revoked_at: None,
            created_at: Utc::now(),
            last_seen_at: Utc::now(),
        };

        let registered = service
            .register_device(device.clone(), user_id)
            .await
            .unwrap();
        assert_eq!(registered.device_name, "My Device");

        // Revoke works
        service
            .revoke_device(device.id, tenant_id, user_id)
            .await
            .unwrap();

        // Getting/revoking revoked device returns error
        let err = service
            .revoke_device(device.id, tenant_id, user_id)
            .await
            .unwrap_err();
        assert!(matches!(err, VaultSyncError::DeviceRevoked));
    }

    #[tokio::test]
    async fn test_register_device_with_vault_unauthorized() {
        let (_, _, service) = setup();
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let other_user = Uuid::new_v4();
        let vault = service
            .create_vault(
                CreateVaultRequest {
                    name: "TestVault".to_string(),
                    adapter: VaultAdapter::ObsidianVault,
                    client_vault_id: None,
                    device_id: "device-1".to_string(),
                },
                tenant_id,
                owner_id,
            )
            .await
            .unwrap();

        let device = test_device(owner_id, tenant_id);
        service
            .register_device(device.clone(), owner_id)
            .await
            .unwrap();

        let device = VaultDevice {
            id: Uuid::new_v4(),
            tenant_id,
            user_id: other_user,
            vault_id: Some(vault.id),
            device_name: "Bad Actor".to_string(),
            client_type: "desktop".to_string(),
            client_version: None,
            last_sync_rev: None,
            revoked_at: None,
            created_at: Utc::now(),
            last_seen_at: Utc::now(),
        };

        let err = service.register_device(device, owner_id).await.unwrap_err();
        assert!(matches!(err, VaultSyncError::Unauthorized));
    }

    #[tokio::test]
    async fn test_revoke_device_unauthorized() {
        let (_, _, service) = setup();
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let other_user = Uuid::new_v4();
        let vault = service
            .create_vault(
                CreateVaultRequest {
                    name: "TestVault".to_string(),
                    adapter: VaultAdapter::ObsidianVault,
                    client_vault_id: None,
                    device_id: "device-1".to_string(),
                },
                tenant_id,
                owner_id,
            )
            .await
            .unwrap();

        let device = test_device(owner_id, tenant_id);
        service
            .register_device(device.clone(), owner_id)
            .await
            .unwrap();

        let device = VaultDevice {
            id: Uuid::new_v4(),
            tenant_id,
            user_id: owner_id,
            vault_id: Some(vault.id),
            device_name: "Owner Device".to_string(),
            client_type: "desktop".to_string(),
            client_version: None,
            last_sync_rev: None,
            revoked_at: None,
            created_at: Utc::now(),
            last_seen_at: Utc::now(),
        };
        service
            .register_device(device.clone(), owner_id)
            .await
            .unwrap();

        let err = service
            .revoke_device(device.id, tenant_id, other_user)
            .await
            .unwrap_err();
        assert!(matches!(err, VaultSyncError::Unauthorized));
    }

    #[tokio::test]
    async fn test_create_vault_name_validation() {
        let (_, _, service) = setup();
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();

        // Empty name
        let err = service
            .create_vault(
                CreateVaultRequest {
                    name: "   ".to_string(),
                    adapter: VaultAdapter::ObsidianVault,
                    client_vault_id: None,
                    device_id: "device-1".to_string(),
                },
                tenant_id,
                owner_id,
            )
            .await
            .unwrap_err();
        assert!(matches!(err, VaultSyncError::InvalidName(_)));

        // Too long
        let err = service
            .create_vault(
                CreateVaultRequest {
                    name: "a".repeat(129),
                    adapter: VaultAdapter::ObsidianVault,
                    client_vault_id: None,
                    device_id: "device-1".to_string(),
                },
                tenant_id,
                owner_id,
            )
            .await
            .unwrap_err();
        assert!(matches!(err, VaultSyncError::InvalidName(_)));

        // Path traversal ..
        let err = service
            .create_vault(
                CreateVaultRequest {
                    name: "../etc".to_string(),
                    adapter: VaultAdapter::ObsidianVault,
                    client_vault_id: None,
                    device_id: "device-1".to_string(),
                },
                tenant_id,
                owner_id,
            )
            .await
            .unwrap_err();
        assert!(matches!(err, VaultSyncError::InvalidName(_)));

        // Forward slash
        let err = service
            .create_vault(
                CreateVaultRequest {
                    name: "a/b".to_string(),
                    adapter: VaultAdapter::ObsidianVault,
                    client_vault_id: None,
                    device_id: "device-1".to_string(),
                },
                tenant_id,
                owner_id,
            )
            .await
            .unwrap_err();
        assert!(matches!(err, VaultSyncError::InvalidName(_)));

        // Backslash
        let err = service
            .create_vault(
                CreateVaultRequest {
                    name: "a\\b".to_string(),
                    adapter: VaultAdapter::ObsidianVault,
                    client_vault_id: None,
                    device_id: "device-1".to_string(),
                },
                tenant_id,
                owner_id,
            )
            .await
            .unwrap_err();
        assert!(matches!(err, VaultSyncError::InvalidName(_)));

        // Null byte
        let err = service
            .create_vault(
                CreateVaultRequest {
                    name: "vault\0name".to_string(),
                    adapter: VaultAdapter::ObsidianVault,
                    client_vault_id: None,
                    device_id: "device-1".to_string(),
                },
                tenant_id,
                owner_id,
            )
            .await
            .unwrap_err();
        assert!(matches!(err, VaultSyncError::InvalidName(_)));

        // Control character
        let err = service
            .create_vault(
                CreateVaultRequest {
                    name: "vault\nname".to_string(),
                    adapter: VaultAdapter::ObsidianVault,
                    client_vault_id: None,
                    device_id: "device-1".to_string(),
                },
                tenant_id,
                owner_id,
            )
            .await
            .unwrap_err();
        assert!(matches!(err, VaultSyncError::InvalidName(_)));

        // Leading/trailing whitespace is trimmed
        let vault = service
            .create_vault(
                CreateVaultRequest {
                    name: "  MyVault  ".to_string(),
                    adapter: VaultAdapter::ObsidianVault,
                    client_vault_id: None,
                    device_id: "device-1".to_string(),
                },
                tenant_id,
                owner_id,
            )
            .await
            .unwrap();
        assert_eq!(vault.name, "MyVault");
        assert_eq!(
            vault.root_path,
            Some("My Files/Vaults/Obsidian/MyVault".to_string())
        );
    }

    async fn create_web_editable_vault(
        service: &VaultSyncService<MockVaultStore, MockObjectStore>,
        tenant_id: Uuid,
        owner_id: Uuid,
    ) -> Vault {
        let vault = service
            .create_vault(
                CreateVaultRequest {
                    name: "WebVault".to_string(),
                    adapter: VaultAdapter::ObsidianVault,
                    client_vault_id: None,
                    device_id: "device-1".to_string(),
                },
                tenant_id,
                owner_id,
            )
            .await
            .unwrap();
        let mut updated = vault.clone();
        updated.write_policy = VaultWritePolicy::WebEditingEnabled;
        service.store.update_vault(&updated).await.unwrap();
        updated
    }

    fn upload_file_sha256(content: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        hex::encode(Sha256::digest(content))
    }

    async fn upload_test_file(
        service: &VaultSyncService<MockVaultStore, MockObjectStore>,
        vault_id: Uuid,
        relative_path: &str,
        content_type: Option<String>,
        content: Bytes,
        tenant_id: Uuid,
        owner_id: Uuid,
    ) -> VaultFile {
        let device = test_device(owner_id, tenant_id);
        service
            .register_device(device.clone(), owner_id)
            .await
            .unwrap();
        let sha256 = upload_file_sha256(&content);
        service
            .upload_file(
                UploadVaultFileRequest {
                    vault_id,
                    relative_path: relative_path.to_string(),
                    content_type,
                    sha256,
                    size: content.len() as i64,
                    base_server_rev: 0,
                    device_id: device.id.to_string(),
                    content,
                },
                tenant_id,
                owner_id,
            )
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn test_get_file_content_for_webui_success() {
        let (_, object_store, service) = setup();
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let vault = create_web_editable_vault(&service, tenant_id, owner_id).await;

        let content = Bytes::from_static(b"# Hello\nworld");
        upload_test_file(
            &service,
            vault.id,
            "notes/hello.md",
            Some("text/markdown".to_string()),
            content.clone(),
            tenant_id,
            owner_id,
        )
        .await;

        let resp = service
            .get_file_content_for_webui(vault.id, "notes/hello.md", tenant_id, owner_id)
            .await
            .unwrap();
        assert_eq!(resp.path, "notes/hello.md");
        assert_eq!(resp.content, "# Hello\nworld");
        assert_eq!(resp.server_rev, 1);
        assert_eq!(resp.content_type, Some("text/markdown".to_string()));
        assert_eq!(resp.size, 13);

        // Blob was stored by upload; ensure it is the same bytes.
        let sha256 = upload_file_sha256(&content);
        let stored = object_store
            .get(&format!("blobs/{}", sha256))
            .await
            .unwrap();
        assert_eq!(stored, content);
    }

    #[tokio::test]
    async fn test_get_file_content_for_webui_unauthorized() {
        let (_, _, service) = setup();
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let other_user = Uuid::new_v4();
        let vault = create_web_editable_vault(&service, tenant_id, owner_id).await;

        let content = Bytes::from_static(b"text");
        upload_test_file(
            &service,
            vault.id,
            "notes/hello.md",
            Some("text/markdown".to_string()),
            content,
            tenant_id,
            owner_id,
        )
        .await;

        let err = service
            .get_file_content_for_webui(vault.id, "notes/hello.md", tenant_id, other_user)
            .await
            .unwrap_err();
        assert!(matches!(err, VaultSyncError::Unauthorized));
    }

    #[tokio::test]
    async fn test_get_file_content_for_webui_not_editable() {
        let (_, _, service) = setup();
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let vault = create_web_editable_vault(&service, tenant_id, owner_id).await;

        let content = Bytes::from_static(b"binary");
        upload_test_file(
            &service,
            vault.id,
            "notes/image.png",
            Some("image/png".to_string()),
            content,
            tenant_id,
            owner_id,
        )
        .await;

        let err = service
            .get_file_content_for_webui(vault.id, "notes/image.png", tenant_id, owner_id)
            .await
            .unwrap_err();
        match err {
            VaultSyncError::NotEditable(msg) => assert!(msg.contains("notes/image.png")),
            _ => panic!("Expected NotEditable, got {:?}", err),
        }
    }

    #[tokio::test]
    async fn test_save_file_content_for_webui_success() {
        let (_, object_store, service) = setup();
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let vault = create_web_editable_vault(&service, tenant_id, owner_id).await;

        let original = Bytes::from_static(b"original");
        upload_test_file(
            &service,
            vault.id,
            "notes/hello.md",
            Some("text/markdown".to_string()),
            original,
            tenant_id,
            owner_id,
        )
        .await;

        let req = SaveVaultFileContentRequest {
            content: "updated content".to_string(),
            expected_revision: 1,
        };
        let resp = service
            .save_file_content_for_webui(vault.id, "notes/hello.md", req, tenant_id, owner_id)
            .await
            .unwrap();
        assert_eq!(resp.path, "notes/hello.md");
        assert_eq!(resp.server_rev, 2);

        // Reload and verify.
        let loaded = service
            .get_file_content_for_webui(vault.id, "notes/hello.md", tenant_id, owner_id)
            .await
            .unwrap();
        assert_eq!(loaded.content, "updated content");
        assert_eq!(loaded.server_rev, 2);

        // A web_ui device was created.
        let device = service
            .store
            .get_webui_device(tenant_id, owner_id, vault.id)
            .await
            .unwrap();
        assert!(device.is_some());
        let device = device.unwrap();
        assert_eq!(device.client_type, "web_ui");

        // New blob stored.
        let new_sha256 = upload_file_sha256(b"updated content");
        let stored = object_store
            .get(&format!("blobs/{}", new_sha256))
            .await
            .unwrap();
        assert_eq!(stored, Bytes::from_static(b"updated content"));
    }

    #[tokio::test]
    async fn test_save_file_content_for_webui_policy_denied() {
        let (_, _, service) = setup();
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let vault = service
            .create_vault(
                CreateVaultRequest {
                    name: "ReadOnlyVault".to_string(),
                    adapter: VaultAdapter::ObsidianVault,
                    client_vault_id: None,
                    device_id: "device-1".to_string(),
                },
                tenant_id,
                owner_id,
            )
            .await
            .unwrap();
        let mut updated = vault.clone();
        updated.write_policy = VaultWritePolicy::ReadOnly;
        service.store.update_vault(&updated).await.unwrap();

        let content = Bytes::from_static(b"text");
        upload_test_file(
            &service,
            vault.id,
            "notes/hello.md",
            Some("text/markdown".to_string()),
            content,
            tenant_id,
            owner_id,
        )
        .await;

        let req = SaveVaultFileContentRequest {
            content: "updated".to_string(),
            expected_revision: 1,
        };
        let err = service
            .save_file_content_for_webui(vault.id, "notes/hello.md", req, tenant_id, owner_id)
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            VaultSyncError::WritePolicyDenied { policy } if policy == "read_only"
        ));
    }

    #[tokio::test]
    async fn test_save_file_content_for_webui_conflict() {
        let (_, _, service) = setup();
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let vault = create_web_editable_vault(&service, tenant_id, owner_id).await;

        let content = Bytes::from_static(b"v1");
        upload_test_file(
            &service,
            vault.id,
            "notes/hello.md",
            Some("text/markdown".to_string()),
            content,
            tenant_id,
            owner_id,
        )
        .await;

        let req = SaveVaultFileContentRequest {
            content: "v2".to_string(),
            expected_revision: 0, // stale
        };
        let err = service
            .save_file_content_for_webui(vault.id, "notes/hello.md", req, tenant_id, owner_id)
            .await
            .unwrap_err();
        match err {
            VaultSyncError::Conflict {
                client_rev,
                current_rev,
                ..
            } => {
                assert_eq!(client_rev, 0);
                assert_eq!(current_rev, 1);
            }
            _ => panic!("Expected Conflict, got {:?}", err),
        }
    }

    #[tokio::test]
    async fn test_save_file_content_for_webui_tombstone_conflict() {
        let (_, _, service) = setup();
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let vault = create_web_editable_vault(&service, tenant_id, owner_id).await;

        let device = test_device(owner_id, tenant_id);
        service
            .register_device(device.clone(), owner_id)
            .await
            .unwrap();

        let content = Bytes::from_static(b"hello");
        upload_test_file(
            &service,
            vault.id,
            "notes/hello.md",
            Some("text/markdown".to_string()),
            content,
            tenant_id,
            owner_id,
        )
        .await;

        service
            .delete_file(
                DeleteVaultFileRequest {
                    vault_id: vault.id,
                    relative_path: "notes/hello.md".to_string(),
                    base_server_rev: 1,
                    device_id: device.id.to_string(),
                },
                tenant_id,
                owner_id,
            )
            .await
            .unwrap();

        let req = SaveVaultFileContentRequest {
            content: "updated".to_string(),
            expected_revision: 2,
        };
        let err = service
            .save_file_content_for_webui(vault.id, "notes/hello.md", req, tenant_id, owner_id)
            .await
            .unwrap_err();
        assert!(matches!(err, VaultSyncError::TombstoneConflict));
    }

    #[tokio::test]
    async fn test_save_file_content_for_webui_file_not_found() {
        let (_, _, service) = setup();
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let vault = create_web_editable_vault(&service, tenant_id, owner_id).await;

        let req = SaveVaultFileContentRequest {
            content: "updated".to_string(),
            expected_revision: 0,
        };
        let err = service
            .save_file_content_for_webui(vault.id, "notes/missing.md", req, tenant_id, owner_id)
            .await
            .unwrap_err();
        assert!(matches!(err, VaultSyncError::FileNotFound(_)));
    }

    #[tokio::test]
    async fn test_save_file_content_for_webui_sync_client_only_denied() {
        let (_, _, service) = setup();
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let vault = service
            .create_vault(
                CreateVaultRequest {
                    name: "SyncClientOnlyVault".to_string(),
                    adapter: VaultAdapter::ObsidianVault,
                    client_vault_id: None,
                    device_id: "device-1".to_string(),
                },
                tenant_id,
                owner_id,
            )
            .await
            .unwrap();
        let mut updated = vault.clone();
        updated.write_policy = VaultWritePolicy::SyncClientOnly;
        service.store.update_vault(&updated).await.unwrap();

        let content = Bytes::from_static(b"text");
        upload_test_file(
            &service,
            vault.id,
            "notes/hello.md",
            Some("text/markdown".to_string()),
            content,
            tenant_id,
            owner_id,
        )
        .await;

        let req = SaveVaultFileContentRequest {
            content: "updated".to_string(),
            expected_revision: 1,
        };
        let err = service
            .save_file_content_for_webui(vault.id, "notes/hello.md", req, tenant_id, owner_id)
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            VaultSyncError::WritePolicyDenied { policy } if policy == "sync_client_only"
        ));
    }

    #[tokio::test]
    async fn test_is_editable_file_txt_with_text_content_type() {
        let file = VaultFile {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            vault_id: Uuid::new_v4(),
            relative_path: "notes/hello.txt".to_string(),
            content_type: Some("text/plain".to_string()),
            sha256: None,
            size: Some(10),
            server_rev: 1,
            mtime_client: None,
            mtime_server: Utc::now(),
            deleted: false,
            deleted_at: None,
            last_writer_device_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        assert!(VaultSyncService::<MockVaultStore, MockObjectStore>::is_editable_file(&file));
    }

    #[tokio::test]
    async fn test_is_editable_file_txt_without_text_content_type() {
        let file = VaultFile {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            vault_id: Uuid::new_v4(),
            relative_path: "notes/data.txt".to_string(),
            content_type: Some("application/octet-stream".to_string()),
            sha256: None,
            size: Some(10),
            server_rev: 1,
            mtime_client: None,
            mtime_server: Utc::now(),
            deleted: false,
            deleted_at: None,
            last_writer_device_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        assert!(!VaultSyncService::<MockVaultStore, MockObjectStore>::is_editable_file(&file));
    }

    #[tokio::test]
    async fn test_save_file_content_for_webui_size_exceeded() {
        let (_, _, service) = setup();
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let vault = create_web_editable_vault(&service, tenant_id, owner_id).await;

        let original = Bytes::from_static(b"original");
        upload_test_file(
            &service,
            vault.id,
            "notes/hello.md",
            Some("text/markdown".to_string()),
            original,
            tenant_id,
            owner_id,
        )
        .await;

        let oversized = "x".repeat(
            (VaultSyncService::<MockVaultStore, MockObjectStore>::MAX_WEBUI_EDIT_SIZE + 1) as usize,
        );
        let req = SaveVaultFileContentRequest {
            content: oversized,
            expected_revision: 1,
        };
        let err = service
            .save_file_content_for_webui(vault.id, "notes/hello.md", req, tenant_id, owner_id)
            .await
            .unwrap_err();
        match err {
            VaultSyncError::NotEditable(msg) => {
                assert!(msg.contains("file exceeds"));
                assert!(msg.contains(
                    &VaultSyncService::<MockVaultStore, MockObjectStore>::MAX_WEBUI_EDIT_SIZE
                        .to_string()
                ));
            }
            _ => panic!("Expected NotEditable, got {:?}", err),
        }
    }

    #[tokio::test]
    async fn test_get_file_content_for_webui_deleted_file() {
        let (_, _, service) = setup();
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let vault = create_web_editable_vault(&service, tenant_id, owner_id).await;

        let device = test_device(owner_id, tenant_id);
        service
            .register_device(device.clone(), owner_id)
            .await
            .unwrap();

        let content = Bytes::from_static(b"hello");
        upload_test_file(
            &service,
            vault.id,
            "notes/hello.md",
            Some("text/markdown".to_string()),
            content,
            tenant_id,
            owner_id,
        )
        .await;

        service
            .delete_file(
                DeleteVaultFileRequest {
                    vault_id: vault.id,
                    relative_path: "notes/hello.md".to_string(),
                    base_server_rev: 1,
                    device_id: device.id.to_string(),
                },
                tenant_id,
                owner_id,
            )
            .await
            .unwrap();

        let err = service
            .get_file_content_for_webui(vault.id, "notes/hello.md", tenant_id, owner_id)
            .await
            .unwrap_err();
        assert!(matches!(err, VaultSyncError::FileNotFound(_)));
    }

    #[tokio::test]
    async fn test_save_file_content_for_webui_last_writer_device_id() {
        let (_, _, service) = setup();
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let vault = create_web_editable_vault(&service, tenant_id, owner_id).await;

        let original = Bytes::from_static(b"original");
        upload_test_file(
            &service,
            vault.id,
            "notes/hello.md",
            Some("text/markdown".to_string()),
            original,
            tenant_id,
            owner_id,
        )
        .await;

        let req = SaveVaultFileContentRequest {
            content: "updated content".to_string(),
            expected_revision: 1,
        };
        service
            .save_file_content_for_webui(vault.id, "notes/hello.md", req, tenant_id, owner_id)
            .await
            .unwrap();

        let device = service
            .store
            .get_webui_device(tenant_id, owner_id, vault.id)
            .await
            .unwrap()
            .expect("WebUI device should exist");

        let file = service
            .store
            .get_file_including_deleted(vault.id, "notes/hello.md", tenant_id)
            .await
            .unwrap();
        assert_eq!(file.last_writer_device_id, Some(device.id.to_string()));
    }
}
