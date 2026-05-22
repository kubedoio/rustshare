//! Kanban service for file-backed Kanban boards.
//!
//! Boards are folders under /Kanban.
//! Columns are subfolders like 00-Backlog, 01-Ready, etc.
//! Cards are folders inside columns containing index.md and .rustshare-card.json.

use bytes::Bytes;
use chrono::{DateTime, Utc};
use regex::Regex;
use rustshare_core::{
    domain::{File, Folder, UserId},
    services::{FileService, FolderService},
};
use rustshare_storage::{EventStore, MetadataStore, ObjectStore};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use rustshare_infrastructure::repositories::{PermissionResolverRepository, UserRepository};

// ============================================================================
// Errors
// ============================================================================

#[derive(Debug, thiserror::Error)]
pub enum KanbanError {
    #[error("Board not found")]
    BoardNotFound,
    #[error("Card not found")]
    CardNotFound,
    #[error("Column not found: {0}")]
    ColumnNotFound(String),
    #[error("Permission denied")]
    PermissionDenied,
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("Database error: {0}")]
    Database(String),
    #[error("Invalid name: {0}")]
    InvalidName(String),
    #[error("Invalid data: {0}")]
    InvalidData(String),
    #[error("Not found: {0}")]
    NotFound(String),
}

impl From<rustshare_core::services::FileError> for KanbanError {
    fn from(e: rustshare_core::services::FileError) -> Self {
        match e {
            rustshare_core::services::FileError::NotFound(_) => KanbanError::CardNotFound,
            rustshare_core::services::FileError::PermissionDenied { .. } => {
                KanbanError::PermissionDenied
            }
            rustshare_core::services::FileError::InvalidName(s) => KanbanError::InvalidName(s),
            rustshare_core::services::FileError::Database(s) => KanbanError::Database(s),
            _ => KanbanError::Storage(e.to_string()),
        }
    }
}

impl From<rustshare_core::services::FolderError> for KanbanError {
    fn from(e: rustshare_core::services::FolderError) -> Self {
        match e {
            rustshare_core::services::FolderError::NotFound(_) => KanbanError::BoardNotFound,
            rustshare_core::services::FolderError::PermissionDenied { .. } => {
                KanbanError::PermissionDenied
            }
            rustshare_core::services::FolderError::InvalidName(s) => KanbanError::InvalidName(s),
            rustshare_core::services::FolderError::Database(s) => KanbanError::Database(s),
            _ => KanbanError::Storage(e.to_string()),
        }
    }
}

impl From<sqlx::Error> for KanbanError {
    fn from(e: sqlx::Error) -> Self {
        KanbanError::Database(e.to_string())
    }
}

impl From<serde_json::Error> for KanbanError {
    fn from(e: serde_json::Error) -> Self {
        KanbanError::InvalidData(e.to_string())
    }
}

// ============================================================================
// Public types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KanbanLabel {
    pub id: String,
    pub name: String,
    pub color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KanbanAssignee {
    pub id: String,
    pub display_name: String,
    pub initials: String,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KanbanChecklist {
    pub done: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KanbanChecklistItem {
    pub id: String,
    pub text: String,
    pub done: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KanbanChecklistGroup {
    pub id: String,
    pub title: String,
    pub items: Vec<KanbanChecklistItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KanbanSettings {
    pub show_description_on_cards: bool,
    pub description_preview_lines: usize,
    pub show_assignees: bool,
    pub show_labels: bool,
    pub show_due_date: bool,
    pub show_attachment_badge: bool,
    pub show_checklist_badge: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KanbanBoardSummary {
    pub id: String,
    pub title: String,
    pub slug: String,
    pub path: String,
    pub column_count: usize,
    pub card_count: usize,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub archived: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KanbanBoard {
    pub id: String,
    pub title: String,
    pub slug: String,
    pub path: String,
    pub columns: Vec<KanbanColumn>,
    pub labels: Vec<KanbanLabel>,
    pub settings: KanbanSettings,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub archived: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KanbanColumn {
    pub id: String,
    pub title: String,
    pub slug: String,
    pub order: i32,
    pub status: String,
    pub wip_limit: Option<usize>,
    pub cards: Vec<KanbanCard>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KanbanCard {
    pub id: String,
    pub title: String,
    pub slug: String,
    pub content: String,
    pub description_preview: String,
    pub column_id: String,
    pub status: String,
    pub order: i32,
    pub labels: Vec<KanbanLabel>,
    pub assignees: Vec<KanbanAssignee>,
    pub due_date: Option<DateTime<Utc>>,
    pub priority: String,
    pub attachments_count: usize,
    pub checklist: KanbanChecklist,
    pub checklists: Vec<KanbanChecklistGroup>,
    pub archived: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub path: String,
    pub schema_version: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KanbanCardAttachment {
    pub id: String,
    pub name: String,
    pub size: i64,
    pub mime_type: String,
    pub created_at: DateTime<Utc>,
    pub created_by: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KanbanCardDetail {
    #[serde(flatten)]
    pub summary: KanbanCard,
    pub attachments: Vec<KanbanCardAttachment>,
    pub checklists: Vec<KanbanChecklistGroup>,
    pub activity: Vec<KanbanEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateBoardInput {
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateLabelInput {
    pub name: String,
    pub color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateLabelInput {
    pub name: Option<String>,
    pub color: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateBoardInput {
    pub title: Option<String>,
    pub labels: Option<Vec<KanbanLabel>>,
    pub settings: Option<KanbanSettings>,
    pub archived: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateCardInput {
    pub title: String,
    pub column_id: Option<String>,
    pub content: Option<String>,
    pub priority: Option<String>,
    pub labels: Option<Vec<String>>,
    pub assignees: Option<Vec<String>>,
    pub due_date: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCardInput {
    pub title: Option<String>,
    pub content: Option<String>,
    pub priority: Option<String>,
    pub labels: Option<Vec<String>>,
    pub assignees: Option<Vec<String>>,
    pub due_date: Option<DateTime<Utc>>,
    pub archived: Option<bool>,
    pub checklists: Option<Vec<KanbanChecklistGroup>>,
    pub activity: Option<Vec<KanbanEvent>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoveCardInput {
    pub board_id: String,
    pub target_column_id: String,
    pub target_order: Option<i32>,
    pub before_card_id: Option<String>,
    pub after_card_id: Option<String>,
}

// ============================================================================
// Internal metadata types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct BoardMetadata {
    pub id: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub title: String,
    pub slug: String,
    pub module: String,
    pub schema_version: String,
    pub columns: Vec<ColumnDef>,
    pub labels: Vec<KanbanLabel>,
    pub settings: KanbanSettings,
    pub archived: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ColumnDef {
    pub id: String,
    pub title: String,
    pub slug: String,
    pub order: i32,
    pub status: String,
    pub wip_limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ColumnMetadata {
    pub id: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub title: String,
    pub slug: String,
    pub order: i32,
    pub status: String,
    pub board_id: String,
    pub wip_limit: Option<usize>,
    pub schema_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CardMetadata {
    pub id: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub board_id: String,
    pub column_id: String,
    pub title: String,
    pub slug: String,
    pub status: String,
    pub order: i32,
    pub assignees: Vec<String>,
    pub labels: Vec<String>,
    pub due_date: Option<DateTime<Utc>>,
    pub priority: String,
    pub attachments_count: usize,
    pub checklist_done: usize,
    pub checklist_total: usize,
    pub checklists: Vec<KanbanChecklistGroup>,
    pub archived: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub description_preview: Option<String>,
    pub schema_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attachments: Option<Vec<KanbanCardAttachment>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activity: Option<Vec<KanbanEvent>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KanbanEvent {
    pub event_type: String,
    pub timestamp: DateTime<Utc>,
    pub actor: String,
    pub payload: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

// ============================================================================
// Markdown card format
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CardMarkdownFrontmatter {
    pub id: String,
    pub title: String,
    pub board: String,
    pub column: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slug: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<KanbanLabel>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignees: Option<Vec<KanbanAssignee>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_date: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachments: Option<Vec<KanbanCardAttachment>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checklists: Option<Vec<KanbanChecklistGroup>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activity: Option<Vec<KanbanEvent>>,
}

impl CardMarkdownFrontmatter {
    fn from_metadata(meta: &CardMetadata) -> Self {
        Self {
            id: meta.id.clone(),
            title: meta.title.clone(),
            board: meta.board_id.clone(),
            column: meta.column_id.clone(),
            slug: Some(meta.slug.clone()),
            status: Some(meta.status.clone()),
            priority: if meta.priority.is_empty() {
                None
            } else {
                Some(meta.priority.clone())
            },
            labels: None,
            assignees: None,
            position: Some(meta.order),
            due_date: meta.due_date,
            created_at: Some(meta.created_at),
            updated_at: Some(meta.updated_at),
            archived: Some(meta.archived),
            attachments: meta.attachments.clone(),
            checklists: if meta.checklists.is_empty() {
                None
            } else {
                Some(meta.checklists.clone())
            },
            activity: meta.activity.clone(),
        }
    }

    fn into_metadata(self) -> CardMetadata {
        let slug = self.slug.unwrap_or_else(|| slugify(&self.title));
        let status = self.status.unwrap_or_else(|| "unknown".to_string());
        CardMetadata {
            id: self.id,
            type_: "kanban.card".to_string(),
            board_id: self.board,
            column_id: self.column,
            title: self.title,
            slug,
            status,
            order: self.position.unwrap_or(0),
            assignees: self
                .assignees
                .map(|a| a.into_iter().map(|x| x.id).collect())
                .unwrap_or_default(),
            labels: self
                .labels
                .map(|l| l.into_iter().map(|x| x.id).collect())
                .unwrap_or_default(),
            due_date: self.due_date,
            priority: self.priority.unwrap_or_else(|| "medium".to_string()),
            attachments_count: self.attachments.as_ref().map(|a| a.len()).unwrap_or(0),
            checklist_done: 0,
            checklist_total: 0,
            checklists: self.checklists.unwrap_or_default(),
            archived: self.archived.unwrap_or(false),
            created_at: self.created_at.unwrap_or_else(Utc::now),
            updated_at: self.updated_at.unwrap_or_else(Utc::now),
            description_preview: None,
            schema_version: "2.0".to_string(),
            attachments: self.attachments,
            activity: self.activity,
        }
    }
}

fn parse_card_markdown(
    content: &str,
    card_id: &str,
) -> Result<(CardMetadata, String), KanbanError> {
    if !content.trim_start().starts_with("---") {
        return Err(KanbanError::InvalidData(
            "Missing YAML frontmatter".to_string(),
        ));
    }

    let after_open = &content.trim_start()[3..];
    let Some(end_idx) = after_open.find("---") else {
        return Err(KanbanError::InvalidData(
            "Unclosed YAML frontmatter".to_string(),
        ));
    };

    let yaml_str = &after_open[..end_idx];
    let body = after_open[end_idx + 3..].trim_start();

    let frontmatter: CardMarkdownFrontmatter = serde_yaml::from_str(yaml_str)
        .map_err(|e| KanbanError::InvalidData(format!("Invalid YAML frontmatter: {e}")))?;

    if frontmatter.id != card_id {
        return Err(KanbanError::InvalidData(format!(
            "Card ID mismatch: expected {}, got {}",
            card_id, frontmatter.id
        )));
    }

    let description = extract_description_from_markdown_body(body);

    let mut meta = frontmatter.into_metadata();
    meta.description_preview = Some(derive_preview(&description, &meta.title));
    Ok((meta, description))
}

fn extract_description_from_markdown_body(body: &str) -> String {
    let body = body.trim();
    if let Some(pos) = body.find("## Description") {
        let after = &body[pos + 14..];
        let after = after.trim_start();
        // Find the next ## heading
        if let Some(next_pos) = after.find("\n## ") {
            return after[..next_pos].trim().to_string();
        }
        return after.trim().to_string();
    }
    body.to_string()
}

// ============================================================================
// Service
// ============================================================================

pub struct KanbanService {
    pub file_service:
        Arc<FileService<EventStore, MetadataStore, ObjectStore, PermissionResolverRepository>>,
    pub folder_service: Arc<FolderService<EventStore, MetadataStore, PermissionResolverRepository>>,
    pub metadata_store: Arc<MetadataStore>,
    pub object_store: Arc<ObjectStore>,
    pub user_repository: Arc<UserRepository>,
}

impl KanbanService {
    pub fn new(
        file_service: Arc<
            FileService<EventStore, MetadataStore, ObjectStore, PermissionResolverRepository>,
        >,
        folder_service: Arc<FolderService<EventStore, MetadataStore, PermissionResolverRepository>>,
        metadata_store: Arc<MetadataStore>,
        object_store: Arc<ObjectStore>,
        user_repository: Arc<UserRepository>,
    ) -> Self {
        Self {
            file_service,
            folder_service,
            metadata_store,
            object_store,
            user_repository,
        }
    }

    pub async fn get_assignable_users(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<KanbanAssignee>, KanbanError> {
        let users = self
            .user_repository
            .list_by_tenant(tenant_id)
            .await
            .map_err(|e| KanbanError::Database(e.to_string()))?;

        Ok(users
            .into_iter()
            .map(|u| {
                let display_name = if u.display_name.is_empty() {
                    u.username.clone()
                } else {
                    u.display_name
                };
                KanbanAssignee {
                    id: u.id.to_string(),
                    display_name: display_name.clone(),
                    avatar_url: u.avatar_path,
                    initials: get_initials(&display_name),
                }
            })
            .collect())
    }

    pub async fn add_card_label(
        &self,
        card_id: Uuid,
        label_id: String,
        user_id: UserId,
    ) -> Result<(), KanbanError> {
        let card_folder = self
            .folder_service
            .get_folder(card_id, user_id)
            .await
            .map_err(KanbanError::from)?;
        let mut meta = self.load_card_metadata(&card_folder, user_id).await?;

        if meta.labels.contains(&label_id) {
            return Ok(());
        }

        meta.labels.push(label_id.clone());
        meta.updated_at = Utc::now();

        self.write_card_metadata(&card_folder, &meta, user_id, card_folder.tenant_id)
            .await?;

        self.append_kanban_event(
            &card_folder,
            KanbanEvent {
                event_type: "card.label.added".to_string(),
                timestamp: Utc::now(),
                actor: user_id.to_string(),
                payload: serde_json::json!({ "labelId": label_id }),
                id: None,
                text: None,
            },
            user_id,
        )
        .await?;

        Ok(())
    }

    pub async fn remove_card_label(
        &self,
        card_id: Uuid,
        label_id: String,
        user_id: UserId,
    ) -> Result<(), KanbanError> {
        let card_folder = self
            .folder_service
            .get_folder(card_id, user_id)
            .await
            .map_err(KanbanError::from)?;
        let mut meta = self.load_card_metadata(&card_folder, user_id).await?;

        let initial_len = meta.labels.len();
        meta.labels.retain(|l| l != &label_id);

        if meta.labels.len() == initial_len {
            return Ok(());
        }

        meta.updated_at = Utc::now();
        self.write_card_metadata(&card_folder, &meta, user_id, card_folder.tenant_id)
            .await?;

        self.append_kanban_event(
            &card_folder,
            KanbanEvent {
                event_type: "card.label.removed".to_string(),
                timestamp: Utc::now(),
                actor: user_id.to_string(),
                payload: serde_json::json!({ "labelId": label_id }),
                id: None,
                text: None,
            },
            user_id,
        )
        .await?;

        Ok(())
    }

    pub async fn assign_card_member(
        &self,
        card_id: Uuid,
        assignee_id: String,
        user_id: UserId,
    ) -> Result<(), KanbanError> {
        let card_folder = self
            .folder_service
            .get_folder(card_id, user_id)
            .await
            .map_err(KanbanError::from)?;
        let mut meta = self.load_card_metadata(&card_folder, user_id).await?;

        if meta.assignees.contains(&assignee_id) {
            return Ok(());
        }

        meta.assignees.push(assignee_id.clone());
        meta.updated_at = Utc::now();

        self.write_card_metadata(&card_folder, &meta, user_id, card_folder.tenant_id)
            .await?;

        self.append_kanban_event(
            &card_folder,
            KanbanEvent {
                event_type: "card.assignee.added".to_string(),
                timestamp: Utc::now(),
                actor: user_id.to_string(),
                payload: serde_json::json!({ "assigneeId": assignee_id }),
                id: None,
                text: None,
            },
            user_id,
        )
        .await?;

        Ok(())
    }

    pub async fn unassign_card_member(
        &self,
        card_id: Uuid,
        assignee_id: String,
        user_id: UserId,
    ) -> Result<(), KanbanError> {
        let card_folder = self
            .folder_service
            .get_folder(card_id, user_id)
            .await
            .map_err(KanbanError::from)?;
        let mut meta = self.load_card_metadata(&card_folder, user_id).await?;

        let initial_len = meta.assignees.len();
        meta.assignees.retain(|a| a != &assignee_id);

        if meta.assignees.len() == initial_len {
            return Ok(());
        }

        meta.updated_at = Utc::now();
        self.write_card_metadata(&card_folder, &meta, user_id, card_folder.tenant_id)
            .await?;

        self.append_kanban_event(
            &card_folder,
            KanbanEvent {
                event_type: "card.assignee.removed".to_string(),
                timestamp: Utc::now(),
                actor: user_id.to_string(),
                payload: serde_json::json!({ "assigneeId": assignee_id }),
                id: None,
                text: None,
            },
            user_id,
        )
        .await?;

        Ok(())
    }

    // -------------------------------------------------------------------------
    // Boards
    // -------------------------------------------------------------------------

    pub async fn list_boards(
        &self,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<Vec<KanbanBoardSummary>, KanbanError> {
        let root = match self.find_kanban_root(user_id, tenant_id).await? {
            Some(r) => r,
            None => return Ok(Vec::new()),
        };

        let contents = self
            .folder_service
            .list_contents(root.id, user_id)
            .await
            .map_err(KanbanError::from)?;

        let mut boards = Vec::new();
        for folder in contents.folders {
            let meta = self.load_board_metadata(&folder, user_id).await?;
            let column_count = meta.columns.len();
            let mut card_count = 0usize;
            for col in &meta.columns {
                let col_path = format!("{}/{}", folder.path.trim_end_matches('/'), col.slug);
                if let Some(col_folder) = self
                    .metadata_store
                    .find_folder_by_path(&col_path, folder.owner_id)
                    .await
                    .map_err(|e| KanbanError::Database(e.to_string()))?
                {
                    let col_contents = self
                        .folder_service
                        .list_contents(col_folder.id, user_id)
                        .await
                        .map_err(KanbanError::from)?;
                    card_count += col_contents
                        .folders
                        .into_iter()
                        .filter(|f| f.name.starts_with("CARD-"))
                        .count();
                }
            }
            boards.push(KanbanBoardSummary {
                id: folder.id.to_string(),
                title: meta.title,
                slug: meta.slug,
                path: folder.path.clone(),
                column_count,
                card_count,
                created_at: folder.created_at,
                updated_at: folder.updated_at,
                archived: meta.archived,
            });
        }

        boards.sort_by_key(|a| a.title.to_lowercase());
        Ok(boards)
    }

    async fn unique_board_folder_name(
        &self,
        base_name: &str,
        parent_id: Uuid,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<String, KanbanError> {
        let existing = self
            .metadata_store
            .list_folders(Some(parent_id), user_id, tenant_id)
            .await
            .map_err(|e| KanbanError::Database(e.to_string()))?;

        if !existing.iter().any(|f| f.name == base_name) {
            return Ok(base_name.to_string());
        }

        for i in 2..=1000 {
            let candidate = format!("{}-{}", base_name, i);
            if !existing.iter().any(|f| f.name == candidate) {
                return Ok(candidate);
            }
        }

        Err(KanbanError::InvalidName(format!(
            "Could not find unique name for board '{}'",
            base_name
        )))
    }

    pub async fn create_board(
        &self,
        input: CreateBoardInput,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<KanbanBoard, KanbanError> {
        let root = self.ensure_kanban_root(user_id, tenant_id).await?;
        let slug = slugify(&input.title);
        Self::validate_board_slug(&slug)?;
        let name = if slug.is_empty() {
            "untitled-board".to_string()
        } else {
            slug.clone()
        };

        let name = self
            .unique_board_folder_name(&name, root.id, user_id, tenant_id)
            .await?;

        // Ensure unique name under root
        let board_folder = self
            .folder_service
            .create_folder(name.clone(), Some(root.id), user_id, tenant_id)
            .await
            .map_err(|e| match e {
                rustshare_core::services::FolderError::DuplicateName { .. } => {
                    KanbanError::InvalidName(format!("Board '{}' already exists", name))
                }
                _ => KanbanError::from(e),
            })?;

        // Create standard columns
        let columns = standard_columns();
        for col in &columns {
            self.folder_service
                .create_folder(col.slug.clone(), Some(board_folder.id), user_id, tenant_id)
                .await
                .map_err(KanbanError::from)?;
        }

        let board_meta = BoardMetadata {
            id: board_folder.id.to_string(),
            type_: "kanban.board".to_string(),
            title: input.title.clone(),
            slug: slug.clone(),
            module: "kanban".to_string(),
            schema_version: "1.0".to_string(),
            columns: columns.clone(),
            labels: default_labels(),
            settings: default_settings(),
            archived: false,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        self.write_board_metadata(&board_folder, &board_meta, user_id, tenant_id)
            .await?;
        self.ensure_events_file(&board_folder, user_id, tenant_id)
            .await?;

        // Write column metadata
        for col in &columns {
            let col_path = format!("{}/{}", board_folder.path.trim_end_matches('/'), col.slug);
            let col_folder = self
                .metadata_store
                .find_folder_by_path(&col_path, board_folder.owner_id)
                .await
                .map_err(|e| KanbanError::Database(e.to_string()))?
                .ok_or_else(|| KanbanError::ColumnNotFound(col.slug.clone()))?;
            let col_meta = ColumnMetadata {
                id: col.id.clone(),
                type_: "kanban.column".to_string(),
                title: col.title.clone(),
                slug: col.slug.clone(),
                order: col.order,
                status: col.status.clone(),
                board_id: board_folder.id.to_string(),
                wip_limit: col.wip_limit,
                schema_version: "1.0".to_string(),
            };
            self.write_column_metadata(&col_folder, &col_meta, user_id, tenant_id)
                .await?;
        }

        self.get_board(board_folder.id.to_string(), user_id, tenant_id)
            .await
    }

    pub async fn get_board(
        &self,
        board_id_or_slug: String,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<KanbanBoard, KanbanError> {
        let board_id = if let Ok(id) = Uuid::parse_str(&board_id_or_slug) {
            id
        } else {
            // Try to find by slug
            let folder = self
                .find_board_by_slug(&board_id_or_slug, user_id, tenant_id)
                .await?
                .ok_or(KanbanError::BoardNotFound)?;
            folder.id
        };

        let board_folder = self
            .folder_service
            .get_folder(board_id, user_id)
            .await
            .map_err(KanbanError::from)?;

        let mut board_meta = self.load_board_metadata(&board_folder, user_id).await?;
        self.ensure_standard_board_columns(&board_folder, &mut board_meta, user_id, tenant_id)
            .await?;

        let mut columns = Vec::new();
        for col_def in &board_meta.columns {
            let col_path = format!(
                "{}/{}",
                board_folder.path.trim_end_matches('/'),
                col_def.slug
            );
            let col_folder = match self
                .metadata_store
                .find_folder_by_path(&col_path, board_folder.owner_id)
                .await
                .map_err(|e| KanbanError::Database(e.to_string()))?
            {
                Some(f) => f,
                None => continue,
            };

            let col_meta = self
                .load_column_metadata(&col_folder, &board_folder.id.to_string(), user_id)
                .await?;

            let col_contents = self
                .folder_service
                .list_contents(col_folder.id, user_id)
                .await
                .map_err(KanbanError::from)?;

            let mut cards = Vec::new();
            for card_folder in col_contents.folders {
                if !card_folder.name.starts_with("CARD-") {
                    continue;
                }
                if let Ok(card) = self.load_card(&card_folder, user_id).await {
                    if !card.archived {
                        cards.push(card);
                    }
                }
            }

            // Sort by order
            cards.sort_by_key(|c| c.order);

            columns.push(KanbanColumn {
                id: col_meta.id,
                title: col_meta.title,
                slug: col_meta.slug,
                order: col_meta.order,
                status: col_meta.status,
                wip_limit: col_meta.wip_limit,
                cards,
            });
        }

        columns.sort_by_key(|c| c.order);

        Ok(KanbanBoard {
            id: board_folder.id.to_string(),
            title: board_meta.title,
            slug: board_meta.slug,
            path: board_folder.path,
            columns,
            labels: board_meta.labels,
            settings: board_meta.settings,
            created_at: board_folder.created_at,
            updated_at: board_folder.updated_at,
            archived: board_meta.archived,
        })
    }

    pub async fn update_board(
        &self,
        board_id_or_slug: String,
        input: UpdateBoardInput,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<KanbanBoard, KanbanError> {
        let board_id = self
            .ensure_board_id(&board_id_or_slug, user_id, tenant_id)
            .await?;

        let board_folder = self
            .folder_service
            .get_folder(board_id, user_id)
            .await
            .map_err(KanbanError::from)?;

        let mut board_meta = self.load_board_metadata(&board_folder, user_id).await?;

        if let Some(title) = input.title {
            board_meta.title = title;
            board_meta.slug = slugify(&board_meta.title);
            board_meta.updated_at = Utc::now();
        }
        if let Some(labels) = input.labels {
            board_meta.labels = labels;
            board_meta.updated_at = Utc::now();
        }
        if let Some(settings) = input.settings {
            board_meta.settings = settings;
            board_meta.updated_at = Utc::now();
        }
        if let Some(archived) = input.archived {
            board_meta.archived = archived;
            board_meta.updated_at = Utc::now();
        }

        self.write_board_metadata(&board_folder, &board_meta, user_id, tenant_id)
            .await?;

        self.get_board(board_id.to_string(), user_id, tenant_id)
            .await
    }

    pub async fn create_label(
        &self,
        board_id: Uuid,
        input: CreateLabelInput,
        user_id: UserId,
    ) -> Result<KanbanLabel, KanbanError> {
        self.validate_color(&input.color)?;

        let board_folder = self
            .folder_service
            .get_folder(board_id, user_id)
            .await
            .map_err(KanbanError::from)?;

        let mut meta = self.load_board_metadata(&board_folder, user_id).await?;

        let label = KanbanLabel {
            id: format!("label_{}", &Uuid::new_v4().to_string()[..8]),
            name: input.name,
            color: input.color,
        };

        meta.labels.push(label.clone());
        meta.updated_at = Utc::now();

        self.write_board_metadata(&board_folder, &meta, user_id, board_folder.tenant_id)
            .await?;

        Ok(label)
    }

    pub async fn update_label(
        &self,
        board_id: Uuid,
        label_id: String,
        input: UpdateLabelInput,
        user_id: UserId,
    ) -> Result<KanbanLabel, KanbanError> {
        if let Some(ref color) = input.color {
            self.validate_color(color)?;
        }

        let board_folder = self
            .folder_service
            .get_folder(board_id, user_id)
            .await
            .map_err(KanbanError::from)?;

        let mut meta = self.load_board_metadata(&board_folder, user_id).await?;

        let mut found = false;
        let mut updated_label = None;
        for label in &mut meta.labels {
            if label.id == label_id {
                if let Some(ref name) = input.name {
                    label.name = name.clone();
                }
                if let Some(ref color) = input.color {
                    label.color = color.clone();
                }
                updated_label = Some(label.clone());
                found = true;
                break;
            }
        }

        if !found {
            return Err(KanbanError::NotFound(format!(
                "Label {} not found",
                label_id
            )));
        }

        meta.updated_at = Utc::now();
        self.write_board_metadata(&board_folder, &meta, user_id, board_folder.tenant_id)
            .await?;

        Ok(updated_label.unwrap())
    }

    pub async fn delete_label(
        &self,
        board_id: Uuid,
        label_id: String,
        user_id: UserId,
    ) -> Result<(), KanbanError> {
        let board_folder = self
            .folder_service
            .get_folder(board_id, user_id)
            .await
            .map_err(KanbanError::from)?;

        let mut meta = self.load_board_metadata(&board_folder, user_id).await?;

        let initial_len = meta.labels.len();
        meta.labels.retain(|l| l.id != label_id);

        if meta.labels.len() == initial_len {
            return Err(KanbanError::NotFound(format!(
                "Label {} not found",
                label_id
            )));
        }

        meta.updated_at = Utc::now();
        self.write_board_metadata(&board_folder, &meta, user_id, board_folder.tenant_id)
            .await?;

        Ok(())
    }

    fn validate_color(&self, color: &str) -> Result<(), KanbanError> {
        Self::validate_color_static(color)
    }

    pub fn validate_color_static(color: &str) -> Result<(), KanbanError> {
        let approved = ["green", "yellow", "orange", "red", "purple", "blue", "gray"];
        if !approved.contains(&color) {
            return Err(KanbanError::InvalidData(format!(
                "Invalid color: {}. Approved colors: {:?}",
                color, approved
            )));
        }
        Ok(())
    }

    /// Validate a board or card slug to prevent path traversal and injection.
    pub fn validate_slug(slug: &str) -> Result<(), KanbanError> {
        if slug.is_empty() {
            return Err(KanbanError::InvalidName("Slug cannot be empty".to_string()));
        }
        // Reject absolute paths
        if slug.starts_with('/') || slug.starts_with('\\') {
            return Err(KanbanError::InvalidName(
                "Absolute paths are not allowed".to_string(),
            ));
        }
        // Reject traversal patterns (including encoded)
        if slug.contains("..") || slug.contains("%2e%2e") || slug.contains("%2E%2E") {
            return Err(KanbanError::InvalidName(
                "Path traversal is not allowed".to_string(),
            ));
        }
        // Reject null bytes
        if slug.contains('\0') {
            return Err(KanbanError::InvalidName(
                "Null bytes are not allowed".to_string(),
            ));
        }
        // Reject control characters
        if slug.chars().any(|c| c.is_control()) {
            return Err(KanbanError::InvalidName(
                "Control characters are not allowed".to_string(),
            ));
        }
        Ok(())
    }

    /// Validate a board slug specifically.
    pub fn validate_board_slug(slug: &str) -> Result<(), KanbanError> {
        Self::validate_slug(slug)?;
        // Board slugs must be reasonable length
        if slug.len() > 120 {
            return Err(KanbanError::InvalidName("Board slug too long".to_string()));
        }
        Ok(())
    }

    /// Validate a card slug specifically.
    pub fn validate_card_slug(slug: &str) -> Result<(), KanbanError> {
        Self::validate_slug(slug)?;
        // Card slugs must start with CARD- prefix as created by the system
        if !slug.starts_with("CARD-") {
            return Err(KanbanError::InvalidName(
                "Invalid card slug format".to_string(),
            ));
        }
        if slug.len() > 200 {
            return Err(KanbanError::InvalidName("Card slug too long".to_string()));
        }
        Ok(())
    }

    /// Sanitize and validate an attachment filename.
    pub fn sanitize_attachment_name(name: &str) -> Result<String, KanbanError> {
        if name.is_empty() || name.len() > 255 {
            return Err(KanbanError::InvalidName(
                "Invalid attachment name length".to_string(),
            ));
        }
        // Reject path separators and traversal
        if name.contains('/') || name.contains('\\') || name.contains("..") || name.contains('\0') {
            return Err(KanbanError::InvalidName(
                "Invalid attachment name".to_string(),
            ));
        }
        // Reject names that are just dots
        if name == "." || name == ".." {
            return Err(KanbanError::InvalidName(
                "Invalid attachment name".to_string(),
            ));
        }
        // Reject hidden metadata filenames (before stripping dots)
        let trimmed = name.trim();
        if trimmed == ".rustshare-board.json"
            || trimmed == ".rustshare-column.json"
            || trimmed == ".rustshare-card.json"
            || trimmed == "events.jsonl"
            || trimmed == "index.md"
        {
            return Err(KanbanError::InvalidName("Reserved filename".to_string()));
        }
        // Strip leading/trailing whitespace and dots
        let sanitized = trimmed
            .trim_start_matches('.')
            .trim_end_matches('.')
            .to_string();
        if sanitized.is_empty() {
            return Err(KanbanError::InvalidName(
                "Invalid attachment name".to_string(),
            ));
        }
        // Reject hidden metadata filenames again after sanitization
        if sanitized == "rustshare-board.json"
            || sanitized == "rustshare-column.json"
            || sanitized == "rustshare-card.json"
        {
            return Err(KanbanError::InvalidName("Reserved filename".to_string()));
        }
        Ok(sanitized)
    }

    pub async fn archive_board(
        &self,
        board_id_or_slug: String,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<(), KanbanError> {
        let board_id = self
            .ensure_board_id(&board_id_or_slug, user_id, tenant_id)
            .await?;
        let board_folder = self
            .folder_service
            .get_folder(board_id, user_id)
            .await
            .map_err(KanbanError::from)?;

        // Archive by renaming with an archive prefix
        let archive_name = format!("ARCHIVED-{}", board_folder.name);
        self.folder_service
            .rename_folder(board_folder.id, archive_name, user_id)
            .await
            .map_err(KanbanError::from)?;

        Ok(())
    }

    // -------------------------------------------------------------------------
    // Cards
    // -------------------------------------------------------------------------

    pub async fn list_cards(
        &self,
        board_id_or_slug: String,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<Vec<KanbanCard>, KanbanError> {
        let board = self.get_board(board_id_or_slug, user_id, tenant_id).await?;
        let mut cards = Vec::new();
        for col in board.columns {
            cards.extend(col.cards);
        }
        Ok(cards)
    }

    pub async fn create_card(
        &self,
        board_id_or_slug: String,
        input: CreateCardInput,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<KanbanCard, KanbanError> {
        let board_id = self
            .ensure_board_id(&board_id_or_slug, user_id, tenant_id)
            .await?;
        let column_id = input
            .column_id
            .clone()
            .ok_or_else(|| KanbanError::InvalidData("column_id is required".to_string()))?;
        let board_folder = self
            .folder_service
            .get_folder(board_id, user_id)
            .await
            .map_err(KanbanError::from)?;

        let board_meta = self.load_board_metadata(&board_folder, user_id).await?;
        let col_def = board_meta
            .columns
            .iter()
            .find(|c| c.id == column_id)
            .ok_or_else(|| KanbanError::ColumnNotFound(column_id.clone()))?;

        let col_path = format!(
            "{}/{}",
            board_folder.path.trim_end_matches('/'),
            col_def.slug
        );
        let col_folder = self
            .metadata_store
            .find_folder_by_path(&col_path, board_folder.owner_id)
            .await
            .map_err(|e| KanbanError::Database(e.to_string()))?
            .ok_or_else(|| KanbanError::ColumnNotFound(column_id.clone()))?;

        let seq = self.next_card_sequence(col_folder.id, user_id).await?;
        let card_slug = slugify(&input.title);
        let card_name = format!("CARD-{:04}-{}", seq, card_slug);
        Self::validate_card_slug(&card_name)?;
        let order = (seq as i32).saturating_mul(1000);

        let card_folder = self
            .folder_service
            .create_folder(card_name.clone(), Some(col_folder.id), user_id, tenant_id)
            .await
            .map_err(KanbanError::from)?;

        let content = input.content.unwrap_or_default();
        let preview = derive_preview(&content, &input.title);
        let card_meta = CardMetadata {
            id: card_folder.id.to_string(),
            type_: "kanban.card".to_string(),
            board_id: board_folder.id.to_string(),
            column_id: col_def.id.clone(),
            title: input.title.clone(),
            slug: format!("CARD-{:04}-{}", seq, card_slug),
            status: col_def.status.clone(),
            order,
            assignees: input.assignees.unwrap_or_default(),
            labels: input.labels.unwrap_or_default(),
            due_date: input.due_date,
            priority: input.priority.unwrap_or_else(|| "medium".to_string()),
            attachments_count: 0,
            checklist_done: 0,
            checklist_total: 0,
            checklists: vec![],
            archived: false,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            description_preview: Some(preview),
            schema_version: "1.0".to_string(),
            attachments: None,
            activity: None,
        };

        // Create attachments folder
        self.folder_service
            .create_folder(
                "attachments".to_string(),
                Some(card_folder.id),
                user_id,
                tenant_id,
            )
            .await
            .map_err(KanbanError::from)?;

        self.write_card_markdown(&card_folder, &card_meta, &content, user_id, tenant_id)
            .await?;
        self.ensure_events_file(&card_folder, user_id, tenant_id)
            .await?;

        self.append_kanban_event(
            &card_folder,
            KanbanEvent {
                event_type: "card.created".to_string(),
                timestamp: Utc::now(),
                actor: user_id.to_string(),
                payload: serde_json::json!({
                    "columnId": col_def.id,
                    "title": input.title,
                }),
                id: None,
                text: None,
            },
            user_id,
        )
        .await?;

        Ok(KanbanCard {
            id: card_meta.id,
            title: card_meta.title,
            slug: card_meta.slug,
            content,
            description_preview: card_meta.description_preview.clone().unwrap_or_default(),
            column_id: card_meta.column_id,
            status: card_meta.status,
            order: card_meta.order,
            labels: self.map_labels(&card_meta.labels, &board_meta.labels),
            assignees: self.map_assignees(&card_meta.assignees).await,
            due_date: card_meta.due_date,
            priority: card_meta.priority,
            attachments_count: card_meta.attachments_count,
            checklist: KanbanChecklist {
                done: card_meta.checklist_done,
                total: card_meta.checklist_total,
            },
            checklists: vec![],
            archived: card_meta.archived,
            created_at: card_meta.created_at,
            updated_at: card_meta.updated_at,
            path: card_folder.path.clone(),
            schema_version: card_meta.schema_version,
        })
    }

    pub async fn get_card(
        &self,
        card_id: Uuid,
        user_id: UserId,
    ) -> Result<KanbanCard, KanbanError> {
        let card_folder = self
            .folder_service
            .get_folder(card_id, user_id)
            .await
            .map_err(KanbanError::from)?;

        self.load_card(&card_folder, user_id).await
    }

    pub async fn get_card_detail(
        &self,
        card_id: Uuid,
        user_id: UserId,
    ) -> Result<KanbanCardDetail, KanbanError> {
        let card_folder = self
            .folder_service
            .get_folder(card_id, user_id)
            .await
            .map_err(KanbanError::from)?;

        let summary = self.load_card(&card_folder, user_id).await?;

        let md_filename = format!("{}.md", card_folder.id);
        let (frontmatter_attachments, frontmatter_activity) = if let Some(file) = self
            .find_file_in_folder(card_folder.id, user_id, &md_filename)
            .await?
        {
            let bytes = self
                .object_store
                .get(&file.storage_key())
                .await
                .map_err(|e| KanbanError::Storage(e.to_string()))?;
            let content = String::from_utf8_lossy(&bytes).to_string();
            if let Ok((meta, _)) = parse_card_markdown(&content, &card_folder.id.to_string()) {
                (meta.attachments, meta.activity)
            } else {
                (None, None)
            }
        } else {
            (None, None)
        };

        let attachments = match frontmatter_attachments {
            Some(a) if !a.is_empty() => a,
            _ => {
                self.load_card_attachments(&card_folder, user_id, card_folder.tenant_id)
                    .await?
            }
        };

        let activity = match frontmatter_activity {
            Some(a) if !a.is_empty() => a,
            _ => self.load_card_activity(&card_folder, user_id).await?,
        };

        Ok(KanbanCardDetail {
            checklists: summary.checklists.clone(),
            summary,
            attachments,
            activity,
        })
    }

    pub async fn update_card(
        &self,
        card_id: Uuid,
        input: UpdateCardInput,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<KanbanCard, KanbanError> {
        let card_folder = self
            .folder_service
            .get_folder(card_id, user_id)
            .await
            .map_err(KanbanError::from)?;

        let mut card_meta = self.load_card_metadata(&card_folder, user_id).await?;

        if let Some(ref title) = input.title {
            card_meta.title = title.clone();
            card_meta.updated_at = Utc::now();
        }
        if let Some(ref content) = input.content {
            self.write_card_index(&card_folder, content, user_id, tenant_id)
                .await?;
            card_meta.description_preview = Some(derive_preview(content, &card_meta.title));
            card_meta.updated_at = Utc::now();
        } else if input.title.is_some() {
            // Title changed, may need to re-derive to remove heading if it matches new title
            let content = self.load_card_description(&card_folder, user_id).await?;
            card_meta.description_preview = Some(derive_preview(&content, &card_meta.title));
        }
        if let Some(priority) = input.priority {
            card_meta.priority = priority;
            card_meta.updated_at = Utc::now();
        }
        if let Some(labels) = input.labels {
            card_meta.labels = labels;
            card_meta.updated_at = Utc::now();
        }
        if let Some(assignees) = input.assignees {
            card_meta.assignees = assignees;
            card_meta.updated_at = Utc::now();
        }
        if let Some(due_date) = input.due_date {
            card_meta.due_date = Some(due_date);
            card_meta.updated_at = Utc::now();
        }
        if let Some(archived) = input.archived {
            card_meta.archived = archived;
            card_meta.updated_at = Utc::now();
        }
        if let Some(checklists) = input.checklists {
            card_meta.checklists = checklists;
            self.recalculate_checklist_summary(&mut card_meta);
            card_meta.updated_at = Utc::now();
        }
        if let Some(activity) = input.activity {
            card_meta.activity = Some(activity);
            card_meta.updated_at = Utc::now();
        }

        self.write_card_metadata(&card_folder, &card_meta, user_id, tenant_id)
            .await?;

        self.get_card(card_id, user_id).await
    }

    pub async fn update_card_description(
        &self,
        card_id: Uuid,
        content: String,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<KanbanCard, KanbanError> {
        let card_folder = self
            .folder_service
            .get_folder(card_id, user_id)
            .await
            .map_err(KanbanError::from)?;

        let mut card_meta = self.load_card_metadata(&card_folder, user_id).await?;
        self.write_card_index(&card_folder, &content, user_id, tenant_id)
            .await?;
        card_meta.description_preview = Some(derive_preview(&content, &card_meta.title));
        card_meta.updated_at = Utc::now();

        self.write_card_metadata(&card_folder, &card_meta, user_id, tenant_id)
            .await?;

        self.get_card(card_id, user_id).await
    }

    pub async fn move_card(
        &self,
        card_id: Uuid,
        input: MoveCardInput,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<KanbanBoard, KanbanError> {
        let card_folder = self
            .folder_service
            .get_folder(card_id, user_id)
            .await
            .map_err(KanbanError::from)?;

        let (mut card_meta, description) = self.load_card_markdown(&card_folder, user_id).await?;
        let board_id = Uuid::parse_str(&input.board_id)
            .map_err(|_| KanbanError::InvalidData("Invalid board id".to_string()))?;

        let board_folder = self
            .folder_service
            .get_folder(board_id, user_id)
            .await
            .map_err(KanbanError::from)?;

        let board_meta = self.load_board_metadata(&board_folder, user_id).await?;
        let col_def = board_meta
            .columns
            .iter()
            .find(|c| c.id == input.target_column_id)
            .ok_or_else(|| KanbanError::ColumnNotFound(input.target_column_id.clone()))?;

        let col_path = format!(
            "{}/{}",
            board_folder.path.trim_end_matches('/'),
            col_def.slug
        );
        let col_folder = self
            .metadata_store
            .find_folder_by_path(&col_path, board_folder.owner_id)
            .await
            .map_err(|e| KanbanError::Database(e.to_string()))?
            .ok_or_else(|| KanbanError::ColumnNotFound(input.target_column_id.clone()))?;

        let old_column_id = card_meta.column_id.clone();
        let old_order = card_meta.order;

        // Calculate target order
        let target_order = if let (Some(before_id), Some(after_id)) =
            (&input.before_card_id, &input.after_card_id)
        {
            let before_meta = self.load_card_by_id_meta(before_id, user_id).await?;
            let after_meta = self.load_card_by_id_meta(after_id, user_id).await?;
            (before_meta.order + after_meta.order) / 2
        } else if let Some(before_id) = &input.before_card_id {
            let before_meta = self.load_card_by_id_meta(before_id, user_id).await?;
            before_meta.order + 1000
        } else if let Some(after_id) = &input.after_card_id {
            let after_meta = self.load_card_by_id_meta(after_id, user_id).await?;
            after_meta.order / 2
        } else {
            input.target_order.unwrap_or(1000)
        };

        // Move folder if parent changed
        if Some(col_folder.id) != card_folder.parent_folder_id {
            self.folder_service
                .move_folder(card_folder.id, Some(col_folder.id), user_id)
                .await
                .map_err(KanbanError::from)?;
        }

        // Update metadata
        card_meta.column_id = input.target_column_id.clone();
        card_meta.status = col_def.status.clone();
        card_meta.order = target_order;
        card_meta.updated_at = Utc::now();

        // Append activity to frontmatter
        let move_event = KanbanEvent {
            event_type: "card.moved".to_string(),
            timestamp: Utc::now(),
            actor: user_id.to_string(),
            payload: serde_json::json!({
                "fromColumnId": old_column_id,
                "toColumnId": input.target_column_id,
                "oldOrder": old_order,
                "newOrder": target_order,
            }),
            id: Some(format!("act_{}", &Uuid::new_v4().to_string()[..8])),
            text: Some(format!(
                "Moved this card from {} to {}",
                old_column_id, input.target_column_id
            )),
        };
        let mut activity = card_meta.activity.unwrap_or_default();
        activity.push(move_event.clone());
        card_meta.activity = Some(activity);

        self.write_card_markdown(&card_folder, &card_meta, &description, user_id, tenant_id)
            .await?;

        // Rebalance orders if they become too dense in the target column
        self.rebalance_column_if_needed(col_folder.id, user_id, tenant_id)
            .await?;

        // Append events
        self.append_kanban_event(
            &card_folder,
            KanbanEvent {
                event_type: "card.moved".to_string(),
                timestamp: Utc::now(),
                actor: user_id.to_string(),
                payload: serde_json::json!({
                    "fromColumnId": old_column_id,
                    "toColumnId": input.target_column_id,
                    "oldOrder": old_order,
                    "newOrder": target_order,
                }),
                id: None,
                text: None,
            },
            user_id,
        )
        .await?;

        self.append_kanban_event(
            &board_folder,
            KanbanEvent {
                event_type: "board.card_moved".to_string(),
                timestamp: Utc::now(),
                actor: user_id.to_string(),
                payload: serde_json::json!({
                    "cardId": card_id.to_string(),
                    "fromColumnId": old_column_id,
                    "toColumnId": input.target_column_id,
                }),
                id: None,
                text: None,
            },
            user_id,
        )
        .await?;

        self.get_board(input.board_id, user_id, tenant_id).await
    }

    async fn load_card_by_id_meta(
        &self,
        card_id_str: &str,
        user_id: UserId,
    ) -> Result<CardMetadata, KanbanError> {
        let card_id = Uuid::parse_str(card_id_str)
            .map_err(|_| KanbanError::InvalidData("Invalid card id".to_string()))?;
        let folder = self
            .folder_service
            .get_folder(card_id, user_id)
            .await
            .map_err(KanbanError::from)?;
        self.load_card_metadata(&folder, user_id).await
    }

    pub async fn archive_card(
        &self,
        card_id: Uuid,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<KanbanCard, KanbanError> {
        let card_folder = self
            .folder_service
            .get_folder(card_id, user_id)
            .await
            .map_err(KanbanError::from)?;

        let mut card_meta = self.load_card_metadata(&card_folder, user_id).await?;
        card_meta.archived = true;
        card_meta.updated_at = Utc::now();

        self.write_card_metadata(&card_folder, &card_meta, user_id, tenant_id)
            .await?;

        self.append_kanban_event(
            &card_folder,
            KanbanEvent {
                event_type: "card.archived".to_string(),
                timestamp: Utc::now(),
                actor: user_id.to_string(),
                payload: serde_json::json!({}),
                id: None,
                text: None,
            },
            user_id,
        )
        .await?;

        self.load_card(&card_folder, user_id).await
    }

    pub async fn delete_card(
        &self,
        card_id: Uuid,
        user_id: UserId,
        _tenant_id: Uuid,
    ) -> Result<(), KanbanError> {
        self.folder_service
            .delete_folder(card_id, user_id)
            .await
            .map_err(KanbanError::from)?;
        Ok(())
    }

    // -------------------------------------------------------------------------
    // Metadata loading / writing helpers
    // -------------------------------------------------------------------------

    pub async fn load_board_from_files(
        &self,
        board_path: &str,
        user_id: UserId,
    ) -> Result<KanbanBoard, KanbanError> {
        let board_folder = self
            .metadata_store
            .find_folder_by_path(board_path, user_id)
            .await
            .map_err(|e| KanbanError::Database(e.to_string()))?
            .ok_or(KanbanError::BoardNotFound)?;
        self.get_board(board_folder.id.to_string(), user_id, board_folder.tenant_id)
            .await
    }

    pub(crate) async fn write_board_metadata(
        &self,
        board_folder: &Folder,
        metadata: &BoardMetadata,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<(), KanbanError> {
        let content = serde_json::to_string_pretty(metadata)?;
        self.write_file_by_name(
            board_folder.id,
            ".rustshare-board.json",
            &content,
            "application/json",
            user_id,
            tenant_id,
        )
        .await?;
        Ok(())
    }

    pub async fn load_card_from_files(
        &self,
        card_path: &str,
        user_id: UserId,
    ) -> Result<KanbanCard, KanbanError> {
        let card_folder = self
            .metadata_store
            .find_folder_by_path(card_path, user_id)
            .await
            .map_err(|e| KanbanError::Database(e.to_string()))?
            .ok_or(KanbanError::CardNotFound)?;

        let column_folder = if let Some(parent_id) = card_folder.parent_folder_id {
            self.folder_service
                .get_folder(parent_id, user_id)
                .await
                .map_err(KanbanError::from)?
        } else {
            return Err(KanbanError::InvalidData(
                "Card has no parent column".to_string(),
            ));
        };

        let _col_meta = self
            .load_column_metadata(&column_folder, "", user_id)
            .await?;

        self.load_card(&card_folder, user_id).await
    }

    pub(crate) async fn write_card_metadata(
        &self,
        card_folder: &Folder,
        metadata: &CardMetadata,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<(), KanbanError> {
        let description = self.load_card_description(card_folder, user_id).await?;
        self.write_card_markdown(card_folder, metadata, &description, user_id, tenant_id)
            .await
    }

    pub(crate) async fn append_kanban_event(
        &self,
        scope_folder: &Folder,
        event: KanbanEvent,
        user_id: UserId,
    ) -> Result<(), KanbanError> {
        let line = serde_json::to_string(&event)?;
        let file = self
            .find_file_in_folder(scope_folder.id, user_id, "events.jsonl")
            .await?;

        let new_content = if let Some(f) = file {
            let existing = self
                .object_store
                .get(&f.storage_key())
                .await
                .map_err(|e| KanbanError::Storage(e.to_string()))?;
            let mut text = String::from_utf8_lossy(&existing).to_string();
            if !text.is_empty() && !text.ends_with('\n') {
                text.push('\n');
            }
            text.push_str(&line);
            text.push('\n');
            let updated = self
                .file_service
                .update_file(f.id, user_id, f.current_version, Bytes::from(text))
                .await
                .map_err(KanbanError::from)?;
            updated
        } else {
            let created = self
                .file_service
                .upload_file(
                    user_id,
                    "events.jsonl".to_string(),
                    Some(scope_folder.id),
                    Bytes::from(format!("{}\n", line)),
                    "application/jsonlines".to_string(),
                    scope_folder.tenant_id,
                )
                .await
                .map_err(KanbanError::from)?;
            created
        };

        // Avoid unused warning
        let _ = new_content;
        Ok(())
    }

    // -------------------------------------------------------------------------
    // Initialization (called after template creation)
    // -------------------------------------------------------------------------

    pub async fn initialize_board(
        &self,
        board_folder_id: Uuid,
        user_id: UserId,
        tenant_id: Uuid,
        module_config: Option<serde_json::Value>,
    ) -> Result<(), KanbanError> {
        let board_folder = self
            .folder_service
            .get_folder(board_folder_id, user_id)
            .await
            .map_err(KanbanError::from)?;

        let title = board_folder.name.replace('-', " ");
        let slug = slugify(&title);

        // Try to extract kanban config from template module_config
        let (columns, labels, settings) = if let Some(config) = module_config {
            if let Some(kanban_config) = config.get("kanban") {
                let columns: Vec<ColumnDef> = kanban_config
                    .get("columns")
                    .and_then(|c| serde_json::from_value(c.clone()).ok())
                    .unwrap_or_else(standard_columns);
                let labels: Vec<KanbanLabel> = kanban_config
                    .get("labels")
                    .and_then(|l| serde_json::from_value(l.clone()).ok())
                    .unwrap_or_else(default_labels);
                let settings: KanbanSettings = kanban_config
                    .get("settings")
                    .and_then(|s| serde_json::from_value(s.clone()).ok())
                    .unwrap_or_else(default_settings);
                (columns, labels, settings)
            } else {
                (standard_columns(), default_labels(), default_settings())
            }
        } else {
            (standard_columns(), default_labels(), default_settings())
        };

        let board_meta = BoardMetadata {
            id: board_folder.id.to_string(),
            type_: "kanban.board".to_string(),
            title: title.clone(),
            slug: slug.clone(),
            module: "kanban".to_string(),
            schema_version: "1.0".to_string(),
            columns: columns.clone(),
            created_at: board_folder.created_at,
            updated_at: Utc::now(),
            archived: false,
            labels,
            settings,
        };

        self.write_board_metadata(&board_folder, &board_meta, user_id, tenant_id)
            .await?;
        self.ensure_events_file(&board_folder, user_id, tenant_id)
            .await?;

        for col in &columns {
            let col_path = format!("{}/{}", board_folder.path.trim_end_matches('/'), col.slug);
            let col_folder = match self
                .metadata_store
                .find_folder_by_path(&col_path, board_folder.owner_id)
                .await
                .map_err(|e| KanbanError::Database(e.to_string()))?
            {
                Some(folder) => folder,
                None => self
                    .folder_service
                    .create_folder(col.slug.clone(), Some(board_folder.id), user_id, tenant_id)
                    .await
                    .map_err(KanbanError::from)?,
            };

            let col_meta = ColumnMetadata {
                id: col.id.clone(),
                type_: "kanban.column".to_string(),
                title: col.title.clone(),
                slug: col.slug.clone(),
                order: col.order,
                status: col.status.clone(),
                board_id: board_folder.id.to_string(),
                wip_limit: col.wip_limit,
                schema_version: "1.0".to_string(),
            };
            self.write_column_metadata(&col_folder, &col_meta, user_id, tenant_id)
                .await?;
        }

        Ok(())
    }

    pub async fn ensure_board_id(
        &self,
        board_id_or_slug: &str,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<Uuid, KanbanError> {
        if let Ok(id) = Uuid::parse_str(board_id_or_slug) {
            Ok(id)
        } else {
            let folder = self
                .find_board_by_slug(board_id_or_slug, user_id, tenant_id)
                .await?
                .ok_or(KanbanError::BoardNotFound)?;
            Ok(folder.id)
        }
    }

    pub async fn find_board_by_slug(
        &self,
        slug: &str,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<Option<Folder>, KanbanError> {
        let root = match self.find_kanban_root(user_id, tenant_id).await? {
            Some(r) => r,
            None => return Ok(None),
        };

        let contents = self
            .folder_service
            .list_contents(root.id, user_id)
            .await
            .map_err(KanbanError::from)?;

        for folder in contents.folders {
            if folder.name == slug {
                return Ok(Some(folder));
            }
            if let Ok(meta) = self.load_board_metadata(&folder, user_id).await {
                if meta.slug == slug {
                    return Ok(Some(folder));
                }
            }
        }
        Ok(None)
    }

    // -------------------------------------------------------------------------
    // Private helpers
    // -------------------------------------------------------------------------

    async fn ensure_standard_board_columns(
        &self,
        board_folder: &Folder,
        board_meta: &mut BoardMetadata,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<(), KanbanError> {
        let standards = standard_columns();
        let board_path = board_folder.path.trim_end_matches('/');
        let mut changed = false;
        let legacy_slugs: std::collections::HashSet<&str> =
            ["00-Backlog", "01-In-Progress", "02-Done"]
                .iter()
                .cloned()
                .collect();
        let current_slugs: std::collections::HashSet<&str> =
            board_meta.columns.iter().map(|c| c.slug.as_str()).collect();
        let looks_like_legacy = !board_meta.columns.is_empty()
            && current_slugs.len() == 3
            && current_slugs == legacy_slugs;
        let should_upgrade_to_standard = board_meta.columns.is_empty() || looks_like_legacy;

        if looks_like_legacy {
            for (legacy_slug, standard_slug) in
                [("01-In-Progress", "02-In-Progress"), ("02-Done", "04-Done")]
            {
                let legacy_path = format!("{}/{}", board_path, legacy_slug);
                let standard_path = format!("{}/{}", board_path, standard_slug);
                let standard_exists = self
                    .metadata_store
                    .find_folder_by_path(&standard_path, board_folder.owner_id)
                    .await
                    .map_err(|e| KanbanError::Database(e.to_string()))?
                    .is_some();

                if !standard_exists {
                    if let Some(legacy_folder) = self
                        .metadata_store
                        .find_folder_by_path(&legacy_path, board_folder.owner_id)
                        .await
                        .map_err(|e| KanbanError::Database(e.to_string()))?
                    {
                        self.folder_service
                            .rename_folder(legacy_folder.id, standard_slug.to_string(), user_id)
                            .await
                            .map_err(KanbanError::from)?;
                        changed = true;
                    }
                }
            }
        }

        if should_upgrade_to_standard {
            board_meta.columns = standards;
            board_meta.updated_at = Utc::now();
            changed = true;
        }

        for col in &board_meta.columns {
            let col_path = format!("{}/{}", board_path, col.slug);
            let col_folder = match self
                .metadata_store
                .find_folder_by_path(&col_path, board_folder.owner_id)
                .await
                .map_err(|e| KanbanError::Database(e.to_string()))?
            {
                Some(folder) => folder,
                None => {
                    changed = true;
                    self.folder_service
                        .create_folder(col.slug.clone(), Some(board_folder.id), user_id, tenant_id)
                        .await
                        .map_err(KanbanError::from)?
                }
            };

            let col_meta = ColumnMetadata {
                id: col.id.clone(),
                type_: "kanban.column".to_string(),
                title: col.title.clone(),
                slug: col.slug.clone(),
                order: col.order,
                status: col.status.clone(),
                board_id: board_folder.id.to_string(),
                wip_limit: col.wip_limit,
                schema_version: "1.0".to_string(),
            };
            self.write_column_metadata(&col_folder, &col_meta, user_id, tenant_id)
                .await?;
        }

        if changed {
            self.write_board_metadata(board_folder, board_meta, user_id, tenant_id)
                .await?;
        }

        Ok(())
    }

    async fn find_kanban_root(
        &self,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<Option<Folder>, KanbanError> {
        // Legacy: check root path
        let row = sqlx::query!(
            "SELECT id, name, path, parent_folder_id, owner_id, created_at, updated_at, starred_at, deleted_at, tenant_id FROM folders WHERE path = '/Kanban' AND tenant_id = $1 AND owner_id = $2 AND deleted_at IS NULL LIMIT 1",
            tenant_id,
            user_id
        )
        .fetch_optional(self.metadata_store.pool())
        .await
        .map_err(|e| KanbanError::Database(e.to_string()))?;

        if let Some(r) = row {
            return Ok(Some(Folder {
                id: r.id,
                name: r.name,
                path: r.path,
                parent_folder_id: r.parent_folder_id,
                owner_id: r.owner_id,
                created_at: r.created_at,
                updated_at: r.updated_at,
                starred_at: r.starred_at,
                deleted_at: r.deleted_at,
                tenant_id: r.tenant_id,
                ancestor_ids: None,
            }));
        }

        // New: check under /Workspace
        let row = sqlx::query!(
            "SELECT id, name, path, parent_folder_id, owner_id, created_at, updated_at, starred_at, deleted_at, tenant_id FROM folders WHERE path = '/Workspace/Kanban' AND tenant_id = $1 AND owner_id = $2 AND deleted_at IS NULL LIMIT 1",
            tenant_id,
            user_id
        )
        .fetch_optional(self.metadata_store.pool())
        .await
        .map_err(|e| KanbanError::Database(e.to_string()))?;

        Ok(row.map(|r| Folder {
            id: r.id,
            name: r.name,
            path: r.path,
            parent_folder_id: r.parent_folder_id,
            owner_id: r.owner_id,
            created_at: r.created_at,
            updated_at: r.updated_at,
            starred_at: r.starred_at,
            deleted_at: r.deleted_at,
            tenant_id: r.tenant_id,
            ancestor_ids: None,
        }))
    }

    async fn ensure_workspace_folder(
        &self,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<Folder, KanbanError> {
        let folders = self
            .metadata_store
            .list_folders(None, user_id, tenant_id)
            .await
            .map_err(|e| KanbanError::Database(e.to_string()))?;
        if let Some(ws) = folders.into_iter().find(|f| f.name == "Workspace") {
            return Ok(ws);
        }
        self.folder_service
            .create_folder_or_get("Workspace".into(), None, user_id, tenant_id)
            .await
            .map_err(KanbanError::from)
    }

    async fn ensure_kanban_root(
        &self,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<Folder, KanbanError> {
        if let Some(root) = self.find_kanban_root(user_id, tenant_id).await? {
            return Ok(root);
        }
        let ws = self.ensure_workspace_folder(user_id, tenant_id).await?;
        let folder = self
            .folder_service
            .create_folder_or_get("Kanban".to_string(), Some(ws.id), user_id, tenant_id)
            .await
            .map_err(KanbanError::from)?;
        Ok(folder)
    }

    async fn load_board_metadata(
        &self,
        board_folder: &Folder,
        user_id: UserId,
    ) -> Result<BoardMetadata, KanbanError> {
        if let Some(file) = self
            .find_file_in_folder(board_folder.id, user_id, ".rustshare-board.json")
            .await?
        {
            let content = self
                .object_store
                .get(&file.storage_key())
                .await
                .map_err(|e| KanbanError::Storage(e.to_string()))?;
            // If the board metadata is from a template stub or otherwise incomplete,
            // fall back to deriving from the folder structure rather than failing.
            if let Ok(meta) = serde_json::from_slice::<BoardMetadata>(&content) {
                return Ok(meta);
            }
        }

        // Fallback: derive metadata from folder structure
        let title = board_folder.name.replace('-', " ");
        let slug = slugify(&title);
        let columns = standard_columns();
        Ok(BoardMetadata {
            id: board_folder.id.to_string(),
            type_: "kanban.board".to_string(),
            title,
            slug,
            module: "kanban".to_string(),
            schema_version: "1.0".to_string(),
            columns,
            labels: default_labels(),
            settings: default_settings(),
            archived: false,
            created_at: board_folder.created_at,
            updated_at: board_folder.updated_at,
        })
    }

    async fn load_column_metadata(
        &self,
        col_folder: &Folder,
        board_id: &str,
        user_id: UserId,
    ) -> Result<ColumnMetadata, KanbanError> {
        if let Some(file) = self
            .find_file_in_folder(col_folder.id, user_id, ".rustshare-column.json")
            .await?
        {
            let content = self
                .object_store
                .get(&file.storage_key())
                .await
                .map_err(|e| KanbanError::Storage(e.to_string()))?;
            let meta: ColumnMetadata = serde_json::from_slice(&content)
                .map_err(|e| KanbanError::InvalidData(format!("Corrupt column metadata: {e}")))?;
            return Ok(meta);
        }

        // Fallback
        let (status, order) = parse_column_slug(&col_folder.name);
        Ok(ColumnMetadata {
            id: format!("column_{}", status),
            type_: "kanban.column".to_string(),
            title: col_folder
                .name
                .replace("00-", "")
                .replace("01-", "")
                .replace("02-", "")
                .replace("03-", "")
                .replace("04-", "")
                .replace('-', " "),
            slug: col_folder.name.clone(),
            order,
            status,
            board_id: board_id.to_string(),
            wip_limit: None,
            schema_version: "1.0".to_string(),
        })
    }

    async fn load_card_metadata(
        &self,
        card_folder: &Folder,
        user_id: UserId,
    ) -> Result<CardMetadata, KanbanError> {
        let md_filename = format!("{}.md", card_folder.id);
        if let Some(file) = self
            .find_file_in_folder(card_folder.id, user_id, &md_filename)
            .await?
        {
            let content = self
                .object_store
                .get(&file.storage_key())
                .await
                .map_err(|e| KanbanError::Storage(e.to_string()))?;
            let (meta, _) = parse_card_markdown(
                &String::from_utf8_lossy(&content),
                &card_folder.id.to_string(),
            )?;
            return Ok(meta);
        }

        // Fallback to JSON
        let file = self
            .find_file_in_folder(card_folder.id, user_id, ".rustshare-card.json")
            .await?
            .ok_or_else(|| KanbanError::InvalidData("Missing card metadata".to_string()))?;

        let content = self
            .object_store
            .get(&file.storage_key())
            .await
            .map_err(|e| KanbanError::Storage(e.to_string()))?;
        let meta: CardMetadata = serde_json::from_slice(&content)
            .map_err(|e| KanbanError::InvalidData(format!("Corrupt card metadata: {e}")))?;
        Ok(meta)
    }

    async fn load_card(
        &self,
        card_folder: &Folder,
        user_id: UserId,
    ) -> Result<KanbanCard, KanbanError> {
        let (mut meta, content) = self.load_card_markdown(card_folder, user_id).await?;

        let board_id = Uuid::parse_str(&meta.board_id)
            .map_err(|_| KanbanError::InvalidData("Invalid board id".to_string()))?;
        let board_folder = self
            .folder_service
            .get_folder(board_id, user_id)
            .await
            .map_err(KanbanError::from)?;
        let board_meta = self.load_board_metadata(&board_folder, user_id).await?;

        let preview = derive_preview(&content, &meta.title);
        meta.description_preview = Some(preview.clone());

        Ok(KanbanCard {
            id: meta.id,
            title: meta.title,
            slug: meta.slug,
            content: content.clone(),
            description_preview: preview,
            column_id: meta.column_id,
            status: meta.status,
            order: meta.order,
            labels: self.map_labels(&meta.labels, &board_meta.labels),
            assignees: self.map_assignees(&meta.assignees).await,
            due_date: meta.due_date,
            priority: meta.priority,
            attachments_count: meta.attachments_count,
            checklist: KanbanChecklist {
                done: meta.checklist_done,
                total: meta.checklist_total,
            },
            checklists: meta.checklists,
            archived: meta.archived,
            created_at: meta.created_at,
            updated_at: meta.updated_at,
            path: card_folder.path.clone(),
            schema_version: meta.schema_version,
        })
    }

    fn map_labels(&self, card_labels: &[String], board_labels: &[KanbanLabel]) -> Vec<KanbanLabel> {
        card_labels
            .iter()
            .filter_map(|id| board_labels.iter().find(|l| &l.id == id).cloned())
            .collect()
    }

    async fn map_assignees(&self, assignee_ids: &[String]) -> Vec<KanbanAssignee> {
        let mut assignees = Vec::new();
        for id_str in assignee_ids {
            if let Ok(id) = Uuid::parse_str(id_str) {
                if let Ok(Some(user)) = self.metadata_store.find_user_by_id(id).await {
                    assignees.push(KanbanAssignee {
                        id: user.id.to_string(),
                        display_name: user.display_name.clone(),
                        initials: get_initials(&user.display_name),
                        avatar_url: user.avatar_path.clone(),
                    });
                }
            }
        }
        assignees
    }

    async fn write_column_metadata(
        &self,
        col_folder: &Folder,
        metadata: &ColumnMetadata,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<(), KanbanError> {
        let content = serde_json::to_string_pretty(metadata)?;
        self.write_file_by_name(
            col_folder.id,
            ".rustshare-column.json",
            &content,
            "application/json",
            user_id,
            tenant_id,
        )
        .await?;
        Ok(())
    }

    async fn write_card_index(
        &self,
        card_folder: &Folder,
        content: &str,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<(), KanbanError> {
        let mut metadata = self.load_card_metadata(card_folder, user_id).await?;
        metadata.updated_at = Utc::now();
        self.write_card_markdown(card_folder, &metadata, content, user_id, tenant_id)
            .await
    }

    async fn ensure_events_file(
        &self,
        folder: &Folder,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<(), KanbanError> {
        if self
            .find_file_in_folder(folder.id, user_id, "events.jsonl")
            .await?
            .is_none()
        {
            self.file_service
                .upload_file(
                    user_id,
                    "events.jsonl".to_string(),
                    Some(folder.id),
                    Bytes::from_static(b""),
                    "application/jsonlines".to_string(),
                    tenant_id,
                )
                .await
                .map_err(KanbanError::from)?;
        }
        Ok(())
    }

    async fn find_file_in_folder(
        &self,
        folder_id: Uuid,
        user_id: UserId,
        name: &str,
    ) -> Result<Option<File>, KanbanError> {
        // Bypass folder_service.list_contents because it filters out
        // hidden kanban metadata files (.rustshare-board.json, etc.)
        let folder = self
            .folder_service
            .get_folder(folder_id, user_id)
            .await
            .map_err(KanbanError::from)?;
        let files = self
            .metadata_store
            .list_files_by_parent(Some(folder_id), folder.tenant_id)
            .await
            .map_err(|e| KanbanError::Database(e.to_string()))?;
        Ok(files.into_iter().find(|f| f.name == name))
    }

    async fn write_file_by_name(
        &self,
        folder_id: Uuid,
        name: &str,
        content: &str,
        mime_type: &str,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<File, KanbanError> {
        self.write_binary_file_by_name(
            folder_id,
            name,
            Bytes::from(content.to_string()),
            mime_type,
            user_id,
            tenant_id,
        )
        .await
    }

    async fn write_binary_file_by_name(
        &self,
        folder_id: Uuid,
        name: &str,
        content: Bytes,
        mime_type: &str,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<File, KanbanError> {
        if let Some(existing) = self.find_file_in_folder(folder_id, user_id, name).await? {
            let updated = self
                .file_service
                .update_file(existing.id, user_id, existing.current_version, content)
                .await
                .map_err(KanbanError::from)?;
            Ok(updated)
        } else {
            let created = self
                .file_service
                .upload_file(
                    user_id,
                    name.to_string(),
                    Some(folder_id),
                    content,
                    mime_type.to_string(),
                    tenant_id,
                )
                .await
                .map_err(KanbanError::from)?;
            Ok(created)
        }
    }

    async fn load_card_markdown(
        &self,
        card_folder: &Folder,
        user_id: UserId,
    ) -> Result<(CardMetadata, String), KanbanError> {
        let md_filename = format!("{}.md", card_folder.id);
        if let Some(file) = self
            .find_file_in_folder(card_folder.id, user_id, &md_filename)
            .await?
        {
            let bytes = self
                .object_store
                .get(&file.storage_key())
                .await
                .map_err(|e| KanbanError::Storage(e.to_string()))?;
            let (meta, description) = parse_card_markdown(
                &String::from_utf8_lossy(&bytes),
                &card_folder.id.to_string(),
            )?;
            return Ok((meta, description));
        }

        // Fallback to JSON + index.md
        let meta = self.load_card_metadata(card_folder, user_id).await?;
        let description = if let Some(file) = self
            .find_file_in_folder(card_folder.id, user_id, "index.md")
            .await?
        {
            let bytes = self
                .object_store
                .get(&file.storage_key())
                .await
                .map_err(|e| KanbanError::Storage(e.to_string()))?;
            String::from_utf8_lossy(&bytes).to_string()
        } else {
            String::new()
        };
        Ok((meta, description))
    }

    async fn write_card_markdown(
        &self,
        card_folder: &Folder,
        metadata: &CardMetadata,
        description: &str,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<(), KanbanError> {
        let frontmatter = CardMarkdownFrontmatter::from_metadata(metadata);
        let yaml = serde_yaml::to_string(&frontmatter)
            .map_err(|e| KanbanError::InvalidData(format!("YAML serialization error: {e}")))?;
        let body = format!("## Description\n\n{}", description);
        let content = format!("---\n{}---\n\n{}", yaml, body);

        let filename = format!("{}.md", card_folder.id);
        self.write_file_by_name(
            card_folder.id,
            &filename,
            &content,
            "text/markdown",
            user_id,
            tenant_id,
        )
        .await?;
        Ok(())
    }

    async fn load_card_description(
        &self,
        card_folder: &Folder,
        user_id: UserId,
    ) -> Result<String, KanbanError> {
        let md_filename = format!("{}.md", card_folder.id);
        if let Some(file) = self
            .find_file_in_folder(card_folder.id, user_id, &md_filename)
            .await?
        {
            let bytes = self
                .object_store
                .get(&file.storage_key())
                .await
                .map_err(|e| KanbanError::Storage(e.to_string()))?;
            let (_, description) = parse_card_markdown(
                &String::from_utf8_lossy(&bytes),
                &card_folder.id.to_string(),
            )?;
            return Ok(description);
        }

        if let Some(file) = self
            .find_file_in_folder(card_folder.id, user_id, "index.md")
            .await?
        {
            let bytes = self
                .object_store
                .get(&file.storage_key())
                .await
                .map_err(|e| KanbanError::Storage(e.to_string()))?;
            return Ok(String::from_utf8_lossy(&bytes).to_string());
        }

        Ok(String::new())
    }

    async fn rebalance_column_if_needed(
        &self,
        column_folder_id: Uuid,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<(), KanbanError> {
        let contents = self
            .folder_service
            .list_contents(column_folder_id, user_id)
            .await
            .map_err(KanbanError::from)?;

        let mut cards: Vec<(Folder, CardMetadata)> = Vec::new();
        for folder in contents.folders {
            if !folder.name.starts_with("CARD-") {
                continue;
            }
            if let Ok(meta) = self.load_card_metadata(&folder, user_id).await {
                if !meta.archived {
                    cards.push((folder, meta));
                }
            }
        }

        if cards.len() < 2 {
            return Ok(());
        }

        cards.sort_by_key(|(_, meta)| meta.order);

        let mut needs_rebalance = false;
        for i in 1..cards.len() {
            let gap = (cards[i].1.order - cards[i - 1].1.order).abs();
            if gap < 10 {
                needs_rebalance = true;
                break;
            }
        }

        if !needs_rebalance {
            return Ok(());
        }

        for (idx, (folder, mut meta)) in cards.into_iter().enumerate() {
            meta.order = ((idx + 1) as i32).saturating_mul(1000);
            meta.updated_at = Utc::now();
            self.write_card_metadata(&folder, &meta, user_id, tenant_id)
                .await?;
        }

        Ok(())
    }

    async fn next_card_sequence(
        &self,
        column_folder_id: Uuid,
        user_id: UserId,
    ) -> Result<u32, KanbanError> {
        let contents = self
            .folder_service
            .list_contents(column_folder_id, user_id)
            .await
            .map_err(KanbanError::from)?;
        let mut max_seq = 0u32;
        for folder in contents.folders {
            if let Some(seq_str) = folder.name.strip_prefix("CARD-") {
                if let Some(end) = seq_str.find('-') {
                    if let Ok(seq) = seq_str[..end].parse::<u32>() {
                        max_seq = max_seq.max(seq);
                    }
                }
            }
        }
        Ok(max_seq + 1)
    }

    async fn load_card_activity(
        &self,
        card_folder: &Folder,
        user_id: UserId,
    ) -> Result<Vec<KanbanEvent>, KanbanError> {
        let file = match self
            .find_file_in_folder(card_folder.id, user_id, "events.jsonl")
            .await?
        {
            Some(f) => f,
            None => return Ok(vec![]),
        };

        let bytes = self
            .object_store
            .get(&file.storage_key())
            .await
            .map_err(|e| KanbanError::Storage(e.to_string()))?;

        let content = String::from_utf8_lossy(&bytes);
        let mut events = vec![];
        for line in content.lines() {
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(event) = serde_json::from_str::<KanbanEvent>(line) {
                events.push(event);
            }
        }

        events.sort_by_key(|a| std::cmp::Reverse(a.timestamp));
        Ok(events)
    }

    async fn load_card_attachments(
        &self,
        card_folder: &Folder,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<Vec<KanbanCardAttachment>, KanbanError> {
        let folders = self
            .metadata_store
            .list_folders(Some(card_folder.id), user_id, tenant_id)
            .await
            .map_err(|e| KanbanError::Database(e.to_string()))?;

        let attachments_folder = match folders.into_iter().find(|f| f.name == "attachments") {
            Some(f) => f,
            None => return Ok(vec![]),
        };

        let files = self
            .metadata_store
            .list_files(Some(attachments_folder.id), user_id, tenant_id)
            .await
            .map_err(|e| KanbanError::Database(e.to_string()))?;

        Ok(files
            .into_iter()
            .map(|f| KanbanCardAttachment {
                id: f.id.to_string(),
                name: f.name,
                size: f.size,
                mime_type: f.mime_type,
                created_at: f.created_at,
                created_by: f.owner_id.to_string(),
                path: None,
            })
            .collect())
    }

    // --- Card Attachments ---

    pub async fn add_card_attachment(
        &self,
        card_id: Uuid,
        filename: String,
        content: Bytes,
        mime_type: String,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<KanbanCardAttachment, KanbanError> {
        // Enforce 10MB limit
        if content.len() > 10 * 1024 * 1024 {
            return Err(KanbanError::InvalidData(
                "Attachment exceeds 10MB limit".to_string(),
            ));
        }

        // Sanitize filename
        let filename = Self::sanitize_attachment_name(&filename)?;

        let card_folder = self
            .folder_service
            .get_folder(card_id, user_id)
            .await
            .map_err(KanbanError::from)?;
        let attachments_folder = self
            .get_attachments_folder(card_id, user_id, tenant_id)
            .await?;

        let file = self
            .write_binary_file_by_name(
                attachments_folder.id,
                &filename,
                content,
                &mime_type,
                user_id,
                tenant_id,
            )
            .await?;

        let mut card_meta = self.load_card_metadata(&card_folder, user_id).await?;
        card_meta.attachments_count += 1;
        card_meta.updated_at = Utc::now();
        self.write_card_metadata(&card_folder, &card_meta, user_id, tenant_id)
            .await?;

        self.append_kanban_event(
            &card_folder,
            KanbanEvent {
                event_type: "card.attachment_added".to_string(),
                timestamp: Utc::now(),
                actor: user_id.to_string(),
                payload: serde_json::json!({ "filename": filename, "attachmentId": file.id.to_string() }),
                id: None,
                text: None,
            },
            user_id,
        ).await?;

        Ok(KanbanCardAttachment {
            id: file.id.to_string(),
            name: file.name,
            size: file.size,
            mime_type: file.mime_type,
            created_at: file.created_at,
            created_by: file.owner_id.to_string(),
            path: None,
        })
    }

    pub async fn delete_card_attachment(
        &self,
        card_id: Uuid,
        attachment_id: Uuid,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<(), KanbanError> {
        let card_folder = self
            .folder_service
            .get_folder(card_id, user_id)
            .await
            .map_err(KanbanError::from)?;

        self.file_service
            .delete_file(attachment_id, user_id)
            .await
            .map_err(KanbanError::from)?;

        let mut card_meta = self.load_card_metadata(&card_folder, user_id).await?;
        if card_meta.attachments_count > 0 {
            card_meta.attachments_count -= 1;
        }
        card_meta.updated_at = Utc::now();
        self.write_card_metadata(&card_folder, &card_meta, user_id, tenant_id)
            .await?;

        self.append_kanban_event(
            &card_folder,
            KanbanEvent {
                event_type: "card.attachment_deleted".to_string(),
                timestamp: Utc::now(),
                actor: user_id.to_string(),
                payload: serde_json::json!({ "attachmentId": attachment_id.to_string() }),
                id: None,
                text: None,
            },
            user_id,
        )
        .await?;

        Ok(())
    }

    async fn get_attachments_folder(
        &self,
        card_folder_id: Uuid,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<Folder, KanbanError> {
        let folders = self
            .metadata_store
            .list_folders(Some(card_folder_id), user_id, tenant_id)
            .await
            .map_err(|e| KanbanError::Database(e.to_string()))?;
        if let Some(f) = folders.into_iter().find(|f| f.name == "attachments") {
            Ok(f)
        } else {
            self.folder_service
                .create_folder(
                    "attachments".to_string(),
                    Some(card_folder_id),
                    user_id,
                    tenant_id,
                )
                .await
                .map_err(KanbanError::from)
        }
    }

    // --- Card Checklists ---

    pub async fn add_checklist(
        &self,
        card_id: Uuid,
        title: String,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<KanbanChecklistGroup, KanbanError> {
        let card_folder = self
            .folder_service
            .get_folder(card_id, user_id)
            .await
            .map_err(KanbanError::from)?;
        let mut card_meta = self.load_card_metadata(&card_folder, user_id).await?;

        let group = KanbanChecklistGroup {
            id: Uuid::new_v4().to_string(),
            title: title.clone(),
            items: vec![],
        };

        card_meta.checklists.push(group.clone());
        card_meta.updated_at = Utc::now();
        self.write_card_metadata(&card_folder, &card_meta, user_id, tenant_id)
            .await?;

        self.append_kanban_event(
            &card_folder,
            KanbanEvent {
                event_type: "card.checklist_added".to_string(),
                timestamp: Utc::now(),
                actor: user_id.to_string(),
                payload: serde_json::json!({ "title": title, "checklistId": group.id }),
                id: None,
                text: None,
            },
            user_id,
        )
        .await?;

        Ok(group)
    }

    pub async fn add_checklist_item(
        &self,
        card_id: Uuid,
        checklist_id: String,
        text: String,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<KanbanChecklistItem, KanbanError> {
        let card_folder = self
            .folder_service
            .get_folder(card_id, user_id)
            .await
            .map_err(KanbanError::from)?;
        let mut card_meta = self.load_card_metadata(&card_folder, user_id).await?;

        let checklist = card_meta
            .checklists
            .iter_mut()
            .find(|c| c.id == checklist_id)
            .ok_or_else(|| KanbanError::NotFound(format!("Checklist {}", checklist_id)))?;

        let item = KanbanChecklistItem {
            id: Uuid::new_v4().to_string(),
            text: text.clone(),
            done: false,
        };

        checklist.items.push(item.clone());
        self.recalculate_checklist_summary(&mut card_meta);
        card_meta.updated_at = Utc::now();
        self.write_card_metadata(&card_folder, &card_meta, user_id, tenant_id)
            .await?;

        self.append_kanban_event(
            &card_folder,
            KanbanEvent {
                event_type: "card.checklist_item_added".to_string(),
                timestamp: Utc::now(),
                actor: user_id.to_string(),
                payload: serde_json::json!({ "checklistId": checklist_id, "text": text, "itemId": item.id }),
                id: None,
                text: None,
            },
            user_id,
        ).await?;

        Ok(item)
    }

    pub async fn toggle_checklist_item(
        &self,
        card_id: Uuid,
        checklist_id: String,
        item_id: String,
        done: bool,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<(), KanbanError> {
        let card_folder = self
            .folder_service
            .get_folder(card_id, user_id)
            .await
            .map_err(KanbanError::from)?;
        let mut card_meta = self.load_card_metadata(&card_folder, user_id).await?;

        let checklist = card_meta
            .checklists
            .iter_mut()
            .find(|c| c.id == checklist_id)
            .ok_or_else(|| KanbanError::NotFound(format!("Checklist {}", checklist_id)))?;

        let item = checklist
            .items
            .iter_mut()
            .find(|i| i.id == item_id)
            .ok_or_else(|| KanbanError::NotFound(format!("Item {}", item_id)))?;

        item.done = done;
        self.recalculate_checklist_summary(&mut card_meta);
        card_meta.updated_at = Utc::now();
        self.write_card_metadata(&card_folder, &card_meta, user_id, tenant_id)
            .await?;

        self.append_kanban_event(
            &card_folder,
            KanbanEvent {
                event_type: "card.checklist_item_toggled".to_string(),
                timestamp: Utc::now(),
                actor: user_id.to_string(),
                payload: serde_json::json!({ "checklistId": checklist_id, "itemId": item_id, "done": done }),
                id: None,
                text: None,
            },
            user_id,
        ).await?;

        Ok(())
    }

    pub async fn delete_checklist_item(
        &self,
        card_id: Uuid,
        checklist_id: String,
        item_id: String,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<(), KanbanError> {
        let card_folder = self
            .folder_service
            .get_folder(card_id, user_id)
            .await
            .map_err(KanbanError::from)?;
        let mut card_meta = self.load_card_metadata(&card_folder, user_id).await?;

        let checklist = card_meta
            .checklists
            .iter_mut()
            .find(|c| c.id == checklist_id)
            .ok_or_else(|| KanbanError::NotFound(format!("Checklist {}", checklist_id)))?;

        checklist.items.retain(|i| i.id != item_id);
        self.recalculate_checklist_summary(&mut card_meta);
        card_meta.updated_at = Utc::now();
        self.write_card_metadata(&card_folder, &card_meta, user_id, tenant_id)
            .await?;

        self.append_kanban_event(
            &card_folder,
            KanbanEvent {
                event_type: "card.checklist_item_deleted".to_string(),
                timestamp: Utc::now(),
                actor: user_id.to_string(),
                payload: serde_json::json!({ "checklistId": checklist_id, "itemId": item_id }),
                id: None,
                text: None,
            },
            user_id,
        )
        .await?;

        Ok(())
    }

    pub async fn delete_checklist(
        &self,
        card_id: Uuid,
        checklist_id: String,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<(), KanbanError> {
        let card_folder = self
            .folder_service
            .get_folder(card_id, user_id)
            .await
            .map_err(KanbanError::from)?;
        let mut card_meta = self.load_card_metadata(&card_folder, user_id).await?;

        card_meta.checklists.retain(|c| c.id != checklist_id);
        self.recalculate_checklist_summary(&mut card_meta);
        card_meta.updated_at = Utc::now();
        self.write_card_metadata(&card_folder, &card_meta, user_id, tenant_id)
            .await?;

        self.append_kanban_event(
            &card_folder,
            KanbanEvent {
                event_type: "card.checklist_deleted".to_string(),
                timestamp: Utc::now(),
                actor: user_id.to_string(),
                payload: serde_json::json!({ "checklistId": checklist_id }),
                id: None,
                text: None,
            },
            user_id,
        )
        .await?;

        Ok(())
    }

    fn recalculate_checklist_summary(&self, meta: &mut CardMetadata) {
        let mut done = 0;
        let mut total = 0;
        for group in &meta.checklists {
            for item in &group.items {
                total += 1;
                if item.done {
                    done += 1;
                }
            }
        }
        meta.checklist_done = done;
        meta.checklist_total = total;
    }
}

// ============================================================================
// Utilities
// ============================================================================

fn standard_columns() -> Vec<ColumnDef> {
    vec![
        ColumnDef {
            id: "column_backlog".to_string(),
            title: "Backlog".to_string(),
            slug: "00-Backlog".to_string(),
            order: 0,
            status: "backlog".to_string(),
            wip_limit: None,
        },
        ColumnDef {
            id: "column_ready".to_string(),
            title: "Ready".to_string(),
            slug: "01-Ready".to_string(),
            order: 1,
            status: "ready".to_string(),
            wip_limit: None,
        },
        ColumnDef {
            id: "column_in_progress".to_string(),
            title: "In Progress".to_string(),
            slug: "02-In-Progress".to_string(),
            order: 2,
            status: "in_progress".to_string(),
            wip_limit: None,
        },
        ColumnDef {
            id: "column_review".to_string(),
            title: "Review".to_string(),
            slug: "03-Review".to_string(),
            order: 3,
            status: "review".to_string(),
            wip_limit: None,
        },
        ColumnDef {
            id: "column_done".to_string(),
            title: "Done".to_string(),
            slug: "04-Done".to_string(),
            order: 4,
            status: "done".to_string(),
            wip_limit: None,
        },
    ]
}

fn default_labels() -> Vec<KanbanLabel> {
    vec![
        KanbanLabel {
            id: "label_green".to_string(),
            name: "Low".to_string(),
            color: "green".to_string(),
        },
        KanbanLabel {
            id: "label_yellow".to_string(),
            name: "Medium".to_string(),
            color: "yellow".to_string(),
        },
        KanbanLabel {
            id: "label_red".to_string(),
            name: "High".to_string(),
            color: "red".to_string(),
        },
        KanbanLabel {
            id: "label_blue".to_string(),
            name: "UI".to_string(),
            color: "blue".to_string(),
        },
        KanbanLabel {
            id: "label_purple".to_string(),
            name: "Backend".to_string(),
            color: "purple".to_string(),
        },
        KanbanLabel {
            id: "label_orange".to_string(),
            name: "Bug".to_string(),
            color: "orange".to_string(),
        },
        KanbanLabel {
            id: "label_gray".to_string(),
            name: "DevOps".to_string(),
            color: "gray".to_string(),
        },
    ]
}

fn default_settings() -> KanbanSettings {
    KanbanSettings {
        show_description_on_cards: true,
        description_preview_lines: 2,
        show_assignees: true,
        show_labels: true,
        show_due_date: true,
        show_attachment_badge: true,
        show_checklist_badge: true,
    }
}

fn derive_preview(content: &str, title: &str) -> String {
    if content.trim().is_empty() {
        return String::new();
    }

    // 1. Remove frontmatter
    let mut text = content.to_string();
    if text.starts_with("---") {
        if let Some(end) = text[3..].find("---") {
            text = text[end + 6..].to_string();
        }
    }

    // 2. Remove first heading if it matches title
    text = text.trim().to_string();
    if let Some(first_line) = text.lines().next() {
        let trimmed_line = first_line.trim_start_matches('#').trim();
        if !trimmed_line.is_empty() && trimmed_line.to_lowercase() == title.to_lowercase() {
            text = text
                .lines()
                .skip(1)
                .collect::<Vec<_>>()
                .join("\n")
                .trim()
                .to_string();
        }
    }

    // 3. Strip Markdown lightly
    // Task lists: - [ ] or - [x]
    text = text.replace("- [ ]", "").replace("- [x]", "");

    // Images: ![alt](url) -> alt
    if let Ok(re_img) = Regex::new(r"!\[([^\]]*)\]\([^\)]*\)") {
        text = re_img.replace_all(&text, "$1").to_string();
    }

    // Links: [text](url) -> text
    if let Ok(re_link) = Regex::new(r"\[([^\]]*)\]\([^\)]*\)") {
        text = re_link.replace_all(&text, "$1").to_string();
    }

    // Bold/Italic/Strikethrough
    text = text
        .replace("**", "")
        .replace("__", "")
        .replace("~~", "")
        .replace(['*', '_'], "");

    // 4. First meaningful paragraph
    let mut preview = String::new();
    for paragraph in text.split("\n\n") {
        let p = paragraph.trim();
        // Skip headings and empty lines
        if !p.is_empty() && !p.starts_with('#') {
            preview = p.to_string();
            break;
        }
    }

    if preview.is_empty() {
        preview = text.replace('\n', " ").trim().to_string();
    }

    // 5. Collapse whitespace
    if let Ok(re_ws) = Regex::new(r"\s+") {
        preview = re_ws.replace_all(&preview, " ").to_string();
    }

    // 6. Limit length (140-180 chars)
    if preview.chars().count() > 160 {
        let mut truncated: String = preview.chars().take(160).collect();
        if let Some(last_space) = truncated.rfind(' ') {
            truncated.truncate(last_space);
        }
        truncated.push_str("...");
        preview = truncated;
    }

    preview.trim().to_string()
}

fn get_initials(name: &str) -> String {
    name.split_whitespace()
        .filter_map(|s| s.chars().next())
        .take(2)
        .collect::<String>()
        .to_uppercase()
}

fn slugify(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn parse_column_slug(slug: &str) -> (String, i32) {
    if let Some(rest) = slug.strip_prefix("00-") {
        (rest.to_lowercase().replace('-', "_"), 0)
    } else if let Some(rest) = slug.strip_prefix("01-") {
        (rest.to_lowercase().replace('-', "_"), 1)
    } else if let Some(rest) = slug.strip_prefix("02-") {
        (rest.to_lowercase().replace('-', "_"), 2)
    } else if let Some(rest) = slug.strip_prefix("03-") {
        (rest.to_lowercase().replace('-', "_"), 3)
    } else if let Some(rest) = slug.strip_prefix("04-") {
        (rest.to_lowercase().replace('-', "_"), 4)
    } else {
        (slug.to_lowercase().replace('-', "_"), 99)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Slugify ──────────────────────────────────────────────────────────

    #[test]
    fn test_slugify_basic() {
        assert_eq!(slugify("Hello World!"), "hello-world");
        assert_eq!(slugify("Valid-Slug-123"), "valid-slug-123");
    }

    #[test]
    fn test_slugify_strips_traversal_chars() {
        assert_eq!(slugify("../../../etc/passwd"), "etc-passwd");
        assert_eq!(slugify("..\\..\\windows"), "windows");
        assert_eq!(slugify("  Path/Traversal/Test  "), "path-traversal-test");
    }

    #[test]
    fn test_slugify_strips_special_chars() {
        assert_eq!(
            slugify("<script>alert(1)</script>"),
            "script-alert-1-script"
        );
        assert_eq!(slugify("a\0b"), "a-b");
    }

    #[test]
    fn test_slugify_empty_input() {
        assert_eq!(slugify(""), "");
        assert_eq!(slugify("!!!"), "");
    }

    // ── Color validation ─────────────────────────────────────────────────

    #[test]
    fn test_approved_colors_accepted() {
        for color in &["green", "yellow", "orange", "red", "purple", "blue", "gray"] {
            assert!(
                KanbanService::validate_color_static(color).is_ok(),
                "Should accept '{}'",
                color
            );
        }
    }

    #[test]
    fn test_invalid_colors_rejected() {
        for color in &[
            "#ff0000",
            "rgb(0,0,0)",
            "crimson",
            "transparent",
            "",
            "javascript:",
        ] {
            assert!(
                KanbanService::validate_color_static(color).is_err(),
                "Should reject '{}'",
                color
            );
        }
    }

    // ── Preview generation ───────────────────────────────────────────────

    #[test]
    fn test_derive_preview_strips_markdown() {
        let content = "# Title\n\nThis is a [link](http://example.com) and an ![image](img.jpg).\n- List item 1\n- List item 2";
        let preview = derive_preview(content, "Title");
        assert_eq!(
            preview,
            "This is a link and an image. - List item 1 - List item 2"
        );
    }

    #[test]
    fn test_derive_preview_removes_frontmatter() {
        let content = "---\ntitle: Hello\n---\n\nBody text here.";
        let preview = derive_preview(content, "Hello");
        assert!(preview.contains("Body text here"), "Preview: {}", preview);
        assert!(!preview.contains("---"));
    }

    #[test]
    fn test_derive_preview_truncates_long_content() {
        let long_content = format!("# Title\n\n{}", "A very long sentence. ".repeat(50));
        let preview = derive_preview(&long_content, "Title");
        assert!(
            preview.len() <= 200,
            "Preview should be truncated, got {} chars",
            preview.len()
        );
        assert!(preview.ends_with("..."));
    }

    #[test]
    fn test_derive_preview_empty() {
        let preview = derive_preview("", "Title");
        assert!(preview.is_empty() || preview.len() < 10);
    }

    // ── Parse column slug ────────────────────────────────────────────────

    #[test]
    fn test_parse_column_slug() {
        assert_eq!(parse_column_slug("00-Backlog"), ("backlog".to_string(), 0));
        assert_eq!(parse_column_slug("04-Done"), ("done".to_string(), 4));
        assert_eq!(parse_column_slug("custom"), ("custom".to_string(), 99));
    }

    // ── Get initials ─────────────────────────────────────────────────────

    #[test]
    fn test_get_initials() {
        assert_eq!(get_initials("John Doe"), "JD");
        assert_eq!(get_initials("Alice"), "A");
        assert_eq!(get_initials(""), "");
    }

    // ── Slug validation ──────────────────────────────────────────────────

    #[test]
    fn test_validate_slug_rejects_traversal() {
        assert!(KanbanService::validate_slug("../etc/passwd").is_err());
        assert!(KanbanService::validate_slug("..\\windows").is_err());
        assert!(KanbanService::validate_slug("%2e%2e%2fetc").is_err());
        assert!(KanbanService::validate_slug("%2E%2E").is_err());
    }

    #[test]
    fn test_validate_slug_rejects_absolute_paths() {
        assert!(KanbanService::validate_slug("/etc/passwd").is_err());
        assert!(KanbanService::validate_slug("\\windows\\system32").is_err());
    }

    #[test]
    fn test_validate_slug_rejects_control_chars() {
        assert!(KanbanService::validate_slug("hello\0world").is_err());
        assert!(KanbanService::validate_slug("hello\nworld").is_err());
    }

    #[test]
    fn test_validate_slug_accepts_valid() {
        assert!(KanbanService::validate_slug("my-board").is_ok());
        assert!(KanbanService::validate_slug("CARD-0001-task-name").is_ok());
    }

    #[test]
    fn test_validate_board_slug_rejects_invalid() {
        assert!(KanbanService::validate_board_slug("../backdoor").is_err());
        assert!(KanbanService::validate_board_slug("/absolute").is_err());
    }

    #[test]
    fn test_validate_card_slug_requires_prefix() {
        assert!(KanbanService::validate_card_slug("CARD-0001-hello").is_ok());
        assert!(KanbanService::validate_card_slug("malicious-name").is_err());
    }

    // ── Attachment filename sanitization ─────────────────────────────────

    #[test]
    fn test_sanitize_attachment_name_rejects_traversal() {
        assert!(KanbanService::sanitize_attachment_name("../secret.txt").is_err());
        assert!(KanbanService::sanitize_attachment_name("..\\secret.txt").is_err());
        assert!(KanbanService::sanitize_attachment_name("/etc/passwd").is_err());
    }

    #[test]
    fn test_sanitize_attachment_name_rejects_reserved_names() {
        assert!(KanbanService::sanitize_attachment_name(".rustshare-board.json").is_err());
        assert!(KanbanService::sanitize_attachment_name(".rustshare-card.json").is_err());
        assert!(KanbanService::sanitize_attachment_name("events.jsonl").is_err());
        assert!(KanbanService::sanitize_attachment_name("index.md").is_err());
    }

    #[test]
    fn test_sanitize_attachment_name_accepts_valid() {
        assert_eq!(
            KanbanService::sanitize_attachment_name("report.pdf").unwrap(),
            "report.pdf"
        );
        assert_eq!(
            KanbanService::sanitize_attachment_name("  image.png  ").unwrap(),
            "image.png"
        );
        assert_eq!(
            KanbanService::sanitize_attachment_name(".hidden.txt").unwrap(),
            "hidden.txt"
        );
    }

    #[test]
    fn test_sanitize_attachment_name_rejects_empty() {
        assert!(KanbanService::sanitize_attachment_name("").is_err());
        assert!(KanbanService::sanitize_attachment_name("   ").is_err());
        assert!(KanbanService::sanitize_attachment_name("...").is_err());
    }

    // ── Markdown card format ─────────────────────────────────────────────

    #[test]
    fn test_parse_card_markdown_basic() {
        let content = r#"---
id: 550e8400-e29b-41d4-a716-446655440000
title: "Task title"
board: board-slug
column: 01-ready
priority: medium
position: 1000
created_at: 2026-05-11T18:53:00Z
updated_at: 2026-05-11T18:53:00Z
---

## Description

Add a more detailed description here.
"#;

        let (meta, desc) =
            parse_card_markdown(content, "550e8400-e29b-41d4-a716-446655440000").unwrap();
        assert_eq!(meta.id, "550e8400-e29b-41d4-a716-446655440000");
        assert_eq!(meta.title, "Task title");
        assert_eq!(meta.board_id, "board-slug");
        assert_eq!(meta.column_id, "01-ready");
        assert_eq!(meta.priority, "medium");
        assert_eq!(meta.order, 1000);
        assert_eq!(desc, "Add a more detailed description here.");
    }

    #[test]
    fn test_parse_card_markdown_with_sections() {
        let content = r#"---
id: 550e8400-e29b-41d4-a716-446655440000
title: "Task title"
board: board-slug
column: 01-ready
---

## Description

First paragraph.

## Notes

Some notes here.
"#;

        let (_, desc) =
            parse_card_markdown(content, "550e8400-e29b-41d4-a716-446655440000").unwrap();
        assert!(desc.contains("First paragraph."));
        assert!(!desc.contains("Some notes here."));
    }

    #[test]
    fn test_parse_card_markdown_no_description_heading() {
        let content = r#"---
id: 550e8400-e29b-41d4-a716-446655440000
title: "Task title"
board: board-slug
column: 01-ready
---

Plain body text without heading.
"#;

        let (_, desc) =
            parse_card_markdown(content, "550e8400-e29b-41d4-a716-446655440000").unwrap();
        assert_eq!(desc, "Plain body text without heading.");
    }

    #[test]
    fn test_parse_card_markdown_id_mismatch() {
        let content = r#"---
id: wrong-id
title: "Task title"
board: board-slug
column: 01-ready
---

Description.
"#;

        assert!(parse_card_markdown(content, "550e8400-e29b-41d4-a716-446655440000").is_err());
    }

    #[test]
    fn test_card_markdown_frontmatter_roundtrip() {
        let meta = CardMetadata {
            id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            type_: "kanban.card".to_string(),
            board_id: "board-1".to_string(),
            column_id: "col-1".to_string(),
            title: "Test Card".to_string(),
            slug: "test-card".to_string(),
            status: "ready".to_string(),
            order: 1000,
            assignees: vec!["user-1".to_string()],
            labels: vec!["label-1".to_string()],
            due_date: None,
            priority: "high".to_string(),
            attachments_count: 1,
            checklist_done: 1,
            checklist_total: 3,
            checklists: vec![KanbanChecklistGroup {
                id: "chk-1".to_string(),
                title: "Checklist".to_string(),
                items: vec![
                    KanbanChecklistItem {
                        id: "item-1".to_string(),
                        text: "Item 1".to_string(),
                        done: true,
                    },
                    KanbanChecklistItem {
                        id: "item-2".to_string(),
                        text: "Item 2".to_string(),
                        done: false,
                    },
                ],
            }],
            archived: false,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            description_preview: Some("Preview".to_string()),
            schema_version: "2.0".to_string(),
            attachments: Some(vec![KanbanCardAttachment {
                id: "att-1".to_string(),
                name: "file.pdf".to_string(),
                size: 1024,
                mime_type: "application/pdf".to_string(),
                created_at: Utc::now(),
                created_by: "user-1".to_string(),
                path: Some("attachments/file.pdf".to_string()),
            }]),
            activity: Some(vec![KanbanEvent {
                event_type: "card.created".to_string(),
                timestamp: Utc::now(),
                actor: "user-1".to_string(),
                payload: serde_json::json!({}),
                id: Some("act-1".to_string()),
                text: Some("Created".to_string()),
            }]),
        };

        let frontmatter = CardMarkdownFrontmatter::from_metadata(&meta);
        let yaml = serde_yaml::to_string(&frontmatter).unwrap();
        let restored: CardMarkdownFrontmatter = serde_yaml::from_str(&yaml).unwrap();
        let restored_meta = restored.into_metadata();

        assert_eq!(restored_meta.id, meta.id);
        assert_eq!(restored_meta.title, meta.title);
        assert_eq!(restored_meta.board_id, meta.board_id);
        assert_eq!(restored_meta.column_id, meta.column_id);
        assert_eq!(restored_meta.priority, meta.priority);
        assert_eq!(restored_meta.order, meta.order);
        assert_eq!(restored_meta.attachments_count, meta.attachments_count);
        assert_eq!(restored_meta.checklists.len(), meta.checklists.len());
        assert_eq!(restored_meta.attachments.as_ref().unwrap().len(), 1);
        assert_eq!(restored_meta.activity.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn test_extract_description_from_markdown_body() {
        let body = "## Description\n\nFirst paragraph.\n\n## Notes\n\nSome notes.";
        let desc = extract_description_from_markdown_body(body);
        assert_eq!(desc, "First paragraph.");

        let body_no_heading = "Plain text here.";
        let desc_no_heading = extract_description_from_markdown_body(body_no_heading);
        assert_eq!(desc_no_heading, "Plain text here.");
    }

    #[test]
    fn test_fallback_json_metadata_still_loadable() {
        // This test verifies that a CardMetadata struct without the new optional fields
        // can still be deserialized (backward compatibility).
        let json = r#"{
            "id": "550e8400-e29b-41d4-a716-446655440000",
            "type": "kanban.card",
            "board_id": "board-1",
            "column_id": "col-1",
            "title": "Legacy Card",
            "slug": "legacy-card",
            "status": "ready",
            "order": 1000,
            "assignees": [],
            "labels": [],
            "priority": "medium",
            "attachments_count": 0,
            "checklist_done": 0,
            "checklist_total": 0,
            "checklists": [],
            "archived": false,
            "created_at": "2026-05-11T18:53:00Z",
            "updated_at": "2026-05-11T18:53:00Z",
            "schema_version": "1.0"
        }"#;
        let meta: CardMetadata = serde_json::from_str(json).unwrap();
        assert_eq!(meta.id, "550e8400-e29b-41d4-a716-446655440000");
        assert_eq!(meta.attachments, None);
        assert_eq!(meta.activity, None);
    }
}
