//! Integration tests: Admin webhook management SQL lifecycle + HMAC verification (Task 6).
//!
//! Tests:
//!   - Create webhook with encrypted secret
//!   - List webhooks, verify in list
//!   - Update webhook events
//!   - Verify HMAC-SHA256 computation for webhook signing
//!   - Delete webhook, verify gone from DB
//!
//! Note: The test-fire endpoint (POST /webhooks/:id/test) sends an HTTP request
//! to an external URL. That external HTTP call cannot be tested here without a
//! live server. This file tests all in-DB logic and the HMAC computation
//! that the handler performs before sending.
//!
//! Run with: cargo test --test admin_webhooks_test -- --ignored

use base64::{engine::general_purpose::STANDARD, Engine};
use hmac::{Hmac, KeyInit, Mac};
use rustshare_crypto::{decrypt_secret, encrypt_secret, SecretEncryptionKey};
use sha2::Sha256;
use sqlx::Row;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn test_pool() -> sqlx::PgPool {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://rustshare:changeme@localhost:5432/rustshare".to_string());
    sqlx::PgPool::connect(&url)
        .await
        .expect("DB connect failed")
}

fn test_encryption_key() -> SecretEncryptionKey {
    let key_bytes = [0x42u8; 32];
    let b64 = STANDARD.encode(key_bytes);
    std::env::set_var("RUSTSHARE_SECRET_ENCRYPTION_KEY", &b64);
    SecretEncryptionKey::from_env().expect("test key must load from env")
}

async fn create_test_admin(pool: &sqlx::PgPool, suffix: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, username, email, password_hash, display_name, is_admin, storage_quota)
         VALUES ($1, $2, $3, $4, $5, true, 10737418240)",
    )
    .bind(id)
    .bind(format!("wh_admin_{suffix}"))
    .bind(format!("whadmin_{suffix}@test.local"))
    .bind("$argon2id$v=19$m=4096,t=3,p=1$placeholder_hash")
    .bind(format!("Webhook Admin {suffix}"))
    .execute(pool)
    .await
    .expect("create test admin");
    id
}

async fn cleanup_users(pool: &sqlx::PgPool, user_ids: &[Uuid]) {
    for id in user_ids {
        sqlx::query("DELETE FROM admin_actions WHERE actor_id = $1 OR target_id = $1")
            .bind(id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(id)
            .execute(pool)
            .await
            .ok();
    }
}

const COLS: &str = "id, name, url, secret_enc, enabled, events, created_by, created_at, updated_at";

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Create a webhook with an encrypted secret and verify it appears in the list.
#[tokio::test]
async fn test_create_webhook_and_list() {
    let pool = test_pool().await;
    let suffix = &Uuid::new_v4().to_string()[..8];
    let actor_id = create_test_admin(&pool, suffix).await;
    let key = test_encryption_key();

    let plaintext_secret = "webhook-hmac-signing-secret";
    let secret_enc = encrypt_secret(plaintext_secret, &key).expect("encrypt webhook secret");

    // Insert webhook (same INSERT as the handler)
    let row = sqlx::query(&format!(
        "INSERT INTO webhook_configs (name, url, secret_enc, enabled, events, created_by)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING {COLS}"
    ))
    .bind(format!("Test Webhook {suffix}"))
    .bind("https://example.com/webhook")
    .bind(&secret_enc)
    .bind(true)
    .bind(vec!["file.uploaded", "file.deleted"])
    .bind(actor_id)
    .fetch_one(&pool)
    .await
    .expect("insert webhook");

    let webhook_id: Uuid = row.try_get("id").unwrap();
    let name: String = row.try_get("name").unwrap();
    let stored_enc: Option<String> = row.try_get("secret_enc").unwrap();
    let enabled: bool = row.try_get("enabled").unwrap();

    assert!(name.starts_with("Test Webhook"));
    assert!(enabled);
    let stored = stored_enc.as_deref().expect("secret_enc must be set");
    assert_ne!(stored, plaintext_secret, "Plaintext must not be stored");

    // Verify it appears in list query (same SELECT as list_webhooks handler)
    let list_rows = sqlx::query(&format!(
        "SELECT {COLS} FROM webhook_configs ORDER BY created_at DESC"
    ))
    .fetch_all(&pool)
    .await
    .expect("list webhooks");

    let found = list_rows
        .iter()
        .any(|r| r.try_get::<Uuid, _>("id").unwrap() == webhook_id);
    assert!(found, "Newly created webhook must appear in list");

    // Cleanup
    sqlx::query("DELETE FROM webhook_configs WHERE id = $1")
        .bind(webhook_id)
        .execute(&pool)
        .await
        .ok();
    cleanup_users(&pool, &[actor_id]).await;
}

/// Update webhook events and verify the new events are stored.
#[tokio::test]
async fn test_update_webhook_events() {
    let pool = test_pool().await;
    let suffix = &Uuid::new_v4().to_string()[..8];
    let actor_id = create_test_admin(&pool, suffix).await;

    let row = sqlx::query(&format!(
        "INSERT INTO webhook_configs (name, url, enabled, events, created_by)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING {COLS}"
    ))
    .bind(format!("Update Events Webhook {suffix}"))
    .bind("https://example.com/webhook2")
    .bind(true)
    .bind(vec!["file.uploaded"])
    .bind(actor_id)
    .fetch_one(&pool)
    .await
    .expect("insert webhook");

    let webhook_id: Uuid = row.try_get("id").unwrap();

    // Update events
    let new_events = vec!["user.created", "user.disabled", "user.deleted"];
    sqlx::query("UPDATE webhook_configs SET events = $2, updated_at = NOW() WHERE id = $1")
        .bind(webhook_id)
        .bind(&new_events)
        .execute(&pool)
        .await
        .expect("update webhook events");

    let updated_events: Vec<String> =
        sqlx::query_scalar("SELECT events FROM webhook_configs WHERE id = $1")
            .bind(webhook_id)
            .fetch_one(&pool)
            .await
            .expect("fetch updated events");

    assert_eq!(updated_events.len(), 3);
    assert!(updated_events.contains(&"user.created".to_string()));
    assert!(updated_events.contains(&"user.disabled".to_string()));
    assert!(updated_events.contains(&"user.deleted".to_string()));

    // Cleanup
    sqlx::query("DELETE FROM webhook_configs WHERE id = $1")
        .bind(webhook_id)
        .execute(&pool)
        .await
        .ok();
    cleanup_users(&pool, &[actor_id]).await;
}

/// Verify HMAC-SHA256 computation used for webhook signing.
/// This mirrors the logic in the test_webhook handler (webhooks.rs).
///
/// Note: Sending the actual HTTP request to an external URL is not tested here
/// — that requires a live server and is outside the scope of DB integration tests.
#[tokio::test]
async fn test_webhook_hmac_signature_computation() {
    // Known test vector
    let key = b"test-secret";
    let body = r#"{"event":"ping","timestamp":1234567890}"#;

    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC init");
    mac.update(body.as_bytes());
    let result = mac.finalize().into_bytes();
    let hex_sig = hex::encode(result);

    // Must produce a valid 64-character hex string (SHA-256 output = 32 bytes)
    assert_eq!(
        hex_sig.len(),
        64,
        "HMAC-SHA256 hex signature must be 64 characters"
    );
    assert!(
        hex_sig.chars().all(|c| c.is_ascii_hexdigit()),
        "Signature must be a valid hex string"
    );

    // Recomputing with the same key + body must produce the same signature (deterministic)
    let mut mac2 = Hmac::<Sha256>::new_from_slice(key).expect("HMAC init 2");
    mac2.update(body.as_bytes());
    let hex_sig2 = hex::encode(mac2.finalize().into_bytes());
    assert_eq!(
        hex_sig, hex_sig2,
        "HMAC must be deterministic for same key+body"
    );

    // A different body must produce a different signature
    let body2 = r#"{"event":"ping","timestamp":9999999999}"#;
    let mut mac3 = Hmac::<Sha256>::new_from_slice(key).expect("HMAC init 3");
    mac3.update(body2.as_bytes());
    let hex_sig3 = hex::encode(mac3.finalize().into_bytes());
    assert_ne!(
        hex_sig, hex_sig3,
        "Different body must produce different signature"
    );
}

/// Verify that the encrypted webhook secret round-trips through encrypt/decrypt.
#[tokio::test]
async fn test_webhook_secret_roundtrip() {
    let key = test_encryption_key();
    let plaintext = "my-webhook-signing-secret";

    let encrypted = encrypt_secret(plaintext, &key).expect("encrypt");
    let decrypted = decrypt_secret(&encrypted, &key).expect("decrypt");

    assert_eq!(
        decrypted, plaintext,
        "Webhook secret must round-trip correctly"
    );
}

/// Delete a webhook and verify it is gone from the DB.
#[tokio::test]
async fn test_delete_webhook() {
    let pool = test_pool().await;
    let suffix = &Uuid::new_v4().to_string()[..8];
    let actor_id = create_test_admin(&pool, suffix).await;

    let row = sqlx::query(&format!(
        "INSERT INTO webhook_configs (name, url, enabled, events, created_by)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING {COLS}"
    ))
    .bind(format!("Delete Test Webhook {suffix}"))
    .bind("https://example.com/webhook3")
    .bind(true)
    .bind(vec!["share.created"])
    .bind(actor_id)
    .fetch_one(&pool)
    .await
    .expect("insert webhook");

    let webhook_id: Uuid = row.try_get("id").unwrap();

    // Delete
    let result = sqlx::query("DELETE FROM webhook_configs WHERE id = $1")
        .bind(webhook_id)
        .execute(&pool)
        .await
        .expect("delete webhook");

    assert_eq!(result.rows_affected(), 1, "Delete must affect 1 row");

    // Verify gone
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM webhook_configs WHERE id = $1)")
            .bind(webhook_id)
            .fetch_one(&pool)
            .await
            .expect("check webhook existence");

    assert!(!exists, "Webhook must not exist after deletion");

    cleanup_users(&pool, &[actor_id]).await;
}
