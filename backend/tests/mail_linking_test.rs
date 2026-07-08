//! Integration tests for linking imported mail artifacts to RustShare objects.

use rustshare_core::domain::{LinkTargetType, MailMessage, MailSourceMode};

mod contracts;
use contracts::common::{cleanup_tenant, cleanup_user, create_test_user, setup_test_env};

#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn create_mail_link_to_folder_happy_path() {
    let ctx = setup_test_env().await;
    let tenant_id = ctx.tenant_id;
    let user = create_test_user(&ctx.metadata_store, "mail_link_owner", tenant_id).await;

    let target_folder = ctx
        .create_test_folder(user.id, "Link Target Folder", None)
        .await;

    let mut message = MailMessage::new(tenant_id, user.id, user.id, MailSourceMode::EmlUpload);
    message.subject = Some("Link Test".to_string());
    ctx.metadata_store
        .create_mail_message(&message)
        .await
        .expect("failed to create mail message");

    let mail_service = ctx.mail_service();

    let link = mail_service
        .link_message(
            tenant_id,
            user.id,
            message.id,
            LinkTargetType::Folder,
            target_folder.id,
        )
        .await
        .expect("link_message should succeed");

    assert_eq!(link.message_id, message.id);
    assert_eq!(link.target_type, "folder");
    assert_eq!(link.target_id, target_folder.id);
    assert_eq!(link.created_by, user.id);

    cleanup_user(&ctx.pool, user.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn create_mail_link_is_idempotent() {
    let ctx = setup_test_env().await;
    let tenant_id = ctx.tenant_id;
    let user = create_test_user(&ctx.metadata_store, "mail_link_idempotent", tenant_id).await;

    let target_folder = ctx
        .create_test_folder(user.id, "Link Target Folder", None)
        .await;

    let mut message = MailMessage::new(tenant_id, user.id, user.id, MailSourceMode::EmlUpload);
    message.subject = Some("Idempotent Link Test".to_string());
    ctx.metadata_store
        .create_mail_message(&message)
        .await
        .expect("failed to create mail message");

    let mail_service = ctx.mail_service();

    let first = mail_service
        .link_message(
            tenant_id,
            user.id,
            message.id,
            LinkTargetType::Folder,
            target_folder.id,
        )
        .await
        .expect("first link_message should succeed");

    let second = mail_service
        .link_message(
            tenant_id,
            user.id,
            message.id,
            LinkTargetType::Folder,
            target_folder.id,
        )
        .await
        .expect("second link_message should succeed");

    assert_eq!(
        first.id, second.id,
        "duplicate link should return the same row"
    );

    cleanup_user(&ctx.pool, user.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn create_mail_link_denied_when_user_cannot_read_target() {
    let ctx = setup_test_env().await;
    let tenant_id = ctx.tenant_id;
    let owner = create_test_user(&ctx.metadata_store, "mail_link_owner", tenant_id).await;
    let other = create_test_user(&ctx.metadata_store, "mail_link_other", tenant_id).await;

    // Owner creates a folder that other user cannot read.
    let target_folder = ctx
        .create_test_folder(owner.id, "Private Target Folder", None)
        .await;

    // Other user creates a mail message and tries to link it to owner's folder.
    let mut message = MailMessage::new(tenant_id, other.id, other.id, MailSourceMode::EmlUpload);
    message.subject = Some("Unauthorized Link Test".to_string());
    ctx.metadata_store
        .create_mail_message(&message)
        .await
        .expect("failed to create mail message");

    let mail_service = ctx.mail_service();

    let err = mail_service
        .link_message(
            tenant_id,
            other.id,
            message.id,
            LinkTargetType::Folder,
            target_folder.id,
        )
        .await
        .expect_err("link_message should fail when caller cannot read target");

    assert!(
        matches!(
            err,
            rustshare_server::services::mail_service::MailError::PermissionDenied
        ),
        "expected PermissionDenied, got {:?}",
        err
    );

    cleanup_user(&ctx.pool, owner.id).await;
    cleanup_user(&ctx.pool, other.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn list_and_delete_mail_links() {
    let ctx = setup_test_env().await;
    let tenant_id = ctx.tenant_id;
    let user = create_test_user(&ctx.metadata_store, "mail_link_list", tenant_id).await;

    let target_folder = ctx
        .create_test_folder(user.id, "Link Target Folder", None)
        .await;

    let mut message = MailMessage::new(tenant_id, user.id, user.id, MailSourceMode::EmlUpload);
    message.subject = Some("List Link Test".to_string());
    ctx.metadata_store
        .create_mail_message(&message)
        .await
        .expect("failed to create mail message");

    let mail_service = ctx.mail_service();

    let link = mail_service
        .link_message(
            tenant_id,
            user.id,
            message.id,
            LinkTargetType::Folder,
            target_folder.id,
        )
        .await
        .expect("link_message should succeed");

    let links = mail_service
        .list_message_links(tenant_id, user.id, message.id)
        .await
        .expect("list_message_links should succeed");
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].id, link.id);

    mail_service
        .unlink_message(tenant_id, user.id, link.id)
        .await
        .expect("unlink_message should succeed");

    let links_after = mail_service
        .list_message_links(tenant_id, user.id, message.id)
        .await
        .expect("list_message_links should succeed after unlink");
    assert!(links_after.is_empty());

    // Idempotent: unlinking again should succeed without error.
    mail_service
        .unlink_message(tenant_id, user.id, link.id)
        .await
        .expect("repeated unlink_message should succeed");

    cleanup_user(&ctx.pool, user.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn create_mail_link_to_note_happy_path() {
    let ctx = setup_test_env().await;
    let tenant_id = ctx.tenant_id;
    let user = create_test_user(&ctx.metadata_store, "mail_link_note_owner", tenant_id).await;

    let note = ctx
        .note_service()
        .create_note(
            user.id,
            tenant_id,
            Some("Link Target Note".to_string()),
            None,
            None,
        )
        .await
        .expect("failed to create note");

    let mut message = MailMessage::new(tenant_id, user.id, user.id, MailSourceMode::EmlUpload);
    message.subject = Some("Note Link Test".to_string());
    ctx.metadata_store
        .create_mail_message(&message)
        .await
        .expect("failed to create mail message");

    let mail_service = ctx.mail_service();

    let link = mail_service
        .link_message(
            tenant_id,
            user.id,
            message.id,
            LinkTargetType::Note,
            note.id,
        )
        .await
        .expect("link_message should succeed for note target");

    assert_eq!(link.message_id, message.id);
    assert_eq!(link.target_type, "note");
    assert_eq!(link.target_id, note.id);
    assert_eq!(link.created_by, user.id);

    cleanup_user(&ctx.pool, user.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn find_mail_link_by_id_includes_soft_deleted_links() {
    let ctx = setup_test_env().await;
    let tenant_id = ctx.tenant_id;
    let user = create_test_user(&ctx.metadata_store, "mail_link_soft_delete", tenant_id).await;

    let target_folder = ctx
        .create_test_folder(user.id, "Link Target Folder", None)
        .await;

    let mut message = MailMessage::new(tenant_id, user.id, user.id, MailSourceMode::EmlUpload);
    message.subject = Some("Soft Delete Link Test".to_string());
    ctx.metadata_store
        .create_mail_message(&message)
        .await
        .expect("failed to create mail message");

    let mail_service = ctx.mail_service();

    let link = mail_service
        .link_message(
            tenant_id,
            user.id,
            message.id,
            LinkTargetType::Folder,
            target_folder.id,
        )
        .await
        .expect("link_message should succeed");

    mail_service
        .unlink_message(tenant_id, user.id, link.id)
        .await
        .expect("unlink_message should succeed");

    // find_mail_link_by_id must include soft-deleted rows so DELETE retries
    // can validate URL ownership without returning 404.
    let found = mail_service
        .find_mail_link_by_id(tenant_id, user.id, link.id)
        .await
        .expect("find_mail_link_by_id should return the soft-deleted link");
    assert_eq!(found.id, link.id);
    assert!(found.deleted_at.is_some());

    cleanup_user(&ctx.pool, user.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn create_mail_link_after_soft_delete_gets_new_id() {
    let ctx = setup_test_env().await;
    let tenant_id = ctx.tenant_id;
    let user = create_test_user(&ctx.metadata_store, "mail_link_relink", tenant_id).await;

    let target_folder = ctx
        .create_test_folder(user.id, "Link Target Folder", None)
        .await;

    let mut message = MailMessage::new(tenant_id, user.id, user.id, MailSourceMode::EmlUpload);
    message.subject = Some("Relink Test".to_string());
    ctx.metadata_store
        .create_mail_message(&message)
        .await
        .expect("failed to create mail message");

    let mail_service = ctx.mail_service();

    let first = mail_service
        .link_message(
            tenant_id,
            user.id,
            message.id,
            LinkTargetType::Folder,
            target_folder.id,
        )
        .await
        .expect("first link_message should succeed");

    mail_service
        .unlink_message(tenant_id, user.id, first.id)
        .await
        .expect("unlink_message should succeed");

    let second = mail_service
        .link_message(
            tenant_id,
            user.id,
            message.id,
            LinkTargetType::Folder,
            target_folder.id,
        )
        .await
        .expect("re-linking after soft delete should succeed");

    assert_ne!(
        first.id, second.id,
        "re-linking after soft delete should create a new row"
    );

    cleanup_user(&ctx.pool, user.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}
