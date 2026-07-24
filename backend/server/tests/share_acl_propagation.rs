//! Integration tests for share lifecycle ACL propagation to the AI index.
//!
//! These tests verify that creating, updating, and revoking shares triggers a
//! best-effort ACL refresh for the affected resource so the AI search index
//! reflects the current permission state.
//!
//! Requires a running PostgreSQL database and S3-compatible object store.

mod common;

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::{Path, State};
use rustshare_core::domain::SharePermissions;
use rustshare_server::handlers::{AuthenticatedUser, ValidatedJson};
use rustshare_server::services::note_index_sink::NoteIndexSink;
use uuid::Uuid;

/// Recording sink that captures `index_note` calls.
struct RecordingSink {
    indexed: Arc<Mutex<Vec<(Uuid, Uuid)>>>, // (tenant_id, file_id)
}

impl RecordingSink {
    fn new() -> Self {
        Self {
            indexed: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn take_indexed(&self) -> Vec<(Uuid, Uuid)> {
        std::mem::take(&mut *self.indexed.lock().unwrap())
    }
}

impl NoteIndexSink for RecordingSink {
    fn index_note(
        &self,
        file_id: Uuid,
        _file_name: String,
        _file_path: String,
        _content: String,
        _mime_type: String,
        _owner_id: rustshare_core::domain::UserId,
        _acl: rustshare_core::services::IndexAclProjection,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        let indexed = Arc::clone(&self.indexed);
        Box::pin(async move {
            indexed.lock().unwrap().push((Uuid::nil(), file_id));
        })
    }

    fn update_acl(
        &self,
        _tenant_id: Uuid,
        _note_id: Uuid,
        _acl: rustshare_core::services::NoteAclPayload,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        Box::pin(async {})
    }

    fn remove_note(
        &self,
        _tenant_id: Uuid,
        _note_id: Uuid,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        Box::pin(async {})
    }
}

async fn setup_with_recording_sink() -> (rustshare_server::AppState, Arc<RecordingSink>) {
    let (mut state, _url) = common::setup_test_server().await;
    let sink = Arc::new(RecordingSink::new());

    let note_service = Arc::new(
        rustshare_server::services::note_service::NoteService::new(
            state.file_service.clone(),
            state.folder_service.clone(),
            state.metadata_store.clone(),
            state.object_store.clone(),
            state.permission_resolver.clone(),
            state.db_pool.clone(),
        )
        .with_index_sink(Some(sink.clone())),
    );
    state.note_service = note_service;

    (state, sink)
}

async fn insert_test_user(pool: &sqlx::PgPool, tenant_id: Uuid, user_id: Uuid, email: &str) {
    sqlx::query(
        r#"
        INSERT INTO users (id, username, email, password_hash, tenant_id, display_name, storage_quota)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        ON CONFLICT (id) DO NOTHING
        "#,
    )
    .bind(user_id)
    .bind(format!("user{}", user_id))
    .bind(email)
    .bind("hash")
    .bind(tenant_id)
    .bind(format!("User {}", user_id))
    .bind(10_737_418_240i64)
    .execute(pool)
    .await
    .unwrap();
}

async fn insert_test_tenant(pool: &sqlx::PgPool) -> Uuid {
    let tenant_id = Uuid::new_v4();
    sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, $2)")
        .bind(tenant_id)
        .bind(format!("Test Tenant {}", tenant_id))
        .execute(pool)
        .await
        .unwrap();
    tenant_id
}

async fn insert_test_markdown_file(pool: &sqlx::PgPool, tenant_id: Uuid, owner_id: Uuid) -> Uuid {
    let file_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO files (id, name, path, storage_key, content_hash, size, mime_type, owner_id, tenant_id, current_version)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
        "#,
    )
    .bind(file_id)
    .bind("note.md")
    .bind("/Workspace/Notes/note.md")
    .bind(format!("notes/{}", file_id))
    .bind("hash")
    .bind(100i64)
    .bind("text/markdown")
    .bind(owner_id)
    .bind(tenant_id)
    .bind(1i32)
    .execute(pool)
    .await
    .unwrap();
    file_id
}

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn test_share_create_triggers_acl_refresh() {
    let (state, sink) = setup_with_recording_sink().await;
    let tenant_id = insert_test_tenant(&state.db_pool).await;
    let owner_id = Uuid::new_v4();
    let recipient_id = Uuid::new_v4();
    insert_test_user(&state.db_pool, tenant_id, owner_id, "owner@example.com").await;
    insert_test_user(
        &state.db_pool,
        tenant_id,
        recipient_id,
        "recipient@example.com",
    )
    .await;
    let file_id = insert_test_markdown_file(&state.db_pool, tenant_id, owner_id).await;

    rustshare_server::handlers::user_shares::create_file_share(
        State(state.clone()),
        Path(file_id),
        AuthenticatedUser {
            user_id: owner_id,
            tenant_id,
        },
        ValidatedJson(
            rustshare_server::handlers::user_shares::CreateFileShareRequest {
                recipient_email: "recipient@example.com".to_string(),
                permission: SharePermissions::View,
            },
        ),
    )
    .await
    .unwrap();

    // Wait for the spawned ACL refresh task to complete.
    tokio::time::sleep(Duration::from_millis(200)).await;

    let indexed = sink.take_indexed();
    assert!(
        indexed.iter().any(|(_, id)| *id == file_id),
        "Share creation should trigger AI index ACL refresh for the file"
    );
}

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn test_share_update_triggers_acl_refresh() {
    let (state, sink) = setup_with_recording_sink().await;
    let tenant_id = insert_test_tenant(&state.db_pool).await;
    let owner_id = Uuid::new_v4();
    let recipient_id = Uuid::new_v4();
    insert_test_user(&state.db_pool, tenant_id, owner_id, "owner@example.com").await;
    insert_test_user(
        &state.db_pool,
        tenant_id,
        recipient_id,
        "recipient@example.com",
    )
    .await;
    let file_id = insert_test_markdown_file(&state.db_pool, tenant_id, owner_id).await;

    let response = rustshare_server::handlers::user_shares::create_file_share(
        State(state.clone()),
        Path(file_id),
        AuthenticatedUser {
            user_id: owner_id,
            tenant_id,
        },
        ValidatedJson(
            rustshare_server::handlers::user_shares::CreateFileShareRequest {
                recipient_email: "recipient@example.com".to_string(),
                permission: SharePermissions::View,
            },
        ),
    )
    .await
    .unwrap();

    // Extract share id from the JSON response.
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let share_id = Uuid::parse_str(created["share_id"].as_str().unwrap()).unwrap();

    // Drain the create refresh.
    tokio::time::sleep(Duration::from_millis(200)).await;
    let _ = sink.take_indexed();

    rustshare_server::handlers::user_shares::update_recipient_permission(
        State(state.clone()),
        Path(share_id),
        AuthenticatedUser {
            user_id: owner_id,
            tenant_id,
        },
        axum::Json(
            rustshare_server::handlers::user_shares::UpdatePermissionRequest {
                permission: SharePermissions::Edit,
            },
        ),
    )
    .await
    .unwrap();

    tokio::time::sleep(Duration::from_millis(200)).await;
    let indexed = sink.take_indexed();
    assert!(
        indexed.iter().any(|(_, id)| *id == file_id),
        "Share permission update should trigger AI index ACL refresh for the file"
    );
}

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn test_share_revoke_triggers_acl_refresh() {
    let (state, sink) = setup_with_recording_sink().await;
    let tenant_id = insert_test_tenant(&state.db_pool).await;
    let owner_id = Uuid::new_v4();
    let recipient_id = Uuid::new_v4();
    insert_test_user(&state.db_pool, tenant_id, owner_id, "owner@example.com").await;
    insert_test_user(
        &state.db_pool,
        tenant_id,
        recipient_id,
        "recipient@example.com",
    )
    .await;
    let file_id = insert_test_markdown_file(&state.db_pool, tenant_id, owner_id).await;

    let response = rustshare_server::handlers::user_shares::create_file_share(
        State(state.clone()),
        Path(file_id),
        AuthenticatedUser {
            user_id: owner_id,
            tenant_id,
        },
        ValidatedJson(
            rustshare_server::handlers::user_shares::CreateFileShareRequest {
                recipient_email: "recipient@example.com".to_string(),
                permission: SharePermissions::View,
            },
        ),
    )
    .await
    .unwrap();

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let share_id = Uuid::parse_str(created["share_id"].as_str().unwrap()).unwrap();

    tokio::time::sleep(Duration::from_millis(200)).await;
    let _ = sink.take_indexed();

    rustshare_server::handlers::user_shares::remove_recipient(
        State(state.clone()),
        Path(share_id),
        AuthenticatedUser {
            user_id: owner_id,
            tenant_id,
        },
    )
    .await
    .unwrap();

    tokio::time::sleep(Duration::from_millis(200)).await;
    let indexed = sink.take_indexed();
    assert!(
        indexed.iter().any(|(_, id)| *id == file_id),
        "Share revocation should trigger AI index ACL refresh for the file"
    );
}
