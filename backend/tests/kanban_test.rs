//! Contract tests for RustShare Kanban module.
//!
//! Run with: cargo test --test kanban_test -- --ignored

use rustshare_core::domain::User;
use rustshare_core::events::EventBroadcaster;
use rustshare_core::services::{FileService, FolderService, PermissionResolver};
use rustshare_infrastructure::repositories::PermissionResolverRepository;
use rustshare_server::services::kanban_service::{
    KanbanService, CreateBoardInput, CreateCardInput,
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

    Arc::new(KanbanService::new(
        file_service,
        folder_service,
        metadata_store,
        object_store,
    ))
}

#[tokio::test]
#[ignore] // Requires database and S3
async fn contract_create_board_creates_folder_structure_and_metadata() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "kanban_contract_user_1", tenant_id).await;
    let service = create_kanban_service(event_store, metadata_store.clone(), object_store, &pool);

    let board = service
        .create_board(CreateBoardInput { title: "Product Launch".to_string() }, user.id, tenant_id)
        .await
        .expect("create_board should succeed");

    assert_eq!(board.title, "Product Launch");
    assert_eq!(board.columns.len(), 5);
    assert!(board.columns.iter().any(|c| c.slug == "00-Backlog"));
    assert!(board.columns.iter().any(|c| c.slug == "04-Done"));

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore]
async fn contract_create_card_creates_folder_and_metadata() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "kanban_contract_user_2", tenant_id).await;
    let service = create_kanban_service(event_store, metadata_store.clone(), object_store, &pool);

    let board = service
        .create_board(CreateBoardInput { title: "Test Board".to_string() }, user.id, tenant_id)
        .await
        .unwrap();

    let backlog = board.columns.iter().find(|c| c.slug == "00-Backlog").unwrap();
    let card = service
        .create_card(
            board.id.parse().unwrap(),
            CreateCardInput {
                title: "My First Card".to_string(),
                column_id: Some(backlog.id.clone()),
                content: Some("# Card content\n".to_string()),
                priority: Some("high".to_string()),
                tags: Some(vec!["urgent".to_string()]),
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
    assert!(card.tags.contains(&"urgent".to_string()));

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore]
async fn contract_move_card_updates_column_and_metadata() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "kanban_contract_user_3", tenant_id).await;
    let service = create_kanban_service(event_store, metadata_store.clone(), object_store, &pool);

    let board = service
        .create_board(CreateBoardInput { title: "Move Test".to_string() }, user.id, tenant_id)
        .await
        .unwrap();

    let backlog = board.columns.iter().find(|c| c.slug == "00-Backlog").unwrap();
    let ready = board.columns.iter().find(|c| c.slug == "01-Ready").unwrap();

    let card = service
        .create_card(
            board.id.parse().unwrap(),
            CreateCardInput {
                title: "Movable Card".to_string(),
                column_id: Some(backlog.id.clone()),
                content: None,
                priority: None,
                tags: None,
            },
            user.id,
            tenant_id,
        )
        .await
        .unwrap();

    let updated_board = service
        .move_card(
            card.id.parse().unwrap(),
            ready.id.clone(),
            2000,
            user.id,
            tenant_id,
        )
        .await
        .expect("move_card should succeed");

    let updated_ready = updated_board.columns.iter().find(|c| c.id == ready.id).unwrap();
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
#[ignore]
async fn contract_list_boards_returns_created_boards() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "kanban_contract_user_4", tenant_id).await;
    let service = create_kanban_service(event_store, metadata_store.clone(), object_store, &pool);

    service
        .create_board(CreateBoardInput { title: "Alpha".to_string() }, user.id, tenant_id)
        .await
        .unwrap();
    service
        .create_board(CreateBoardInput { title: "Beta".to_string() }, user.id, tenant_id)
        .await
        .unwrap();

    let boards = service.list_boards(user.id, tenant_id).await.unwrap();
    assert_eq!(boards.len(), 2);
    assert!(boards.iter().any(|b| b.title == "Alpha"));
    assert!(boards.iter().any(|b| b.title == "Beta"));

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore]
async fn contract_archive_card_hides_from_board() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "kanban_contract_user_5", tenant_id).await;
    let service = create_kanban_service(event_store, metadata_store.clone(), object_store, &pool);

    let board = service
        .create_board(CreateBoardInput { title: "Archive Test".to_string() }, user.id, tenant_id)
        .await
        .unwrap();

    let backlog = board.columns.iter().find(|c| c.slug == "00-Backlog").unwrap();
    let card = service
        .create_card(
            board.id.parse().unwrap(),
            CreateCardInput {
                title: "To Archive".to_string(),
                column_id: Some(backlog.id.clone()),
                content: None,
                priority: None,
                tags: None,
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
        .get_board(board.id.parse().unwrap(), user.id, tenant_id)
        .await
        .unwrap();
    let refreshed_backlog = refreshed_board.columns.iter().find(|c| c.id == backlog.id).unwrap();
    assert!(!refreshed_backlog.cards.iter().any(|c| c.id == card.id));

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore]
async fn contract_delete_card_removes_folder() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "kanban_contract_user_6", tenant_id).await;
    let service = create_kanban_service(event_store, metadata_store.clone(), object_store, &pool);

    let board = service
        .create_board(CreateBoardInput { title: "Delete Test".to_string() }, user.id, tenant_id)
        .await
        .unwrap();

    let backlog = board.columns.iter().find(|c| c.slug == "00-Backlog").unwrap();
    let card = service
        .create_card(
            board.id.parse().unwrap(),
            CreateCardInput {
                title: "To Delete".to_string(),
                column_id: Some(backlog.id.clone()),
                content: None,
                priority: None,
                tags: None,
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

    let result = service.get_card(card.id.parse().unwrap(), user.id, tenant_id).await;
    assert!(result.is_err());

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore]
async fn contract_move_card_rebalances_orders_when_too_dense() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "kanban_contract_user_7", tenant_id).await;
    let service = create_kanban_service(event_store, metadata_store.clone(), object_store, &pool);

    let board = service
        .create_board(CreateBoardInput { title: "Dense Test".to_string() }, user.id, tenant_id)
        .await
        .unwrap();

    let backlog = board.columns.iter().find(|c| c.slug == "00-Backlog").unwrap();

    // Create three cards
    let _card1 = service
        .create_card(
            board.id.parse().unwrap(),
            CreateCardInput { title: "Card 1".to_string(), column_id: Some(backlog.id.clone()), content: None, priority: None, tags: None },
            user.id,
            tenant_id,
        )
        .await
        .unwrap();

    let _card2 = service
        .create_card(
            board.id.parse().unwrap(),
            CreateCardInput { title: "Card 2".to_string(), column_id: Some(backlog.id.clone()), content: None, priority: None, tags: None },
            user.id,
            tenant_id,
        )
        .await
        .unwrap();

    let card3 = service
        .create_card(
            board.id.parse().unwrap(),
            CreateCardInput { title: "Card 3".to_string(), column_id: Some(backlog.id.clone()), content: None, priority: None, tags: None },
            user.id,
            tenant_id,
        )
        .await
        .unwrap();

    // Move card3 between card1 and card2 with a very small gap to trigger rebalancing
    service
        .move_card(card3.id.parse().unwrap(), backlog.id.clone(), 1005, user.id, tenant_id)
        .await
        .unwrap();

    let refreshed_board = service
        .get_board(board.id.parse().unwrap(), user.id, tenant_id)
        .await
        .unwrap();

    let refreshed_backlog = refreshed_board.columns.iter().find(|c| c.id == backlog.id).unwrap();
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
