//! SearchService for name/path search operations.
//!
//! This service handles search queries, including:
//! - Query parsing and tokenization
//! - Calling the search index repository
//! - Permission filtering of results
//! - Result ranking and formatting

use anyhow::Result;
use std::sync::Arc;
use uuid::Uuid;

use crate::domain::{SharePermissions, UserId};
use crate::services::permission_resolver::{PermissionResolver, PermissionResolverOps, Resource};

/// Search result from the index (mirrors storage::schemas::SearchResult)
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// Resource ID
    pub id: Uuid,
    /// Resource type: "file" or "folder"
    pub resource_type: String,
    /// Resource name
    pub name: String,
    /// Full path
    pub path: String,
    /// Owner ID
    pub owner_id: Uuid,
    /// Last modified
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Trait for search index operations needed by SearchService.
///
/// This trait abstracts the search index to allow for testing without storage dependencies.
#[allow(async_fn_in_trait)]
pub trait SearchIndexRepository: Send + Sync {
    /// Search for resources matching the query
    async fn search(&self, tenant_id: Uuid, query: &str, limit: usize)
        -> Result<Vec<SearchResult>>;
}

/// Search result item with permission information
#[derive(Debug, Clone)]
pub struct SearchResultItem {
    /// Resource ID
    pub id: Uuid,
    /// Resource type: "file" or "folder"
    pub resource_type: String,
    /// Resource name
    pub name: String,
    /// Full path
    pub path: String,
    /// Owner ID
    pub owner_id: Uuid,
    /// Whether the requesting user has view permission
    pub can_view: bool,
    /// The permission level the user has
    pub permission: Option<SharePermissions>,
}

/// Search service for file/folder search operations
pub struct SearchService<SR, PR>
where
    SR: SearchIndexRepository,
    PR: PermissionResolverOps,
{
    search_repo: Arc<SR>,
    permission_resolver: Arc<PermissionResolver<PR>>,
}

impl<SR, PR> SearchService<SR, PR>
where
    SR: SearchIndexRepository,
    PR: PermissionResolverOps,
{
    /// Create a new SearchService instance
    pub fn new(search_repo: Arc<SR>, permission_resolver: Arc<PermissionResolver<PR>>) -> Self {
        Self {
            search_repo,
            permission_resolver,
        }
    }

    /// Search for files and folders by name/path
    ///
    /// # Arguments
    /// * `user_id` - The ID of the user performing the search
    /// * `tenant_id` - The tenant ID for scoping the search
    /// * `query` - The search query string
    /// * `limit` - Maximum number of results to return
    ///
    /// # Returns
    /// A list of search results filtered by the user's permissions.
    /// Only resources the user has view permission on are returned.
    pub async fn search(
        &self,
        user_id: UserId,
        tenant_id: Uuid,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchResultItem>> {
        // Get raw search results from the index
        let raw_results = self
            .search_repo
            .search(tenant_id, query, limit * 2) // Request more to account for filtering
            .await
            .map_err(|e| anyhow::anyhow!("Search failed: {}", e))?;

        // Filter out hidden metadata files
        let raw_results: Vec<SearchResult> = raw_results
            .into_iter()
            .filter(|r| {
                !r.name.starts_with(".rustshare")
                    && r.name != "events.jsonl"
                    && r.name != "index.md"
                    && r.name != "__primary__.md"
                    && !r.name.ends_with(".editor.json")
            })
            .collect();

        // Filter results by permission
        let mut results = Vec::new();
        for result in raw_results {
            // Determine resource type and check permission
            let resource = if result.resource_type == "file" {
                Resource::File(result.id)
            } else {
                Resource::Folder(result.id)
            };

            // Resolve permission for this user on this resource
            let permission = match self
                .permission_resolver
                .resolve_permission(user_id, resource)
                .await
            {
                Ok(perm) => perm,
                Err(e) => {
                    tracing::warn!(
                        "Permission resolution failed for resource {}: {}. Skipping.",
                        result.id,
                        e
                    );
                    continue;
                }
            };

            // Only include if user has view permission
            if permission.is_some() {
                results.push(SearchResultItem {
                    id: result.id,
                    resource_type: result.resource_type,
                    name: result.name,
                    path: result.path,
                    owner_id: result.owner_id,
                    can_view: true,
                    permission,
                });

                // Stop once we have enough results
                if results.len() >= limit {
                    break;
                }
            }
        }

        Ok(results)
    }

    /// Search across all resources (admin only)
    ///
    /// This variant doesn't filter by permission and returns all matching results.
    /// Should only be used for admin operations.
    pub async fn search_unrestricted(
        &self,
        tenant_id: Uuid,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchResultItem>> {
        let raw_results = self
            .search_repo
            .search(tenant_id, query, limit)
            .await
            .map_err(|e| anyhow::anyhow!("Search failed: {}", e))?;

        // Filter out hidden metadata files
        let raw_results: Vec<SearchResult> = raw_results
            .into_iter()
            .filter(|r| {
                !r.name.starts_with(".rustshare")
                    && r.name != "events.jsonl"
                    && r.name != "index.md"
                    && r.name != "__primary__.md"
                    && !r.name.ends_with(".editor.json")
            })
            .collect();

        Ok(raw_results
            .into_iter()
            .map(|result| SearchResultItem {
                id: result.id,
                resource_type: result.resource_type,
                name: result.name,
                path: result.path,
                owner_id: result.owner_id,
                can_view: true,
                permission: Some(SharePermissions::Admin),
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{File, Folder, Share, SharePermissions};
    use chrono::{Duration, Utc};
    use std::collections::HashMap;
    use std::sync::Mutex;

    // Mock SearchIndexRepository
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
            self.results
                .lock()
                .unwrap()
                .entry(tenant_id)
                .or_default()
                .push(result);
        }
    }

    impl SearchIndexRepository for MockSearchRepo {
        async fn search(
            &self,
            tenant_id: Uuid,
            _query: &str,
            _limit: usize,
        ) -> Result<Vec<SearchResult>> {
            let results = self.results.lock().unwrap();
            Ok(results.get(&tenant_id).cloned().unwrap_or_default())
        }
    }

    // Configurable Mock PermissionResolverOps
    struct MockPermissionOps {
        files: Mutex<HashMap<Uuid, File>>,
        folders: Mutex<HashMap<Uuid, Folder>>,
        shares: Mutex<Vec<Share>>,
    }

    impl MockPermissionOps {
        fn new() -> Self {
            Self {
                files: Mutex::new(HashMap::new()),
                folders: Mutex::new(HashMap::new()),
                shares: Mutex::new(Vec::new()),
            }
        }

        fn add_file(&self, file: File) {
            self.files.lock().unwrap().insert(file.id, file);
        }

        #[allow(dead_code)]
        fn add_folder(&self, folder: Folder) {
            self.folders.lock().unwrap().insert(folder.id, folder);
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
            recipient_user_id: UserId,
        ) -> Result<Option<Share>> {
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
            file_id: Option<Uuid>,
            folder_id: Option<Uuid>,
            group_ids: &[Uuid],
        ) -> Result<Vec<Share>> {
            let shares = self.shares.lock().unwrap();
            Ok(shares
                .iter()
                .filter(|s| {
                    s.file_id == file_id
                        && s.folder_id == folder_id
                        && s.recipient_group_id
                            .map(|gid| group_ids.contains(&gid))
                            .unwrap_or(false)
                })
                .cloned()
                .collect())
        }

        async fn find_user_shares_for_folders(
            &self,
            folder_ids: &[Uuid],
            recipient_user_id: UserId,
        ) -> Result<Vec<Share>> {
            let shares = self.shares.lock().unwrap();
            Ok(shares
                .iter()
                .filter(|s| {
                    s.file_id.is_none()
                        && s.folder_id
                            .map(|fid| folder_ids.contains(&fid))
                            .unwrap_or(false)
                        && s.recipient_user_id == Some(recipient_user_id)
                })
                .cloned()
                .collect())
        }

        async fn find_group_shares_for_folders(
            &self,
            folder_ids: &[Uuid],
            group_ids: &[Uuid],
        ) -> Result<Vec<Share>> {
            let shares = self.shares.lock().unwrap();
            Ok(shares
                .iter()
                .filter(|s| {
                    s.file_id.is_none()
                        && s.folder_id
                            .map(|fid| folder_ids.contains(&fid))
                            .unwrap_or(false)
                        && s.recipient_group_id
                            .map(|gid| group_ids.contains(&gid))
                            .unwrap_or(false)
                })
                .cloned()
                .collect())
        }

        async fn find_file_by_id(&self, id: Uuid) -> Result<Option<File>> {
            Ok(self.files.lock().unwrap().get(&id).cloned())
        }

        async fn find_folder_by_id(&self, id: Uuid) -> Result<Option<Folder>> {
            Ok(self.folders.lock().unwrap().get(&id).cloned())
        }

        async fn get_user_group_ids(&self, _user_id: UserId) -> Result<Vec<Uuid>> {
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
            updated_at: Utc::now(),
        }
    }

    fn make_file(id: Uuid, name: &str, owner_id: UserId, tenant_id: Uuid) -> File {
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
            created_at: Utc::now(),
            modified_at: Utc::now(),
            starred_at: None,
            deleted_at: None,
            tenant_id,
        }
    }

    fn make_share(
        file_id: Uuid,
        recipient_user_id: Uuid,
        permissions: SharePermissions,
        revoked_at: Option<chrono::DateTime<Utc>>,
        expires_at: Option<chrono::DateTime<Utc>>,
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
            created_at: Utc::now(),
            revoked_at,
            tenant_id: Uuid::new_v4(),
        }
    }

    #[tokio::test]
    async fn test_search_returns_own_files() {
        let search_repo = Arc::new(MockSearchRepo::new());
        let permission_ops = Arc::new(MockPermissionOps::new());
        let permission_resolver = Arc::new(PermissionResolver::new(permission_ops.clone()));
        let service = SearchService::new(search_repo.clone(), permission_resolver);

        let user_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();

        let file = make_file(file_id, "document.txt", user_id, tenant_id);
        permission_ops.add_file(file);
        search_repo.add_result(tenant_id, search_result(file_id, "document.txt", user_id));

        let results = service
            .search(user_id, tenant_id, "document", 10)
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "document.txt");
    }

    #[tokio::test]
    async fn test_search_excludes_other_tenants() {
        let search_repo = Arc::new(MockSearchRepo::new());
        let permission_ops = Arc::new(MockPermissionOps::new());
        let permission_resolver = Arc::new(PermissionResolver::new(permission_ops.clone()));
        let service = SearchService::new(search_repo.clone(), permission_resolver);

        let user_id = Uuid::new_v4();
        let tenant_a = Uuid::new_v4();
        let tenant_b = Uuid::new_v4();
        let file_a = Uuid::new_v4();
        let file_b = Uuid::new_v4();

        permission_ops.add_file(make_file(file_a, "tenant_a_doc.txt", user_id, tenant_a));
        permission_ops.add_file(make_file(file_b, "tenant_b_doc.txt", user_id, tenant_b));

        search_repo.add_result(tenant_a, search_result(file_a, "tenant_a_doc.txt", user_id));
        search_repo.add_result(tenant_b, search_result(file_b, "tenant_b_doc.txt", user_id));

        let results = service
            .search(user_id, tenant_a, "tenant", 10)
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "tenant_a_doc.txt");
    }

    #[tokio::test]
    async fn test_search_excludes_unauthorized_shared_content() {
        let search_repo = Arc::new(MockSearchRepo::new());
        let permission_ops = Arc::new(MockPermissionOps::new());
        let permission_resolver = Arc::new(PermissionResolver::new(permission_ops.clone()));
        let service = SearchService::new(search_repo.clone(), permission_resolver);

        let owner_id = Uuid::new_v4();
        let other_user = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();

        permission_ops.add_file(make_file(file_id, "private.txt", owner_id, tenant_id));
        search_repo.add_result(tenant_id, search_result(file_id, "private.txt", owner_id));

        let results = service
            .search(other_user, tenant_id, "private", 10)
            .await
            .unwrap();
        assert!(results.is_empty(), "Unauthorized files should be excluded");
    }

    #[tokio::test]
    async fn test_search_excludes_deleted_files() {
        let search_repo = Arc::new(MockSearchRepo::new());
        let permission_ops = Arc::new(MockPermissionOps::new());
        let permission_resolver = Arc::new(PermissionResolver::new(permission_ops.clone()));
        let service = SearchService::new(search_repo.clone(), permission_resolver);

        let user_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();

        // File is in the index but NOT in the permission ops (simulating deletion)
        search_repo.add_result(tenant_id, search_result(file_id, "deleted.txt", user_id));

        let results = service
            .search(user_id, tenant_id, "deleted", 10)
            .await
            .unwrap();
        assert!(
            results.is_empty(),
            "Deleted files should be excluded from search"
        );
    }

    #[tokio::test]
    async fn test_search_excludes_revoked_shares() {
        let search_repo = Arc::new(MockSearchRepo::new());
        let permission_ops = Arc::new(MockPermissionOps::new());
        let permission_resolver = Arc::new(PermissionResolver::new(permission_ops.clone()));
        let service = SearchService::new(search_repo.clone(), permission_resolver);

        let owner_id = Uuid::new_v4();
        let recipient_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();

        permission_ops.add_file(make_file(file_id, "shared.txt", owner_id, tenant_id));
        permission_ops.add_share(make_share(
            file_id,
            recipient_id,
            SharePermissions::View,
            Some(Utc::now()), // revoked
            None,
        ));
        search_repo.add_result(tenant_id, search_result(file_id, "shared.txt", owner_id));

        let results = service
            .search(recipient_id, tenant_id, "shared", 10)
            .await
            .unwrap();
        assert!(
            results.is_empty(),
            "Revoked shares should be excluded from search"
        );
    }

    #[tokio::test]
    async fn test_search_excludes_expired_shares() {
        let search_repo = Arc::new(MockSearchRepo::new());
        let permission_ops = Arc::new(MockPermissionOps::new());
        let permission_resolver = Arc::new(PermissionResolver::new(permission_ops.clone()));
        let service = SearchService::new(search_repo.clone(), permission_resolver);

        let owner_id = Uuid::new_v4();
        let recipient_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();

        permission_ops.add_file(make_file(file_id, "shared.txt", owner_id, tenant_id));
        permission_ops.add_share(make_share(
            file_id,
            recipient_id,
            SharePermissions::View,
            None,
            Some(Utc::now() - Duration::hours(1)), // expired
        ));
        search_repo.add_result(tenant_id, search_result(file_id, "shared.txt", owner_id));

        let results = service
            .search(recipient_id, tenant_id, "shared", 10)
            .await
            .unwrap();
        assert!(
            results.is_empty(),
            "Expired shares should be excluded from search"
        );
    }

    #[tokio::test]
    async fn test_search_excludes_hidden_metadata_and_sidecars() {
        let search_repo = Arc::new(MockSearchRepo::new());
        let permission_ops = Arc::new(MockPermissionOps::new());
        let permission_resolver = Arc::new(PermissionResolver::new(permission_ops.clone()));
        let service = SearchService::new(search_repo.clone(), permission_resolver);

        let user_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        let hidden_files = vec![
            ".rustshare_hidden",
            "events.jsonl",
            "index.md",
            "__primary__.md",
            "config.editor.json",
        ];

        for name in &hidden_files {
            let file_id = Uuid::new_v4();
            permission_ops.add_file(make_file(file_id, name, user_id, tenant_id));
            search_repo.add_result(tenant_id, search_result(file_id, name, user_id));
        }

        let results = service
            .search(user_id, tenant_id, "rustshare", 10)
            .await
            .unwrap();
        assert!(
            results.is_empty(),
            "Hidden metadata files should be excluded"
        );
    }

    #[tokio::test]
    async fn test_search_gracefully_handles_permission_errors() {
        let search_repo = Arc::new(MockSearchRepo::new());
        let permission_ops = Arc::new(MockPermissionOps::new());
        let permission_resolver = Arc::new(PermissionResolver::new(permission_ops.clone()));
        let service = SearchService::new(search_repo.clone(), permission_resolver);

        let user_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();

        // File in index but not in ops (simulating stale index entry)
        search_repo.add_result(tenant_id, search_result(file_id, "stale.txt", user_id));

        // Search should succeed, just skip the stale result
        let results = service
            .search(user_id, tenant_id, "stale", 10)
            .await
            .unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_search_respects_limit() {
        let search_repo = Arc::new(MockSearchRepo::new());
        let permission_ops = Arc::new(MockPermissionOps::new());
        let permission_resolver = Arc::new(PermissionResolver::new(permission_ops.clone()));
        let service = SearchService::new(search_repo.clone(), permission_resolver);

        let user_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        for i in 0..5 {
            let file_id = Uuid::new_v4();
            permission_ops.add_file(make_file(
                file_id,
                &format!("doc{}.txt", i),
                user_id,
                tenant_id,
            ));
            search_repo.add_result(
                tenant_id,
                search_result(file_id, &format!("doc{}.txt", i), user_id),
            );
        }

        let results = service.search(user_id, tenant_id, "doc", 3).await.unwrap();
        assert_eq!(results.len(), 3, "Search should respect the limit");
    }
}

#[cfg(test)]
mod compilation_check {
    #[test]
    fn test_this_module_is_compiled() {
        // Module compilation check
    }
}
