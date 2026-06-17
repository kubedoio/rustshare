//! Integration tests: Admin SMTP config SQL lifecycle (Task 6).
//!
//! Tests:
//!   - Update the pre-seeded smtp_config row with host, port, encrypted password
//!   - Read back, verify password_enc is set (not plaintext)
//!   - Update config again and verify updated_at changes
//!
//! Run with: cargo test --test admin_config_smtp_test -- --ignored

use base64::{engine::general_purpose::STANDARD, Engine};
use rustshare_crypto::{decrypt_secret, encrypt_secret, SecretEncryptionKey};
use sqlx::Row;
use uuid::Uuid;

const SMTP_CONFIG_ID: &str = "00000000-0000-0000-0000-000000000002";

/// SMTP config tests mutate a single pre-seeded row, so they must run
/// serially to avoid reading state written by a concurrent test.
static SMTP_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

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
    .bind(format!("smtp_admin_{suffix}"))
    .bind(format!("smtpadmin_{suffix}@test.local"))
    .bind("$argon2id$v=19$m=4096,t=3,p=1$placeholder_hash")
    .bind(format!("SMTP Admin {suffix}"))
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

async fn reset_smtp_config(pool: &sqlx::PgPool) {
    let smtp_id: Uuid = SMTP_CONFIG_ID.parse().unwrap();
    sqlx::query(
        "UPDATE smtp_config
         SET enabled = false, host = NULL, port = NULL, username = NULL,
             password_enc = NULL, from_address = NULL, from_name = NULL,
             tls_mode = NULL, updated_by = NULL, updated_at = NOW()
         WHERE id = $1",
    )
    .bind(smtp_id)
    .execute(pool)
    .await
    .expect("reset smtp_config");
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Update SMTP config with encrypted password, verify password_enc is stored (not plaintext).
#[tokio::test]
async fn test_smtp_config_update_stores_encrypted_password() {
    let _guard = SMTP_TEST_LOCK.lock().await;

    let pool = test_pool().await;
    let suffix = &Uuid::new_v4().to_string()[..8];
    let actor_id = create_test_admin(&pool, suffix).await;
    let key = test_encryption_key();
    let smtp_id: Uuid = SMTP_CONFIG_ID.parse().unwrap();
    let plaintext_password = "smtp-plaintext-password-123";

    // Start from a known empty state in case a previous run left data behind.
    reset_smtp_config(&pool).await;

    let password_enc = encrypt_secret(plaintext_password, &key).expect("encrypt SMTP password");

    // Same SQL as the handler uses
    sqlx::query(
        "UPDATE smtp_config
         SET enabled      = $2,
             host         = $3,
             port         = $4,
             username     = $5,
             password_enc = $6,
             from_address = $7,
             from_name    = $8,
             tls_mode     = $9,
             updated_by   = $10,
             updated_at   = NOW()
         WHERE id = $1",
    )
    .bind(smtp_id)
    .bind(true)
    .bind("smtp.example.com")
    .bind(587_i32)
    .bind("noreply@example.com")
    .bind(&password_enc)
    .bind("noreply@example.com")
    .bind("RustShare Notifications")
    .bind("starttls")
    .bind(actor_id)
    .execute(&pool)
    .await
    .expect("update smtp_config");

    // Read back
    let row = sqlx::query(
        "SELECT enabled, host, port, username, password_enc, tls_mode, updated_by
         FROM smtp_config WHERE id = $1",
    )
    .bind(smtp_id)
    .fetch_one(&pool)
    .await
    .expect("fetch smtp_config");

    let enabled: bool = row.try_get("enabled").unwrap();
    let host: Option<String> = row.try_get("host").unwrap();
    let port: Option<i32> = row.try_get("port").unwrap();
    let stored_enc: Option<String> = row.try_get("password_enc").unwrap();
    let tls_mode: Option<String> = row.try_get("tls_mode").unwrap();

    assert!(enabled, "SMTP must be enabled after update");
    assert_eq!(host.as_deref(), Some("smtp.example.com"));
    assert_eq!(port, Some(587));
    assert_eq!(tls_mode.as_deref(), Some("starttls"));

    let stored = stored_enc.as_deref().expect("password_enc must be set");
    assert_ne!(
        stored, plaintext_password,
        "Plaintext password must not be stored"
    );

    // Round-trip decrypt must recover the original password
    let recovered = decrypt_secret(stored, &key).expect("decrypt stored password");
    assert_eq!(
        recovered, plaintext_password,
        "Decrypted password must match original"
    );

    // Cleanup
    reset_smtp_config(&pool).await;
    cleanup_users(&pool, &[actor_id]).await;
}

/// Second update must change updated_at.
#[tokio::test]
async fn test_smtp_config_update_changes_updated_at() {
    let _guard = SMTP_TEST_LOCK.lock().await;

    let pool = test_pool().await;
    let suffix = &Uuid::new_v4().to_string()[..8];
    let actor_id = create_test_admin(&pool, suffix).await;
    let smtp_id: Uuid = SMTP_CONFIG_ID.parse().unwrap();

    reset_smtp_config(&pool).await;

    // First update — set a timestamp we can compare against
    sqlx::query(
        "UPDATE smtp_config
         SET host = $2, port = $3, updated_by = $4, updated_at = NOW()
         WHERE id = $1",
    )
    .bind(smtp_id)
    .bind("smtp1.example.com")
    .bind(25_i32)
    .bind(actor_id)
    .execute(&pool)
    .await
    .expect("first update");

    let first_updated_at: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT updated_at FROM smtp_config WHERE id = $1")
            .bind(smtp_id)
            .fetch_one(&pool)
            .await
            .expect("fetch updated_at after first update");

    // Sleep briefly to ensure NOW() advances
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    // Second update
    sqlx::query(
        "UPDATE smtp_config
         SET host = $2, port = $3, updated_by = $4, updated_at = NOW()
         WHERE id = $1",
    )
    .bind(smtp_id)
    .bind("smtp2.example.com")
    .bind(587_i32)
    .bind(actor_id)
    .execute(&pool)
    .await
    .expect("second update");

    let second_updated_at: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT updated_at FROM smtp_config WHERE id = $1")
            .bind(smtp_id)
            .fetch_one(&pool)
            .await
            .expect("fetch updated_at after second update");

    assert!(
        second_updated_at > first_updated_at,
        "updated_at must increase after second update"
    );

    // Cleanup
    reset_smtp_config(&pool).await;
    cleanup_users(&pool, &[actor_id]).await;
}
