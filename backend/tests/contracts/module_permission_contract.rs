//! Module Tenant and Permission Contract Tests (LB-02)
//!
//! Negative contract tests for module object routes across tenant and share boundaries.
//! These tests document the desired contract: module data must not leak across tenants
//! and must not be accessible by unauthorized users within the same tenant.

use crate::common::*;
use rustshare_core::events::EventBroadcaster;
use rustshare_core::services::{FileService, FolderService, PermissionResolver};
use rustshare_infrastructure::repositories::PermissionResolverRepository;
use rustshare_server::services::brainstorming_service::{BrainstormError, BrainstormingService};
use rustshare_server::services::decision_service::{DecisionError, DecisionService};
use rustshare_server::services::kanban_service::{
    CreateBoardInput, CreateCardInput, KanbanError, KanbanService, MoveCardInput,
};
use rustshare_server::services::meeting_service::{MeetingError, MeetingService};
use rustshare_server::services::note_service::{NoteError, NoteService};
use rustshare_server::services::standup_service::{StandupError, StandupService};
use rustshare_storage::{EventStore, MetadataStore, ObjectStore};
use std::sync::Arc;
use uuid::Uuid;

fn create_file_service(
    event_store: Arc<EventStore>,
    metadata_store: Arc<MetadataStore>,
    object_store: Arc<ObjectStore>,
    pool: &sqlx::PgPool,
) -> FileService<EventStore, MetadataStore, ObjectStore, PermissionResolverRepository> {
    let broadcaster = Arc::new(EventBroadcaster::new(100));
    let permission_resolver = Arc::new(PermissionResolver::new(Arc::new(
        PermissionResolverRepository::new(pool.clone()),
    )));
    FileService::new(
        event_store,
        metadata_store,
        object_store,
        broadcaster,
        permission_resolver,
    )
}

fn create_folder_service(
    event_store: Arc<EventStore>,
    metadata_store: Arc<MetadataStore>,
    pool: &sqlx::PgPool,
) -> FolderService<EventStore, MetadataStore, PermissionResolverRepository> {
    let broadcaster = Arc::new(EventBroadcaster::new(100));
    let permission_resolver = Arc::new(PermissionResolver::new(Arc::new(
        PermissionResolverRepository::new(pool.clone()),
    )));
    FolderService::new(
        event_store,
        metadata_store,
        broadcaster,
        permission_resolver,
    )
}

fn create_note_service(
    event_store: Arc<EventStore>,
    metadata_store: Arc<MetadataStore>,
    object_store: Arc<ObjectStore>,
    pool: &sqlx::PgPool,
) -> Arc<NoteService> {
    let file_service = Arc::new(create_file_service(
        event_store.clone(),
        metadata_store.clone(),
        object_store.clone(),
        pool,
    ));
    let folder_service = Arc::new(create_folder_service(
        event_store.clone(),
        metadata_store.clone(),
        pool,
    ));
    Arc::new(NoteService::new(
        file_service,
        folder_service,
        metadata_store,
        object_store,
    ))
}

fn create_decision_service(
    event_store: Arc<EventStore>,
    metadata_store: Arc<MetadataStore>,
    object_store: Arc<ObjectStore>,
    pool: &sqlx::PgPool,
) -> Arc<DecisionService> {
    let file_service = Arc::new(create_file_service(
        event_store.clone(),
        metadata_store.clone(),
        object_store.clone(),
        pool,
    ));
    let folder_service = Arc::new(create_folder_service(
        event_store.clone(),
        metadata_store.clone(),
        pool,
    ));
    Arc::new(DecisionService::new(
        file_service,
        folder_service,
        metadata_store,
        object_store,
    ))
}

fn create_kanban_service(
    event_store: Arc<EventStore>,
    metadata_store: Arc<MetadataStore>,
    object_store: Arc<ObjectStore>,
    pool: &sqlx::PgPool,
) -> Arc<KanbanService> {
    let file_service = Arc::new(create_file_service(
        event_store.clone(),
        metadata_store.clone(),
        object_store.clone(),
        pool,
    ));
    let folder_service = Arc::new(create_folder_service(
        event_store.clone(),
        metadata_store.clone(),
        pool,
    ));
    let user_repository = Arc::new(
        rustshare_infrastructure::repositories::UserRepository::new(pool.clone()),
    );
    Arc::new(KanbanService::new(
        file_service,
        folder_service,
        metadata_store,
        object_store,
        user_repository,
    ))
}

fn create_meeting_service(
    event_store: Arc<EventStore>,
    metadata_store: Arc<MetadataStore>,
    object_store: Arc<ObjectStore>,
    pool: &sqlx::PgPool,
) -> Arc<MeetingService> {
    let file_service = Arc::new(create_file_service(
        event_store.clone(),
        metadata_store.clone(),
        object_store.clone(),
        pool,
    ));
    let folder_service = Arc::new(create_folder_service(
        event_store.clone(),
        metadata_store.clone(),
        pool,
    ));
    Arc::new(MeetingService::new(
        file_service,
        folder_service,
        metadata_store,
        object_store,
    ))
}

fn create_standup_service(
    event_store: Arc<EventStore>,
    metadata_store: Arc<MetadataStore>,
    object_store: Arc<ObjectStore>,
    pool: &sqlx::PgPool,
) -> Arc<StandupService> {
    let file_service = Arc::new(create_file_service(
        event_store.clone(),
        metadata_store.clone(),
        object_store.clone(),
        pool,
    ));
    let folder_service = Arc::new(create_folder_service(
        event_store.clone(),
        metadata_store.clone(),
        pool,
    ));
    Arc::new(StandupService::new(
        file_service,
        folder_service,
        metadata_store,
        object_store,
    ))
}

fn create_brainstorming_service(
    event_store: Arc<EventStore>,
    metadata_store: Arc<MetadataStore>,
    object_store: Arc<ObjectStore>,
    pool: &sqlx::PgPool,
) -> Arc<BrainstormingService> {
    let file_service = Arc::new(create_file_service(
        event_store.clone(),
        metadata_store.clone(),
        object_store.clone(),
        pool,
    ));
    let folder_service = Arc::new(create_folder_service(
        event_store.clone(),
        metadata_store.clone(),
        pool,
    ));
    Arc::new(BrainstormingService::new(
        file_service,
        folder_service,
        metadata_store,
        object_store,
    ))
}

// ============================================================================
// Notes
// ============================================================================

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_notes_cross_tenant_get_denied() {
    let ctx = setup_test_env().await;
    let tenant_a = setup_test_tenant(&ctx.pool).await;
    let tenant_b = setup_test_tenant(&ctx.pool).await;
    let user_a = create_test_user(&ctx.metadata_store, "note_ct_a", tenant_a).await;
    let user_b = create_test_user(&ctx.metadata_store, "note_ct_b", tenant_b).await;
    let service = create_note_service(
        ctx.event_store.clone(),
        ctx.metadata_store.clone(),
        ctx.object_store.clone(),
        &ctx.pool,
    );

    let note = service
        .create_note(user_a.id, tenant_a, Some("Secret".to_string()), None, Some("content".to_string()))
        .await
        .unwrap();

    let result = service.get_note(note.id, user_b.id, tenant_b).await;
    assert!(
        matches!(result, Err(NoteError::PermissionDenied)),
        "Cross-tenant get_note should be denied, got {:?}",
        result
    );

    cleanup_user(&ctx.pool, user_a.id).await;
    cleanup_user(&ctx.pool, user_b.id).await;
    cleanup_tenant(&ctx.pool, tenant_a).await;
    cleanup_tenant(&ctx.pool, tenant_b).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_notes_cross_tenant_save_denied() {
    let ctx = setup_test_env().await;
    let tenant_a = setup_test_tenant(&ctx.pool).await;
    let tenant_b = setup_test_tenant(&ctx.pool).await;
    let user_a = create_test_user(&ctx.metadata_store, "note_ct_a2", tenant_a).await;
    let user_b = create_test_user(&ctx.metadata_store, "note_ct_b2", tenant_b).await;
    let service = create_note_service(
        ctx.event_store.clone(),
        ctx.metadata_store.clone(),
        ctx.object_store.clone(),
        &ctx.pool,
    );

    let note = service
        .create_note(user_a.id, tenant_a, Some("Secret".to_string()), None, Some("content".to_string()))
        .await
        .unwrap();

    let result = service.save_note(note.id, user_b.id, tenant_b, "hacked".to_string(), None, None).await;
    assert!(
        matches!(result, Err(NoteError::PermissionDenied)),
        "Cross-tenant save_note should be denied, got {:?}",
        result
    );

    cleanup_user(&ctx.pool, user_a.id).await;
    cleanup_user(&ctx.pool, user_b.id).await;
    cleanup_tenant(&ctx.pool, tenant_a).await;
    cleanup_tenant(&ctx.pool, tenant_b).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_notes_cross_tenant_delete_denied() {
    let ctx = setup_test_env().await;
    let tenant_a = setup_test_tenant(&ctx.pool).await;
    let tenant_b = setup_test_tenant(&ctx.pool).await;
    let user_a = create_test_user(&ctx.metadata_store, "note_ct_a3", tenant_a).await;
    let user_b = create_test_user(&ctx.metadata_store, "note_ct_b3", tenant_b).await;
    let service = create_note_service(
        ctx.event_store.clone(),
        ctx.metadata_store.clone(),
        ctx.object_store.clone(),
        &ctx.pool,
    );

    let note = service
        .create_note(user_a.id, tenant_a, Some("Secret".to_string()), None, Some("content".to_string()))
        .await
        .unwrap();

    let result = service.delete_note(note.id, user_b.id, tenant_b).await;
    assert!(
        matches!(result, Err(NoteError::PermissionDenied)),
        "Cross-tenant delete_note should be denied, got {:?}",
        result
    );

    cleanup_user(&ctx.pool, user_a.id).await;
    cleanup_user(&ctx.pool, user_b.id).await;
    cleanup_tenant(&ctx.pool, tenant_a).await;
    cleanup_tenant(&ctx.pool, tenant_b).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_notes_cross_tenant_list_does_not_leak() {
    let ctx = setup_test_env().await;
    let tenant_a = setup_test_tenant(&ctx.pool).await;
    let tenant_b = setup_test_tenant(&ctx.pool).await;
    let user_a = create_test_user(&ctx.metadata_store, "note_ct_a4", tenant_a).await;
    let user_b = create_test_user(&ctx.metadata_store, "note_ct_b4", tenant_b).await;
    let service = create_note_service(
        ctx.event_store.clone(),
        ctx.metadata_store.clone(),
        ctx.object_store.clone(),
        &ctx.pool,
    );

    let _note = service
        .create_note(user_a.id, tenant_a, Some("Secret".to_string()), None, Some("content".to_string()))
        .await
        .unwrap();

    let list_b = service.list_notes(user_b.id, tenant_b, Some(10)).await.unwrap();
    assert!(
        !list_b.iter().any(|n| n.metadata.title == "Secret"),
        "Cross-tenant list_notes should not leak notes"
    );

    cleanup_user(&ctx.pool, user_a.id).await;
    cleanup_user(&ctx.pool, user_b.id).await;
    cleanup_tenant(&ctx.pool, tenant_a).await;
    cleanup_tenant(&ctx.pool, tenant_b).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_notes_same_tenant_unauthorized_denied() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;
    let user_owner = create_test_user(&ctx.metadata_store, "note_owner", tenant_id).await;
    let user_other = create_test_user(&ctx.metadata_store, "note_other", tenant_id).await;
    let service = create_note_service(
        ctx.event_store.clone(),
        ctx.metadata_store.clone(),
        ctx.object_store.clone(),
        &ctx.pool,
    );

    let note = service
        .create_note(user_owner.id, tenant_id, Some("Private".to_string()), None, Some("content".to_string()))
        .await
        .unwrap();

    let result = service.get_note(note.id, user_other.id, tenant_id).await;
    assert!(
        matches!(result, Err(NoteError::PermissionDenied)),
        "Same-tenant unauthorized get_note should be denied, got {:?}",
        result
    );

    cleanup_user(&ctx.pool, user_owner.id).await;
    cleanup_user(&ctx.pool, user_other.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_notes_shared_resource_only_through_intended_path() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;
    let user = create_test_user(&ctx.metadata_store, "note_share_user", tenant_id).await;
    let service = create_note_service(
        ctx.event_store.clone(),
        ctx.metadata_store.clone(),
        ctx.object_store.clone(),
        &ctx.pool,
    );

    let note = service
        .create_note(user.id, tenant_id, Some("Shared".to_string()), None, Some("secret".to_string()))
        .await
        .unwrap();

    // Private note should not be accessible via public route
    let result = service.get_public_note("nonexistentshareid12345678901234").await;
    assert!(result.is_err(), "Random share_id should not access private note");

    // Make public and verify access works through intended path
    let public = service.toggle_visibility(note.id, user.id, tenant_id).await.unwrap();
    let share_id = public.metadata.public_share_id.unwrap();
    let public_note = service.get_public_note(&share_id).await.unwrap();
    assert_eq!(public_note.title, "Shared");

    // Revoke and verify denial
    let _ = service.toggle_visibility(note.id, user.id, tenant_id).await.unwrap();
    let result = service.get_public_note(&share_id).await;
    assert!(result.is_err(), "Revoked public note should deny access");

    cleanup_user(&ctx.pool, user.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

// ============================================================================
// Decisions
// ============================================================================

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_decisions_cross_tenant_get_denied() {
    let ctx = setup_test_env().await;
    let tenant_a = setup_test_tenant(&ctx.pool).await;
    let tenant_b = setup_test_tenant(&ctx.pool).await;
    let user_a = create_test_user(&ctx.metadata_store, "decision_ct_a", tenant_a).await;
    let user_b = create_test_user(&ctx.metadata_store, "decision_ct_b", tenant_b).await;
    let service = create_decision_service(
        ctx.event_store.clone(),
        ctx.metadata_store.clone(),
        ctx.object_store.clone(),
        &ctx.pool,
    );

    let decision = service
        .create_decision(user_a.id, tenant_a, "Secret".to_string(), "Exec".to_string(), "content".to_string())
        .await
        .unwrap();

    let result = service.get_decision(decision.id, user_b.id, tenant_b).await;
    assert!(
        matches!(result, Err(DecisionError::PermissionDenied)),
        "Cross-tenant get_decision should be denied, got {:?}",
        result
    );

    cleanup_user(&ctx.pool, user_a.id).await;
    cleanup_user(&ctx.pool, user_b.id).await;
    cleanup_tenant(&ctx.pool, tenant_a).await;
    cleanup_tenant(&ctx.pool, tenant_b).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_decisions_cross_tenant_update_denied() {
    let ctx = setup_test_env().await;
    let tenant_a = setup_test_tenant(&ctx.pool).await;
    let tenant_b = setup_test_tenant(&ctx.pool).await;
    let user_a = create_test_user(&ctx.metadata_store, "decision_ct_a2", tenant_a).await;
    let user_b = create_test_user(&ctx.metadata_store, "decision_ct_b2", tenant_b).await;
    let service = create_decision_service(
        ctx.event_store.clone(),
        ctx.metadata_store.clone(),
        ctx.object_store.clone(),
        &ctx.pool,
    );

    let decision = service
        .create_decision(user_a.id, tenant_a, "Secret".to_string(), "Exec".to_string(), "content".to_string())
        .await
        .unwrap();

    let result = service
        .update_decision(decision.id, user_b.id, tenant_b, Some("Hacked".to_string()), None, Some("evil".to_string()))
        .await;
    assert!(
        matches!(result, Err(DecisionError::PermissionDenied)),
        "Cross-tenant update_decision should be denied, got {:?}",
        result
    );

    cleanup_user(&ctx.pool, user_a.id).await;
    cleanup_user(&ctx.pool, user_b.id).await;
    cleanup_tenant(&ctx.pool, tenant_a).await;
    cleanup_tenant(&ctx.pool, tenant_b).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_decisions_cross_tenant_rename_denied() {
    let ctx = setup_test_env().await;
    let tenant_a = setup_test_tenant(&ctx.pool).await;
    let tenant_b = setup_test_tenant(&ctx.pool).await;
    let user_a = create_test_user(&ctx.metadata_store, "decision_ct_a3", tenant_a).await;
    let user_b = create_test_user(&ctx.metadata_store, "decision_ct_b3", tenant_b).await;
    let service = create_decision_service(
        ctx.event_store.clone(),
        ctx.metadata_store.clone(),
        ctx.object_store.clone(),
        &ctx.pool,
    );

    let decision = service
        .create_decision(user_a.id, tenant_a, "Secret".to_string(), "Exec".to_string(), "content".to_string())
        .await
        .unwrap();

    let result = service.rename_decision(decision.id, user_b.id, tenant_b, "Hacked".to_string()).await;
    assert!(
        matches!(result, Err(DecisionError::PermissionDenied)),
        "Cross-tenant rename_decision should be denied, got {:?}",
        result
    );

    cleanup_user(&ctx.pool, user_a.id).await;
    cleanup_user(&ctx.pool, user_b.id).await;
    cleanup_tenant(&ctx.pool, tenant_a).await;
    cleanup_tenant(&ctx.pool, tenant_b).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_decisions_cross_tenant_list_does_not_leak() {
    let ctx = setup_test_env().await;
    let tenant_a = setup_test_tenant(&ctx.pool).await;
    let tenant_b = setup_test_tenant(&ctx.pool).await;
    let user_a = create_test_user(&ctx.metadata_store, "decision_ct_a4", tenant_a).await;
    let user_b = create_test_user(&ctx.metadata_store, "decision_ct_b4", tenant_b).await;
    let service = create_decision_service(
        ctx.event_store.clone(),
        ctx.metadata_store.clone(),
        ctx.object_store.clone(),
        &ctx.pool,
    );

    let _decision = service
        .create_decision(user_a.id, tenant_a, "Secret".to_string(), "Exec".to_string(), "content".to_string())
        .await
        .unwrap();

    let list_b = service.list_decisions(user_b.id, tenant_b).await.unwrap();
    assert!(
        !list_b.iter().any(|d| d.metadata.title == "Secret"),
        "Cross-tenant list_decisions should not leak decisions"
    );

    cleanup_user(&ctx.pool, user_a.id).await;
    cleanup_user(&ctx.pool, user_b.id).await;
    cleanup_tenant(&ctx.pool, tenant_a).await;
    cleanup_tenant(&ctx.pool, tenant_b).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_decisions_same_tenant_unauthorized_denied() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;
    let user_owner = create_test_user(&ctx.metadata_store, "decision_owner", tenant_id).await;
    let user_other = create_test_user(&ctx.metadata_store, "decision_other", tenant_id).await;
    let service = create_decision_service(
        ctx.event_store.clone(),
        ctx.metadata_store.clone(),
        ctx.object_store.clone(),
        &ctx.pool,
    );

    let decision = service
        .create_decision(user_owner.id, tenant_id, "Private".to_string(), "Exec".to_string(), "content".to_string())
        .await
        .unwrap();

    let result = service.get_decision(decision.id, user_other.id, tenant_id).await;
    assert!(
        matches!(result, Err(DecisionError::PermissionDenied)),
        "Same-tenant unauthorized get_decision should be denied, got {:?}",
        result
    );

    cleanup_user(&ctx.pool, user_owner.id).await;
    cleanup_user(&ctx.pool, user_other.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

// ============================================================================
// Kanban
// ============================================================================

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_kanban_cross_tenant_get_board_denied() {
    let ctx = setup_test_env().await;
    let tenant_a = setup_test_tenant(&ctx.pool).await;
    let tenant_b = setup_test_tenant(&ctx.pool).await;
    let user_a = create_test_user(&ctx.metadata_store, "kanban_ct_a", tenant_a).await;
    let user_b = create_test_user(&ctx.metadata_store, "kanban_ct_b", tenant_b).await;
    let service = create_kanban_service(
        ctx.event_store.clone(),
        ctx.metadata_store.clone(),
        ctx.object_store.clone(),
        &ctx.pool,
    );

    let board = service
        .create_board(CreateBoardInput { title: "Secret".to_string() }, user_a.id, tenant_a)
        .await
        .unwrap();

    let result = service.get_board(board.id.clone(), user_b.id, tenant_b).await;
    assert!(
        matches!(result, Err(KanbanError::PermissionDenied)),
        "Cross-tenant get_board should be denied, got {:?}",
        result
    );

    cleanup_user(&ctx.pool, user_a.id).await;
    cleanup_user(&ctx.pool, user_b.id).await;
    cleanup_tenant(&ctx.pool, tenant_a).await;
    cleanup_tenant(&ctx.pool, tenant_b).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_kanban_cross_tenant_update_board_denied() {
    let ctx = setup_test_env().await;
    let tenant_a = setup_test_tenant(&ctx.pool).await;
    let tenant_b = setup_test_tenant(&ctx.pool).await;
    let user_a = create_test_user(&ctx.metadata_store, "kanban_ct_a2", tenant_a).await;
    let user_b = create_test_user(&ctx.metadata_store, "kanban_ct_b2", tenant_b).await;
    let service = create_kanban_service(
        ctx.event_store.clone(),
        ctx.metadata_store.clone(),
        ctx.object_store.clone(),
        &ctx.pool,
    );

    let board = service
        .create_board(CreateBoardInput { title: "Secret".to_string() }, user_a.id, tenant_a)
        .await
        .unwrap();

    let result = service
        .update_board(
            board.id.clone(),
            rustshare_server::services::kanban_service::UpdateBoardInput {
                title: Some("Hacked".to_string()),
                labels: None,
                settings: None,
                archived: None,
            },
            user_b.id,
            tenant_b,
        )
        .await;
    assert!(
        matches!(result, Err(KanbanError::PermissionDenied)),
        "Cross-tenant update_board should be denied, got {:?}",
        result
    );

    cleanup_user(&ctx.pool, user_a.id).await;
    cleanup_user(&ctx.pool, user_b.id).await;
    cleanup_tenant(&ctx.pool, tenant_a).await;
    cleanup_tenant(&ctx.pool, tenant_b).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_kanban_cross_tenant_delete_card_denied() {
    let ctx = setup_test_env().await;
    let tenant_a = setup_test_tenant(&ctx.pool).await;
    let tenant_b = setup_test_tenant(&ctx.pool).await;
    let user_a = create_test_user(&ctx.metadata_store, "kanban_ct_a3", tenant_a).await;
    let user_b = create_test_user(&ctx.metadata_store, "kanban_ct_b3", tenant_b).await;
    let service = create_kanban_service(
        ctx.event_store.clone(),
        ctx.metadata_store.clone(),
        ctx.object_store.clone(),
        &ctx.pool,
    );

    let board = service
        .create_board(CreateBoardInput { title: "Secret".to_string() }, user_a.id, tenant_a)
        .await
        .unwrap();
    let backlog = board.columns.iter().find(|c| c.slug == "00-Backlog").unwrap();
    let card = service
        .create_card(
            board.id.clone(),
            CreateCardInput {
                title: "Card".to_string(),
                column_id: Some(backlog.id.clone()),
                content: None,
                priority: None,
                labels: None,
                assignees: None,
                due_date: None,
            },
            user_a.id,
            tenant_a,
        )
        .await
        .unwrap();

    let result = service.delete_card(card.id.parse().unwrap(), user_b.id, tenant_b).await;
    assert!(
        matches!(result, Err(KanbanError::PermissionDenied)),
        "Cross-tenant delete_card should be denied, got {:?}",
        result
    );

    cleanup_user(&ctx.pool, user_a.id).await;
    cleanup_user(&ctx.pool, user_b.id).await;
    cleanup_tenant(&ctx.pool, tenant_a).await;
    cleanup_tenant(&ctx.pool, tenant_b).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_kanban_cross_tenant_list_boards_does_not_leak() {
    let ctx = setup_test_env().await;
    let tenant_a = setup_test_tenant(&ctx.pool).await;
    let tenant_b = setup_test_tenant(&ctx.pool).await;
    let user_a = create_test_user(&ctx.metadata_store, "kanban_ct_a4", tenant_a).await;
    let user_b = create_test_user(&ctx.metadata_store, "kanban_ct_b4", tenant_b).await;
    let service = create_kanban_service(
        ctx.event_store.clone(),
        ctx.metadata_store.clone(),
        ctx.object_store.clone(),
        &ctx.pool,
    );

    let _board = service
        .create_board(CreateBoardInput { title: "Secret".to_string() }, user_a.id, tenant_a)
        .await
        .unwrap();

    let list_b = service.list_boards(user_b.id, tenant_b).await.unwrap();
    assert!(
        !list_b.iter().any(|b| b.title == "Secret"),
        "Cross-tenant list_boards should not leak boards"
    );

    cleanup_user(&ctx.pool, user_a.id).await;
    cleanup_user(&ctx.pool, user_b.id).await;
    cleanup_tenant(&ctx.pool, tenant_a).await;
    cleanup_tenant(&ctx.pool, tenant_b).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_kanban_same_tenant_unauthorized_get_board_denied() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;
    let user_owner = create_test_user(&ctx.metadata_store, "kanban_owner", tenant_id).await;
    let user_other = create_test_user(&ctx.metadata_store, "kanban_other", tenant_id).await;
    let service = create_kanban_service(
        ctx.event_store.clone(),
        ctx.metadata_store.clone(),
        ctx.object_store.clone(),
        &ctx.pool,
    );

    let board = service
        .create_board(CreateBoardInput { title: "Private".to_string() }, user_owner.id, tenant_id)
        .await
        .unwrap();

    let result = service.get_board(board.id.clone(), user_other.id, tenant_id).await;
    assert!(
        matches!(result, Err(KanbanError::PermissionDenied)),
        "Same-tenant unauthorized get_board should be denied, got {:?}",
        result
    );

    cleanup_user(&ctx.pool, user_owner.id).await;
    cleanup_user(&ctx.pool, user_other.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_kanban_same_tenant_unauthorized_get_card_detail_denied() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;
    let user_owner = create_test_user(&ctx.metadata_store, "kanban_owner2", tenant_id).await;
    let user_other = create_test_user(&ctx.metadata_store, "kanban_other2", tenant_id).await;
    let service = create_kanban_service(
        ctx.event_store.clone(),
        ctx.metadata_store.clone(),
        ctx.object_store.clone(),
        &ctx.pool,
    );

    let board = service
        .create_board(CreateBoardInput { title: "Private".to_string() }, user_owner.id, tenant_id)
        .await
        .unwrap();
    let backlog = board.columns.iter().find(|c| c.slug == "00-Backlog").unwrap();
    let card = service
        .create_card(
            board.id.clone(),
            CreateCardInput {
                title: "Card".to_string(),
                column_id: Some(backlog.id.clone()),
                content: Some("detail".to_string()),
                priority: None,
                labels: None,
                assignees: None,
                due_date: None,
            },
            user_owner.id,
            tenant_id,
        )
        .await
        .unwrap();

    let result = service.get_card_detail(card.id.parse().unwrap(), user_other.id).await;
    assert!(
        matches!(result, Err(KanbanError::PermissionDenied)),
        "Same-tenant unauthorized get_card_detail should be denied, got {:?}",
        result
    );

    cleanup_user(&ctx.pool, user_owner.id).await;
    cleanup_user(&ctx.pool, user_other.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

// ============================================================================
// Brainstorming
// ============================================================================

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_brainstorming_cross_tenant_get_board_denied() {
    let ctx = setup_test_env().await;
    let tenant_a = setup_test_tenant(&ctx.pool).await;
    let tenant_b = setup_test_tenant(&ctx.pool).await;
    let user_a = create_test_user(&ctx.metadata_store, "bs_ct_a", tenant_a).await;
    let user_b = create_test_user(&ctx.metadata_store, "bs_ct_b", tenant_b).await;
    let service = create_brainstorming_service(
        ctx.event_store.clone(),
        ctx.metadata_store.clone(),
        ctx.object_store.clone(),
        &ctx.pool,
    );

    let root = service.ensure_brainstorming_root(user_a.id, tenant_a).await.unwrap();
    let board_folder = create_test_folder(&ctx.folder_service(), user_a.id, tenant_a, "secret-board", Some(root.id)).await;
    create_test_file(
        &ctx.file_service(),
        user_a.id,
        tenant_a,
        Some(board_folder.id),
        "board.excalidraw",
        b"{\"type\":\"excalidraw\",\"version\":2,\"elements\":[]}",
    )
    .await;
    let meta = r#"{"id":"s1","type":"brainstorming.board","title":"Secret","slug":"secret-board","template":"template_blank_brainstorm","sourceFile":"board.excalidraw","previewFile":"preview.png","createdAt":"2026-04-30T00:00:00Z","updatedAt":"2026-04-30T00:00:00Z","schemaVersion":"1.0"}"#;
    create_test_file(
        &ctx.file_service(),
        user_a.id,
        tenant_a,
        Some(board_folder.id),
        ".rustshare.json",
        meta.as_bytes(),
    )
    .await;

    let result = service.get_board(board_folder.id, user_b.id, tenant_b).await;
    assert!(
        matches!(result, Err(BrainstormError::PermissionDenied)),
        "Cross-tenant get_board should be denied, got {:?}",
        result
    );

    cleanup_user(&ctx.pool, user_a.id).await;
    cleanup_user(&ctx.pool, user_b.id).await;
    cleanup_tenant(&ctx.pool, tenant_a).await;
    cleanup_tenant(&ctx.pool, tenant_b).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_brainstorming_cross_tenant_save_source_denied() {
    let ctx = setup_test_env().await;
    let tenant_a = setup_test_tenant(&ctx.pool).await;
    let tenant_b = setup_test_tenant(&ctx.pool).await;
    let user_a = create_test_user(&ctx.metadata_store, "bs_ct_a2", tenant_a).await;
    let user_b = create_test_user(&ctx.metadata_store, "bs_ct_b2", tenant_b).await;
    let service = create_brainstorming_service(
        ctx.event_store.clone(),
        ctx.metadata_store.clone(),
        ctx.object_store.clone(),
        &ctx.pool,
    );

    let root = service.ensure_brainstorming_root(user_a.id, tenant_a).await.unwrap();
    let board_folder = create_test_folder(&ctx.folder_service(), user_a.id, tenant_a, "secret-board", Some(root.id)).await;
    create_test_file(
        &ctx.file_service(),
        user_a.id,
        tenant_a,
        Some(board_folder.id),
        "board.excalidraw",
        b"{\"type\":\"excalidraw\",\"version\":2,\"elements\":[]}",
    )
    .await;
    let meta = r#"{"id":"s1","type":"brainstorming.board","title":"Secret","slug":"secret-board","template":"template_blank_brainstorm","sourceFile":"board.excalidraw","previewFile":"preview.png","createdAt":"2026-04-30T00:00:00Z","updatedAt":"2026-04-30T00:00:00Z","schemaVersion":"1.0"}"#;
    create_test_file(
        &ctx.file_service(),
        user_a.id,
        tenant_a,
        Some(board_folder.id),
        ".rustshare.json",
        meta.as_bytes(),
    )
    .await;

    let result = service
        .save_board_source(board_folder.id, user_b.id, tenant_b, r#"{"type":"excalidraw","version":2,"elements":[]}"#.to_string())
        .await;
    assert!(
        matches!(result, Err(BrainstormError::PermissionDenied)),
        "Cross-tenant save_board_source should be denied, got {:?}",
        result
    );

    cleanup_user(&ctx.pool, user_a.id).await;
    cleanup_user(&ctx.pool, user_b.id).await;
    cleanup_tenant(&ctx.pool, tenant_a).await;
    cleanup_tenant(&ctx.pool, tenant_b).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_brainstorming_cross_tenant_delete_board_denied() {
    let ctx = setup_test_env().await;
    let tenant_a = setup_test_tenant(&ctx.pool).await;
    let tenant_b = setup_test_tenant(&ctx.pool).await;
    let user_a = create_test_user(&ctx.metadata_store, "bs_ct_a3", tenant_a).await;
    let user_b = create_test_user(&ctx.metadata_store, "bs_ct_b3", tenant_b).await;
    let service = create_brainstorming_service(
        ctx.event_store.clone(),
        ctx.metadata_store.clone(),
        ctx.object_store.clone(),
        &ctx.pool,
    );

    let root = service.ensure_brainstorming_root(user_a.id, tenant_a).await.unwrap();
    let board_folder = create_test_folder(&ctx.folder_service(), user_a.id, tenant_a, "secret-board", Some(root.id)).await;
    create_test_file(
        &ctx.file_service(),
        user_a.id,
        tenant_a,
        Some(board_folder.id),
        "board.excalidraw",
        b"{\"type\":\"excalidraw\",\"version\":2,\"elements\":[]}",
    )
    .await;
    let meta = r#"{"id":"s1","type":"brainstorming.board","title":"Secret","slug":"secret-board","template":"template_blank_brainstorm","sourceFile":"board.excalidraw","previewFile":"preview.png","createdAt":"2026-04-30T00:00:00Z","updatedAt":"2026-04-30T00:00:00Z","schemaVersion":"1.0"}"#;
    create_test_file(
        &ctx.file_service(),
        user_a.id,
        tenant_a,
        Some(board_folder.id),
        ".rustshare.json",
        meta.as_bytes(),
    )
    .await;

    let result = service.delete_board(board_folder.id, user_b.id, tenant_b).await;
    assert!(
        matches!(result, Err(BrainstormError::PermissionDenied)),
        "Cross-tenant delete_board should be denied, got {:?}",
        result
    );

    cleanup_user(&ctx.pool, user_a.id).await;
    cleanup_user(&ctx.pool, user_b.id).await;
    cleanup_tenant(&ctx.pool, tenant_a).await;
    cleanup_tenant(&ctx.pool, tenant_b).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_brainstorming_cross_tenant_list_does_not_leak() {
    let ctx = setup_test_env().await;
    let tenant_a = setup_test_tenant(&ctx.pool).await;
    let tenant_b = setup_test_tenant(&ctx.pool).await;
    let user_a = create_test_user(&ctx.metadata_store, "bs_ct_a4", tenant_a).await;
    let user_b = create_test_user(&ctx.metadata_store, "bs_ct_b4", tenant_b).await;
    let service = create_brainstorming_service(
        ctx.event_store.clone(),
        ctx.metadata_store.clone(),
        ctx.object_store.clone(),
        &ctx.pool,
    );

    let root = service.ensure_brainstorming_root(user_a.id, tenant_a).await.unwrap();
    let board_folder = create_test_folder(&ctx.folder_service(), user_a.id, tenant_a, "secret-board", Some(root.id)).await;
    create_test_file(
        &ctx.file_service(),
        user_a.id,
        tenant_a,
        Some(board_folder.id),
        "board.excalidraw",
        b"{\"type\":\"excalidraw\",\"version\":2,\"elements\":[]}",
    )
    .await;
    let meta = r#"{"id":"s1","type":"brainstorming.board","title":"Secret","slug":"secret-board","template":"template_blank_brainstorm","sourceFile":"board.excalidraw","previewFile":"preview.png","createdAt":"2026-04-30T00:00:00Z","updatedAt":"2026-04-30T00:00:00Z","schemaVersion":"1.0"}"#;
    create_test_file(
        &ctx.file_service(),
        user_a.id,
        tenant_a,
        Some(board_folder.id),
        ".rustshare.json",
        meta.as_bytes(),
    )
    .await;

    let list_b = service.list_boards(user_b.id, tenant_b).await.unwrap();
    assert!(
        !list_b.iter().any(|b| b.title == "Secret"),
        "Cross-tenant list_boards should not leak boards"
    );

    cleanup_user(&ctx.pool, user_a.id).await;
    cleanup_user(&ctx.pool, user_b.id).await;
    cleanup_tenant(&ctx.pool, tenant_a).await;
    cleanup_tenant(&ctx.pool, tenant_b).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_brainstorming_same_tenant_unauthorized_get_board_denied() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;
    let user_owner = create_test_user(&ctx.metadata_store, "bs_owner", tenant_id).await;
    let user_other = create_test_user(&ctx.metadata_store, "bs_other", tenant_id).await;
    let service = create_brainstorming_service(
        ctx.event_store.clone(),
        ctx.metadata_store.clone(),
        ctx.object_store.clone(),
        &ctx.pool,
    );

    let root = service.ensure_brainstorming_root(user_owner.id, tenant_id).await.unwrap();
    let board_folder = create_test_folder(&ctx.folder_service(), user_owner.id, tenant_id, "private-board", Some(root.id)).await;
    create_test_file(
        &ctx.file_service(),
        user_owner.id,
        tenant_id,
        Some(board_folder.id),
        "board.excalidraw",
        b"{\"type\":\"excalidraw\",\"version\":2,\"elements\":[]}",
    )
    .await;
    let meta = r#"{"id":"p1","type":"brainstorming.board","title":"Private","slug":"private-board","template":"template_blank_brainstorm","sourceFile":"board.excalidraw","previewFile":"preview.png","createdAt":"2026-04-30T00:00:00Z","updatedAt":"2026-04-30T00:00:00Z","schemaVersion":"1.0"}"#;
    create_test_file(
        &ctx.file_service(),
        user_owner.id,
        tenant_id,
        Some(board_folder.id),
        ".rustshare.json",
        meta.as_bytes(),
    )
    .await;

    let result = service.get_board(board_folder.id, user_other.id, tenant_id).await;
    assert!(
        matches!(result, Err(BrainstormError::PermissionDenied)),
        "Same-tenant unauthorized get_board should be denied, got {:?}",
        result
    );

    cleanup_user(&ctx.pool, user_owner.id).await;
    cleanup_user(&ctx.pool, user_other.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

// ============================================================================
// Meetings
// ============================================================================

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_meetings_cross_tenant_get_denied() {
    let ctx = setup_test_env().await;
    let tenant_a = setup_test_tenant(&ctx.pool).await;
    let tenant_b = setup_test_tenant(&ctx.pool).await;
    let user_a = create_test_user(&ctx.metadata_store, "meeting_ct_a", tenant_a).await;
    let user_b = create_test_user(&ctx.metadata_store, "meeting_ct_b", tenant_b).await;
    let service = create_meeting_service(
        ctx.event_store.clone(),
        ctx.metadata_store.clone(),
        ctx.object_store.clone(),
        &ctx.pool,
    );

    let meeting = service
        .create_meeting(
            user_a.id,
            tenant_a,
            "Secret".to_string(),
            "Exec".to_string(),
            chrono::Utc::now(),
            "confidential".to_string(),
        )
        .await
        .unwrap();

    let result = service.get_meeting(meeting.id, user_b.id, tenant_b).await;
    assert!(
        matches!(result, Err(MeetingError::PermissionDenied)),
        "Cross-tenant get_meeting should be denied, got {:?}",
        result
    );

    cleanup_user(&ctx.pool, user_a.id).await;
    cleanup_user(&ctx.pool, user_b.id).await;
    cleanup_tenant(&ctx.pool, tenant_a).await;
    cleanup_tenant(&ctx.pool, tenant_b).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_meetings_cross_tenant_update_denied() {
    let ctx = setup_test_env().await;
    let tenant_a = setup_test_tenant(&ctx.pool).await;
    let tenant_b = setup_test_tenant(&ctx.pool).await;
    let user_a = create_test_user(&ctx.metadata_store, "meeting_ct_a2", tenant_a).await;
    let user_b = create_test_user(&ctx.metadata_store, "meeting_ct_b2", tenant_b).await;
    let service = create_meeting_service(
        ctx.event_store.clone(),
        ctx.metadata_store.clone(),
        ctx.object_store.clone(),
        &ctx.pool,
    );

    let meeting = service
        .create_meeting(
            user_a.id,
            tenant_a,
            "Secret".to_string(),
            "Exec".to_string(),
            chrono::Utc::now(),
            "confidential".to_string(),
        )
        .await
        .unwrap();

    let result = service
        .update_meeting(meeting.id, user_b.id, tenant_b, Some("Hacked".to_string()), Some("evil".to_string()), None)
        .await;
    assert!(
        matches!(result, Err(MeetingError::PermissionDenied)),
        "Cross-tenant update_meeting should be denied, got {:?}",
        result
    );

    cleanup_user(&ctx.pool, user_a.id).await;
    cleanup_user(&ctx.pool, user_b.id).await;
    cleanup_tenant(&ctx.pool, tenant_a).await;
    cleanup_tenant(&ctx.pool, tenant_b).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_meetings_cross_tenant_list_does_not_leak() {
    let ctx = setup_test_env().await;
    let tenant_a = setup_test_tenant(&ctx.pool).await;
    let tenant_b = setup_test_tenant(&ctx.pool).await;
    let user_a = create_test_user(&ctx.metadata_store, "meeting_ct_a3", tenant_a).await;
    let user_b = create_test_user(&ctx.metadata_store, "meeting_ct_b3", tenant_b).await;
    let service = create_meeting_service(
        ctx.event_store.clone(),
        ctx.metadata_store.clone(),
        ctx.object_store.clone(),
        &ctx.pool,
    );

    let _meeting = service
        .create_meeting(
            user_a.id,
            tenant_a,
            "Secret".to_string(),
            "Exec".to_string(),
            chrono::Utc::now(),
            "confidential".to_string(),
        )
        .await
        .unwrap();

    let list_b = service.list_meetings(user_b.id, tenant_b).await.unwrap();
    assert!(
        !list_b.iter().any(|m| m.metadata.title == "Secret"),
        "Cross-tenant list_meetings should not leak meetings"
    );

    cleanup_user(&ctx.pool, user_a.id).await;
    cleanup_user(&ctx.pool, user_b.id).await;
    cleanup_tenant(&ctx.pool, tenant_a).await;
    cleanup_tenant(&ctx.pool, tenant_b).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_meetings_same_tenant_unauthorized_denied() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;
    let user_owner = create_test_user(&ctx.metadata_store, "meeting_owner", tenant_id).await;
    let user_other = create_test_user(&ctx.metadata_store, "meeting_other", tenant_id).await;
    let service = create_meeting_service(
        ctx.event_store.clone(),
        ctx.metadata_store.clone(),
        ctx.object_store.clone(),
        &ctx.pool,
    );

    let meeting = service
        .create_meeting(
            user_owner.id,
            tenant_id,
            "Private".to_string(),
            "Exec".to_string(),
            chrono::Utc::now(),
            "notes".to_string(),
        )
        .await
        .unwrap();

    let result = service.get_meeting(meeting.id, user_other.id, tenant_id).await;
    assert!(
        matches!(result, Err(MeetingError::PermissionDenied)),
        "Same-tenant unauthorized get_meeting should be denied, got {:?}",
        result
    );

    cleanup_user(&ctx.pool, user_owner.id).await;
    cleanup_user(&ctx.pool, user_other.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

// ============================================================================
// Standups
// ============================================================================

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_standups_cross_tenant_get_denied() {
    let ctx = setup_test_env().await;
    let tenant_a = setup_test_tenant(&ctx.pool).await;
    let tenant_b = setup_test_tenant(&ctx.pool).await;
    let user_a = create_test_user(&ctx.metadata_store, "standup_ct_a", tenant_a).await;
    let user_b = create_test_user(&ctx.metadata_store, "standup_ct_b", tenant_b).await;
    let service = create_standup_service(
        ctx.event_store.clone(),
        ctx.metadata_store.clone(),
        ctx.object_store.clone(),
        &ctx.pool,
    );

    let standup = service
        .create_standup(
            user_a.id,
            tenant_a,
            "Secret".to_string(),
            chrono::Utc::now(),
            "confidential".to_string(),
        )
        .await
        .unwrap();

    let result = service.get_standup(standup.id, user_b.id, tenant_b).await;
    assert!(
        matches!(result, Err(StandupError::PermissionDenied)),
        "Cross-tenant get_standup should be denied, got {:?}",
        result
    );

    cleanup_user(&ctx.pool, user_a.id).await;
    cleanup_user(&ctx.pool, user_b.id).await;
    cleanup_tenant(&ctx.pool, tenant_a).await;
    cleanup_tenant(&ctx.pool, tenant_b).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_standups_cross_tenant_update_denied() {
    let ctx = setup_test_env().await;
    let tenant_a = setup_test_tenant(&ctx.pool).await;
    let tenant_b = setup_test_tenant(&ctx.pool).await;
    let user_a = create_test_user(&ctx.metadata_store, "standup_ct_a2", tenant_a).await;
    let user_b = create_test_user(&ctx.metadata_store, "standup_ct_b2", tenant_b).await;
    let service = create_standup_service(
        ctx.event_store.clone(),
        ctx.metadata_store.clone(),
        ctx.object_store.clone(),
        &ctx.pool,
    );

    let standup = service
        .create_standup(
            user_a.id,
            tenant_a,
            "Secret".to_string(),
            chrono::Utc::now(),
            "confidential".to_string(),
        )
        .await
        .unwrap();

    let result = service
        .update_standup(standup.id, user_b.id, tenant_b, Some("Hacked".to_string()), Some("evil".to_string()))
        .await;
    assert!(
        matches!(result, Err(StandupError::PermissionDenied)),
        "Cross-tenant update_standup should be denied, got {:?}",
        result
    );

    cleanup_user(&ctx.pool, user_a.id).await;
    cleanup_user(&ctx.pool, user_b.id).await;
    cleanup_tenant(&ctx.pool, tenant_a).await;
    cleanup_tenant(&ctx.pool, tenant_b).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_standups_cross_tenant_list_does_not_leak() {
    let ctx = setup_test_env().await;
    let tenant_a = setup_test_tenant(&ctx.pool).await;
    let tenant_b = setup_test_tenant(&ctx.pool).await;
    let user_a = create_test_user(&ctx.metadata_store, "standup_ct_a3", tenant_a).await;
    let user_b = create_test_user(&ctx.metadata_store, "standup_ct_b3", tenant_b).await;
    let service = create_standup_service(
        ctx.event_store.clone(),
        ctx.metadata_store.clone(),
        ctx.object_store.clone(),
        &ctx.pool,
    );

    let _standup = service
        .create_standup(
            user_a.id,
            tenant_a,
            "Secret".to_string(),
            chrono::Utc::now(),
            "confidential".to_string(),
        )
        .await
        .unwrap();

    let list_b = service.list_standups(user_b.id, tenant_b).await.unwrap();
    assert!(
        !list_b.iter().any(|s| s.metadata.title == "Secret"),
        "Cross-tenant list_standups should not leak standups"
    );

    cleanup_user(&ctx.pool, user_a.id).await;
    cleanup_user(&ctx.pool, user_b.id).await;
    cleanup_tenant(&ctx.pool, tenant_a).await;
    cleanup_tenant(&ctx.pool, tenant_b).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_standups_same_tenant_unauthorized_denied() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;
    let user_owner = create_test_user(&ctx.metadata_store, "standup_owner", tenant_id).await;
    let user_other = create_test_user(&ctx.metadata_store, "standup_other", tenant_id).await;
    let service = create_standup_service(
        ctx.event_store.clone(),
        ctx.metadata_store.clone(),
        ctx.object_store.clone(),
        &ctx.pool,
    );

    let standup = service
        .create_standup(
            user_owner.id,
            tenant_id,
            "Private".to_string(),
            chrono::Utc::now(),
            "notes".to_string(),
        )
        .await
        .unwrap();

    let result = service.get_standup(standup.id, user_other.id, tenant_id).await;
    assert!(
        matches!(result, Err(StandupError::PermissionDenied)),
        "Same-tenant unauthorized get_standup should be denied, got {:?}",
        result
    );

    cleanup_user(&ctx.pool, user_owner.id).await;
    cleanup_user(&ctx.pool, user_other.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}
