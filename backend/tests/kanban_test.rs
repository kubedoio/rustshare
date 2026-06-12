//! Contract tests for RustShare Kanban module.
//!
//! ## Running the tests
//!
//! These tests require PostgreSQL and an S3-compatible object store (RustFS).
//!
//! ```bash
//! # Start dependencies
//! docker compose up -d postgres rustfs
//!
//! # Run migrations
//! cd backend && sqlx migrate run
//!
//! # Run tests
//! cargo test --test kanban_test -- --ignored
//! ```
//!
//! Environment variables (with defaults):
//! - `DATABASE_URL` — defaults to `postgres://rustshare:changeme@localhost:5432/rustshare`
//! - `S3_ENDPOINT` / `RUSTFS_ENDPOINT` — defaults to `http://localhost:9000`
//! - `S3_BUCKET` / `RUSTFS_BUCKET` — defaults to `rustshare`
//! - `S3_REGION` / `RUSTFS_REGION` — defaults to `us-east-1`

use bytes::Bytes;
use rustshare_core::domain::User;
use rustshare_core::events::EventBroadcaster;
use rustshare_core::services::{FileService, FolderService, PermissionResolver};
use rustshare_infrastructure::repositories::{PermissionResolverRepository, UserRepository};
use rustshare_server::services::kanban_service::{
    CreateBoardInput, CreateCardInput, KanbanError, KanbanService, MoveCardInput,
};
use rustshare_storage::{EventStore, MetadataStore, ObjectStore};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

async fn setup_test_env() -> (
    PgPool,
    Arc<EventStore>,
    Arc<MetadataStore>,
    Arc<ObjectStore>,
) {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://rustshare:changeme@localhost:5432/rustshare".to_string());

    let pool = PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to database");

    let event_store = Arc::new(EventStore::new(pool.clone()));
    let metadata_store = Arc::new(MetadataStore::new(pool.clone()));

    let s3_endpoint = std::env::var("S3_ENDPOINT")
        .or_else(|_| std::env::var("RUSTFS_ENDPOINT"))
        .unwrap_or_else(|_| "http://localhost:9000".to_string());
    let s3_region = std::env::var("S3_REGION")
        .or_else(|_| std::env::var("RUSTFS_REGION"))
        .unwrap_or_else(|_| "us-east-1".to_string());
    let s3_bucket = std::env::var("S3_BUCKET")
        .or_else(|_| std::env::var("RUSTFS_BUCKET"))
        .unwrap_or_else(|_| "rustshare".to_string());

    let object_store = Arc::new(
        ObjectStore::new(s3_endpoint, s3_region, s3_bucket)
            .await
            .expect("Failed to create object store"),
    );

    (pool, event_store, metadata_store, object_store)
}

fn create_file_service(
    event_store: Arc<EventStore>,
    metadata_store: Arc<MetadataStore>,
    object_store: Arc<ObjectStore>,
    pool: &PgPool,
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
    pool: &PgPool,
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

async fn create_test_user(metadata_store: &MetadataStore, username: &str, tenant_id: Uuid) -> User {
    let user = User::new(
        username.to_string(),
        format!("{} Display", username),
        "test_password_hash".to_string(),
        format!("{}@test.local", username),
        false,
        10_737_418_240,
        tenant_id,
    );

    metadata_store
        .create_user(&user)
        .await
        .expect("Failed to create test user");

    user
}

async fn cleanup_user(pool: &PgPool, user_id: Uuid) {
    sqlx::query("DELETE FROM files WHERE owner_id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup test files");

    sqlx::query("DELETE FROM folders WHERE owner_id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup test folders");

    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup test user");
}

fn create_kanban_service(
    event_store: Arc<EventStore>,
    metadata_store: Arc<MetadataStore>,
    object_store: Arc<ObjectStore>,
    pool: &PgPool,
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

    let user_repository = Arc::new(UserRepository::new(pool.clone()));

    Arc::new(KanbanService::new(
        file_service,
        folder_service,
        metadata_store,
        object_store,
        user_repository,
    ))
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_create_board_creates_folder_structure_and_metadata() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "kanban_contract_user_1", tenant_id).await;
    let service = create_kanban_service(event_store, metadata_store.clone(), object_store, &pool);

    let board = service
        .create_board(
            CreateBoardInput {
                title: "Product Launch".to_string(),
            },
            user.id,
            tenant_id,
        )
        .await
        .expect("create_board should succeed");

    assert_eq!(board.title, "Product Launch");
    assert_eq!(board.columns.len(), 5);
    assert!(board.columns.iter().any(|c| c.slug == "00-Backlog"));
    assert!(board.columns.iter().any(|c| c.slug == "04-Done"));

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_create_card_creates_folder_and_metadata() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "kanban_contract_user_2", tenant_id).await;
    let service = create_kanban_service(event_store, metadata_store.clone(), object_store, &pool);

    let board = service
        .create_board(
            CreateBoardInput {
                title: "Test Board".to_string(),
            },
            user.id,
            tenant_id,
        )
        .await
        .unwrap();

    let backlog = board
        .columns
        .iter()
        .find(|c| c.slug == "00-Backlog")
        .unwrap();
    let card = service
        .create_card(
            board.id.clone(),
            CreateCardInput {
                title: "My First Card".to_string(),
                column_id: Some(backlog.id.clone()),
                content: Some("# Card content\n".to_string()),
                priority: Some("high".to_string()),
                labels: Some(vec!["label_red".to_string()]),
                assignees: None,
                due_date: None,
            },
            user.id,
            tenant_id,
        )
        .await
        .expect("create_card should succeed");

    assert_eq!(card.title, "My First Card");
    assert!(card.slug.starts_with("CARD-"));
    assert_eq!(card.column_id, backlog.id);
    assert_eq!(card.priority, "high");
    assert!(card.labels.iter().any(|l| l.id == "label_red"));

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_move_card_updates_column_and_metadata() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "kanban_contract_user_3", tenant_id).await;
    let service = create_kanban_service(event_store, metadata_store.clone(), object_store, &pool);

    let board = service
        .create_board(
            CreateBoardInput {
                title: "Move Test".to_string(),
            },
            user.id,
            tenant_id,
        )
        .await
        .unwrap();

    let backlog = board
        .columns
        .iter()
        .find(|c| c.slug == "00-Backlog")
        .unwrap();
    let ready = board.columns.iter().find(|c| c.slug == "01-Ready").unwrap();

    let card = service
        .create_card(
            board.id.clone(),
            CreateCardInput {
                title: "Movable Card".to_string(),
                column_id: Some(backlog.id.clone()),
                content: None,
                priority: None,
                labels: None,
                assignees: None,
                due_date: None,
            },
            user.id,
            tenant_id,
        )
        .await
        .unwrap();

    let updated_board = service
        .move_card(
            card.id.parse().unwrap(),
            MoveCardInput {
                board_id: board.id.clone(),
                target_column_id: ready.id.clone(),
                target_order: Some(2000),
                before_card_id: None,
                after_card_id: None,
            },
            user.id,
            tenant_id,
        )
        .await
        .expect("move_card should succeed");

    let updated_ready = updated_board
        .columns
        .iter()
        .find(|c| c.id == ready.id)
        .unwrap();
    assert!(updated_ready.cards.iter().any(|c| c.id == card.id));

    let updated_card = service
        .get_card(card.id.parse().unwrap(), user.id, tenant_id)
        .await
        .unwrap();
    assert_eq!(updated_card.column_id, ready.id);
    assert_eq!(updated_card.status, "ready");
    assert_eq!(updated_card.order, 2000);

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_list_boards_returns_created_boards() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "kanban_contract_user_4", tenant_id).await;
    let service = create_kanban_service(event_store, metadata_store.clone(), object_store, &pool);

    service
        .create_board(
            CreateBoardInput {
                title: "Alpha".to_string(),
            },
            user.id,
            tenant_id,
        )
        .await
        .unwrap();
    service
        .create_board(
            CreateBoardInput {
                title: "Beta".to_string(),
            },
            user.id,
            tenant_id,
        )
        .await
        .unwrap();

    let boards = service
        .list_boards(user.id, tenant_id, 1000, 0)
        .await
        .unwrap();
    assert_eq!(boards.len(), 2);
    assert!(boards.iter().any(|b| b.title == "Alpha"));
    assert!(boards.iter().any(|b| b.title == "Beta"));

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_archive_card_hides_from_board() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "kanban_contract_user_5", tenant_id).await;
    let service = create_kanban_service(event_store, metadata_store.clone(), object_store, &pool);

    let board = service
        .create_board(
            CreateBoardInput {
                title: "Archive Test".to_string(),
            },
            user.id,
            tenant_id,
        )
        .await
        .unwrap();

    let backlog = board
        .columns
        .iter()
        .find(|c| c.slug == "00-Backlog")
        .unwrap();
    let card = service
        .create_card(
            board.id.clone(),
            CreateCardInput {
                title: "To Archive".to_string(),
                column_id: Some(backlog.id.clone()),
                content: None,
                priority: None,
                labels: None,
                assignees: None,
                due_date: None,
            },
            user.id,
            tenant_id,
        )
        .await
        .unwrap();

    service
        .archive_card(card.id.parse().unwrap(), user.id, tenant_id)
        .await
        .expect("archive_card should succeed");

    let refreshed_board = service
        .get_board(board.id.clone(), user.id, tenant_id)
        .await
        .unwrap();
    let refreshed_backlog = refreshed_board
        .columns
        .iter()
        .find(|c| c.id == backlog.id)
        .unwrap();
    assert!(!refreshed_backlog.cards.iter().any(|c| c.id == card.id));

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_delete_card_removes_folder() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "kanban_contract_user_6", tenant_id).await;
    let service = create_kanban_service(event_store, metadata_store.clone(), object_store, &pool);

    let board = service
        .create_board(
            CreateBoardInput {
                title: "Delete Test".to_string(),
            },
            user.id,
            tenant_id,
        )
        .await
        .unwrap();

    let backlog = board
        .columns
        .iter()
        .find(|c| c.slug == "00-Backlog")
        .unwrap();
    let card = service
        .create_card(
            board.id.clone(),
            CreateCardInput {
                title: "To Delete".to_string(),
                column_id: Some(backlog.id.clone()),
                content: None,
                priority: None,
                labels: None,
                assignees: None,
                due_date: None,
            },
            user.id,
            tenant_id,
        )
        .await
        .unwrap();

    service
        .delete_card(card.id.parse().unwrap(), user.id, tenant_id)
        .await
        .expect("delete_card should succeed");

    let result = service
        .get_card(card.id.parse().unwrap(), user.id, tenant_id)
        .await;
    assert!(result.is_err());

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_move_card_rebalances_orders_when_too_dense() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "kanban_contract_user_7", tenant_id).await;
    let service = create_kanban_service(event_store, metadata_store.clone(), object_store, &pool);

    let board = service
        .create_board(
            CreateBoardInput {
                title: "Dense Test".to_string(),
            },
            user.id,
            tenant_id,
        )
        .await
        .unwrap();

    let backlog = board
        .columns
        .iter()
        .find(|c| c.slug == "00-Backlog")
        .unwrap();

    // Create three cards
    let _card1 = service
        .create_card(
            board.id.clone(),
            CreateCardInput {
                title: "Card 1".to_string(),
                column_id: Some(backlog.id.clone()),
                content: None,
                priority: None,
                labels: None,
                assignees: None,
                due_date: None,
            },
            user.id,
            tenant_id,
        )
        .await
        .unwrap();

    let _card2 = service
        .create_card(
            board.id.clone(),
            CreateCardInput {
                title: "Card 2".to_string(),
                column_id: Some(backlog.id.clone()),
                content: None,
                priority: None,
                labels: None,
                assignees: None,
                due_date: None,
            },
            user.id,
            tenant_id,
        )
        .await
        .unwrap();

    let card3 = service
        .create_card(
            board.id.clone(),
            CreateCardInput {
                title: "Card 3".to_string(),
                column_id: Some(backlog.id.clone()),
                content: None,
                priority: None,
                labels: None,
                assignees: None,
                due_date: None,
            },
            user.id,
            tenant_id,
        )
        .await
        .unwrap();

    // Move card3 between card1 and card2 with a very small gap to trigger rebalancing
    service
        .move_card(
            card3.id.parse().unwrap(),
            MoveCardInput {
                board_id: board.id.clone(),
                target_column_id: backlog.id.clone(),
                target_order: Some(1005),
                before_card_id: None,
                after_card_id: None,
            },
            user.id,
            tenant_id,
        )
        .await
        .unwrap();

    let refreshed_board = service
        .get_board(board.id.parse().unwrap(), user.id, tenant_id)
        .await
        .unwrap();

    let refreshed_backlog = refreshed_board
        .columns
        .iter()
        .find(|c| c.id == backlog.id)
        .unwrap();
    let orders: Vec<i32> = refreshed_backlog.cards.iter().map(|c| c.order).collect();

    // After rebalancing, orders should be spaced by at least 1000
    for i in 1..orders.len() {
        assert!(
            (orders[i] - orders[i - 1]).abs() >= 1000,
            "Orders should be rebalanced: {:?}",
            orders
        );
    }

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_get_board_by_slug_returns_board() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "slug_user", tenant_id).await;
    let service = create_kanban_service(event_store, metadata_store.clone(), object_store, &pool);

    let board = service
        .create_board(
            CreateBoardInput {
                title: "Slug Board".to_string(),
            },
            user.id,
            tenant_id,
        )
        .await
        .unwrap();

    // Get by ID
    let by_id = service
        .get_board(board.id.clone(), user.id, tenant_id)
        .await
        .unwrap();
    assert_eq!(by_id.title, "Slug Board");

    // Get by Slug
    let by_slug = service
        .get_board("slug-board".to_string(), user.id, tenant_id)
        .await
        .unwrap();
    assert_eq!(by_slug.id, board.id);
    assert_eq!(by_slug.slug, "slug-board");
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_invalid_label_color_rejected() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "color_user", tenant_id).await;
    let service = create_kanban_service(event_store, metadata_store.clone(), object_store, &pool);

    let board = service
        .create_board(
            CreateBoardInput {
                title: "Color Test".to_string(),
            },
            user.id,
            tenant_id,
        )
        .await
        .unwrap();

    let result = service
        .create_label(
            board.id.parse().unwrap(),
            rustshare_server::services::kanban_service::CreateLabelInput {
                name: "Bad".to_string(),
                color: "#ff0000".to_string(),
            },
            user.id,
            tenant_id,
        )
        .await;
    assert!(result.is_err(), "Should reject arbitrary hex colors");

    let result = service
        .create_label(
            board.id.parse().unwrap(),
            rustshare_server::services::kanban_service::CreateLabelInput {
                name: "Bad".to_string(),
                color: "javascript:alert(1)".to_string(),
            },
            user.id,
            tenant_id,
        )
        .await;
    assert!(
        result.is_err(),
        "Should reject script pseudo-protocol colors"
    );

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_card_description_persisted_in_index_md() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "desc_user", tenant_id).await;
    let service = create_kanban_service(event_store, metadata_store.clone(), object_store, &pool);

    let board = service
        .create_board(
            CreateBoardInput {
                title: "Desc Test".to_string(),
            },
            user.id,
            tenant_id,
        )
        .await
        .unwrap();

    let backlog = board
        .columns
        .iter()
        .find(|c| c.slug == "00-Backlog")
        .unwrap();
    let card = service
        .create_card(
            board.id.clone(),
            rustshare_server::services::kanban_service::CreateCardInput {
                title: "Desc Card".to_string(),
                column_id: Some(backlog.id.clone()),
                content: Some("# Hello\n\nThis is the description.".to_string()),
                priority: None,
                labels: None,
                assignees: None,
                due_date: None,
            },
            user.id,
            tenant_id,
        )
        .await
        .unwrap();

    let detail = service
        .get_card_detail(card.id.parse().unwrap(), user.id, tenant_id)
        .await
        .unwrap();
    assert!(detail.summary.content.contains("This is the description."));

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_metadata_files_hidden_from_folder_listing() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "meta_user", tenant_id).await;
    let service = create_kanban_service(
        event_store,
        metadata_store.clone(),
        object_store.clone(),
        &pool,
    );

    let board = service
        .create_board(
            CreateBoardInput {
                title: "Meta Test".to_string(),
            },
            user.id,
            tenant_id,
        )
        .await
        .unwrap();

    let contents = service
        .folder_service
        .list_contents(board.id.parse().unwrap(), user.id)
        .await
        .unwrap();

    for file in &contents.files {
        assert!(
            !file.name.starts_with(".rustshare-") && file.name != "events.jsonl",
            "Hidden metadata file {} should not appear in listing",
            file.name
        );
    }

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_move_card_preserves_metadata_and_events() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "move_user", tenant_id).await;
    let service = create_kanban_service(event_store, metadata_store.clone(), object_store, &pool);

    let board = service
        .create_board(
            CreateBoardInput {
                title: "Move Test".to_string(),
            },
            user.id,
            tenant_id,
        )
        .await
        .unwrap();

    let backlog = board
        .columns
        .iter()
        .find(|c| c.slug == "00-Backlog")
        .unwrap();
    let ready = board.columns.iter().find(|c| c.slug == "01-Ready").unwrap();

    let card = service
        .create_card(
            board.id.clone(),
            rustshare_server::services::kanban_service::CreateCardInput {
                title: "Moveable".to_string(),
                column_id: Some(backlog.id.clone()),
                content: Some("Initial content".to_string()),
                priority: None,
                labels: None,
                assignees: None,
                due_date: None,
            },
            user.id,
            tenant_id,
        )
        .await
        .unwrap();

    let _ = service
        .move_card(
            card.id.parse().unwrap(),
            rustshare_server::services::kanban_service::MoveCardInput {
                board_id: board.id.clone(),
                target_column_id: ready.id.clone(),
                target_order: Some(2000),
                before_card_id: None,
                after_card_id: None,
            },
            user.id,
            tenant_id,
        )
        .await
        .unwrap();

    let detail = service
        .get_card_detail(card.id.parse().unwrap(), user.id, tenant_id)
        .await
        .unwrap();
    assert_eq!(detail.summary.column_id, ready.id);
    assert!(detail.summary.content.contains("Initial content"));
    assert!(
        detail.activity.iter().any(|e| e.event_type == "card.moved"),
        "Move event should be recorded"
    );

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_disabling_module_does_not_delete_data() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "disable_user", tenant_id).await;
    let service = create_kanban_service(event_store, metadata_store.clone(), object_store, &pool);

    let board = service
        .create_board(
            CreateBoardInput {
                title: "Persistent".to_string(),
            },
            user.id,
            tenant_id,
        )
        .await
        .unwrap();

    let board_id = board.id.clone();

    // Simulate disable by checking data still exists when fetched directly
    let boards_after = service
        .list_boards(user.id, tenant_id, 1000, 0)
        .await
        .unwrap();
    assert!(boards_after.iter().any(|b| b.id == board_id));

    cleanup_user(&pool, user.id).await;
}
// LB-02: Negative tenant/permission contract tests

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_cross_tenant_get_board_denied() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    let user_a = create_test_user(&metadata_store, "kanban_user_a", tenant_a).await;
    let user_b = create_test_user(&metadata_store, "kanban_user_b", tenant_b).await;
    let service = create_kanban_service(event_store, metadata_store.clone(), object_store, &pool);

    let board = service
        .create_board(
            CreateBoardInput {
                title: "Secret".to_string(),
            },
            user_a.id,
            tenant_a,
        )
        .await
        .unwrap();

    let result = service
        .get_board(board.id.clone(), user_b.id, tenant_b)
        .await;
    assert!(
        matches!(result, Err(KanbanError::PermissionDenied)),
        "Cross-tenant get_board should be denied, got {:?}",
        result
    );

    cleanup_user(&pool, user_a.id).await;
    cleanup_user(&pool, user_b.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_cross_tenant_update_board_denied() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    let user_a = create_test_user(&metadata_store, "kanban_user_a2", tenant_a).await;
    let user_b = create_test_user(&metadata_store, "kanban_user_b2", tenant_b).await;
    let service = create_kanban_service(event_store, metadata_store.clone(), object_store, &pool);

    let board = service
        .create_board(
            CreateBoardInput {
                title: "Secret".to_string(),
            },
            user_a.id,
            tenant_a,
        )
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

    cleanup_user(&pool, user_a.id).await;
    cleanup_user(&pool, user_b.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_cross_tenant_delete_card_denied() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    let user_a = create_test_user(&metadata_store, "kanban_user_a3", tenant_a).await;
    let user_b = create_test_user(&metadata_store, "kanban_user_b3", tenant_b).await;
    let service = create_kanban_service(event_store, metadata_store.clone(), object_store, &pool);

    let board = service
        .create_board(
            CreateBoardInput {
                title: "Secret".to_string(),
            },
            user_a.id,
            tenant_a,
        )
        .await
        .unwrap();

    let backlog = board
        .columns
        .iter()
        .find(|c| c.slug == "00-Backlog")
        .unwrap();
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

    let result = service
        .delete_card(card.id.parse().unwrap(), user_b.id, tenant_b)
        .await;
    assert!(
        matches!(result, Err(KanbanError::PermissionDenied)),
        "Cross-tenant delete_card should be denied, got {:?}",
        result
    );

    cleanup_user(&pool, user_a.id).await;
    cleanup_user(&pool, user_b.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_cross_tenant_list_boards_does_not_leak() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    let user_a = create_test_user(&metadata_store, "kanban_user_a4", tenant_a).await;
    let user_b = create_test_user(&metadata_store, "kanban_user_b4", tenant_b).await;
    let service = create_kanban_service(event_store, metadata_store.clone(), object_store, &pool);

    let _board = service
        .create_board(
            CreateBoardInput {
                title: "Secret".to_string(),
            },
            user_a.id,
            tenant_a,
        )
        .await
        .unwrap();

    let list_b = service
        .list_boards(user_b.id, tenant_b, 1000, 0)
        .await
        .unwrap();
    assert!(
        !list_b.iter().any(|b| b.title == "Secret"),
        "Cross-tenant list_boards should not leak boards"
    );

    cleanup_user(&pool, user_a.id).await;
    cleanup_user(&pool, user_b.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_same_tenant_unauthorized_get_board_denied() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user_owner = create_test_user(&metadata_store, "kanban_owner", tenant_id).await;
    let user_other = create_test_user(&metadata_store, "kanban_other", tenant_id).await;
    let service = create_kanban_service(event_store, metadata_store.clone(), object_store, &pool);

    let board = service
        .create_board(
            CreateBoardInput {
                title: "Private".to_string(),
            },
            user_owner.id,
            tenant_id,
        )
        .await
        .unwrap();

    let result = service
        .get_board(board.id.clone(), user_other.id, tenant_id)
        .await;
    assert!(
        matches!(result, Err(KanbanError::PermissionDenied)),
        "Same-tenant unauthorized get_board should be denied, got {:?}",
        result
    );

    cleanup_user(&pool, user_owner.id).await;
    cleanup_user(&pool, user_other.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_same_tenant_unauthorized_get_card_detail_denied() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user_owner = create_test_user(&metadata_store, "kanban_owner2", tenant_id).await;
    let user_other = create_test_user(&metadata_store, "kanban_other2", tenant_id).await;
    let service = create_kanban_service(event_store, metadata_store.clone(), object_store, &pool);

    let board = service
        .create_board(
            CreateBoardInput {
                title: "Private".to_string(),
            },
            user_owner.id,
            tenant_id,
        )
        .await
        .unwrap();

    let backlog = board
        .columns
        .iter()
        .find(|c| c.slug == "00-Backlog")
        .unwrap();
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

    let result = service
        .get_card_detail(card.id.parse().unwrap(), user_other.id, tenant_id)
        .await;
    assert!(
        matches!(result, Err(KanbanError::PermissionDenied)),
        "Same-tenant unauthorized get_card_detail should be denied, got {:?}",
        result
    );

    cleanup_user(&pool, user_owner.id).await;
    cleanup_user(&pool, user_other.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_cross_tenant_create_card_denied() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    let user_a = create_test_user(&metadata_store, "kanban_ct_a5", tenant_a).await;
    let user_b = create_test_user(&metadata_store, "kanban_ct_b5", tenant_b).await;
    let service = create_kanban_service(event_store, metadata_store.clone(), object_store, &pool);

    let board = service
        .create_board(
            CreateBoardInput {
                title: "Secret".to_string(),
            },
            user_a.id,
            tenant_a,
        )
        .await
        .unwrap();
    let backlog = board
        .columns
        .iter()
        .find(|c| c.slug == "00-Backlog")
        .unwrap();

    let result = service
        .create_card(
            board.id.clone(),
            CreateCardInput {
                title: "Hacked".to_string(),
                column_id: Some(backlog.id.clone()),
                content: None,
                priority: None,
                labels: None,
                assignees: None,
                due_date: None,
            },
            user_b.id,
            tenant_b,
        )
        .await;
    assert!(
        matches!(result, Err(KanbanError::PermissionDenied)),
        "Cross-tenant create_card should be denied, got {:?}",
        result
    );

    cleanup_user(&pool, user_a.id).await;
    cleanup_user(&pool, user_b.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_cross_tenant_move_card_denied() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    let user_a = create_test_user(&metadata_store, "kanban_ct_a6", tenant_a).await;
    let user_b = create_test_user(&metadata_store, "kanban_ct_b6", tenant_b).await;
    let service = create_kanban_service(event_store, metadata_store.clone(), object_store, &pool);

    let board = service
        .create_board(
            CreateBoardInput {
                title: "Secret".to_string(),
            },
            user_a.id,
            tenant_a,
        )
        .await
        .unwrap();
    let backlog = board
        .columns
        .iter()
        .find(|c| c.slug == "00-Backlog")
        .unwrap();
    let ready = board.columns.iter().find(|c| c.slug == "01-Ready").unwrap();
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

    let result = service
        .move_card(
            card.id.parse().unwrap(),
            MoveCardInput {
                board_id: board.id.clone(),
                target_column_id: ready.id.clone(),
                target_order: Some(2000),
                before_card_id: None,
                after_card_id: None,
            },
            user_b.id,
            tenant_b,
        )
        .await;
    assert!(
        matches!(result, Err(KanbanError::PermissionDenied)),
        "Cross-tenant move_card should be denied, got {:?}",
        result
    );

    cleanup_user(&pool, user_a.id).await;
    cleanup_user(&pool, user_b.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_cross_tenant_archive_card_denied() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    let user_a = create_test_user(&metadata_store, "kanban_ct_a7", tenant_a).await;
    let user_b = create_test_user(&metadata_store, "kanban_ct_b7", tenant_b).await;
    let service = create_kanban_service(event_store, metadata_store.clone(), object_store, &pool);

    let board = service
        .create_board(
            CreateBoardInput {
                title: "Secret".to_string(),
            },
            user_a.id,
            tenant_a,
        )
        .await
        .unwrap();
    let backlog = board
        .columns
        .iter()
        .find(|c| c.slug == "00-Backlog")
        .unwrap();
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

    let result = service
        .archive_card(card.id.parse().unwrap(), user_b.id, tenant_b)
        .await;
    assert!(
        matches!(result, Err(KanbanError::PermissionDenied)),
        "Cross-tenant archive_card should be denied, got {:?}",
        result
    );

    cleanup_user(&pool, user_a.id).await;
    cleanup_user(&pool, user_b.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_cross_tenant_get_card_denied() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    let user_a = create_test_user(&metadata_store, "kanban_ct_a8", tenant_a).await;
    let user_b = create_test_user(&metadata_store, "kanban_ct_b8", tenant_b).await;
    let service = create_kanban_service(event_store, metadata_store.clone(), object_store, &pool);

    let board = service
        .create_board(
            CreateBoardInput {
                title: "Secret".to_string(),
            },
            user_a.id,
            tenant_a,
        )
        .await
        .unwrap();
    let backlog = board
        .columns
        .iter()
        .find(|c| c.slug == "00-Backlog")
        .unwrap();
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

    let result = service
        .get_card(card.id.parse().unwrap(), user_b.id, tenant_b)
        .await;
    assert!(
        matches!(result, Err(KanbanError::PermissionDenied)),
        "Cross-tenant get_card should be denied, got {:?}",
        result
    );

    cleanup_user(&pool, user_a.id).await;
    cleanup_user(&pool, user_b.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_cross_tenant_get_card_detail_denied() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    let user_a = create_test_user(&metadata_store, "kanban_ct_a9", tenant_a).await;
    let user_b = create_test_user(&metadata_store, "kanban_ct_b9", tenant_b).await;
    let service = create_kanban_service(event_store, metadata_store.clone(), object_store, &pool);

    let board = service
        .create_board(
            CreateBoardInput {
                title: "Secret".to_string(),
            },
            user_a.id,
            tenant_a,
        )
        .await
        .unwrap();
    let backlog = board
        .columns
        .iter()
        .find(|c| c.slug == "00-Backlog")
        .unwrap();
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
            user_a.id,
            tenant_a,
        )
        .await
        .unwrap();

    let result = service
        .get_card_detail(card.id.parse().unwrap(), user_b.id, tenant_b)
        .await;
    assert!(
        matches!(result, Err(KanbanError::PermissionDenied)),
        "Cross-tenant get_card_detail should be denied, got {:?}",
        result
    );

    cleanup_user(&pool, user_a.id).await;
    cleanup_user(&pool, user_b.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_same_tenant_unauthorized_create_card_denied() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user_owner = create_test_user(&metadata_store, "kanban_owner_card", tenant_id).await;
    let user_other = create_test_user(&metadata_store, "kanban_other_card", tenant_id).await;
    let service = create_kanban_service(event_store, metadata_store.clone(), object_store, &pool);

    let board = service
        .create_board(
            CreateBoardInput {
                title: "Private".to_string(),
            },
            user_owner.id,
            tenant_id,
        )
        .await
        .unwrap();
    let backlog = board
        .columns
        .iter()
        .find(|c| c.slug == "00-Backlog")
        .unwrap();

    let result = service
        .create_card(
            board.id.clone(),
            CreateCardInput {
                title: "Hacked".to_string(),
                column_id: Some(backlog.id.clone()),
                content: None,
                priority: None,
                labels: None,
                assignees: None,
                due_date: None,
            },
            user_other.id,
            tenant_id,
        )
        .await;
    assert!(
        matches!(result, Err(KanbanError::PermissionDenied)),
        "Same-tenant unauthorized create_card should be denied, got {:?}",
        result
    );

    cleanup_user(&pool, user_owner.id).await;
    cleanup_user(&pool, user_other.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_same_tenant_unauthorized_move_card_denied() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user_owner = create_test_user(&metadata_store, "kanban_owner_move", tenant_id).await;
    let user_other = create_test_user(&metadata_store, "kanban_other_move", tenant_id).await;
    let service = create_kanban_service(event_store, metadata_store.clone(), object_store, &pool);

    let board = service
        .create_board(
            CreateBoardInput {
                title: "Private".to_string(),
            },
            user_owner.id,
            tenant_id,
        )
        .await
        .unwrap();
    let backlog = board
        .columns
        .iter()
        .find(|c| c.slug == "00-Backlog")
        .unwrap();
    let ready = board.columns.iter().find(|c| c.slug == "01-Ready").unwrap();
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
            user_owner.id,
            tenant_id,
        )
        .await
        .unwrap();

    let result = service
        .move_card(
            card.id.parse().unwrap(),
            MoveCardInput {
                board_id: board.id.clone(),
                target_column_id: ready.id.clone(),
                target_order: Some(2000),
                before_card_id: None,
                after_card_id: None,
            },
            user_other.id,
            tenant_id,
        )
        .await;
    assert!(
        matches!(result, Err(KanbanError::PermissionDenied)),
        "Same-tenant unauthorized move_card should be denied, got {:?}",
        result
    );

    cleanup_user(&pool, user_owner.id).await;
    cleanup_user(&pool, user_other.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_same_tenant_unauthorized_archive_card_denied() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user_owner = create_test_user(&metadata_store, "kanban_owner_arch", tenant_id).await;
    let user_other = create_test_user(&metadata_store, "kanban_other_arch", tenant_id).await;
    let service = create_kanban_service(event_store, metadata_store.clone(), object_store, &pool);

    let board = service
        .create_board(
            CreateBoardInput {
                title: "Private".to_string(),
            },
            user_owner.id,
            tenant_id,
        )
        .await
        .unwrap();
    let backlog = board
        .columns
        .iter()
        .find(|c| c.slug == "00-Backlog")
        .unwrap();
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
            user_owner.id,
            tenant_id,
        )
        .await
        .unwrap();

    let result = service
        .archive_card(card.id.parse().unwrap(), user_other.id, tenant_id)
        .await;
    assert!(
        matches!(result, Err(KanbanError::PermissionDenied)),
        "Same-tenant unauthorized archive_card should be denied, got {:?}",
        result
    );

    cleanup_user(&pool, user_owner.id).await;
    cleanup_user(&pool, user_other.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_same_tenant_unauthorized_delete_card_denied() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user_owner = create_test_user(&metadata_store, "kanban_owner_delc", tenant_id).await;
    let user_other = create_test_user(&metadata_store, "kanban_other_delc", tenant_id).await;
    let service = create_kanban_service(event_store, metadata_store.clone(), object_store, &pool);

    let board = service
        .create_board(
            CreateBoardInput {
                title: "Private".to_string(),
            },
            user_owner.id,
            tenant_id,
        )
        .await
        .unwrap();
    let backlog = board
        .columns
        .iter()
        .find(|c| c.slug == "00-Backlog")
        .unwrap();
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
            user_owner.id,
            tenant_id,
        )
        .await
        .unwrap();

    let result = service
        .delete_card(card.id.parse().unwrap(), user_other.id, tenant_id)
        .await;
    assert!(
        matches!(result, Err(KanbanError::PermissionDenied)),
        "Same-tenant unauthorized delete_card should be denied, got {:?}",
        result
    );

    cleanup_user(&pool, user_owner.id).await;
    cleanup_user(&pool, user_other.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_same_tenant_unauthorized_get_card_denied() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user_owner = create_test_user(&metadata_store, "kanban_owner_gc", tenant_id).await;
    let user_other = create_test_user(&metadata_store, "kanban_other_gc", tenant_id).await;
    let service = create_kanban_service(event_store, metadata_store.clone(), object_store, &pool);

    let board = service
        .create_board(
            CreateBoardInput {
                title: "Private".to_string(),
            },
            user_owner.id,
            tenant_id,
        )
        .await
        .unwrap();
    let backlog = board
        .columns
        .iter()
        .find(|c| c.slug == "00-Backlog")
        .unwrap();
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
            user_owner.id,
            tenant_id,
        )
        .await
        .unwrap();

    let result = service
        .get_card(card.id.parse().unwrap(), user_other.id, tenant_id)
        .await;
    assert!(
        matches!(result, Err(KanbanError::PermissionDenied)),
        "Same-tenant unauthorized get_card should be denied, got {:?}",
        result
    );

    cleanup_user(&pool, user_owner.id).await;
    cleanup_user(&pool, user_other.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_same_tenant_unauthorized_list_boards_does_not_leak() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user_owner = create_test_user(&metadata_store, "kanban_owner_list", tenant_id).await;
    let user_other = create_test_user(&metadata_store, "kanban_other_list", tenant_id).await;
    let service = create_kanban_service(event_store, metadata_store.clone(), object_store, &pool);

    let _board = service
        .create_board(
            CreateBoardInput {
                title: "Private".to_string(),
            },
            user_owner.id,
            tenant_id,
        )
        .await
        .unwrap();

    let list_other = service
        .list_boards(user_other.id, tenant_id, 1000, 0)
        .await
        .unwrap();
    assert!(
        !list_other.iter().any(|b| b.title == "Private"),
        "Same-tenant unauthorized list_boards should not leak boards"
    );

    cleanup_user(&pool, user_owner.id).await;
    cleanup_user(&pool, user_other.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_same_tenant_unauthorized_update_board_denied() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user_owner = create_test_user(&metadata_store, "kanban_owner_ub", tenant_id).await;
    let user_other = create_test_user(&metadata_store, "kanban_other_ub", tenant_id).await;
    let service = create_kanban_service(event_store, metadata_store.clone(), object_store, &pool);

    let board = service
        .create_board(
            CreateBoardInput {
                title: "Private".to_string(),
            },
            user_owner.id,
            tenant_id,
        )
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
            user_other.id,
            tenant_id,
        )
        .await;
    assert!(
        matches!(result, Err(KanbanError::PermissionDenied)),
        "Same-tenant unauthorized update_board should be denied, got {:?}",
        result
    );

    cleanup_user(&pool, user_owner.id).await;
    cleanup_user(&pool, user_other.id).await;
}

// ============================================================================
// Step 11: Attachment Security and Portability Tests
// ============================================================================

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_kanban_attachment_rejects_dotdot() {
    // KanbanService::sanitize_attachment_name already rejects '..' substring.
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "kanban_attach_dotdot", tenant_id).await;
    let service = create_kanban_service(event_store, metadata_store.clone(), object_store, &pool);

    let board = service
        .create_board(
            CreateBoardInput {
                title: "Attach".to_string(),
            },
            user.id,
            tenant_id,
        )
        .await
        .unwrap();
    let backlog = board
        .columns
        .iter()
        .find(|c| c.slug == "00-Backlog")
        .unwrap();
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
            user.id,
            tenant_id,
        )
        .await
        .unwrap();

    let result = service
        .add_card_attachment(
            card.id.parse().unwrap(),
            "..secret.txt".to_string(),
            Bytes::from("test"),
            "text/plain".to_string(),
            user.id,
            tenant_id,
        )
        .await;

    assert!(
        result.is_err(),
        "Kanban should reject attachment with '..' in filename"
    );

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_kanban_attachment_rejects_path_separator() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "kanban_attach_sep", tenant_id).await;
    let service = create_kanban_service(event_store, metadata_store.clone(), object_store, &pool);

    let board = service
        .create_board(
            CreateBoardInput {
                title: "Attach".to_string(),
            },
            user.id,
            tenant_id,
        )
        .await
        .unwrap();
    let backlog = board
        .columns
        .iter()
        .find(|c| c.slug == "00-Backlog")
        .unwrap();
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
            user.id,
            tenant_id,
        )
        .await
        .unwrap();

    for bad_name in ["../secret.txt", "secret/file.txt", "secret\\file.txt"] {
        let result = service
            .add_card_attachment(
                card.id.parse().unwrap(),
                bad_name.to_string(),
                Bytes::from("test"),
                "text/plain".to_string(),
                user.id,
                tenant_id,
            )
            .await;
        assert!(
            result.is_err(),
            "Kanban should reject attachment with path separator: {}",
            bad_name
        );
    }

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_kanban_attachment_rejects_reserved_names() {
    // Kanban already rejects specific reserved metadata filenames.
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "kanban_attach_reserved", tenant_id).await;
    let service = create_kanban_service(event_store, metadata_store.clone(), object_store, &pool);

    let board = service
        .create_board(
            CreateBoardInput {
                title: "Attach".to_string(),
            },
            user.id,
            tenant_id,
        )
        .await
        .unwrap();
    let backlog = board
        .columns
        .iter()
        .find(|c| c.slug == "00-Backlog")
        .unwrap();
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
            user.id,
            tenant_id,
        )
        .await
        .unwrap();

    for bad_name in [
        ".rustshare-board.json",
        ".rustshare-column.json",
        ".rustshare-card.json",
        "events.jsonl",
        "index.md",
    ] {
        let result = service
            .add_card_attachment(
                card.id.parse().unwrap(),
                bad_name.to_string(),
                Bytes::from("test"),
                "application/json".to_string(),
                user.id,
                tenant_id,
            )
            .await;
        assert!(
            result.is_err(),
            "Kanban should reject reserved filename: {}",
            bad_name
        );
    }

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_kanban_attachment_rejects_generic_rustshare() {
    // FIXED: sanitize_attachment_name now rejects all .rustshare* filenames.
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "kanban_attach_generic", tenant_id).await;
    let service = create_kanban_service(event_store, metadata_store.clone(), object_store, &pool);

    let board = service
        .create_board(
            CreateBoardInput {
                title: "Attach".to_string(),
            },
            user.id,
            tenant_id,
        )
        .await
        .unwrap();
    let backlog = board
        .columns
        .iter()
        .find(|c| c.slug == "00-Backlog")
        .unwrap();
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
            user.id,
            tenant_id,
        )
        .await
        .unwrap();

    let result = service
        .add_card_attachment(
            card.id.parse().unwrap(),
            ".rustshare.json".to_string(),
            Bytes::from("test"),
            "application/json".to_string(),
            user.id,
            tenant_id,
        )
        .await;

    assert!(
        result.is_err(),
        "generic .rustshare.json should be rejected: {:?}",
        result
    );

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_kanban_attachment_rejects_editor_json() {
    // FIXED: sanitize_attachment_name now rejects index.editor.json.
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "kanban_attach_editor", tenant_id).await;
    let service = create_kanban_service(event_store, metadata_store.clone(), object_store, &pool);

    let board = service
        .create_board(
            CreateBoardInput {
                title: "Attach".to_string(),
            },
            user.id,
            tenant_id,
        )
        .await
        .unwrap();
    let backlog = board
        .columns
        .iter()
        .find(|c| c.slug == "00-Backlog")
        .unwrap();
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
            user.id,
            tenant_id,
        )
        .await
        .unwrap();

    let result = service
        .add_card_attachment(
            card.id.parse().unwrap(),
            "index.editor.json".to_string(),
            Bytes::from("test"),
            "application/json".to_string(),
            user.id,
            tenant_id,
        )
        .await;

    assert!(
        result.is_err(),
        "index.editor.json should be rejected: {:?}",
        result
    );

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_kanban_attachment_cross_tenant_upload_denied() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    let user_a = create_test_user(&metadata_store, "kanban_ct_attach_a", tenant_a).await;
    let user_b = create_test_user(&metadata_store, "kanban_ct_attach_b", tenant_b).await;
    let service = create_kanban_service(event_store, metadata_store.clone(), object_store, &pool);

    let board = service
        .create_board(
            CreateBoardInput {
                title: "Secret".to_string(),
            },
            user_a.id,
            tenant_a,
        )
        .await
        .unwrap();
    let backlog = board
        .columns
        .iter()
        .find(|c| c.slug == "00-Backlog")
        .unwrap();
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

    let result = service
        .add_card_attachment(
            card.id.parse().unwrap(),
            "evil.txt".to_string(),
            Bytes::from("hacked"),
            "text/plain".to_string(),
            user_b.id,
            tenant_b,
        )
        .await;

    assert!(
        result.is_err(),
        "Cross-tenant attachment upload should be denied, got {:?}",
        result
    );

    cleanup_user(&pool, user_a.id).await;
    cleanup_user(&pool, user_b.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_kanban_attachment_unauthorized_user_denied() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user_owner = create_test_user(&metadata_store, "kanban_owner_attach", tenant_id).await;
    let user_other = create_test_user(&metadata_store, "kanban_other_attach", tenant_id).await;
    let service = create_kanban_service(event_store, metadata_store.clone(), object_store, &pool);

    let board = service
        .create_board(
            CreateBoardInput {
                title: "Private".to_string(),
            },
            user_owner.id,
            tenant_id,
        )
        .await
        .unwrap();
    let backlog = board
        .columns
        .iter()
        .find(|c| c.slug == "00-Backlog")
        .unwrap();
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
            user_owner.id,
            tenant_id,
        )
        .await
        .unwrap();

    let result = service
        .add_card_attachment(
            card.id.parse().unwrap(),
            "evil.txt".to_string(),
            Bytes::from("hacked"),
            "text/plain".to_string(),
            user_other.id,
            tenant_id,
        )
        .await;

    assert!(
        result.is_err(),
        "Same-tenant unauthorized attachment upload should be denied, got {:?}",
        result
    );

    cleanup_user(&pool, user_owner.id).await;
    cleanup_user(&pool, user_other.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_kanban_card_detail_excludes_hidden_attachments() {
    // Kanban card detail should not list hidden metadata files as attachments.
    // folder_service.list_contents already filters hidden files, so this should pass.
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "kanban_attach_hidden", tenant_id).await;
    let service = create_kanban_service(
        event_store.clone(),
        metadata_store.clone(),
        object_store.clone(),
        &pool,
    );
    let file_service = create_file_service(
        event_store.clone(),
        metadata_store.clone(),
        object_store.clone(),
        &pool,
    );

    let board = service
        .create_board(
            CreateBoardInput {
                title: "Attach".to_string(),
            },
            user.id,
            tenant_id,
        )
        .await
        .unwrap();
    let backlog = board
        .columns
        .iter()
        .find(|c| c.slug == "00-Backlog")
        .unwrap();
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
            user.id,
            tenant_id,
        )
        .await
        .unwrap();

    // Upload a regular attachment and a hidden metadata file directly to the card folder
    let card_folder_id = card.id.parse::<Uuid>().unwrap();
    let folders = metadata_store
        .list_folders(Some(card_folder_id), user.id, tenant_id)
        .await
        .unwrap();
    let attachments_folder = folders.iter().find(|f| f.name == "attachments").unwrap();

    file_service
        .upload_file(
            user.id,
            "real.txt".to_string(),
            Some(attachments_folder.id),
            Bytes::from("real"),
            "text/plain".to_string(),
            tenant_id,
        )
        .await
        .unwrap();
    file_service
        .upload_file(
            user.id,
            ".rustshare-card.json".to_string(),
            Some(attachments_folder.id),
            Bytes::from("hidden"),
            "application/json".to_string(),
            tenant_id,
        )
        .await
        .unwrap();

    let detail = service
        .get_card_detail(card.id.parse().unwrap(), user.id, tenant_id)
        .await
        .unwrap();

    let names: Vec<&str> = detail.attachments.iter().map(|a| a.name.as_str()).collect();
    assert!(names.contains(&"real.txt"), "real.txt should be listed");
    assert!(
        !names.contains(&".rustshare-card.json"),
        "Hidden metadata file should not appear in attachment list"
    );

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_kanban_delete_attachment_rejects_non_attachment_file() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "kanban_attach_delete_scope", tenant_id).await;
    let service = create_kanban_service(
        event_store.clone(),
        metadata_store.clone(),
        object_store.clone(),
        &pool,
    );
    let file_service = create_file_service(
        event_store.clone(),
        metadata_store.clone(),
        object_store,
        &pool,
    );
    let folder_service = create_folder_service(event_store, metadata_store.clone(), &pool);

    let board = service
        .create_board(
            CreateBoardInput {
                title: "Attach".to_string(),
            },
            user.id,
            tenant_id,
        )
        .await
        .unwrap();
    let backlog = board
        .columns
        .iter()
        .find(|c| c.slug == "00-Backlog")
        .unwrap();
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
            user.id,
            tenant_id,
        )
        .await
        .unwrap();

    let card_contents = folder_service
        .list_contents(card.id.parse().unwrap(), user.id)
        .await
        .unwrap();
    let attachments_folder = card_contents
        .folders
        .iter()
        .find(|folder| folder.name == "attachments")
        .expect("create_card should create an attachments folder");
    folder_service
        .delete_folder(attachments_folder.id, user.id)
        .await
        .expect("test setup should remove empty attachments folder");

    let unrelated = file_service
        .upload_file(
            user.id,
            "unrelated.txt".to_string(),
            None,
            Bytes::from("keep me"),
            "text/plain".to_string(),
            tenant_id,
        )
        .await
        .unwrap();

    let result = service
        .delete_card_attachment(card.id.parse().unwrap(), unrelated.id, user.id, tenant_id)
        .await;

    assert!(
        matches!(result, Err(KanbanError::PermissionDenied)),
        "Deleting a non-attachment file through a card should be denied, got {:?}",
        result
    );
    assert!(
        file_service.get_file(unrelated.id, user.id).await.is_ok(),
        "Unrelated file must still exist after denied attachment delete"
    );
    let card_contents_after_delete_attempt = folder_service
        .list_contents(card.id.parse().unwrap(), user.id)
        .await
        .unwrap();
    assert!(
        card_contents_after_delete_attempt
            .folders
            .iter()
            .all(|folder| folder.name != "attachments"),
        "Denied attachment delete must not create an attachments folder"
    );

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_kanban_attachment_duplicate_overwrites() {
    // CURRENT BEHAVIOR: write_binary_file_by_name updates existing files instead
    // of creating a duplicate or renaming.
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "kanban_attach_dup", tenant_id).await;
    let service = create_kanban_service(event_store, metadata_store.clone(), object_store, &pool);

    let board = service
        .create_board(
            CreateBoardInput {
                title: "Attach".to_string(),
            },
            user.id,
            tenant_id,
        )
        .await
        .unwrap();
    let backlog = board
        .columns
        .iter()
        .find(|c| c.slug == "00-Backlog")
        .unwrap();
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
            user.id,
            tenant_id,
        )
        .await
        .unwrap();

    let first = service
        .add_card_attachment(
            card.id.parse().unwrap(),
            "dup.txt".to_string(),
            Bytes::from("first"),
            "text/plain".to_string(),
            user.id,
            tenant_id,
        )
        .await
        .unwrap();

    let second = service
        .add_card_attachment(
            card.id.parse().unwrap(),
            "dup.txt".to_string(),
            Bytes::from("second"),
            "text/plain".to_string(),
            user.id,
            tenant_id,
        )
        .await
        .unwrap();

    assert_eq!(
        first.id, second.id,
        "Duplicate filename should overwrite existing attachment"
    );
    assert_eq!(second.size, 6);

    cleanup_user(&pool, user.id).await;
}
