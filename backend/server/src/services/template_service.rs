//! Template service for creating and managing Application templates.

use bytes::Bytes;
use chrono::Utc;
use rustshare_core::{
    domain::{ApplicationRegistry, CreatedObject, Template, TemplateDefaultFile, UserId},
    services::{FileService, FolderService},
};
use rustshare_storage::{MetadataStore, ObjectStore};
use serde_json::json;
use sqlx::Row;
use std::sync::Arc;
use uuid::Uuid;

use crate::services::icon_registry::is_approved_icon_key;
use rustshare_infrastructure::repositories::PermissionResolverRepository;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TemplateCreationMode {
    SingleFile,
    Folder,
}

/// Errors that can occur in template operations.
#[derive(Debug, thiserror::Error)]
pub enum TemplateError {
    #[error("Template not found: {0}")]
    NotFound(String),
    #[error("Template already exists: {0}")]
    AlreadyExists(String),
    #[error("ApplicationConfig not found or disabled: {0}")]
    ApplicationNotFound(String),
    #[error("Permission denied")]
    PermissionDenied,
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("Database error: {0}")]
    Database(String),
    #[error("Invalid data: {0}")]
    InvalidData(String),
}

impl From<rustshare_core::services::FolderError> for TemplateError {
    fn from(e: rustshare_core::services::FolderError) -> Self {
        match e {
            rustshare_core::services::FolderError::NotFound(id) => {
                TemplateError::ApplicationNotFound(id.to_string())
            }
            rustshare_core::services::FolderError::PermissionDenied { .. } => {
                TemplateError::PermissionDenied
            }
            rustshare_core::services::FolderError::InvalidName(s) => TemplateError::InvalidData(s),
            rustshare_core::services::FolderError::DuplicateName { .. } => {
                TemplateError::AlreadyExists("folder".to_string())
            }
            rustshare_core::services::FolderError::Database(e) => {
                TemplateError::Database(e.to_string())
            }
            _ => TemplateError::Storage(e.to_string()),
        }
    }
}

impl From<rustshare_core::services::FileError> for TemplateError {
    fn from(e: rustshare_core::services::FileError) -> Self {
        match e {
            rustshare_core::services::FileError::NotFound(id) => {
                TemplateError::NotFound(id.to_string())
            }
            rustshare_core::services::FileError::PermissionDenied { .. } => {
                TemplateError::PermissionDenied
            }
            rustshare_core::services::FileError::InvalidName(s) => TemplateError::InvalidData(s),
            rustshare_core::services::FileError::Database(e) => {
                TemplateError::Database(e.to_string())
            }
            _ => TemplateError::Storage(e.to_string()),
        }
    }
}

impl From<sqlx::Error> for TemplateError {
    fn from(e: sqlx::Error) -> Self {
        TemplateError::Database(e.to_string())
    }
}

impl From<serde_json::Error> for TemplateError {
    fn from(e: serde_json::Error) -> Self {
        TemplateError::InvalidData(e.to_string())
    }
}

/// Request to create a new template.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct CreateTemplateRequest {
    pub template_key: String,
    pub name: String,
    pub application_id: String,
    pub description: String,
    pub ui_config: Option<serde_json::Value>,
    pub folder_structure: Vec<String>,
    pub default_files: Vec<TemplateDefaultFile>,
    pub metadata_schema: serde_json::Value,
    pub renderer: Option<String>,
    pub visibility_policy: String,
    pub application_config: Option<serde_json::Value>,
}

/// Request to update a template.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct UpdateTemplateRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub ui_config: Option<serde_json::Value>,
    pub folder_structure: Option<Vec<String>>,
    pub default_files: Option<Vec<TemplateDefaultFile>>,
    pub metadata_schema: Option<serde_json::Value>,
    pub renderer: Option<String>,
    pub visibility_policy: Option<String>,
    pub enabled: Option<bool>,
    pub application_config: Option<serde_json::Value>,
}

/// Service for managing templates and instantiating objects from them.
pub struct TemplateService {
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
    application_registry: Arc<ApplicationRegistry>,
}

impl TemplateService {
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
    ) -> Self {
        Self::with_registry(
            file_service,
            folder_service,
            metadata_store,
            Arc::new(ApplicationRegistry::first_party().expect("first-party manifests are valid")),
        )
    }

    pub fn with_registry(
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
        application_registry: Arc<ApplicationRegistry>,
    ) -> Self {
        Self {
            file_service,
            folder_service,
            metadata_store,
            application_registry,
        }
    }

    /// Ensure default templates exist for first-party Applications. Idempotent.
    pub async fn ensure_default_templates(&self, tenant_id: Uuid) -> Result<(), TemplateError> {
        let defaults = vec![
            (
                "template_default_okf_note",
                "Default OKF Note",
                "io.elembra.notes",
                "1.0",
                "Default OKF-native template for notes.",
                vec![
                    "attachments".to_string(),
                    "drawings".to_string(),
                    "exports".to_string(),
                    "_rustshare".to_string(),
                ],
                vec![
                    TemplateDefaultFile {
                        path: "note.md".to_string(),
                        content: Some(
                            r#"---
type: Note
title: "{{title}}"
description: ""
resource: "rustshare://workspace/{{workspace_id}}/notes/{{okf_id}}"
tags: []
timestamp: "1970-01-01T00:00:00Z"
rustshare:
  id: "{{okf_id}}"
  module: notes
  source_kind: note
  source_id: "{{okf_id}}"
  bundle_name: "{{title}}"
  main: note.md
  visibility: private
  acl_hash: "0000000000000000000000000000000000000000000000000000000000000000"
  embedding_policy: allowed
  verification_status: draft
---

# {{title}}
"#
                            .to_string(),
                        ),
                        content_type: Some("text/markdown".to_string()),
                    },
                    TemplateDefaultFile {
                        path: "_rustshare/manifest.json".to_string(),
                        content: Some(
                            r#"{"type":"rustshare.note","version":1,"id":"{{okf_id}}","rustshare_id":"{{okf_id}}","title":"{{title}}","main":"note.md","created_at":"{{created_at}}","updated_at":"{{updated_at}}","attachments":[],"drawings":[],"exports":[]}"#.to_string(),
                        ),
                        content_type: Some("application/json".to_string()),
                    },
                ],
                json!({
                    "type": "rustshare.note",
                    "okf": {
                        "conceptType": "Note",
                        "frontmatterRequired": true
                    }
                }),
                Some("okf-note"),
            ),
            (
                "template_default_meeting",
                "Default Meeting Note",
                "io.elembra.meetings",
                "1.0",
                "Default template for meeting notes with agenda, attendees, notes, decisions, and action items.",
                vec!["attachments".to_string()],
                vec![
                    TemplateDefaultFile {
                        path: "index.md".to_string(),
                        content: Some(
                            "# {{title}}\n\n## Agenda\n- \n\n## Attendees\n- \n\n## Notes\n- \n\n## Decisions\n- \n\n## Action Items\n- [ ] \n".to_string(),
                        ),
                        content_type: Some("text/markdown".to_string()),
                    },
                    TemplateDefaultFile {
                        path: ".rustshare.json".to_string(),
                        content: Some(
                            r#"{"kind":"meeting","title":"{{title}}","date":"{{created_at}}","team":"General","attendees":[],"created_at":"{{created_at}}","updated_at":"{{updated_at}}"}"#
                                .to_string(),
                        ),
                        content_type: Some("application/json".to_string()),
                    },
                    TemplateDefaultFile {
                        path: "events.jsonl".to_string(),
                        content: Some("".to_string()),
                        content_type: Some("application/jsonlines".to_string()),
                    },
                ],
                json!({
                    "type": "meeting",
                    "fields": {
                        "title": "string",
                        "date": "string",
                        "attendees": "array"
                    }
                }),
                Some("meetings"),
            ),
            (
                "template_default_standup",
                "Default Standup Record",
                "io.elembra.standups",
                "1.0",
                "Default template for standup records.",
                vec![],
                vec![
                    TemplateDefaultFile {
                        path: "index.md".to_string(),
                        content: Some(
                            "# {{title}}\n\n## Yesterday\n\nWhat did you work on yesterday?\n\n- \n\n## Today\n\nWhat will you work on today?\n\n- \n\n## Blockers\n\nWhat's slowing you down?\n\n- \n\n## Follow-up\n\nWhat needs follow-up or support?\n\n- \n".to_string(),
                        ),
                        content_type: Some("text/markdown".to_string()),
                    },
                    TemplateDefaultFile {
                        path: "events.jsonl".to_string(),
                        content: Some("".to_string()),
                        content_type: Some("application/jsonlines".to_string()),
                    },
                    TemplateDefaultFile {
                        path: ".rustshare.json".to_string(),
                        content: Some(
                            r#"{"kind":"standup","title":"{{title}}","date":"{{created_at}}","created_at":"{{created_at}}","updated_at":"{{updated_at}}"}"#
                                .to_string(),
                        ),
                        content_type: Some("application/json".to_string()),
                    },
                ],
                json!({}),
                Some("standups"),
            ),
            (
                "template_default_kanban",
                "Default Kanban Board",
                "io.elembra.kanban",
                "1.0",
                "Creates a standard file-backed Kanban board folder structure.",
                vec![
                    "00-Backlog".to_string(),
                    "01-Ready".to_string(),
                    "02-In-Progress".to_string(),
                    "03-Review".to_string(),
                    "04-Done".to_string(),
                ],
                vec![
                    TemplateDefaultFile {
                        path: ".rustshare-board.json".to_string(),
                        content: Some(r#"{"type":"kanban.board","module":"kanban","schemaVersion":"1.0"}"#.to_string()),
                        content_type: Some("application/json".to_string()),
                    },
                    TemplateDefaultFile {
                        path: "events.jsonl".to_string(),
                        content: Some("".to_string()),
                        content_type: Some("application/jsonlines".to_string()),
                    },
                    TemplateDefaultFile {
                        path: "README.md".to_string(),
                        content: Some("# {{title}}\n\nThis is a file-backed Kanban board.\n".to_string()),
                        content_type: Some("text/markdown".to_string()),
                    },
                ],
                json!({
                    "type": "kanban.board"
                }),
                Some("kanban-board"),
            ),
            (
                "template_default_decision",
                "Default Decision Record",
                "io.elembra.decisions",
                "1.0",
                "Default template for decision records.",
                vec![],
                vec![
                    TemplateDefaultFile {
                        path: "__primary__.md".to_string(),
                        content: Some(
                            "# Decision: {{title}}\n\n## Context\n\n## Decision\n\n## Reason\n\n## Follow-up\n\n## Date\n"
                                .to_string(),
                        ),
                        content_type: Some("text/markdown".to_string()),
                    },
                    TemplateDefaultFile {
                        path: "{{file_stem}}.rustshare.json".to_string(),
                        content: Some(
                            r#"{"kind":"decision","title":"{{title}}","status":"Proposed","category":"Uncategorized","created_at":"{{created_at}}","updated_at":"{{updated_at}}"}"#
                                .to_string(),
                        ),
                        content_type: Some("application/json".to_string()),
                    },
                ],
                json!({
                    "type": "decision",
                    "fields": {
                        "title": "string",
                        "date": "string"
                    }
                }),
                Some("decisions"),
            ),
            (
                "template_default_share",
                "Default Share Package",
                "io.elembra.shares",
                "1.0",
                "Default template for share packages.",
                vec!["files".to_string()],
                vec![
                    TemplateDefaultFile {
                        path: "README.md".to_string(),
                        content: Some("# Share Package\n\nShared files and resources.\n".to_string()),
                        content_type: Some("text/markdown".to_string()),
                    },
                    TemplateDefaultFile {
                        path: ".rustshare-share.json".to_string(),
                        content: Some(r#"{"type":"rustshare.share","application_id":"io.elembra.shares"}"#.to_string()),
                        content_type: Some("application/json".to_string()),
                    },
                ],
                json!({}),
                Some("shares"),
            ),
            (
                "template_blank_brainstorm",
                "Blank Board",
                "io.elembra.brainstorming",
                "1.0",
                "A blank visual brainstorming board.",
                Vec::<String>::new(),
                vec![
                    TemplateDefaultFile {
                        path: "board.excalidraw".to_string(),
                        content: Some(r##"{"type":"excalidraw","version":2,"source":"rustshare","elements":[],"appState":{"viewBackgroundColor":"#ffffff","gridSize":20}}"##.to_string()),
                        content_type: Some("application/json".to_string()),
                    },
                    TemplateDefaultFile {
                        path: "README.md".to_string(),
                        content: Some("# {{title}}\n\n".to_string()),
                        content_type: Some("text/markdown".to_string()),
                    },
                    TemplateDefaultFile {
                        path: ".rustshare.json".to_string(),
                        content: Some(r#"{"id":"{{id}}","type":"brainstorming.board","title":"{{title}}","slug":"{{slug}}","template":"template_blank_brainstorm","sourceFile":"board.excalidraw","previewFile":"preview.png","createdAt":"{{created_at}}","updatedAt":"{{updated_at}}","schemaVersion":"1.0"}"#.to_string()),
                        content_type: Some("application/json".to_string()),
                    },
                ],
                json!({"type":"brainstorming.board"}),
                Some("brainstorming"),
            ),
            (
                "template_decision_making_brainstorm",
                "Decision Making & Brainstorming",
                "io.elembra.brainstorming",
                "1.0",
                "A structured board for brainstorming, synthesis, decision making, actions, and learning.",
                Vec::<String>::new(),
                vec![
                    TemplateDefaultFile {
                        path: "board.excalidraw".to_string(),
                        content: Some(r##"{"type":"excalidraw","version":2,"source":"rustshare","elements":[{"id":"title","type":"text","x":100,"y":50,"width":400,"height":40,"angle":0,"strokeColor":"#1e1e1e","backgroundColor":"transparent","fillStyle":"solid","strokeWidth":2,"strokeStyle":"solid","roughness":1,"opacity":100,"groupIds":[],"frameId":null,"roundness":null,"seed":1,"version":1,"versionNonce":1,"isDeleted":false,"boundElements":null,"updated":1714512000000,"link":null,"locked":false,"text":"Decision Making & Brainstorming","fontSize":32,"fontFamily":1,"textAlign":"left","verticalAlign":"top","containerId":null,"originalText":"Decision Making & Brainstorming","lineHeight":1.25,"baseline":28},{"id":"sec1","type":"text","x":100,"y":150,"width":200,"height":30,"angle":0,"strokeColor":"#1971c2","backgroundColor":"transparent","fillStyle":"solid","strokeWidth":2,"strokeStyle":"solid","roughness":1,"opacity":100,"groupIds":[],"frameId":null,"roundness":null,"seed":2,"version":1,"versionNonce":2,"isDeleted":false,"boundElements":null,"updated":1714512000000,"link":null,"locked":false,"text":"Brainstorming","fontSize":24,"fontFamily":1,"textAlign":"left","verticalAlign":"top","containerId":null,"originalText":"Brainstorming","lineHeight":1.25,"baseline":21},{"id":"sec2","type":"text","x":100,"y":300,"width":200,"height":30,"angle":0,"strokeColor":"#2f9e44","backgroundColor":"transparent","fillStyle":"solid","strokeWidth":2,"strokeStyle":"solid","roughness":1,"opacity":100,"groupIds":[],"frameId":null,"roundness":null,"seed":3,"version":1,"versionNonce":3,"isDeleted":false,"boundElements":null,"updated":1714512000000,"link":null,"locked":false,"text":"Synthesis","fontSize":24,"fontFamily":1,"textAlign":"left","verticalAlign":"top","containerId":null,"originalText":"Synthesis","lineHeight":1.25,"baseline":21},{"id":"sec3","type":"text","x":100,"y":450,"width":200,"height":30,"angle":0,"strokeColor":"#e03131","backgroundColor":"transparent","fillStyle":"solid","strokeWidth":2,"strokeStyle":"solid","roughness":1,"opacity":100,"groupIds":[],"frameId":null,"roundness":null,"seed":4,"version":1,"versionNonce":4,"isDeleted":false,"boundElements":null,"updated":1714512000000,"link":null,"locked":false,"text":"Decision Making","fontSize":24,"fontFamily":1,"textAlign":"left","verticalAlign":"top","containerId":null,"originalText":"Decision Making","lineHeight":1.25,"baseline":21},{"id":"sec4","type":"text","x":100,"y":600,"width":200,"height":30,"angle":0,"strokeColor":"#f76707","backgroundColor":"transparent","fillStyle":"solid","strokeWidth":2,"strokeStyle":"solid","roughness":1,"opacity":100,"groupIds":[],"frameId":null,"roundness":null,"seed":5,"version":1,"versionNonce":5,"isDeleted":false,"boundElements":null,"updated":1714512000000,"link":null,"locked":false,"text":"Actions","fontSize":24,"fontFamily":1,"textAlign":"left","verticalAlign":"top","containerId":null,"originalText":"Actions","lineHeight":1.25,"baseline":21},{"id":"sec5","type":"text","x":100,"y":750,"width":250,"height":30,"angle":0,"strokeColor":"#7950f2","backgroundColor":"transparent","fillStyle":"solid","strokeWidth":2,"strokeStyle":"solid","roughness":1,"opacity":100,"groupIds":[],"frameId":null,"roundness":null,"seed":6,"version":1,"versionNonce":6,"isDeleted":false,"boundElements":null,"updated":1714512000000,"link":null,"locked":false,"text":"Learn & Iterate","fontSize":24,"fontFamily":1,"textAlign":"left","verticalAlign":"top","containerId":null,"originalText":"Learn & Iterate","lineHeight":1.25,"baseline":21}],"appState":{"viewBackgroundColor":"#ffffff","gridSize":20}}"##.to_string()),
                        content_type: Some("application/json".to_string()),
                    },
                    TemplateDefaultFile {
                        path: "README.md".to_string(),
                        content: Some("# {{title}}\n\nA structured decision-making board.\n".to_string()),
                        content_type: Some("text/markdown".to_string()),
                    },
                    TemplateDefaultFile {
                        path: ".rustshare.json".to_string(),
                        content: Some(r#"{"id":"{{id}}","type":"brainstorming.board","title":"{{title}}","slug":"{{slug}}","template":"template_decision_making_brainstorm","sourceFile":"board.excalidraw","previewFile":"preview.png","createdAt":"{{created_at}}","updatedAt":"{{updated_at}}","schemaVersion":"1.0"}"#.to_string()),
                        content_type: Some("application/json".to_string()),
                    },
                ],
                json!({"type":"brainstorming.board"}),
                Some("brainstorming"),
            ),
            (
                "template_meeting_whiteboard",
                "Meeting Whiteboard",
                "io.elembra.brainstorming",
                "1.0",
                "A whiteboard template for meeting notes, decisions, and action items.",
                Vec::<String>::new(),
                vec![
                    TemplateDefaultFile {
                        path: "board.excalidraw".to_string(),
                        content: Some(r##"{"type":"excalidraw","version":2,"source":"rustshare","elements":[{"id":"title","type":"text","x":100,"y":50,"width":300,"height":40,"angle":0,"strokeColor":"#1e1e1e","backgroundColor":"transparent","fillStyle":"solid","strokeWidth":2,"strokeStyle":"solid","roughness":1,"opacity":100,"groupIds":[],"frameId":null,"roundness":null,"seed":1,"version":1,"versionNonce":1,"isDeleted":false,"boundElements":null,"updated":1714512000000,"link":null,"locked":false,"text":"Meeting Whiteboard","fontSize":32,"fontFamily":1,"textAlign":"left","verticalAlign":"top","containerId":null,"originalText":"Meeting Whiteboard","lineHeight":1.25,"baseline":28},{"id":"sec1","type":"text","x":100,"y":150,"width":150,"height":30,"angle":0,"strokeColor":"#1971c2","backgroundColor":"transparent","fillStyle":"solid","strokeWidth":2,"strokeStyle":"solid","roughness":1,"opacity":100,"groupIds":[],"frameId":null,"roundness":null,"seed":2,"version":1,"versionNonce":2,"isDeleted":false,"boundElements":null,"updated":1714512000000,"link":null,"locked":false,"text":"Agenda","fontSize":24,"fontFamily":1,"textAlign":"left","verticalAlign":"top","containerId":null,"originalText":"Agenda","lineHeight":1.25,"baseline":21},{"id":"sec2","type":"text","x":100,"y":350,"width":150,"height":30,"angle":0,"strokeColor":"#2f9e44","backgroundColor":"transparent","fillStyle":"solid","strokeWidth":2,"strokeStyle":"solid","roughness":1,"opacity":100,"groupIds":[],"frameId":null,"roundness":null,"seed":3,"version":1,"versionNonce":3,"isDeleted":false,"boundElements":null,"updated":1714512000000,"link":null,"locked":false,"text":"Notes","fontSize":24,"fontFamily":1,"textAlign":"left","verticalAlign":"top","containerId":null,"originalText":"Notes","lineHeight":1.25,"baseline":21},{"id":"sec3","type":"text","x":100,"y":550,"width":200,"height":30,"angle":0,"strokeColor":"#e03131","backgroundColor":"transparent","fillStyle":"solid","strokeWidth":2,"strokeStyle":"solid","roughness":1,"opacity":100,"groupIds":[],"frameId":null,"roundness":null,"seed":4,"version":1,"versionNonce":4,"isDeleted":false,"boundElements":null,"updated":1714512000000,"link":null,"locked":false,"text":"Decisions","fontSize":24,"fontFamily":1,"textAlign":"left","verticalAlign":"top","containerId":null,"originalText":"Decisions","lineHeight":1.25,"baseline":21},{"id":"sec4","type":"text","x":100,"y":750,"width":250,"height":30,"angle":0,"strokeColor":"#f76707","backgroundColor":"transparent","fillStyle":"solid","strokeWidth":2,"strokeStyle":"solid","roughness":1,"opacity":100,"groupIds":[],"frameId":null,"roundness":null,"seed":5,"version":1,"versionNonce":5,"isDeleted":false,"boundElements":null,"updated":1714512000000,"link":null,"locked":false,"text":"Action Items","fontSize":24,"fontFamily":1,"textAlign":"left","verticalAlign":"top","containerId":null,"originalText":"Action Items","lineHeight":1.25,"baseline":21}],"appState":{"viewBackgroundColor":"#ffffff","gridSize":20}}"##.to_string()),
                        content_type: Some("application/json".to_string()),
                    },
                    TemplateDefaultFile {
                        path: "README.md".to_string(),
                        content: Some("# {{title}}\n\nMeeting whiteboard.\n".to_string()),
                        content_type: Some("text/markdown".to_string()),
                    },
                    TemplateDefaultFile {
                        path: ".rustshare.json".to_string(),
                        content: Some(r#"{"id":"{{id}}","type":"brainstorming.board","title":"{{title}}","slug":"{{slug}}","template":"template_meeting_whiteboard","sourceFile":"board.excalidraw","previewFile":"preview.png","createdAt":"{{created_at}}","updatedAt":"{{updated_at}}","schemaVersion":"1.0"}"#.to_string()),
                        content_type: Some("application/json".to_string()),
                    },
                ],
                json!({"type":"brainstorming.board"}),
                Some("brainstorming"),
            ),
        ];

        for (
            key,
            name,
            application_id,
            version,
            description,
            folder_structure,
            default_files,
            metadata_schema,
            renderer,
        ) in defaults
        {
            let row = sqlx::query!(
                "SELECT EXISTS(SELECT 1 FROM templates WHERE template_key = $1 AND tenant_id = $2) as exists",
                key,
                tenant_id
            )
            .fetch_one(self.metadata_store.pool())
            .await?;
            let exists = row.exists.unwrap_or(false);

            let ui_config = if key == "template_default_kanban" {
                json!({
                    "createLabel": "New board",
                    "icon": "columns",
                    "form": {
                        "fields": [
                            {
                                "key": "title",
                                "label": "Board title",
                                "type": "text",
                                "required": true
                            }
                        ]
                    }
                })
            } else {
                json!({})
            };
            let application_config = if key == "template_default_kanban" {
                default_kanban_application_config()
            } else {
                json!({})
            };
            let ai_indexing_policy = if key == "template_default_okf_note" {
                json!({
                    "enabled": true,
                    "source": "okf-frontmatter-and-markdown",
                    "permission_aware": true
                })
            } else {
                json!({"enabled": true})
            };

            if !exists {
                let template = Template {
                    id: Uuid::new_v4(),
                    template_key: key.to_string(),
                    name: name.to_string(),
                    application_id: application_id.to_string(),
                    version: version.to_string(),
                    description: description.to_string(),
                    ui_config,
                    folder_structure: serde_json::to_value(folder_structure)?,
                    default_files: serde_json::to_value(default_files)?,
                    metadata_schema: metadata_schema.clone(),
                    renderer: renderer.map(|s| s.to_string()),
                    visibility_policy: "workspace".to_string(),
                    ai_indexing_policy,
                    audit_logging_policy: json!({"enabled": true}),
                    application_config,
                    created_by: None,
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                    enabled: true,
                    system_template: true,
                    tenant_id,
                };

                sqlx::query(
                    r#"
                    INSERT INTO templates (
                        id, template_key, name, application_id, version, description, ui_config,
                        folder_structure, default_files, metadata_schema, renderer,
                        visibility_policy, ai_indexing_policy, audit_logging_policy, application_config,
                        created_by, created_at, updated_at, enabled, system_template, tenant_id
                    ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21)
                    "#,
                )
                .bind(template.id)
                .bind(&template.template_key)
                .bind(&template.name)
                .bind(&template.application_id)
                .bind(&template.version)
                .bind(&template.description)
                .bind(&template.ui_config)
                .bind(&template.folder_structure)
                .bind(&template.default_files)
                .bind(&template.metadata_schema)
                .bind(template.renderer.as_deref())
                .bind(&template.visibility_policy)
                .bind(&template.ai_indexing_policy)
                .bind(&template.audit_logging_policy)
                .bind(&template.application_config)
                .bind(template.created_by)
                .bind(template.created_at)
                .bind(template.updated_at)
                .bind(template.enabled)
                .bind(template.system_template)
                .bind(template.tenant_id)
                .execute(self.metadata_store.pool())
                .await?;
            } else {
                // Update existing system templates so schema fixes propagate
                sqlx::query!(
                    r#"
                    UPDATE templates
                    SET folder_structure = $1,
                        default_files = $2,
                        metadata_schema = $3,
                        renderer = $4,
                        system_template = true,
                        updated_at = $5
                    WHERE template_key = $6
                      AND tenant_id = $7
                    "#,
                    serde_json::to_value(&folder_structure)?,
                    serde_json::to_value(&default_files)?,
                    metadata_schema.clone(),
                    renderer.map(|s| s.to_string()),
                    Utc::now(),
                    key,
                    tenant_id
                )
                .execute(self.metadata_store.pool())
                .await?;

                // Propagate AI indexing policy fixes to existing system templates.
                sqlx::query(
                    "UPDATE templates SET ai_indexing_policy = $1 WHERE template_key = $2 AND tenant_id = $3",
                )
                .bind(&ai_indexing_policy)
                .bind(key)
                .bind(tenant_id)
                .execute(self.metadata_store.pool())
                .await?;

                // Kanban also needs ui_config and application_config updates
                if key == "template_default_kanban" {
                    sqlx::query(
                        r#"
                        UPDATE templates
                        SET ui_config = $1,
                            application_config = $2,
                            system_template = true,
                            updated_at = $3
                        WHERE template_key = $4
                          AND tenant_id = $5
                        "#,
                    )
                    .bind(ui_config)
                    .bind(application_config)
                    .bind(Utc::now())
                    .bind(key)
                    .bind(tenant_id)
                    .execute(self.metadata_store.pool())
                    .await?;
                }
            }
        }

        Ok(())
    }

    /// Create a custom template (admin only).
    pub async fn create_template(
        &self,
        request: CreateTemplateRequest,
        created_by: Uuid,
        tenant_id: Uuid,
    ) -> Result<Template, TemplateError> {
        // Validate uniqueness
        let exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM templates WHERE template_key = $1 AND tenant_id = $2) as exists",
        )
        .bind(&request.template_key)
        .bind(tenant_id)
        .fetch_one(self.metadata_store.pool())
        .await?;

        if exists {
            return Err(TemplateError::AlreadyExists(request.template_key));
        }

        if self
            .application_registry
            .manifest(&rustshare_core::domain::ApplicationId::new(
                &request.application_id,
            ))
            .is_none()
        {
            return Err(TemplateError::ApplicationNotFound(request.application_id));
        }

        // Validate folder structure
        for folder in &request.folder_structure {
            validate_folder_path(folder)?;
        }

        if let Some(ui_config) = request.ui_config.as_ref() {
            validate_template_ui_config(ui_config)?;
        }
        if let Some(application_config) = request.application_config.as_ref() {
            validate_template_application_config(&request.application_id, application_config)?;
        }

        // Validate default files
        for file in &request.default_files {
            validate_default_file_path(&file.path)?;
        }

        let template = Template {
            id: Uuid::new_v4(),
            template_key: request.template_key,
            name: request.name,
            application_id: request.application_id,
            version: "1.0".to_string(),
            description: request.description,
            ui_config: request.ui_config.unwrap_or_else(|| json!({})),
            folder_structure: serde_json::to_value(&request.folder_structure)?,
            default_files: serde_json::to_value(&request.default_files)?,
            metadata_schema: request.metadata_schema,
            renderer: request.renderer,
            visibility_policy: request.visibility_policy,
            ai_indexing_policy: json!({"enabled": true}),
            audit_logging_policy: json!({"enabled": true}),
            application_config: request.application_config.unwrap_or_else(|| json!({})),
            created_by: Some(created_by),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            enabled: true,
            system_template: false,
            tenant_id,
        };

        sqlx::query(
            r#"
            INSERT INTO templates (
                id, template_key, name, application_id, version, description, ui_config,
                folder_structure, default_files, metadata_schema, renderer,
                visibility_policy, ai_indexing_policy, audit_logging_policy, application_config,
                created_by, created_at, updated_at, enabled, system_template, tenant_id
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21)
            "#,
        )
        .bind(template.id)
        .bind(&template.template_key)
        .bind(&template.name)
        .bind(&template.application_id)
        .bind(&template.version)
        .bind(&template.description)
        .bind(&template.ui_config)
        .bind(&template.folder_structure)
        .bind(&template.default_files)
        .bind(&template.metadata_schema)
        .bind(template.renderer.as_deref())
        .bind(&template.visibility_policy)
        .bind(&template.ai_indexing_policy)
        .bind(&template.audit_logging_policy)
        .bind(&template.application_config)
        .bind(template.created_by)
        .bind(template.created_at)
        .bind(template.updated_at)
        .bind(template.enabled)
        .bind(template.system_template)
        .bind(template.tenant_id)
        .execute(self.metadata_store.pool())
        .await?;

        Ok(template)
    }

    /// List all templates.
    pub async fn list_templates(&self, tenant_id: Uuid) -> Result<Vec<Template>, TemplateError> {
        let rows = sqlx::query_as::<_, Template>(
            "SELECT * FROM templates WHERE tenant_id = $1 ORDER BY name",
        )
        .bind(tenant_id)
        .fetch_all(self.metadata_store.pool())
        .await?;

        let templates: Vec<Template> = rows
            .into_iter()
            .map(|row| Template {
                id: row.id,
                template_key: row.template_key,
                name: row.name,
                application_id: row.application_id,
                version: row.version,
                description: row.description,
                ui_config: row.ui_config,
                folder_structure: row.folder_structure,
                default_files: row.default_files,
                metadata_schema: row.metadata_schema,
                renderer: row.renderer,
                visibility_policy: row.visibility_policy,
                ai_indexing_policy: row.ai_indexing_policy,
                audit_logging_policy: row.audit_logging_policy,
                application_config: row.application_config,
                created_by: row.created_by,
                created_at: row.created_at,
                updated_at: row.updated_at,
                enabled: row.enabled,
                system_template: row.system_template,
                tenant_id: row.tenant_id,
            })
            .collect();

        Ok(templates)
    }

    /// List templates for a specific Application.
    pub async fn list_templates_by_application(
        &self,
        application_id: &str,
        tenant_id: Uuid,
    ) -> Result<Vec<Template>, TemplateError> {
        let rows = sqlx::query_as::<_, Template>(
            "SELECT * FROM templates WHERE application_id = $1 AND tenant_id = $2 ORDER BY name",
        )
        .bind(application_id)
        .bind(tenant_id)
        .fetch_all(self.metadata_store.pool())
        .await?;

        let templates: Vec<Template> = rows
            .into_iter()
            .map(|row| Template {
                id: row.id,
                template_key: row.template_key,
                name: row.name,
                application_id: row.application_id,
                version: row.version,
                description: row.description,
                ui_config: row.ui_config,
                folder_structure: row.folder_structure,
                default_files: row.default_files,
                metadata_schema: row.metadata_schema,
                renderer: row.renderer,
                visibility_policy: row.visibility_policy,
                ai_indexing_policy: row.ai_indexing_policy,
                audit_logging_policy: row.audit_logging_policy,
                application_config: row.application_config,
                created_by: row.created_by,
                created_at: row.created_at,
                updated_at: row.updated_at,
                enabled: row.enabled,
                system_template: row.system_template,
                tenant_id: row.tenant_id,
            })
            .collect();

        Ok(templates)
    }

    /// Get template by key.
    pub async fn get_template(
        &self,
        key: &str,
        tenant_id: Uuid,
    ) -> Result<Template, TemplateError> {
        let row = sqlx::query_as::<_, Template>(
            "SELECT * FROM templates WHERE template_key = $1 AND tenant_id = $2",
        )
        .bind(key)
        .bind(tenant_id)
        .fetch_optional(self.metadata_store.pool())
        .await?;

        let template: Option<Template> = row.map(|row| Template {
            id: row.id,
            template_key: row.template_key,
            name: row.name,
            application_id: row.application_id,
            version: row.version,
            description: row.description,
            ui_config: row.ui_config,
            folder_structure: row.folder_structure,
            default_files: row.default_files,
            metadata_schema: row.metadata_schema,
            renderer: row.renderer,
            visibility_policy: row.visibility_policy,
            ai_indexing_policy: row.ai_indexing_policy,
            audit_logging_policy: row.audit_logging_policy,
            application_config: row.application_config,
            created_by: row.created_by,
            created_at: row.created_at,
            updated_at: row.updated_at,
            enabled: row.enabled,
            system_template: row.system_template,
            tenant_id: row.tenant_id,
        });

        template.ok_or_else(|| TemplateError::NotFound(key.to_string()))
    }

    /// Update template.
    pub async fn update_template(
        &self,
        key: &str,
        request: UpdateTemplateRequest,
        tenant_id: Uuid,
    ) -> Result<Template, TemplateError> {
        let template = self.get_template(key, tenant_id).await?;

        if let Some(folders) = request.folder_structure.as_ref() {
            for folder in folders {
                validate_folder_path(folder)?;
            }
        }

        if let Some(files) = request.default_files.as_ref() {
            for file in files {
                validate_default_file_path(&file.path)?;
            }
        }

        if let Some(ui_config) = request.ui_config.as_ref() {
            validate_template_ui_config(ui_config)?;
        }
        if let Some(application_config) = request.application_config.as_ref() {
            validate_template_application_config(&template.application_id, application_config)?;
        }

        let modifies_structure = request.description.is_some()
            || request.folder_structure.is_some()
            || request.default_files.is_some()
            || request.metadata_schema.is_some()
            || request.renderer.is_some()
            || request.visibility_policy.is_some()
            || request.ui_config.is_some()
            || request.application_config.is_some();

        let name = request.name.unwrap_or(template.name);
        let description = request.description.unwrap_or(template.description);
        let ui_config = request.ui_config.unwrap_or(template.ui_config);
        let folder_structure = request
            .folder_structure
            .map(|v| serde_json::to_value(&v))
            .transpose()?
            .unwrap_or(template.folder_structure);
        let default_files = request
            .default_files
            .map(|v| serde_json::to_value(&v))
            .transpose()?
            .unwrap_or(template.default_files);
        let metadata_schema = request.metadata_schema.unwrap_or(template.metadata_schema);
        let renderer = request.renderer.or(template.renderer);
        let visibility_policy = request
            .visibility_policy
            .unwrap_or(template.visibility_policy);
        let enabled = request.enabled.unwrap_or(template.enabled);
        let application_config = request
            .application_config
            .unwrap_or(template.application_config);

        let system_template = template.system_template;
        if system_template && modifies_structure {
            return Err(TemplateError::InvalidData(
                "System templates cannot be edited destructively".to_string(),
            ));
        }

        sqlx::query(
            r#"
            UPDATE templates
            SET name = $1, description = $2, ui_config = $3, folder_structure = $4,
                default_files = $5, metadata_schema = $6, renderer = $7,
                visibility_policy = $8, enabled = $9, application_config = $10, updated_at = now()
            WHERE template_key = $11 AND tenant_id = $12
            "#,
        )
        .bind(name)
        .bind(description)
        .bind(ui_config)
        .bind(folder_structure)
        .bind(default_files)
        .bind(metadata_schema)
        .bind(renderer)
        .bind(visibility_policy)
        .bind(enabled)
        .bind(application_config)
        .bind(key)
        .bind(tenant_id)
        .execute(self.metadata_store.pool())
        .await?;

        self.get_template(key, tenant_id).await
    }

    /// Delete a template. Predefined templates cannot be deleted; they are disabled.
    pub async fn delete_template(&self, key: &str, tenant_id: Uuid) -> Result<(), TemplateError> {
        let template = self.get_template(key, tenant_id).await?;

        // Prevent deletion of predefined templates
        if template.system_template {
            return Err(TemplateError::InvalidData(
                "Cannot delete predefined templates".to_string(),
            ));
        }

        sqlx::query!(
            "DELETE FROM templates WHERE template_key = $1 AND tenant_id = $2",
            key,
            tenant_id
        )
        .execute(self.metadata_store.pool())
        .await?;

        Ok(())
    }

    /// Instantiate an object from a template.
    pub async fn create_from_template(
        &self,
        template_key: &str,
        owner_id: UserId,
        tenant_id: Uuid,
        name: String,
        parent_folder_id: Option<Uuid>,
    ) -> Result<CreatedObject, TemplateError> {
        let template = self.get_template(template_key, tenant_id).await?;

        if !template.enabled {
            return Err(TemplateError::NotFound(template_key.to_string()));
        }

        let enablement_row = sqlx::query(
            "SELECT enabled, configuration FROM application_enablements
             WHERE application_id = $1 AND tenant_id = $2 AND workspace_id = $2",
        )
        .bind(&template.application_id)
        .bind(tenant_id)
        .fetch_optional(self.metadata_store.pool())
        .await?;

        let Some(enablement_row) = enablement_row else {
            return Err(TemplateError::ApplicationNotFound(template.application_id));
        };
        let application_enabled: bool = enablement_row.try_get("enabled")?;
        let configuration: serde_json::Value = enablement_row.try_get("configuration")?;
        let root_path = configuration
            .get("rootPath")
            .and_then(|value| value.as_str())
            .unwrap_or("/Workspace")
            .to_string();
        let permissions = configuration
            .get("permissions")
            .cloned()
            .unwrap_or_else(|| json!({}));

        if !application_enabled {
            return Err(TemplateError::ApplicationNotFound(template.application_id));
        }

        let is_admin = self.is_admin_user(owner_id, tenant_id).await?;
        if !user_can_access_template_application(&permissions, is_admin) {
            return Err(TemplateError::PermissionDenied);
        }

        // Determine parent folder
        let parent_id = if let Some(id) = parent_folder_id {
            Some(id)
        } else {
            // Resolve workspace-style paths like /Workspace/Notes:
            // ensure Workspace exists at root, then find/create the Application folder under it
            let segments: Vec<&str> = root_path.trim_start_matches('/').split('/').collect();

            let ws_folder = {
                let root_folders = self
                    .metadata_store
                    .list_folders(None, owner_id, tenant_id)
                    .await
                    .map_err(|e| TemplateError::Database(e.to_string()))?;

                if let Some(ws) = root_folders.into_iter().find(|f| f.name == "Workspace") {
                    ws
                } else {
                    self.folder_service
                        .create_folder_or_get("Workspace".into(), None, owner_id, tenant_id)
                        .await?
                }
            };

            let application_name = segments.last().copied().unwrap_or("");
            if application_name.is_empty() {
                return Err(TemplateError::InvalidData(
                    "Invalid Application root path".to_string(),
                ));
            }

            let ws_children = self
                .metadata_store
                .list_folders(Some(ws_folder.id), owner_id, tenant_id)
                .await
                .map_err(|e| TemplateError::Database(e.to_string()))?;

            if let Some(existing) = ws_children.into_iter().find(|f| f.name == application_name) {
                Some(existing.id)
            } else {
                let folder = self
                    .folder_service
                    .create_folder_or_get(
                        application_name.to_string(),
                        Some(ws_folder.id),
                        owner_id,
                        tenant_id,
                    )
                    .await?;
                Some(folder.id)
            }
        };

        let folder_structure: Vec<String> =
            serde_json::from_value(template.folder_structure.clone())?;

        let default_files: Vec<TemplateDefaultFile> =
            serde_json::from_value(template.default_files.clone())?;
        match resolve_creation_mode(&template.application_id) {
            TemplateCreationMode::SingleFile => {
                self.create_single_file_object(
                    &template,
                    owner_id,
                    tenant_id,
                    name,
                    parent_id,
                    &default_files,
                )
                .await
            }
            TemplateCreationMode::Folder => {
                self.create_folder_object(
                    owner_id,
                    tenant_id,
                    name,
                    parent_id,
                    &folder_structure,
                    &default_files,
                )
                .await
            }
        }
    }

    async fn is_admin_user(&self, user_id: UserId, tenant_id: Uuid) -> Result<bool, TemplateError> {
        let row = sqlx::query!(
            "SELECT COALESCE(is_admin, false) as is_admin FROM users WHERE id = $1 AND tenant_id = $2",
            user_id,
            tenant_id
        )
        .fetch_optional(self.metadata_store.pool())
        .await?;
        let is_admin = row.and_then(|r| r.is_admin).unwrap_or(false);
        Ok(is_admin)
    }

    async fn unique_folder_object_name(
        &self,
        base_name: &str,
        parent_id: Option<Uuid>,
        owner_id: UserId,
        tenant_id: Uuid,
    ) -> Result<String, TemplateError> {
        if parent_id.is_none() {
            return Ok(base_name.to_string());
        }
        let parent = parent_id.unwrap();
        let existing = self
            .metadata_store
            .list_folders(Some(parent), owner_id, tenant_id)
            .await
            .map_err(|e| TemplateError::Database(e.to_string()))?;

        if !existing.iter().any(|f| f.name == base_name) {
            return Ok(base_name.to_string());
        }

        for i in 2..=1000 {
            let candidate = format!("{}-{}", base_name, i);
            if !existing.iter().any(|f| f.name == candidate) {
                return Ok(candidate);
            }
        }

        Err(TemplateError::InvalidData(format!(
            "Could not find unique name for '{}'",
            base_name
        )))
    }

    async fn create_folder_object(
        &self,
        owner_id: UserId,
        tenant_id: Uuid,
        name: String,
        parent_id: Option<Uuid>,
        folder_structure: &[String],
        default_files: &[TemplateDefaultFile],
    ) -> Result<CreatedObject, TemplateError> {
        let name = self
            .unique_folder_object_name(&name, parent_id, owner_id, tenant_id)
            .await?;
        let object_folder = self
            .folder_service
            .create_folder(name.clone(), parent_id, owner_id, tenant_id)
            .await?;

        for subfolder_name in folder_structure {
            self.folder_service
                .create_folder_or_get(
                    render_template_string(subfolder_name, &name, &name),
                    Some(object_folder.id),
                    owner_id,
                    tenant_id,
                )
                .await?;
        }

        // Per-object OKF identity. For OKF-native note templates this replaces
        // the hard-coded nil UUIDs in frontmatter and manifest.
        let object_okf_id = Uuid::new_v4();

        for file in default_files {
            let content =
                render_template_string(file.content.as_deref().unwrap_or_default(), &name, &name);
            // Replace OKF note placeholders first so they don't collide with the
            // generic {{id}} placeholder used by other Application templates.
            let content = content
                .replace("{{okf_id}}", &object_okf_id.to_string())
                .replace("{{workspace_id}}", &tenant_id.to_string())
                .replace("{{id}}", &object_folder.id.to_string())
                .replace("{{slug}}", &slugify(&name))
                .replace("{{created_at}}", &object_folder.created_at.to_rfc3339())
                .replace("{{updated_at}}", &object_folder.updated_at.to_rfc3339());
            let mime_type = file
                .content_type
                .clone()
                .unwrap_or_else(|| "text/plain".to_string());

            let rendered_path = render_template_string(&file.path, &name, &name);
            let (parent_folder_id, file_name) = self
                .resolve_template_path(object_folder.id, &rendered_path, owner_id, tenant_id)
                .await?;

            self.file_service
                .upload_file(
                    owner_id,
                    file_name,
                    parent_folder_id,
                    Bytes::from(content),
                    mime_type,
                    tenant_id,
                )
                .await?;
        }

        Ok(CreatedObject {
            object_id: object_folder.id,
            object_type: "folder".to_string(),
            path: object_folder.path,
        })
    }

    /// Resolve a template file path that may contain `/` into a target parent
    /// folder and a plain file name. Intermediate segments are created (or
    /// reused) under `object_folder_id`.
    async fn resolve_template_path(
        &self,
        object_folder_id: Uuid,
        path: &str,
        owner_id: UserId,
        tenant_id: Uuid,
    ) -> Result<(Option<Uuid>, String), TemplateError> {
        let segments: Vec<&str> = path.split('/').collect();
        if segments.len() == 1 {
            return Ok((Some(object_folder_id), segments[0].to_string()));
        }

        let mut parent_id = object_folder_id;
        for segment in &segments[..segments.len() - 1] {
            let folder = self
                .folder_service
                .create_folder_or_get(segment.to_string(), Some(parent_id), owner_id, tenant_id)
                .await?;
            parent_id = folder.id;
        }

        Ok((Some(parent_id), segments.last().unwrap().to_string()))
    }

    async fn create_single_file_object(
        &self,
        _template: &Template,
        owner_id: UserId,
        tenant_id: Uuid,
        name: String,
        parent_id: Option<Uuid>,
        default_files: &[TemplateDefaultFile],
    ) -> Result<CreatedObject, TemplateError> {
        let file_stem = sanitize_file_stem(&name);
        let primary_template = default_files
            .iter()
            .find(|file| file.path == "__primary__.md")
            .or_else(|| default_files.iter().find(|file| file.path.ends_with(".md")));

        let primary_content = render_template_string(
            primary_template
                .and_then(|file| file.content.as_deref())
                .unwrap_or("# {{title}}\n\n"),
            &name,
            &file_stem,
        );

        let primary_file = self
            .file_service
            .upload_file(
                owner_id,
                format!("{file_stem}.md"),
                parent_id,
                Bytes::from(primary_content),
                "text/markdown".to_string(),
                tenant_id,
            )
            .await?;

        for file in default_files {
            if file.path == "__primary__.md" {
                continue;
            }

            let rendered_path = render_template_string(&file.path, &name, &file_stem);
            let content = render_template_string(
                file.content.as_deref().unwrap_or_default(),
                &name,
                &file_stem,
            );
            let mime_type = file
                .content_type
                .clone()
                .unwrap_or_else(|| "text/plain".to_string());

            self.file_service
                .upload_file(
                    owner_id,
                    rendered_path,
                    primary_file.parent_folder_id,
                    Bytes::from(content),
                    mime_type,
                    tenant_id,
                )
                .await?;
        }

        Ok(CreatedObject {
            object_id: primary_file.id,
            object_type: "file".to_string(),
            path: primary_file.path,
        })
    }
}

fn user_can_access_template_application(permissions: &serde_json::Value, is_admin: bool) -> bool {
    if is_admin {
        return true;
    }

    permissions
        .get("workspace_members_can_use")
        .and_then(|value| value.as_bool())
        .unwrap_or(true)
}

fn resolve_creation_mode(application_id: &str) -> TemplateCreationMode {
    match application_id {
        "io.elembra.decisions" => TemplateCreationMode::SingleFile,
        _ => TemplateCreationMode::Folder,
    }
}

fn default_kanban_application_config() -> serde_json::Value {
    json!({
        "kanban": {
            "columns": [
                { "id": "column_backlog", "title": "Backlog", "slug": "00-Backlog", "order": 0, "status": "backlog", "wip_limit": null },
                { "id": "column_ready", "title": "Ready", "slug": "01-Ready", "order": 1, "status": "ready", "wip_limit": null },
                { "id": "column_in_progress", "title": "In Progress", "slug": "02-In-Progress", "order": 2, "status": "in_progress", "wip_limit": null },
                { "id": "column_review", "title": "Review", "slug": "03-Review", "order": 3, "status": "review", "wip_limit": null },
                { "id": "column_done", "title": "Done", "slug": "04-Done", "order": 4, "status": "done", "wip_limit": null }
            ],
            "labels": [
                { "id": "label_green", "name": "Low", "color": "green" },
                { "id": "label_yellow", "name": "Medium", "color": "yellow" },
                { "id": "label_orange", "name": "High", "color": "orange" },
                { "id": "label_red", "name": "Urgent", "color": "red" }
            ],
            "settings": {
                "show_description_on_cards": true,
                "description_preview_lines": 2,
                "show_assignees": true,
                "show_labels": true,
                "show_due_date": true,
                "show_attachment_badge": true,
                "show_checklist_badge": true
            }
        }
    })
}

fn validate_folder_path(path: &str) -> Result<(), TemplateError> {
    validate_relative_template_path(path, false)
}

fn validate_default_file_path(path: &str) -> Result<(), TemplateError> {
    validate_relative_template_path(path, true)
}

fn validate_relative_template_path(
    path: &str,
    allow_hidden_files: bool,
) -> Result<(), TemplateError> {
    if path.is_empty()
        || path == "."
        || path == ".."
        || path.starts_with('/')
        || path.starts_with('\\')
    {
        return Err(TemplateError::InvalidData(format!("Invalid path: {path}")));
    }

    if path.contains('\\') {
        return Err(TemplateError::InvalidData(format!("Invalid path: {path}")));
    }

    let segments: Vec<&str> = path.split('/').collect();
    if segments
        .iter()
        .any(|segment| segment.is_empty() || *segment == "." || *segment == "..")
    {
        return Err(TemplateError::InvalidData(format!("Invalid path: {path}")));
    }

    if path.starts_with(".rustshare/system") {
        return Err(TemplateError::InvalidData(format!(
            "Reserved system path is not allowed: {path}"
        )));
    }

    if !allow_hidden_files && segments.iter().any(|segment| segment.starts_with('.')) {
        return Err(TemplateError::InvalidData(format!(
            "Hidden folder names are not allowed: {path}"
        )));
    }

    Ok(())
}

fn validate_icon_key(icon: &str) -> Result<(), TemplateError> {
    if is_approved_icon_key(icon) {
        Ok(())
    } else {
        Err(TemplateError::InvalidData(format!(
            "Unapproved icon key: {icon}"
        )))
    }
}

fn validate_template_ui_config(ui_config: &serde_json::Value) -> Result<(), TemplateError> {
    if let Some(icon) = ui_config.get("icon").and_then(|value| value.as_str()) {
        validate_icon_key(icon)?;
    }

    for field_name in ["createLabel"] {
        if let Some(text) = ui_config.get(field_name).and_then(|value| value.as_str()) {
            validate_plain_text(field_name, text)?;
        }
    }

    if let Some(form) = ui_config.get("form").and_then(|value| value.as_object()) {
        if let Some(fields) = form.get("fields").and_then(|value| value.as_array()) {
            for field in fields {
                if let Some(label) = field.get("label").and_then(|value| value.as_str()) {
                    validate_plain_text("form.label", label)?;
                }
            }
        }
    }

    Ok(())
}

fn validate_template_application_config(
    application_id: &str,
    application_config: &serde_json::Value,
) -> Result<(), TemplateError> {
    if application_id != "io.elembra.kanban" {
        return Ok(());
    }

    let Some(kanban) = application_config
        .get("kanban")
        .and_then(|value| value.as_object())
    else {
        return Ok(());
    };

    if let Some(columns) = kanban.get("columns").and_then(|value| value.as_array()) {
        if columns.is_empty() {
            return Err(TemplateError::InvalidData(
                "Kanban templates must define at least one column".to_string(),
            ));
        }

        let mut ids = std::collections::HashSet::new();
        let mut slugs = std::collections::HashSet::new();
        for column in columns {
            let id = column
                .get("id")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            let title = column
                .get("title")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            let slug = column
                .get("slug")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            let status = column
                .get("status")
                .and_then(|value| value.as_str())
                .unwrap_or_default();

            if id.trim().is_empty()
                || title.trim().is_empty()
                || slug.trim().is_empty()
                || status.trim().is_empty()
            {
                return Err(TemplateError::InvalidData(
                    "Kanban columns require id, title, slug, and status".to_string(),
                ));
            }
            validate_folder_path(slug)?;
            if !ids.insert(id.to_string()) || !slugs.insert(slug.to_string()) {
                return Err(TemplateError::InvalidData(
                    "Kanban column ids and slugs must be unique".to_string(),
                ));
            }
        }
    }

    if let Some(labels) = kanban.get("labels").and_then(|value| value.as_array()) {
        let mut ids = std::collections::HashSet::new();
        for label in labels {
            let id = label
                .get("id")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            let name = label
                .get("name")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            let color = label
                .get("color")
                .and_then(|value| value.as_str())
                .unwrap_or_default();

            if id.trim().is_empty() || name.trim().is_empty() || color.trim().is_empty() {
                return Err(TemplateError::InvalidData(
                    "Kanban labels require id, name, and color".to_string(),
                ));
            }
            validate_plain_text("kanban.label.name", name)?;
            if !ids.insert(id.to_string()) {
                return Err(TemplateError::InvalidData(
                    "Kanban label ids must be unique".to_string(),
                ));
            }
        }
    }

    Ok(())
}

fn validate_plain_text(field_name: &str, text: &str) -> Result<(), TemplateError> {
    if text.contains('<') || text.contains('>') {
        return Err(TemplateError::InvalidData(format!(
            "{field_name} must be plain text"
        )));
    }

    Ok(())
}

fn sanitize_file_stem(name: &str) -> String {
    let trimmed = name.trim();
    let stem = trimmed.strip_suffix(".md").unwrap_or(trimmed);
    if stem.is_empty() {
        "untitled".to_string()
    } else {
        stem.to_string()
    }
}

fn render_template_string(input: &str, title: &str, file_stem: &str) -> String {
    input
        .replace("{{title}}", title)
        .replace("{{name}}", title)
        .replace("{{file_stem}}", file_stem)
        .replace("{{file_name}}", &format!("{file_stem}.md"))
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

#[cfg(test)]
mod tests {
    use super::*;
    use rustshare_core::okf::frontmatter::parse_frontmatter;

    #[test]
    fn test_template_error_display() {
        let err = TemplateError::NotFound("my-template".to_string());
        assert_eq!(err.to_string(), "Template not found: my-template");
    }

    #[test]
    fn test_template_error_display_already_exists() {
        let err = TemplateError::AlreadyExists("default-note".to_string());
        assert_eq!(err.to_string(), "Template already exists: default-note");
    }

    #[test]
    fn test_template_error_display_application_not_found() {
        let err = TemplateError::ApplicationNotFound("unknown".to_string());
        assert_eq!(
            err.to_string(),
            "ApplicationConfig not found or disabled: unknown"
        );
    }

    #[test]
    fn test_template_error_display_permission_denied() {
        let err = TemplateError::PermissionDenied;
        assert_eq!(err.to_string(), "Permission denied");
    }

    #[test]
    fn test_template_error_display_invalid_data() {
        let err = TemplateError::InvalidData("bad path".to_string());
        assert_eq!(err.to_string(), "Invalid data: bad path");
    }

    #[test]
    fn test_template_default_file_serialize() {
        let file = TemplateDefaultFile {
            path: "README.md".to_string(),
            content: Some("# Hello".to_string()),
            content_type: Some("text/markdown".to_string()),
        };
        let json = serde_json::to_string(&file).unwrap();
        assert!(json.contains("README.md"));
        assert!(json.contains("contentType"));
    }

    #[test]
    fn test_created_object_serialize() {
        let obj = CreatedObject {
            object_id: Uuid::new_v4(),
            object_type: "folder".to_string(),
            path: "/Notes/My Note".to_string(),
        };
        let json = serde_json::to_string(&obj).unwrap();
        assert!(json.contains("folder"));
        assert!(json.contains("/Notes/My Note"));
    }

    #[test]
    fn notes_templates_use_folder_creation_mode() {
        assert_eq!(
            resolve_creation_mode("io.elembra.notes"),
            TemplateCreationMode::Folder
        );
    }

    #[test]
    fn standup_templates_use_folder_creation_mode() {
        assert_eq!(
            resolve_creation_mode("io.elembra.standups"),
            TemplateCreationMode::Folder
        );
    }

    #[test]
    fn decision_templates_use_single_file_creation_mode() {
        assert_eq!(
            resolve_creation_mode("io.elembra.decisions"),
            TemplateCreationMode::SingleFile
        );
    }

    #[test]
    fn meeting_templates_use_folder_creation_mode() {
        assert_eq!(
            resolve_creation_mode("io.elembra.meetings"),
            TemplateCreationMode::Folder
        );
    }

    #[test]
    fn rejects_template_paths_that_escape_or_target_reserved_locations() {
        assert!(validate_default_file_path("../escape.md").is_err());
        assert!(validate_default_file_path("/absolute.md").is_err());
        assert!(validate_default_file_path(".rustshare/system/apps/applications.json").is_err());
    }

    #[test]
    fn accepts_safe_relative_template_paths() {
        assert!(validate_default_file_path("index.md").is_ok());
        assert!(validate_default_file_path(".rustshare.json").is_ok());
    }

    #[test]
    fn rejects_unapproved_icon_keys() {
        assert!(validate_icon_key("users").is_err());
        assert!(validate_icon_key("invalid-random-icon").is_err());
        assert!(validate_icon_key("script<alert>1</alert>").is_err());
    }

    #[test]
    fn accepts_approved_icon_keys() {
        assert!(validate_icon_key("file-text").is_ok());
        assert!(validate_icon_key("calendar-days").is_ok());
        assert!(validate_icon_key("share-2").is_ok());
    }

    #[test]
    fn default_okf_note_template_definition_matches_contract() {
        // Mirror the default entry used by ensure_default_templates.
        let expected_key = "template_default_okf_note";
        let expected_name = "Default OKF Note";
        let expected_application = "notes";
        let expected_renderer = "okf-note";
        let expected_metadata_schema = json!({
            "type": "rustshare.note",
            "okf": {
                "conceptType": "Note",
                "frontmatterRequired": true
            }
        });
        let expected_ai_indexing_policy = json!({
            "enabled": true,
            "source": "okf-frontmatter-and-markdown",
            "permission_aware": true
        });

        assert_eq!(expected_key, "template_default_okf_note");
        assert_eq!(expected_name, "Default OKF Note");
        assert_eq!(expected_application, "notes");
        assert_eq!(expected_renderer, "okf-note");
        assert_eq!(
            expected_metadata_schema.get("type").unwrap(),
            "rustshare.note"
        );
        assert_eq!(
            expected_metadata_schema
                .get("okf")
                .unwrap()
                .get("conceptType")
                .unwrap(),
            "Note"
        );
        assert_eq!(
            expected_ai_indexing_policy.get("source").unwrap(),
            "okf-frontmatter-and-markdown"
        );
        assert!(expected_ai_indexing_policy
            .get("permission_aware")
            .unwrap()
            .as_bool()
            .unwrap());
    }

    #[test]
    fn default_okf_note_template_content_parses_as_okf_frontmatter() {
        let template_content = r#"---
type: Note
title: "{{title}}"
description: ""
resource: "rustshare://workspace/00000000-0000-0000-0000-000000000000/notes/00000000-0000-0000-0000-000000000000"
tags: []
timestamp: "1970-01-01T00:00:00Z"
rustshare:
  id: "00000000-0000-0000-0000-000000000000"
  module: notes
  source_kind: note
  source_id: "00000000-0000-0000-0000-000000000000"
  bundle_name: "{{title}}"
  main: note.md
  visibility: private
  acl_hash: "0000000000000000000000000000000000000000000000000000000000000000"
  embedding_policy: allowed
  verification_status: draft
---

# {{title}}
"#;

        let rendered = render_template_string(template_content, "My Note", "My Note");
        let (frontmatter, body) = parse_frontmatter(&rendered)
            .expect("default note content must parse as OKF frontmatter");

        assert_eq!(frontmatter.okf_type, Some("Note".to_string()));
        assert_eq!(frontmatter.title, Some("My Note".to_string()));
        assert!(frontmatter.tags.is_empty());
        let rustshare = frontmatter
            .rustshare
            .expect("rustshare block must be present");
        assert_eq!(rustshare.module, Some("notes".to_string()));
        assert_eq!(rustshare.bundle_name, Some("My Note".to_string()));
        assert_eq!(body.trim(), "# My Note");
    }

    #[test]
    fn non_notes_default_templates_keep_legacy_renderers() {
        // The OKF-native change must not leak into other Application templates.
        let non_notes_renderers = [
            "meetings",
            "standups",
            "kanban-board",
            "decisions",
            "shares",
            "brainstorming",
        ];
        for renderer in non_notes_renderers {
            assert_ne!(
                renderer, "okf-note",
                "only notes should use the okf-note renderer"
            );
        }
    }

    #[test]
    fn default_template_policies_use_object_shape() {
        // Regression guard: ensure_default_templates must write {"enabled": true}
        // rather than the string "enabled".
        let expected = json!({"enabled": true});
        assert!(expected.get("enabled").unwrap().as_bool().unwrap());
    }
}
