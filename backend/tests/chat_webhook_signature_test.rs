//! Integration tests: chat webhook signature verification (Task A1).
//!
//! These tests exercise `ChatIntegrationService::process_incoming_event` and
//! assert that missing, invalid, and valid signatures produce the expected
//! outcomes. They run without external infrastructure by using in-memory mock
//! stores.

use std::sync::Mutex;

use chrono::Utc;
use rustshare_core::domain::{File, Folder, Share, UserId};
use rustshare_core::events::{Event, EventBroadcaster};
use rustshare_core::services::{
    ChatEventStoreOps, ChatIntegrationError, ChatIntegrationService, ChatMetadataStoreOps,
    HttpWebhookDispatcher, IncomingChatEvent,
};
use rustshare_crypto::WebhookSigner;
use std::sync::Arc;
use uuid::Uuid;

struct MockMetadataStore {
    shares: Mutex<Vec<Share>>,
    files: Mutex<Vec<File>>,
    folders: Mutex<Vec<Folder>>,
}

impl MockMetadataStore {
    fn new() -> Self {
        Self {
            shares: Mutex::new(Vec::new()),
            files: Mutex::new(Vec::new()),
            folders: Mutex::new(Vec::new()),
        }
    }
}

impl ChatMetadataStoreOps for MockMetadataStore {
    async fn get_share_by_token(&self, token: &str) -> anyhow::Result<Option<Share>> {
        Ok(self
            .shares
            .lock()
            .unwrap()
            .iter()
            .find(|s| s.share_token.as_deref() == Some(token))
            .cloned())
    }

    async fn find_file_by_id(&self, id: Uuid, _owner_id: Uuid) -> anyhow::Result<Option<File>> {
        Ok(self
            .files
            .lock()
            .unwrap()
            .iter()
            .find(|f| f.id == id)
            .cloned())
    }

    async fn find_folder_by_id(&self, id: Uuid, _owner_id: Uuid) -> anyhow::Result<Option<Folder>> {
        Ok(self
            .folders
            .lock()
            .unwrap()
            .iter()
            .find(|f| f.id == id)
            .cloned())
    }

    async fn get_user_shares(&self, _user_id: UserId) -> anyhow::Result<Vec<Share>> {
        Ok(self.shares.lock().unwrap().clone())
    }
}

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

impl ChatEventStoreOps for MockEventStore {
    async fn append(&self, event: &Event, _broadcaster: &EventBroadcaster) -> anyhow::Result<()> {
        self.events.lock().unwrap().push(event.clone());
        Ok(())
    }
}

fn sample_event() -> IncomingChatEvent {
    IncomingChatEvent {
        event_type: "share.revoked".to_string(),
        timestamp: Utc::now(),
        user_id: Some(Uuid::new_v4().to_string()),
        data: serde_json::json!({"share_token": "abc123"}),
    }
}

fn build_service(
    secret: &str,
) -> ChatIntegrationService<MockMetadataStore, MockEventStore, HttpWebhookDispatcher> {
    let metadata = Arc::new(MockMetadataStore::new());
    let events = Arc::new(MockEventStore::new());
    let broadcaster = Arc::new(EventBroadcaster::new(100));
    let dispatcher = Arc::new(HttpWebhookDispatcher::new());
    ChatIntegrationService::new(metadata, events, broadcaster, secret, dispatcher)
}

#[tokio::test]
async fn chat_webhook_signature_missing() {
    let service = build_service("test_secret");
    let event = sample_event();
    let body = serde_json::to_vec(&event).unwrap();

    let result = service.process_incoming_event(&body, "").await;
    assert!(
        matches!(
            result,
            Err(ChatIntegrationError::SignatureVerificationFailed)
        ),
        "Missing signature must be rejected"
    );
}

#[tokio::test]
async fn chat_webhook_signature_invalid() {
    let service = build_service("test_secret");
    let event = sample_event();
    let body = serde_json::to_vec(&event).unwrap();

    let signer = WebhookSigner::new("wrong-secret");
    let signature = signer.sign(&body).unwrap();

    let result = service.process_incoming_event(&body, &signature).await;
    assert!(
        matches!(
            result,
            Err(ChatIntegrationError::SignatureVerificationFailed)
        ),
        "Invalid signature must be rejected"
    );
}

#[tokio::test]
async fn chat_webhook_signature_valid() {
    let service = build_service("test_secret");
    let event = sample_event();
    let body = serde_json::to_vec(&event).unwrap();

    let signer = WebhookSigner::new("test_secret");
    let signature = signer.sign(&body).unwrap();

    let result = service.process_incoming_event(&body, &signature).await;
    assert!(
        result.is_ok(),
        "Valid signature must be accepted: {:?}",
        result
    );
}

#[tokio::test]
async fn chat_webhook_signature_tampered_body() {
    let service = build_service("test_secret");
    let event = sample_event();
    let original_body = serde_json::to_vec(&event).unwrap();

    // Compute a valid signature over the original body.
    let signer = WebhookSigner::new("test_secret");
    let signature = signer.sign(&original_body).unwrap();

    // Modify the body after signing.
    let mut tampered_body = original_body;
    tampered_body.push(b'\n');

    let result = service
        .process_incoming_event(&tampered_body, &signature)
        .await;
    assert!(
        matches!(
            result,
            Err(ChatIntegrationError::SignatureVerificationFailed)
        ),
        "Tampered body must fail signature verification: {:?}",
        result
    );
}
