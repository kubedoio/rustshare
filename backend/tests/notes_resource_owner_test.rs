//! Source authorization for Notes ResourceRefs delegates to the existing
//! file-backed Notes permission checks.
//!
//! Run with the guarded disposable PostgreSQL and object store:
//! `SQLX_OFFLINE=true cargo test -p rustshare-server --test notes_resource_owner_test -- --ignored --test-threads=1`

use std::sync::Arc;

use rustshare_core::domain::{
    ApplicationId, ApplicationRegistry, PrincipalId, TenantId, WorkspaceId,
};
use rustshare_resource_auth::{
    PrincipalContext, Purpose, ResourceOwnerRegistry, ResourceRef, SourceAuthorizer,
};
use uuid::Uuid;

mod support;
use support::calendar_harness::{
    cleanup_tenant, create_test_tenant, create_test_user, setup_test_env, SERIAL,
};

#[tokio::test]
#[ignore = "requires the guarded disposable PostgreSQL and object-store services"]
async fn note_refs_follow_current_file_authorization_and_revoke_immediately() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env().await;
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let foreign_tenant_id = create_test_tenant(&state.db_pool).await;
    let owner = create_test_user(&state, "notes_resource_owner", tenant_id).await;
    let recipient = create_test_user(&state, "notes_resource_recipient", tenant_id).await;
    let foreign = create_test_user(&state, "notes_resource_foreign", foreign_tenant_id).await;
    let note = state
        .note_service
        .create_note(
            owner.id,
            tenant_id,
            Some("Linked note".into()),
            None,
            Some("Current note contents".into()),
        )
        .await
        .expect("create note fixture");

    let registry = ApplicationRegistry::first_party().expect("first-party applications");
    let mut owners = ResourceOwnerRegistry::new();
    owners
        .register(
            Arc::new(rustshare_server::authz::NotesResourceOwner::new(
                state.note_service.clone(),
            )),
            &registry,
        )
        .expect("register Notes owner against its manifest");
    let authorizer = SourceAuthorizer::new(owners);
    let resource = ResourceRef::new(
        ApplicationId::new("io.elembra.notes"),
        "notes.resource",
        note.id.to_string(),
    );
    let principal = |user_id, tenant_id| {
        PrincipalContext::user(
            PrincipalId(user_id),
            TenantId(tenant_id),
            WorkspaceId(tenant_id),
        )
    };

    let own = authorizer
        .resolve(
            &principal(owner.id, tenant_id),
            &resource,
            Purpose::UserOpen,
        )
        .await
        .expect("owner can resolve their note");
    assert!(own.available);
    assert_eq!(own.display_name, "Linked note");

    sqlx::query(
        "INSERT INTO shares (id, file_id, share_token, created_by, permissions, recipient_user_id, tenant_id)
         VALUES ($1, $2, NULL, $3, 'View', $4, $5)",
    )
    .bind(Uuid::new_v4())
    .bind(note.id)
    .bind(owner.id)
    .bind(recipient.id)
    .bind(tenant_id)
    .execute(&state.db_pool)
    .await
    .expect("grant current Files read access");
    let shared = authorizer
        .resolve(
            &principal(recipient.id, tenant_id),
            &resource,
            Purpose::UserOpen,
        )
        .await
        .expect("recipient can resolve the shared note");
    assert!(shared.available);

    sqlx::query("DELETE FROM shares WHERE file_id = $1 AND recipient_user_id = $2")
        .bind(note.id)
        .bind(recipient.id)
        .execute(&state.db_pool)
        .await
        .expect("revoke Files read access");
    assert!(authorizer
        .resolve(
            &principal(recipient.id, tenant_id),
            &resource,
            Purpose::UserOpen
        )
        .await
        .is_err());
    assert!(authorizer
        .resolve(
            &principal(foreign.id, foreign_tenant_id),
            &resource,
            Purpose::UserOpen,
        )
        .await
        .is_err());

    state
        .note_service
        .delete_note(note.id, owner.id, tenant_id)
        .await
        .expect("clean up note fixture");
    cleanup_tenant(&state.db_pool, tenant_id).await;
    cleanup_tenant(&state.db_pool, foreign_tenant_id).await;
}
