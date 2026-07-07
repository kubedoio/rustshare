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

    cleanup_user(&ctx.pool, user.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}
