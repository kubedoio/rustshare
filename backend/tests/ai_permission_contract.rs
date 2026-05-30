//! AI Permission Contract Tests (A-01 through A-07)
//!
//! Verifies that AI search and operations respect all permission boundaries
//! and cannot be used as an alternate access path around normal effective permissions.

use std::collections::HashMap;
use std::sync::Mutex;
use uuid::Uuid;

use rustshare_core::domain::{File, Share, SharePermissions};
use rustshare_core::services::{AiService, ContentIndexer, EmbeddingGenerator, PermissionResolver, PermissionResolverOps};

// Mock embedding generator
struct MockEmbeddingGenerator;

impl EmbeddingGenerator for MockEmbeddingGenerator {
    async fn generate(&self, _text: &str) -> Vec<f32> {
        vec![0.0f32; 768]
    }
}

// Mock permission ops
struct MockPermissionOps {
    files: Mutex<HashMap<Uuid, File>>,
    shares: Mutex<Vec<Share>>,
}

impl MockPermissionOps {
    fn new() -> Self {
        Self {
            files: Mutex::new(HashMap::new()),
            shares: Mutex::new(Vec::new()),
        }
    }

    fn add_file(&self, file: File) {
        self.files.lock().unwrap().insert(file.id, file);
    }

    fn add_share(&self, share: Share) {
        self.shares.lock().unwrap().push(share);
    }
}

impl PermissionResolverOps for MockPermissionOps {
    async fn find_user_share(
        &self,
        file_id: Option<Uuid>,
        folder_id: Option<Uuid>,
        recipient_user_id: Uuid,
    ) -> anyhow::Result<Option<Share>> {
        let shares = self.shares.lock().unwrap();
        Ok(shares
            .iter()
            .find(|s| {
                s.file_id == file_id
                    && s.folder_id == folder_id
                    && s.recipient_user_id == Some(recipient_user_id)
            })
            .cloned())
    }

    async fn find_group_shares(
        &self,
        _file_id: Option<Uuid>,
        _folder_id: Option<Uuid>,
        _group_ids: &[Uuid],
    ) -> anyhow::Result<Vec<Share>> {
        Ok(Vec::new())
    }

    async fn find_user_shares_for_folders(
        &self,
        _folder_ids: &[Uuid],
        _recipient_user_id: Uuid,
    ) -> anyhow::Result<Vec<Share>> {
        Ok(Vec::new())
    }

    async fn find_group_shares_for_folders(
        &self,
        _folder_ids: &[Uuid],
        _group_ids: &[Uuid],
    ) -> anyhow::Result<Vec<Share>> {
        Ok(Vec::new())
    }

    async fn find_file_by_id(&self, id: Uuid) -> anyhow::Result<Option<File>> {
        Ok(self.files.lock().unwrap().get(&id).cloned())
    }

    async fn find_folder_by_id(&self, _id: Uuid) -> anyhow::Result<Option<rustshare_core::domain::Folder>> {
        Ok(None)
    }

    async fn get_user_group_ids(&self, _user_id: Uuid) -> anyhow::Result<Vec<Uuid>> {
        Ok(Vec::new())
    }
}

fn make_file(id: Uuid, name: &str, owner_id: Uuid, tenant_id: Uuid) -> File {
    File {
        id,
        name: name.to_string(),
        path: format!("/{}", name),
        content_hash: "hash".to_string(),
        size: 100,
        mime_type: "text/plain".to_string(),
        parent_folder_id: None,
        owner_id,
        current_version: 1,
        created_at: chrono::Utc::now(),
        modified_at: chrono::Utc::now(),
        starred_at: None,
        deleted_at: None,
        tenant_id,
    }
}

fn make_share(
    file_id: Uuid,
    recipient_user_id: Uuid,
    permissions: SharePermissions,
    revoked_at: Option<chrono::DateTime<chrono::Utc>>,
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
) -> Share {
    Share {
        id: Uuid::new_v4(),
        file_id: Some(file_id),
        folder_id: None,
        share_token: Some(Uuid::new_v4().to_string()),
        permissions,
        password_hash: None,
        expires_at,
        upload_only: false,
        access_count: 0,
        recipient_user_id: Some(recipient_user_id),
        recipient_group_id: None,
        created_by: Uuid::new_v4(),
        created_at: chrono::Utc::now(),
        revoked_at,
        tenant_id: Uuid::new_v4(),
    }
}

fn build_service(
    ops: std::sync::Arc<MockPermissionOps>,
) -> AiService<MockEmbeddingGenerator, MockPermissionOps> {
    let generator = std::sync::Arc::new(MockEmbeddingGenerator);
    let indexer = std::sync::Arc::new(ContentIndexer::new(generator));
    let permission_resolver = std::sync::Arc::new(PermissionResolver::new(ops));
    AiService::new(indexer, permission_resolver)
}

/// A-05: AI excludes revoked shares
#[tokio::test]
async fn test_ai_excludes_revoked_shares() {
    let ops = std::sync::Arc::new(MockPermissionOps::new());
    let service = build_service(ops.clone());

    let owner_id = Uuid::new_v4();
    let recipient_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();
    let file_id = Uuid::new_v4();

    ops.add_file(make_file(file_id, "shared.txt", owner_id, tenant_id));
    ops.add_share(make_share(
        file_id,
        recipient_id,
        SharePermissions::View,
        Some(chrono::Utc::now()),
        None,
    ));

    // Index the file so it appears in search
    service
        .index_file(
            file_id,
            "shared.txt".to_string(),
            "/shared.txt".to_string(),
            "shared content".to_string(),
            "text/plain".to_string(),
            owner_id,
            tenant_id,
        )
        .await
        .unwrap();

    let results = service
        .semantic_search("shared", recipient_id, tenant_id, 10)
        .await
        .unwrap();
    assert!(
        results.is_empty(),
        "AI should exclude revoked shares"
    );
}

/// A-06: AI excludes expired shares
#[tokio::test]
async fn test_ai_excludes_expired_shares() {
    let ops = std::sync::Arc::new(MockPermissionOps::new());
    let service = build_service(ops.clone());

    let owner_id = Uuid::new_v4();
    let recipient_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();
    let file_id = Uuid::new_v4();

    ops.add_file(make_file(file_id, "shared.txt", owner_id, tenant_id));
    ops.add_share(make_share(
        file_id,
        recipient_id,
        SharePermissions::View,
        None,
        Some(chrono::Utc::now() - chrono::Duration::hours(1)),
    ));

    // Index the file so it appears in search
    service
        .index_file(
            file_id,
            "shared.txt".to_string(),
            "/shared.txt".to_string(),
            "shared content".to_string(),
            "text/plain".to_string(),
            owner_id,
            tenant_id,
        )
        .await
        .unwrap();

    let results = service
        .semantic_search("shared", recipient_id, tenant_id, 10)
        .await
        .unwrap();
    assert!(
        results.is_empty(),
        "AI should exclude expired shares"
    );
}

/// A-07: AI excludes hidden metadata and module sidecars
#[tokio::test]
async fn test_ai_excludes_hidden_metadata() {
    let ops = std::sync::Arc::new(MockPermissionOps::new());
    let service = build_service(ops.clone());

    let user_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    for name in &[
        ".rustshare_hidden",
        "events.jsonl",
        "index.md",
        "__primary__.md",
        "config.editor.json",
    ] {
        let file_id = Uuid::new_v4();
        ops.add_file(make_file(file_id, name, user_id, tenant_id));
        service
            .index_file(
                file_id,
                name.to_string(),
                format!("/{}", name),
                "hidden content".to_string(),
                "text/plain".to_string(),
                user_id,
                tenant_id,
            )
            .await
            .unwrap();
    }

    let results = service
        .semantic_search("hidden", user_id, tenant_id, 10)
        .await
        .unwrap();
    assert!(
        results.is_empty(),
        "AI should exclude hidden metadata files"
    );
}
