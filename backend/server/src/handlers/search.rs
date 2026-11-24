//! Unified search handler: `POST /api/v1/search`.
//!
//! Ranked, permission-aware search across Files/Notes, Buzz Chat, and Mail. Every
//! returned result is reauthorized by its current owning source and snippets
//! come only from authorized content (see `services::unified_search`).

use axum::{extract::State, Json};
use rustshare_core::domain::ApplicationId;
use rustshare_resource_auth::{PrincipalContext, Purpose, ResourceRef};
use serde::{Deserialize, Serialize};

use crate::handlers::{AppError, AuthenticatedUser};
use crate::services::unified_search::{
    parse_source_filter, SearchProvenance, SearchRunOptions, SearchScope, SearchSource,
    UnifiedSearchError, MAX_QUERY_CHARS,
};
use crate::AppState;
use std::collections::HashSet;
use uuid::Uuid;

#[derive(sqlx::FromRow)]
struct MeetingActionCandidate {
    action_id: Uuid,
    meeting_id: Uuid,
}

#[derive(sqlx::FromRow)]
struct MeetingActionSearchHit {
    title: String,
    description: Option<String>,
    updated_at: chrono::DateTime<chrono::Utc>,
    calendar_event_id: Option<Uuid>,
}

/// Unified search request.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct SearchRequest {
    /// The search query
    pub query: String,
    /// Maximum number of results (default: 10, max: 50)
    #[serde(default = "default_limit")]
    pub limit: usize,
    /// Restrict the search to `"files"`, `"chat"`, and/or `"mail"`.
    /// Omitted or empty searches both.
    #[serde(default)]
    pub sources: Option<Vec<String>>,
    /// Search the workspace (default), one Files folder/note, or a Chat channel.
    #[serde(default)]
    pub scope: Option<SearchScopeRequest>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SearchScopeRequest {
    Workspace,
    Folder {
        #[serde(rename = "resourceRef")]
        resource_ref: String,
    },
    Note {
        #[serde(rename = "resourceRef")]
        resource_ref: String,
    },
    ChatChannel {
        #[serde(rename = "communityId")]
        community_id: String,
        #[serde(rename = "channelId")]
        channel_id: String,
    },
}

pub(super) fn parse_scope(
    scope: Option<SearchScopeRequest>,
    sources: &[SearchSource],
) -> Result<SearchScope, String> {
    match scope {
        None | Some(SearchScopeRequest::Workspace) => Ok(SearchScope::Workspace),
        Some(SearchScopeRequest::Folder { resource_ref }) => {
            if !sources.is_empty() && !sources.contains(&SearchSource::Files) {
                return Err("folder scope requires the Files source".into());
            }
            let resource =
                ResourceRef::from_uri(&resource_ref).map_err(|error| error.to_string())?;
            if resource.application.to_string() != "io.elembra.files"
                || resource.resource_type != "folder"
            {
                return Err("scope is not a Files folder".into());
            }
            Ok(SearchScope::Folder(resource))
        }
        Some(SearchScopeRequest::Note { resource_ref }) => {
            if !sources.is_empty() && !sources.contains(&SearchSource::Files) {
                return Err("note scope requires the Files source".into());
            }
            let resource =
                ResourceRef::from_uri(&resource_ref).map_err(|error| error.to_string())?;
            if resource.application.to_string() != "io.elembra.files"
                || resource.resource_type != "file"
            {
                return Err("scope is not a Files note".into());
            }
            Ok(SearchScope::Resource(resource))
        }
        Some(SearchScopeRequest::ChatChannel {
            community_id,
            channel_id,
        }) => {
            if !sources.is_empty() && !sources.contains(&SearchSource::Chat) {
                return Err("channel scope requires the Chat source".into());
            }
            if community_id.trim().is_empty()
                || channel_id.trim().is_empty()
                || community_id.chars().count() > 256
                || channel_id.chars().count() > 256
            {
                return Err("invalid chat channel scope".into());
            }
            Ok(SearchScope::ChatChannel {
                community_id,
                channel_id,
            })
        }
    }
}

fn default_limit() -> usize {
    10
}

/// One ranked, permission-aware search result.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct SearchResultItem {
    pub source_application: String,
    pub source_type: String,
    /// Canonical `elembra://` resource reference (the cross-Application
    /// identity contract; feed it back into source authorization to open).
    pub resource_ref: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub occurred_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
    pub score: f32,
    pub provenance: SearchProvenance,
}

/// Unified search response.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct SearchResponse {
    /// Stable identifier used to correlate this query with Memory source audit events.
    pub request_id: uuid::Uuid,
    pub results: Vec<SearchResultItem>,
    pub total: usize,
}

/// POST /api/v1/search
///
/// Ranked, permission-aware search across Files/Notes, Buzz Chat, and Mail. Results
/// are reauthorized by their current owning sources; snippets come only from
/// authorized content.
#[utoipa::path(
    post,
    path = "/api/v1/search",
    tag = "Search",
    request_body = SearchRequest,
    responses(
        (status = 200, description = "Search results", body = SearchResponse),
        (status = 400, description = "Invalid request", body = crate::handlers::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 500, description = "Source access audit unavailable", body = crate::handlers::ErrorResponse),
        (status = 429, description = "Rate limited", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn unified_search(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Json(request): Json<SearchRequest>,
) -> Result<Json<SearchResponse>, AppError> {
    // Contract A-05: input validation.
    let query = request.query.trim();
    if query.is_empty() {
        return Err(AppError::bad_request("Query cannot be empty"));
    }
    if query.chars().count() > MAX_QUERY_CHARS {
        return Err(AppError::bad_request("Query too long (max 1000 chars)"));
    }

    // Parse the requested sources; omitted or empty means both.
    let sources = parse_source_filter(request.sources.as_deref()).map_err(AppError::bad_request)?;
    let scope = parse_scope(request.scope, &sources).map_err(AppError::bad_request)?;

    // The 1:1 tenant/workspace mapping: WorkspaceId == TenantId today.
    let ctx = PrincipalContext::user(
        rustshare_core::domain::PrincipalId(auth.user_id),
        rustshare_core::domain::TenantId(auth.tenant_id),
        rustshare_core::domain::WorkspaceId(auth.tenant_id),
    );

    let request_id = uuid::Uuid::new_v4();
    let response = state
        .unified_search_service
        .search_scoped_for_run(
            &ctx,
            query,
            &sources,
            request.limit,
            &scope,
            SearchRunOptions {
                request_id,
                include_resource_context: false,
            },
        )
        .await
        .map_err(|error| match error {
            UnifiedSearchError::InvalidQuery(message) => AppError::bad_request(message),
            UnifiedSearchError::AuditUnavailable => {
                AppError::internal("Search source audit unavailable")
            }
        })?;

    let mut results = Vec::with_capacity(response.results.len());
    for mut r in response.results {
        if let Some(file_id) = r.provenance.file_id {
            if let Ok(Some(file)) = state
                .metadata_store
                .find_file_by_id_for_tenant(file_id, auth.tenant_id)
                .await
            {
                if let Some(folder_id) = file.parent_folder_id {
                    if let Ok(meeting) = state
                        .meeting_service
                        .get_meeting(folder_id, auth.user_id, auth.tenant_id)
                        .await
                    {
                        r.provenance.meeting_id = Some(folder_id);
                        if file.name == "index.md" {
                            r.source_type = "meeting".to_string();
                            r.title = meeting.metadata.title;
                        }
                        let linked_event = sqlx::query_scalar::<_, Uuid>(
                            "SELECT event_id FROM calendar_event_meeting_links WHERE tenant_id=$1 AND meeting_folder_id=$2",
                        )
                        .bind(auth.tenant_id)
                        .bind(folder_id)
                        .fetch_optional(&state.db_pool)
                        .await
                        .ok()
                        .flatten();
                        if let Some(event_id) = linked_event {
                            if state
                                .calendar_service
                                .get_event(auth.tenant_id, auth.user_id, event_id)
                                .await
                                .is_ok()
                            {
                                r.provenance.calendar_event_id = Some(event_id);
                            }
                        }
                    }
                }
            }
        }
        results.push(SearchResultItem {
            source_application: r.source_application,
            source_type: r.source_type,
            resource_ref: r.resource_ref,
            title: r.title,
            snippet: r.snippet,
            location: r.location,
            occurred_at: r.occurred_at,
            updated_at: r.updated_at,
            score: r.score,
            provenance: r.provenance,
        });
    }

    let action_candidates = if matches!(scope, SearchScope::Workspace)
        && (sources.is_empty() || sources.contains(&SearchSource::Files))
    {
        sqlx::query_as::<_, MeetingActionCandidate>(
            "SELECT id AS action_id, meeting_id FROM meeting_actions WHERE tenant_id=$1 AND to_tsvector('simple', title || ' ' || coalesce(description,'')) @@ plainto_tsquery('simple', $2) ORDER BY updated_at DESC, id ASC LIMIT $3",
        )
        .bind(auth.tenant_id)
        .bind(query)
        .bind(request.limit.clamp(1, 50) as i64 * 4)
        .fetch_all(&state.db_pool)
        .await
        .unwrap_or_default()
    } else {
        Vec::new()
    };
    for candidate in action_candidates {
        let resource = ResourceRef::new(
            ApplicationId::new("io.elembra.files"),
            "folder",
            candidate.meeting_id.to_string(),
        );
        let Ok(resolved) = state
            .source_authorizer
            .resolve(&ctx, &resource, Purpose::SearchPreview)
            .await
        else {
            continue;
        };
        if !resolved.available {
            continue;
        }
        let Ok(meeting) = state
            .meeting_service
            .get_meeting(candidate.meeting_id, auth.user_id, auth.tenant_id)
            .await
        else {
            continue;
        };
        let Ok(Some(hit)) = sqlx::query_as::<_, MeetingActionSearchHit>(
            "SELECT title, description, updated_at, calendar_event_id FROM meeting_actions WHERE id=$1 AND meeting_id=$2 AND tenant_id=$3 AND to_tsvector('simple', title || ' ' || coalesce(description,'')) @@ plainto_tsquery('simple', $4)",
        )
        .bind(candidate.action_id)
        .bind(candidate.meeting_id)
        .bind(auth.tenant_id)
        .bind(query)
        .fetch_optional(&state.db_pool)
        .await
        else {
            continue;
        };
        let calendar_event_id = if let Some(event_id) = hit.calendar_event_id {
            state
                .calendar_service
                .get_event(auth.tenant_id, auth.user_id, event_id)
                .await
                .ok()
                .map(|_| event_id)
        } else {
            None
        };
        let snippet = hit
            .description
            .map(|description| description.chars().take(240).collect::<String>());
        let score = if hit.title.to_lowercase().contains(&query.to_lowercase()) {
            0.85
        } else {
            0.75
        };
        results.push(SearchResultItem {
            source_application: "io.elembra.files".to_string(),
            source_type: "meeting_action".to_string(),
            resource_ref: resource.to_uri(),
            title: hit.title,
            snippet,
            location: None,
            occurred_at: None,
            updated_at: Some(hit.updated_at),
            score,
            provenance: SearchProvenance {
                file_id: None,
                note_id: None,
                mime_type: None,
                message_id: None,
                community_id: None,
                channel_id: None,
                channel_kind: None,
                author_pubkey: None,
                meeting_id: Some(meeting.id),
                calendar_event_id,
                action_id: Some(candidate.action_id),
            },
        });
    }

    results.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.resource_ref.cmp(&b.resource_ref))
            .then_with(|| a.provenance.action_id.cmp(&b.provenance.action_id))
    });
    let mut seen = HashSet::new();
    results.retain(|result| {
        seen.insert(
            matches!(result.source_type.as_str(), "meeting" | "meeting_action")
                .then_some(result.provenance.meeting_id)
                .flatten()
                .map(|id| format!("meeting:{id}"))
                .or_else(|| result.provenance.action_id.map(|id| format!("action:{id}")))
                .unwrap_or_else(|| result.resource_ref.clone()),
        )
    });
    results.truncate(request.limit.clamp(1, 50));

    Ok(Json(SearchResponse {
        request_id,
        total: results.len(),
        results,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scoped_search_requests_require_the_matching_source_and_resource_type() {
        let files = [SearchSource::Files];
        let chat = [SearchSource::Chat];
        let folder = SearchScopeRequest::Folder {
            resource_ref: "elembra://io.elembra.files/folder/f-1".into(),
        };
        assert!(matches!(
            parse_scope(Some(folder), &files),
            Ok(SearchScope::Folder(_))
        ));
        assert!(parse_scope(
            Some(SearchScopeRequest::Folder {
                resource_ref: "elembra://io.elembra.files/folder/f-1".into()
            }),
            &chat
        )
        .is_err());
        assert!(parse_scope(
            Some(SearchScopeRequest::Note {
                resource_ref: "elembra://io.elembra.files/folder/f-1".into()
            }),
            &files
        )
        .is_err());
        assert!(matches!(
            parse_scope(
                Some(SearchScopeRequest::ChatChannel {
                    community_id: "community".into(),
                    channel_id: "channel".into()
                }),
                &chat
            ),
            Ok(SearchScope::ChatChannel { .. })
        ));
    }
}
