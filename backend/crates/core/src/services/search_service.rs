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
    async fn search(
        &self,
        tenant_id: Uuid,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>>;
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
    pub fn new(
        search_repo: Arc<SR>,
        permission_resolver: Arc<PermissionResolver<PR>>,
    ) -> Self {
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
            let permission = self
                .permission_resolver
                .resolve_permission(user_id, resource)
                .await?;

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

// Tests disabled - require async_trait crate
#[cfg(IGNORE)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::collections::HashMap;
    use std::sync::Mutex;

    // Mock SearchIndexRepository
    struct MockSearchRepo {
        results: Mutex<HashMap<String, Vec<SearchResult>>>,
    }

    impl MockSearchRepo {
        fn new() -> Self {
            Self {
                results: Mutex::new(HashMap::new()),
            }
        }

        fn add_result(&self, query: &str, result: SearchResult) {
            self.results
                .lock()
                .unwrap()
                .entry(query.to_string())
                .or_default()
                .push(result);
        }
    }

    impl SearchIndexRepository for MockSearchRepo {
        async fn search(
            &self,
            _tenant_id: Uuid,
            query: &str,
            _limit: usize,
        ) -> Result<Vec<SearchResult>> {
            let results = self.results.lock().unwrap();
            Ok(results.get(query).cloned().unwrap_or_default())
        }
    }

    // Mock PermissionResolverOps
    struct MockPermissionOps;

    #[async_trait::async_trait]
    impl PermissionResolverOps for MockPermissionOps {
        async fn find_user_share(
            &self,
            _file_id: Option<Uuid>,
            _folder_id: Option<Uuid>,
            _recipient_user_id: UserId,
        ) -> Result<Option<crate::domain::Share>> {
            Ok(None)
        }

        async fn find_group_shares(
            &self,
            _file_id: Option<Uuid>,
            _folder_id: Option<Uuid>,
            _group_ids: &[Uuid],
        ) -> Result<Vec<crate::domain::Share>> {
            Ok(Vec::new())
        }

        async fn find_file_by_id(&self, id: Uuid) -> Result<Option<crate::domain::File>> {
            Ok(Some(crate::domain::File::new(
                "test.txt".to_string(),
                "/test.txt".to_string(),
                "hash".to_string(),
                100,
                "text/plain".to_string(),
                None,
                id, // owner is the same as requested user
            )))
        }

        async fn find_folder_by_id(&self, id: Uuid) -> Result<Option<crate::domain::Folder>> {
            Ok(Some(crate::domain::Folder::new_root(id)))
        }

        async fn get_user_group_ids(&self, _user_id: UserId) -> Result<Vec<Uuid>> {
            Ok(Vec::new())
        }
    }

    #[tokio::test]
    async fn test_search_filters_by_permission() {
        let search_repo = Arc::new(MockSearchRepo::new());
        let permission_ops = Arc::new(MockPermissionOps);
        let permission_resolver = Arc::new(PermissionResolver::new(permission_ops));

        let service = SearchService::new(search_repo.clone(), permission_resolver);

        let user_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();

        // Add a search result
        search_repo.add_result(
            "document",
            SearchResult {
                id: file_id,
                resource_type: "file".to_string(),
                name: "document.txt".to_string(),
                path: "/Documents/document.txt".to_string(),
                owner_id: user_id,
                updated_at: Utc::now(),
            },
        );

        let results = service
            .search(user_id, tenant_id, "document", 10)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "document.txt");
    }
}
