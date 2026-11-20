//! Database-backed checks for the folder metadata error boundary.
//!
//! Run with the guarded test DB and object store:
//! `SQLX_OFFLINE=true cargo test -p rustshare-server --test folder_metadata_error_test -- --ignored --test-threads=1`

use rustshare_core::domain::Folder;
use rustshare_core::services::{FolderError, FolderMetadataError, FolderMetadataStoreOps};
use uuid::Uuid;

mod support;
use support::calendar_harness::{
    cleanup_tenant, create_test_tenant, create_test_user, setup_test_env, SERIAL,
};

#[tokio::test]
#[ignore = "requires the guarded disposable PostgreSQL and object-store services"]
async fn folder_create_maps_only_the_named_unique_conflict_to_duplicate_name() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env().await;
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let user = create_test_user(&state, "folder_metadata_error", tenant_id).await;

    let parent = state
        .folder_service
        .create_folder("Workspace".into(), None, user.id, tenant_id)
        .await
        .expect("create parent folder");
    state
        .folder_service
        .create_folder("Notes".into(), Some(parent.id), user.id, tenant_id)
        .await
        .expect("create first child folder");
    assert!(matches!(
        state
            .folder_service
            .create_folder("Notes".into(), Some(parent.id), user.id, tenant_id)
            .await,
        Err(FolderError::DuplicateName { .. })
    ));

    let orphan = Folder::new_child_with_ancestors(
        "Orphan".into(),
        "/missing/Orphan".into(),
        Uuid::new_v4(),
        None,
        user.id,
        tenant_id,
    );
    let mut tx = state
        .db_pool
        .begin()
        .await
        .expect("begin storage test transaction");
    let result = FolderMetadataStoreOps::create_folder_in_tx(
        state.metadata_store.as_ref(),
        &mut tx,
        &orphan,
    )
    .await;
    assert!(matches!(result, Err(FolderMetadataError::Failed(_))));
    tx.rollback().await.expect("rollback orphan insert");

    cleanup_tenant(&state.db_pool, tenant_id).await;
}
