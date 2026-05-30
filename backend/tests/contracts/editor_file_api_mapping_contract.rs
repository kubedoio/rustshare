//! Editor File API Mapping Contract Tests (HP-01)
//!
//! Tests that existing file/note/folder APIs satisfy the editor contract.
//!
//! Editor operation → Backend route mapping:
//! - Get document      → GET /api/v1/files/{id}  or GET /api/v1/notes/{id}
//! - Save content      → PUT /api/v1/files/{id} with If-Match  or POST /api/v1/files/{id}/edit
//! - Upload attachment → POST /api/v1/files/upload (parent_folder_id = attachments folder)
//! - List attachments  → GET /api/v1/folders/{id}/contents
//! - Delete attachment → DELETE /api/v1/files/{id}

use crate::common::*;
use rustshare_core::services::FileError;

// ============================================================================
// E-MAP-01: Document get maps to file get
// ============================================================================

/// A Markdown file can be retrieved by ID and carries version metadata.
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_document_get_maps_to_file_get() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;
    let user = create_test_user(&ctx.metadata_store, "editor_user", tenant_id).await;
    let file_service = ctx.file_service();

    let file = create_test_file(
        &file_service,
        user.id,
        tenant_id,
        None,
        "document.md",
        b"# Hello World",
    )
    .await;

    // Document get → file get
    let retrieved = file_service
        .get_file(file.id, user.id)
        .await
        .expect("get_file should succeed");
    assert_eq!(retrieved.id, file.id);
    assert_eq!(retrieved.name, "document.md");
    assert_eq!(retrieved.current_version, 1);
    assert!(
        retrieved.mime_type.contains("markdown")
            || retrieved.mime_type == "application/octet-stream"
    );

    cleanup_user(&ctx.pool, user.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

// ============================================================================
// E-MAP-02: Save with baseRevision maps to file update with If-Match
// ============================================================================

/// Saving with a matching baseRevision (file version) succeeds and bumps the version.
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_document_save_maps_to_file_update_with_optimistic_locking() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;
    let user = create_test_user(&ctx.metadata_store, "editor_user", tenant_id).await;
    let file_service = ctx.file_service();

    let file = create_test_file(
        &file_service,
        user.id,
        tenant_id,
        None,
        "document.md",
        b"# Version 1",
    )
    .await;

    assert_eq!(
        file.current_version, 1,
        "New file should start at version 1"
    );

    // Save with correct baseRevision (If-Match: 1) → success
    let updated = file_service
        .update_file(file.id, user.id, 1, bytes::Bytes::from("# Version 2"))
        .await
        .expect("update with correct version should succeed");

    assert_eq!(updated.current_version, 2, "Version should bump after save");

    cleanup_user(&ctx.pool, user.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

/// Saving with a stale baseRevision returns a version conflict.
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_document_save_stale_revision_returns_conflict() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;
    let user = create_test_user(&ctx.metadata_store, "editor_user", tenant_id).await;
    let file_service = ctx.file_service();

    let file = create_test_file(
        &file_service,
        user.id,
        tenant_id,
        None,
        "document.md",
        b"# Version 1",
    )
    .await;

    // Another save bumps version to 2
    let _updated = file_service
        .update_file(file.id, user.id, 1, bytes::Bytes::from("# Version 2"))
        .await
        .expect("first update should succeed");

    // Save with stale baseRevision 1 → conflict
    let result = file_service
        .update_file(file.id, user.id, 1, bytes::Bytes::from("# Version 3"))
        .await;

    assert!(
        matches!(result, Err(FileError::VersionConflict { .. })),
        "Stale baseRevision should produce VersionConflict"
    );

    cleanup_user(&ctx.pool, user.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

// ============================================================================
// E-MAP-02b: Note save maps to note_service.save_note
// ============================================================================

/// Saving a note document uses the note service rather than raw file update.
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_note_save_maps_to_note_service_save() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;
    let user = create_test_user(&ctx.metadata_store, "editor_note_user", tenant_id).await;
    let note_service = ctx.note_service();

    // Create a note (folder-backed document)
    let note = note_service
        .create_note(
            user.id,
            tenant_id,
            Some("Editor Test Note".to_string()),
            None,
            Some("# Initial heading".to_string()),
        )
        .await
        .expect("create_note should succeed");

    let initial_version = note.current_version;

    // Save note content via note service
    let saved = note_service
        .save_note(
            note.id,
            user.id,
            "# Updated heading\n\nNew paragraph.".to_string(),
            None,
            None,
        )
        .await
        .expect("save_note should succeed");

    assert_eq!(saved.content, "# Updated heading\n\nNew paragraph.");
    assert!(
        saved.current_version >= initial_version,
        "Note version should increment or stay same after save"
    );

    cleanup_user(&ctx.pool, user.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

// ============================================================================
// E-MAP-03: Attachment upload maps to file upload into attachments folder
// ============================================================================

/// Uploading an attachment creates a file inside the designated attachments folder.
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_attachment_upload_maps_to_file_upload_in_folder() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;
    let user = create_test_user(&ctx.metadata_store, "editor_user", tenant_id).await;
    let folder_service = ctx.folder_service();
    let file_service = ctx.file_service();

    // Create document folder and attachments subfolder
    let doc_folder = create_test_folder(
        &folder_service,
        user.id,
        tenant_id,
        "Project-Brainstorm",
        None,
    )
    .await;
    let attachments_folder = create_test_folder(
        &folder_service,
        user.id,
        tenant_id,
        "attachments",
        Some(doc_folder.id),
    )
    .await;

    // Upload attachment into attachments folder
    let attachment = file_service
        .upload_file(
            user.id,
            "diagram.png".to_string(),
            Some(attachments_folder.id),
            bytes::Bytes::from("fake image data"),
            "image/png".to_string(),
            tenant_id,
        )
        .await
        .expect("upload should succeed");

    assert_eq!(attachment.name, "diagram.png");
    assert_eq!(attachment.parent_folder_id, Some(attachments_folder.id));

    // Verify it appears in folder listing
    let files_in_folder = ctx
        .metadata_store
        .list_files_by_parent(Some(attachments_folder.id), tenant_id)
        .await
        .expect("list files should succeed");

    assert_eq!(files_in_folder.len(), 1);
    assert_eq!(files_in_folder[0].id, attachment.id);

    cleanup_user(&ctx.pool, user.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

// ============================================================================
// E-MAP-04: Attachment list excludes hidden files
// ============================================================================

/// The attachments folder may contain hidden metadata files; listing must not expose them.
/// This test verifies the underlying data model. Handler-level exclusion is documented
/// in the folder contents SQL (see `handlers/folders.rs`).
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_attachment_list_excludes_hidden_files_at_handler_level() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;
    let user = create_test_user(&ctx.metadata_store, "editor_user", tenant_id).await;
    let folder_service = ctx.folder_service();
    let file_service = ctx.file_service();

    let doc_folder = create_test_folder(
        &folder_service,
        user.id,
        tenant_id,
        "Project-Brainstorm",
        None,
    )
    .await;
    let attachments_folder = create_test_folder(
        &folder_service,
        user.id,
        tenant_id,
        "attachments",
        Some(doc_folder.id),
    )
    .await;

    // Upload visible attachment
    let _visible = file_service
        .upload_file(
            user.id,
            "diagram.png".to_string(),
            Some(attachments_folder.id),
            bytes::Bytes::from("fake image data"),
            "image/png".to_string(),
            tenant_id,
        )
        .await
        .expect("upload should succeed");

    // Upload hidden metadata file (simulating note.editor.json)
    let _hidden = file_service
        .upload_file(
            user.id,
            "note.editor.json".to_string(),
            Some(attachments_folder.id),
            bytes::Bytes::from("{}"),
            "application/json".to_string(),
            tenant_id,
        )
        .await
        .expect("upload of hidden file should succeed at service level");

    // Service-level list returns all files (no hidden exclusion)
    let all_files = ctx
        .metadata_store
        .list_files_by_parent(Some(attachments_folder.id), tenant_id)
        .await
        .expect("list files should succeed");

    assert_eq!(
        all_files.len(),
        2,
        "Metadata store returns all files including hidden"
    );

    // folder_service.list_contents filters hidden files (is_hidden_file)
    let contents = folder_service
        .list_contents(attachments_folder.id, user.id)
        .await
        .expect("list_contents should succeed");

    assert!(
        contents.files.iter().any(|f| f.name == "diagram.png"),
        "Visible attachment should appear in list_contents"
    );
    assert!(
        !contents.files.iter().any(|f| f.name == "note.editor.json"),
        "Hidden .editor.json should be excluded from list_contents"
    );

    // The handler layer (`get_folder_contents`) filters hidden files via SQL:
    //   AND f.name NOT LIKE '.rustshare-%'
    //   AND f.name NOT IN ('index.md', '__primary__.md')
    //   AND f.name NOT LIKE '%.editor.json'
    //
    // This test documents that contract; a handler-level integration test would
    // verify the HTTP response contains only "diagram.png".

    cleanup_user(&ctx.pool, user.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

// ============================================================================
// E-MAP-05: Attachment delete maps to file delete
// ============================================================================

/// Deleting an attachment is a standard file deletion.
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_attachment_delete_maps_to_file_delete() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;
    let user = create_test_user(&ctx.metadata_store, "editor_user", tenant_id).await;
    let folder_service = ctx.folder_service();
    let file_service = ctx.file_service();

    let doc_folder = create_test_folder(
        &folder_service,
        user.id,
        tenant_id,
        "Project-Brainstorm",
        None,
    )
    .await;
    let attachments_folder = create_test_folder(
        &folder_service,
        user.id,
        tenant_id,
        "attachments",
        Some(doc_folder.id),
    )
    .await;

    let attachment = file_service
        .upload_file(
            user.id,
            "brief.pdf".to_string(),
            Some(attachments_folder.id),
            bytes::Bytes::from("fake pdf data"),
            "application/pdf".to_string(),
            tenant_id,
        )
        .await
        .expect("upload should succeed");

    // Delete attachment → file delete
    file_service
        .delete_file(attachment.id, user.id)
        .await
        .expect("delete should succeed");

    // Verify file is gone (or trashed, depending on service implementation)
    let result = file_service.get_file(attachment.id, user.id).await;
    assert!(
        matches!(result, Err(FileError::NotFound { .. })),
        "Deleted attachment should not be found"
    );

    cleanup_user(&ctx.pool, user.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

// ============================================================================
// E-MAP-06: Permission enforcement on attachment operations
// ============================================================================

/// A user without write permission cannot upload into another user's attachments folder.
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_attachment_upload_denied_without_permission() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;
    let owner = create_test_user(&ctx.metadata_store, "owner", tenant_id).await;
    let intruder = create_test_user(&ctx.metadata_store, "intruder", tenant_id).await;
    let folder_service = ctx.folder_service();
    let file_service = ctx.file_service();

    let doc_folder = create_test_folder(
        &folder_service,
        owner.id,
        tenant_id,
        "Project-Brainstorm",
        None,
    )
    .await;
    let attachments_folder = create_test_folder(
        &folder_service,
        owner.id,
        tenant_id,
        "attachments",
        Some(doc_folder.id),
    )
    .await;

    // Intruder tries to upload into owner's attachments folder
    let result = file_service
        .upload_file(
            intruder.id,
            "evil.png".to_string(),
            Some(attachments_folder.id),
            bytes::Bytes::from("fake image data"),
            "image/png".to_string(),
            tenant_id,
        )
        .await;

    assert!(
        matches!(result, Err(FileError::PermissionDenied { .. })),
        "Upload without permission should be denied"
    );

    cleanup_user(&ctx.pool, owner.id).await;
    cleanup_user(&ctx.pool, intruder.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

// ============================================================================
// E-STUB: Deferred dedicated editor API routes
// ============================================================================

/// Stub for future `GET /api/editor/documents/{id}` route.
/// This test is pending until a dedicated editor service is justified.
#[tokio::test]
#[ignore = "Pending: dedicated editor API not yet implemented (deferred per ADR-0023)"]
async fn test_stub_editor_document_get_route() {
    // When implemented, this route should return composed document metadata,
    // Markdown content, attachments list, and permissions in a single response.
}

/// Stub for future `PUT /api/editor/documents/{id}/content` route.
/// This test is pending until a dedicated editor service is justified.
#[tokio::test]
#[ignore = "Pending: dedicated editor API not yet implemented (deferred per ADR-0023)"]
async fn test_stub_editor_document_save_route() {
    // When implemented, this route should accept Markdown + baseRevision,
    // perform automatic attachment sanitization, and return updated revision.
}

/// Stub for future `POST /api/editor/documents/{id}/attachments` route.
/// This test is pending until a dedicated editor service is justified.
#[tokio::test]
#[ignore = "Pending: dedicated editor API not yet implemented (deferred per ADR-0023)"]
async fn test_stub_editor_attachment_upload_route() {
    // When implemented, this route should accept a file, auto-sanitize the name,
    // store it in the document's attachments folder, and return attachment metadata.
}

/// Stub for future `GET /api/editor/documents/{id}/attachments` route.
/// This test is pending until a dedicated editor service is justified.
#[tokio::test]
#[ignore = "Pending: dedicated editor API not yet implemented (deferred per ADR-0023)"]
async fn test_stub_editor_attachment_list_route() {
    // When implemented, this route should return only visible attachments,
    // excluding hidden metadata files, with pre-signed download URLs if needed.
}

/// Stub for future `DELETE /api/editor/documents/{id}/attachments/{attachment_id}` route.
/// This test is pending until a dedicated editor service is justified.
#[tokio::test]
#[ignore = "Pending: dedicated editor API not yet implemented (deferred per ADR-0023)"]
async fn test_stub_editor_attachment_delete_route() {
    // When implemented, this route should delete the attachment file and
    // optionally remove references from the document Markdown.
}
