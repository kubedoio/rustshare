//! Search Authorization Contract Tests (Q-01 through Q-06)
//!
//! Verifies that search respects all permission boundaries and cannot be used
//! as an alternate access path around normal effective permissions.

use std::collections::HashMap;
use std::sync::Mutex;
use uuid::Uuid;

use rustshare_core::domain::{File, Folder, Share, SharePermissions};
use rustshare_core::services::permission_resolver::{PermissionResolver, PermissionResolverOps, Resource};
use rustshare_core::services::search_service::{SearchIndexRepository, SearchResult, SearchResultItem, SearchService};

// Mock search repository
struct MockSearchRepo {
    results: Mutex<HashMap<Uuid, Vec<SearchResult>>>,
}

impl MockSearchRepo {
    fn new() -> Self {
        Self {
            results: Mutex::new(HashMap::new()),
        }
    }

    fn add_result(&self, tenant_id: Uuid, result: SearchResult) {
        self.results.lock().unwrap().entry(tenant_id).or_default().push(result);
    }
}

impl SearchIndexRepository for MockSearchRepo {
    async fn search(
        &self,
        tenant_id: Uuid,
        _query: &str,
        _limit: usize,
    ) -> anyhow::Result<Vec<SearchResult>> {
        let results = self.results.lock().unwrap();
        Ok(results.get(&tenant_id).cloned().unwrap_or_default())
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

    async fn find_folder_by_id(&self, id: Uuid) -> anyhow::Result<Option<Folder>> {
        Ok(None)
    }

    async fn get_user_group_ids(&self, _user_id: Uuid) -> anyhow::Result<Vec<Uuid>> {
        Ok(Vec::new())
    }
}

fn search_result(id: Uuid, name: &str, owner_id: Uuid) -> SearchResult {
    SearchResult {
        id,
        resource_type: "file".to_string(),
        name: name.to_string(),
        path: format!("/{}", name),
        owner_id,
        updated_at: chrono::Utc::now(),
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
    search_repo: std::sync::Arc<MockSearchRepo>,
    ops: std::sync::Arc<MockPermissionOps>,
) -> SearchService<MockSearchRepo, MockPermissionOps> {
    let permission_resolver = std::sync::Arc::new(PermissionResolver::new(ops));
    SearchService::new(search_repo, permission_resolver)
}

/// Q-01: Search returns files user has access to (own files)
#[tokio::test]
async fn test_search_returns_own_files() {
    let repo = std::sync::Arc::new(MockSearchRepo::new());
    let ops = std::sync::Arc::new(MockPermissionOps::new());
    let service = build_service(repo.clone(), ops.clone());

    let user_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();
    let file_id = Uuid::new_v4();

    ops.add_file(make_file(file_id, "document.txt", user_id, tenant_id));
    repo.add_result(tenant_id, search_result(file_id, "document.txt", user_id));

    let results = service.search(user_id, tenant_id, "document", 10).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "document.txt");
}

/// Q-02: Search excludes other tenants
#[tokio::test]
async fn test_search_excludes_other_tenants() {
    let repo = std::sync::Arc::new(MockSearchRepo::new());
    let ops = std::sync::Arc::new(MockPermissionOps::new());
    let service = build_service(repo.clone(), ops.clone());

    let user_id = Uuid::new_v4();
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    let file_a = Uuid::new_v4();
    let file_b = Uuid::new_v4();

    ops.add_file(make_file(file_a, "tenant_a_doc.txt", user_id, tenant_a));
    ops.add_file(make_file(file_b, "tenant_b_doc.txt", user_id, tenant_b));
    repo.add_result(tenant_a, search_result(file_a, "tenant_a_doc.txt", user_id));
    repo.add_result(tenant_b, search_result(file_b, "tenant_b_doc.txt", user_id));

    let results = service.search(user_id, tenant_a, "tenant", 10).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "tenant_a_doc.txt");
}

/// Q-03: Search excludes unauthorized shared content
#[tokio::test]
async fn test_search_excludes_unauthorized_content() {
    let repo = std::sync::Arc::new(MockSearchRepo::new());
    let ops = std::sync::Arc::new(MockPermissionOps::new());
    let service = build_service(repo.clone(), ops.clone());

    let owner_id = Uuid::new_v4();
    let other_user = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();
    let file_id = Uuid::new_v4();

    ops.add_file(make_file(file_id, "private.txt", owner_id, tenant_id));
    repo.add_result(tenant_id, search_result(file_id, "private.txt", owner_id));

    let results = service.search(other_user, tenant_id, "private", 10).await.unwrap();
    assert!(results.is_empty());
}

/// Q-04: Search excludes deleted files
#[tokio::test]
async fn test_search_excludes_deleted_files() {
    let repo = std::sync::Arc::new(MockSearchRepo::new());
    let ops = std::sync::Arc::new(MockPermissionOps::new());
    let service = build_service(repo.clone(), ops.clone());

    let user_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();
    let file_id = Uuid::new_v4();

    // File in index but not in permission ops (deleted)
    repo.add_result(tenant_id, search_result(file_id, "deleted.txt", user_id));

    let results = service.search(user_id, tenant_id, "deleted", 10).await.unwrap();
    assert!(results.is_empty());
}

/// Q-05: Search excludes revoked shares
#[tokio::test]
async fn test_search_excludes_revoked_shares() {
    let repo = std::sync::Arc::new(MockSearchRepo::new());
    let ops = std::sync::Arc::new(MockPermissionOps::new());
    let service = build_service(repo.clone(), ops.clone());

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
    repo.add_result(tenant_id, search_result(file_id, "shared.txt", owner_id));

    let results = service.search(recipient_id, tenant_id, "shared", 10).await.unwrap();
    assert!(results.is_empty());
}

/// Q-06: Search excludes expired shares
#[tokio::test]
async fn test_search_excludes_expired_shares() {
    let repo = std::sync::Arc::new(MockSearchRepo::new());
    let ops = std::sync::Arc::new(MockPermissionOps::new());
    let service = build_service(repo.clone(), ops.clone());

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
    repo.add_result(tenant_id, search_result(file_id, "shared.txt", owner_id));

    let results = service.search(recipient_id, tenant_id, "shared", 10).await.unwrap();
    assert!(results.is_empty());
}

/// Q-07: Search excludes hidden metadata and module sidecars
#[tokio::test]
async fn test_search_excludes_hidden_metadata() {
    let repo = std::sync::Arc::new(MockSearchRepo::new());
    let ops = std::sync::Arc::new(MockPermissionOps::new());
    let service = build_service(repo.clone(), ops.clone());

    let user_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    for name in &[".rustshare_hidden", "events.jsonl", "index.md", "__primary__.md", "config.editor.json"] {
        let file_id = Uuid::new_v4();
        ops.add_file(make_file(file_id, name, user_id, tenant_id));
        repo.add_result(tenant_id, search_result(file_id, name, user_id));
    }

    let results = service.search(user_id, tenant_id, "rustshare", 10).await.unwrap();
    assert!(results.is_empty());
}
