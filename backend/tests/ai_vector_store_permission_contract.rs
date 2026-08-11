//! Backend-agnostic permission contract for AI vector stores.
//!
//! Runs the same semantic scenarios against both the in-memory and PostgreSQL
//! pgvector backends to prove that ACL enforcement is store-independent.

use std::collections::HashMap;
use uuid::Uuid;

use rustshare_core::services::ai::{
    EmbeddingPolicy, IndexPrincipal, IndexVisibility, IndexedDocument, NoteAclPayload,
    RetrievalPrincipal, VectorStore,
};
use rustshare_core::services::InMemoryVectorStore;
use rustshare_infrastructure::PgVectorStore;

const EMBEDDING_DIM: usize = 768;

fn unit_embedding() -> Vec<f32> {
    vec![1.0f32; EMBEDDING_DIM]
}

#[allow(clippy::too_many_arguments)]
fn make_acl_payload(
    tenant_id: Uuid,
    note_id: Uuid,
    source_file_id: Uuid,
    owner_id: Uuid,
    read_principals: Vec<IndexPrincipal>,
    visibility: IndexVisibility,
    embedding_policy: EmbeddingPolicy,
    acl_version: i64,
) -> NoteAclPayload {
    NoteAclPayload {
        tenant_id,
        workspace_id: tenant_id,
        note_id,
        source_file_id,
        source_folder_id: None,
        owner_id,
        read_acl: read_principals.iter().map(|p| p.to_string()).collect(),
        visibility: visibility.to_string(),
        acl_hash: format!("hash-{acl_version}"),
        acl_version,
        embedding_policy: embedding_policy.to_string(),
    }
}

fn make_indexed_doc(
    chunk_id: Uuid,
    tenant_id: Uuid,
    _note_id: Uuid,
    owner_id: Uuid,
    content: &str,
    acl: NoteAclPayload,
) -> IndexedDocument {
    IndexedDocument {
        file_id: chunk_id,
        file_name: "contract.md".to_string(),
        file_path: "/contract.md".to_string(),
        content: content.to_string(),
        embedding: unit_embedding(),
        mime_type: "text/markdown".to_string(),
        owner_id,
        tenant_id,
        indexed_at: chrono::Utc::now(),
        acl: Some(acl),
        chunk_id,
    }
}

fn make_principal(
    tenant_id: Uuid,
    workspace_id: Option<Uuid>,
    user_id: Uuid,
    group_ids: Vec<Uuid>,
) -> RetrievalPrincipal {
    RetrievalPrincipal {
        tenant_id,
        workspace_id,
        user_id,
        group_ids,
        min_acl_versions: HashMap::new(),
    }
}

fn owner_principal(tenant_id: Uuid, owner_id: Uuid) -> RetrievalPrincipal {
    make_principal(tenant_id, None, owner_id, vec![])
}

fn user_principal(tenant_id: Uuid, user_id: Uuid) -> RetrievalPrincipal {
    make_principal(tenant_id, None, user_id, vec![])
}

fn group_member_principal(tenant_id: Uuid, user_id: Uuid, group_id: Uuid) -> RetrievalPrincipal {
    make_principal(tenant_id, None, user_id, vec![group_id])
}

fn workspace_member_principal(
    tenant_id: Uuid,
    workspace_id: Uuid,
    user_id: Uuid,
) -> RetrievalPrincipal {
    make_principal(tenant_id, Some(workspace_id), user_id, vec![])
}

/// Run the full permission contract against any VectorStore implementation.
///
/// `supports_acl_less_chunks` should be `true` only for backends that can
/// represent a chunk with no ACL payload at all (e.g., the in-memory store).
/// PostgreSQL stores every ACL column with a NOT NULL default, so the legacy
/// ACL-less path is tested only where it can actually occur.
async fn run_permission_contract<S: VectorStore>(store: &S, supports_acl_less_chunks: bool) {
    // Scenario 1: owner can retrieve their own private note.
    {
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let note_id = Uuid::new_v4();
        let chunk_id = Uuid::new_v4();
        let acl = make_acl_payload(
            tenant_id,
            note_id,
            chunk_id,
            owner_id,
            vec![IndexPrincipal::Owner(owner_id)],
            IndexVisibility::Private,
            EmbeddingPolicy::Allowed,
            1,
        );
        let doc = make_indexed_doc(chunk_id, tenant_id, note_id, owner_id, "owner content", acl);
        store
            .upsert_chunk(tenant_id, chunk_id, &doc, doc.acl.as_ref().unwrap())
            .await
            .unwrap();

        let results = store
            .search_with_acl(&owner_principal(tenant_id, owner_id), &unit_embedding(), 10)
            .await
            .unwrap();
        assert_eq!(results.len(), 1, "owner must retrieve own private note");

        let _ = store.clear_tenant(tenant_id).await;
    }

    // Scenario 2: unrelated user in the same tenant cannot retrieve a private note.
    {
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let stranger_id = Uuid::new_v4();
        let note_id = Uuid::new_v4();
        let chunk_id = Uuid::new_v4();
        let acl = make_acl_payload(
            tenant_id,
            note_id,
            chunk_id,
            owner_id,
            vec![IndexPrincipal::Owner(owner_id)],
            IndexVisibility::Private,
            EmbeddingPolicy::Allowed,
            1,
        );
        let doc = make_indexed_doc(
            chunk_id,
            tenant_id,
            note_id,
            owner_id,
            "private content",
            acl,
        );
        store
            .upsert_chunk(tenant_id, chunk_id, &doc, doc.acl.as_ref().unwrap())
            .await
            .unwrap();

        let results = store
            .search_with_acl(
                &user_principal(tenant_id, stranger_id),
                &unit_embedding(),
                10,
            )
            .await
            .unwrap();
        assert!(
            results.is_empty(),
            "stranger must not retrieve private note"
        );

        let _ = store.clear_tenant(tenant_id).await;
    }

    // Scenario 3: user with a direct read share can retrieve the note.
    {
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let shared_user_id = Uuid::new_v4();
        let note_id = Uuid::new_v4();
        let chunk_id = Uuid::new_v4();
        let acl = make_acl_payload(
            tenant_id,
            note_id,
            chunk_id,
            owner_id,
            vec![
                IndexPrincipal::Owner(owner_id),
                IndexPrincipal::User(shared_user_id),
            ],
            IndexVisibility::Private,
            EmbeddingPolicy::Allowed,
            1,
        );
        let doc = make_indexed_doc(
            chunk_id,
            tenant_id,
            note_id,
            owner_id,
            "shared content",
            acl,
        );
        store
            .upsert_chunk(tenant_id, chunk_id, &doc, doc.acl.as_ref().unwrap())
            .await
            .unwrap();

        let results = store
            .search_with_acl(
                &user_principal(tenant_id, shared_user_id),
                &unit_embedding(),
                10,
            )
            .await
            .unwrap();
        assert_eq!(results.len(), 1, "shared user must retrieve note");

        let _ = store.clear_tenant(tenant_id).await;
    }

    // Scenario 4: group member can retrieve a group-shared note.
    {
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let group_member_id = Uuid::new_v4();
        let group_id = Uuid::new_v4();
        let note_id = Uuid::new_v4();
        let chunk_id = Uuid::new_v4();
        let acl = make_acl_payload(
            tenant_id,
            note_id,
            chunk_id,
            owner_id,
            vec![IndexPrincipal::Group(group_id)],
            IndexVisibility::Private,
            EmbeddingPolicy::Allowed,
            1,
        );
        let doc = make_indexed_doc(chunk_id, tenant_id, note_id, owner_id, "group content", acl);
        store
            .upsert_chunk(tenant_id, chunk_id, &doc, doc.acl.as_ref().unwrap())
            .await
            .unwrap();

        let results = store
            .search_with_acl(
                &group_member_principal(tenant_id, group_member_id, group_id),
                &unit_embedding(),
                10,
            )
            .await
            .unwrap();
        assert_eq!(
            results.len(),
            1,
            "group member must retrieve group-shared note"
        );

        let _ = store.clear_tenant(tenant_id).await;
    }

    // Scenario 5: workspace-visible note is retrievable by a workspace member.
    {
        let tenant_id = Uuid::new_v4();
        let workspace_id = tenant_id; // Domain guarantees single workspace per tenant.
        let owner_id = Uuid::new_v4();
        let member_id = Uuid::new_v4();
        let note_id = Uuid::new_v4();
        let chunk_id = Uuid::new_v4();
        let acl = make_acl_payload(
            tenant_id,
            note_id,
            chunk_id,
            owner_id,
            vec![],
            IndexVisibility::Workspace,
            EmbeddingPolicy::Allowed,
            1,
        );
        let doc = make_indexed_doc(
            chunk_id,
            tenant_id,
            note_id,
            owner_id,
            "workspace content",
            acl,
        );
        store
            .upsert_chunk(tenant_id, chunk_id, &doc, doc.acl.as_ref().unwrap())
            .await
            .unwrap();

        let results = store
            .search_with_acl(
                &workspace_member_principal(tenant_id, workspace_id, member_id),
                &unit_embedding(),
                10,
            )
            .await
            .unwrap();
        assert_eq!(
            results.len(),
            1,
            "workspace member must retrieve workspace note"
        );

        let _ = store.clear_tenant(tenant_id).await;
    }

    // Scenario 6: public note is retrievable by any tenant user.
    {
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let public_user_id = Uuid::new_v4();
        let note_id = Uuid::new_v4();
        let chunk_id = Uuid::new_v4();
        let acl = make_acl_payload(
            tenant_id,
            note_id,
            chunk_id,
            owner_id,
            vec![IndexPrincipal::Public],
            IndexVisibility::Public,
            EmbeddingPolicy::Allowed,
            1,
        );
        let doc = make_indexed_doc(
            chunk_id,
            tenant_id,
            note_id,
            owner_id,
            "public content",
            acl,
        );
        store
            .upsert_chunk(tenant_id, chunk_id, &doc, doc.acl.as_ref().unwrap())
            .await
            .unwrap();

        let results = store
            .search_with_acl(
                &user_principal(tenant_id, public_user_id),
                &unit_embedding(),
                10,
            )
            .await
            .unwrap();
        assert_eq!(
            results.len(),
            1,
            "any tenant user must retrieve public note"
        );

        let _ = store.clear_tenant(tenant_id).await;
    }

    // Scenario 7: cross-tenant access fails closed.
    {
        let tenant_a = Uuid::new_v4();
        let tenant_b = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let other_tenant_user = Uuid::new_v4();
        let note_id = Uuid::new_v4();
        let chunk_id = Uuid::new_v4();
        let acl = make_acl_payload(
            tenant_a,
            note_id,
            chunk_id,
            owner_id,
            vec![IndexPrincipal::Public],
            IndexVisibility::Public,
            EmbeddingPolicy::Allowed,
            1,
        );
        let doc = make_indexed_doc(
            chunk_id,
            tenant_a,
            note_id,
            owner_id,
            "cross tenant content",
            acl,
        );
        store
            .upsert_chunk(tenant_a, chunk_id, &doc, doc.acl.as_ref().unwrap())
            .await
            .unwrap();

        let results = store
            .search_with_acl(
                &user_principal(tenant_b, other_tenant_user),
                &unit_embedding(),
                10,
            )
            .await
            .unwrap();
        assert!(
            results.is_empty(),
            "cross-tenant user must not retrieve note"
        );

        let _ = store.clear_tenant(tenant_a).await;
    }

    // Scenario 8: embedding-denied note is not retrievable.
    {
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let note_id = Uuid::new_v4();
        let chunk_id = Uuid::new_v4();
        let acl = make_acl_payload(
            tenant_id,
            note_id,
            chunk_id,
            owner_id,
            vec![IndexPrincipal::Owner(owner_id)],
            IndexVisibility::Private,
            EmbeddingPolicy::Denied,
            1,
        );
        let doc = make_indexed_doc(
            chunk_id,
            tenant_id,
            note_id,
            owner_id,
            "denied content",
            acl,
        );
        store
            .upsert_chunk(tenant_id, chunk_id, &doc, doc.acl.as_ref().unwrap())
            .await
            .unwrap();

        let results = store
            .search_with_acl(&owner_principal(tenant_id, owner_id), &unit_embedding(), 10)
            .await
            .unwrap();
        assert!(
            results.is_empty(),
            "embedding-denied note must not be retrieved"
        );

        let _ = store.clear_tenant(tenant_id).await;
    }

    // Scenario 9: missing ACL fails closed (legacy ACL-less chunk).
    // Only backends that can represent a chunk with no ACL payload can run this.
    if supports_acl_less_chunks {
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let note_id = Uuid::new_v4();
        let chunk_id = Uuid::new_v4();
        let acl = make_acl_payload(
            tenant_id,
            note_id,
            chunk_id,
            owner_id,
            vec![IndexPrincipal::Owner(owner_id)],
            IndexVisibility::Private,
            EmbeddingPolicy::Allowed,
            1,
        );
        let mut doc = make_indexed_doc(
            chunk_id,
            tenant_id,
            note_id,
            owner_id,
            "legacy content",
            acl,
        );
        doc.acl = None;
        store
            .upsert_chunk(
                tenant_id,
                chunk_id,
                &doc,
                &make_acl_payload(
                    tenant_id,
                    note_id,
                    chunk_id,
                    owner_id,
                    vec![IndexPrincipal::Owner(owner_id)],
                    IndexVisibility::Private,
                    EmbeddingPolicy::Allowed,
                    1,
                ),
            )
            .await
            .unwrap();

        let results = store
            .search_with_acl(&owner_principal(tenant_id, owner_id), &unit_embedding(), 10)
            .await
            .unwrap();
        assert!(results.is_empty(), "legacy ACL-less chunk must fail closed");

        let _ = store.clear_tenant(tenant_id).await;
    }

    // Scenario 10: malformed ACL fails closed.
    {
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let note_id = Uuid::new_v4();
        let chunk_id = Uuid::new_v4();
        let mut acl = make_acl_payload(
            tenant_id,
            note_id,
            chunk_id,
            owner_id,
            vec![IndexPrincipal::Owner(owner_id)],
            IndexVisibility::Private,
            EmbeddingPolicy::Allowed,
            1,
        );
        acl.visibility = "not-a-visibility".to_string();
        let doc = make_indexed_doc(
            chunk_id,
            tenant_id,
            note_id,
            owner_id,
            "malformed content",
            acl.clone(),
        );
        store
            .upsert_chunk(tenant_id, chunk_id, &doc, &acl)
            .await
            .unwrap();

        let results = store
            .search_with_acl(&owner_principal(tenant_id, owner_id), &unit_embedding(), 10)
            .await
            .unwrap();
        assert!(results.is_empty(), "malformed ACL must fail closed");

        let _ = store.clear_tenant(tenant_id).await;
    }

    // Scenario 11: stale ACL version fails closed.
    {
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let note_id = Uuid::new_v4();
        let chunk_id = Uuid::new_v4();
        let acl = make_acl_payload(
            tenant_id,
            note_id,
            chunk_id,
            owner_id,
            vec![IndexPrincipal::Owner(owner_id)],
            IndexVisibility::Private,
            EmbeddingPolicy::Allowed,
            1,
        );
        let doc = make_indexed_doc(chunk_id, tenant_id, note_id, owner_id, "stale content", acl);
        store
            .upsert_chunk(tenant_id, chunk_id, &doc, doc.acl.as_ref().unwrap())
            .await
            .unwrap();

        let mut principal = owner_principal(tenant_id, owner_id);
        principal.min_acl_versions.insert(note_id, 2);

        let results = store
            .search_with_acl(&principal, &unit_embedding(), 10)
            .await
            .unwrap();
        assert!(results.is_empty(), "stale ACL version must fail closed");

        let _ = store.clear_tenant(tenant_id).await;
    }

    // Scenario 12: share revocation removes access without rebuilding the index.
    {
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let shared_user_id = Uuid::new_v4();
        let note_id = Uuid::new_v4();
        let chunk_id = Uuid::new_v4();
        let acl_v1 = make_acl_payload(
            tenant_id,
            note_id,
            chunk_id,
            owner_id,
            vec![
                IndexPrincipal::Owner(owner_id),
                IndexPrincipal::User(shared_user_id),
            ],
            IndexVisibility::Private,
            EmbeddingPolicy::Allowed,
            1,
        );
        let doc = make_indexed_doc(
            chunk_id,
            tenant_id,
            note_id,
            owner_id,
            "revocable content",
            acl_v1.clone(),
        );
        store
            .upsert_chunk(tenant_id, chunk_id, &doc, &acl_v1)
            .await
            .unwrap();

        let results = store
            .search_with_acl(
                &user_principal(tenant_id, shared_user_id),
                &unit_embedding(),
                10,
            )
            .await
            .unwrap();
        assert_eq!(
            results.len(),
            1,
            "shared user must retrieve note before revocation"
        );

        // Revoke via ACL update.
        let acl_v2 = make_acl_payload(
            tenant_id,
            note_id,
            chunk_id,
            owner_id,
            vec![IndexPrincipal::Owner(owner_id)],
            IndexVisibility::Private,
            EmbeddingPolicy::Allowed,
            2,
        );
        let updated = store
            .update_note_acl(tenant_id, note_id, &acl_v2)
            .await
            .unwrap();
        assert!(updated > 0, "ACL update must modify at least one chunk");

        let results = store
            .search_with_acl(
                &user_principal(tenant_id, shared_user_id),
                &unit_embedding(),
                10,
            )
            .await
            .unwrap();
        assert!(
            results.is_empty(),
            "shared user must not retrieve note after revocation"
        );

        let _ = store.clear_tenant(tenant_id).await;
    }

    // Scenario 13: inaccessible workspace rows do not consume the result limit.
    {
        let tenant_id = Uuid::new_v4();
        let caller_id = Uuid::new_v4();
        let workspace_owner_id = Uuid::new_v4();
        let shared_owner_id = Uuid::new_v4();

        let workspace_chunk_id = Uuid::new_v4();
        let workspace_acl = make_acl_payload(
            tenant_id,
            Uuid::new_v4(),
            workspace_chunk_id,
            workspace_owner_id,
            vec![],
            IndexVisibility::Workspace,
            EmbeddingPolicy::Allowed,
            1,
        );
        let workspace_doc = make_indexed_doc(
            workspace_chunk_id,
            tenant_id,
            workspace_acl.note_id,
            workspace_owner_id,
            "closer inaccessible workspace content",
            workspace_acl.clone(),
        );
        store
            .upsert_chunk(
                tenant_id,
                workspace_chunk_id,
                &workspace_doc,
                &workspace_acl,
            )
            .await
            .unwrap();

        let shared_chunk_id = Uuid::new_v4();
        let shared_acl = make_acl_payload(
            tenant_id,
            Uuid::new_v4(),
            shared_chunk_id,
            shared_owner_id,
            vec![IndexPrincipal::User(caller_id)],
            IndexVisibility::Private,
            EmbeddingPolicy::Allowed,
            1,
        );
        let mut shared_doc = make_indexed_doc(
            shared_chunk_id,
            tenant_id,
            shared_acl.note_id,
            shared_owner_id,
            "lower-ranked directly shared content",
            shared_acl.clone(),
        );
        shared_doc.embedding[EMBEDDING_DIM / 2..].fill(0.0);
        store
            .upsert_chunk(tenant_id, shared_chunk_id, &shared_doc, &shared_acl)
            .await
            .unwrap();

        let results = store
            .search_with_acl(&user_principal(tenant_id, caller_id), &unit_embedding(), 1)
            .await
            .unwrap();
        assert_eq!(
            results.first().map(|(doc, _)| doc.chunk_id),
            Some(shared_chunk_id),
            "inaccessible workspace rows must not hide allowed lower-ranked rows"
        );

        let _ = store.clear_tenant(tenant_id).await;
    }

    // --- Keyword search scenarios (keyword_search_with_acl) ---

    // Scenario 14: owner can find their own private note by keyword.
    {
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let note_id = Uuid::new_v4();
        let chunk_id = Uuid::new_v4();
        let acl = make_acl_payload(
            tenant_id,
            note_id,
            chunk_id,
            owner_id,
            vec![IndexPrincipal::Owner(owner_id)],
            IndexVisibility::Private,
            EmbeddingPolicy::Allowed,
            1,
        );
        let doc = make_indexed_doc(
            chunk_id,
            tenant_id,
            note_id,
            owner_id,
            "the quarterly budget plan",
            acl,
        );
        store
            .upsert_chunk(tenant_id, chunk_id, &doc, doc.acl.as_ref().unwrap())
            .await
            .unwrap();

        let results = store
            .keyword_search_with_acl(&owner_principal(tenant_id, owner_id), "budget", 10)
            .await
            .unwrap();
        assert_eq!(
            results.len(),
            1,
            "owner must find own private note by keyword"
        );
        assert!(
            results[0].1 > 0.0,
            "keyword result must carry a positive score"
        );

        let _ = store.clear_tenant(tenant_id).await;
    }

    // Scenario 15: unrelated user in the same tenant cannot find a private note by keyword.
    {
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let stranger_id = Uuid::new_v4();
        let note_id = Uuid::new_v4();
        let chunk_id = Uuid::new_v4();
        let acl = make_acl_payload(
            tenant_id,
            note_id,
            chunk_id,
            owner_id,
            vec![IndexPrincipal::Owner(owner_id)],
            IndexVisibility::Private,
            EmbeddingPolicy::Allowed,
            1,
        );
        let doc = make_indexed_doc(
            chunk_id,
            tenant_id,
            note_id,
            owner_id,
            "private budget details",
            acl,
        );
        store
            .upsert_chunk(tenant_id, chunk_id, &doc, doc.acl.as_ref().unwrap())
            .await
            .unwrap();

        let results = store
            .keyword_search_with_acl(&user_principal(tenant_id, stranger_id), "budget", 10)
            .await
            .unwrap();
        assert!(
            results.is_empty(),
            "stranger must not find private note by keyword"
        );

        let _ = store.clear_tenant(tenant_id).await;
    }

    // Scenario 16: stale ACL version fails closed for keyword search.
    {
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let note_id = Uuid::new_v4();
        let chunk_id = Uuid::new_v4();
        let acl = make_acl_payload(
            tenant_id,
            note_id,
            chunk_id,
            owner_id,
            vec![IndexPrincipal::Owner(owner_id)],
            IndexVisibility::Private,
            EmbeddingPolicy::Allowed,
            1,
        );
        let doc = make_indexed_doc(
            chunk_id,
            tenant_id,
            note_id,
            owner_id,
            "stale budget content",
            acl,
        );
        store
            .upsert_chunk(tenant_id, chunk_id, &doc, doc.acl.as_ref().unwrap())
            .await
            .unwrap();

        let mut principal = owner_principal(tenant_id, owner_id);
        principal.min_acl_versions.insert(note_id, 2);

        let results = store
            .keyword_search_with_acl(&principal, "budget", 10)
            .await
            .unwrap();
        assert!(
            results.is_empty(),
            "stale ACL version must fail closed for keyword search"
        );

        let _ = store.clear_tenant(tenant_id).await;
    }

    // Scenario 17: cross-tenant keyword search fails closed.
    {
        let tenant_a = Uuid::new_v4();
        let tenant_b = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let other_tenant_user = Uuid::new_v4();
        let note_id = Uuid::new_v4();
        let chunk_id = Uuid::new_v4();
        let acl = make_acl_payload(
            tenant_a,
            note_id,
            chunk_id,
            owner_id,
            vec![IndexPrincipal::Public],
            IndexVisibility::Public,
            EmbeddingPolicy::Allowed,
            1,
        );
        let doc = make_indexed_doc(
            chunk_id,
            tenant_a,
            note_id,
            owner_id,
            "cross tenant budget content",
            acl,
        );
        store
            .upsert_chunk(tenant_a, chunk_id, &doc, doc.acl.as_ref().unwrap())
            .await
            .unwrap();

        let results = store
            .keyword_search_with_acl(&user_principal(tenant_b, other_tenant_user), "budget", 10)
            .await
            .unwrap();
        assert!(
            results.is_empty(),
            "cross-tenant user must not find note by keyword"
        );

        let _ = store.clear_tenant(tenant_a).await;
    }
}

#[tokio::test]
async fn test_in_memory_vector_store_permission_contract() {
    let store = InMemoryVectorStore::new();
    run_permission_contract(&store, true).await;
}

#[tokio::test]
#[ignore = "Requires PostgreSQL with pgvector"]
async fn test_pgvector_store_permission_contract() {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://rustshare:changeme@localhost:5432/rustshare".to_string());
    let pool = sqlx::PgPool::connect(&database_url)
        .await
        .expect("failed to connect to test database");
    let store = PgVectorStore::new(pool);
    run_permission_contract(&store, false).await;
}
