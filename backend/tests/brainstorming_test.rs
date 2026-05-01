//! Integration tests for the Brainstorming module.

use bytes::Bytes;
use rustshare_core::domain::User;
use rustshare_server::services::brainstorming_service::BrainstormingService;
use std::sync::Arc;
use uuid::Uuid;

mod contracts;
use contracts::common::{
    cleanup_tenant, cleanup_user, create_test_file, create_test_folder, create_test_user,
    setup_test_env, setup_test_tenant,
};

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn test_list_brainstorm_boards_empty() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;
    let user = create_test_user(&ctx.metadata_store, "brainstorm_empty", tenant_id).await;

    let service = BrainstormingService::new(
        Arc::new(ctx.file_service()),
        Arc::new(ctx.folder_service()),
        Arc::clone(&ctx.metadata_store),
        Arc::clone(&ctx.object_store),
    );

    let boards = service
        .list_boards(user.id, tenant_id)
        .await
        .expect("list_boards should succeed");
    assert!(boards.is_empty());

    cleanup_user(&ctx.pool, user.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn test_create_and_get_brainstorm_board() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;
    let user = create_test_user(&ctx.metadata_store, "brainstorm_create", tenant_id).await;

    let file_service = ctx.file_service();
    let folder_service = ctx.folder_service();

    let service = BrainstormingService::new(
        Arc::new(file_service),
        Arc::new(folder_service),
        Arc::clone(&ctx.metadata_store),
        Arc::clone(&ctx.object_store),
    );

    // Create root folder and board folder manually (simulating template creation)
    let root = service
        .ensure_brainstorming_root(user.id, tenant_id)
        .await
        .expect("ensure root");

    let board_folder = create_test_folder(
        &ctx.folder_service(),
        user.id,
        tenant_id,
        "test-board",
        Some(root.id),
    )
    .await;

    // Create required files
    create_test_file(
        &ctx.file_service(),
        user.id,
        tenant_id,
        Some(board_folder.id),
        "board.excalidraw",
        b"{\"type\":\"excalidraw\",\"version\":2,\"elements\":[]}",
    )
    .await;

    let meta_content = r#"{"id":"test-id","type":"brainstorming.board","title":"Test Board","slug":"test-board","template":"template_blank_brainstorm","sourceFile":"board.excalidraw","previewFile":"preview.png","createdAt":"2026-04-30T00:00:00Z","updatedAt":"2026-04-30T00:00:00Z","schemaVersion":"1.0"}"#;
    create_test_file(
        &ctx.file_service(),
        user.id,
        tenant_id,
        Some(board_folder.id),
        ".rustshare.json",
        meta_content.as_bytes(),
    )
    .await;

    // List boards
    let boards = service
        .list_boards(user.id, tenant_id)
        .await
        .expect("list boards");
    assert_eq!(boards.len(), 1);
    assert_eq!(boards[0].title, "Test Board");
    assert_eq!(boards[0].slug, "test-board");

    // Get board
    let board = service
        .get_board(board_folder.id, user.id, tenant_id)
        .await
        .expect("get board");
    assert_eq!(board.title, "Test Board");
    assert!(board.source_file_id.is_some());

    cleanup_user(&ctx.pool, user.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn test_save_board_source_updates_metadata() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;
    let user = create_test_user(&ctx.metadata_store, "brainstorm_save", tenant_id).await;

    let service = BrainstormingService::new(
        Arc::new(ctx.file_service()),
        Arc::new(ctx.folder_service()),
        Arc::clone(&ctx.metadata_store),
        Arc::clone(&ctx.object_store),
    );

    let root = service
        .ensure_brainstorming_root(user.id, tenant_id)
        .await
        .expect("ensure root");

    let board_folder = create_test_folder(
        &ctx.folder_service(),
        user.id,
        tenant_id,
        "save-test",
        Some(root.id),
    )
    .await;

    create_test_file(
        &ctx.file_service(),
        user.id,
        tenant_id,
        Some(board_folder.id),
        "board.excalidraw",
        b"{\"type\":\"excalidraw\",\"version\":2,\"elements\":[]}",
    )
    .await;

    let meta_content = r#"{"id":"save-id","type":"brainstorming.board","title":"Save Test","slug":"save-test","template":"template_blank_brainstorm","sourceFile":"board.excalidraw","previewFile":"preview.png","createdAt":"2026-04-30T00:00:00Z","updatedAt":"2026-04-30T00:00:00Z","schemaVersion":"1.0"}"#;
    create_test_file(
        &ctx.file_service(),
        user.id,
        tenant_id,
        Some(board_folder.id),
        ".rustshare.json",
        meta_content.as_bytes(),
    )
    .await;

    let original_updated = service
        .get_board(board_folder.id, user.id, tenant_id)
        .await
        .unwrap()
        .updated_at;

    // Save new source
    let new_source = r#"{"type":"excalidraw","version":2,"elements":[{"id":"text1","type":"text","x":100,"y":100,"width":100,"height":20,"text":"Hello"}]}"#;
    service
        .save_board_source(board_folder.id, user.id, tenant_id, new_source.to_string())
        .await
        .expect("save source");

    // Verify source updated
    let source = service
        .get_board_source(board_folder.id, user.id, tenant_id)
        .await
        .expect("get source");
    assert!(source.contains("Hello"));

    // Verify metadata updatedAt changed
    let updated_board = service
        .get_board(board_folder.id, user.id, tenant_id)
        .await
        .expect("get board after save");
    assert!(updated_board.updated_at > original_updated);

    cleanup_user(&ctx.pool, user.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn test_update_board_preview() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;
    let user = create_test_user(&ctx.metadata_store, "brainstorm_preview", tenant_id).await;

    let service = BrainstormingService::new(
        Arc::new(ctx.file_service()),
        Arc::new(ctx.folder_service()),
        Arc::clone(&ctx.metadata_store),
        Arc::clone(&ctx.object_store),
    );

    let root = service
        .ensure_brainstorming_root(user.id, tenant_id)
        .await
        .expect("ensure root");

    let board_folder = create_test_folder(
        &ctx.folder_service(),
        user.id,
        tenant_id,
        "preview-test",
        Some(root.id),
    )
    .await;

    create_test_file(
        &ctx.file_service(),
        user.id,
        tenant_id,
        Some(board_folder.id),
        "board.excalidraw",
        b"{\"type\":\"excalidraw\",\"version\":2,\"elements\":[]}",
    )
    .await;

    let meta_content = r#"{"id":"preview-id","type":"brainstorming.board","title":"Preview Test","slug":"preview-test","template":"template_blank_brainstorm","sourceFile":"board.excalidraw","previewFile":"preview.png","createdAt":"2026-04-30T00:00:00Z","updatedAt":"2026-04-30T00:00:00Z","schemaVersion":"1.0"}"#;
    create_test_file(
        &ctx.file_service(),
        user.id,
        tenant_id,
        Some(board_folder.id),
        ".rustshare.json",
        meta_content.as_bytes(),
    )
    .await;

    // Update preview
    let png_bytes = Bytes::from_static(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]); // PNG magic
    let board = service
        .update_board_preview(board_folder.id, user.id, tenant_id, png_bytes)
        .await
        .expect("update preview");

    assert!(board.preview_file_id.is_some());

    // Update preview again
    let png_bytes2 = Bytes::from_static(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00]);
    let board2 = service
        .update_board_preview(board_folder.id, user.id, tenant_id, png_bytes2)
        .await
        .expect("update preview again");

    assert_eq!(board2.preview_file_id, board.preview_file_id);

    cleanup_user(&ctx.pool, user.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn test_delete_board() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;
    let user = create_test_user(&ctx.metadata_store, "brainstorm_delete", tenant_id).await;

    let service = BrainstormingService::new(
        Arc::new(ctx.file_service()),
        Arc::new(ctx.folder_service()),
        Arc::clone(&ctx.metadata_store),
        Arc::clone(&ctx.object_store),
    );

    let root = service
        .ensure_brainstorming_root(user.id, tenant_id)
        .await
        .expect("ensure root");

    let board_folder = create_test_folder(
        &ctx.folder_service(),
        user.id,
        tenant_id,
        "delete-test",
        Some(root.id),
    )
    .await;

    create_test_file(
        &ctx.file_service(),
        user.id,
        tenant_id,
        Some(board_folder.id),
        "board.excalidraw",
        b"{\"type\":\"excalidraw\",\"version\":2,\"elements\":[]}",
    )
    .await;

    // Delete board
    service
        .delete_board(board_folder.id, user.id, tenant_id)
        .await
        .expect("delete board");

    // Verify it's gone
    let boards = service.list_boards(user.id, tenant_id).await.expect("list boards");
    assert!(boards.is_empty());

    cleanup_user(&ctx.pool, user.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn test_invalid_excalidraw_rejected() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;
    let user = create_test_user(&ctx.metadata_store, "brainstorm_invalid", tenant_id).await;

    let service = BrainstormingService::new(
        Arc::new(ctx.file_service()),
        Arc::new(ctx.folder_service()),
        Arc::clone(&ctx.metadata_store),
        Arc::clone(&ctx.object_store),
    );

    let root = service
        .ensure_brainstorming_root(user.id, tenant_id)
        .await
        .expect("ensure root");

    let board_folder = create_test_folder(
        &ctx.folder_service(),
        user.id,
        tenant_id,
        "invalid-test",
        Some(root.id),
    )
    .await;

    create_test_file(
        &ctx.file_service(),
        user.id,
        tenant_id,
        Some(board_folder.id),
        "board.excalidraw",
        b"{\"type\":\"excalidraw\",\"version\":2,\"elements\":[]}",
    )
    .await;

    // Missing type field
    let result = service
        .save_board_source(
            board_folder.id,
            user.id,
            tenant_id,
            r#"{"version":2,"elements":[]}"#.to_string(),
        )
        .await;
    assert!(result.is_err());

    // Missing elements array
    let result2 = service
        .save_board_source(
            board_folder.id,
            user.id,
            tenant_id,
            r#"{"type":"excalidraw","version":2}"#.to_string(),
        )
        .await;
    assert!(result2.is_err());

    cleanup_user(&ctx.pool, user.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}
