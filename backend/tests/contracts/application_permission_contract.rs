//! ApplicationConfig Tenant and Permission Contract Tests (LB-02)
//!
//! Negative contract tests for module object routes across tenant and share boundaries.
//! These tests document the desired contract: module data must not leak across tenants
//! and must not be accessible by unauthorized users within the same tenant.

use crate::common::*;
use rustshare_core::domain::SharePermissions;
use rustshare_core::events::EventBroadcaster;
use rustshare_core::services::{FileService, FolderService, PermissionResolver};
use rustshare_infrastructure::repositories::PermissionResolverRepository;
use rustshare_server::services::brainstorming_service::{BrainstormError, BrainstormingService};
use rustshare_server::services::decision_service::{DecisionError, DecisionService};
use rustshare_server::services::kanban_service::{
    CreateBoardInput, CreateCardInput, KanbanError, KanbanService,
};
use rustshare_server::services::meeting_service::{MeetingError, MeetingService};
use rustshare_server::services::note_service::{NoteError, NoteService, NoteVisibility};
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
    let permission_resolver = Arc::new(PermissionResolver::new(Arc::new(
        PermissionResolverRepository::new(pool.clone()),
    )));

    Arc::new(NoteService::new(
        file_service,
        folder_service,
        metadata_store,
        object_store,
        permission_resolver,
        pool.clone(),
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
    let user_repository = Arc::new(rustshare_infrastructure::repositories::UserRepository::new(
        pool.clone(),
    ));
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
    let tenant_a = ctx.tenant_id;
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
        .create_note(
            user_a.id,
            tenant_a,
            Some("Secret".to_string()),
            None,
            Some("content".to_string()),
        )
        .await
        .unwrap();

    let result = service.get_note(note.id, user_b.id, tenant_b).await;
    assert!(
        matches!(result, Err(NoteError::PermissionDenied)),
        "Cross-tenant get_note should be denied, got {:?}",
        result
    );

    cleanup_tenant(&ctx.pool, tenant_b).await;
    ctx.cleanup().await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_notes_cross_tenant_save_denied() {
    let ctx = setup_test_env().await;
    let tenant_a = ctx.tenant_id;
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
        .create_note(
            user_a.id,
            tenant_a,
            Some("Secret".to_string()),
            None,
            Some("content".to_string()),
        )
        .await
        .unwrap();

    let result = service
        .save_note(
            note.id,
            user_b.id,
            tenant_b,
            "hacked".to_string(),
            None,
            None,
        )
        .await;
    assert!(
        matches!(result, Err(NoteError::PermissionDenied)),
        "Cross-tenant save_note should be denied, got {:?}",
        result
    );

    cleanup_tenant(&ctx.pool, tenant_b).await;
    ctx.cleanup().await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_notes_cross_tenant_delete_denied() {
    let ctx = setup_test_env().await;
    let tenant_a = ctx.tenant_id;
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
        .create_note(
            user_a.id,
            tenant_a,
            Some("Secret".to_string()),
            None,
            Some("content".to_string()),
        )
        .await
        .unwrap();

    let result = service.delete_note(note.id, user_b.id, tenant_b).await;
    assert!(
        matches!(result, Err(NoteError::PermissionDenied)),
        "Cross-tenant delete_note should be denied, got {:?}",
        result
    );

    cleanup_tenant(&ctx.pool, tenant_b).await;
    ctx.cleanup().await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_notes_cross_tenant_list_does_not_leak() {
    let ctx = setup_test_env().await;
    let tenant_a = ctx.tenant_id;
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
        .create_note(
            user_a.id,
            tenant_a,
            Some("Secret".to_string()),
            None,
            Some("content".to_string()),
        )
        .await
        .unwrap();

    let list_b = service
        .list_notes(user_b.id, tenant_b, 10, 0)
        .await
        .unwrap();
    assert!(
        !list_b.iter().any(|n| n.metadata.title == "Secret"),
        "Cross-tenant list_notes should not leak notes"
    );

    cleanup_tenant(&ctx.pool, tenant_b).await;
    ctx.cleanup().await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_notes_same_tenant_unauthorized_denied() {
    let ctx = setup_test_env().await;
    let tenant_id = ctx.tenant_id;
    let user_owner = create_test_user(&ctx.metadata_store, "note_owner", tenant_id).await;
    let user_other = create_test_user(&ctx.metadata_store, "note_other", tenant_id).await;
    let service = create_note_service(
        ctx.event_store.clone(),
        ctx.metadata_store.clone(),
        ctx.object_store.clone(),
        &ctx.pool,
    );

    let note = service
        .create_note(
            user_owner.id,
            tenant_id,
            Some("Private".to_string()),
            None,
            Some("content".to_string()),
        )
        .await
        .unwrap();

    let result = service.get_note(note.id, user_other.id, tenant_id).await;
    assert!(
        matches!(result, Err(NoteError::PermissionDenied)),
        "Same-tenant unauthorized get_note should be denied, got {:?}",
        result
    );

    ctx.cleanup().await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_notes_shared_resource_only_through_intended_path() {
    let ctx = setup_test_env().await;
    let tenant_id = ctx.tenant_id;
    let user = create_test_user(&ctx.metadata_store, "note_share_user", tenant_id).await;
    let service = create_note_service(
        ctx.event_store.clone(),
        ctx.metadata_store.clone(),
        ctx.object_store.clone(),
        &ctx.pool,
    );

    let note = service
        .create_note(
            user.id,
            tenant_id,
            Some("Shared".to_string()),
            None,
            Some("secret".to_string()),
        )
        .await
        .unwrap();

    // Private note should not be accessible via public route
    let result = service
        .get_public_note("nonexistentshareid12345678901234")
        .await;
    assert!(
        result.is_err(),
        "Random share_id should not access private note"
    );

    // Make public and verify access works through intended path
    let public = service
        .toggle_visibility(note.id, user.id, tenant_id)
        .await
        .unwrap();
    let share_id = public.metadata.public_share_id.unwrap();
    let public_note = service.get_public_note(&share_id).await.unwrap();
    assert_eq!(public_note.title, "Shared");

    // Revoke and verify denial
    let _ = service
        .toggle_visibility(note.id, user.id, tenant_id)
        .await
        .unwrap();
    let result = service.get_public_note(&share_id).await;
    assert!(result.is_err(), "Revoked public note should deny access");

    ctx.cleanup().await;
}

/// Plant a crafted visible sidecar `{note_name}.rustshare.json` by editing
/// the EXISTING sidecar file (create_note already writes one via
/// save_metadata). A note owner can do this over the regular files API:
/// nothing seals bundle folders or validates sidecar content. The sidecar
/// fields under test (`visibility`, `public_share_id`) must therefore be
/// treated as untrusted input by toggle_visibility.
async fn plant_crafted_sidecar(
    ctx: &TestContext,
    planter_id: Uuid,
    note: &rustshare_server::services::note_service::Note,
    tenant_id: Uuid,
    visibility: &str,
    public_share_id: Option<&str>,
) {
    let parent = note.parent_folder_id.expect("folder-backed note");
    let siblings = ctx
        .metadata_store
        .list_files(Some(parent), note.owner_id, tenant_id)
        .await
        .expect("list bundle folder");
    let sidecar_name = format!("{}.rustshare.json", note.name);
    let sidecar = siblings
        .into_iter()
        .find(|f| f.name == sidecar_name)
        .expect("existing sidecar written by create_note/save_metadata");

    let now = chrono::Utc::now().to_rfc3339();
    let mut json = serde_json::json!({
        "kind": "note",
        "title": note.metadata.title,
        "visibility": visibility,
        "created_at": now,
        "updated_at": now,
        "excerpt": "",
        "mime_type": "text/markdown",
        "extension": "md",
    });
    if let Some(sid) = public_share_id {
        json["public_share_id"] = serde_json::Value::String(sid.to_string());
    }
    ctx.file_service()
        .edit_file(
            sidecar.id,
            planter_id,
            bytes::Bytes::from(json.to_string()),
            "overwrite",
            None,
        )
        .await
        .expect("plant crafted sidecar content");
}

/// Regression (GHSA-rvj5-hgxh-w7m7 review): the visibility sidecar is
/// client-writable, so an attacker owning note B can plant
/// `visibility: "public"` with a VICTIM's share id (disclosed to any share
/// recipient via note metadata) and unpublish the victim's note by toggling
/// their own. The unpublish branch must verify index ownership and refuse.
#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_notes_crafted_sidecar_cannot_unpublish_another_notes_public_url() {
    let ctx = setup_test_env().await;
    let tenant_id = ctx.tenant_id;
    let victim = ctx.create_test_user("note_adv_victim_u").await;
    let attacker = ctx.create_test_user("note_adv_attacker_u").await;
    let service = create_note_service(
        ctx.event_store.clone(),
        ctx.metadata_store.clone(),
        ctx.object_store.clone(),
        &ctx.pool,
    );

    // Victim's note, published. The share id is disclosed to every View/Edit
    // share holder via GET /api/v1/notes/{id} (metadata.public_share_id).
    let note_a = service
        .create_note(
            victim.id,
            tenant_id,
            Some("VictimNote".to_string()),
            None,
            Some("victim content".to_string()),
        )
        .await
        .unwrap();
    let public = service
        .toggle_visibility(note_a.id, victim.id, tenant_id)
        .await
        .unwrap();
    let share_id = public.metadata.public_share_id.clone().unwrap();
    assert!(service.get_public_note(&share_id).await.is_ok());

    // Attacker creates their OWN note and plants a sidecar claiming
    // visibility=public with the VICTIM's share id.
    let note_b = service
        .create_note(
            attacker.id,
            tenant_id,
            Some("AttackerNote".to_string()),
            None,
            Some("attacker content".to_string()),
        )
        .await
        .unwrap();
    plant_crafted_sidecar(
        &ctx,
        attacker.id,
        &note_b,
        tenant_id,
        "public",
        Some(&share_id),
    )
    .await;

    // Toggling the attacker's own note must be refused: the index entry for
    // the claimed share id belongs to the victim's note.
    let result = service
        .toggle_visibility(note_b.id, attacker.id, tenant_id)
        .await;
    assert!(
        matches!(result, Err(NoteError::Conflict(_))),
        "toggle must refuse a public share id registered to another note, got {:?}",
        result
    );

    // The victim's public URL must be intact and still serve victim content.
    let still_public = service.get_public_note(&share_id).await;
    assert!(
        still_public.is_ok(),
        "crafted sidecar must not unpublish the victim's note: {:?}",
        still_public.err()
    );
    assert_eq!(still_public.unwrap().title, "VictimNote");

    // The same forged share id must not be removable through delete_note
    // either: deleting the attacker's own note must leave the victim's
    // public index entry untouched.
    service
        .delete_note(note_b.id, attacker.id, tenant_id)
        .await
        .expect("attacker deletes own note");

    let after_delete = service.get_public_note(&share_id).await;
    assert!(
        after_delete.is_ok(),
        "crafted sidecar must not leak into delete_note's index cleanup: {:?}",
        after_delete.err()
    );
    assert_eq!(after_delete.unwrap().title, "VictimNote");

    // Owner unpublishes the victim note to leave no public residue.
    let _ = service
        .toggle_visibility(note_a.id, victim.id, tenant_id)
        .await
        .unwrap();

    ctx.cleanup().await;
}

/// Regression (GHSA-rvj5-hgxh-w7m7 review): a forged
/// `visibility: "private"` sidecar carrying the victim's share id made the
/// publish branch reuse that id and clobber the victim's index entry, so the
/// victim's public URL served attacker content. Publishing must always
/// allocate a fresh share id instead of trusting the sidecar.
#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_notes_crafted_sidecar_cannot_hijack_another_notes_public_url() {
    let ctx = setup_test_env().await;
    let tenant_id = ctx.tenant_id;
    let victim = ctx.create_test_user("note_adv_victim_h").await;
    let attacker = ctx.create_test_user("note_adv_attacker_h").await;
    let service = create_note_service(
        ctx.event_store.clone(),
        ctx.metadata_store.clone(),
        ctx.object_store.clone(),
        &ctx.pool,
    );

    let note_a = service
        .create_note(
            victim.id,
            tenant_id,
            Some("VictimNote".to_string()),
            None,
            Some("victim content".to_string()),
        )
        .await
        .unwrap();
    let public = service
        .toggle_visibility(note_a.id, victim.id, tenant_id)
        .await
        .unwrap();
    let share_id = public.metadata.public_share_id.clone().unwrap();

    let note_b = service
        .create_note(
            attacker.id,
            tenant_id,
            Some("AttackerNote".to_string()),
            None,
            Some("ATTACKER-CONTROLLED CONTENT".to_string()),
        )
        .await
        .unwrap();
    plant_crafted_sidecar(
        &ctx,
        attacker.id,
        &note_b,
        tenant_id,
        "private",
        Some(&share_id),
    )
    .await;

    // The attacker's own publish succeeds, but with a FRESH share id.
    let toggled = service
        .toggle_visibility(note_b.id, attacker.id, tenant_id)
        .await;
    assert!(
        toggled.is_ok(),
        "attacker publishing their own note should succeed: {:?}",
        toggled.err()
    );
    let attacker_share_id = toggled
        .unwrap()
        .metadata
        .public_share_id
        .expect("publish allocates a share id");
    assert_ne!(
        attacker_share_id, share_id,
        "publish must never reuse a sidecar-supplied share id"
    );

    // The victim's public URL still serves the victim's content.
    let victim_read = service.get_public_note(&share_id).await;
    assert!(
        victim_read.is_ok(),
        "victim's public URL must not be clobbered: {:?}",
        victim_read.err()
    );
    let victim_read = victim_read.unwrap();
    assert_eq!(victim_read.title, "VictimNote");
    assert!(victim_read.content.contains("victim content"));

    // The attacker's fresh URL serves attacker content under the new id.
    let attacker_read = service.get_public_note(&attacker_share_id).await;
    assert!(
        attacker_read.is_ok(),
        "attacker's fresh public URL should work: {:?}",
        attacker_read.err()
    );
    assert_eq!(attacker_read.unwrap().title, "AttackerNote");

    // Both parties unpublish to leave no public residue.
    let _ = service
        .toggle_visibility(note_b.id, attacker.id, tenant_id)
        .await
        .unwrap();
    let _ = service
        .toggle_visibility(note_a.id, victim.id, tenant_id)
        .await
        .unwrap();

    ctx.cleanup().await;
}

/// Coverage: a group-derived Admin share must allow toggle_visibility, the
/// same as a direct user Admin share.
#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_notes_group_admin_share_can_toggle_visibility() {
    let ctx = setup_test_env().await;
    let tenant_id = ctx.tenant_id;
    let owner = ctx.create_test_user("note_pub_owner_g").await;
    let member = ctx.create_test_user("note_pub_member_g").await;
    let service = create_note_service(
        ctx.event_store.clone(),
        ctx.metadata_store.clone(),
        ctx.object_store.clone(),
        &ctx.pool,
    );

    let note = service
        .create_note(
            owner.id,
            tenant_id,
            Some("GroupAdminNote".to_string()),
            None,
            Some("content".to_string()),
        )
        .await
        .unwrap();
    let group_id = create_test_group(&ctx.pool, "note-pub-g", tenant_id, owner.id).await;
    add_user_to_group(&ctx.pool, group_id, member.id).await;
    share_file_to_group(
        &ctx.metadata_store,
        note.id,
        owner.id,
        group_id,
        SharePermissions::Admin,
        tenant_id,
    )
    .await;

    let res = service
        .toggle_visibility(note.id, member.id, tenant_id)
        .await;
    assert!(
        res.is_ok(),
        "group-Admin recipient should be able to publish: {:?}",
        res.err()
    );
    assert_eq!(res.unwrap().metadata.visibility, NoteVisibility::Public);

    ctx.cleanup().await;
}

/// Coverage: an Admin share inherited through folder ancestry (the note's
/// bundle folder) must allow toggle_visibility.
#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_notes_folder_ancestry_admin_can_toggle_visibility() {
    let ctx = setup_test_env().await;
    let tenant_id = ctx.tenant_id;
    let owner = ctx.create_test_user("note_pub_owner_f").await;
    let user = ctx.create_test_user("note_pub_user_f").await;
    let service = create_note_service(
        ctx.event_store.clone(),
        ctx.metadata_store.clone(),
        ctx.object_store.clone(),
        &ctx.pool,
    );

    let note = service
        .create_note(
            owner.id,
            tenant_id,
            Some("FolderAdminNote".to_string()),
            None,
            Some("content".to_string()),
        )
        .await
        .unwrap();
    let bundle_folder = note.parent_folder_id.expect("folder-backed note");
    share_folder_to_user(
        &ctx.metadata_store,
        bundle_folder,
        owner.id,
        user.id,
        SharePermissions::Admin,
        tenant_id,
    )
    .await;

    let res = service.toggle_visibility(note.id, user.id, tenant_id).await;
    assert!(
        res.is_ok(),
        "inherited folder-Admin should be able to publish: {:?}",
        res.err()
    );
    assert_eq!(res.unwrap().metadata.visibility, NoteVisibility::Public);

    ctx.cleanup().await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_notes_view_share_recipient_cannot_publish_or_unpublish() {
    let ctx = setup_test_env().await;
    let tenant_id = ctx.tenant_id;
    let owner = ctx.create_test_user("note_pub_owner_v").await;
    let viewer = ctx.create_test_user("note_pub_viewer_v").await;
    let service = create_note_service(
        ctx.event_store.clone(),
        ctx.metadata_store.clone(),
        ctx.object_store.clone(),
        &ctx.pool,
    );

    let note = service
        .create_note(
            owner.id,
            tenant_id,
            Some("OwnerSecret".to_string()),
            None,
            Some("classified body".to_string()),
        )
        .await
        .unwrap();
    share_file_to_user(
        &ctx.metadata_store,
        note.id,
        owner.id,
        viewer.id,
        SharePermissions::View,
        tenant_id,
    )
    .await;

    // View-only recipient can read (share works): the file resolves for them
    // through the same permission primitive toggle_visibility starts with...
    let file = ctx.file_service().get_file(note.id, viewer.id).await;
    assert!(
        file.is_ok(),
        "View share recipient should be able to resolve the note file"
    );

    // ...but cannot publish it.
    let result = service
        .toggle_visibility(note.id, viewer.id, tenant_id)
        .await;
    assert!(
        matches!(result, Err(NoteError::PermissionDenied)),
        "View-only share recipient must not be able to publish the note, got {:?}",
        result
    );

    // Denied publish must leave state untouched: still private, no share id.
    let after = service
        .get_note(note.id, owner.id, tenant_id)
        .await
        .unwrap();
    assert_eq!(
        after.metadata.visibility,
        NoteVisibility::Private,
        "denied publish must not change visibility"
    );
    assert!(
        after.metadata.public_share_id.is_none(),
        "denied publish must not allocate a public share id"
    );

    // Owner publishes; the View-only recipient must also not be able to
    // change public visibility (unpublish) of the owner's note.
    let public = service
        .toggle_visibility(note.id, owner.id, tenant_id)
        .await
        .unwrap();
    let share_id = public.metadata.public_share_id.clone().unwrap();

    let result = service
        .toggle_visibility(note.id, viewer.id, tenant_id)
        .await;
    assert!(
        matches!(result, Err(NoteError::PermissionDenied)),
        "View-only share recipient must not be able to unpublish the note, got {:?}",
        result
    );

    // Denied unpublish must leave the public index and visibility intact.
    let after = service
        .get_note(note.id, owner.id, tenant_id)
        .await
        .unwrap();
    assert_eq!(
        after.metadata.visibility,
        NoteVisibility::Public,
        "denied unpublish must not change visibility"
    );
    assert_eq!(
        after.metadata.public_share_id.as_deref(),
        Some(share_id.as_str())
    );
    let still_public = service.get_public_note(&share_id).await;
    assert!(
        still_public.is_ok(),
        "denied unpublish must not remove the note from the public index"
    );

    // Owner unpublishes to leave no public residue.
    let _ = service
        .toggle_visibility(note.id, owner.id, tenant_id)
        .await
        .unwrap();

    ctx.cleanup().await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_notes_edit_share_recipient_cannot_toggle_visibility() {
    let ctx = setup_test_env().await;
    let tenant_id = ctx.tenant_id;
    let owner = ctx.create_test_user("note_pub_owner_e").await;
    let editor = ctx.create_test_user("note_pub_editor_e").await;
    let service = create_note_service(
        ctx.event_store.clone(),
        ctx.metadata_store.clone(),
        ctx.object_store.clone(),
        &ctx.pool,
    );

    let note = service
        .create_note(
            owner.id,
            tenant_id,
            Some("EditorNote".to_string()),
            None,
            Some("draft".to_string()),
        )
        .await
        .unwrap();
    share_file_to_user(
        &ctx.metadata_store,
        note.id,
        owner.id,
        editor.id,
        SharePermissions::Edit,
        tenant_id,
    )
    .await;

    // Editor workflow still works: content editing is permitted with Edit.
    let saved = service
        .save_note(
            note.id,
            editor.id,
            tenant_id,
            "edited body".to_string(),
            None,
            None,
        )
        .await;
    assert!(
        saved.is_ok(),
        "Edit share recipient should be able to edit note content, got {:?}",
        saved
    );

    // Editing content does not grant control over public visibility.
    let result = service
        .toggle_visibility(note.id, editor.id, tenant_id)
        .await;
    assert!(
        matches!(result, Err(NoteError::PermissionDenied)),
        "Edit share recipient must not be able to change public visibility, got {:?}",
        result
    );

    let after = service
        .get_note(note.id, owner.id, tenant_id)
        .await
        .unwrap();
    assert_eq!(
        after.metadata.visibility,
        NoteVisibility::Private,
        "denied toggle must not change visibility"
    );
    assert!(
        after.metadata.public_share_id.is_none(),
        "denied toggle must not allocate a public share id"
    );

    ctx.cleanup().await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_notes_no_share_user_cannot_toggle_visibility() {
    let ctx = setup_test_env().await;
    let tenant_id = ctx.tenant_id;
    let owner = ctx.create_test_user("note_pub_owner_n").await;
    let stranger = ctx.create_test_user("note_pub_stranger_n").await;
    let service = create_note_service(
        ctx.event_store.clone(),
        ctx.metadata_store.clone(),
        ctx.object_store.clone(),
        &ctx.pool,
    );

    let note = service
        .create_note(
            owner.id,
            tenant_id,
            Some("HiddenNote".to_string()),
            None,
            Some("secret".to_string()),
        )
        .await
        .unwrap();

    let result = service
        .toggle_visibility(note.id, stranger.id, tenant_id)
        .await;
    assert!(
        matches!(result, Err(NoteError::PermissionDenied)),
        "user without any share must not be able to change public visibility, got {:?}",
        result
    );

    let after = service
        .get_note(note.id, owner.id, tenant_id)
        .await
        .unwrap();
    assert_eq!(
        after.metadata.visibility,
        NoteVisibility::Private,
        "denied toggle must not change visibility"
    );
    assert!(
        after.metadata.public_share_id.is_none(),
        "denied toggle must not allocate a public share id"
    );

    ctx.cleanup().await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_notes_admin_share_recipient_can_toggle_visibility() {
    let ctx = setup_test_env().await;
    let tenant_id = ctx.tenant_id;
    let owner = ctx.create_test_user("note_pub_owner_a").await;
    let admin = ctx.create_test_user("note_pub_admin_a").await;
    let service = create_note_service(
        ctx.event_store.clone(),
        ctx.metadata_store.clone(),
        ctx.object_store.clone(),
        &ctx.pool,
    );

    let note = service
        .create_note(
            owner.id,
            tenant_id,
            Some("DelegatedNote".to_string()),
            None,
            Some("shared work".to_string()),
        )
        .await
        .unwrap();
    share_file_to_user(
        &ctx.metadata_store,
        note.id,
        owner.id,
        admin.id,
        SharePermissions::Admin,
        tenant_id,
    )
    .await;

    // Admin (the highest delegated share tier, per SharePermissions) may
    // publish, mirroring the delegation semantics of delete_file.
    let public = service
        .toggle_visibility(note.id, admin.id, tenant_id)
        .await;
    assert!(
        public.is_ok(),
        "Admin share recipient should be able to publish, got {:?}",
        public
    );
    let public = public.unwrap();
    assert_eq!(public.metadata.visibility, NoteVisibility::Public);
    let share_id = public.metadata.public_share_id.clone().unwrap();

    let public_note = service.get_public_note(&share_id).await;
    assert!(
        public_note.is_ok(),
        "note published by Admin share recipient should be publicly readable"
    );

    // The owner retains control and can unpublish afterwards.
    let private = service
        .toggle_visibility(note.id, owner.id, tenant_id)
        .await
        .unwrap();
    assert_eq!(private.metadata.visibility, NoteVisibility::Private);
    let result = service.get_public_note(&share_id).await;
    assert!(result.is_err(), "unpublished note must deny public access");

    ctx.cleanup().await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_notes_owner_toggle_visibility_publish_unpublish() {
    let ctx = setup_test_env().await;
    let tenant_id = ctx.tenant_id;
    let owner = ctx.create_test_user("note_pub_owner_p").await;
    let service = create_note_service(
        ctx.event_store.clone(),
        ctx.metadata_store.clone(),
        ctx.object_store.clone(),
        &ctx.pool,
    );

    let note = service
        .create_note(
            owner.id,
            tenant_id,
            Some("PublicNote".to_string()),
            None,
            Some("public body".to_string()),
        )
        .await
        .unwrap();

    // Owner publishes.
    let public = service
        .toggle_visibility(note.id, owner.id, tenant_id)
        .await
        .unwrap();
    assert_eq!(public.metadata.visibility, NoteVisibility::Public);
    let share_id = public.metadata.public_share_id.clone().unwrap();
    let public_note = service.get_public_note(&share_id).await.unwrap();
    assert_eq!(public_note.title, "PublicNote");

    // Owner unpublishes; public access is revoked and the share id rotated out.
    let private = service
        .toggle_visibility(note.id, owner.id, tenant_id)
        .await
        .unwrap();
    assert_eq!(private.metadata.visibility, NoteVisibility::Private);
    assert!(private.metadata.public_share_id.is_none());
    let result = service.get_public_note(&share_id).await;
    assert!(
        result.is_err(),
        "unpublished note must not be readable through the public route"
    );

    // Republishing allocates a FRESH share id: the sidecar is client-writable,
    // so a previously used id must never be trusted again (the old URL stays
    // dead). Callers receive the new id/public_url in the toggle response.
    let republished = service
        .toggle_visibility(note.id, owner.id, tenant_id)
        .await
        .unwrap();
    let new_share_id = republished
        .metadata
        .public_share_id
        .clone()
        .expect("republish allocates a share id");
    assert_ne!(
        new_share_id, share_id,
        "republish must rotate the public share id"
    );
    assert!(service.get_public_note(&share_id).await.is_err());
    let republished_note = service.get_public_note(&new_share_id).await;
    assert!(
        republished_note.is_ok(),
        "republished note must be readable under the fresh share id"
    );
    assert_eq!(republished_note.unwrap().title, "PublicNote");

    ctx.cleanup().await;
}

// ============================================================================
// Decisions
// ============================================================================

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_decisions_cross_tenant_get_denied() {
    let ctx = setup_test_env().await;
    let tenant_a = ctx.tenant_id;
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
        .create_decision(
            user_a.id,
            tenant_a,
            "Secret".to_string(),
            "Exec".to_string(),
            "content".to_string(),
        )
        .await
        .unwrap();

    let result = service.get_decision(decision.id, user_b.id, tenant_b).await;
    assert!(
        matches!(result, Err(DecisionError::PermissionDenied)),
        "Cross-tenant get_decision should be denied, got {:?}",
        result
    );

    cleanup_tenant(&ctx.pool, tenant_b).await;
    ctx.cleanup().await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_decisions_cross_tenant_update_denied() {
    let ctx = setup_test_env().await;
    let tenant_a = ctx.tenant_id;
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
        .create_decision(
            user_a.id,
            tenant_a,
            "Secret".to_string(),
            "Exec".to_string(),
            "content".to_string(),
        )
        .await
        .unwrap();

    let result = service
        .update_decision(
            decision.id,
            user_b.id,
            tenant_b,
            Some("Hacked".to_string()),
            None,
            Some("evil".to_string()),
        )
        .await;
    assert!(
        matches!(result, Err(DecisionError::PermissionDenied)),
        "Cross-tenant update_decision should be denied, got {:?}",
        result
    );

    cleanup_tenant(&ctx.pool, tenant_b).await;
    ctx.cleanup().await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_decisions_cross_tenant_rename_denied() {
    let ctx = setup_test_env().await;
    let tenant_a = ctx.tenant_id;
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
        .create_decision(
            user_a.id,
            tenant_a,
            "Secret".to_string(),
            "Exec".to_string(),
            "content".to_string(),
        )
        .await
        .unwrap();

    let result = service
        .rename_decision(decision.id, user_b.id, tenant_b, "Hacked".to_string())
        .await;
    assert!(
        matches!(result, Err(DecisionError::PermissionDenied)),
        "Cross-tenant rename_decision should be denied, got {:?}",
        result
    );

    cleanup_tenant(&ctx.pool, tenant_b).await;
    ctx.cleanup().await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_decisions_cross_tenant_list_does_not_leak() {
    let ctx = setup_test_env().await;
    let tenant_a = ctx.tenant_id;
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
        .create_decision(
            user_a.id,
            tenant_a,
            "Secret".to_string(),
            "Exec".to_string(),
            "content".to_string(),
        )
        .await
        .unwrap();

    let list_b = service
        .list_decisions(user_b.id, tenant_b, 1000, 0)
        .await
        .unwrap();
    assert!(
        !list_b.iter().any(|d| d.metadata.title == "Secret"),
        "Cross-tenant list_decisions should not leak decisions"
    );

    cleanup_tenant(&ctx.pool, tenant_b).await;
    ctx.cleanup().await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_decisions_same_tenant_unauthorized_denied() {
    let ctx = setup_test_env().await;
    let tenant_id = ctx.tenant_id;
    let user_owner = create_test_user(&ctx.metadata_store, "decision_owner", tenant_id).await;
    let user_other = create_test_user(&ctx.metadata_store, "decision_other", tenant_id).await;
    let service = create_decision_service(
        ctx.event_store.clone(),
        ctx.metadata_store.clone(),
        ctx.object_store.clone(),
        &ctx.pool,
    );

    let decision = service
        .create_decision(
            user_owner.id,
            tenant_id,
            "Private".to_string(),
            "Exec".to_string(),
            "content".to_string(),
        )
        .await
        .unwrap();

    let result = service
        .get_decision(decision.id, user_other.id, tenant_id)
        .await;
    assert!(
        matches!(result, Err(DecisionError::PermissionDenied)),
        "Same-tenant unauthorized get_decision should be denied, got {:?}",
        result
    );

    ctx.cleanup().await;
}

// ============================================================================
// Kanban
// ============================================================================

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_kanban_cross_tenant_get_board_denied() {
    let ctx = setup_test_env().await;
    let tenant_a = ctx.tenant_id;
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

    cleanup_tenant(&ctx.pool, tenant_b).await;
    ctx.cleanup().await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_kanban_cross_tenant_update_board_denied() {
    let ctx = setup_test_env().await;
    let tenant_a = ctx.tenant_id;
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

    cleanup_tenant(&ctx.pool, tenant_b).await;
    ctx.cleanup().await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_kanban_cross_tenant_delete_card_denied() {
    let ctx = setup_test_env().await;
    let tenant_a = ctx.tenant_id;
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

    cleanup_tenant(&ctx.pool, tenant_b).await;
    ctx.cleanup().await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_kanban_cross_tenant_list_boards_does_not_leak() {
    let ctx = setup_test_env().await;
    let tenant_a = ctx.tenant_id;
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

    cleanup_tenant(&ctx.pool, tenant_b).await;
    ctx.cleanup().await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_kanban_same_tenant_unauthorized_get_board_denied() {
    let ctx = setup_test_env().await;
    let tenant_id = ctx.tenant_id;
    let user_owner = create_test_user(&ctx.metadata_store, "kanban_owner", tenant_id).await;
    let user_other = create_test_user(&ctx.metadata_store, "kanban_other", tenant_id).await;
    let service = create_kanban_service(
        ctx.event_store.clone(),
        ctx.metadata_store.clone(),
        ctx.object_store.clone(),
        &ctx.pool,
    );

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

    ctx.cleanup().await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_kanban_same_tenant_unauthorized_get_card_detail_denied() {
    let ctx = setup_test_env().await;
    let tenant_id = ctx.tenant_id;
    let user_owner = create_test_user(&ctx.metadata_store, "kanban_owner2", tenant_id).await;
    let user_other = create_test_user(&ctx.metadata_store, "kanban_other2", tenant_id).await;
    let service = create_kanban_service(
        ctx.event_store.clone(),
        ctx.metadata_store.clone(),
        ctx.object_store.clone(),
        &ctx.pool,
    );

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

    ctx.cleanup().await;
}

// ============================================================================
// Brainstorming
// ============================================================================

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_brainstorming_cross_tenant_get_board_denied() {
    let ctx = setup_test_env().await;
    let tenant_a = ctx.tenant_id;
    let tenant_b = setup_test_tenant(&ctx.pool).await;
    let user_a = create_test_user(&ctx.metadata_store, "bs_ct_a", tenant_a).await;
    let user_b = create_test_user(&ctx.metadata_store, "bs_ct_b", tenant_b).await;
    let service = create_brainstorming_service(
        ctx.event_store.clone(),
        ctx.metadata_store.clone(),
        ctx.object_store.clone(),
        &ctx.pool,
    );

    let root = service
        .ensure_brainstorming_root(user_a.id, tenant_a)
        .await
        .unwrap();
    let board_folder = create_test_folder(
        &ctx.folder_service(),
        user_a.id,
        tenant_a,
        "secret-board",
        Some(root.id),
    )
    .await;
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
    create_hidden_test_file(
        &ctx,
        user_a.id,
        tenant_a,
        &board_folder,
        ".rustshare.json",
        meta.as_bytes(),
        "application/json",
    )
    .await;

    let result = service
        .get_board(board_folder.id, user_b.id, tenant_b)
        .await;
    assert!(
        matches!(result, Err(BrainstormError::PermissionDenied)),
        "Cross-tenant get_board should be denied, got {:?}",
        result
    );

    cleanup_tenant(&ctx.pool, tenant_b).await;
    ctx.cleanup().await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_brainstorming_cross_tenant_save_source_denied() {
    let ctx = setup_test_env().await;
    let tenant_a = ctx.tenant_id;
    let tenant_b = setup_test_tenant(&ctx.pool).await;
    let user_a = create_test_user(&ctx.metadata_store, "bs_ct_a2", tenant_a).await;
    let user_b = create_test_user(&ctx.metadata_store, "bs_ct_b2", tenant_b).await;
    let service = create_brainstorming_service(
        ctx.event_store.clone(),
        ctx.metadata_store.clone(),
        ctx.object_store.clone(),
        &ctx.pool,
    );

    let root = service
        .ensure_brainstorming_root(user_a.id, tenant_a)
        .await
        .unwrap();
    let board_folder = create_test_folder(
        &ctx.folder_service(),
        user_a.id,
        tenant_a,
        "secret-board",
        Some(root.id),
    )
    .await;
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
    create_hidden_test_file(
        &ctx,
        user_a.id,
        tenant_a,
        &board_folder,
        ".rustshare.json",
        meta.as_bytes(),
        "application/json",
    )
    .await;

    let result = service
        .save_board_source(
            board_folder.id,
            user_b.id,
            tenant_b,
            r#"{"type":"excalidraw","version":2,"elements":[]}"#.to_string(),
        )
        .await;
    assert!(
        matches!(result, Err(BrainstormError::PermissionDenied)),
        "Cross-tenant save_board_source should be denied, got {:?}",
        result
    );

    cleanup_tenant(&ctx.pool, tenant_b).await;
    ctx.cleanup().await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_brainstorming_cross_tenant_delete_board_denied() {
    let ctx = setup_test_env().await;
    let tenant_a = ctx.tenant_id;
    let tenant_b = setup_test_tenant(&ctx.pool).await;
    let user_a = create_test_user(&ctx.metadata_store, "bs_ct_a3", tenant_a).await;
    let user_b = create_test_user(&ctx.metadata_store, "bs_ct_b3", tenant_b).await;
    let service = create_brainstorming_service(
        ctx.event_store.clone(),
        ctx.metadata_store.clone(),
        ctx.object_store.clone(),
        &ctx.pool,
    );

    let root = service
        .ensure_brainstorming_root(user_a.id, tenant_a)
        .await
        .unwrap();
    let board_folder = create_test_folder(
        &ctx.folder_service(),
        user_a.id,
        tenant_a,
        "secret-board",
        Some(root.id),
    )
    .await;
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
    create_hidden_test_file(
        &ctx,
        user_a.id,
        tenant_a,
        &board_folder,
        ".rustshare.json",
        meta.as_bytes(),
        "application/json",
    )
    .await;

    let result = service
        .delete_board(board_folder.id, user_b.id, tenant_b)
        .await;
    assert!(
        matches!(result, Err(BrainstormError::PermissionDenied)),
        "Cross-tenant delete_board should be denied, got {:?}",
        result
    );

    cleanup_tenant(&ctx.pool, tenant_b).await;
    ctx.cleanup().await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_brainstorming_cross_tenant_list_does_not_leak() {
    let ctx = setup_test_env().await;
    let tenant_a = ctx.tenant_id;
    let tenant_b = setup_test_tenant(&ctx.pool).await;
    let user_a = create_test_user(&ctx.metadata_store, "bs_ct_a4", tenant_a).await;
    let user_b = create_test_user(&ctx.metadata_store, "bs_ct_b4", tenant_b).await;
    let service = create_brainstorming_service(
        ctx.event_store.clone(),
        ctx.metadata_store.clone(),
        ctx.object_store.clone(),
        &ctx.pool,
    );

    let root = service
        .ensure_brainstorming_root(user_a.id, tenant_a)
        .await
        .unwrap();
    let board_folder = create_test_folder(
        &ctx.folder_service(),
        user_a.id,
        tenant_a,
        "secret-board",
        Some(root.id),
    )
    .await;
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
    create_hidden_test_file(
        &ctx,
        user_a.id,
        tenant_a,
        &board_folder,
        ".rustshare.json",
        meta.as_bytes(),
        "application/json",
    )
    .await;

    let list_b = service
        .list_boards(user_b.id, tenant_b, 1000, 0)
        .await
        .unwrap();
    assert!(
        !list_b.iter().any(|b| b.title == "Secret"),
        "Cross-tenant list_boards should not leak boards"
    );

    cleanup_tenant(&ctx.pool, tenant_b).await;
    ctx.cleanup().await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_brainstorming_same_tenant_unauthorized_get_board_denied() {
    let ctx = setup_test_env().await;
    let tenant_id = ctx.tenant_id;
    let user_owner = create_test_user(&ctx.metadata_store, "bs_owner", tenant_id).await;
    let user_other = create_test_user(&ctx.metadata_store, "bs_other", tenant_id).await;
    let service = create_brainstorming_service(
        ctx.event_store.clone(),
        ctx.metadata_store.clone(),
        ctx.object_store.clone(),
        &ctx.pool,
    );

    let root = service
        .ensure_brainstorming_root(user_owner.id, tenant_id)
        .await
        .unwrap();
    let board_folder = create_test_folder(
        &ctx.folder_service(),
        user_owner.id,
        tenant_id,
        "private-board",
        Some(root.id),
    )
    .await;
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
    create_hidden_test_file(
        &ctx,
        user_owner.id,
        tenant_id,
        &board_folder,
        ".rustshare.json",
        meta.as_bytes(),
        "application/json",
    )
    .await;

    let result = service
        .get_board(board_folder.id, user_other.id, tenant_id)
        .await;
    assert!(
        matches!(result, Err(BrainstormError::PermissionDenied)),
        "Same-tenant unauthorized get_board should be denied, got {:?}",
        result
    );

    ctx.cleanup().await;
}

// ============================================================================
// Meetings
// ============================================================================

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_meetings_cross_tenant_get_denied() {
    let ctx = setup_test_env().await;
    let tenant_a = ctx.tenant_id;
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

    cleanup_tenant(&ctx.pool, tenant_b).await;
    ctx.cleanup().await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_meetings_cross_tenant_update_denied() {
    let ctx = setup_test_env().await;
    let tenant_a = ctx.tenant_id;
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
        .update_meeting(
            meeting.id,
            user_b.id,
            tenant_b,
            Some("Hacked".to_string()),
            Some("evil".to_string()),
            None,
        )
        .await;
    assert!(
        matches!(result, Err(MeetingError::PermissionDenied)),
        "Cross-tenant update_meeting should be denied, got {:?}",
        result
    );

    cleanup_tenant(&ctx.pool, tenant_b).await;
    ctx.cleanup().await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_meetings_cross_tenant_list_does_not_leak() {
    let ctx = setup_test_env().await;
    let tenant_a = ctx.tenant_id;
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

    let list_b = service
        .list_meetings(user_b.id, tenant_b, 1000, 0)
        .await
        .unwrap();
    assert!(
        !list_b.iter().any(|m| m.metadata.title == "Secret"),
        "Cross-tenant list_meetings should not leak meetings"
    );

    cleanup_tenant(&ctx.pool, tenant_b).await;
    ctx.cleanup().await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_meetings_same_tenant_unauthorized_denied() {
    let ctx = setup_test_env().await;
    let tenant_id = ctx.tenant_id;
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

    let result = service
        .get_meeting(meeting.id, user_other.id, tenant_id)
        .await;
    assert!(
        matches!(result, Err(MeetingError::PermissionDenied)),
        "Same-tenant unauthorized get_meeting should be denied, got {:?}",
        result
    );

    ctx.cleanup().await;
}

// ============================================================================
// Standups
// ============================================================================

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_standups_cross_tenant_get_denied() {
    let ctx = setup_test_env().await;
    let tenant_a = ctx.tenant_id;
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

    cleanup_tenant(&ctx.pool, tenant_b).await;
    ctx.cleanup().await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_standups_cross_tenant_update_denied() {
    let ctx = setup_test_env().await;
    let tenant_a = ctx.tenant_id;
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
        .update_standup(
            standup.id,
            user_b.id,
            tenant_b,
            Some("Hacked".to_string()),
            Some("evil".to_string()),
        )
        .await;
    assert!(
        matches!(result, Err(StandupError::PermissionDenied)),
        "Cross-tenant update_standup should be denied, got {:?}",
        result
    );

    cleanup_tenant(&ctx.pool, tenant_b).await;
    ctx.cleanup().await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_standups_cross_tenant_list_does_not_leak() {
    let ctx = setup_test_env().await;
    let tenant_a = ctx.tenant_id;
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

    let list_b = service
        .list_standups(user_b.id, tenant_b, 1000, 0)
        .await
        .unwrap();
    assert!(
        !list_b.iter().any(|s| s.metadata.title == "Secret"),
        "Cross-tenant list_standups should not leak standups"
    );

    cleanup_tenant(&ctx.pool, tenant_b).await;
    ctx.cleanup().await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_standups_same_tenant_unauthorized_denied() {
    let ctx = setup_test_env().await;
    let tenant_id = ctx.tenant_id;
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

    let result = service
        .get_standup(standup.id, user_other.id, tenant_id)
        .await;
    assert!(
        matches!(result, Err(StandupError::PermissionDenied)),
        "Same-tenant unauthorized get_standup should be denied, got {:?}",
        result
    );

    ctx.cleanup().await;
}
