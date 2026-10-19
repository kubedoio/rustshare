//! Meeting service for RustShare meeting notes.
//!
//! Meeting notes use a folder-per-meeting pattern:
//! /Workspace/Meetings/{team}/{yyyy}/{yyyy-mm-dd}-{slug}/
//!   index.md
//!   .rustshare.json

use anyhow::Result;
use bytes::Bytes;
use chrono::{DateTime, Datelike, Utc};
use rustshare_core::{
    domain::{Folder, UserId},
    services::{FileService, FolderService},
};
use rustshare_storage::{MetadataStore, ObjectStore};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinHandle;
use uuid::Uuid;

fn utc_now() -> DateTime<Utc> {
    Utc::now()
}

/// Meeting-specific metadata sidecar schema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct MeetingMetadata {
    #[serde(alias = "type")]
    pub kind: String,
    #[serde(default)]
    pub title: String,
    #[serde(default = "utc_now")]
    pub date: DateTime<Utc>,
    #[serde(default)]
    pub team: String,
    #[serde(default)]
    pub attendees: Vec<String>,
    /// Compatibility mirror of the canonical association row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calendar_event_id: Option<Uuid>,
    /// Source provenance for a manual copy; this does not link the copy to the event.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub copied_from_meeting_id: Option<Uuid>,
    #[serde(default = "utc_now")]
    pub created_at: DateTime<Utc>,
    #[serde(default = "utc_now")]
    pub updated_at: DateTime<Utc>,
}

impl MeetingMetadata {
    pub fn new(title: impl Into<String>, team: impl Into<String>, date: DateTime<Utc>) -> Self {
        let now = Utc::now();
        Self {
            kind: "meeting".to_string(),
            title: title.into(),
            date,
            team: team.into(),
            attendees: Vec::new(),
            calendar_event_id: None,
            copied_from_meeting_id: None,
            created_at: now,
            updated_at: now,
        }
    }
}

/// Unified meeting note payload.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct MeetingNote {
    pub id: Uuid,     // Folder ID
    pub name: String, // Folder name
    pub path: String,
    pub content: String,           // From index.md
    pub metadata: MeetingMetadata, // From .rustshare.json
    pub owner_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Meeting summary for listings.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct MeetingSummary {
    pub id: Uuid,
    pub name: String,
    pub path: String,
    pub parent_folder_id: Option<Uuid>,
    pub metadata: MeetingMetadata,
    pub modified_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct CalendarMeetingLink {
    meeting_folder_id: Uuid,
    created_event_id: Uuid,
    state: String,
    creation_event_emitted: bool,
}

struct CalendarMeetingDraft {
    event_id: Uuid,
    title: String,
    date: DateTime<Utc>,
    content: String,
}

enum CalendarMeetingClaim {
    Ready(CalendarMeetingLink),
    Creating {
        link: CalendarMeetingLink,
        lease_token: Uuid,
    },
}

/// Meeting returned by the event opener, with its durable creation-event identity.
#[derive(Debug, Clone)]
pub struct CalendarMeetingOpen {
    pub note: MeetingNote,
    pub created_event_id: Uuid,
    pub creation_event_pending: bool,
}

struct MeetingCreationLease(JoinHandle<()>);

impl Drop for MeetingCreationLease {
    fn drop(&mut self) {
        self.0.abort();
    }
}

const CALENDAR_MEETING_LEASE_SECONDS: i64 = 30;
const LEGACY_SCAN_LEASE_SECONDS: i64 = 30;
const LEGACY_SCAN_WAIT: Duration = Duration::from_secs(5);

#[derive(Debug, thiserror::Error)]
pub enum MeetingError {
    #[error("Meeting not found: {0}")]
    NotFound(Uuid),
    #[error("Permission denied")]
    PermissionDenied,
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("Database error: {0}")]
    Database(String),
    #[error("Invalid data: {0}")]
    InvalidData(String),
    #[error("Legacy meeting links are still being indexed; retry shortly")]
    RecoveryInProgress,
}

impl From<rustshare_core::services::FolderError> for MeetingError {
    fn from(e: rustshare_core::services::FolderError) -> Self {
        match e {
            rustshare_core::services::FolderError::NotFound(id) => MeetingError::NotFound(id),
            rustshare_core::services::FolderError::PermissionDenied { .. } => {
                MeetingError::PermissionDenied
            }
            rustshare_core::services::FolderError::InvalidName(s) => MeetingError::InvalidData(s),
            _ => MeetingError::Storage(e.to_string()),
        }
    }
}

impl From<rustshare_core::services::FileError> for MeetingError {
    fn from(e: rustshare_core::services::FileError) -> Self {
        MeetingError::Storage(e.to_string())
    }
}

use rustshare_infrastructure::repositories::PermissionResolverRepository;

#[derive(Clone)]
pub struct MeetingService {
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

impl MeetingService {
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

    /// Ensure the root "Meetings" folder exists under /Workspace.
    /// Ensure the canonical /Workspace/Meetings folder exists.
    ///
    /// Legacy module root policy: new writes are always directed to the
    /// canonical /Workspace/Meetings path. Legacy roots are read-only.
    async fn ensure_meetings_folder(
        &self,
        owner_id: UserId,
        tenant_id: Uuid,
    ) -> Result<Folder, MeetingError> {
        let ws = self.ensure_workspace_folder(owner_id, tenant_id).await?;
        let ws_folders = self
            .metadata_store
            .list_folders(Some(ws.id), owner_id, tenant_id)
            .await
            .map_err(|e| MeetingError::Database(e.to_string()))?;
        if let Some(folder) = ws_folders.into_iter().find(|f| f.name == "Meetings") {
            return Ok(folder);
        }
        let folder = self
            .folder_service
            .create_folder_or_get("Meetings".to_string(), Some(ws.id), owner_id, tenant_id)
            .await
            .map_err(|e| MeetingError::Storage(e.to_string()))?;
        Ok(folder)
    }

    async fn ensure_workspace_folder(
        &self,
        owner_id: UserId,
        tenant_id: Uuid,
    ) -> Result<Folder, MeetingError> {
        let folders = self
            .metadata_store
            .list_folders(None, owner_id, tenant_id)
            .await
            .map_err(|e| MeetingError::Database(e.to_string()))?;
        if let Some(ws) = folders.into_iter().find(|f| f.name == "Workspace") {
            return Ok(ws);
        }
        self.folder_service
            .create_folder_or_get("Workspace".into(), None, owner_id, tenant_id)
            .await
            .map_err(|e| MeetingError::Storage(e.to_string()))
    }

    /// Ensure path /{team}/{yyyy}/ exists under Meetings.
    async fn ensure_meeting_path(
        &self,
        root_id: Uuid,
        team: &str,
        year: i32,
        owner_id: UserId,
        tenant_id: Uuid,
    ) -> Result<Folder, MeetingError> {
        // Team folder
        let folders = self
            .metadata_store
            .list_folders(Some(root_id), owner_id, tenant_id)
            .await
            .map_err(|e| MeetingError::Database(e.to_string()))?;
        let team_folder = if let Some(f) = folders.into_iter().find(|f| f.name == team) {
            f
        } else {
            self.folder_service
                .create_folder_or_get(team.to_string(), Some(root_id), owner_id, tenant_id)
                .await
                .map_err(|e| MeetingError::Storage(e.to_string()))?
        };

        // Year folder
        let folders = self
            .metadata_store
            .list_folders(Some(team_folder.id), owner_id, tenant_id)
            .await
            .map_err(|e| MeetingError::Database(e.to_string()))?;
        let year_str = year.to_string();
        let year_folder = if let Some(f) = folders.into_iter().find(|f| f.name == year_str) {
            f
        } else {
            self.folder_service
                .create_folder_or_get(year_str, Some(team_folder.id), owner_id, tenant_id)
                .await
                .map_err(|e| MeetingError::Storage(e.to_string()))?
        };

        Ok(year_folder)
    }

    /// List all meeting notes.
    pub async fn list_meetings(
        &self,
        user_id: UserId,
        tenant_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<MeetingSummary>, MeetingError> {
        // Meetings are folders that contain .rustshare.json with kind="meeting"
        // This is expensive to scan everything.
        // Better to list all folders in the /Meetings subtree.
        let folders = self
            .metadata_store
            .list_all_folders(user_id, tenant_id)
            .await
            .map_err(|e| MeetingError::Database(e.to_string()))?;

        let mut summaries = Vec::new();
        for folder in folders {
            if !(folder.path.starts_with("/Meetings/")
                || folder.path.starts_with("/Workspace/Meetings/"))
            {
                continue;
            }

            // Check if it's a meeting folder (has .rustshare.json)
            if let Ok(Some(meta)) = self.load_metadata(folder.id, user_id, tenant_id).await {
                summaries.push(MeetingSummary {
                    id: folder.id,
                    name: folder.name,
                    path: folder.path,
                    parent_folder_id: folder.parent_folder_id,
                    metadata: meta,
                    modified_at: folder.updated_at,
                });
            }
        }

        summaries.sort_by_key(|s| std::cmp::Reverse(s.modified_at));

        let paginated: Vec<MeetingSummary> = summaries
            .into_iter()
            .skip(offset as usize)
            .take(limit as usize)
            .collect();

        Ok(paginated)
    }

    /// Create a new meeting note.
    pub async fn create_meeting(
        &self,
        owner_id: UserId,
        tenant_id: Uuid,
        title: String,
        team: String,
        date: DateTime<Utc>,
        content: String,
    ) -> Result<MeetingNote, MeetingError> {
        self.create_meeting_with_metadata(
            owner_id,
            tenant_id,
            MeetingMetadata::new(title, team, date),
            content,
        )
        .await
    }

    /// Create or reopen one event-linked note using a durable, lease-based reservation.
    /// A retry resumes the reserved folder; it never deletes that folder on write failure.
    pub async fn create_meeting_for_calendar_event(
        &self,
        owner_id: UserId,
        tenant_id: Uuid,
        event_id: Uuid,
        title: String,
        date: DateTime<Utc>,
        content: String,
    ) -> Result<CalendarMeetingOpen, MeetingError> {
        let draft = CalendarMeetingDraft {
            event_id,
            title,
            date,
            content,
        };
        loop {
            match self
                .claim_calendar_meeting(owner_id, tenant_id, draft.event_id)
                .await?
            {
                CalendarMeetingClaim::Ready(link) => {
                    match self
                        .get_meeting(link.meeting_folder_id, owner_id, tenant_id)
                        .await
                    {
                        Ok(note) => {
                            return Ok(CalendarMeetingOpen {
                                note,
                                created_event_id: link.created_event_id,
                                creation_event_pending: !link.creation_event_emitted,
                            });
                        }
                        Err(MeetingError::NotFound(_)) => {
                            self.forget_missing_calendar_meeting(
                                owner_id, tenant_id, event_id, &link,
                            )
                            .await?;
                        }
                        Err(error) => return Err(error),
                    }
                }
                CalendarMeetingClaim::Creating { link, lease_token } => {
                    return self
                        .create_or_resume_calendar_meeting(
                            owner_id,
                            tenant_id,
                            draft,
                            link,
                            lease_token,
                        )
                        .await;
                }
            }
        }
    }

    async fn claim_calendar_meeting(
        &self,
        owner_id: UserId,
        tenant_id: Uuid,
        event_id: Uuid,
    ) -> Result<CalendarMeetingClaim, MeetingError> {
        loop {
            let existing = sqlx::query_as::<_, CalendarMeetingLink>(
                "SELECT meeting_folder_id, created_event_id, state, creation_event_emitted
                   FROM calendar_event_meeting_links
                  WHERE tenant_id = $1 AND owner_id = $2 AND event_id = $3",
            )
            .bind(tenant_id)
            .bind(owner_id)
            .bind(event_id)
            .fetch_optional(self.metadata_store.pool())
            .await
            .map_err(|error| MeetingError::Database(error.to_string()))?;
            if existing.is_none() {
                self.ensure_legacy_calendar_links_scanned(owner_id, tenant_id)
                    .await?;
            }

            let lease_token = Uuid::new_v4();
            let link = sqlx::query_as::<_, CalendarMeetingLink>(
                "INSERT INTO calendar_event_meeting_links
                    (tenant_id, owner_id, event_id, meeting_folder_id, state, lease_token, lease_expires_at)
                 VALUES ($1, $2, $3, gen_random_uuid(), 'creating', $4,
                         now() + ($5 * interval '1 second'))
                 ON CONFLICT (tenant_id, owner_id, event_id) DO NOTHING
                 RETURNING meeting_folder_id, created_event_id, state, creation_event_emitted",
            )
            .bind(tenant_id)
            .bind(owner_id)
            .bind(event_id)
            .bind(lease_token)
            .bind(CALENDAR_MEETING_LEASE_SECONDS as f64)
            .fetch_optional(self.metadata_store.pool())
            .await
            .map_err(|error| MeetingError::Database(error.to_string()))?;
            if let Some(link) = link {
                return Ok(CalendarMeetingClaim::Creating { link, lease_token });
            }

            let reclaimed = sqlx::query_as::<_, CalendarMeetingLink>(
                "UPDATE calendar_event_meeting_links
                    SET lease_token = $4,
                        lease_expires_at = now() + ($5 * interval '1 second'),
                        updated_at = now()
                  WHERE tenant_id = $1 AND owner_id = $2 AND event_id = $3
                    AND state = 'creating' AND lease_expires_at <= now()
                 RETURNING meeting_folder_id, created_event_id, state, creation_event_emitted",
            )
            .bind(tenant_id)
            .bind(owner_id)
            .bind(event_id)
            .bind(lease_token)
            .bind(CALENDAR_MEETING_LEASE_SECONDS as f64)
            .fetch_optional(self.metadata_store.pool())
            .await
            .map_err(|error| MeetingError::Database(error.to_string()))?;
            if let Some(link) = reclaimed {
                return Ok(CalendarMeetingClaim::Creating { link, lease_token });
            }

            let link = sqlx::query_as::<_, CalendarMeetingLink>(
                "SELECT meeting_folder_id, created_event_id, state, creation_event_emitted
                   FROM calendar_event_meeting_links
                  WHERE tenant_id = $1 AND owner_id = $2 AND event_id = $3",
            )
            .bind(tenant_id)
            .bind(owner_id)
            .bind(event_id)
            .fetch_one(self.metadata_store.pool())
            .await
            .map_err(|error| MeetingError::Database(error.to_string()))?;
            if link.state == "ready" {
                return Ok(CalendarMeetingClaim::Ready(link));
            }

            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    async fn ensure_legacy_calendar_links_scanned(
        &self,
        owner_id: UserId,
        tenant_id: Uuid,
    ) -> Result<(), MeetingError> {
        let deadline = tokio::time::Instant::now() + LEGACY_SCAN_WAIT;
        let mut launched_scan = false;
        loop {
            if !launched_scan {
                let lease_token = Uuid::new_v4();
                let acquired = sqlx::query_scalar::<_, Uuid>(
                    "INSERT INTO calendar_event_meeting_legacy_scans
                        (tenant_id, owner_id, lease_token, lease_expires_at)
                     VALUES ($1, $2, $3, now() + ($4 * interval '1 second'))
                     ON CONFLICT (tenant_id, owner_id) DO NOTHING
                     RETURNING lease_token",
                )
                .bind(tenant_id)
                .bind(owner_id)
                .bind(lease_token)
                .bind(LEGACY_SCAN_LEASE_SECONDS as f64)
                .fetch_optional(self.metadata_store.pool())
                .await
                .map_err(|error| MeetingError::Database(error.to_string()))?
                .is_some();

                let acquired = if acquired {
                    true
                } else {
                    sqlx::query_scalar::<_, Uuid>(
                        "UPDATE calendar_event_meeting_legacy_scans
                            SET lease_token = $3, lease_expires_at = now() + ($4 * interval '1 second')
                          WHERE tenant_id = $1 AND owner_id = $2 AND completed_at IS NULL
                            AND lease_expires_at <= now()
                         RETURNING lease_token",
                    )
                    .bind(tenant_id)
                    .bind(owner_id)
                    .bind(lease_token)
                    .bind(LEGACY_SCAN_LEASE_SECONDS as f64)
                    .fetch_optional(self.metadata_store.pool())
                    .await
                    .map_err(|error| MeetingError::Database(error.to_string()))?
                    .is_some()
                };

                if acquired {
                    self.start_legacy_calendar_link_scan(owner_id, tenant_id, lease_token);
                    launched_scan = true;
                }
            }

            let scan = sqlx::query_as::<_, (Option<DateTime<Utc>>, Option<DateTime<Utc>>)>(
                "SELECT completed_at, lease_expires_at
                   FROM calendar_event_meeting_legacy_scans
                  WHERE tenant_id = $1 AND owner_id = $2",
            )
            .bind(tenant_id)
            .bind(owner_id)
            .fetch_optional(self.metadata_store.pool())
            .await
            .map_err(|error| MeetingError::Database(error.to_string()))?;
            if scan.as_ref().is_some_and(|row| row.0.is_some()) {
                return Ok(());
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(MeetingError::RecoveryInProgress);
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    fn start_legacy_calendar_link_scan(
        &self,
        owner_id: UserId,
        tenant_id: Uuid,
        lease_token: Uuid,
    ) {
        let service = self.clone();
        tokio::spawn(async move {
            let heartbeat_service = service.clone();
            let heartbeat = tokio::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(10));
                loop {
                    interval.tick().await;
                    let updated = sqlx::query(
                        "UPDATE calendar_event_meeting_legacy_scans
                            SET lease_expires_at = now() + ($4 * interval '1 second')
                          WHERE tenant_id = $1 AND owner_id = $2 AND lease_token = $3
                            AND completed_at IS NULL",
                    )
                    .bind(tenant_id)
                    .bind(owner_id)
                    .bind(lease_token)
                    .bind(LEGACY_SCAN_LEASE_SECONDS as f64)
                    .execute(heartbeat_service.metadata_store.pool())
                    .await;
                    if !matches!(updated, Ok(result) if result.rows_affected() == 1) {
                        break;
                    }
                }
            });

            let imported = service
                .import_legacy_calendar_links(owner_id, tenant_id)
                .await;
            heartbeat.abort();
            if let Err(error) = imported {
                tracing::warn!(%owner_id, %tenant_id, %error, "legacy meeting link scan failed");
                let _ = sqlx::query(
                    "UPDATE calendar_event_meeting_legacy_scans
                        SET lease_expires_at = now() + interval '10 seconds'
                      WHERE tenant_id = $1 AND owner_id = $2 AND lease_token = $3",
                )
                .bind(tenant_id)
                .bind(owner_id)
                .bind(lease_token)
                .execute(service.metadata_store.pool())
                .await;
                return;
            }

            if let Err(error) = sqlx::query(
                "UPDATE calendar_event_meeting_legacy_scans
                    SET completed_at = now(), lease_token = NULL, lease_expires_at = NULL
                  WHERE tenant_id = $1 AND owner_id = $2 AND lease_token = $3
                    AND completed_at IS NULL",
            )
            .bind(tenant_id)
            .bind(owner_id)
            .bind(lease_token)
            .execute(service.metadata_store.pool())
            .await
            {
                tracing::warn!(%owner_id, %tenant_id, %error, "could not finish legacy meeting link scan");
            }
        });
    }

    async fn import_legacy_calendar_links(
        &self,
        owner_id: UserId,
        tenant_id: Uuid,
    ) -> Result<(), MeetingError> {
        let folders = self
            .metadata_store
            .list_all_folders(owner_id, tenant_id)
            .await
            .map_err(|error| MeetingError::Database(error.to_string()))?;
        let mut legacy = HashMap::<Uuid, (DateTime<Utc>, Uuid)>::new();
        for folder in folders.into_iter().filter(|folder| {
            folder.path.starts_with("/Meetings/") || folder.path.starts_with("/Workspace/Meetings/")
        }) {
            let metadata = match self.load_metadata(folder.id, owner_id, tenant_id).await {
                Ok(Some(metadata)) => metadata,
                Ok(None) | Err(MeetingError::InvalidData(_)) => continue,
                Err(error) => return Err(error),
            };
            let Some(event_id) = metadata.calendar_event_id else {
                continue;
            };
            let candidate = (metadata.created_at, folder.id);
            if legacy
                .get(&event_id)
                .is_none_or(|current| candidate < *current)
            {
                legacy.insert(event_id, candidate);
            }
        }

        for (event_id, (_, meeting_folder_id)) in legacy {
            sqlx::query(
                "INSERT INTO calendar_event_meeting_links
                    (tenant_id, owner_id, event_id, meeting_folder_id, state,
                     creation_event_emitted)
                 VALUES ($1, $2, $3, $4, 'ready', true)
                 ON CONFLICT DO NOTHING",
            )
            .bind(tenant_id)
            .bind(owner_id)
            .bind(event_id)
            .bind(meeting_folder_id)
            .execute(self.metadata_store.pool())
            .await
            .map_err(|error| MeetingError::Database(error.to_string()))?;
        }
        Ok(())
    }

    async fn forget_missing_calendar_meeting(
        &self,
        owner_id: UserId,
        tenant_id: Uuid,
        event_id: Uuid,
        link: &CalendarMeetingLink,
    ) -> Result<(), MeetingError> {
        sqlx::query(
            "DELETE FROM calendar_event_meeting_links
              WHERE tenant_id = $1 AND owner_id = $2 AND event_id = $3
                AND meeting_folder_id = $4 AND state = 'ready'",
        )
        .bind(tenant_id)
        .bind(owner_id)
        .bind(event_id)
        .bind(link.meeting_folder_id)
        .execute(self.metadata_store.pool())
        .await
        .map_err(|error| MeetingError::Database(error.to_string()))?;
        Ok(())
    }

    async fn create_or_resume_calendar_meeting(
        &self,
        owner_id: UserId,
        tenant_id: Uuid,
        draft: CalendarMeetingDraft,
        link: CalendarMeetingLink,
        lease_token: Uuid,
    ) -> Result<CalendarMeetingOpen, MeetingError> {
        let heartbeat =
            self.start_calendar_meeting_heartbeat(owner_id, tenant_id, draft.event_id, lease_token);
        let result = self
            .write_calendar_meeting(owner_id, tenant_id, &draft, link.meeting_folder_id)
            .await;
        drop(heartbeat);
        let note = match result {
            Ok(note) => note,
            Err(error) => {
                self.release_calendar_meeting_lease(
                    owner_id,
                    tenant_id,
                    draft.event_id,
                    lease_token,
                )
                .await;
                return Err(error);
            }
        };

        let finalized = sqlx::query(
            "UPDATE calendar_event_meeting_links
                SET state = 'ready', lease_token = NULL, lease_expires_at = NULL, updated_at = now()
              WHERE tenant_id = $1 AND owner_id = $2 AND event_id = $3
                AND state = 'creating' AND lease_token = $4",
        )
        .bind(tenant_id)
        .bind(owner_id)
        .bind(draft.event_id)
        .bind(lease_token)
        .execute(self.metadata_store.pool())
        .await
        .map_err(|error| MeetingError::Database(error.to_string()))?;

        if finalized.rows_affected() == 1 {
            return Ok(CalendarMeetingOpen {
                note,
                created_event_id: link.created_event_id,
                creation_event_pending: !link.creation_event_emitted,
            });
        }

        loop {
            match self
                .claim_calendar_meeting(owner_id, tenant_id, draft.event_id)
                .await?
            {
                CalendarMeetingClaim::Ready(current) => {
                    let note = self
                        .get_meeting(current.meeting_folder_id, owner_id, tenant_id)
                        .await?;
                    return Ok(CalendarMeetingOpen {
                        note,
                        created_event_id: current.created_event_id,
                        creation_event_pending: !current.creation_event_emitted,
                    });
                }
                CalendarMeetingClaim::Creating {
                    link: current,
                    lease_token,
                } => {
                    let finalized = sqlx::query(
                        "UPDATE calendar_event_meeting_links
                            SET state = 'ready', lease_token = NULL, lease_expires_at = NULL, updated_at = now()
                          WHERE tenant_id = $1 AND owner_id = $2 AND event_id = $3
                            AND state = 'creating' AND lease_token = $4",
                    )
                    .bind(tenant_id)
                    .bind(owner_id)
                    .bind(draft.event_id)
                    .bind(lease_token)
                    .execute(self.metadata_store.pool())
                    .await
                    .map_err(|error| MeetingError::Database(error.to_string()))?;
                    if finalized.rows_affected() == 1 {
                        return Ok(CalendarMeetingOpen {
                            note,
                            created_event_id: current.created_event_id,
                            creation_event_pending: !current.creation_event_emitted,
                        });
                    }
                }
            }
        }
    }

    pub async fn mark_calendar_meeting_creation_event_emitted(
        &self,
        owner_id: UserId,
        tenant_id: Uuid,
        event_id: Uuid,
        created_event_id: Uuid,
    ) -> Result<(), MeetingError> {
        sqlx::query(
            "UPDATE calendar_event_meeting_links
                SET creation_event_emitted = true, updated_at = now()
              WHERE tenant_id = $1 AND owner_id = $2 AND event_id = $3
                AND created_event_id = $4 AND state = 'ready'",
        )
        .bind(tenant_id)
        .bind(owner_id)
        .bind(event_id)
        .bind(created_event_id)
        .execute(self.metadata_store.pool())
        .await
        .map_err(|error| MeetingError::Database(error.to_string()))?;
        Ok(())
    }

    fn start_calendar_meeting_heartbeat(
        &self,
        owner_id: UserId,
        tenant_id: Uuid,
        event_id: Uuid,
        lease_token: Uuid,
    ) -> MeetingCreationLease {
        let metadata_store = Arc::clone(&self.metadata_store);
        MeetingCreationLease(tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(10));
            interval.tick().await;
            loop {
                interval.tick().await;
                let result = sqlx::query(
                    "UPDATE calendar_event_meeting_links
                        SET lease_expires_at = now() + ($5 * interval '1 second'), updated_at = now()
                      WHERE tenant_id = $1 AND owner_id = $2 AND event_id = $3
                        AND state = 'creating' AND lease_token = $4",
                )
                .bind(tenant_id)
                .bind(owner_id)
                .bind(event_id)
                .bind(lease_token)
                .bind(CALENDAR_MEETING_LEASE_SECONDS as f64)
                .execute(metadata_store.pool())
                .await;
                if !matches!(result, Ok(query) if query.rows_affected() == 1) {
                    break;
                }
            }
        }))
    }

    async fn release_calendar_meeting_lease(
        &self,
        owner_id: UserId,
        tenant_id: Uuid,
        event_id: Uuid,
        lease_token: Uuid,
    ) {
        let _ = sqlx::query(
            "UPDATE calendar_event_meeting_links
                SET lease_expires_at = now(), updated_at = now()
              WHERE tenant_id = $1 AND owner_id = $2 AND event_id = $3
                AND state = 'creating' AND lease_token = $4",
        )
        .bind(tenant_id)
        .bind(owner_id)
        .bind(event_id)
        .bind(lease_token)
        .execute(self.metadata_store.pool())
        .await;
    }

    async fn write_calendar_meeting(
        &self,
        owner_id: UserId,
        tenant_id: Uuid,
        draft: &CalendarMeetingDraft,
        folder_id: Uuid,
    ) -> Result<MeetingNote, MeetingError> {
        let root = self.ensure_meetings_folder(owner_id, tenant_id).await?;
        let parent = self
            .ensure_meeting_path(root.id, "General", draft.date.year(), owner_id, tenant_id)
            .await?;
        let folder = match self.folder_service.get_folder(folder_id, owner_id).await {
            Ok(folder)
                if folder.tenant_id == tenant_id
                    && folder.owner_id == owner_id
                    && folder.parent_folder_id == Some(parent.id) =>
            {
                folder
            }
            Ok(_) => return Err(MeetingError::PermissionDenied),
            Err(rustshare_core::services::FolderError::NotFound(_)) => {
                let slug = slug::slugify(&draft.title);
                let slug: String = slug.chars().take(120).collect();
                let name = format!(
                    "{}-{}-{}",
                    draft.date.format("%Y-%m-%d"),
                    slug,
                    folder_id.simple()
                );
                self.folder_service
                    .create_folder_with_id(folder_id, name, Some(parent.id), owner_id, tenant_id)
                    .await?
            }
            Err(error) => return Err(error.into()),
        };

        let files = self
            .metadata_store
            .list_files(Some(folder.id), owner_id, tenant_id)
            .await
            .map_err(|error| MeetingError::Database(error.to_string()))?;
        if !files.iter().any(|file| file.name == "index.md") {
            self.file_service
                .upload_file(
                    owner_id,
                    "index.md".into(),
                    Some(folder.id),
                    Bytes::from(draft.content.clone()),
                    "text/markdown".into(),
                    tenant_id,
                )
                .await?;
        }
        let metadata = self.load_metadata(folder.id, owner_id, tenant_id).await?;
        if let Some(metadata) = metadata {
            if metadata.calendar_event_id != Some(draft.event_id) {
                return Err(MeetingError::InvalidData(
                    "reserved meeting folder has different calendar provenance".into(),
                ));
            }
        } else {
            let metadata = MeetingMetadata {
                calendar_event_id: Some(draft.event_id),
                ..MeetingMetadata::new(draft.title.clone(), "General", draft.date)
            };
            let bytes = serde_json::to_vec_pretty(&metadata)
                .map_err(|error| MeetingError::InvalidData(error.to_string()))?;
            if files.iter().any(|file| file.name == ".rustshare.json") {
                return Err(MeetingError::InvalidData(
                    "reserved meeting folder contains an unreadable metadata sidecar".into(),
                ));
            }
            self.file_service
                .upload_file(
                    owner_id,
                    ".rustshare.json".into(),
                    Some(folder.id),
                    Bytes::from(bytes),
                    "application/json".into(),
                    tenant_id,
                )
                .await?;
        }
        self.get_meeting(folder.id, owner_id, tenant_id).await
    }

    async fn create_meeting_with_metadata(
        &self,
        owner_id: UserId,
        tenant_id: Uuid,
        meta: MeetingMetadata,
        content: String,
    ) -> Result<MeetingNote, MeetingError> {
        let title = &meta.title;
        let team = &meta.team;
        let date = meta.date;
        let root = self.ensure_meetings_folder(owner_id, tenant_id).await?;
        let parent = self
            .ensure_meeting_path(root.id, team, date.year(), owner_id, tenant_id)
            .await?;

        // Folder name: yyyy-mm-dd-slug
        let slug = slug::slugify(title);
        let folder_name = format!(
            "{}-{}-{}",
            date.format("%Y-%m-%d"),
            slug,
            &Uuid::new_v4().to_string()[0..8]
        );

        let meeting_folder = self
            .folder_service
            .create_folder(folder_name, Some(parent.id), owner_id, tenant_id)
            .await
            .map_err(|e| MeetingError::Storage(e.to_string()))?;

        let create_files = async {
            self.file_service
                .upload_file(
                    owner_id,
                    "index.md".to_string(),
                    Some(meeting_folder.id),
                    Bytes::from(content.clone()),
                    "text/markdown".to_string(),
                    tenant_id,
                )
                .await?;

            let meta_data = serde_json::to_vec_pretty(&meta)
                .map_err(|e| MeetingError::InvalidData(e.to_string()))?;
            self.file_service
                .upload_file(
                    owner_id,
                    ".rustshare.json".to_string(),
                    Some(meeting_folder.id),
                    Bytes::from(meta_data),
                    "application/json".to_string(),
                    tenant_id,
                )
                .await?;
            Ok::<(), MeetingError>(())
        }
        .await;

        if let Err(error) = create_files {
            if let Err(rollback_error) = self
                .folder_service
                .delete_folder(meeting_folder.id, owner_id)
                .await
            {
                return Err(MeetingError::Storage(format!(
                    "{error}; failed to remove partial meeting folder: {rollback_error}"
                )));
            }
            return Err(error);
        }

        Ok(MeetingNote {
            id: meeting_folder.id,
            name: meeting_folder.name,
            path: meeting_folder.path,
            content,
            metadata: meta,
            owner_id,
            created_at: meeting_folder.created_at,
            updated_at: meeting_folder.updated_at,
        })
    }

    async fn load_metadata(
        &self,
        folder_id: Uuid,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<Option<MeetingMetadata>, MeetingError> {
        let files = self
            .metadata_store
            .list_files(Some(folder_id), user_id, tenant_id)
            .await
            .map_err(|e| MeetingError::Database(e.to_string()))?;

        if let Some(sidecar) = files.into_iter().find(|f| f.name == ".rustshare.json") {
            let data = self
                .object_store
                .get(&sidecar.storage_key())
                .await
                .map_err(|e| MeetingError::Storage(e.to_string()))?;
            let meta: MeetingMetadata = match serde_json::from_slice(&data) {
                Ok(m) => m,
                Err(parse_err) => {
                    // Fallback: old templates wrote {"type":"rustshare.module","application_id":"meetings"}
                    if let Ok(marker) = serde_json::from_slice::<serde_json::Value>(&data) {
                        if marker.get("type").and_then(|v| v.as_str()) == Some("rustshare.module")
                            && marker.get("application_id").and_then(|v| v.as_str())
                                == Some("meetings")
                        {
                            MeetingMetadata {
                                kind: "meeting".to_string(),
                                title: marker
                                    .get("title")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string(),
                                date: Utc::now(),
                                team: "General".to_string(),
                                attendees: Vec::new(),
                                calendar_event_id: None,
                                copied_from_meeting_id: None,
                                created_at: Utc::now(),
                                updated_at: Utc::now(),
                            }
                        } else {
                            return Err(MeetingError::InvalidData(parse_err.to_string()));
                        }
                    } else {
                        return Err(MeetingError::InvalidData(parse_err.to_string()));
                    }
                }
            };
            if meta.kind == "meeting" {
                return Ok(Some(meta));
            }
        }
        Ok(None)
    }

    pub async fn get_meeting(
        &self,
        id: Uuid,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<MeetingNote, MeetingError> {
        let folder = self.folder_service.get_folder(id, user_id).await?;
        if folder.tenant_id != tenant_id {
            return Err(MeetingError::PermissionDenied);
        }
        let files = self
            .metadata_store
            .list_files_by_parent(Some(id), folder.tenant_id)
            .await
            .map_err(|e| MeetingError::Database(e.to_string()))?;

        // Load index.md
        let content = if let Some(index_file) = files.iter().find(|f| f.name == "index.md") {
            let bytes = self
                .object_store
                .get(&index_file.storage_key())
                .await
                .map_err(|e| MeetingError::Storage(e.to_string()))?;
            String::from_utf8_lossy(&bytes).to_string()
        } else {
            String::new()
        };

        // Load metadata (gracefully fall back to defaults if sidecar is missing/invalid)
        let meta = self
            .load_metadata(id, user_id, folder.tenant_id)
            .await?
            .unwrap_or_else(|| MeetingMetadata {
                kind: "meeting".to_string(),
                title: folder.name.clone(),
                date: folder.created_at,
                team: String::new(),
                attendees: vec![],
                calendar_event_id: None,
                copied_from_meeting_id: None,
                created_at: folder.created_at,
                updated_at: folder.updated_at,
            });

        Ok(MeetingNote {
            id: folder.id,
            name: folder.name,
            path: folder.path,
            content,
            metadata: meta,
            owner_id: folder.owner_id,
            created_at: folder.created_at,
            updated_at: folder.updated_at,
        })
    }

    pub async fn delete_meeting(
        &self,
        id: Uuid,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<(), MeetingError> {
        let folder = self.folder_service.get_folder(id, user_id).await?;
        if folder.tenant_id != tenant_id {
            return Err(MeetingError::PermissionDenied);
        }
        self.folder_service
            .delete_folder(id, user_id)
            .await
            .map_err(MeetingError::from)?;
        sqlx::query(
            "DELETE FROM calendar_event_meeting_links
              WHERE tenant_id = $1 AND owner_id = $2 AND meeting_folder_id = $3",
        )
        .bind(tenant_id)
        .bind(user_id)
        .bind(id)
        .execute(self.metadata_store.pool())
        .await
        .map_err(|error| MeetingError::Database(error.to_string()))?;
        Ok(())
    }

    pub async fn update_meeting(
        &self,
        id: Uuid,
        user_id: UserId,
        tenant_id: Uuid,
        title: Option<String>,
        content: Option<String>,
        attendees: Option<Vec<String>>,
    ) -> Result<MeetingNote, MeetingError> {
        let folder = self.folder_service.get_folder(id, user_id).await?;
        if folder.tenant_id != tenant_id {
            return Err(MeetingError::PermissionDenied);
        }
        let mut meta = self
            .load_metadata(id, user_id, folder.tenant_id)
            .await?
            .unwrap_or_else(|| MeetingMetadata {
                kind: "meeting".to_string(),
                title: folder.name.clone(),
                date: folder.created_at,
                team: String::new(),
                attendees: vec![],
                calendar_event_id: None,
                copied_from_meeting_id: None,
                created_at: folder.created_at,
                updated_at: folder.updated_at,
            });

        if let Some(t) = title {
            meta.title = t;
        }
        if let Some(a) = attendees {
            meta.attendees = a;
        }
        meta.updated_at = Utc::now();

        // Update sidecar
        let files = self
            .metadata_store
            .list_files_by_parent(Some(id), folder.tenant_id)
            .await
            .map_err(|e| MeetingError::Database(e.to_string()))?;
        if let Some(sidecar) = files.iter().find(|f| f.name == ".rustshare.json") {
            let meta_data = serde_json::to_vec_pretty(&meta)
                .map_err(|e| MeetingError::InvalidData(e.to_string()))?;
            self.file_service
                .edit_file(
                    sidecar.id,
                    user_id,
                    Bytes::from(meta_data),
                    "overwrite",
                    None,
                )
                .await?;
        }

        // Update content
        if let Some(c) = content {
            if let Some(index_file) = files.iter().find(|f| f.name == "index.md") {
                self.file_service
                    .edit_file(index_file.id, user_id, Bytes::from(c), "overwrite", None)
                    .await?;
            }
        }

        self.get_meeting(id, user_id, tenant_id).await
    }

    /// Move a meeting folder to a different parent folder.
    pub async fn move_meeting(
        &self,
        id: Uuid,
        user_id: UserId,
        tenant_id: Uuid,
        target_folder_id: Option<Uuid>,
    ) -> Result<MeetingNote, MeetingError> {
        let folder = self.folder_service.get_folder(id, user_id).await?;
        if folder.tenant_id != tenant_id {
            return Err(MeetingError::PermissionDenied);
        }
        self.folder_service
            .move_folder(id, target_folder_id, user_id)
            .await?;
        self.get_meeting(id, user_id, tenant_id).await
    }

    /// Duplicate a meeting in the same parent folder with a regenerated folder name.
    pub async fn duplicate_meeting(
        &self,
        id: Uuid,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<MeetingNote, MeetingError> {
        let original = self.get_meeting(id, user_id, tenant_id).await?;
        let original_folder = self.folder_service.get_folder(id, user_id).await?;
        if original_folder.tenant_id != tenant_id {
            return Err(MeetingError::PermissionDenied);
        }

        let parent_folder_id = original_folder.parent_folder_id;
        let copy_title = format!("{} (copy)", original.metadata.title);
        let new_folder_name =
            Self::unique_meeting_folder_name(&original.metadata.date, &copy_title);

        let new_folder = self
            .folder_service
            .create_folder(new_folder_name, parent_folder_id, user_id, tenant_id)
            .await?;

        // Copy index.md
        self.file_service
            .upload_file(
                user_id,
                "index.md".to_string(),
                Some(new_folder.id),
                Bytes::from(original.content),
                "text/markdown".to_string(),
                tenant_id,
            )
            .await?;

        // Copy .rustshare.json with updated title/timestamps
        let mut meta = original.metadata.clone();
        meta.title = copy_title;
        meta.created_at = Utc::now();
        meta.updated_at = Utc::now();
        meta.calendar_event_id = None;
        meta.copied_from_meeting_id = Some(original.id);
        let meta_data = serde_json::to_vec_pretty(&meta)
            .map_err(|e| MeetingError::InvalidData(e.to_string()))?;
        self.file_service
            .upload_file(
                user_id,
                ".rustshare.json".to_string(),
                Some(new_folder.id),
                Bytes::from(meta_data),
                "application/json".to_string(),
                tenant_id,
            )
            .await?;

        self.get_meeting(new_folder.id, user_id, tenant_id).await
    }

    fn unique_meeting_folder_name(date: &DateTime<Utc>, title: &str) -> String {
        let slug = slug::slugify(title);
        format!(
            "{}-{}-{}",
            date.format("%Y-%m-%d"),
            slug,
            &Uuid::new_v4().to_string()[0..8]
        )
    }
}
