//! Integration test for the `.eml` import flow.

use std::path::Path;
use std::sync::Arc;

use rustshare_server::services::mail_service::MailService;
use sha2::{Digest, Sha256};

mod contracts;
use contracts::common::{cleanup_tenant, cleanup_user, create_test_user, setup_test_env};

#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn import_eml_creates_message_and_persists_source_blob() {
    let ctx = setup_test_env().await;
    let tenant_id = ctx.tenant_id;
    let user = create_test_user(&ctx.metadata_store, "mail_import", tenant_id).await;

    let fixture_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../crates/core/tests/fixtures/eml/simple_plain.eml");
    let raw_source = std::fs::read(&fixture_path).expect("failed to read simple_plain.eml fixture");

    let file_service = Arc::new(ctx.file_service());
    let folder_service = Arc::new(ctx.folder_service());

    let mail_service = MailService::new(
        Arc::clone(&ctx.metadata_store),
        Arc::clone(&ctx.object_store),
        file_service,
        folder_service,
        Arc::clone(&ctx.permission_resolver()),
        Arc::clone(&ctx.event_store),
    );

    let message = mail_service
        .import_eml(tenant_id, user.id, user.id, raw_source.clone())
        .await
        .expect("import_eml should succeed");

    assert_eq!(message.subject, Some("Hello World".to_string()));
    assert_eq!(message.from_address, Some("sender@example.com".to_string()));
    assert_eq!(message.source_mode, "eml_upload");
    assert_eq!(message.visibility, "private");

    let blob_key = message.blob_key.expect("blob_key should be set");
    assert!(blob_key.starts_with("blobs/"));

    let expected_sha256 = hex::encode(Sha256::digest(&raw_source));
    assert_eq!(message.blob_sha256, Some(expected_sha256));

    let stored_bytes = ctx
        .object_store
        .get(&blob_key)
        .await
        .expect("object_store.get should succeed");
    assert_eq!(stored_bytes.to_vec(), raw_source);

    let fetched = mail_service
        .get_message(tenant_id, user.id, message.id)
        .await
        .expect("get_message should succeed");
    assert_eq!(fetched.id, message.id);
    assert_eq!(fetched.folder_id, message.folder_id);
    assert!(fetched.folder_id.is_some(), "folder_id should be set");

    let folder_id = fetched.folder_id.unwrap();
    let message_folder = ctx
        .metadata_store
        .find_folder_by_id(folder_id, user.id)
        .await
        .expect("find_folder_by_id should not fail")
        .expect("message folder should exist");
    assert!(message_folder.path.starts_with("/Workspace/Mail/"));

    let files = ctx
        .metadata_store
        .list_files(Some(folder_id), user.id, tenant_id)
        .await
        .expect("list_files should not fail");
    let source_file = files
        .iter()
        .find(|f| f.name == "source.eml")
        .expect("source.eml should exist in message folder");
    let stored_source = ctx
        .object_store
        .get(&source_file.storage_key())
        .await
        .expect("source.eml object should be retrievable");
    assert_eq!(stored_source.to_vec(), raw_source);

    // Best-effort cleanup of persisted object-storage blobs.
    let _ = ctx.object_store.delete(&blob_key).await;
    let _ = ctx.object_store.delete(&source_file.storage_key()).await;

    // Best-effort cleanup of mail rows (owner deletion cascades, but this
    // keeps manual test runs tidy).
    let _ = sqlx::query("DELETE FROM mail_message_parts WHERE message_id = $1")
        .bind(message.id)
        .execute(&ctx.pool)
        .await;
    let _ = sqlx::query("DELETE FROM mail_attachments WHERE message_id = $1")
        .bind(message.id)
        .execute(&ctx.pool)
        .await;
    let _ = sqlx::query("DELETE FROM mail_messages WHERE id = $1")
        .bind(message.id)
        .execute(&ctx.pool)
        .await;

    cleanup_user(&ctx.pool, user.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn import_eml_promotes_attachments_to_file_artifacts() {
    let ctx = setup_test_env().await;
    let tenant_id = ctx.tenant_id;
    let user = create_test_user(&ctx.metadata_store, "mail_import_attach", tenant_id).await;

    let fixture_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../crates/core/tests/fixtures/eml/with_attachment.eml");
    let raw_source =
        std::fs::read(&fixture_path).expect("failed to read with_attachment.eml fixture");

    let file_service = Arc::new(ctx.file_service());
    let folder_service = Arc::new(ctx.folder_service());

    let mail_service = MailService::new(
        Arc::clone(&ctx.metadata_store),
        Arc::clone(&ctx.object_store),
        file_service,
        folder_service,
        Arc::clone(&ctx.permission_resolver()),
        Arc::clone(&ctx.event_store),
    );

    let message = mail_service
        .import_eml(tenant_id, user.id, user.id, raw_source.clone())
        .await
        .expect("import_eml should succeed");

    assert_eq!(message.subject, Some("With attachment".to_string()));
    assert!(message.has_attachments, "message should have attachments");

    let folder_id = message.folder_id.expect("message folder_id should be set");

    let attachments = ctx
        .metadata_store
        .list_mail_attachments_by_message_id(message.id, tenant_id, user.id)
        .await
        .expect("list_mail_attachments_by_message_id should succeed");
    assert!(
        !attachments.is_empty(),
        "mail_attachments should not be empty"
    );

    let files = ctx
        .metadata_store
        .list_files(Some(folder_id), user.id, tenant_id)
        .await
        .expect("list_files should not fail");
    let source_file = files
        .iter()
        .find(|f| f.name == "source.eml")
        .expect("source.eml should exist in message folder");

    let mut object_keys_to_clean: Vec<String> = vec![
        message.blob_key.expect("message blob_key should be set"),
        source_file.storage_key(),
    ];

    for att in &attachments {
        let file_id = att
            .file_id
            .expect("attachment file_id should be set to a File artifact");
        let file = ctx
            .metadata_store
            .find_file_by_id(file_id, user.id)
            .await
            .expect("find_file_by_id should not fail")
            .expect("attachment File artifact should exist");

        assert_eq!(file.parent_folder_id, Some(folder_id));
        assert_eq!(file.name, att.filename);

        let stored = ctx
            .object_store
            .get(&file.storage_key())
            .await
            .expect("attachment file object should be retrievable");
        assert_eq!(
            stored.len() as i64,
            att.size_bytes.expect("attachment size should be set")
        );

        object_keys_to_clean.push(file.storage_key());

        let blob_key = att
            .blob_key
            .as_ref()
            .expect("attachment blob_key should be kept as source evidence");
        let blob_bytes = ctx
            .object_store
            .get(blob_key)
            .await
            .expect("attachment source blob should be retrievable");
        assert_eq!(blob_bytes.to_vec(), stored.to_vec());
        object_keys_to_clean.push(blob_key.clone());
    }

    for key in &object_keys_to_clean {
        let _ = ctx.object_store.delete(key).await;
    }

    // Best-effort cleanup of mail rows.
    let _ = sqlx::query("DELETE FROM mail_message_parts WHERE message_id = $1")
        .bind(message.id)
        .execute(&ctx.pool)
        .await;
    let _ = sqlx::query("DELETE FROM mail_attachments WHERE message_id = $1")
        .bind(message.id)
        .execute(&ctx.pool)
        .await;
    let _ = sqlx::query("DELETE FROM mail_messages WHERE id = $1")
        .bind(message.id)
        .execute(&ctx.pool)
        .await;

    cleanup_user(&ctx.pool, user.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}
