use rustshare_storage::MetadataStore;
use sqlx::PgPool;
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires disposable PostgreSQL with current migrations"]
async fn delete_cas_rejects_a_file_restored_after_observation() {
    let pool = PgPool::connect(&std::env::var("DATABASE_URL").expect("DATABASE_URL"))
        .await
        .expect("connect to disposable PostgreSQL");
    let id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();
    let owner_id = Uuid::new_v4();
    let file_id = Uuid::new_v4();
    sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, $2)")
        .bind(tenant_id)
        .bind(format!("forget-cas-{id}"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO users (id, username, email, password_hash, tenant_id, display_name, storage_quota) VALUES ($1,$2,$3,'hash',$4,'owner',10737418240)")
        .bind(owner_id)
        .bind(format!("forget-cas-{id}"))
        .bind(format!("forget-cas-{id}@example.test"))
        .bind(tenant_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO files (id,name,path,size,mime_type,content_hash,storage_key,owner_id,tenant_id,current_version) VALUES ($1,'shell.md','/shell.md',1,'text/markdown','same-content','key',$2,$3,1)")
        .bind(file_id)
        .bind(owner_id)
        .bind(tenant_id)
        .execute(&pool)
        .await
        .unwrap();

    let observed_modified_at: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT modified_at FROM files WHERE id = $1")
            .bind(file_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let store = MetadataStore::new(pool.clone());
    sqlx::query("UPDATE files SET deleted_at = now() WHERE id = $1")
        .bind(file_id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(store
        .restore_file(file_id, owner_id, tenant_id)
        .await
        .unwrap());
    let mut tx = pool.begin().await.unwrap();
    let deleted = store
        .delete_file_if_unchanged_in_tx(
            &mut tx,
            file_id,
            owner_id,
            "same-content",
            1,
            observed_modified_at,
            tenant_id,
        )
        .await
        .unwrap();
    assert!(
        !deleted,
        "restored file must not match the prior lifecycle revision"
    );
    tx.rollback().await.unwrap();

    let still_active: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT deleted_at FROM files WHERE id = $1")
            .bind(file_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(still_active.is_none());

    sqlx::query("DELETE FROM files WHERE id = $1")
        .bind(file_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(owner_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM tenants WHERE id = $1")
        .bind(tenant_id)
        .execute(&pool)
        .await
        .unwrap();
}
