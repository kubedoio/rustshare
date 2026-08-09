//! Integration tests for the cross-Application source-authorization contract
//! (GitHub issue #211).
//!
//! These exercise the `rustshare-resource-auth` contract
//! (`ResourceRef` / `PrincipalContext` / `SourceAuthorizer`) through the real
//! `rustshare_server::authz::FilesResourceOwner` adapter, which delegates
//! every decision to the authoritative Files permission semantics
//! (`PermissionResolver`), tenant-scoped metadata lookups and object storage.
//!
//! The suite asserts the fail-closed contract:
//! * possession of a `ResourceRef` grants nothing by itself;
//! * service/workload identity never silently substitutes for a Principal;
//! * Agents act only under explicit, bounded delegation;
//! * revoked shares and cross-tenant refs fail closed immediately;
//! * content and delivery URLs are produced only after current authorization;
//! * stale/malicious index hints never materialize into LLM context.
//!
//! Run with:
//!   set -a; . ./backend/.env; set +a; SQLX_OFFLINE=true cargo test --test source_authorization_test -p rustshare-server -- --ignored --test-threads=1
//! (requires DATABASE_URL and S3-compatible object storage).

use bytes::Bytes;
use chrono::{Duration, Utc};
use rustshare_core::domain::{
    ActionCapability, ApplicationId, ApplicationRegistry, PrincipalId, Share, SharePermissions,
    TenantId, WorkspaceId,
};
use rustshare_core::services::PermissionResolver;
use rustshare_infrastructure::repositories::PermissionResolverRepository;
use rustshare_resource_auth::{
    Candidate, Decision, Delegation, PrincipalContext, PrincipalKind, Purpose, Representation,
    ResourceOwnerRegistry, ResourceRef, SourceAuthorizer, SourceError, WorkloadIdentity,
    FILES_DELETE, FILES_READ, FILES_SHARE, FILES_WRITE, MAX_BATCH_SIZE,
};
use rustshare_server::authz::FilesResourceOwner;
use std::sync::Arc;
use uuid::Uuid;

mod contracts;
use contracts::common::{setup_test_env, TestContext};

/// The `io.elembra.files` Application identity.
fn files_application() -> ApplicationId {
    ApplicationId::new("io.elembra.files")
}

/// Build a fresh TestContext and a `SourceAuthorizer` seeded with the Files
/// owner adapter backed by that context's stores. The owner is registered
/// against the canonical first-party ApplicationRegistry (registration
/// validates that `io.elembra.files` exists and declares the served surface).
async fn setup() -> (TestContext, SourceAuthorizer) {
    let ctx = setup_test_env().await;
    let repo = Arc::new(PermissionResolverRepository::new(ctx.pool.clone()));
    let resolver = Arc::new(PermissionResolver::new(Arc::clone(&repo)));
    let application_registry =
        ApplicationRegistry::first_party().expect("first-party manifests are valid");
    let mut registry = ResourceOwnerRegistry::new();
    registry
        .register(
            Arc::new(FilesResourceOwner::new(
                Arc::clone(&resolver),
                repo,
                ctx.metadata_store.clone(),
                ctx.object_store.clone(),
            )),
            &application_registry,
        )
        .expect("the io.elembra.files owner registers against the canonical registry");
    (ctx, SourceAuthorizer::new(registry))
}

/// A plain human-user principal context. RustShare maps tenant 1:1 to
/// workspace, so the workspace id is the tenant id.
fn user_ctx(principal_id: Uuid, tenant_id: Uuid) -> PrincipalContext {
    PrincipalContext::user(
        PrincipalId(principal_id),
        TenantId(tenant_id),
        WorkspaceId(tenant_id),
    )
}

/// A canonical ref for a Files file.
fn file_ref(file_id: Uuid) -> ResourceRef {
    ResourceRef::new(files_application(), "file", file_id.to_string())
}

/// A canonical ref for a Files folder.
fn folder_ref(folder_id: Uuid) -> ResourceRef {
    ResourceRef::new(files_application(), "folder", folder_id.to_string())
}

fn read_action() -> ActionCapability {
    ActionCapability::new(FILES_READ)
}

fn write_action() -> ActionCapability {
    ActionCapability::new(FILES_WRITE)
}

fn delete_action() -> ActionCapability {
    ActionCapability::new(FILES_DELETE)
}

/// Create a user-to-user share of a file, mirroring what the share
/// repository's `create_user_share` persists.
async fn share_file_to_user(
    ctx: &TestContext,
    file_id: Uuid,
    owner_id: Uuid,
    recipient_id: Uuid,
    permissions: SharePermissions,
) -> Share {
    let share = Share {
        id: Uuid::new_v4(),
        file_id: Some(file_id),
        folder_id: None,
        share_token: None,
        permissions,
        password_hash: None,
        expires_at: None,
        upload_only: false,
        access_count: 0,
        recipient_user_id: Some(recipient_id),
        recipient_group_id: None,
        created_by: owner_id,
        created_at: Utc::now(),
        revoked_at: None,
        tenant_id: ctx.tenant_id,
    };
    ctx.metadata_store
        .create_share(&share)
        .await
        .expect("create user share");
    share
}

/// Create a user-to-user share of a folder.
async fn share_folder_to_user(
    ctx: &TestContext,
    folder_id: Uuid,
    owner_id: Uuid,
    recipient_id: Uuid,
    permissions: SharePermissions,
) -> Share {
    let share = Share {
        id: Uuid::new_v4(),
        file_id: None,
        folder_id: Some(folder_id),
        share_token: None,
        permissions,
        password_hash: None,
        expires_at: None,
        upload_only: false,
        access_count: 0,
        recipient_user_id: Some(recipient_id),
        recipient_group_id: None,
        created_by: owner_id,
        created_at: Utc::now(),
        revoked_at: None,
        tenant_id: ctx.tenant_id,
    };
    ctx.metadata_store
        .create_share(&share)
        .await
        .expect("create folder share");
    share
}

/// The owning user can authorize, resolve and fetch their own file, and the
/// fetched bytes are the exact uploaded content.
#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn authorized_owner_read_resolves_and_fetches() {
    let (ctx, authorizer) = setup().await;
    let owner = ctx
        .create_test_user(&format!("owner_{}", Uuid::new_v4()))
        .await;
    let file = ctx
        .create_test_file(owner.id, None, "hello.txt", b"hello source auth")
        .await;

    let principal = user_ctx(owner.id, ctx.tenant_id);
    let reference = file_ref(file.id);

    assert_eq!(
        authorizer
            .authorize(&principal, &read_action(), &reference)
            .await,
        Decision::Allow,
        "owner must be allowed to read their own file"
    );

    let resolved = authorizer
        .resolve(&principal, &reference, Purpose::UserOpen)
        .await
        .expect("owner resolves own file");
    assert_eq!(resolved.display_name, "hello.txt");
    assert_eq!(
        resolved.media_type.as_deref(),
        Some("application/octet-stream")
    );
    assert_eq!(resolved.size, Some(b"hello source auth".len() as i64));
    assert!(resolved.available, "current file must resolve as available");

    for representation in [Representation::Raw, Representation::Text] {
        let fetched = authorizer
            .fetch(&principal, &reference, representation)
            .await
            .expect("owner fetches own file");
        assert_eq!(fetched.resource, reference);
        assert_eq!(fetched.representation, representation);
        assert_eq!(
            fetched.data,
            Bytes::from("hello source auth"),
            "fetch must return the exact uploaded bytes"
        );
        assert_eq!(fetched.size, Some(b"hello source auth".len() as i64));
    }

    let metadata = authorizer
        .fetch(&principal, &reference, Representation::Metadata)
        .await
        .expect("owner fetches metadata");
    assert!(
        metadata.data.is_empty(),
        "metadata representation must carry no content"
    );
    assert_eq!(
        metadata.media_type.as_deref(),
        Some("application/octet-stream")
    );

    let _ = ctx.cleanup().await;
}

/// A same-tenant user without any share is denied everywhere: authorize says
/// Deny and every content-producing call fails with Unauthorized.
#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn unauthorized_read_fails_closed() {
    let (ctx, authorizer) = setup().await;
    let owner = ctx
        .create_test_user(&format!("owner_{}", Uuid::new_v4()))
        .await;
    let stranger = ctx
        .create_test_user(&format!("stranger_{}", Uuid::new_v4()))
        .await;
    let file = ctx
        .create_test_file(owner.id, None, "private.txt", b"secret")
        .await;

    let principal = user_ctx(stranger.id, ctx.tenant_id);
    let reference = file_ref(file.id);

    assert_eq!(
        authorizer
            .authorize(&principal, &read_action(), &reference)
            .await,
        Decision::Deny,
        "an unshared same-tenant user must be denied"
    );
    assert!(
        matches!(
            authorizer
                .resolve(&principal, &reference, Purpose::UserOpen)
                .await,
            Err(SourceError::Unauthorized)
        ),
        "resolve without authorization must fail closed"
    );
    assert!(
        matches!(
            authorizer
                .fetch(&principal, &reference, Representation::Text)
                .await,
            Err(SourceError::Unauthorized)
        ),
        "fetch without authorization must fail closed"
    );
    assert!(
        matches!(
            authorizer
                .fetch_delivery_url(&principal, &reference, Purpose::UserOpen, 60)
                .await,
            Err(SourceError::Unauthorized)
        ),
        "delivery URL without authorization must fail closed"
    );

    let _ = ctx.cleanup().await;
}

/// Revoking a share changes the decision immediately — no index/cache refresh
/// is required for the denial to take effect.
#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn revoked_share_immediately_denies() {
    let (ctx, authorizer) = setup().await;
    let owner = ctx
        .create_test_user(&format!("owner_{}", Uuid::new_v4()))
        .await;
    let recipient = ctx
        .create_test_user(&format!("recipient_{}", Uuid::new_v4()))
        .await;
    let file = ctx
        .create_test_file(owner.id, None, "shared.txt", b"shared content")
        .await;

    let share = share_file_to_user(
        &ctx,
        file.id,
        owner.id,
        recipient.id,
        SharePermissions::View,
    )
    .await;

    let principal = user_ctx(recipient.id, ctx.tenant_id);
    let reference = file_ref(file.id);

    assert_eq!(
        authorizer
            .authorize(&principal, &read_action(), &reference)
            .await,
        Decision::Allow,
        "recipient is allowed while the share is active"
    );

    ctx.metadata_store
        .revoke_share(share.id, owner.id)
        .await
        .expect("revoke share");

    assert_eq!(
        authorizer
            .authorize(&principal, &read_action(), &reference)
            .await,
        Decision::Deny,
        "revoked share must deny immediately"
    );

    let _ = ctx.cleanup().await;
}

/// Group shares authorize group members only; a same-tenant non-member stays
/// denied.
#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn group_share_allows_members_only() {
    let (ctx, authorizer) = setup().await;
    let owner = ctx
        .create_test_user(&format!("owner_{}", Uuid::new_v4()))
        .await;
    let member = ctx
        .create_test_user(&format!("member_{}", Uuid::new_v4()))
        .await;
    let outsider = ctx
        .create_test_user(&format!("outsider_{}", Uuid::new_v4()))
        .await;
    let file = ctx
        .create_test_file(owner.id, None, "group.txt", b"group content")
        .await;

    let group_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO user_groups (id, name, tenant_id, created_by) VALUES ($1, $2, $3, $4)",
    )
    .bind(group_id)
    .bind(format!("g_{}", Uuid::new_v4()))
    .bind(ctx.tenant_id)
    .bind(owner.id)
    .execute(&ctx.pool)
    .await
    .expect("create user group");
    sqlx::query("INSERT INTO group_members (group_id, user_id) VALUES ($1, $2)")
        .bind(group_id)
        .bind(member.id)
        .execute(&ctx.pool)
        .await
        .expect("add member to group");

    let share = Share {
        id: Uuid::new_v4(),
        file_id: Some(file.id),
        folder_id: None,
        share_token: None,
        permissions: SharePermissions::View,
        password_hash: None,
        expires_at: None,
        upload_only: false,
        access_count: 0,
        recipient_user_id: None,
        recipient_group_id: Some(group_id),
        created_by: owner.id,
        created_at: Utc::now(),
        revoked_at: None,
        tenant_id: ctx.tenant_id,
    };
    ctx.metadata_store
        .create_share(&share)
        .await
        .expect("create group share");

    let reference = file_ref(file.id);
    assert_eq!(
        authorizer
            .authorize(
                &user_ctx(member.id, ctx.tenant_id),
                &read_action(),
                &reference
            )
            .await,
        Decision::Allow,
        "group member must be allowed via the group share"
    );
    assert_eq!(
        authorizer
            .authorize(
                &user_ctx(outsider.id, ctx.tenant_id),
                &read_action(),
                &reference
            )
            .await,
        Decision::Deny,
        "a same-tenant non-member must stay denied"
    );

    let _ = ctx.cleanup().await;
}

/// A ref pointing at a resource outside the principal's tenant fails closed:
/// it is NotFound, never Allow, and content calls fail with NotFound.
#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn cross_tenant_ref_fails_closed() {
    let (ctx, authorizer) = setup().await;
    let owner = ctx
        .create_test_user(&format!("owner_{}", Uuid::new_v4()))
        .await;
    let file = ctx
        .create_test_file(owner.id, None, "tenant.txt", b"cross-tenant")
        .await;

    let foreign_tenant = Uuid::new_v4();
    let principal = user_ctx(owner.id, foreign_tenant);
    let reference = file_ref(file.id);

    assert_eq!(
        authorizer
            .authorize(&principal, &read_action(), &reference)
            .await,
        Decision::NotFound,
        "cross-tenant ref must never be allowed"
    );
    assert!(
        matches!(
            authorizer
                .resolve(&principal, &reference, Purpose::UserOpen)
                .await,
            Err(SourceError::NotFound)
        ),
        "cross-tenant resolve must fail closed"
    );
    assert!(
        matches!(
            authorizer
                .fetch(&principal, &reference, Representation::Text)
                .await,
            Err(SourceError::NotFound)
        ),
        "cross-tenant fetch must fail closed"
    );

    let _ = ctx.cleanup().await;
}

/// A shared Edit folder recipient can read and write the folder but not delete
/// it: `files.delete` requires Admin, which Edit does not grant.
#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn shared_edit_recipient_write_allowed_delete_denied() {
    let (ctx, authorizer) = setup().await;
    let owner = ctx
        .create_test_user(&format!("owner_{}", Uuid::new_v4()))
        .await;
    let recipient = ctx
        .create_test_user(&format!("recipient_{}", Uuid::new_v4()))
        .await;
    let folder = ctx.create_test_folder(owner.id, "shared", None).await;

    share_folder_to_user(
        &ctx,
        folder.id,
        owner.id,
        recipient.id,
        SharePermissions::Edit,
    )
    .await;

    let principal = user_ctx(recipient.id, ctx.tenant_id);
    let reference = folder_ref(folder.id);

    assert_eq!(
        authorizer
            .authorize(&principal, &read_action(), &reference)
            .await,
        Decision::Allow,
        "Edit share grants read"
    );
    assert_eq!(
        authorizer
            .authorize(&principal, &write_action(), &reference)
            .await,
        Decision::Allow,
        "Edit share grants write"
    );
    assert_eq!(
        authorizer
            .authorize(&principal, &delete_action(), &reference)
            .await,
        Decision::Deny,
        "Edit share does not grant delete (Admin required)"
    );

    let _ = ctx.cleanup().await;
}

/// A shared Admin folder recipient can delete the folder: `files.delete` maps
/// to the Admin permission level.
#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn shared_admin_recipient_delete_allowed() {
    let (ctx, authorizer) = setup().await;
    let owner = ctx
        .create_test_user(&format!("owner_{}", Uuid::new_v4()))
        .await;
    let recipient = ctx
        .create_test_user(&format!("recipient_{}", Uuid::new_v4()))
        .await;
    let folder = ctx.create_test_folder(owner.id, "shared", None).await;

    share_folder_to_user(
        &ctx,
        folder.id,
        owner.id,
        recipient.id,
        SharePermissions::Admin,
    )
    .await;

    let principal = user_ctx(recipient.id, ctx.tenant_id);
    let reference = folder_ref(folder.id);

    assert_eq!(
        authorizer
            .authorize(&principal, &delete_action(), &reference)
            .await,
        Decision::Allow,
        "Admin share grants delete on the shared folder"
    );

    let _ = ctx.cleanup().await;
}

/// Batch authorization preserves the ref/decision association, returns
/// decisions in input order, and one denied or invalid ref never changes
/// another ref's outcome.
#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn batch_preserves_ref_association_and_mixed_decisions() {
    let (ctx, authorizer) = setup().await;
    let owner = ctx
        .create_test_user(&format!("owner_{}", Uuid::new_v4()))
        .await;
    let other = ctx
        .create_test_user(&format!("other_{}", Uuid::new_v4()))
        .await;
    let owned = ctx
        .create_test_file(owner.id, None, "owned.txt", b"owned")
        .await;
    let unshared = ctx
        .create_test_file(other.id, None, "unshared.txt", b"other's file")
        .await;
    let missing = Uuid::new_v4();
    let malformed = ResourceRef::new(files_application(), "FILE", "abc");

    let refs = vec![
        file_ref(owned.id),
        file_ref(unshared.id),
        file_ref(missing),
        malformed,
    ];
    let principal = user_ctx(owner.id, ctx.tenant_id);

    let decisions = authorizer
        .authorize_batch(&principal, &read_action(), &refs)
        .await
        .expect("batch authorization succeeds");

    assert_eq!(decisions.len(), 4);
    for (decision, reference) in decisions.iter().zip(&refs) {
        assert_eq!(
            &decision.resource, reference,
            "each decision must be associated with its own input ref"
        );
    }
    assert_eq!(decisions[0].decision, Decision::Allow, "owned file allowed");
    assert_eq!(
        decisions[1].decision,
        Decision::Deny,
        "unshared file denied"
    );
    assert_eq!(
        decisions[2].decision,
        Decision::NotFound,
        "nonexistent uuid is not found"
    );
    assert_eq!(
        decisions[3].decision,
        Decision::Invalid,
        "malformed ref is invalid"
    );

    let _ = ctx.cleanup().await;
}

/// A batch larger than the contract bound is rejected outright, failing
/// closed instead of authorizing anything.
#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn batch_rejects_oversized_input() {
    let (ctx, authorizer) = setup().await;
    let owner = ctx
        .create_test_user(&format!("owner_{}", Uuid::new_v4()))
        .await;
    let principal = user_ctx(owner.id, ctx.tenant_id);

    let refs: Vec<ResourceRef> = (0..=MAX_BATCH_SIZE)
        .map(|_| file_ref(Uuid::new_v4()))
        .collect();
    assert!(
        matches!(
            authorizer
                .authorize_batch(&principal, &read_action(), &refs)
                .await,
            Err(SourceError::BatchTooLarge {
                actual: 65,
                limit: 64
            })
        ),
        "65 refs must be rejected as BatchTooLarge"
    );

    let _ = ctx.cleanup().await;
}

/// A Service principal with workload identity but no delegation cannot act on
/// any resource — not even the owner's own file — in single or batch calls.
#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn service_identity_alone_cannot_bypass() {
    let (ctx, authorizer) = setup().await;
    let owner = ctx
        .create_test_user(&format!("owner_{}", Uuid::new_v4()))
        .await;
    let file = ctx
        .create_test_file(owner.id, None, "svc.txt", b"service")
        .await;

    let service = PrincipalContext {
        principal_id: PrincipalId(Uuid::new_v4()),
        principal_kind: PrincipalKind::Service,
        tenant_id: TenantId(ctx.tenant_id),
        workspace_id: WorkspaceId(ctx.tenant_id),
        group_ids: Vec::new(),
        grants: Vec::new(),
        authentication: None,
        delegation: None,
        workload_identity: Some(WorkloadIdentity {
            application_id: Some(ApplicationId::new("io.elembra.memory")),
            subject: Some("memory-worker".into()),
        }),
        correlation_id: None,
    };
    let reference = file_ref(file.id);

    assert_eq!(
        authorizer
            .authorize(&service, &read_action(), &reference)
            .await,
        Decision::Deny,
        "workload identity alone must never bypass the principal"
    );

    let decisions = authorizer
        .authorize_batch(&service, &read_action(), &[file_ref(file.id)])
        .await
        .expect("batch authorization succeeds");
    assert_eq!(
        decisions[0].decision,
        Decision::Deny,
        "batch must deny the delegation-less service too"
    );

    let _ = ctx.cleanup().await;
}

/// An Agent acts under its issuer's authority, bounded by the delegated
/// action set, expiry and optional resource scope.
#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn agent_delegation_is_bounded_by_actions_and_expiry() {
    let (ctx, authorizer) = setup().await;
    let owner = ctx
        .create_test_user(&format!("owner_{}", Uuid::new_v4()))
        .await;
    let file = ctx
        .create_test_file(owner.id, None, "agent.txt", b"agent")
        .await;
    let other_file = ctx
        .create_test_file(owner.id, None, "other.txt", b"other")
        .await;

    let agent_id = PrincipalId(Uuid::new_v4());
    let reference = file_ref(file.id);
    let agent = PrincipalContext {
        principal_id: agent_id,
        principal_kind: PrincipalKind::Agent,
        tenant_id: TenantId(ctx.tenant_id),
        workspace_id: WorkspaceId(ctx.tenant_id),
        group_ids: Vec::new(),
        grants: Vec::new(),
        authentication: None,
        delegation: Some(Delegation {
            issuer_principal_id: PrincipalId(owner.id),
            delegate_principal_id: agent_id,
            actions: vec![read_action()],
            workspace_id: None,
            resource_scope: None,
            expires_at: None,
            grant_id: Some("grant-1".into()),
        }),
        workload_identity: None,
        correlation_id: None,
    };

    assert_eq!(
        authorizer
            .authorize(&agent, &read_action(), &reference)
            .await,
        Decision::Allow,
        "the agent inherits the issuer's (owner's) read authority"
    );
    assert_eq!(
        authorizer
            .authorize(&agent, &write_action(), &reference)
            .await,
        Decision::Deny,
        "write is not in the delegated action set"
    );

    // Expired delegations fail closed.
    let mut expired = agent.clone();
    expired
        .delegation
        .as_mut()
        .expect("delegation present")
        .expires_at = Some(Utc::now() - Duration::seconds(1));
    assert_eq!(
        authorizer
            .authorize(&expired, &read_action(), &reference)
            .await,
        Decision::Deny,
        "expired delegation must fail closed"
    );

    // A resource scope bounds the delegation to the listed refs.
    let mut scoped = agent.clone();
    scoped
        .delegation
        .as_mut()
        .expect("delegation present")
        .resource_scope = Some(vec![reference.clone()]);
    assert_eq!(
        authorizer
            .authorize(&scoped, &read_action(), &reference)
            .await,
        Decision::Allow,
        "a ref inside the delegation scope stays allowed"
    );
    assert_eq!(
        authorizer
            .authorize(&scoped, &read_action(), &file_ref(other_file.id))
            .await,
        Decision::Deny,
        "a ref outside the delegation scope must be denied"
    );

    let _ = ctx.cleanup().await;
}

/// Delivery URLs are produced only after current authorization: the owner
/// gets one, an unshared recipient does not, a share grants one, and revoking
/// the share removes the grant immediately. TTLs are clamped into the owner's
/// 1..=900 second window.
#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn delivery_url_generated_only_after_authorization() {
    let (ctx, authorizer) = setup().await;
    let owner = ctx
        .create_test_user(&format!("owner_{}", Uuid::new_v4()))
        .await;
    let recipient = ctx
        .create_test_user(&format!("recipient_{}", Uuid::new_v4()))
        .await;
    let file = ctx
        .create_test_file(owner.id, None, "dl.txt", b"deliver me")
        .await;

    let owner_principal = user_ctx(owner.id, ctx.tenant_id);
    let recipient_principal = user_ctx(recipient.id, ctx.tenant_id);
    let reference = file_ref(file.id);

    let url = authorizer
        .fetch_delivery_url(&owner_principal, &reference, Purpose::UserOpen, 60)
        .await
        .expect("owner gets a delivery URL");
    assert!(
        url.starts_with("http"),
        "delivery URL must be an http(s) URL, got: {url}"
    );

    // Extreme TTLs are clamped into the 1..=900 second window, not rejected.
    for ttl in [0, 5000] {
        let clamped = authorizer
            .fetch_delivery_url(&owner_principal, &reference, Purpose::UserOpen, ttl)
            .await;
        assert!(
            clamped.is_ok(),
            "ttl {ttl} must be clamped to the allowed window, got: {clamped:?}"
        );
    }

    assert!(
        matches!(
            authorizer
                .fetch_delivery_url(&recipient_principal, &reference, Purpose::UserOpen, 60)
                .await,
            Err(SourceError::Unauthorized)
        ),
        "an unshared recipient must not get a delivery URL"
    );

    // A share grants the URL; revoking it removes the grant immediately.
    let share = share_file_to_user(
        &ctx,
        file.id,
        owner.id,
        recipient.id,
        SharePermissions::View,
    )
    .await;
    assert_eq!(
        authorizer
            .authorize(&recipient_principal, &read_action(), &reference)
            .await,
        Decision::Allow,
        "recipient is allowed while the share is active"
    );
    let shared_url = authorizer
        .fetch_delivery_url(&recipient_principal, &reference, Purpose::UserOpen, 60)
        .await
        .expect("shared recipient gets a delivery URL");
    assert!(shared_url.starts_with("http"));

    ctx.metadata_store
        .revoke_share(share.id, owner.id)
        .await
        .expect("revoke share");
    assert!(
        matches!(
            authorizer
                .fetch_delivery_url(&recipient_principal, &reference, Purpose::UserOpen, 60)
                .await,
            Err(SourceError::Unauthorized)
        ),
        "a revoked recipient must not get a delivery URL"
    );

    let _ = ctx.cleanup().await;
}

/// Versioned fetches honor the immutable `sha256:<content-hash>` selector:
/// the historical bytes come back for a real version, unknown versions are
/// unavailable, and an unversioned ref returns the current content.
#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn versioned_fetch_honors_immutable_version() {
    let (ctx, authorizer) = setup().await;
    let owner = ctx
        .create_test_user(&format!("owner_{}", Uuid::new_v4()))
        .await;
    let file = ctx
        .create_test_file(owner.id, None, "v.txt", b"v1 content")
        .await;
    let v1_hash = file.content_hash.clone();

    let updated = ctx
        .file_service()
        .update_file(
            file.id,
            owner.id,
            file.current_version,
            Bytes::from("v2 content"),
        )
        .await
        .expect("owner updates the file to v2");
    assert_eq!(
        updated.current_version, 2,
        "update must bump the file's current version"
    );

    let principal = user_ctx(owner.id, ctx.tenant_id);

    let v1_ref = file_ref(file.id).with_version(format!("sha256:{v1_hash}"));
    let fetched = authorizer
        .fetch(&principal, &v1_ref, Representation::Text)
        .await
        .expect("immutable version fetch succeeds");
    assert_eq!(
        fetched.data,
        Bytes::from("v1 content"),
        "versioned fetch must return the immutable historical bytes"
    );

    let unknown_ref = file_ref(file.id).with_version(format!("sha256:{}", "deadbeef".repeat(8)));
    assert!(
        matches!(
            authorizer
                .fetch(&principal, &unknown_ref, Representation::Text)
                .await,
            Err(SourceError::VersionUnavailable)
        ),
        "an unknown version must fail with VersionUnavailable"
    );
    let resolved = authorizer
        .resolve(&principal, &unknown_ref, Purpose::UserOpen)
        .await
        .expect("resolve with an unknown version still succeeds");
    assert!(
        !resolved.available,
        "an unknown version must resolve as unavailable"
    );

    let current = authorizer
        .fetch(&principal, &file_ref(file.id), Representation::Text)
        .await
        .expect("current version fetch succeeds");
    assert_eq!(
        current.data,
        Bytes::from("v2 content"),
        "an unversioned ref must return the current content"
    );

    let _ = ctx.cleanup().await;
}

/// Search/index materialization reauthorizes every candidate and never lets
/// stale or malicious `cached_text` hints into the output: only authorized
/// source content is materialized.
#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn search_candidate_never_materializes_unauthorized_content() {
    let (ctx, authorizer) = setup().await;
    let owner = ctx
        .create_test_user(&format!("owner_{}", Uuid::new_v4()))
        .await;
    let other = ctx
        .create_test_user(&format!("other_{}", Uuid::new_v4()))
        .await;
    let file_a = ctx
        .create_test_file(owner.id, None, "a.txt", b"public content")
        .await;
    let file_b = ctx
        .create_test_file(other.id, None, "b.txt", b"private")
        .await;

    let candidates = vec![
        Candidate {
            resource: file_ref(file_a.id),
            cached_text: Some("cache-hint-a".into()),
        },
        Candidate {
            resource: file_ref(file_b.id),
            cached_text: Some("ATTACKER INDEXED SECRET".into()),
        },
        Candidate {
            resource: ResourceRef::new(files_application(), "FILE", "abc"),
            cached_text: Some("another secret".into()),
        },
    ];

    let principal = user_ctx(owner.id, ctx.tenant_id);
    let materialized = authorizer
        .materialize(&principal, &read_action(), candidates)
        .await
        .expect("materialization succeeds");

    assert_eq!(
        materialized.len(),
        1,
        "only the authorized candidate may materialize"
    );
    assert_eq!(materialized[0].resource, file_ref(file_a.id));
    assert_eq!(
        materialized[0].data,
        Bytes::from("public content"),
        "materialized data is the real authorized source content"
    );
    let output = String::from_utf8_lossy(&materialized[0].data);
    assert!(
        !output.contains("ATTACKER INDEXED SECRET"),
        "stale index text must never materialize"
    );
    assert!(
        !output.contains("cache-hint-a"),
        "cached hints must never materialize"
    );

    let _ = ctx.cleanup().await;
}

/// A public share authorizes public-share sessions, never arbitrary
/// principals: a random same-tenant user stays denied even though the file
/// has a public link.
#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn public_share_does_not_grant_through_plain_principal() {
    let (ctx, authorizer) = setup().await;
    let owner = ctx
        .create_test_user(&format!("owner_{}", Uuid::new_v4()))
        .await;
    let bystander = ctx
        .create_test_user(&format!("bystander_{}", Uuid::new_v4()))
        .await;
    let file = ctx
        .create_test_file(owner.id, None, "pub.txt", b"public")
        .await;

    let share = Share {
        id: Uuid::new_v4(),
        file_id: Some(file.id),
        folder_id: None,
        share_token: Some(format!("token_{}", Uuid::new_v4())),
        permissions: SharePermissions::View,
        password_hash: None,
        expires_at: None,
        upload_only: false,
        access_count: 0,
        recipient_user_id: None,
        recipient_group_id: None,
        created_by: owner.id,
        created_at: Utc::now(),
        revoked_at: None,
        tenant_id: ctx.tenant_id,
    };
    ctx.metadata_store
        .create_share(&share)
        .await
        .expect("create public share");

    let principal = user_ctx(bystander.id, ctx.tenant_id);
    assert_eq!(
        authorizer
            .authorize(&principal, &read_action(), &file_ref(file.id))
            .await,
        Decision::Deny,
        "public shares must not grant access to arbitrary principals"
    );

    let _ = ctx.cleanup().await;
}
/// A forged delegation grants no more than its issuer's current authority: an
/// agent delegated by a user with no access to the file stays denied. This
/// locks the invariant that delegations cannot amplify authority and that the
/// delegation's issuer is always re-evaluated at the source.
#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn forged_delegation_grants_no_more_than_issuer_holds() {
    let (ctx, authorizer) = setup().await;
    let owner = ctx
        .create_test_user(&format!("owner_{}", Uuid::new_v4()))
        .await;
    let powerless = ctx
        .create_test_user(&format!("powerless_{}", Uuid::new_v4()))
        .await;
    let file = ctx
        .create_test_file(owner.id, None, "owned.txt", b"owned")
        .await;

    let agent_id = PrincipalId(Uuid::new_v4());
    let agent = PrincipalContext {
        principal_id: agent_id,
        principal_kind: PrincipalKind::Agent,
        tenant_id: TenantId(ctx.tenant_id),
        workspace_id: WorkspaceId(ctx.tenant_id),
        group_ids: Vec::new(),
        grants: Vec::new(),
        authentication: None,
        delegation: Some(Delegation {
            issuer_principal_id: PrincipalId(powerless.id),
            delegate_principal_id: agent_id,
            actions: vec![read_action()],
            workspace_id: None,
            resource_scope: None,
            expires_at: None,
            grant_id: Some("forged-grant".into()),
        }),
        workload_identity: None,
        correlation_id: None,
    };

    assert_eq!(
        authorizer
            .authorize(&agent, &read_action(), &file_ref(file.id))
            .await,
        Decision::Deny,
        "a delegation from a powerless issuer must not grant access"
    );
    assert!(
        matches!(
            authorizer
                .fetch(&agent, &file_ref(file.id), Representation::Text)
                .await,
            Err(SourceError::Unauthorized)
        ),
        "content must not be fetched through a forged delegation"
    );

    let _ = ctx.cleanup().await;
}

/// `group_ids`/`grants` carried in a PrincipalContext are informational and
/// must never be trusted as grants: a non-member whose context lists the
/// group id (spoofed at the boundary) stays denied because the source derives
/// membership from authoritative state.
#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn context_group_ids_are_never_trusted() {
    let (ctx, authorizer) = setup().await;
    let owner = ctx
        .create_test_user(&format!("owner_{}", Uuid::new_v4()))
        .await;
    let outsider = ctx
        .create_test_user(&format!("outsider_{}", Uuid::new_v4()))
        .await;
    let file = ctx
        .create_test_file(owner.id, None, "guarded.txt", b"guarded")
        .await;

    let group_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO user_groups (id, name, tenant_id, created_by) VALUES ($1, $2, $3, $4)",
    )
    .bind(group_id)
    .bind(format!("g_{}", Uuid::new_v4()))
    .bind(ctx.tenant_id)
    .bind(owner.id)
    .execute(&ctx.pool)
    .await
    .expect("create user group");

    let share = Share {
        id: Uuid::new_v4(),
        file_id: Some(file.id),
        folder_id: None,
        share_token: None,
        permissions: SharePermissions::View,
        password_hash: None,
        expires_at: None,
        upload_only: false,
        access_count: 0,
        recipient_user_id: None,
        recipient_group_id: Some(group_id),
        created_by: owner.id,
        created_at: Utc::now(),
        revoked_at: None,
        tenant_id: ctx.tenant_id,
    };
    ctx.metadata_store
        .create_share(&share)
        .await
        .expect("create group share");

    // The outsider is NOT a member, but their context claims membership.
    let spoofed = PrincipalContext {
        principal_id: PrincipalId(outsider.id),
        principal_kind: PrincipalKind::User,
        tenant_id: TenantId(ctx.tenant_id),
        workspace_id: WorkspaceId(ctx.tenant_id),
        group_ids: vec![group_id],
        grants: vec![read_action()],
        authentication: None,
        delegation: None,
        workload_identity: None,
        correlation_id: None,
    };

    assert_eq!(
        authorizer
            .authorize(&spoofed, &read_action(), &file_ref(file.id))
            .await,
        Decision::Deny,
        "client-supplied group_ids/grants must never grant access"
    );

    let _ = ctx.cleanup().await;
}

/// `files.share` requires Admin and, like the legacy `resolve_permission`
/// based recipient share management, inherits Admin from folder ancestry: an
/// Admin recipient of a shared folder authorizes share management on files
/// inside it, while an Edit recipient does not.
#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn files_share_requires_admin_and_inherits_from_folder() {
    let (ctx, authorizer) = setup().await;
    let owner = ctx
        .create_test_user(&format!("owner_{}", Uuid::new_v4()))
        .await;
    let admin = ctx
        .create_test_user(&format!("admin_{}", Uuid::new_v4()))
        .await;
    let editor = ctx
        .create_test_user(&format!("editor_{}", Uuid::new_v4()))
        .await;
    let root = ctx.create_test_folder(owner.id, "root", None).await;
    let file = ctx
        .create_test_file(owner.id, Some(root.id), "in-shared.txt", b"inside")
        .await;

    let share = |recipient: Uuid, permissions: SharePermissions| Share {
        id: Uuid::new_v4(),
        file_id: None,
        folder_id: Some(root.id),
        share_token: None,
        permissions,
        password_hash: None,
        expires_at: None,
        upload_only: false,
        access_count: 0,
        recipient_user_id: Some(recipient),
        recipient_group_id: None,
        created_by: owner.id,
        created_at: Utc::now(),
        revoked_at: None,
        tenant_id: ctx.tenant_id,
    };
    ctx.metadata_store
        .create_share(&share(admin.id, SharePermissions::Admin))
        .await
        .expect("create admin folder share");
    ctx.metadata_store
        .create_share(&share(editor.id, SharePermissions::Edit))
        .await
        .expect("create edit folder share");

    let reference = file_ref(file.id);
    assert_eq!(
        authorizer
            .authorize(
                &user_ctx(admin.id, ctx.tenant_id),
                &ActionCapability::new(FILES_SHARE),
                &reference
            )
            .await,
        Decision::Allow,
        "folder-inherited Admin authorizes files.share on files inside the folder"
    );
    assert_eq!(
        authorizer
            .authorize(
                &user_ctx(editor.id, ctx.tenant_id),
                &ActionCapability::new(FILES_SHARE),
                &reference
            )
            .await,
        Decision::Deny,
        "Edit does not authorize share management"
    );

    let _ = ctx.cleanup().await;
}

/// Revoking a group share takes immediate effect through the contract
/// (no index or cache refresh involved).
#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn group_share_revocation_takes_immediate_effect() {
    let (ctx, authorizer) = setup().await;
    let owner = ctx
        .create_test_user(&format!("owner_{}", Uuid::new_v4()))
        .await;
    let member = ctx
        .create_test_user(&format!("member_{}", Uuid::new_v4()))
        .await;
    let file = ctx.create_test_file(owner.id, None, "g.txt", b"g").await;

    let group_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO user_groups (id, name, tenant_id, created_by) VALUES ($1, $2, $3, $4)",
    )
    .bind(group_id)
    .bind(format!("g_{}", Uuid::new_v4()))
    .bind(ctx.tenant_id)
    .bind(owner.id)
    .execute(&ctx.pool)
    .await
    .expect("create user group");
    sqlx::query("INSERT INTO group_members (group_id, user_id) VALUES ($1, $2)")
        .bind(group_id)
        .bind(member.id)
        .execute(&ctx.pool)
        .await
        .expect("add member to group");

    let share = Share {
        id: Uuid::new_v4(),
        file_id: Some(file.id),
        folder_id: None,
        share_token: None,
        permissions: SharePermissions::View,
        password_hash: None,
        expires_at: None,
        upload_only: false,
        access_count: 0,
        recipient_user_id: None,
        recipient_group_id: Some(group_id),
        created_by: owner.id,
        created_at: Utc::now(),
        revoked_at: None,
        tenant_id: ctx.tenant_id,
    };
    ctx.metadata_store
        .create_share(&share)
        .await
        .expect("create group share");

    let reference = file_ref(file.id);
    let principal = user_ctx(member.id, ctx.tenant_id);
    assert_eq!(
        authorizer
            .authorize(&principal, &read_action(), &reference)
            .await,
        Decision::Allow,
        "group member is allowed while the group share is active"
    );

    ctx.metadata_store
        .revoke_share(share.id, owner.id)
        .await
        .expect("revoke group share");
    assert_eq!(
        authorizer
            .authorize(&principal, &read_action(), &reference)
            .await,
        Decision::Deny,
        "revoked group share must deny immediately"
    );

    let _ = ctx.cleanup().await;
}

/// A shared recipient (non-owner) can fetch an immutable version of a shared
/// file: the version lookup is owner-scoped by the file's real owner, so
/// recipient access keeps working for historical versions.
#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn shared_recipient_can_fetch_immutable_version() {
    let (ctx, authorizer) = setup().await;
    let owner = ctx
        .create_test_user(&format!("owner_{}", Uuid::new_v4()))
        .await;
    let recipient = ctx
        .create_test_user(&format!("recipient_{}", Uuid::new_v4()))
        .await;
    let file = ctx.create_test_file(owner.id, None, "sv.txt", b"sv1").await;
    let v1_hash = file.content_hash.clone();
    let updated = ctx
        .file_service()
        .update_file(file.id, owner.id, file.current_version, Bytes::from("sv2"))
        .await
        .expect("owner updates the file to v2");
    assert_eq!(updated.current_version, 2);

    share_file_to_user(
        &ctx,
        file.id,
        owner.id,
        recipient.id,
        SharePermissions::View,
    )
    .await;

    let recipient_principal = user_ctx(recipient.id, ctx.tenant_id);
    let v1_ref = file_ref(file.id).with_version(format!("sha256:{v1_hash}"));
    let fetched = authorizer
        .fetch(&recipient_principal, &v1_ref, Representation::Text)
        .await
        .expect("shared recipient fetches the immutable version");
    assert_eq!(
        fetched.data,
        Bytes::from("sv1"),
        "shared recipient must get the historical bytes"
    );

    let _ = ctx.cleanup().await;
}

/// Non-`sha256:` version selectors fail closed: resolution reports the
/// version as unavailable and fetch refuses it.
#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn non_sha256_version_selector_fails_closed() {
    let (ctx, authorizer) = setup().await;
    let owner = ctx
        .create_test_user(&format!("owner_{}", Uuid::new_v4()))
        .await;
    let file = ctx.create_test_file(owner.id, None, "nv.txt", b"nv").await;

    let principal = user_ctx(owner.id, ctx.tenant_id);
    // Syntax-valid selectors the owner cannot interpret: an unsupported
    // prefix and a non-hex sha256 value both fail closed.
    for bad_version in ["drive-revision:abc", "sha256:zzzzzzzz"] {
        let bad_ref = file_ref(file.id).with_version(bad_version);
        let resolved = authorizer
            .resolve(&principal, &bad_ref, Purpose::UserOpen)
            .await
            .expect("resolve still works for an unknown version selector");
        assert!(
            !resolved.available,
            "unknown version selector `{bad_version}` must resolve as unavailable"
        );
        assert!(
            matches!(
                authorizer
                    .fetch(&principal, &bad_ref, Representation::Text)
                    .await,
                Err(SourceError::VersionUnavailable)
            ),
            "unknown version selector `{bad_version}` must refuse fetch"
        );
    }

    let _ = ctx.cleanup().await;
}

/// `materialize` omits (never aborts and never substitutes stale content) an
/// allowed candidate whose source content cannot be fetched — e.g. the blob
/// vanished from object storage.
#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn materialize_omits_allowed_candidate_when_fetch_fails() {
    let (ctx, authorizer) = setup().await;
    let owner = ctx
        .create_test_user(&format!("owner_{}", Uuid::new_v4()))
        .await;
    let file = ctx
        .create_test_file(owner.id, None, "gone.txt", b"will disappear")
        .await;

    // Simulate a source-unavailable condition: the blob no longer exists.
    ctx.object_store
        .delete(&file.storage_key())
        .await
        .expect("delete blob to simulate source unavailability");

    let materialized = authorizer
        .materialize(
            &user_ctx(owner.id, ctx.tenant_id),
            &read_action(),
            vec![Candidate {
                resource: file_ref(file.id),
                cached_text: Some("STALE INDEX HINT".into()),
            }],
        )
        .await
        .expect("materialize must not abort on an unfetchable candidate");
    assert!(
        materialized.is_empty(),
        "the unfetchable candidate must be omitted, and stale hints never materialize"
    );

    let _ = ctx.cleanup().await;
}
