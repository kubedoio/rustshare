use aes_gcm::{
    aead::{consts::U12, Aead, KeyInit, Payload},
    Aes256Gcm, Nonce,
};
use async_trait::async_trait;
use rustshare_crypto::managed_identity_vault::{
    DekKeyManagement, IdentityContext, KeyEncryptionContext, ManagedIdentityEnvelope, VaultError,
    WrappedDek,
};
use rustshare_storage::{ManagedIdentityStatus, ManagedIdentityVaultStore};
use sqlx::PgPool;
use std::collections::HashMap;
use uuid::Uuid;
use zeroize::Zeroizing;

#[derive(Clone)]
struct TestKms {
    keys: HashMap<String, Zeroizing<[u8; 32]>>,
    current_version: String,
}

impl TestKms {
    fn new(byte: u8, version: &str) -> Self {
        Self {
            keys: HashMap::from([(version.to_owned(), Zeroizing::new([byte; 32]))]),
            current_version: version.to_owned(),
        }
    }

    fn with_rotation(old: Self, byte: u8, version: &str) -> Self {
        let mut keys = old.keys;
        keys.insert(version.to_owned(), Zeroizing::new([byte; 32]));
        Self {
            keys,
            current_version: version.to_owned(),
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
        let key = self
            .keys
            .get(&self.current_version)
            .ok_or(VaultError::KeyManagement)?;
        let cipher = Aes256Gcm::new_from_slice(&key[..]).map_err(|_| VaultError::KeyManagement)?;
        let nonce = random_nonce();
        let ciphertext = cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: dek,
                    aad: &kms_context_aad(context),
                },
            )
            .map_err(|_| VaultError::KeyManagement)?;
        let mut wrapped = nonce.as_slice().to_vec();
        wrapped.extend(ciphertext);
        Ok(WrappedDek {
            key_id: "test-only".into(),
            key_version: self.current_version.clone(),
            ciphertext: wrapped,
        })
    }

    async fn unwrap_dek(
        &self,
        wrapped: &WrappedDek,
        context: &KeyEncryptionContext,
    ) -> Result<Zeroizing<[u8; 32]>, VaultError> {
        if wrapped.key_id != "test-only" {
            return Err(VaultError::KeyManagement);
        }
        let key = self
            .keys
            .get(&wrapped.key_version)
            .ok_or(VaultError::KeyManagement)?;
        if wrapped.ciphertext.len() != 60 {
            return Err(VaultError::KeyManagement);
        }
        let cipher = Aes256Gcm::new_from_slice(&key[..]).map_err(|_| VaultError::KeyManagement)?;
        let nonce = Nonce::<U12>::try_from(&wrapped.ciphertext[..12])
            .map_err(|_| VaultError::KeyManagement)?;
        let plaintext = cipher
            .decrypt(
                &nonce,
                Payload {
                    msg: &wrapped.ciphertext[12..],
                    aad: &kms_context_aad(context),
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

fn random_nonce() -> Nonce<U12> {
    rand::random::<[u8; 12]>().into()
}

fn kms_context_aad(context: &KeyEncryptionContext) -> Vec<u8> {
    let mut aad = Vec::new();
    aad.extend_from_slice(b"rustshare-managed-nostr-identity\0");
    aad.extend_from_slice(context.tenant_id.as_bytes());
    aad.extend_from_slice(context.principal_id.as_bytes());
    aad.extend_from_slice(context.identity_id.as_bytes());
    aad.extend_from_slice(&context.generation.to_be_bytes());
    aad.extend_from_slice(&context.envelope_version.to_be_bytes());
    aad.extend_from_slice(context.public_key.as_bytes());
    aad
}

fn kms(byte: u8) -> TestKms {
    TestKms::new(byte, "v1")
}

fn context(tenant_id: Uuid, principal_id: Uuid, generation: i64) -> IdentityContext {
    IdentityContext {
        tenant_id,
        principal_id,
        identity_id: Uuid::new_v4(),
        generation,
    }
}

async fn scoped_count(pool: &PgPool, tenant_id: Uuid, principal_id: Uuid) -> i64 {
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('app.current_tenant_id', $1, true)")
        .bind(tenant_id.to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    let count = sqlx::query_scalar(
        "SELECT count(*) FROM managed_chat_identities WHERE tenant_id=$1 AND principal_id=$2",
    )
    .bind(tenant_id)
    .bind(principal_id)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    count
}

#[tokio::test]
#[ignore = "requires PostgreSQL; exercised by the integration-tests workflow"]
async fn managed_identity_vault_postgres_invariants_and_recovery() {
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must point at a disposable PostgreSQL database");
    let pool = PgPool::connect(&database_url).await.unwrap();
    sqlx::migrate!("../migrations").run(&pool).await.unwrap();

    // Existing client-held identities and replacement/recovery state remain intact.
    for table in [
        "chat_identity_bindings",
        "chat_identity_replacements",
        "chat_identity_replacement_commands",
    ] {
        let exists: bool = sqlx::query_scalar("SELECT to_regclass($1) IS NOT NULL")
            .bind(table)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(exists, "legacy Chat table missing: {table}");
    }

    let store = ManagedIdentityVaultStore::new(pool.clone());
    let tenant = Uuid::new_v4();
    let principal = Uuid::new_v4();
    let key_manager = kms(44);
    let identity_context = context(tenant, principal, 1);
    let (left, right) = tokio::join!(
        async {
            let envelope = ManagedIdentityEnvelope::generate(identity_context, &key_manager)
                .await
                .unwrap();
            store.insert_or_load(Uuid::from_u128(1), &envelope).await
        },
        async {
            let envelope = ManagedIdentityEnvelope::generate(identity_context, &key_manager)
                .await
                .unwrap();
            store.insert_or_load(Uuid::from_u128(1), &envelope).await
        }
    );
    let left = left.unwrap();
    let right = right.unwrap();
    assert_eq!(left.identity_id, right.identity_id);
    assert_eq!(scoped_count(&pool, tenant, principal).await, 1);

    // A database-only record contains only AEAD ciphertext and wrapped DEK.
    let stored = store.get(tenant, left.identity_id).await.unwrap();
    assert_eq!(stored.ciphertext.len(), 48);
    assert!(!stored.wrapped_dek.is_empty());
    assert_eq!(
        stored
            .envelope()
            .unwrap()
            .recover(identity_context, &key_manager)
            .await
            .unwrap()
            .public_key(),
        stored.public_key
    );
    assert!(stored
        .envelope()
        .unwrap()
        .recover(identity_context, &kms(45))
        .await
        .is_err());

    let original_envelope = stored.envelope().unwrap();
    let mut rewrapped = original_envelope.clone();
    let rotated_kms = TestKms::with_rotation(key_manager.clone(), 46, "v2");
    rewrapped.rewrap_dek(&rotated_kms).await.unwrap();
    store
        .persist_rewrapped_dek(&original_envelope, &rewrapped)
        .await
        .unwrap();
    let after_rotation = store.get(tenant, left.identity_id).await.unwrap();
    assert_eq!(after_rotation.public_key, stored.public_key);
    assert_eq!(after_rotation.ciphertext, stored.ciphertext);
    assert_eq!(after_rotation.kms_key_version, "v2");
    assert_eq!(
        after_rotation
            .envelope()
            .unwrap()
            .recover(identity_context, &rotated_kms)
            .await
            .unwrap()
            .public_key(),
        stored.public_key
    );
    let mut stale_rewrap = original_envelope.clone();
    stale_rewrap.rewrap_dek(&rotated_kms).await.unwrap();
    assert_eq!(
        store
            .persist_rewrapped_dek(&original_envelope, &stale_rewrap)
            .await
            .unwrap_err(),
        rustshare_storage::ManagedIdentityStoreError::Conflict
    );

    let mut tx = pool.begin().await.unwrap();
    sqlx::query(
        "CREATE ROLE managed_identity_vault_rls_probe \
         NOLOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOINHERIT NOBYPASSRLS",
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "GRANT SELECT, UPDATE ON managed_chat_identities TO managed_identity_vault_rls_probe",
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query("GRANT managed_identity_vault_rls_probe TO CURRENT_USER")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("SET LOCAL ROLE managed_identity_vault_rls_probe")
        .execute(&mut *tx)
        .await
        .unwrap();
    let unscoped_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM managed_chat_identities WHERE tenant_id=$1")
            .bind(tenant)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert_eq!(unscoped_count, 0);

    sqlx::query("SELECT set_config('app.current_tenant_id', $1, true)")
        .bind(tenant.to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    let mutation = sqlx::query(
        "UPDATE managed_chat_identities SET public_key=repeat('0',64) \
         WHERE tenant_id=$1 AND identity_id=$2",
    )
    .bind(tenant)
    .bind(left.identity_id)
    .execute(&mut *tx)
    .await;
    assert!(mutation.is_err());
    tx.rollback().await.unwrap();

    // Global uniqueness prevents attribution of a key to another tenant.
    let mut reused_key = stored.envelope().unwrap();
    reused_key.context.tenant_id = Uuid::new_v4();
    reused_key.context.principal_id = Uuid::new_v4();
    reused_key.context.identity_id = Uuid::new_v4();
    assert!(store
        .insert_or_load(Uuid::new_v4(), &reused_key)
        .await
        .is_err());

    // Different concurrent requests cannot create a second active identity.
    let racing_principal = Uuid::new_v4();
    let competing_context = context(tenant, racing_principal, 1);
    let (a, b) = tokio::join!(
        async {
            let envelope = ManagedIdentityEnvelope::generate(competing_context, &key_manager)
                .await
                .unwrap();
            store.insert_or_load(Uuid::new_v4(), &envelope).await
        },
        async {
            let envelope = ManagedIdentityEnvelope::generate(competing_context, &key_manager)
                .await
                .unwrap();
            store.insert_or_load(Uuid::new_v4(), &envelope).await
        }
    );
    assert!(a.is_ok() ^ b.is_ok(), "a={a:?}, b={b:?}");
    assert_eq!(scoped_count(&pool, tenant, principal).await, 1);
    assert_eq!(scoped_count(&pool, tenant, racing_principal).await, 1);

    store
        .disable_or_revoke(tenant, left.identity_id, ManagedIdentityStatus::Disabled)
        .await
        .unwrap();
    assert_eq!(
        store.get(tenant, left.identity_id).await.unwrap().status,
        "disabled"
    );
    assert_eq!(
        store
            .get(tenant, left.identity_id)
            .await
            .unwrap()
            .envelope()
            .unwrap_err(),
        rustshare_storage::ManagedIdentityStoreError::NotFound
    );
    assert_eq!(
        store
            .get(Uuid::new_v4(), left.identity_id)
            .await
            .unwrap_err()
            .to_string(),
        "managed identity was not found or is not active"
    );
    assert_eq!(
        store
            .disable_or_revoke(tenant, left.identity_id, ManagedIdentityStatus::Revoked)
            .await
            .unwrap_err()
            .to_string(),
        "managed identity was not found or is not active"
    );

    let next_context = context(tenant, principal, 2);
    let next = ManagedIdentityEnvelope::generate(next_context, &key_manager)
        .await
        .unwrap();
    let next = store.insert_or_load(Uuid::new_v4(), &next).await.unwrap();
    store
        .disable_or_revoke(tenant, next.identity_id, ManagedIdentityStatus::Revoked)
        .await
        .unwrap();
    assert_eq!(
        store.get(tenant, next.identity_id).await.unwrap().status,
        "revoked"
    );
    assert_eq!(
        store
            .get(tenant, next.identity_id)
            .await
            .unwrap()
            .envelope()
            .unwrap_err(),
        rustshare_storage::ManagedIdentityStoreError::NotFound
    );

    // The immutable identity row retains attribution while state changes.
    let immutable: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM pg_trigger WHERE tgname='managed_chat_identities_immutable_attribution' AND NOT tgisinternal)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(immutable);
    pool.close().await;
}
