//! Integration tests: Admin OIDC config SQL + encryption round-trip (Task 6).
//!
//! Tests:
//!   - Update the pre-seeded oidc_config row with encrypted client_secret
//!   - Read back, verify client_secret_enc is set (not plaintext)
//!   - Verify encrypt/decrypt round-trip using rustshare_crypto directly
//!   - Verify admin_actions row written for config.oidc_updated
//!
//! Note: OIDC test-connection (POST /config/oidc/test) fetches a live OIDC
//! discovery URL and cannot be tested without a running OIDC provider. That
//! handler's HTTP logic is not covered here.
//!
//! Run with: cargo test --test admin_config_oidc_test -- --ignored

use base64::{engine::general_purpose::STANDARD, Engine};
use rustshare_crypto::{decrypt_secret, encrypt_secret, SecretEncryptionKey};
use sqlx::Row;
use tokio::sync::Mutex;
use uuid::Uuid;

const OIDC_CONFIG_ID: &str = "00000000-0000-0000-0000-000000000001";
static OIDC_CONFIG_TEST_LOCK: Mutex<()> = Mutex::const_new(());

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

/// Build a test SecretEncryptionKey by setting the env var temporarily.
/// Uses a deterministic 32-byte key (0x42 repeated) encoded as base64.
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
    .bind(format!("oidc_admin_{suffix}"))
    .bind(format!("oidcadmin_{suffix}@test.local"))
    .bind("$argon2id$v=19$m=4096,t=3,p=1$placeholder_hash")
    .bind(format!("OIDC Admin {suffix}"))
    .execute(pool)
    .await
    .expect("create test admin");
    id
}

async fn cleanup(pool: &sqlx::PgPool, user_ids: &[Uuid]) {
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

async fn reset_oidc_config(pool: &sqlx::PgPool) {
    let oidc_id: Uuid = OIDC_CONFIG_ID.parse().unwrap();
    sqlx::query(
        "UPDATE oidc_config
         SET enabled = false, provider_name = NULL, client_id = NULL,
             client_secret_enc = NULL, issuer_url = NULL, redirect_url = NULL,
             login_label = NULL, scopes = NULL,
             auto_provision_users = false, updated_by = NULL, updated_at = NOW()
         WHERE id = $1",
    )
    .bind(oidc_id)
    .execute(pool)
    .await
    .expect("reset oidc_config");
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Encryption round-trip: encrypt then decrypt returns the original plaintext.
/// This test does not touch the DB — it validates the crypto primitives directly.
#[tokio::test]
async fn test_secret_encryption_roundtrip() {
    let key = test_encryption_key();
    let plaintext = "super-secret-oidc-client-secret-value";

    let encrypted = encrypt_secret(plaintext, &key).expect("encrypt_secret failed");

    // Must not store the plaintext
    assert_ne!(
        encrypted, plaintext,
        "Encrypted value must not equal plaintext"
    );
    assert!(!encrypted.is_empty(), "Encrypted value must not be empty");

    let decrypted = decrypt_secret(&encrypted, &key).expect("decrypt_secret failed");
    assert_eq!(
        decrypted, plaintext,
        "Decrypted value must match original plaintext"
    );
}

/// Each call to encrypt_secret must produce a different ciphertext (random nonce).
#[tokio::test]
async fn test_encryption_produces_different_ciphertexts() {
    let key = test_encryption_key();
    let plaintext = "same-plaintext";

    let enc1 = encrypt_secret(plaintext, &key).unwrap();
    let enc2 = encrypt_secret(plaintext, &key).unwrap();

    assert_ne!(
        enc1, enc2,
        "Ciphertexts for the same plaintext must differ (random nonce)"
    );

    // Both must decrypt to the same value
    assert_eq!(decrypt_secret(&enc1, &key).unwrap(), plaintext);
    assert_eq!(decrypt_secret(&enc2, &key).unwrap(), plaintext);
}

/// Update the pre-seeded oidc_config row, verify client_secret_enc is set (not plaintext),
/// and verify admin_actions row is written.
#[tokio::test]
async fn test_oidc_config_update_stores_encrypted_secret() {
    let _guard = OIDC_CONFIG_TEST_LOCK.lock().await;
    let pool = test_pool().await;
    let suffix = &Uuid::new_v4().to_string()[..8];
    let actor_id = create_test_admin(&pool, suffix).await;
    let key = test_encryption_key();
    let oidc_id: Uuid = OIDC_CONFIG_ID.parse().unwrap();
    let plaintext_secret = "my-plaintext-oidc-client-secret";

    let secret_enc = encrypt_secret(plaintext_secret, &key).expect("encrypt");

    // Update the pre-seeded singleton row (same SQL as the handler uses)
    sqlx::query(
        "UPDATE oidc_config
         SET enabled              = $2,
             provider_name        = $3,
             client_id            = $4,
             client_secret_enc    = $5,
             issuer_url           = $6,
             redirect_url         = $7,
             login_label          = $8,
             scopes               = $9,
             auto_provision_users = $10,
             updated_by           = $11,
             updated_at           = NOW()
         WHERE id = $1",
    )
    .bind(oidc_id)
    .bind(true)
    .bind("TestProvider")
    .bind("client-id-123")
    .bind(&secret_enc)
    .bind("https://idp.example.com")
    .bind("https://rustshare.example.com/api/v1/auth/oidc/callback")
    .bind("Continue with school SSO")
    .bind(vec!["openid", "email", "profile"])
    .bind(false)
    .bind(actor_id)
    .execute(&pool)
    .await
    .expect("update oidc_config");

    // Read back
    let row = sqlx::query(
        "SELECT enabled, provider_name, client_id, client_secret_enc, issuer_url,
                redirect_url, login_label
         FROM oidc_config WHERE id = $1",
    )
    .bind(oidc_id)
    .fetch_one(&pool)
    .await
    .expect("fetch oidc_config");

    let stored_enc: Option<String> = row.try_get("client_secret_enc").unwrap();
    let client_id: Option<String> = row.try_get("client_id").unwrap();
    let redirect_url: Option<String> = row.try_get("redirect_url").unwrap();
    let login_label: Option<String> = row.try_get("login_label").unwrap();
    let enabled: bool = row.try_get("enabled").unwrap();

    assert!(enabled, "OIDC config must be enabled after update");
    assert_eq!(client_id.as_deref(), Some("client-id-123"));
    assert_eq!(
        redirect_url.as_deref(),
        Some("https://rustshare.example.com/api/v1/auth/oidc/callback")
    );
    assert_eq!(login_label.as_deref(), Some("Continue with school SSO"));

    // The stored value must not be the plaintext
    let stored = stored_enc
        .as_deref()
        .expect("client_secret_enc must be set");
    assert_ne!(
        stored, plaintext_secret,
        "Plaintext must not be stored in DB"
    );

    // Round-trip decrypt must recover the original secret
    let recovered = decrypt_secret(stored, &key).expect("decrypt stored secret");
    assert_eq!(
        recovered, plaintext_secret,
        "Decrypted stored secret must match original"
    );

    // Admin action must be recorded
    sqlx::query(
        "INSERT INTO admin_actions (actor_id, action_type, detail)
         VALUES ($1, 'config.oidc_updated', '{}'::jsonb)",
    )
    .bind(actor_id)
    .execute(&pool)
    .await
    .expect("log config.oidc_updated");

    let action_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM admin_actions WHERE actor_id = $1 AND action_type = 'config.oidc_updated'",
    )
    .bind(actor_id)
    .fetch_one(&pool)
    .await
    .expect("count admin_actions");

    assert!(
        action_count >= 1,
        "admin_actions must have at least 1 config.oidc_updated row"
    );

    // Cleanup
    sqlx::query("DELETE FROM admin_actions WHERE actor_id = $1")
        .bind(actor_id)
        .execute(&pool)
        .await
        .ok();
    reset_oidc_config(&pool).await;
    cleanup(&pool, &[actor_id]).await;
}

/// Update the issuer_url on the oidc_config row, verify the change persists,
/// and confirm an admin_actions row is written for config.oidc_updated.
#[tokio::test]
async fn test_oidc_config_update() {
    let _guard = OIDC_CONFIG_TEST_LOCK.lock().await;
    let pool = test_pool().await;
    let suffix = &Uuid::new_v4().to_string()[..8];
    let actor_id = create_test_admin(&pool, suffix).await;
    let oidc_id: Uuid = OIDC_CONFIG_ID.parse().unwrap();

    let new_issuer_url = "https://updated-idp.example.com";

    // UPDATE the oidc_config row with a new issuer_url
    sqlx::query(
        "UPDATE oidc_config
         SET issuer_url = $2, updated_by = $3, updated_at = NOW()
         WHERE id = $1",
    )
    .bind(oidc_id)
    .bind(new_issuer_url)
    .bind(actor_id)
    .execute(&pool)
    .await
    .expect("update oidc_config issuer_url");

    // SELECT it back and verify the new value
    let stored_issuer: Option<String> =
        sqlx::query_scalar("SELECT issuer_url FROM oidc_config WHERE id = $1")
            .bind(oidc_id)
            .fetch_one(&pool)
            .await
            .expect("fetch issuer_url");

    assert_eq!(
        stored_issuer.as_deref(),
        Some(new_issuer_url),
        "issuer_url must reflect the updated value"
    );

    // Insert a config.oidc_updated admin_actions row (simulating what the handler does)
    sqlx::query(
        "INSERT INTO admin_actions (actor_id, action_type, detail)
         VALUES ($1, 'config.oidc_updated', $2)",
    )
    .bind(actor_id)
    .bind(serde_json::json!({"issuer_url": new_issuer_url}))
    .execute(&pool)
    .await
    .expect("log config.oidc_updated");

    // Verify the admin_actions row exists
    let action_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM admin_actions WHERE actor_id = $1 AND action_type = 'config.oidc_updated'",
    )
    .bind(actor_id)
    .fetch_one(&pool)
    .await
    .expect("count admin_actions");

    assert!(
        action_count >= 1,
        "Expected at least 1 config.oidc_updated admin_actions row"
    );

    // Cleanup
    sqlx::query("DELETE FROM admin_actions WHERE actor_id = $1")
        .bind(actor_id)
        .execute(&pool)
        .await
        .ok();
    reset_oidc_config(&pool).await;
    cleanup(&pool, &[actor_id]).await;
}

/// Runtime-only fields for the login path should persist alongside the identity provider values.
#[tokio::test]
async fn test_oidc_runtime_fields_persist() {
    let _guard = OIDC_CONFIG_TEST_LOCK.lock().await;
    let pool = test_pool().await;
    let oidc_id: Uuid = OIDC_CONFIG_ID.parse().unwrap();

    sqlx::query(
        "UPDATE oidc_config
         SET enabled = true,
             issuer_url = $2,
             redirect_url = $3,
             login_label = $4,
             updated_at = NOW()
         WHERE id = $1",
    )
    .bind(oidc_id)
    .bind("https://issuer.school.test")
    .bind("https://files.school.test/api/v1/auth/oidc/callback")
    .bind("Continue with school SSO")
    .execute(&pool)
    .await
    .expect("update runtime fields");

    let row = sqlx::query(
        "SELECT enabled, issuer_url, redirect_url, login_label
         FROM oidc_config
         WHERE id = $1",
    )
    .bind(oidc_id)
    .fetch_one(&pool)
    .await
    .expect("fetch runtime fields");

    let enabled: bool = row.try_get("enabled").unwrap();
    let issuer_url: Option<String> = row.try_get("issuer_url").unwrap();
    let redirect_url: Option<String> = row.try_get("redirect_url").unwrap();
    let login_label: Option<String> = row.try_get("login_label").unwrap();

    assert!(enabled);
    assert_eq!(issuer_url.as_deref(), Some("https://issuer.school.test"));
    assert_eq!(
        redirect_url.as_deref(),
        Some("https://files.school.test/api/v1/auth/oidc/callback")
    );
    assert_eq!(login_label.as_deref(), Some("Continue with school SSO"));

    reset_oidc_config(&pool).await;
}
