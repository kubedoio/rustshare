//! Kanban service for file-backed Kanban boards.
//!
//! Boards are folders under /Kanban.
//! Columns are subfolders like 00-Backlog, 01-Ready, etc.
//! Cards are folders inside columns containing index.md and .rustshare-card.json.

use bytes::Bytes;
use chrono::{DateTime, Utc};
use rustshare_core::{
    domain::{File, Folder, UserId},
    services::{FileService, FolderService},
};
use rustshare_storage::{MetadataStore, ObjectStore};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::sync::Arc;
use uuid::Uuid;

use rustshare_infrastructure::repositories::PermissionResolverRepository;

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
pub struct KanbanBoardSummary {
    pub id: String,
    pub title: String,
    pub slug: String,
    pub path: String,
    pub column_count: usize,
    pub card_count: usize,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KanbanBoard {
    pub id: String,
    pub title: String,
    pub slug: String,
    pub path: String,
    pub columns: Vec<KanbanColumn>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KanbanColumn {
    pub id: String,
    pub title: String,
    pub slug: String,
    pub order: i32,
    pub status: String,
    pub cards: Vec<KanbanCard>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KanbanCard {
    pub id: String,
    pub title: String,
    pub slug: String,
    pub content: String,
    pub column_id: String,
    pub status: String,
    pub order: i32,
    pub assignees: Vec<String>,
    pub tags: Vec<String>,
    pub priority: String,
    pub archived: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateBoardInput {
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateBoardInput {
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateCardInput {
    pub title: String,
    pub column_id: Option<String>,
    pub content: Option<String>,
    pub priority: Option<String>,
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCardInput {
    pub title: Option<String>,
    pub content: Option<String>,
    pub priority: Option<String>,
    pub tags: Option<Vec<String>>,
    pub assignees: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoveCardInput {
    pub target_column_id: String,
    pub target_order: i32,
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
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ColumnDef {
    pub id: String,
    pub title: String,
    pub slug: String,
    pub order: i32,
    pub status: String,
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
    pub tags: Vec<String>,
    pub priority: String,
    pub archived: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub schema_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct KanbanEvent {
    pub event_type: String,
    pub timestamp: DateTime<Utc>,
    pub actor: String,
    pub payload: serde_json::Value,
}

// ============================================================================
// Service
// ============================================================================

pub struct KanbanService {
    file_service: Arc<
        FileService<
            rustshare_storage::EventStore,
            MetadataStore,
            ObjectStore,
            PermissionResolverRepository,
        >,
    >,
    folder_service: Arc<
        FolderService<rustshare_storage::EventStore, MetadataStore, PermissionResolverRepository>,
    >,
    metadata_store: Arc<MetadataStore>,
    object_store: Arc<ObjectStore>,
}

impl KanbanService {
    pub fn new(
        file_service: Arc<
            FileService<
                rustshare_storage::EventStore,
                MetadataStore,
                ObjectStore,
                PermissionResolverRepository,
            >,
        >,
        folder_service: Arc<
            FolderService<
                rustshare_storage::EventStore,
                MetadataStore,
                PermissionResolverRepository,
            >,
        >,
        metadata_store: Arc<MetadataStore>,
        object_store: Arc<ObjectStore>,
    ) -> Self {
        Self {
            file_service,
            folder_service,
            metadata_store,
            object_store,
        }
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
            });
        }

        boards.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
        Ok(boards)
    }

    pub async fn create_board(
        &self,
        input: CreateBoardInput,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<KanbanBoard, KanbanError> {
        let root = self.ensure_kanban_root(user_id, tenant_id).await?;
        let slug = slugify(&input.title);
        let name = if slug.is_empty() {
            "untitled-board".to_string()
        } else {
            slug.clone()
        };

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
                schema_version: "1.0".to_string(),
            };
            self.write_column_metadata(&col_folder, &col_meta, user_id, tenant_id)
                .await?;
        }

        self.get_board(board_folder.id.to_string(), user_id, tenant_id).await
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
            let folder = self.find_board_by_slug(&board_id_or_slug, user_id, tenant_id).await?
                .ok_or(KanbanError::BoardNotFound)?;
            folder.id
        };

        let board_folder = self
            .folder_service
            .get_folder(board_id, user_id)
            .await
            .map_err(KanbanError::from)?;

        let board_meta = self.load_board_metadata(&board_folder, user_id).await?;

        let mut columns = Vec::new();
        for col_def in &board_meta.columns {
            let col_path = format!("{}/{}", board_folder.path.trim_end_matches('/'), col_def.slug);
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
                if let Ok(card) = self
                    .load_card(&card_folder, &col_meta.id, user_id)
                    .await
                {
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
            created_at: board_folder.created_at,
            updated_at: board_folder.updated_at,
        })
    }

    pub async fn update_board(
        &self,
        board_id_or_slug: String,
        input: UpdateBoardInput,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<KanbanBoard, KanbanError> {
        let board_id = self.ensure_board_id(&board_id_or_slug, user_id, tenant_id).await?;
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

        self.write_board_metadata(&board_folder, &board_meta, user_id, tenant_id)
            .await?;

        self.get_board(board_id.to_string(), user_id, tenant_id).await
    }

    pub async fn archive_board(
        &self,
        board_id_or_slug: String,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<(), KanbanError> {
        let board_id = self.ensure_board_id(&board_id_or_slug, user_id, tenant_id).await?;
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
        let board_id = self.ensure_board_id(&board_id_or_slug, user_id, tenant_id).await?;
        let column_id = input.column_id.clone().ok_or_else(|| {
            KanbanError::InvalidData("column_id is required".to_string())
        })?;
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

        let col_path = format!("{}/{}", board_folder.path.trim_end_matches('/'), col_def.slug);
        let col_folder = self
            .metadata_store
            .find_folder_by_path(&col_path, board_folder.owner_id)
            .await
            .map_err(|e| KanbanError::Database(e.to_string()))?
            .ok_or_else(|| KanbanError::ColumnNotFound(column_id.clone()))?;

        let seq = self.next_card_sequence(col_folder.id, user_id).await?;
        let card_slug = slugify(&input.title);
        let card_name = format!("CARD-{:04}-{}", seq, card_slug);
        let order = (seq as i32).saturating_mul(1000);

        let card_folder = self
            .folder_service
            .create_folder(card_name.clone(), Some(col_folder.id), user_id, tenant_id)
            .await
            .map_err(KanbanError::from)?;

        let content = input.content.unwrap_or_default();
        let card_meta = CardMetadata {
            id: card_folder.id.to_string(),
            type_: "kanban.card".to_string(),
            board_id: board_folder.id.to_string(),
            column_id: col_def.id.clone(),
            title: input.title.clone(),
            slug: format!("CARD-{:04}-{}", seq, card_slug),
            status: col_def.status.clone(),
            order,
            assignees: Vec::new(),
            tags: input.tags.unwrap_or_default(),
            priority: input.priority.unwrap_or_else(|| "normal".to_string()),
            archived: false,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            schema_version: "1.0".to_string(),
        };

        self.write_card_index(&card_folder, &content, user_id, tenant_id)
            .await?;
        self.write_card_metadata(&card_folder, &card_meta, user_id, tenant_id)
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
            },
            user_id,
        )
        .await?;

        Ok(KanbanCard {
            id: card_folder.id.to_string(),
            title: card_meta.title,
            slug: card_meta.slug,
            content,
            column_id: card_meta.column_id,
            status: card_meta.status,
            order: card_meta.order,
            assignees: card_meta.assignees,
            tags: card_meta.tags,
            priority: card_meta.priority,
            archived: card_meta.archived,
            created_at: card_meta.created_at,
            updated_at: card_meta.updated_at,
        })
    }

    pub async fn get_card(
        &self,
        card_id: Uuid,
        user_id: UserId,
        _tenant_id: Uuid,
    ) -> Result<KanbanCard, KanbanError> {
        let card_folder = self
            .folder_service
            .get_folder(card_id, user_id)
            .await
            .map_err(KanbanError::from)?;

        let column_folder = if let Some(parent_id) = card_folder.parent_folder_id {
            self.folder_service
                .get_folder(parent_id, user_id)
                .await
                .map_err(KanbanError::from)?
        } else {
            return Err(KanbanError::InvalidData("Card has no parent column".to_string()));
        };

        let col_meta = self
            .load_column_metadata(&column_folder, "", user_id)
            .await?;

        self.load_card(&card_folder, &col_meta.id, user_id).await
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

        if let Some(title) = input.title {
            card_meta.title = title;
            card_meta.updated_at = Utc::now();
        }
        if let Some(content) = input.content {
            self.write_card_index(&card_folder, &content, user_id, tenant_id)
                .await?;
            card_meta.updated_at = Utc::now();
        }
        if let Some(priority) = input.priority {
            card_meta.priority = priority;
            card_meta.updated_at = Utc::now();
        }
        if let Some(tags) = input.tags {
            card_meta.tags = tags;
            card_meta.updated_at = Utc::now();
        }
        if let Some(assignees) = input.assignees {
            card_meta.assignees = assignees;
            card_meta.updated_at = Utc::now();
        }

        self.write_card_metadata(&card_folder, &card_meta, user_id, tenant_id)
            .await?;

        self.load_card(&card_folder, &card_meta.column_id, user_id).await
    }

    pub async fn move_card(
        &self,
        card_id: Uuid,
        target_column_id: String,
        target_order: i32,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<KanbanBoard, KanbanError> {
        let card_folder = self
            .folder_service
            .get_folder(card_id, user_id)
            .await
            .map_err(KanbanError::from)?;

        let mut card_meta = self.load_card_metadata(&card_folder, user_id).await?;
        let board_id = Uuid::parse_str(&card_meta.board_id)
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
            .find(|c| c.id == target_column_id)
            .ok_or_else(|| KanbanError::ColumnNotFound(target_column_id.clone()))?;

        let col_path = format!("{}/{}", board_folder.path.trim_end_matches('/'), col_def.slug);
        let col_folder = self
            .metadata_store
            .find_folder_by_path(&col_path, board_folder.owner_id)
            .await
            .map_err(|e| KanbanError::Database(e.to_string()))?
            .ok_or_else(|| KanbanError::ColumnNotFound(target_column_id.clone()))?;

        let old_column_id = card_meta.column_id.clone();
        let old_order = card_meta.order;

        // Move folder if parent changed
        if Some(col_folder.id) != card_folder.parent_folder_id {
            self.folder_service
                .move_folder(card_folder.id, Some(col_folder.id), user_id)
                .await
                .map_err(KanbanError::from)?;
        }

        // Update metadata
        card_meta.column_id = target_column_id.clone();
        card_meta.status = col_def.status.clone();
        card_meta.order = target_order;
        card_meta.updated_at = Utc::now();

        self.write_card_metadata(&card_folder, &card_meta, user_id, tenant_id)
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
                    "toColumnId": target_column_id,
                    "oldOrder": old_order,
                    "newOrder": target_order,
                }),
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
                    "toColumnId": target_column_id,
                }),
            },
            user_id,
        )
        .await?;

        self.get_board(board_id.to_string(), user_id, tenant_id).await
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
            },
            user_id,
        )
        .await?;

        self.load_card(&card_folder, &card_meta.column_id, user_id).await
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
            return Err(KanbanError::InvalidData("Card has no parent column".to_string()));
        };

        let col_meta = self
            .load_column_metadata(&column_folder, "", user_id)
            .await?;

        self.load_card(&card_folder, &col_meta.id, user_id).await
    }

    pub(crate) async fn write_card_metadata(
        &self,
        card_folder: &Folder,
        metadata: &CardMetadata,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<(), KanbanError> {
        let content = serde_json::to_string_pretty(metadata)?;
        self.write_file_by_name(
            card_folder.id,
            ".rustshare-card.json",
            &content,
            "application/json",
            user_id,
            tenant_id,
        )
        .await?;
        Ok(())
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
    ) -> Result<(), KanbanError> {
        let board_folder = self
            .folder_service
            .get_folder(board_folder_id, user_id)
            .await
            .map_err(KanbanError::from)?;

        // If board metadata already exists and looks valid, skip
        if self
            .find_file_in_folder(board_folder.id, user_id, ".rustshare-board.json")
            .await?
            .is_some()
        {
            return Ok(());
        }

        let title = board_folder.name.replace('-', " ");
        let slug = slugify(&title);
        let columns = standard_columns();

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
        };

        self.write_board_metadata(&board_folder, &board_meta, user_id, tenant_id)
            .await?;
        self.ensure_events_file(&board_folder, user_id, tenant_id)
            .await?;

        for col in &columns {
            let col_path = format!("{}/{}", board_folder.path.trim_end_matches('/'), col.slug);
            if let Some(col_folder) = self
                .metadata_store
                .find_folder_by_path(&col_path, board_folder.owner_id)
                .await
                .map_err(|e| KanbanError::Database(e.to_string()))?
            {
                let col_meta = ColumnMetadata {
                    id: col.id.clone(),
                    type_: "kanban.column".to_string(),
                    title: col.title.clone(),
                    slug: col.slug.clone(),
                    order: col.order,
                    status: col.status.clone(),
                    board_id: board_folder.id.to_string(),
                    schema_version: "1.0".to_string(),
                };
                self.write_column_metadata(&col_folder, &col_meta, user_id, tenant_id)
                    .await?;
            }
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
            let folder = self.find_board_by_slug(board_id_or_slug, user_id, tenant_id).await?
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

    async fn find_kanban_root(
        &self,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<Option<Folder>, KanbanError> {
        let row = sqlx::query(
            "SELECT id, name, path, parent_folder_id, owner_id, created_at, updated_at, starred_at, deleted_at, tenant_id FROM folders WHERE path = '/Kanban' AND tenant_id = $1 AND owner_id = $2 AND deleted_at IS NULL LIMIT 1",
        )
        .bind(tenant_id)
        .bind(user_id)
        .fetch_optional(self.metadata_store.pool())
        .await
        .map_err(|e| KanbanError::Database(e.to_string()))?;

        Ok(row.map(|row| Folder {
            id: row.get("id"),
            name: row.get("name"),
            path: row.get("path"),
            parent_folder_id: row.get("parent_folder_id"),
            owner_id: row.get("owner_id"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
            starred_at: row.get("starred_at"),
            deleted_at: row.get("deleted_at"),
            tenant_id: row.get("tenant_id"),
            ancestor_ids: None,
        }))
    }

    async fn ensure_kanban_root(
        &self,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<Folder, KanbanError> {
        if let Some(root) = self.find_kanban_root(user_id, tenant_id).await? {
            return Ok(root);
        }
        let folder = self
            .folder_service
            .create_folder("Kanban".to_string(), None, user_id, tenant_id)
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
            title: col_folder.name.replace("00-", "").replace("01-", "").replace("02-", "").replace("03-", "").replace("04-", "").replace('-', " "),
            slug: col_folder.name.clone(),
            order,
            status,
            board_id: board_id.to_string(),
            schema_version: "1.0".to_string(),
        })
    }

    async fn load_card_metadata(
        &self,
        card_folder: &Folder,
        user_id: UserId,
    ) -> Result<CardMetadata, KanbanError> {
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
        _column_id: &str,
        user_id: UserId,
    ) -> Result<KanbanCard, KanbanError> {
        let meta = self.load_card_metadata(card_folder, user_id).await?;

        let content = if let Some(file) = self
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

        Ok(KanbanCard {
            id: meta.id,
            title: meta.title,
            slug: meta.slug,
            content,
            column_id: meta.column_id,
            status: meta.status,
            order: meta.order,
            assignees: meta.assignees,
            tags: meta.tags,
            priority: meta.priority,
            archived: meta.archived,
            created_at: meta.created_at,
            updated_at: meta.updated_at,
        })
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
        self.write_file_by_name(
            card_folder.id,
            "index.md",
            content,
            "text/markdown",
            user_id,
            tenant_id,
        )
        .await?;
        Ok(())
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
        let contents = self
            .folder_service
            .list_contents(folder_id, user_id)
            .await
            .map_err(KanbanError::from)?;
        Ok(contents.files.into_iter().find(|f| f.name == name))
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
        let bytes = Bytes::from(content.to_string());
        if let Some(existing) = self.find_file_in_folder(folder_id, user_id, name).await? {
            let updated = self
                .file_service
                .update_file(existing.id, user_id, existing.current_version, bytes)
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
                    bytes,
                    mime_type.to_string(),
                    tenant_id,
                )
                .await
                .map_err(KanbanError::from)?;
            Ok(created)
        }
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
        },
        ColumnDef {
            id: "column_ready".to_string(),
            title: "Ready".to_string(),
            slug: "01-Ready".to_string(),
            order: 1,
            status: "ready".to_string(),
        },
        ColumnDef {
            id: "column_in_progress".to_string(),
            title: "In Progress".to_string(),
            slug: "02-In-Progress".to_string(),
            order: 2,
            status: "in_progress".to_string(),
        },
        ColumnDef {
            id: "column_review".to_string(),
            title: "Review".to_string(),
            slug: "03-Review".to_string(),
            order: 3,
            status: "review".to_string(),
        },
        ColumnDef {
            id: "column_done".to_string(),
            title: "Done".to_string(),
            slug: "04-Done".to_string(),
            order: 4,
            status: "done".to_string(),
        },
    ]
}

fn parse_column_slug(folder_name: &str) -> (String, i32) {
    let lower = folder_name.to_lowercase();
    match lower.as_str() {
        "00-backlog" => ("backlog".to_string(), 0),
        "01-ready" => ("ready".to_string(), 1),
        "02-in-progress" => ("in_progress".to_string(), 2),
        "03-review" => ("review".to_string(), 3),
        "04-done" => ("done".to_string(), 4),
        _ => {
            let status = lower.replace("00-", "").replace("01-", "").replace("02-", "").replace("03-", "").replace("04-", "").replace('-', "_");
            (status, 99)
        }
    }
}

fn slugify(title: &str) -> String {
    title
        .to_lowercase()
        .replace(|c: char| !c.is_alphanumeric() && c != '-' && c != '_', "-")
        .replace("--", "-")
        .replace("--", "-")
        .trim_matches('-')
        .to_string()
}
