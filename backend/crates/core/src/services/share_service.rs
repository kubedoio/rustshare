//! ShareService for share link management operations.
//!
//! This service handles share link creation and management, including:
//! - Secure token generation (32-char alphanumeric)
//! - File ownership verification
//! - Password hashing and protection
//! - Event sourcing via EventStore
//! - Metadata persistence via MetadataStore

use anyhow::Result;
use chrono::{DateTime, Utc};
use rand::Rng;
use std::sync::Arc;

use rustshare_crypto::PasswordHasher;

use crate::domain::{File, Share, SharePermissions, UserId};
use crate::events::{AggregateType, Event, EventBroadcaster, EventType, ShareCreatedPayload};
use crate::services::ShareError;

/// Trait for event store operations needed by ShareService.
///
/// This trait abstracts the event store to allow for testing without database dependencies.
#[allow(async_fn_in_trait)]
pub trait EventStoreOps: Send + Sync {
    /// Append an event to the event store.
    async fn append(&self, event: &Event, broadcaster: &EventBroadcaster) -> Result<()>;
}

/// Trait for metadata store operations needed by ShareService.
///
/// This trait abstracts the metadata store to allow for testing without database dependencies.
#[allow(async_fn_in_trait)]
pub trait MetadataStoreOps: Send + Sync {
    /// Find a file by ID.
    async fn find_file_by_id(&self, id: uuid::Uuid) -> Result<Option<File>>;

    /// Create a share in the metadata store.
    async fn create_share(&self, share: &Share) -> Result<()>;
}

/// ShareService handles share link creation and management.
///
/// Generic over EventStore and MetadataStore implementations to support
/// different backends and testing with mock implementations.
pub struct ShareService<E: EventStoreOps, M: MetadataStoreOps> {
    event_store: Arc<E>,
    metadata_store: Arc<M>,
    broadcaster: Arc<EventBroadcaster>,
}

impl<E: EventStoreOps, M: MetadataStoreOps> ShareService<E, M> {
    /// Create a new ShareService instance.
    pub fn new(
        event_store: Arc<E>,
        metadata_store: Arc<M>,
        broadcaster: Arc<EventBroadcaster>,
    ) -> Self {
        Self {
            event_store,
            metadata_store,
            broadcaster,
        }
    }

    /// Generate a cryptographically secure 32-character alphanumeric token.
    ///
    /// Returns a unique token suitable for use as a share link identifier.
    pub fn generate_token() -> String {
        const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
        const TOKEN_LENGTH: usize = 32;

        let mut rng = rand::thread_rng();
        (0..TOKEN_LENGTH)
            .map(|_| {
                let idx = rng.gen_range(0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect()
    }

    /// Create a new share link for a file.
    ///
    /// Verifies that:
    /// - The file exists
    /// - The user owns the file
    /// - Password is hashed if provided
    ///
    /// Returns the created Share or a ShareError.
    pub async fn create_share(
        &self,
        file_id: uuid::Uuid,
        user_id: UserId,
        permissions: SharePermissions,
        password: Option<String>,
        expires_at: Option<DateTime<Utc>>,
    ) -> Result<Share, ShareError> {
        // Verify file exists
        let file = self
            .metadata_store
            .find_file_by_id(file_id)
            .await
            .map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))?
            .ok_or(ShareError::FileNotFound(file_id))?;

        // Verify user owns the file
        if file.owner_id != user_id {
            return Err(ShareError::PermissionDenied {
                file_id,
                user_id,
            });
        }

        // Hash password if provided
        let password_hash = if let Some(pwd) = password {
            Some(PasswordHasher::hash(&pwd)
                .map_err(|e| ShareError::PasswordHash(e.to_string()))?)
        } else {
            None
        };

        // Generate unique token
        let token = Self::generate_token();

        // Create Share domain object
        let share = Share::new(
            file_id,
            token.clone(),
            user_id,
            permissions,
            password_hash,
            expires_at,
        );

        // Store in metadata store
        self.metadata_store
            .create_share(&share)
            .await
            .map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))?;

        // Emit ShareCreated event
        let payload = ShareCreatedPayload {
            share_id: share.id,
            file_id: share.file_id,
            share_token: share.share_token.clone(),
            permissions: share.permissions,
            password_protected: share.password_hash.is_some(),
            expires_at: share.expires_at,
            created_by: user_id,
        };

        let event = Event::new(
            EventType::ShareCreated,
            share.id,
            AggregateType::Share,
            serde_json::to_value(&payload).map_err(|_e| {
                ShareError::Database(sqlx::Error::PoolClosed)
            })?,
            user_id,
        );

        self.event_store
            .append(&event, &self.broadcaster)
            .await
            .map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))?;

        Ok(share)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use uuid::Uuid;

    struct MockEventStore {
        events: Mutex<Vec<Event>>,
    }

    impl MockEventStore {
        fn new() -> Self {
            Self {
                events: Mutex::new(Vec::new()),
            }
        }
    }

    impl EventStoreOps for MockEventStore {
        async fn append(&self, event: &Event, _broadcaster: &EventBroadcaster) -> Result<()> {
            self.events.lock().unwrap().push(event.clone());
            Ok(())
        }
    }

    struct MockMetadataStore {
        files: Mutex<Vec<File>>,
        shares: Mutex<Vec<Share>>,
    }

    impl MockMetadataStore {
        fn new() -> Self {
            Self {
                files: Mutex::new(Vec::new()),
                shares: Mutex::new(Vec::new()),
            }
        }

        fn add_file(&self, file: File) {
            self.files.lock().unwrap().push(file);
        }
    }

    impl MetadataStoreOps for MockMetadataStore {
        async fn find_file_by_id(&self, id: Uuid) -> Result<Option<File>> {
            Ok(self.files.lock().unwrap().iter().find(|f| f.id == id).cloned())
        }

        async fn create_share(&self, share: &Share) -> Result<()> {
            self.shares.lock().unwrap().push(share.clone());
            Ok(())
        }
    }

    fn setup_share_service() -> (
        ShareService<MockEventStore, MockMetadataStore>,
        Arc<MockEventStore>,
        Arc<MockMetadataStore>,
    ) {
        let event_store = Arc::new(MockEventStore::new());
        let metadata_store = Arc::new(MockMetadataStore::new());
        let broadcaster = Arc::new(EventBroadcaster::new(100));

        let service = ShareService::new(event_store.clone(), metadata_store.clone(), broadcaster);

        (service, event_store, metadata_store)
    }

    #[test]
    fn test_generate_token_is_unique() {
        let mut tokens = std::collections::HashSet::new();

        for _ in 0..1000 {
            let token = ShareService::<MockEventStore, MockMetadataStore>::generate_token();

            // Verify token length is 32
            assert_eq!(token.len(), 32, "Token length should be 32");

            // Verify token is alphanumeric
            assert!(
                token.chars().all(|c| c.is_alphanumeric()),
                "Token should be alphanumeric: {}",
                token
            );

            // Collect token
            tokens.insert(token);
        }

        // Verify all 1000 tokens are unique
        assert_eq!(
            tokens.len(),
            1000,
            "All 1000 tokens should be unique, but got {} unique",
            tokens.len()
        );
    }

    #[tokio::test]
    async fn test_create_share_success() {
        let (service, event_store, metadata_store) = setup_share_service();

        let owner_id = Uuid::new_v4();

        // Create and add a file
        let file = File::new(
            "document.pdf".to_string(),
            "/documents/document.pdf".to_string(),
            "abc123".to_string(),
            1024,
            "application/pdf".to_string(),
            None,
            owner_id,
        );
        let file_id = file.id;
        metadata_store.add_file(file.clone());

        // Create share
        let share = service
            .create_share(
                file_id,
                owner_id,
                SharePermissions::Read,
                None,
                None,
            )
            .await
            .unwrap();

        // Verify share properties
        assert_eq!(share.file_id, file_id);
        assert_eq!(share.created_by, owner_id);
        assert_eq!(share.permissions, SharePermissions::Read);
        assert_eq!(share.share_token.len(), 32);
        assert!(share.password_hash.is_none());
        assert!(share.expires_at.is_none());

        // Verify share was stored
        let shares = metadata_store.shares.lock().unwrap();
        assert_eq!(shares.len(), 1);
        assert_eq!(shares[0].id, share.id);

        // Verify event was emitted
        let events = event_store.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, EventType::ShareCreated);
        assert_eq!(events[0].aggregate_id, share.id);
        assert_eq!(events[0].aggregate_type, AggregateType::Share);
    }

    #[tokio::test]
    async fn test_create_share_permission_denied() {
        let (service, _, metadata_store) = setup_share_service();

        let owner_id = Uuid::new_v4();
        let other_user = Uuid::new_v4();

        // Create file owned by owner_id
        let file = File::new(
            "private.pdf".to_string(),
            "/private.pdf".to_string(),
            "hash".to_string(),
            512,
            "application/pdf".to_string(),
            None,
            owner_id,
        );
        let file_id = file.id;
        metadata_store.add_file(file);

        // Try to create share as different user
        let result = service
            .create_share(
                file_id,
                other_user,
                SharePermissions::Read,
                None,
                None,
            )
            .await;

        assert!(matches!(
            result,
            Err(ShareError::PermissionDenied { .. })
        ));
    }
}
