//! Share Service V2 Implementation
//!
//! Implements dual-sided sharing:
//! - Owner writes outbound share to their bucket
//! - Recipient receives share document in their bucket
//! - Both sides store a PortableStorageLocator for cross-bucket access

use std::sync::Arc;

use anyhow::Result;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::{
    CrossBucketReader, PortableStorageLocator, UserBucketStore, UserId,
};

use rustshare_core::services::ShareError;

use super::indexes::UserBucketIndexes;
use super::models::*;
use super::paths::UserBucketPaths;

/// Share information returned to users
#[derive(Debug, Clone)]
pub struct ShareInfo {
    pub share_id: Uuid,
    pub resource_id: Uuid,
    pub resource_type: ShareResourceTypeV2,
    pub shared_by: UserId,
    pub shared_with: UserId,
    pub permissions: SharePermissionV2,
    pub created_at: DateTime<Utc>,
}

/// Share service using per-user bucket storage with dual-sided writes
pub struct ShareServiceV2 {
    user_buckets: Arc<dyn UserBucketStore>,
    cross_bucket_reader: Arc<dyn CrossBucketReader>,
    indexes: Arc<UserBucketIndexes>,
    storage_endpoint: String,
}

impl ShareServiceV2 {
    /// Create a new share service
    pub fn new(
        user_buckets: Arc<dyn UserBucketStore>,
        cross_bucket_reader: Arc<dyn CrossBucketReader>,
        indexes: Arc<UserBucketIndexes>,
        storage_endpoint: String,
    ) -> Self {
        Self {
            user_buckets,
            cross_bucket_reader,
            indexes,
            storage_endpoint,
        }
    }

    /// Share a resource with another user
    /// 
    /// This performs dual-sided writes:
    /// 1. Creates OutboundShareDocV2 in owner's bucket
    /// 2. Creates ReceivedShareDocV2 in recipient's bucket
    /// 3. Updates recipient's SharedWithMeIndex
    pub async fn create_share(
        &self,
        owner_id: UserId,
        recipient_id: UserId,
        resource_id: Uuid,
        resource_type: ShareResourceTypeV2,
        permissions: SharePermissionV2,
        expires_at: Option<DateTime<Utc>>,
    ) -> Result<ShareInfo, ShareError> {
        // Verify owner has access to the resource
        let (resource_exists, resource_locator) = self
            .verify_resource_access(owner_id, resource_id, resource_type)
            .await?;

        if !resource_exists {
            return Err(ShareError::ResourceNotFound {
                resource_id,
                resource_type: format!("{:?}", resource_type),
            });
        }

        // Prevent sharing with self
        if owner_id == recipient_id {
            return Err(ShareError::InvalidShare {
                reason: "Cannot share with yourself".to_string(),
            });
        }

        let share_id = Uuid::new_v4();
        let created_at = Utc::now();

        // Create outbound share (owner's perspective)
        let outbound_share = OutboundShareDocV2 {
            schema_version: SCHEMA_VERSION,
            id: share_id,
            resource_type,
            resource_id,
            resource_locator: resource_locator.clone(),
            shared_with_user_id: recipient_id,
            permissions,
            created_at,
            expires_at,
        };

        // Create received share (recipient's perspective)
        let received_share = ReceivedShareDocV2 {
            schema_version: SCHEMA_VERSION,
            share_id,
            resource_type,
            resource_locator,
            permissions,
            shared_by: owner_id,
            created_at,
            expires_at,
        };

        // Write outbound share to owner's bucket
        let owner_paths = UserBucketPaths::new(owner_id);
        self.user_buckets
            .put_object(
                owner_id,
                &owner_paths.outbound_share(share_id),
                Bytes::from(serde_json::to_vec(&outbound_share).unwrap()),
            )
            .await
            .map_err(|e| ShareError::Storage(e.to_string()))?;

        // Write received share to recipient's bucket
        let recipient_paths = UserBucketPaths::new(recipient_id);
        self.user_buckets
            .put_object(
                recipient_id,
                &recipient_paths.received_share(share_id),
                Bytes::from(serde_json::to_vec(&received_share).unwrap()),
            )
            .await
            .map_err(|e| ShareError::Storage(e.to_string()))?;

        // Update recipient's shared with me index
        let entry = SharedWithMeEntry {
            share_id,
            resource_id,
            resource_type,
            shared_by: owner_id,
            permissions,
            shared_at: created_at,
        };
        self.indexes
            .shared_with_me
            .upsert(recipient_id, entry)
            .await
            .map_err(|e| ShareError::Storage(e.to_string()))?;

        Ok(ShareInfo {
            share_id,
            resource_id,
            resource_type,
            shared_by: owner_id,
            shared_with: recipient_id,
            permissions,
            created_at,
        })
    }

    /// Get a share by ID (from owner's perspective)
    pub async fn get_outbound_share(
        &self,
        owner_id: UserId,
        share_id: Uuid,
    ) -> Result<OutboundShareDocV2, ShareError> {
        let paths = UserBucketPaths::new(owner_id);

        let data = self
            .user_buckets
            .get_object(owner_id, &paths.outbound_share(share_id))
            .await
            .map_err(|e| ShareError::Storage(e.to_string()))?
            .ok_or(ShareError::ShareNotFound(share_id))?;

        let share: OutboundShareDocV2 = serde_json::from_slice(&data)
            .map_err(|e| ShareError::Storage(format!("Invalid share document: {}", e)))?;

        Ok(share)
    }

    /// Get a received share by ID (from recipient's perspective)
    pub async fn get_received_share(
        &self,
        recipient_id: UserId,
        share_id: Uuid,
    ) -> Result<ReceivedShareDocV2, ShareError> {
        let paths = UserBucketPaths::new(recipient_id);

        let data = self
            .user_buckets
            .get_object(recipient_id, &paths.received_share(share_id))
            .await
            .map_err(|e| ShareError::Storage(e.to_string()))?
            .ok_or(ShareError::ShareNotFound(share_id))?;

        let share: ReceivedShareDocV2 = serde_json::from_slice(&data)
            .map_err(|e| ShareError::Storage(format!("Invalid share document: {}", e)))?;

        // Check if expired
        if let Some(expires_at) = share.expires_at {
            if Utc::now() > expires_at {
                return Err(ShareError::ShareExpired(share_id));
            }
        }

        Ok(share)
    }

    /// List outbound shares (shares created by the user)
    pub async fn list_outbound_shares(
        &self,
        owner_id: UserId,
    ) -> Result<Vec<OutboundShareDocV2>, ShareError> {
        let paths = UserBucketPaths::new(owner_id);
        let prefix = paths.outbound_shares_prefix();

        let share_keys = self
            .user_buckets
            .list_objects(owner_id, &prefix)
            .await
            .map_err(|e| ShareError::Storage(e.to_string()))?;

        let mut shares = Vec::new();
        for key in share_keys {
            if let Some(data) = self
                .user_buckets
                .get_object(owner_id, &key)
                .await
                .map_err(|e| ShareError::Storage(e.to_string()))?
            {
                if let Ok(share) = serde_json::from_slice::<OutboundShareDocV2>(&data) {
                    shares.push(share);
                }
            }
        }

        Ok(shares)
    }

    /// List inbound shares (shares received by the user)
    pub async fn list_inbound_shares(
        &self,
        recipient_id: UserId,
    ) -> Result<Vec<ReceivedShareDocV2>, ShareError> {
        let paths = UserBucketPaths::new(recipient_id);
        let prefix = paths.received_shares_prefix();

        let share_keys = self
            .user_buckets
            .list_objects(recipient_id, &prefix)
            .await
            .map_err(|e| ShareError::Storage(e.to_string()))?;

        let mut shares = Vec::new();
        let now = Utc::now();

        for key in share_keys {
            if let Some(data) = self
                .user_buckets
                .get_object(recipient_id, &key)
                .await
                .map_err(|e| ShareError::Storage(e.to_string()))?
            {
                if let Ok(share) = serde_json::from_slice::<ReceivedShareDocV2>(&data) {
                    // Filter out expired shares
                    if let Some(expires_at) = share.expires_at {
                        if now > expires_at {
                            continue;
                        }
                    }
                    shares.push(share);
                }
            }
        }

        Ok(shares)
    }

    /// Update share permissions
    pub async fn update_share_permissions(
        &self,
        owner_id: UserId,
        share_id: Uuid,
        new_permissions: SharePermissionV2,
    ) -> Result<ShareInfo, ShareError> {
        // Load outbound share
        let paths = UserBucketPaths::new(owner_id);
        let data = self
            .user_buckets
            .get_object(owner_id, &paths.outbound_share(share_id))
            .await
            .map_err(|e| ShareError::Storage(e.to_string()))?
            .ok_or(ShareError::ShareNotFound(share_id))?;

        let mut outbound_share: OutboundShareDocV2 = serde_json::from_slice(&data)
            .map_err(|e| ShareError::Storage(format!("Invalid share document: {}", e)))?;

        // Update permissions
        outbound_share.permissions = new_permissions;

        // Save outbound share
        self.user_buckets
            .put_object(
                owner_id,
                &paths.outbound_share(share_id),
                Bytes::from(serde_json::to_vec(&outbound_share).unwrap()),
            )
            .await
            .map_err(|e| ShareError::Storage(e.to_string()))?;

        // Update received share
        let recipient_paths = UserBucketPaths::new(outbound_share.shared_with_user_id);
        let recv_data = self
            .user_buckets
            .get_object(outbound_share.shared_with_user_id, &recipient_paths.received_share(share_id))
            .await
            .map_err(|e| ShareError::Storage(e.to_string()))?;

        if let Some(data) = recv_data {
            let mut received_share: ReceivedShareDocV2 = serde_json::from_slice(&data)
                .map_err(|e| ShareError::Storage(format!("Invalid share document: {}", e)))?;

            received_share.permissions = new_permissions;

            self.user_buckets
                .put_object(
                    outbound_share.shared_with_user_id,
                    &recipient_paths.received_share(share_id),
                    Bytes::from(serde_json::to_vec(&received_share).unwrap()),
                )
                .await
                .map_err(|e| ShareError::Storage(e.to_string()))?;

            // Update recipient's index
            let entry = SharedWithMeEntry {
                share_id,
                resource_id: outbound_share.resource_id,
                resource_type: outbound_share.resource_type,
                shared_by: owner_id,
                permissions: new_permissions,
                shared_at: outbound_share.created_at,
            };
            self.indexes
                .shared_with_me
                .upsert(outbound_share.shared_with_user_id, entry)
                .await
                .map_err(|e| ShareError::Storage(e.to_string()))?;
        }

        Ok(ShareInfo {
            share_id,
            resource_id: outbound_share.resource_id,
            resource_type: outbound_share.resource_type,
            shared_by: owner_id,
            shared_with: outbound_share.shared_with_user_id,
            permissions: new_permissions,
            created_at: outbound_share.created_at,
        })
    }

    /// Revoke a share (delete from both sides)
    pub async fn revoke_share(
        &self,
        owner_id: UserId,
        share_id: Uuid,
    ) -> Result<(), ShareError> {
        // Load outbound share to get recipient ID
        let paths = UserBucketPaths::new(owner_id);
        let data = self
            .user_buckets
            .get_object(owner_id, &paths.outbound_share(share_id))
            .await
            .map_err(|e| ShareError::Storage(e.to_string()))?
            .ok_or(ShareError::ShareNotFound(share_id))?;

        let outbound_share: OutboundShareDocV2 = serde_json::from_slice(&data)
            .map_err(|e| ShareError::Storage(format!("Invalid share document: {}", e)))?;

        // Delete outbound share from owner's bucket
        self.user_buckets
            .delete_object(owner_id, &paths.outbound_share(share_id))
            .await
            .map_err(|e| ShareError::Storage(e.to_string()))?;

        // Delete received share from recipient's bucket
        let recipient_paths = UserBucketPaths::new(outbound_share.shared_with_user_id);
        self.user_buckets
            .delete_object(outbound_share.shared_with_user_id, &recipient_paths.received_share(share_id))
            .await
            .map_err(|e| ShareError::Storage(e.to_string()))?;

        // Update recipient's index
        self.indexes
            .shared_with_me
            .remove(outbound_share.shared_with_user_id, share_id)
            .await
            .map_err(|e| ShareError::Storage(e.to_string()))?;

        Ok(())
    }

    /// Access a shared resource using the portable storage locator
    /// 
    /// This demonstrates cross-bucket reading capability
    pub async fn access_shared_resource<T: serde::de::DeserializeOwned>(
        &self,
        _accessor_id: UserId,  // Used for permission verification
        share_id: Uuid,
    ) -> Result<Option<T>, ShareError> {
        // Get the received share
        let share = self.get_received_share(_accessor_id, share_id).await?;

        // Verify permissions
        if !matches!(share.permissions, SharePermissionV2::Read | SharePermissionV2::Write | SharePermissionV2::Admin) {
            return Err(ShareError::PermissionDenied {
                file_id: share.resource_locator.resource_id,
                user_id: _accessor_id,
            });
        }

        // Read from owner's bucket using the cross-bucket reader
        let data = self
            .cross_bucket_reader
            .read_with_locator(&share.resource_locator)
            .await
            .map_err(|e| ShareError::Storage(e.to_string()))?;

        // Deserialize the data
        match data {
            Some(bytes) => {
                let resource = serde_json::from_slice(&bytes)
                    .map_err(|e| ShareError::Storage(format!("Deserialization failed: {}", e)))?;
                Ok(Some(resource))
            }
            None => Ok(None),
        }
    }

    // Helper methods

    /// Verify that a user has access to a resource and return its locator
    async fn verify_resource_access(
        &self,
        owner_id: UserId,
        resource_id: Uuid,
        resource_type: ShareResourceTypeV2,
    ) -> Result<(bool, PortableStorageLocator), ShareError> {
        let paths = UserBucketPaths::new(owner_id);
        let bucket_name = format!("rustshare-user-{}", owner_id);

        match resource_type {
            ShareResourceTypeV2::File => {
                let file_path = paths.file(resource_id);
                let exists = self
                    .user_buckets
                    .get_object(owner_id, &file_path)
                    .await
                    .map_err(|e| ShareError::Storage(e.to_string()))?
                    .is_some();

                let locator = PortableStorageLocator {
                    locator_version: 1,
                    storage_provider_kind: "s3".to_string(),
                    endpoint_ref: self.storage_endpoint.clone(),
                    bucket: bucket_name,
                    key: file_path,
                    resource_type: "file".to_string(),
                    resource_id,
                    version_id: None,
                    content_hash: None,
                };

                Ok((exists, locator))
            }
            ShareResourceTypeV2::Folder => {
                let folder_path = paths.folder(resource_id);
                let exists = self
                    .user_buckets
                    .get_object(owner_id, &folder_path)
                    .await
                    .map_err(|e| ShareError::Storage(e.to_string()))?
                    .is_some();

                let locator = PortableStorageLocator {
                    locator_version: 1,
                    storage_provider_kind: "s3".to_string(),
                    endpoint_ref: self.storage_endpoint.clone(),
                    bucket: bucket_name,
                    key: folder_path,
                    resource_type: "folder".to_string(),
                    resource_id,
                    version_id: None,
                    content_hash: None,
                };

                Ok((exists, locator))
            }
        }
    }
}
