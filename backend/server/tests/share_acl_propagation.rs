//! Integration tests for share lifecycle ACL propagation to the AI index.
//!
//! These tests verify that creating, updating, and revoking shares triggers a
//! best-effort ACL refresh for the affected resource so the AI search index
//! reflects the current permission state.
//!
//! AR-04 coverage (issue #367): group membership mutations (admin add/remove,
//! admin deletion, SCIM v1/v2 provisioning, PUT replacement, PATCH, and
//! deletion) refresh the index ACL for every resource shared with the group.
//! Targets are discovered by `recipient_group_id` with `revoked_at IS NULL`,
//! each share's own tenant scopes the refresh (legacy groups may carry a nil
//! tenant), share/resource tenant equality is validated before indexing, and
//! the refresh context is the resource owner rather than the historical share
//! creator.
//!
//! Requires a running PostgreSQL database and S3-compatible object store.

mod common;

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use chrono::{DateTime, Utc};
use rustshare_core::domain::SharePermissions;
use rustshare_core::services::{ScimGroup, ScimMember, ScimPatchOperation, ScimPatchRequest};
use rustshare_server::handlers::admin::groups as admin_groups;
use rustshare_server::handlers::{scim, scim_v2, AdminUser, ValidatedJson};
use rustshare_server::services::note_index_sink::NoteIndexSink;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Recording sink
// ---------------------------------------------------------------------------

/// One captured `index_note` call with the full ACL projection context.
#[derive(Debug, Clone)]
struct RecordedIndex {
    tenant_id: Uuid,
    note_id: Uuid,
    owner_id: Uuid,
    /// Sorted `Display` strings of the read principals (e.g. `group:{id}`).
    principals: Vec<String>,
    acl_hash: String,
    acl_version: i64,
}

#[derive(Default)]
struct SinkState {
    indexed: Vec<RecordedIndex>,
    update_acl_calls: usize,
    remove_note_calls: usize,
}

/// Recording sink that captures `index_note` calls with their real tenant,
/// resource id, and full ACL projection (principals, hash, version), plus
/// invocation counts for every sink method.
#[derive(Clone, Default)]
struct RecordingSink {
    state: Arc<Mutex<SinkState>>,
}

impl RecordingSink {
    fn new() -> Self {
        Self::default()
    }

    fn take_indexed(&self) -> Vec<RecordedIndex> {
        std::mem::take(&mut self.state.lock().unwrap().indexed)
    }

    fn snapshot(&self) -> Vec<RecordedIndex> {
        self.state.lock().unwrap().indexed.clone()
    }

    fn indexed_count(&self) -> usize {
        self.state.lock().unwrap().indexed.len()
    }

    fn calls_for(&self, note_id: Uuid) -> Vec<RecordedIndex> {
        self.snapshot()
            .into_iter()
            .filter(|record| record.note_id == note_id)
            .collect()
    }

    fn update_acl_calls(&self) -> usize {
        self.state.lock().unwrap().update_acl_calls
    }

    fn remove_note_calls(&self) -> usize {
        self.state.lock().unwrap().remove_note_calls
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
        owner_id: rustshare_core::domain::UserId,
        acl: rustshare_core::services::IndexAclProjection,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        let sink = self.clone();
        Box::pin(async move {
            let mut principals: Vec<String> =
                acl.read_principals.iter().map(|p| p.to_string()).collect();
            principals.sort();
            sink.state.lock().unwrap().indexed.push(RecordedIndex {
                tenant_id: acl.tenant_id,
                note_id: file_id,
                owner_id,
                principals,
                acl_hash: acl.acl_hash.clone(),
                acl_version: acl.acl_version,
            });
        })
    }

    fn update_acl(
        &self,
        _tenant_id: Uuid,
        _note_id: Uuid,
        _acl: rustshare_core::services::NoteAclPayload,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        let sink = self.clone();
        Box::pin(async move {
            sink.state.lock().unwrap().update_acl_calls += 1;
        })
    }

    fn remove_note(
        &self,
        _tenant_id: Uuid,
        _note_id: Uuid,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        let sink = self.clone();
        Box::pin(async move {
            sink.state.lock().unwrap().remove_note_calls += 1;
        })
    }
}

// ---------------------------------------------------------------------------
// Bounded completion polling (replaces fixed sleeps in new checks)
// ---------------------------------------------------------------------------

const SINK_WAIT: Duration = Duration::from_secs(10);

/// Wait until the given note has at least `min` recorded index calls.
async fn wait_for_calls_for(sink: &RecordingSink, note_id: Uuid, min: usize) {
    let deadline = Instant::now() + SINK_WAIT;
    loop {
        let count = sink.calls_for(note_id).len();
        if count >= min {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for note {note_id} to reach {min} index calls, have {count}"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// Assert that no new index calls arrive within a bounded window. Used for
/// negative cases after the triggering work has already completed; a control
/// dispatch afterwards proves the sink pipeline is still alive.
async fn assert_no_new_calls(sink: &RecordingSink, baseline: usize, window: Duration) {
    let deadline = Instant::now() + window;
    loop {
        let count = sink.indexed_count();
        assert_eq!(
            count, baseline,
            "unexpected index dispatch: expected no new calls, have {count}"
        );
        if Instant::now() >= deadline {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

// ---------------------------------------------------------------------------
// Disposable-destination guard
// ---------------------------------------------------------------------------

/// Refuse to run unless every test destination is the disposable test stack.
///
/// The harness (`common::setup_test_server`) dotenv-loads and falls back to
/// defaults that can silently point at the application database (`rustshare`)
/// or bucket (`rustshare`). This guard reads the same live environment —
/// `S3_*` taking precedence over `RUSTFS_*`, exactly like the harness — and
/// panics in setup, before any test logic or pool/object-store construction,
/// when a destination is not provably disposable.
fn assert_disposable_test_destination() {
    // Match the harness's dotenv overlay before reading the environment.
    dotenvy::dotenv().ok();

    // DATABASE_URL: loopback host and a rustshare_test[_...] database name.
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://rustshare:changeme@localhost:5432/rustshare".to_string());
    let parsed = url::Url::parse(&database_url).unwrap_or_else(|error| {
        panic!(
            "refusing to run share_acl_propagation tests: DATABASE_URL \
             ({database_url:?}) is not a valid URL: {error}"
        )
    });
    let host = parsed.host_str().unwrap_or_default();
    if !matches!(host, "127.0.0.1" | "localhost" | "::1") {
        panic!(
            "refusing to run share_acl_propagation tests: DATABASE_URL host \
             {host:?} is not loopback (expected 127.0.0.1, localhost, or ::1); \
             point DATABASE_URL at the disposable rustshare_test database"
        );
    }
    let database_name = parsed.path().trim_start_matches('/');
    if database_name != "rustshare_test" && !database_name.starts_with("rustshare_test_") {
        panic!(
            "refusing to run share_acl_propagation tests: DATABASE_URL database \
             {database_name:?} is not disposable (expected rustshare_test or \
             rustshare_test_*); the harness would otherwise target the \
             application database"
        );
    }

    // S3 endpoint: loopback only (S3_ENDPOINT preferred over RUSTFS_ENDPOINT,
    // matching the harness).
    let s3_endpoint = std::env::var("S3_ENDPOINT")
        .or_else(|_| std::env::var("RUSTFS_ENDPOINT"))
        .unwrap_or_else(|_| "http://localhost:9000".to_string());
    let endpoint = url::Url::parse(&s3_endpoint).unwrap_or_else(|error| {
        panic!(
            "refusing to run share_acl_propagation tests: S3 endpoint \
             ({s3_endpoint:?}) is not a valid URL: {error}"
        )
    });
    let endpoint_host = endpoint.host_str().unwrap_or_default();
    if !matches!(endpoint_host, "127.0.0.1" | "localhost" | "::1") {
        panic!(
            "refusing to run share_acl_propagation tests: S3 endpoint host \
             {endpoint_host:?} is not loopback (expected 127.0.0.1, localhost, \
             or ::1); point S3_ENDPOINT at the disposable test object store"
        );
    }

    // Bucket: the rustshare-test namespace (S3_BUCKET preferred over
    // RUSTFS_BUCKET, matching the harness).
    let bucket = std::env::var("S3_BUCKET")
        .or_else(|_| std::env::var("RUSTFS_BUCKET"))
        .unwrap_or_else(|_| "rustshare".to_string());
    if !bucket.starts_with("rustshare-test") {
        panic!(
            "refusing to run share_acl_propagation tests: bucket {bucket:?} is \
             outside the rustshare-test namespace; the harness would otherwise \
             target the application bucket"
        );
    }
}

// ---------------------------------------------------------------------------
// Fixture helpers
// ---------------------------------------------------------------------------

async fn setup_with_recording_sink() -> (rustshare_server::AppState, Arc<RecordingSink>) {
    assert_disposable_test_destination();
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

async fn insert_test_user_with_external(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    user_id: Uuid,
    email: &str,
    external_id: &str,
) {
    sqlx::query(
        r#"
        INSERT INTO users (id, username, email, password_hash, tenant_id, display_name, storage_quota, external_id)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
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
    .bind(external_id)
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

/// Seed a group directly. `tenant_id` may be `Uuid::nil()` for the legacy
/// nil-tenant fixture that membership handlers can still mutate when both
/// legs of the membership are nil-tenant (post-AR-03).
async fn insert_test_group(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    external_id: Option<&str>,
) -> Uuid {
    let group_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO user_groups (id, name, tenant_id, external_id) VALUES ($1, $2, $3, $4)",
    )
    .bind(group_id)
    .bind(format!("ar04-group-{group_id}"))
    .bind(tenant_id)
    .bind(external_id)
    .execute(pool)
    .await
    .unwrap();
    group_id
}

async fn insert_group_member_direct(pool: &sqlx::PgPool, group_id: Uuid, user_id: Uuid) {
    sqlx::query(
        "INSERT INTO group_members (group_id, user_id) VALUES ($1, $2) ON CONFLICT (group_id, user_id) DO NOTHING",
    )
    .bind(group_id)
    .bind(user_id)
    .execute(pool)
    .await
    .unwrap();
}

#[derive(Debug, Clone, Copy)]
enum ShareTarget {
    File(Uuid),
    Folder(Uuid),
}

/// Seed a group share directly, bypassing the API (legacy fixtures may be
/// cross-tenant in ways the API now rejects post-AR-03).
async fn seed_group_share(
    pool: &sqlx::PgPool,
    target: ShareTarget,
    group_id: Uuid,
    share_tenant_id: Uuid,
    created_by: Uuid,
    revoked: bool,
    expires_at: Option<DateTime<Utc>>,
) -> Uuid {
    let share_id = Uuid::new_v4();
    let (file_id, folder_id) = match target {
        ShareTarget::File(file_id) => (Some(file_id), None),
        ShareTarget::Folder(folder_id) => (None, Some(folder_id)),
    };
    // Group shares carry a NULL share_token (check_share_recipient requires
    // exactly one recipient kind).
    sqlx::query(
        r#"
        INSERT INTO shares (
            id, file_id, folder_id, recipient_group_id,
            permissions, created_by, tenant_id, revoked_at, expires_at, created_at
        ) VALUES ($1, $2, $3, $4, 'View', $5, $6, $7, $8, NOW())
        "#,
    )
    .bind(share_id)
    .bind(file_id)
    .bind(folder_id)
    .bind(group_id)
    .bind(created_by)
    .bind(share_tenant_id)
    .bind(if revoked { Some(Utc::now()) } else { None })
    .bind(expires_at)
    .execute(pool)
    .await
    .unwrap();
    share_id
}

async fn create_test_note(
    state: &rustshare_server::AppState,
    sink: &RecordingSink,
    tenant_id: Uuid,
    owner_id: Uuid,
) -> Uuid {
    let note = state
        .note_service
        .create_note(
            owner_id,
            tenant_id,
            Some("Test note".to_string()),
            None,
            Some("# Test note".to_string()),
        )
        .await
        .unwrap();
    let _ = sink.take_indexed();
    note.id
}

async fn insert_test_pdf(pool: &sqlx::PgPool, tenant_id: Uuid, owner_id: Uuid) -> Uuid {
    let file_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO files (id, name, path, storage_key, content_hash, size, mime_type, owner_id, tenant_id) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(file_id)
    .bind("document.pdf")
    .bind("/document.pdf")
    .bind(format!("tests/{file_id}"))
    .bind("hash")
    .bind(100i64)
    .bind("application/pdf")
    .bind(owner_id)
    .bind(tenant_id)
    .execute(pool)
    .await
    .unwrap();
    file_id
}

async fn count_group_shares(pool: &sqlx::PgPool, group_id: Uuid) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM shares WHERE recipient_group_id = $1")
        .bind(group_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

fn principal_list(record: &RecordedIndex) -> String {
    record.principals.join(",")
}

// ---------------------------------------------------------------------------
// SCIM test environment (serialized env mutation within this test binary)
// ---------------------------------------------------------------------------

static SCIM_ENV_LOCK: Mutex<()> = Mutex::new(());
const SCIM_TEST_TOKEN: &str = "ar04-scim-test-token";

/// Serialize `RUSTSHARE_SCIM_*` env mutation and restore the previous values
/// on drop so other suites in this binary keep a pristine environment.
struct ScimEnv {
    _guard: std::sync::MutexGuard<'static, ()>,
    token: Option<String>,
    tenant: Option<String>,
    default_tenant: Option<String>,
}

impl ScimEnv {
    fn configure(tenant_id: Uuid) -> Self {
        // Poison-resistant: a panicking sibling test must not disable SCIM
        // coverage for the rest of the binary.
        let guard = SCIM_ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let token = std::env::var("RUSTSHARE_SCIM_BEARER_TOKEN").ok();
        let tenant = std::env::var("RUSTSHARE_SCIM_TENANT_ID").ok();
        let default_tenant = std::env::var("RUSTSHARE_DEFAULT_TENANT_ID").ok();
        std::env::set_var("RUSTSHARE_SCIM_BEARER_TOKEN", SCIM_TEST_TOKEN);
        std::env::set_var("RUSTSHARE_SCIM_TENANT_ID", tenant_id.to_string());
        std::env::remove_var("RUSTSHARE_DEFAULT_TENANT_ID");
        Self {
            _guard: guard,
            token,
            tenant,
            default_tenant,
        }
    }
}

impl Drop for ScimEnv {
    fn drop(&mut self) {
        for (key, value) in [
            ("RUSTSHARE_SCIM_BEARER_TOKEN", &self.token),
            ("RUSTSHARE_SCIM_TENANT_ID", &self.tenant),
            ("RUSTSHARE_DEFAULT_TENANT_ID", &self.default_tenant),
        ] {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
    }
}

fn scim_headers() -> axum::http::HeaderMap {
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        axum::http::header::AUTHORIZATION,
        axum::http::HeaderValue::from_str(&format!("Bearer {SCIM_TEST_TOKEN}")).unwrap(),
    );
    headers
}

fn bad_scim_headers() -> axum::http::HeaderMap {
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        axum::http::header::AUTHORIZATION,
        axum::http::HeaderValue::from_static("Bearer wrong-token"),
    );
    headers
}

// ---------------------------------------------------------------------------
// Existing share-lifecycle coverage (user shares)
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn test_non_note_share_refresh_does_not_index_file() {
    let (state, sink) = setup_with_recording_sink().await;
    let tenant_id = insert_test_tenant(&state.db_pool).await;
    let owner_id = Uuid::new_v4();
    insert_test_user(&state.db_pool, tenant_id, owner_id, "pdf-owner@example.com").await;
    let file_id = insert_test_pdf(&state.db_pool, tenant_id, owner_id).await;

    state
        .note_service
        .refresh_note_index_acl(file_id, owner_id, tenant_id)
        .await
        .unwrap();

    assert!(sink.take_indexed().is_empty());
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
    let file_id = create_test_note(&state, &sink, tenant_id, owner_id).await;

    rustshare_server::handlers::user_shares::create_file_share(
        State(state.clone()),
        Path(file_id),
        rustshare_server::handlers::AuthenticatedUser {
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

    wait_for_calls_for(&sink, file_id, 1).await;
    let indexed = sink.take_indexed();
    assert!(
        indexed.iter().any(|record| record.note_id == file_id),
        "Share creation should trigger AI index ACL refresh for the file"
    );
}

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn test_folder_share_refreshes_descendant_note_acl() {
    let (state, sink) = setup_with_recording_sink().await;
    let tenant_id = insert_test_tenant(&state.db_pool).await;
    let owner_id = Uuid::new_v4();
    let recipient_id = Uuid::new_v4();
    insert_test_user(
        &state.db_pool,
        tenant_id,
        owner_id,
        "folder-owner@example.com",
    )
    .await;
    insert_test_user(
        &state.db_pool,
        tenant_id,
        recipient_id,
        "folder-recipient@example.com",
    )
    .await;
    let parent = state
        .folder_service
        .create_folder("Shared".to_string(), None, owner_id, tenant_id)
        .await
        .unwrap();
    let child = state
        .folder_service
        .create_folder("Nested".to_string(), Some(parent.id), owner_id, tenant_id)
        .await
        .unwrap();
    let note = state
        .note_service
        .create_note(
            owner_id,
            tenant_id,
            Some("Nested note".to_string()),
            Some(child.id),
            Some("# Nested note".to_string()),
        )
        .await
        .unwrap();
    let _ = sink.take_indexed();

    rustshare_server::handlers::user_shares::create_folder_share(
        State(state.clone()),
        Path(parent.id),
        rustshare_server::handlers::AuthenticatedUser {
            user_id: owner_id,
            tenant_id,
        },
        ValidatedJson(
            rustshare_server::handlers::user_shares::CreateFolderShareRequest {
                recipient_email: "folder-recipient@example.com".to_string(),
                permission: SharePermissions::View,
            },
        ),
    )
    .await
    .unwrap();

    wait_for_calls_for(&sink, note.id, 1).await;
    assert!(
        sink.take_indexed().iter().any(|r| r.note_id == note.id),
        "Folder share should refresh descendant note ACLs"
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
    let file_id = create_test_note(&state, &sink, tenant_id, owner_id).await;

    let response = rustshare_server::handlers::user_shares::create_file_share(
        State(state.clone()),
        Path(file_id),
        rustshare_server::handlers::AuthenticatedUser {
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
    wait_for_calls_for(&sink, file_id, 1).await;
    let _ = sink.take_indexed();

    rustshare_server::handlers::user_shares::update_recipient_permission(
        State(state.clone()),
        Path(share_id),
        rustshare_server::handlers::AuthenticatedUser {
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

    wait_for_calls_for(&sink, file_id, 1).await;
    let indexed = sink.take_indexed();
    assert!(
        indexed.iter().any(|record| record.note_id == file_id),
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
    let file_id = create_test_note(&state, &sink, tenant_id, owner_id).await;

    let response = rustshare_server::handlers::user_shares::create_file_share(
        State(state.clone()),
        Path(file_id),
        rustshare_server::handlers::AuthenticatedUser {
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

    wait_for_calls_for(&sink, file_id, 1).await;
    let _ = sink.take_indexed();

    rustshare_server::handlers::user_shares::remove_recipient(
        State(state.clone()),
        Path(share_id),
        rustshare_server::handlers::AuthenticatedUser {
            user_id: owner_id,
            tenant_id,
        },
    )
    .await
    .unwrap();

    wait_for_calls_for(&sink, file_id, 1).await;
    let indexed = sink.take_indexed();
    assert!(
        indexed.iter().any(|record| record.note_id == file_id),
        "Share revocation should trigger AI index ACL refresh for the file"
    );
}

// ---------------------------------------------------------------------------
// AR-04 scenario 1 + 2: legacy nil-tenant group; admin add/remove refresh in
// the share's tenant. Fails if the original group-tenant filter is restored.
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn test_legacy_nil_tenant_group_admin_add_remove_refreshes_share_tenant() {
    let (state, sink) = setup_with_recording_sink().await;
    // The resource lives in a real tenant...
    let tenant_id = insert_test_tenant(&state.db_pool).await;
    let owner_id = Uuid::new_v4();
    let member_id = Uuid::new_v4();
    insert_test_user(
        &state.db_pool,
        tenant_id,
        owner_id,
        "ar04-owner@example.com",
    )
    .await;
    // ...the membership fixture is nil-tenant (post-AR-03 both legs must match).
    // The nil tenant is shared with every previous run, so the email must be
    // unique across runs (users are unique on (lower(email), tenant_id)).
    insert_test_user(
        &state.db_pool,
        Uuid::nil(),
        member_id,
        &format!("ar04-nil-member-{member_id}@example.com"),
    )
    .await;
    // Legacy group row: nil tenant.
    let group_id = insert_test_group(&state.db_pool, Uuid::nil(), None).await;
    let note_id = create_test_note(&state, &sink, tenant_id, owner_id).await;
    // Legacy share seeded directly: the SHARE's tenant is the real tenant even
    // though the group tenant is nil. The old group-tenant filter dropped this
    // row from refresh discovery.
    seed_group_share(
        &state.db_pool,
        ShareTarget::File(note_id),
        group_id,
        tenant_id,
        owner_id,
        false,
        None,
    )
    .await;

    admin_groups::add_member(
        State(state.clone()),
        AdminUser { user_id: owner_id },
        Path(group_id),
        axum::Json(admin_groups::AddMemberRequest { user_id: member_id }),
    )
    .await
    .unwrap();

    wait_for_calls_for(&sink, note_id, 1).await;
    let first = sink.calls_for(note_id)[0].clone();
    assert_eq!(
        first.tenant_id, tenant_id,
        "refresh must run in the share's tenant, not the group's nil tenant"
    );
    assert_eq!(first.owner_id, owner_id);
    assert!(
        first.principals.contains(&format!("group:{group_id}")),
        "projection must carry the group principal, got {}",
        principal_list(&first)
    );

    // Admin remove reaches the same resource in the share's tenant again.
    admin_groups::remove_member(
        State(state.clone()),
        AdminUser { user_id: owner_id },
        Path((group_id, member_id)),
    )
    .await
    .unwrap();

    wait_for_calls_for(&sink, note_id, 2).await;
    let calls = sink.calls_for(note_id);
    let second = calls[1].clone();
    assert_eq!(second.tenant_id, tenant_id);
    assert!(
        second.principals.contains(&format!("group:{group_id}")),
        "group principal projection is id-based and survives membership removal"
    );
}

// ---------------------------------------------------------------------------
// AR-04 scenario 3: group folder share (incl. nested notes) via admin mutation
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn test_group_folder_share_refreshes_nested_notes_via_admin_mutation() {
    let (state, sink) = setup_with_recording_sink().await;
    let tenant_id = insert_test_tenant(&state.db_pool).await;
    let owner_id = Uuid::new_v4();
    let member_id = Uuid::new_v4();
    insert_test_user(
        &state.db_pool,
        tenant_id,
        owner_id,
        "ar04-f-owner@example.com",
    )
    .await;
    insert_test_user(
        &state.db_pool,
        tenant_id,
        member_id,
        "ar04-f-member@example.com",
    )
    .await;
    let group_id = insert_test_group(&state.db_pool, tenant_id, None).await;

    let parent = state
        .folder_service
        .create_folder("AR04Shared".to_string(), None, owner_id, tenant_id)
        .await
        .unwrap();
    let child = state
        .folder_service
        .create_folder(
            "AR04Nested".to_string(),
            Some(parent.id),
            owner_id,
            tenant_id,
        )
        .await
        .unwrap();
    let note = state
        .note_service
        .create_note(
            owner_id,
            tenant_id,
            Some("AR04 nested note".to_string()),
            Some(child.id),
            Some("# AR04 nested note".to_string()),
        )
        .await
        .unwrap();
    let _ = sink.take_indexed();

    seed_group_share(
        &state.db_pool,
        ShareTarget::Folder(parent.id),
        group_id,
        tenant_id,
        owner_id,
        false,
        None,
    )
    .await;

    admin_groups::add_member(
        State(state.clone()),
        AdminUser { user_id: owner_id },
        Path(group_id),
        axum::Json(admin_groups::AddMemberRequest { user_id: member_id }),
    )
    .await
    .unwrap();

    wait_for_calls_for(&sink, note.id, 1).await;
    let record = sink.calls_for(note.id)[0].clone();
    assert_eq!(record.tenant_id, tenant_id);
    assert!(
        record.principals.contains(&format!("group:{group_id}")),
        "nested note projection must carry the folder share's group principal"
    );
}

// ---------------------------------------------------------------------------
// AR-04 scenario 4 + 5: admin deletion refreshes saved targets after commit;
// the deleted group principal disappears while other grants remain.
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn test_admin_group_deletion_removes_principal_after_commit() {
    let (state, sink) = setup_with_recording_sink().await;
    let tenant_id = insert_test_tenant(&state.db_pool).await;
    let owner_id = Uuid::new_v4();
    let member_id = Uuid::new_v4();
    let other_user = Uuid::new_v4();
    insert_test_user(
        &state.db_pool,
        tenant_id,
        owner_id,
        "ar04-d-owner@example.com",
    )
    .await;
    insert_test_user(
        &state.db_pool,
        tenant_id,
        member_id,
        "ar04-d-member@example.com",
    )
    .await;
    insert_test_user(
        &state.db_pool,
        tenant_id,
        other_user,
        "ar04-d-other@example.com",
    )
    .await;
    let group_id = insert_test_group(&state.db_pool, tenant_id, None).await;
    insert_group_member_direct(&state.db_pool, group_id, member_id).await;
    let note_id = create_test_note(&state, &sink, tenant_id, owner_id).await;
    seed_group_share(
        &state.db_pool,
        ShareTarget::File(note_id),
        group_id,
        tenant_id,
        owner_id,
        false,
        None,
    )
    .await;
    // An unrelated user grant that must survive the group deletion.
    let user_share_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO shares (id, file_id, recipient_user_id, permissions, created_by, tenant_id, created_at)
        VALUES ($1, $2, $3, 'View', $4, $5, NOW())
        "#,
    )
    .bind(user_share_id)
    .bind(note_id)
    .bind(other_user)
    .bind(owner_id)
    .bind(tenant_id)
    .execute(&state.db_pool)
    .await
    .unwrap();

    admin_groups::delete_group(
        State(state.clone()),
        AdminUser { user_id: owner_id },
        Path(group_id),
    )
    .await
    .unwrap();

    // The deletion dispatched a refresh that rebuilt the projection from the
    // committed state: no group principal, owner and user grant remain.
    wait_for_calls_for(&sink, note_id, 1).await;
    let record = sink.calls_for(note_id)[0].clone();
    assert_eq!(record.tenant_id, tenant_id);
    assert!(
        !record.principals.contains(&format!("group:{group_id}")),
        "deleted group principal must be gone, got {}",
        principal_list(&record)
    );
    assert!(
        record.principals.contains(&format!("owner:{owner_id}")),
        "owner grant must remain, got {}",
        principal_list(&record)
    );
    assert!(
        record.principals.contains(&format!("user:{other_user}")),
        "unrelated user grant must remain, got {}",
        principal_list(&record)
    );
    assert!(
        sink.update_acl_calls() == 0 && sink.remove_note_calls() == 0,
        "group refresh path only re-indexes content"
    );
}

// ---------------------------------------------------------------------------
// AR-04 scenario 6: refresh independent of the share creator retaining access.
// Fails if the creator-dependent refresh context is restored.
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn test_group_refresh_independent_of_share_creator_access() {
    let (state, sink) = setup_with_recording_sink().await;
    let tenant_id = insert_test_tenant(&state.db_pool).await;
    let owner_id = Uuid::new_v4();
    let member_id = Uuid::new_v4();
    // The historical share creator: a real user in the tenant with NO grant
    // on the resource (not the owner, not a recipient).
    let former_creator = Uuid::new_v4();
    insert_test_user(
        &state.db_pool,
        tenant_id,
        owner_id,
        "ar04-c-owner@example.com",
    )
    .await;
    insert_test_user(
        &state.db_pool,
        tenant_id,
        member_id,
        "ar04-c-member@example.com",
    )
    .await;
    insert_test_user(
        &state.db_pool,
        tenant_id,
        former_creator,
        "ar04-c-former@example.com",
    )
    .await;
    let group_id = insert_test_group(&state.db_pool, tenant_id, None).await;
    let note_id = create_test_note(&state, &sink, tenant_id, owner_id).await;
    seed_group_share(
        &state.db_pool,
        ShareTarget::File(note_id),
        group_id,
        tenant_id,
        former_creator,
        false,
        None,
    )
    .await;

    admin_groups::add_member(
        State(state.clone()),
        AdminUser { user_id: owner_id },
        Path(group_id),
        axum::Json(admin_groups::AddMemberRequest { user_id: member_id }),
    )
    .await
    .unwrap();

    wait_for_calls_for(&sink, note_id, 1).await;
    let record = sink.calls_for(note_id)[0].clone();
    assert_eq!(record.tenant_id, tenant_id);
    assert!(
        record.principals.contains(&format!("group:{group_id}")),
        "refresh must succeed using the resource owner context even though the \
         share creator has no access, got {}",
        principal_list(&record)
    );

    // Same guarantee on the deletion path.
    admin_groups::delete_group(
        State(state.clone()),
        AdminUser { user_id: owner_id },
        Path(group_id),
    )
    .await
    .unwrap();

    wait_for_calls_for(&sink, note_id, 2).await;
    let after_delete = sink.calls_for(note_id)[1].clone();
    assert!(
        !after_delete
            .principals
            .contains(&format!("group:{group_id}")),
        "group principal must be gone after deletion"
    );
}

// ---------------------------------------------------------------------------
// AR-04 scenario 7: revoked group-share rows excluded from target discovery
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn test_revoked_group_shares_excluded_from_refresh_discovery() {
    let (state, sink) = setup_with_recording_sink().await;
    let tenant_id = insert_test_tenant(&state.db_pool).await;
    let owner_id = Uuid::new_v4();
    let member_id = Uuid::new_v4();
    insert_test_user(
        &state.db_pool,
        tenant_id,
        owner_id,
        "ar04-r-owner@example.com",
    )
    .await;
    insert_test_user(
        &state.db_pool,
        tenant_id,
        member_id,
        "ar04-r-member@example.com",
    )
    .await;
    let group_id = insert_test_group(&state.db_pool, tenant_id, None).await;
    let active_note = create_test_note(&state, &sink, tenant_id, owner_id).await;
    let revoked_note = create_test_note(&state, &sink, tenant_id, owner_id).await;

    seed_group_share(
        &state.db_pool,
        ShareTarget::File(active_note),
        group_id,
        tenant_id,
        owner_id,
        false,
        None,
    )
    .await;
    seed_group_share(
        &state.db_pool,
        ShareTarget::File(revoked_note),
        group_id,
        tenant_id,
        owner_id,
        true,
        None,
    )
    .await;

    admin_groups::add_member(
        State(state.clone()),
        AdminUser { user_id: owner_id },
        Path(group_id),
        axum::Json(admin_groups::AddMemberRequest { user_id: member_id }),
    )
    .await
    .unwrap();

    wait_for_calls_for(&sink, active_note, 1).await;
    assert!(
        sink.calls_for(revoked_note).is_empty(),
        "revoked group share must not be refreshed"
    );
    assert_eq!(
        sink.calls_for(active_note).len(),
        1,
        "exactly one refresh for the active share target"
    );
}

// ---------------------------------------------------------------------------
// AR-04 scenario 8: expired grants never appear in rebuilt projections
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn test_expired_group_share_not_in_rebuilt_projection() {
    let (state, sink) = setup_with_recording_sink().await;
    let tenant_id = insert_test_tenant(&state.db_pool).await;
    let owner_id = Uuid::new_v4();
    let member_id = Uuid::new_v4();
    insert_test_user(
        &state.db_pool,
        tenant_id,
        owner_id,
        "ar04-e-owner@example.com",
    )
    .await;
    insert_test_user(
        &state.db_pool,
        tenant_id,
        member_id,
        "ar04-e-member@example.com",
    )
    .await;
    let group_id = insert_test_group(&state.db_pool, tenant_id, None).await;
    let active_note = create_test_note(&state, &sink, tenant_id, owner_id).await;
    let expired_note = create_test_note(&state, &sink, tenant_id, owner_id).await;

    seed_group_share(
        &state.db_pool,
        ShareTarget::File(active_note),
        group_id,
        tenant_id,
        owner_id,
        false,
        None,
    )
    .await;
    // Expired but not revoked: discovered as a target, but the resolver must
    // exclude the grant from the rebuilt projection.
    seed_group_share(
        &state.db_pool,
        ShareTarget::File(expired_note),
        group_id,
        tenant_id,
        owner_id,
        false,
        Some(Utc::now() - chrono::Duration::hours(1)),
    )
    .await;

    admin_groups::add_member(
        State(state.clone()),
        AdminUser { user_id: owner_id },
        Path(group_id),
        axum::Json(admin_groups::AddMemberRequest { user_id: member_id }),
    )
    .await
    .unwrap();

    wait_for_calls_for(&sink, active_note, 1).await;
    wait_for_calls_for(&sink, expired_note, 1).await;
    let active = sink.calls_for(active_note)[0].clone();
    let expired = sink.calls_for(expired_note)[0].clone();
    assert!(
        active.principals.contains(&format!("group:{group_id}")),
        "active share keeps the group principal"
    );
    assert!(
        !expired.principals.contains(&format!("group:{group_id}")),
        "expired grant must not appear in the rebuilt projection, got {}",
        principal_list(&expired)
    );
    assert!(
        expired.principals.contains(&format!("owner:{owner_id}")),
        "owner principal remains for the expired-share note"
    );
}

// ---------------------------------------------------------------------------
// AR-04 scenarios 9 + 10: share/resource tenant mismatch rejected without
// indexing cross-organization content (file and folder targets)
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn test_file_share_tenant_mismatch_not_indexed() {
    let (state, sink) = setup_with_recording_sink().await;
    let share_tenant = insert_test_tenant(&state.db_pool).await;
    let resource_tenant = insert_test_tenant(&state.db_pool).await;
    let owner_id = Uuid::new_v4();
    let member_id = Uuid::new_v4();
    insert_test_user(
        &state.db_pool,
        resource_tenant,
        owner_id,
        "ar04-m-owner@example.com",
    )
    .await;
    insert_test_user(
        &state.db_pool,
        share_tenant,
        member_id,
        "ar04-m-member@example.com",
    )
    .await;
    let group_id = insert_test_group(&state.db_pool, share_tenant, None).await;
    let note_id = create_test_note(&state, &sink, resource_tenant, owner_id).await;
    // Legacy mismatch fixture: the share claims the resource lives in
    // `share_tenant`, but the file actually lives in `resource_tenant`.
    seed_group_share(
        &state.db_pool,
        ShareTarget::File(note_id),
        group_id,
        share_tenant,
        owner_id,
        false,
        None,
    )
    .await;

    admin_groups::add_member(
        State(state.clone()),
        AdminUser { user_id: owner_id },
        Path(group_id),
        axum::Json(admin_groups::AddMemberRequest { user_id: member_id }),
    )
    .await
    .unwrap();

    // The mutation committed; a bounded window proves no cross-tenant indexing
    // followed. A direct control refresh then proves the sink is alive.
    assert_no_new_calls(&sink, 0, Duration::from_millis(500)).await;
    state
        .note_service
        .refresh_note_index_acl(note_id, owner_id, resource_tenant)
        .await
        .unwrap();
    wait_for_calls_for(&sink, note_id, 1).await;
    let record = sink.calls_for(note_id)[0].clone();
    assert_eq!(
        record.tenant_id, resource_tenant,
        "control refresh runs in the resource's own tenant"
    );
}

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn test_folder_share_tenant_mismatch_not_indexed() {
    let (state, sink) = setup_with_recording_sink().await;
    let share_tenant = insert_test_tenant(&state.db_pool).await;
    let resource_tenant = insert_test_tenant(&state.db_pool).await;
    let owner_id = Uuid::new_v4();
    let member_id = Uuid::new_v4();
    insert_test_user(
        &state.db_pool,
        resource_tenant,
        owner_id,
        "ar04-fm-owner@example.com",
    )
    .await;
    insert_test_user(
        &state.db_pool,
        share_tenant,
        member_id,
        "ar04-fm-member@example.com",
    )
    .await;
    let group_id = insert_test_group(&state.db_pool, share_tenant, None).await;

    let folder = state
        .folder_service
        .create_folder(
            "AR04MismatchFolder".to_string(),
            None,
            owner_id,
            resource_tenant,
        )
        .await
        .unwrap();
    let note = state
        .note_service
        .create_note(
            owner_id,
            resource_tenant,
            Some("AR04 mismatch note".to_string()),
            Some(folder.id),
            Some("# AR04 mismatch note".to_string()),
        )
        .await
        .unwrap();
    let _ = sink.take_indexed();

    seed_group_share(
        &state.db_pool,
        ShareTarget::Folder(folder.id),
        group_id,
        share_tenant,
        owner_id,
        false,
        None,
    )
    .await;

    admin_groups::add_member(
        State(state.clone()),
        AdminUser { user_id: owner_id },
        Path(group_id),
        axum::Json(admin_groups::AddMemberRequest { user_id: member_id }),
    )
    .await
    .unwrap();

    assert_no_new_calls(&sink, 0, Duration::from_millis(500)).await;
    state
        .note_service
        .refresh_note_index_acl(note.id, owner_id, resource_tenant)
        .await
        .unwrap();
    wait_for_calls_for(&sink, note.id, 1).await;
}

// ---------------------------------------------------------------------------
// AR-04 scenario 11: SCIM v1 provisioning + deletion refresh
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn test_scim_v1_provision_and_delete_refresh_group_shares() {
    let (state, sink) = setup_with_recording_sink().await;
    let tenant_id = insert_test_tenant(&state.db_pool).await;
    let owner_id = Uuid::new_v4();
    let member_id = Uuid::new_v4();
    let group_external = format!("ar04-v1-group-{}", Uuid::new_v4());
    let member_external = format!("ar04-v1-member-{}", Uuid::new_v4());
    insert_test_user(
        &state.db_pool,
        tenant_id,
        owner_id,
        "ar04-s1-owner@example.com",
    )
    .await;
    insert_test_user_with_external(
        &state.db_pool,
        tenant_id,
        member_id,
        "ar04-s1-member@example.com",
        &member_external,
    )
    .await;
    let group_id = insert_test_group(&state.db_pool, tenant_id, Some(&group_external)).await;
    let note_id = create_test_note(&state, &sink, tenant_id, owner_id).await;
    seed_group_share(
        &state.db_pool,
        ShareTarget::File(note_id),
        group_id,
        tenant_id,
        owner_id,
        false,
        None,
    )
    .await;

    let _env = ScimEnv::configure(tenant_id);

    // v1 upsert with a membership sync.
    let result = scim::provision_group(
        State(state.clone()),
        scim_headers(),
        axum::Json(ScimGroup {
            external_id: group_external.clone(),
            display_name: format!("AR04 SCIM v1 group {group_external}"),
            members: Some(vec![ScimMember {
                value: member_external.clone(),
            }]),
        }),
    )
    .await;
    let (status, _) = result.unwrap();
    assert_eq!(status, StatusCode::OK);

    wait_for_calls_for(&sink, note_id, 1).await;
    let record = sink.calls_for(note_id)[0].clone();
    assert_eq!(record.tenant_id, tenant_id);
    assert!(
        record.principals.contains(&format!("group:{group_id}")),
        "v1 provisioning refresh must reach the share's resource"
    );

    // v1 deletion refreshes the saved target and drops the group principal.
    let delete_status = scim::delete_group(
        State(state.clone()),
        scim_headers(),
        Path(group_external.clone()),
    )
    .await;
    assert_eq!(delete_status.unwrap(), StatusCode::NO_CONTENT);

    wait_for_calls_for(&sink, note_id, 2).await;
    let after_delete = sink.calls_for(note_id)[1].clone();
    assert_eq!(after_delete.tenant_id, tenant_id);
    assert!(
        !after_delete
            .principals
            .contains(&format!("group:{group_id}")),
        "v1 deletion refresh must rebuild without the deleted group principal"
    );
}

// ---------------------------------------------------------------------------
// AR-04 scenario 12: SCIM v2 PUT replacement, PATCH, and deletion refresh
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn test_scim_v2_put_patch_and_delete_refresh_group_shares() {
    use rustshare_core::services::{ScimV2Group, ScimV2Member};

    let (state, sink) = setup_with_recording_sink().await;
    let tenant_id = insert_test_tenant(&state.db_pool).await;
    let owner_id = Uuid::new_v4();
    let member_one = Uuid::new_v4();
    let member_two = Uuid::new_v4();
    insert_test_user(
        &state.db_pool,
        tenant_id,
        owner_id,
        "ar04-s2-owner@example.com",
    )
    .await;
    insert_test_user(
        &state.db_pool,
        tenant_id,
        member_one,
        "ar04-s2-m1@example.com",
    )
    .await;
    insert_test_user(
        &state.db_pool,
        tenant_id,
        member_two,
        "ar04-s2-m2@example.com",
    )
    .await;
    let group_id = insert_test_group(&state.db_pool, tenant_id, None).await;
    let note_id = create_test_note(&state, &sink, tenant_id, owner_id).await;
    seed_group_share(
        &state.db_pool,
        ShareTarget::File(note_id),
        group_id,
        tenant_id,
        owner_id,
        false,
        None,
    )
    .await;

    let _env = ScimEnv::configure(tenant_id);

    // PUT replacement: one batched refresh for the whole group, not per member.
    let put_group = ScimV2Group {
        members: Some(vec![
            ScimV2Member {
                value: member_one.to_string(),
                ref_url: None,
                display: None,
                member_type: None,
            },
            ScimV2Member {
                value: member_two.to_string(),
                ref_url: None,
                display: None,
                member_type: None,
            },
        ]),
        ..ScimV2Group::new(format!("AR04 v2 put group {group_id}"))
    };
    let response = scim_v2::update_group(
        State(state.clone()),
        scim_headers(),
        Path(group_id),
        axum::Json(put_group),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    wait_for_calls_for(&sink, note_id, 1).await;

    // PATCH add.
    let patch_add = scim_v2::patch_group(
        State(state.clone()),
        scim_headers(),
        Path(group_id),
        axum::Json(ScimPatchRequest {
            schemas: vec!["urn:ietf:params:scim:api:messages:2.0:PatchOp".to_string()],
            operations: vec![ScimPatchOperation {
                op: "add".to_string(),
                path: Some("members".to_string()),
                value: Some(serde_json::json!([{"value": member_one.to_string()}])),
            }],
        }),
    )
    .await;
    assert_eq!(patch_add.status(), StatusCode::OK);
    wait_for_calls_for(&sink, note_id, 2).await;

    // PATCH remove.
    let patch_remove = scim_v2::patch_group(
        State(state.clone()),
        scim_headers(),
        Path(group_id),
        axum::Json(ScimPatchRequest {
            schemas: vec!["urn:ietf:params:scim:api:messages:2.0:PatchOp".to_string()],
            operations: vec![ScimPatchOperation {
                op: "remove".to_string(),
                path: Some(format!("members[value eq \"{}\"]", member_two)),
                value: None,
            }],
        }),
    )
    .await;
    assert_eq!(patch_remove.status(), StatusCode::OK);
    wait_for_calls_for(&sink, note_id, 3).await;

    // v2 deletion refreshes the saved target after commit.
    let delete = scim_v2::delete_group(State(state.clone()), scim_headers(), Path(group_id)).await;
    assert_eq!(delete.status(), StatusCode::NO_CONTENT);
    wait_for_calls_for(&sink, note_id, 4).await;
    let after_delete = sink.calls_for(note_id)[3].clone();
    assert!(
        !after_delete
            .principals
            .contains(&format!("group:{group_id}")),
        "v2 deletion refresh must drop the group principal"
    );
}

// ---------------------------------------------------------------------------
// AR-04 scenario 13: a later SCIM failure after an earlier committed
// membership write still refreshes (v2 PUT and v1 provisioning)
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn test_scim_v2_put_failure_after_committed_membership_still_refreshes() {
    use rustshare_core::services::{ScimV2Group, ScimV2Member};

    let (state, sink) = setup_with_recording_sink().await;
    let tenant_id = insert_test_tenant(&state.db_pool).await;
    let owner_id = Uuid::new_v4();
    let kept_member = Uuid::new_v4();
    let replaced_member = Uuid::new_v4();
    insert_test_user(
        &state.db_pool,
        tenant_id,
        owner_id,
        "ar04-s3-owner@example.com",
    )
    .await;
    insert_test_user(
        &state.db_pool,
        tenant_id,
        kept_member,
        "ar04-s3-kept@example.com",
    )
    .await;
    insert_test_user(
        &state.db_pool,
        tenant_id,
        replaced_member,
        "ar04-s3-replaced@example.com",
    )
    .await;
    let group_id = insert_test_group(&state.db_pool, tenant_id, None).await;
    insert_group_member_direct(&state.db_pool, group_id, replaced_member).await;
    let note_id = create_test_note(&state, &sink, tenant_id, owner_id).await;
    seed_group_share(
        &state.db_pool,
        ShareTarget::File(note_id),
        group_id,
        tenant_id,
        owner_id,
        false,
        None,
    )
    .await;

    let _env = ScimEnv::configure(tenant_id);

    // The second member's external id contains a NUL byte: PostgreSQL rejects
    // the lookup with a database error AFTER the replacement (clear + add of
    // the first member) has already committed.
    let put_group = ScimV2Group {
        members: Some(vec![
            ScimV2Member {
                value: kept_member.to_string(),
                ref_url: None,
                display: None,
                member_type: None,
            },
            ScimV2Member {
                value: "bad-external\u{0}id".to_string(),
                ref_url: None,
                display: None,
                member_type: None,
            },
        ]),
        ..ScimV2Group::new(format!("AR04 v2 failing put {group_id}"))
    };
    let response = scim_v2::update_group(
        State(state.clone()),
        scim_headers(),
        Path(group_id),
        axum::Json(put_group),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::INTERNAL_SERVER_ERROR,
        "the response step fails after committed membership writes"
    );

    // The committed replacement is visible...
    let member_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM group_members gm WHERE gm.group_id = $1 AND EXISTS \
            (SELECT 1 FROM users u WHERE u.id = gm.user_id AND u.tenant_id = $2)",
    )
    .bind(group_id)
    .bind(tenant_id)
    .fetch_one(&state.db_pool)
    .await
    .unwrap();
    assert_eq!(
        member_count, 1,
        "PUT clear + first member add committed before the failure"
    );

    // ...and the ACL refresh for the group was still dispatched.
    wait_for_calls_for(&sink, note_id, 1).await;
    let record = sink.calls_for(note_id)[0].clone();
    assert_eq!(record.tenant_id, tenant_id);
    assert!(
        record.principals.contains(&format!("group:{group_id}")),
        "refresh must run for the group whose membership committed"
    );
}

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn test_scim_v1_provision_failure_after_committed_membership_still_refreshes() {
    let (state, sink) = setup_with_recording_sink().await;
    let tenant_id = insert_test_tenant(&state.db_pool).await;
    let owner_id = Uuid::new_v4();
    let member_id = Uuid::new_v4();
    let member_external = format!("ar04-v1f-member-{}", Uuid::new_v4());
    insert_test_user(
        &state.db_pool,
        tenant_id,
        owner_id,
        "ar04-s3b-owner@example.com",
    )
    .await;
    insert_test_user_with_external(
        &state.db_pool,
        tenant_id,
        member_id,
        "ar04-s3b-member@example.com",
        &member_external,
    )
    .await;
    let note_id = create_test_note(&state, &sink, tenant_id, owner_id).await;

    let _env = ScimEnv::configure(tenant_id);

    // Provision #1 (create branch): the group INSERT and the first member add
    // commit; the NUL-byte external id then fails the member lookup and the
    // whole provision returns 500.
    let group_external = format!("ar04-v1f-group-{}", Uuid::new_v4());
    let failing_members = || {
        vec![
            ScimMember {
                value: member_external.clone(),
            },
            ScimMember {
                value: "bad-external\u{0}id".to_string(),
            },
        ]
    };
    let result = scim::provision_group(
        State(state.clone()),
        scim_headers(),
        axum::Json(ScimGroup {
            external_id: group_external.clone(),
            display_name: format!("AR04 v1 failing provision {group_external}"),
            members: Some(failing_members()),
        }),
    )
    .await;
    assert_eq!(
        result.unwrap_err().0,
        StatusCode::INTERNAL_SERVER_ERROR,
        "the member lookup fails after the group and first member committed"
    );

    let group_id: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM user_groups WHERE external_id = $1 AND tenant_id = $2")
            .bind(&group_external)
            .bind(tenant_id)
            .fetch_optional(&state.db_pool)
            .await
            .unwrap();
    let group_id = group_id.expect("group creation committed before the failure");

    // The committed membership write is visible even though the request
    // returned an error.
    let member_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM group_members gm WHERE gm.group_id = $1 AND EXISTS \
            (SELECT 1 FROM users u WHERE u.id = gm.user_id AND u.tenant_id = $2)",
    )
    .bind(group_id)
    .bind(tenant_id)
    .fetch_one(&state.db_pool)
    .await
    .unwrap();
    assert_eq!(
        member_count, 1,
        "first member add committed before the failure"
    );

    // Provision #2 on the now-known group (update branch): the display-name
    // update commits, then the same NUL-byte lookup fails the request again.
    // The handler must still dispatch the group ACL refresh, which discovers
    // the share seeded below.
    seed_group_share(
        &state.db_pool,
        ShareTarget::File(note_id),
        group_id,
        tenant_id,
        owner_id,
        false,
        None,
    )
    .await;
    let result = scim::provision_group(
        State(state.clone()),
        scim_headers(),
        axum::Json(ScimGroup {
            external_id: group_external.clone(),
            display_name: format!("AR04 v1 failing provision {group_external}"),
            members: Some(failing_members()),
        }),
    )
    .await;
    assert_eq!(
        result.unwrap_err().0,
        StatusCode::INTERNAL_SERVER_ERROR,
        "update-branch provision fails at the member lookup as well"
    );

    wait_for_calls_for(&sink, note_id, 1).await;
    let record = sink.calls_for(note_id)[0].clone();
    assert_eq!(record.tenant_id, tenant_id);
    assert!(
        record.principals.contains(&format!("group:{group_id}")),
        "refresh must run even though provisioning returned an error"
    );
}

// ---------------------------------------------------------------------------
// AR-04 scenario 14: rejected requests (unauthorized, foreign-group,
// not-found) dispatch nothing
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn test_rejected_requests_dispatch_nothing() {
    let (state, sink) = setup_with_recording_sink().await;
    let tenant_id = insert_test_tenant(&state.db_pool).await;
    let foreign_tenant = insert_test_tenant(&state.db_pool).await;
    let owner_id = Uuid::new_v4();
    let member_id = Uuid::new_v4();
    insert_test_user(
        &state.db_pool,
        tenant_id,
        owner_id,
        "ar04-s4-owner@example.com",
    )
    .await;
    insert_test_user(
        &state.db_pool,
        tenant_id,
        member_id,
        "ar04-s4-member@example.com",
    )
    .await;
    let group_id = insert_test_group(&state.db_pool, tenant_id, None).await;
    let foreign_group = insert_test_group(&state.db_pool, foreign_tenant, None).await;
    let note_id = create_test_note(&state, &sink, tenant_id, owner_id).await;
    seed_group_share(
        &state.db_pool,
        ShareTarget::File(note_id),
        group_id,
        tenant_id,
        owner_id,
        false,
        None,
    )
    .await;

    let _env = ScimEnv::configure(tenant_id);

    // Unauthorized SCIM v1 provision (bad token) — rejected before any state.
    let unauthorized = scim::provision_group(
        State(state.clone()),
        bad_scim_headers(),
        axum::Json(ScimGroup {
            external_id: "ar04-unauthorized".to_string(),
            display_name: "AR04 unauthorized".to_string(),
            members: None,
        }),
    )
    .await;
    assert_eq!(unauthorized.unwrap_err().0, StatusCode::UNAUTHORIZED);

    // SCIM v2 PATCH on a group outside the configured organization.
    let foreign_patch = scim_v2::patch_group(
        State(state.clone()),
        scim_headers(),
        Path(foreign_group),
        axum::Json(ScimPatchRequest {
            schemas: vec!["urn:ietf:params:scim:api:messages:2.0:PatchOp".to_string()],
            operations: vec![ScimPatchOperation {
                op: "add".to_string(),
                path: Some("members".to_string()),
                value: Some(serde_json::json!([{"value": member_id.to_string()}])),
            }],
        }),
    )
    .await;
    assert_eq!(foreign_patch.status(), StatusCode::NOT_FOUND);

    // SCIM v2 PATCH on a nonexistent group.
    let missing_patch = scim_v2::patch_group(
        State(state.clone()),
        scim_headers(),
        Path(Uuid::new_v4()),
        axum::Json(ScimPatchRequest {
            schemas: vec!["urn:ietf:params:scim:api:messages:2.0:PatchOp".to_string()],
            operations: vec![ScimPatchOperation {
                op: "add".to_string(),
                path: Some("members".to_string()),
                value: Some(serde_json::json!([{"value": member_id.to_string()}])),
            }],
        }),
    )
    .await;
    assert_eq!(missing_patch.status(), StatusCode::NOT_FOUND);

    // SCIM v1 delete of a nonexistent external id.
    let missing_delete = scim::delete_group(
        State(state.clone()),
        scim_headers(),
        Path("ar04-no-such-group".to_string()),
    )
    .await;
    assert_eq!(missing_delete.unwrap_err().0, StatusCode::NOT_FOUND);

    // Admin add to a missing group.
    let missing_group_add = admin_groups::add_member(
        State(state.clone()),
        AdminUser { user_id: owner_id },
        Path(Uuid::new_v4()),
        axum::Json(admin_groups::AddMemberRequest { user_id: member_id }),
    )
    .await;
    assert_eq!(
        missing_group_add.unwrap_err().into_response().status(),
        StatusCode::NOT_FOUND
    );

    // Admin remove of a non-membership.
    let missing_membership = admin_groups::remove_member(
        State(state.clone()),
        AdminUser { user_id: owner_id },
        Path((group_id, member_id)),
    )
    .await;
    assert_eq!(
        missing_membership.unwrap_err().into_response().status(),
        StatusCode::NOT_FOUND
    );

    // Admin delete of a missing group.
    let missing_group_delete = admin_groups::delete_group(
        State(state.clone()),
        AdminUser { user_id: owner_id },
        Path(Uuid::new_v4()),
    )
    .await;
    assert_eq!(
        missing_group_delete.unwrap_err().into_response().status(),
        StatusCode::NOT_FOUND
    );

    // All rejected requests completed synchronously; a bounded window proves
    // nothing was dispatched. A valid mutation afterwards proves the pipeline
    // is alive and counting correctly.
    assert_no_new_calls(&sink, 0, Duration::from_millis(500)).await;
    admin_groups::add_member(
        State(state.clone()),
        AdminUser { user_id: owner_id },
        Path(group_id),
        axum::Json(admin_groups::AddMemberRequest { user_id: member_id }),
    )
    .await
    .unwrap();
    wait_for_calls_for(&sink, note_id, 1).await;
}

// ---------------------------------------------------------------------------
// AR-04 scenario 15: a fully rolled-back deletion dispatches nothing
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn test_rolled_back_group_deletion_dispatches_nothing() {
    let (state, sink) = setup_with_recording_sink().await;
    let tenant_id = insert_test_tenant(&state.db_pool).await;
    let owner_id = Uuid::new_v4();
    insert_test_user(
        &state.db_pool,
        tenant_id,
        owner_id,
        "ar04-s5-owner@example.com",
    )
    .await;
    let group_id = insert_test_group(&state.db_pool, tenant_id, None).await;
    let note_id = create_test_note(&state, &sink, tenant_id, owner_id).await;
    seed_group_share(
        &state.db_pool,
        ShareTarget::File(note_id),
        group_id,
        tenant_id,
        owner_id,
        false,
        None,
    )
    .await;

    // The actor id does not exist, so the admin-actions insert violates its
    // foreign key and the whole deletion transaction (including the captured
    // refresh targets) rolls back.
    let nonexistent_actor = Uuid::new_v4();
    let result = admin_groups::delete_group(
        State(state.clone()),
        AdminUser {
            user_id: nonexistent_actor,
        },
        Path(group_id),
    )
    .await;
    assert_eq!(
        result.unwrap_err().into_response().status(),
        StatusCode::INTERNAL_SERVER_ERROR
    );

    // The group and its share both survived the rollback.
    let group_exists: Option<Uuid> = sqlx::query_scalar("SELECT id FROM user_groups WHERE id = $1")
        .bind(group_id)
        .fetch_optional(&state.db_pool)
        .await
        .unwrap();
    assert_eq!(group_exists, Some(group_id));
    assert_eq!(count_group_shares(&state.db_pool, group_id).await, 1);

    // No dispatch left the rolled-back transaction.
    assert_no_new_calls(&sink, 0, Duration::from_millis(500)).await;

    // Control: a direct refresh works, and a valid deletion then refreshes.
    state
        .note_service
        .refresh_note_index_acl(note_id, owner_id, tenant_id)
        .await
        .unwrap();
    wait_for_calls_for(&sink, note_id, 1).await;

    admin_groups::delete_group(
        State(state.clone()),
        AdminUser { user_id: owner_id },
        Path(group_id),
    )
    .await
    .unwrap();
    wait_for_calls_for(&sink, note_id, 2).await;
    let after_delete = sink.calls_for(note_id)[1].clone();
    assert!(
        !after_delete
            .principals
            .contains(&format!("group:{group_id}")),
        "committed deletion rebuilds without the group principal"
    );
}

// ---------------------------------------------------------------------------
// AR-04 scenario 16: concurrent share creation during deletion cannot leave
// an unrefreshed committed grant (group row lock serialization)
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn test_concurrent_share_creation_during_deletion_is_refreshed() {
    let (state, sink) = setup_with_recording_sink().await;
    let tenant_id = insert_test_tenant(&state.db_pool).await;
    let owner_id = Uuid::new_v4();
    insert_test_user(
        &state.db_pool,
        tenant_id,
        owner_id,
        "ar04-s6-owner@example.com",
    )
    .await;
    let group_id = insert_test_group(&state.db_pool, tenant_id, None).await;
    let original_note = create_test_note(&state, &sink, tenant_id, owner_id).await;
    seed_group_share(
        &state.db_pool,
        ShareTarget::File(original_note),
        group_id,
        tenant_id,
        owner_id,
        false,
        None,
    )
    .await;

    // Extra notes the concurrent creator will try to share with the group.
    let mut candidate_notes = Vec::new();
    for _ in 0..4 {
        candidate_notes.push(create_test_note(&state, &sink, tenant_id, owner_id).await);
    }

    // Hold the group row lock so both contenders block deterministically
    // until we release it.
    let mut lock_tx = state.db_pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM user_groups WHERE id = $1 FOR UPDATE")
        .bind(group_id)
        .execute(&mut *lock_tx)
        .await
        .unwrap();

    let deleter = {
        let state = state.clone();
        tokio::spawn(async move {
            admin_groups::delete_group(
                State(state),
                AdminUser { user_id: owner_id },
                Path(group_id),
            )
            .await
        })
    };

    let inserted = Arc::new(Mutex::new(Vec::new()));
    let creator = {
        let pool = state.db_pool.clone();
        let inserted = Arc::clone(&inserted);
        tokio::spawn(async move {
            for (attempt, note_id) in candidate_notes.iter().cycle().enumerate() {
                if attempt >= 50 {
                    break;
                }
                let share_id = Uuid::new_v4();
                let result = sqlx::query(
                    r#"
                    INSERT INTO shares (
                        id, file_id, recipient_group_id,
                        permissions, created_by, tenant_id, created_at
                    ) VALUES ($1, $2, $3, 'View', $4, $5, NOW())
                    ON CONFLICT DO NOTHING
                    "#,
                )
                .bind(share_id)
                .bind(note_id)
                .bind(group_id)
                .bind(owner_id)
                .bind(tenant_id)
                .execute(&pool)
                .await;
                match result {
                    Ok(outcome) => {
                        if outcome.rows_affected() > 0 {
                            inserted.lock().unwrap().push(*note_id);
                        }
                        tokio::time::sleep(Duration::from_millis(5)).await;
                    }
                    // Group is gone: creation must fail instead of leaving a
                    // committed grant behind.
                    Err(_) => break,
                }
            }
        })
    };

    // Let both contenders reach their blocking points, then release the lock.
    tokio::time::sleep(Duration::from_millis(300)).await;
    lock_tx.commit().await.unwrap();

    deleter.await.unwrap().unwrap();
    creator.await.unwrap();

    // Every share committed before the deletion was captured (thanks to the
    // row lock) and refreshed; the deletion then cascaded them all away.
    assert_eq!(
        count_group_shares(&state.db_pool, group_id).await,
        0,
        "no group share rows may survive the deletion"
    );
    let inserted = inserted.lock().unwrap().clone();
    wait_for_calls_for(&sink, original_note, 1).await;
    for note_id in &inserted {
        wait_for_calls_for(&sink, *note_id, 1).await;
    }
    // The rebuilt projection for the original note lost the group principal.
    let record = sink.calls_for(original_note)[0].clone();
    assert!(
        !record.principals.contains(&format!("group:{group_id}")),
        "no committed grant may remain unrefreshed; group principal must be gone"
    );
}

// ---------------------------------------------------------------------------
// AR-04 scenario 17: unchanged effective ACL preserves hash/version semantics
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn test_unchanged_effective_acl_preserves_hash_and_version() {
    let (state, sink) = setup_with_recording_sink().await;
    let tenant_id = insert_test_tenant(&state.db_pool).await;
    let owner_id = Uuid::new_v4();
    let member_one = Uuid::new_v4();
    let member_two = Uuid::new_v4();
    insert_test_user(
        &state.db_pool,
        tenant_id,
        owner_id,
        "ar04-s7-owner@example.com",
    )
    .await;
    insert_test_user(
        &state.db_pool,
        tenant_id,
        member_one,
        "ar04-s7-m1@example.com",
    )
    .await;
    insert_test_user(
        &state.db_pool,
        tenant_id,
        member_two,
        "ar04-s7-m2@example.com",
    )
    .await;
    let group_id = insert_test_group(&state.db_pool, tenant_id, None).await;
    let note_id = create_test_note(&state, &sink, tenant_id, owner_id).await;
    seed_group_share(
        &state.db_pool,
        ShareTarget::File(note_id),
        group_id,
        tenant_id,
        owner_id,
        false,
        None,
    )
    .await;

    admin_groups::add_member(
        State(state.clone()),
        AdminUser { user_id: owner_id },
        Path(group_id),
        axum::Json(admin_groups::AddMemberRequest {
            user_id: member_one,
        }),
    )
    .await
    .unwrap();
    wait_for_calls_for(&sink, note_id, 1).await;
    let first = sink.calls_for(note_id)[0].clone();

    // Swapping one member for another does not change the id-based group
    // principal, so the emitted projection keeps its hash and version: no
    // forced bump, no version persistence.
    admin_groups::remove_member(
        State(state.clone()),
        AdminUser { user_id: owner_id },
        Path((group_id, member_one)),
    )
    .await
    .unwrap();
    wait_for_calls_for(&sink, note_id, 2).await;
    admin_groups::add_member(
        State(state.clone()),
        AdminUser { user_id: owner_id },
        Path(group_id),
        axum::Json(admin_groups::AddMemberRequest {
            user_id: member_two,
        }),
    )
    .await
    .unwrap();
    wait_for_calls_for(&sink, note_id, 3).await;

    let calls = sink.calls_for(note_id);
    let second = calls[1].clone();
    let third = calls[2].clone();
    assert_eq!(first.acl_hash, second.acl_hash);
    assert_eq!(second.acl_hash, third.acl_hash);
    assert_eq!(first.acl_version, second.acl_version);
    assert_eq!(second.acl_version, third.acl_version);
    assert_eq!(first.principals, third.principals);
    assert!(
        third.principals.contains(&format!("group:{group_id}")),
        "group principal stays constant across membership churn"
    );
}
