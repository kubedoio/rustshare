//! Permission-aware unified search over Files/Notes and Buzz Chat.
//!
//! Candidate producers (Files metadata, the note index, the Memory catalog)
//! are only *candidate* sources: final inclusion requires CURRENT source
//! authorization via [`SourceAuthorizer`] — Files → `FilesResourceOwner` →
//! `PermissionResolver`; Chat → `ChatResourceOwner` → the configured
//! `BuzzAuthority`. Snippets are built ONLY from authorized `fetch` bytes; a
//! stale or malicious index hint (`cached_hint`) never reaches the output.
//!
//! Fail-closed semantics: an unavailable candidate source or authorizer drops
//! only what it covers — one broken source never fails the whole search and
//! never corrupts the other sources' results.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use rustshare_core::domain::{ActionCapability, ApplicationId};
use rustshare_core::services::is_hidden_file_name;
use rustshare_core::services::SemanticSearchResult;
use rustshare_memory::MemoryCatalogRecord;
use rustshare_resource_auth::{
    PrincipalContext, Purpose, Representation, ResourceRef, SourceAuthorizer, SourceError,
    CHAT_READ, FILES_READ, MAX_BATCH_SIZE,
};
use rustshare_storage::{MemoryCatalogStore, MetadataStore};
use uuid::Uuid;

use crate::state::AppAiService;

/// Canonical owning Application of Files resources.
const FILES_APPLICATION: &str = "io.elembra.files";
/// Canonical owning Application of Chat resources.
const CHAT_APPLICATION: &str = "io.elembra.chat";
/// Maximum number of source filter values accepted by the HTTP contracts.
pub const MAX_SOURCE_FILTERS: usize = 2;
/// Shared query bound used by Search and Ask Workspace.
pub const MAX_QUERY_CHARS: usize = 1_000;
/// Snippet length cap (in chars) applied to authorized `fetch` text.
const SNIPPET_MAX_CHARS: usize = 240;
/// Window (in chars) of context kept before the first query-term match.
const SNIPPET_PRE_MATCH_CHARS: usize = 40;

/// Which source Applications a query should search.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchSource {
    /// Files metadata + note index (semantic/keyword).
    Files,
    /// Buzz Chat memory catalog.
    Chat,
}

/// Errors raised by the unified search service.
///
/// Only client-input errors are surfaced: per-candidate denials and
/// per-source failures are absorbed (dropped) inside the service, so a broken
/// source can never fail the whole search or reveal a denial.
#[derive(Debug, thiserror::Error)]
pub enum UnifiedSearchError {
    #[error("invalid query: {0}")]
    InvalidQuery(String),
}

/// An internal, pre-authorization candidate.
///
/// `cached_hint` is an index/Memory hint and MUST never reach the response —
/// snippets come only from the authorized `fetch`.
#[derive(Debug, Clone)]
struct Candidate {
    source_application: String,
    source_type: String,
    resource_ref: ResourceRef,
    score: f32,
    title: String,
    /// Candidate/index-sourced context (Files path; Chat community/channel).
    /// Context only, never authorization.
    location: Option<String>,
    occurred_at: Option<DateTime<Utc>>,
    file_id: Option<Uuid>,
    note_id: Option<Uuid>,
    mime_type: Option<String>,
    /// Index/Memory hint that MUST never reach the response; it exists only to
    /// document the never-emitted candidate text (snippets come from the
    /// authorized fetch).
    #[allow(dead_code)]
    cached_hint: Option<String>,
    // Chat provenance extras (citation context, never authorization).
    message_id: Option<String>,
    community_id: Option<String>,
    channel_id: Option<String>,
    channel_kind: Option<String>,
    author_pubkey: Option<String>,
}

/// Response contract of the unified search endpoint.
#[derive(Debug, Clone)]
pub struct UnifiedSearchResponse {
    pub results: Vec<UnifiedSearchResult>,
}

/// One ranked, permission-aware search result.
#[derive(Debug, Clone)]
pub struct UnifiedSearchResult {
    pub source_application: String,
    pub source_type: String,
    pub resource_ref: String,
    pub title: String,
    pub snippet: Option<String>,
    /// Candidate/index-sourced context (Files path; Chat community/channel).
    /// Context only, never authorization.
    pub location: Option<String>,
    pub occurred_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub score: f32,
    pub provenance: SearchProvenance,
}

/// Per-source provenance context for citation (never authorization).
#[derive(serde::Serialize, Debug, Clone, PartialEq, utoipa::ToSchema)]
pub struct SearchProvenance {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub community_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author_pubkey: Option<String>,
}

/// Source bytes that passed a second, RAG-specific authorization/materialization pass.
#[derive(Debug, Clone)]
pub struct RagSource {
    pub resource: ResourceRef,
    pub title: String,
    pub location: Option<String>,
    pub provenance: SearchProvenance,
    pub text: String,
}

/// Unified search over Files/Notes and Buzz Chat.
pub struct UnifiedSearchService {
    authorizer: Arc<SourceAuthorizer>,
    metadata: Arc<MetadataStore>,
    ai: Option<Arc<AppAiService>>,
    memory_catalog: Arc<MemoryCatalogStore>,
}

impl UnifiedSearchService {
    pub fn new(
        authorizer: Arc<SourceAuthorizer>,
        metadata: Arc<MetadataStore>,
        ai: Option<Arc<AppAiService>>,
        memory_catalog: Arc<MemoryCatalogStore>,
    ) -> Self {
        Self {
            authorizer,
            metadata,
            ai,
            memory_catalog,
        }
    }

    /// Run one permission-aware unified search.
    ///
    /// Flow:
    /// 1. validate the query (non-empty, ≤ 1000 chars) and clamp the limit;
    /// 2. collect candidates per source (an empty `sources` slice means both);
    ///    a broken candidate producer logs and contributes nothing;
    /// 3. preliminary dedupe/rank capped at `limit * 4`;
    /// 4. batch reauthorize with the owning source (`FILES_READ` /
    ///    `CHAT_READ`) in chunks of [`MAX_BATCH_SIZE`]; only `Allow` survives
    ///    and a failed chunk fails closed (dropped);
    /// 5. final deterministic rank → top `limit`;
    /// 6. per final result: `resolve` (`SearchPreview`) then `fetch` (Text);
    ///    the snippet is built only from authorized bytes; `VersionUnavailable`
    ///    keeps a reference-only chat result without a snippet; any other
    ///    failure drops only that item (existence-hiding);
    /// 7. return the normalized response contract.
    pub async fn search(
        &self,
        ctx: &PrincipalContext,
        query: &str,
        sources: &[SearchSource],
        limit: usize,
    ) -> Result<UnifiedSearchResponse, UnifiedSearchError> {
        // Step 1: validate input.
        let query = validate_query(query)?;
        let limit = limit.clamp(1, 50);
        // Per-source candidate budget (integer math).
        let budget = limit * 4;

        // Step 2: collect candidates per source.
        let files_requested = sources.is_empty() || sources.contains(&SearchSource::Files);
        let chat_requested = sources.is_empty() || sources.contains(&SearchSource::Chat);
        let mut candidates = Vec::new();
        if files_requested {
            candidates.extend(self.files_candidates(ctx, query, budget).await);
        }
        if chat_requested {
            candidates.extend(self.chat_candidates(ctx, query, budget).await);
        }

        // Step 3: preliminary dedupe/rank, capped at `limit * 4`.
        let ranked = dedupe_and_rank(candidates, budget);

        // Step 4: current source reauthorization (fail closed per chunk).
        let allowed = self.authorize_candidates(ctx, ranked).await;

        // Step 5: final deterministic rank → top `limit`.
        let final_candidates = dedupe_and_rank(allowed, limit);

        // Step 6: resolve + fetch per final result; a per-item failure never
        // aborts the loop or corrupts the other results.
        let mut results = Vec::with_capacity(final_candidates.len());
        for candidate in final_candidates {
            let reference = candidate.resource_ref.clone();
            // resolve() reauthorizes; a failure hides existence and drops only
            // this item.
            let resolved = match self
                .authorizer
                .resolve(ctx, &reference, Purpose::SearchPreview)
                .await
            {
                Ok(resolved) => resolved,
                Err(error) => {
                    tracing::warn!(
                        resource = %reference,
                        %error,
                        "unified search: resolve failed; dropping result"
                    );
                    continue;
                }
            };
            // Title comes from the authorized resolve; fall back to the
            // candidate hint when the owner returned an empty name.
            let title = if resolved.display_name.is_empty() {
                candidate.title
            } else {
                resolved.display_name
            };
            // The snippet is built ONLY from authorized fetch bytes; the
            // cached index/Memory hint never reaches the output.
            let snippet = match self
                .authorizer
                .fetch(ctx, &reference, Representation::Text)
                .await
            {
                Ok(fetched) => Some(authorized_snippet(&fetched.data, query, SNIPPET_MAX_CHARS)),
                // Reference-only chat messages have no indexing copy; the
                // result stays, just without a snippet.
                Err(SourceError::VersionUnavailable) => None,
                Err(error) => {
                    tracing::warn!(
                        resource = %reference,
                        %error,
                        "unified search: fetch failed; dropping result"
                    );
                    continue;
                }
            };
            results.push(UnifiedSearchResult {
                source_application: candidate.source_application,
                source_type: candidate.source_type,
                resource_ref: reference.to_uri(),
                title,
                snippet,
                location: candidate.location,
                occurred_at: candidate.occurred_at,
                updated_at: resolved.updated_at.or(candidate.occurred_at),
                score: candidate.score,
                provenance: SearchProvenance {
                    file_id: candidate.file_id,
                    note_id: candidate.note_id,
                    mime_type: candidate.mime_type,
                    message_id: candidate.message_id,
                    community_id: candidate.community_id,
                    channel_id: candidate.channel_id,
                    channel_kind: candidate.channel_kind,
                    author_pubkey: candidate.author_pubkey,
                },
            });
        }

        // Step 7: return the normalized contract.
        Ok(UnifiedSearchResponse { results })
    }

    /// Reauthorize and fetch search results for RAG. Search output is only a
    /// candidate list; this method is the generation-time authority boundary.
    pub async fn materialize_for_rag(
        &self,
        ctx: &PrincipalContext,
        results: &[UnifiedSearchResult],
        max_sources: usize,
        max_bytes_per_source: usize,
        max_total_bytes: usize,
    ) -> Vec<RagSource> {
        let mut total = 0;
        let mut materialized = Vec::new();
        for result in results.iter().take(max_sources) {
            let Ok(resource) = ResourceRef::from_uri(&result.resource_ref) else {
                continue;
            };
            let Ok(display) = self
                .authorizer
                .resolve(ctx, &resource, Purpose::RagContext)
                .await
            else {
                continue;
            };
            let Ok(fetched) = self
                .authorizer
                .fetch(ctx, &resource, Representation::Text)
                .await
            else {
                // Reference-only Chat records and revoked/deleted resources
                // never become LLM context.
                continue;
            };
            let data = &fetched.data[..fetched.data.len().min(max_bytes_per_source)];
            let text = String::from_utf8_lossy(data);
            let bounded: String = text.chars().take(max_bytes_per_source).collect();
            if bounded.is_empty() || total + bounded.len() > max_total_bytes {
                continue;
            }
            total += bounded.len();
            materialized.push(RagSource {
                resource,
                title: display.display_name.clone(),
                location: result.location.clone(),
                provenance: result.provenance.clone(),
                text: bounded,
            });
        }
        materialized
    }

    /// Files candidates: live `files` rows matching name/path (always) plus
    /// note-index keyword + vector hits when the AI service is configured.
    ///
    /// Candidate-only: the files table and note index are never the
    /// authorization gate — the source authorizer reauthorizes every
    /// candidate against current Files state.
    async fn files_candidates(
        &self,
        ctx: &PrincipalContext,
        query: &str,
        budget: usize,
    ) -> Vec<Candidate> {
        // Name/path candidates from the files table (tenant-scoped, live rows).
        let files = match self
            .metadata
            .search_files_by_name_path(ctx.tenant_id.0, query, budget)
            .await
        {
            Ok(files) => files,
            Err(error) => {
                tracing::warn!(
                    tenant = %ctx.tenant_id,
                    %error,
                    "unified search: files name/path candidates unavailable; skipping source"
                );
                Vec::new()
            }
        };
        let mut candidates: Vec<Candidate> = files
            .into_iter()
            .filter_map(|file| {
                // Hidden metadata files never surface as candidates.
                if is_hidden_file_name(&file.name) {
                    return None;
                }
                let score = name_path_score(&file.name, &file.path, query);
                if score <= 0.0 {
                    return None;
                }
                Some(Candidate {
                    source_application: FILES_APPLICATION.to_string(),
                    source_type: "file".to_string(),
                    resource_ref: ResourceRef::new(
                        ApplicationId::new(FILES_APPLICATION),
                        "file",
                        file.id.to_string(),
                    ),
                    score,
                    title: file.name,
                    location: Some(file.path),
                    occurred_at: Some(file.modified_at),
                    file_id: Some(file.id),
                    note_id: None,
                    mime_type: Some(file.mime_type),
                    cached_hint: None,
                    message_id: None,
                    community_id: None,
                    channel_id: None,
                    channel_kind: None,
                    author_pubkey: None,
                })
            })
            .collect();

        // Note-index candidates (keyword + vector) when AI is configured.
        if let Some(ai) = &self.ai {
            let user_id = ctx.principal_id.0;
            let tenant_id = ctx.tenant_id.0;
            match ai.semantic_search(query, user_id, tenant_id, budget).await {
                Ok(results) => candidates.extend(results.into_iter().map(semantic_candidate)),
                Err(error) => {
                    tracing::warn!(
                        %error,
                        "unified search: semantic candidates unavailable; skipping source"
                    );
                }
            }
            match ai.keyword_search(query, user_id, tenant_id, budget).await {
                Ok(results) => candidates.extend(results.into_iter().map(semantic_candidate)),
                Err(error) => {
                    tracing::warn!(
                        %error,
                        "unified search: keyword candidates unavailable; skipping source"
                    );
                }
            }
        }

        candidates
    }

    /// Chat candidates: Memory catalog rows matching the query.
    ///
    /// Authorization never consults `memory_catalog` — the Chat owner defers
    /// the final channel/message decision to the configured Buzz authority.
    async fn chat_candidates(
        &self,
        ctx: &PrincipalContext,
        query: &str,
        budget: usize,
    ) -> Vec<Candidate> {
        let records = match self
            .memory_catalog
            .search(ctx.tenant_id, query, budget)
            .await
        {
            Ok(records) => records,
            Err(error) => {
                tracing::warn!(
                    tenant = %ctx.tenant_id,
                    %error,
                    "unified search: chat candidates unavailable; skipping source"
                );
                return Vec::new();
            }
        };
        records
            .into_iter()
            .filter_map(|record| {
                let score = chat_candidate_score(
                    record.content.as_deref(),
                    &record.message_id,
                    &record.author_pubkey,
                    &record.channel_id,
                    query,
                );
                if score <= 0.0 {
                    return None;
                }
                Some(chat_candidate(record, score))
            })
            .collect()
    }

    /// Batch reauthorization against the CURRENT owning source. Only
    /// `Decision::Allow` survives; a denied/missing/invalid ref never affects
    /// another, and an authorizer failure fails closed by dropping the chunk.
    async fn authorize_candidates(
        &self,
        ctx: &PrincipalContext,
        candidates: Vec<Candidate>,
    ) -> Vec<Candidate> {
        let files_action = ActionCapability::new(FILES_READ);
        let chat_action = ActionCapability::new(CHAT_READ);

        // Split by the owning Application's action. Unknown applications are
        // dropped (fail closed) rather than silently grouped into an action
        // they do not own.
        let mut files_group = Vec::new();
        let mut chat_group = Vec::new();
        for candidate in candidates {
            match candidate.source_application.as_str() {
                FILES_APPLICATION => files_group.push(candidate),
                CHAT_APPLICATION => chat_group.push(candidate),
                other => {
                    tracing::warn!(
                        source_application = other,
                        "unified search: unknown source application; dropping candidate"
                    );
                }
            }
        }

        let mut allowed = Vec::with_capacity(files_group.len() + chat_group.len());
        for (action, group) in [(&files_action, &files_group), (&chat_action, &chat_group)] {
            for chunk in group.chunks(MAX_BATCH_SIZE) {
                let refs: Vec<ResourceRef> = chunk.iter().map(|c| c.resource_ref.clone()).collect();
                let decisions = match self.authorizer.authorize_batch(ctx, action, &refs).await {
                    Ok(decisions) => decisions,
                    Err(error) => {
                        // Fail closed: an unavailable authorizer drops the
                        // whole chunk rather than risk a stale allow.
                        tracing::warn!(
                            action = %action,
                            %error,
                            "unified search: batch authorization failed; dropping chunk"
                        );
                        continue;
                    }
                };
                for (candidate, decision) in chunk.iter().zip(&decisions) {
                    if decision.decision.is_allow() {
                        allowed.push(candidate.clone());
                    }
                }
            }
        }
        allowed
    }
}

/// Map a permission-filtered note-index hit to a Files candidate. The AI
/// result is still only a candidate: the source authorizer reauthorizes it
/// against current Files state before inclusion.
fn semantic_candidate(result: SemanticSearchResult) -> Candidate {
    Candidate {
        source_application: FILES_APPLICATION.to_string(),
        source_type: "file".to_string(),
        resource_ref: ResourceRef::new(
            ApplicationId::new(FILES_APPLICATION),
            "file",
            result.file_id.to_string(),
        ),
        score: result.relevance_score,
        title: result.file_name,
        location: Some(result.file_path),
        occurred_at: None,
        file_id: Some(result.file_id),
        note_id: Some(result.file_id),
        mime_type: Some(result.mime_type),
        cached_hint: Some(result.snippet),
        message_id: None,
        community_id: None,
        channel_id: None,
        channel_kind: None,
        author_pubkey: None,
    }
}

/// Map a Memory catalog record to a Chat candidate. `content` becomes the
/// never-emitted `cached_hint`; the snippet, when present, comes from the
/// authorized fetch.
fn chat_candidate(record: MemoryCatalogRecord, score: f32) -> Candidate {
    Candidate {
        source_application: CHAT_APPLICATION.to_string(),
        source_type: "message".to_string(),
        resource_ref: ResourceRef::new(
            ApplicationId::new(CHAT_APPLICATION),
            "message",
            record.message_id.clone(),
        ),
        score,
        title: "chat message".to_string(),
        location: Some(format!(
            "community:{} / channel:{}",
            record.community_id, record.channel_id
        )),
        occurred_at: Some(record.occurred_at),
        file_id: None,
        note_id: None,
        mime_type: None,
        cached_hint: record.content,
        message_id: Some(record.message_id),
        community_id: Some(record.community_id),
        channel_id: Some(record.channel_id),
        channel_kind: Some(record.channel_kind.as_str().to_string()),
        author_pubkey: Some(record.author_pubkey),
    }
}

/// Trim and validate the search query; returns the trimmed query.
fn validate_query(query: &str) -> Result<&str, UnifiedSearchError> {
    let query = query.trim();
    if query.is_empty() {
        return Err(UnifiedSearchError::InvalidQuery(
            "Query cannot be empty".to_string(),
        ));
    }
    if query.chars().count() > MAX_QUERY_CHARS {
        return Err(UnifiedSearchError::InvalidQuery(
            "Query too long (max 1000 chars)".to_string(),
        ));
    }
    Ok(query)
}

/// Parse the bounded public source filter. An empty filter means all sources.
pub fn parse_source_filter(names: Option<&[String]>) -> Result<Vec<SearchSource>, String> {
    let Some(names) = names.filter(|names| !names.is_empty()) else {
        return Ok(Vec::new());
    };
    if names.len() > MAX_SOURCE_FILTERS {
        return Err(format!(
            "at most {MAX_SOURCE_FILTERS} source filters are allowed"
        ));
    }
    let mut parsed = Vec::with_capacity(names.len());
    for name in names {
        let source = match name.as_str() {
            "files" => SearchSource::Files,
            "chat" => SearchSource::Chat,
            other => return Err(format!("Unknown source '{other}'")),
        };
        if parsed.contains(&source) {
            return Err(format!("duplicate source filter '{name}'"));
        }
        parsed.push(source);
    }
    Ok(parsed)
}

/// Deterministic name/path match score: exact-name 1.0, name-prefix 0.9,
/// substring in name or path 0.6, no match 0.0. Case-insensitive.
pub fn name_path_score(name: &str, path: &str, query: &str) -> f32 {
    let name_lower = name.to_lowercase();
    let query_lower = query.to_lowercase();
    if name_lower == query_lower {
        1.0
    } else if name_lower.starts_with(&query_lower) {
        0.9
    } else if name_lower.contains(&query_lower) || path.to_lowercase().contains(&query_lower) {
        0.6
    } else {
        0.0
    }
}

/// Chat candidate score: 0.8 when any query term appears in the message
/// content, 0.4 when the message id/author pubkey matches the query exactly
/// or the channel id contains it, 0.0 otherwise. Mirrors the Memory catalog
/// search conditions.
pub fn chat_candidate_score(
    content: Option<&str>,
    message_id: &str,
    author_pubkey: &str,
    channel_id: &str,
    query: &str,
) -> f32 {
    let query_lower = query.to_lowercase();
    let content_match = content.is_some_and(|content| {
        let content_lower = content.to_lowercase();
        query_lower
            .split_whitespace()
            .any(|term| content_lower.contains(term))
    });
    if content_match {
        return 0.8;
    }
    if message_id == query
        || author_pubkey == query
        || channel_id.to_lowercase().contains(&query_lower)
    {
        return 0.4;
    }
    0.0
}

/// Dedupe candidates by canonical `ResourceRef` URI keeping the max score,
/// then sort deterministically: score desc, occurred_at desc (`None` last),
/// source application, ref URI. Stable; truncates to `limit`.
fn dedupe_and_rank(candidates: Vec<Candidate>, limit: usize) -> Vec<Candidate> {
    let mut by_ref: HashMap<String, Candidate> = HashMap::with_capacity(candidates.len());
    for candidate in candidates {
        by_ref
            .entry(candidate.resource_ref.to_uri())
            .and_modify(|existing| {
                if candidate.score > existing.score {
                    let mut winner = candidate.clone();
                    if winner.occurred_at.is_none() {
                        // A note-index hit (no timestamp) winning over a
                        // name/path hit must not lose the timestamp.
                        winner.occurred_at = existing.occurred_at;
                    }
                    *existing = winner;
                } else if existing.occurred_at.is_none() {
                    // Keep the higher-scored candidate, but backfill the
                    // timestamp when only the lower-scored one has it.
                    existing.occurred_at = candidate.occurred_at;
                }
            })
            .or_insert(candidate);
    }

    let mut ranked: Vec<Candidate> = by_ref.into_values().collect();
    ranked.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.occurred_at.cmp(&a.occurred_at))
            .then_with(|| a.source_application.cmp(&b.source_application))
            .then_with(|| a.resource_ref.to_uri().cmp(&b.resource_ref.to_uri()))
    });
    ranked.truncate(limit);
    ranked
}

/// Build a UTF-8-safe snippet from authorized fetch bytes.
///
/// Locates the first query-term occurrence in char-index space, keeps ~40
/// chars of context before it, truncates on char boundaries with a trailing
/// `…` when cut. No match → the first `max_chars` chars (ellipsis when longer).
pub fn authorized_snippet(data: &[u8], query: &str, max_chars: usize) -> String {
    let text = String::from_utf8_lossy(data);
    let chars: Vec<char> = text.chars().collect();
    let text_lower = text.to_lowercase();
    let query_lower = query.to_lowercase();
    let terms: Vec<&str> = query_lower.split_whitespace().collect();

    // First query-term occurrence in char-index space. `char_indices` yields
    // byte indices on char boundaries, so slicing `text_lower` is safe.
    let match_index = if terms.is_empty() {
        None
    } else {
        text_lower
            .char_indices()
            .enumerate()
            .find_map(|(char_index, (byte_index, _))| {
                let rest = &text_lower[byte_index..];
                terms
                    .iter()
                    .any(|term| rest.starts_with(term))
                    .then_some(char_index)
            })
    };

    // `text_lower` can have MORE chars than the original text when
    // `to_lowercase` expands a char (e.g. `İ` → `i` + combining dot, `ẞ` →
    // `ss`), so a match index computed in the lowercased space can exceed the
    // original length. Treat such an index as "no match" (fall back to the
    // head of the text) and clamp the window so the slice can never panic.
    let match_index = match_index.filter(|&index| index < chars.len());
    let window_start = match_index
        .unwrap_or(0)
        .saturating_sub(SNIPPET_PRE_MATCH_CHARS)
        .min(chars.len());
    let mut window_end = window_start.saturating_add(max_chars);
    let truncated = window_end < chars.len();
    if window_end > chars.len() {
        window_end = chars.len();
    }
    let mut snippet: String = chars[window_start..window_end].iter().collect();
    if truncated {
        snippet.push('…');
    }
    snippet
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A Files-shaped candidate; chat tests can override the source name.
    fn candidate(ref_id: &str, score: f32, occurred: Option<&str>, source: &str) -> Candidate {
        Candidate {
            source_application: source.to_string(),
            source_type: "file".to_string(),
            resource_ref: ResourceRef::new(ApplicationId::new(source), "file", ref_id),
            score,
            title: ref_id.to_string(),
            location: None,
            occurred_at: occurred.map(|at| {
                DateTime::parse_from_rfc3339(at)
                    .unwrap()
                    .with_timezone(&Utc)
            }),
            file_id: None,
            note_id: None,
            mime_type: None,
            cached_hint: None,
            message_id: None,
            community_id: None,
            channel_id: None,
            channel_kind: None,
            author_pubkey: None,
        }
    }

    #[test]
    fn hidden_file_names_are_recognized() {
        for name in [
            ".rustshare",
            ".rustshare_meta",
            ".rustshare/state",
            "events.jsonl",
            "index.md",
            "__primary__.md",
            "config.editor.json",
        ] {
            assert!(is_hidden_file_name(name), "expected {name} to be hidden");
        }
        for name in [
            "notes.md",
            "hello.txt",
            "index.md.bak",
            "editor.json",
            "README.md",
        ] {
            assert!(!is_hidden_file_name(name), "expected {name} to be visible");
        }
    }

    #[test]
    fn name_path_score_is_deterministic_and_ordered() {
        assert_eq!(
            name_path_score("report.md", "/docs/report.md", "report.md"),
            1.0,
            "exact name match scores highest"
        );
        assert_eq!(
            name_path_score("reports.md", "/docs/reports.md", "report"),
            0.9,
            "name prefix match"
        );
        assert_eq!(
            name_path_score("2024-annual-report.md", "/docs/annual.md", "annual"),
            0.6,
            "substring in name"
        );
        assert_eq!(
            name_path_score("meeting-notes.md", "/docs/meeting-notes.md", "notes"),
            0.6,
            "substring in name"
        );
        assert_eq!(
            name_path_score("readme.md", "/docs/meeting-notes.md", "notes"),
            0.6,
            "substring in path"
        );
        assert_eq!(
            name_path_score("notes.md", "/docs/meeting-notes.md", "presentation"),
            0.0,
            "no match"
        );
        assert_eq!(
            name_path_score("Notes.MD", "/docs/notes.md", "notes.md"),
            1.0,
            "matching is case-insensitive"
        );
    }

    #[test]
    fn chat_candidate_score_prefers_content_then_metadata() {
        let content = Some("the quarterly plan was approved");
        assert_eq!(
            chat_candidate_score(content, "msg1", "pubkey1", "chan1", "quarterly"),
            0.8,
            "content term match"
        );
        assert_eq!(
            chat_candidate_score(content, "msg1", "pubkey1", "chan1", "approved plan"),
            0.8,
            "any query term in content"
        );
        assert_eq!(
            chat_candidate_score(None, "msg1", "pubkey1", "chan1", "msg1"),
            0.4,
            "message id exact match"
        );
        assert_eq!(
            chat_candidate_score(None, "msg1", "pubkey1", "chan1", "pubkey1"),
            0.4,
            "author pubkey exact match"
        );
        assert_eq!(
            chat_candidate_score(None, "msg1", "pubkey1", "chan1", "chan1"),
            0.4,
            "channel id contains query"
        );
        assert_eq!(
            chat_candidate_score(content, "msg1", "pubkey1", "chan1", "missing"),
            0.0,
            "no match"
        );
    }

    #[test]
    fn dedupe_keeps_max_score_and_ranks_deterministically() {
        let candidates = vec![
            candidate("b", 0.6, Some("2024-01-02T00:00:00Z"), "io.elembra.files"),
            candidate("a", 0.9, Some("2024-01-01T00:00:00Z"), "io.elembra.files"),
            // Duplicate of "b" with a higher score: dedupe must keep 0.9.
            candidate("b", 0.9, Some("2024-01-01T00:00:00Z"), "io.elembra.files"),
            // Same score as "a", but no timestamp: sorts after it.
            candidate("c", 0.9, None, "io.elembra.files"),
        ];
        let ranked = dedupe_and_rank(candidates, 10);
        let ids: Vec<&str> = ranked
            .iter()
            .map(|c| c.resource_ref.resource_id.as_str())
            .collect();
        assert_eq!(ids, vec!["a", "b", "c"], "deterministic rank order");
        let b = ranked
            .iter()
            .find(|c| c.resource_ref.resource_id == "b")
            .expect("b present");
        assert_eq!(b.score, 0.9, "dedupe keeps the max score");
    }

    #[test]
    fn dedupe_and_rank_truncates_to_limit() {
        let candidates: Vec<Candidate> = (0..5)
            .map(|i| {
                candidate(
                    &format!("f{i}"),
                    1.0 - (i as f32) * 0.1,
                    None,
                    "io.elembra.files",
                )
            })
            .collect();
        let ranked = dedupe_and_rank(candidates, 3);
        assert_eq!(ranked.len(), 3);
        assert_eq!(ranked[0].resource_ref.resource_id, "f0");
        assert_eq!(ranked[2].resource_ref.resource_id, "f2");
    }

    #[test]
    fn dedupe_preserves_occurred_at_from_the_losing_candidate() {
        // A note-index hit (no timestamp) wins on score over a name/path hit
        // that carries the modified time; the timestamp must survive.
        let with_ts = candidate("f", 0.6, Some("2024-01-01T00:00:00Z"), "io.elembra.files");
        let no_ts = candidate("f", 0.9, None, "io.elembra.files");
        let ranked = dedupe_and_rank(vec![with_ts.clone(), no_ts.clone()], 10);
        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].score, 0.9, "max score wins");
        assert_eq!(
            ranked[0].occurred_at, with_ts.occurred_at,
            "timestamp from the lower-scored candidate is preserved"
        );
    }

    #[test]
    fn snippet_windows_around_the_match() {
        let text = format!("{}needle{}", "a".repeat(50), "b".repeat(300));
        let snippet = authorized_snippet(text.as_bytes(), "needle", 100);
        assert!(snippet.contains("needle"));
        assert_eq!(
            snippet.chars().count(),
            101,
            "100 chars of window plus the trailing ellipsis"
        );
        // The window starts ~40 chars before the match (50 - 40 = 10).
        let match_pos = snippet.find("needle").unwrap();
        assert!(
            match_pos <= 41,
            "window should start about 40 chars before the match, got {match_pos}"
        );
    }

    #[test]
    fn snippet_without_match_takes_first_chars() {
        let text = "z".repeat(500);
        let snippet = authorized_snippet(text.as_bytes(), "needle", 100);
        assert_eq!(snippet.chars().count(), 101, "100 chars plus ellipsis");
        assert!(snippet.ends_with('…'));

        let short = "hello world";
        let snippet = authorized_snippet(short.as_bytes(), "needle", 100);
        assert_eq!(snippet, "hello world", "short text is not truncated");
        assert!(!snippet.ends_with('…'));
    }

    #[test]
    fn snippet_is_utf8_safe_across_multibyte_boundaries() {
        // "界" is 3 bytes; the match and the truncation point both land on
        // multi-byte boundaries — the helper must never panic or split a char.
        let prefix = format!("{}needle", "界".repeat(10));
        let text = format!("{prefix}{}", "界".repeat(200));
        let snippet = authorized_snippet(text.as_bytes(), "needle", 80);
        assert!(snippet.contains("needle"));
        assert!(
            snippet.chars().count() <= 81,
            "truncation stays on char boundaries"
        );
    }

    #[test]
    fn snippet_never_panics_on_case_mapping_expansion() {
        // `İ` (U+0130) lowercases to TWO chars (i + combining dot), so the
        // lowercased string has MORE chars than the original. A match after a
        // run of them used to compute an out-of-range window and panic; the
        // helper must clamp and fall back to the head of the text.
        let text = format!("{}needle", "İ".repeat(400));
        let snippet = authorized_snippet(text.as_bytes(), "needle", 100);
        assert!(!snippet.is_empty(), "falls back to the head of the text");

        // An expansion char as the query term itself must also be safe.
        let snippet = authorized_snippet(text.as_bytes(), "İ", 100);
        assert!(!snippet.is_empty());
    }

    #[test]
    fn snippet_finds_the_earliest_query_term() {
        let text = "zzz alpha zzz beta";
        let snippet = authorized_snippet(text.as_bytes(), "beta alpha", 50);
        // "alpha" appears first in the text; the snippet must center on it.
        assert!(snippet.contains("alpha"));
        assert_eq!(snippet, "zzz alpha zzz beta");
    }

    #[test]
    fn validate_query_rejects_empty_and_oversized_and_trims() {
        assert!(matches!(
            validate_query("   "),
            Err(UnifiedSearchError::InvalidQuery(_))
        ));
        assert!(matches!(
            validate_query(&"x".repeat(1001)),
            Err(UnifiedSearchError::InvalidQuery(_))
        ));
        assert!(validate_query(&"é".repeat(1000)).is_ok());
        assert!(matches!(
            validate_query(&"é".repeat(1001)),
            Err(UnifiedSearchError::InvalidQuery(_))
        ));
        assert_eq!(
            validate_query("  hello world  ").unwrap(),
            "hello world",
            "query is trimmed"
        );
    }

    #[test]
    fn source_filter_is_bounded_and_rejects_duplicates() {
        let both = vec!["files".to_string(), "chat".to_string()];
        assert_eq!(parse_source_filter(Some(&both)).unwrap().len(), 2);

        let duplicate = vec!["files".to_string(), "files".to_string()];
        assert!(parse_source_filter(Some(&duplicate)).is_err());

        let too_many = vec!["files".to_string(), "chat".to_string(), "files".to_string()];
        assert!(parse_source_filter(Some(&too_many)).is_err());
    }
}
