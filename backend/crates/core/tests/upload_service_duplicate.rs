use std::sync::{Arc, Mutex};

use anyhow::Result;
use bytes::Bytes;
use chrono::Utc;
use rustshare_core::domain::{File, FileVersion, Folder, UserId};
use rustshare_core::events::{Event, EventBroadcaster, EventType};
use rustshare_core::services::{
    ChunkInfo, FileEventStoreOps, UploadError, UploadMetadataStore, UploadObjectStore,
    UploadService, UploadSession, UploadSessionRepository, UploadSessionStatus,
};
use uuid::Uuid;

struct MockUploadRepo {
    session: Mutex<Option<UploadSession>>,
    completed: Mutex<Vec<(Uuid, Uuid)>>,
}

impl MockUploadRepo {
    fn new(session: UploadSession) -> Self {
        Self {
            session: Mutex::new(Some(session)),
            completed: Mutex::new(Vec::new()),
        }
    }
}

impl UploadSessionRepository for MockUploadRepo {
    async fn create_session(&self, _session: &UploadSession) -> Result<(), UploadError> {
        unreachable!()
    }

    async fn get_session(&self, _id: Uuid) -> Result<Option<UploadSession>, UploadError> {
        Ok(self.session.lock().unwrap().clone())
    }

    async fn update_session(&self, session: &UploadSession) -> Result<(), UploadError> {
        *self.session.lock().unwrap() = Some(session.clone());
        Ok(())
    }

    async fn update_chunk_received(
        &self,
        _session_id: Uuid,
        _chunk_index: u32,
        _chunk_hash: &str,
        _size: u64,
    ) -> Result<(), UploadError> {
        unreachable!()
    }

    async fn get_chunk_info(
        &self,
        _session_id: Uuid,
        _chunk_index: u32,
    ) -> Result<Option<ChunkInfo>, UploadError> {
        Ok(None)
    }

    async fn complete_session(&self, session_id: Uuid, file_id: Uuid) -> Result<(), UploadError> {
        self.completed.lock().unwrap().push((session_id, file_id));
        if let Some(session) = self.session.lock().unwrap().as_mut() {
            session.file_id = Some(file_id);
            session.status = UploadSessionStatus::Completed;
        }
        Ok(())
    }

    async fn abort_session(&self, _session_id: Uuid) -> Result<(), UploadError> {
        unreachable!()
    }

    async fn delete_session(&self, _session_id: Uuid) -> Result<(), UploadError> {
        Ok(())
    }

    async fn list_expired_sessions(
        &self,
        _before: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<UploadSession>, UploadError> {
        Ok(Vec::new())
    }

    async fn list_user_sessions(
        &self,
        _user_id: UserId,
    ) -> Result<Vec<UploadSession>, UploadError> {
        Ok(Vec::new())
    }
}

struct MockUploadObjectStore;

impl UploadObjectStore for MockUploadObjectStore {
    async fn put_chunk(
        &self,
        _session_id: Uuid,
        _chunk_index: u32,
        _data: Bytes,
    ) -> Result<(), UploadError> {
        unreachable!()
    }

    async fn put_chunk_from_path(
        &self,
        _session_id: Uuid,
        _chunk_index: u32,
        _path: &std::path::Path,
    ) -> Result<(), UploadError> {
        unreachable!()
    }

    async fn get_chunk(
        &self,
        _session_id: Uuid,
        _chunk_index: u32,
    ) -> Result<Option<Bytes>, UploadError> {
        Ok(Some(Bytes::new()))
    }

    async fn delete_chunk(&self, _session_id: Uuid, _chunk_index: u32) -> Result<(), UploadError> {
        Ok(())
    }

    async fn delete_session_chunks(
        &self,
        _session_id: Uuid,
        _total_chunks: u32,
    ) -> Result<(), UploadError> {
        Ok(())
    }

    async fn delete_object(&self, _key: &str) -> Result<(), UploadError> {
        Ok(())
    }

    async fn chunk_exists(
        &self,
        _session_id: Uuid,
        _chunk_index: u32,
    ) -> Result<bool, UploadError> {
        Ok(true)
    }

    async fn assemble_chunks_to_prefix(
        &self,
        _session_id: Uuid,
        _total_chunks: u32,
        _final_key_prefix: &str,
    ) -> Result<String, UploadError> {
        Ok(rustshare_core::validation::calculate_sha256(&Bytes::new()))
    }
}

struct MockUploadMetadataStore {
    existing_file: Mutex<Option<File>>,
    folders: Mutex<Vec<Folder>>,
    created_files: Mutex<Vec<File>>,
    updated_files: Mutex<Vec<File>>,
    created_versions: Mutex<Vec<FileVersion>>,
}

impl MockUploadMetadataStore {
    fn new(existing_file: Option<File>) -> Self {
        Self::with_folders(existing_file, Vec::new())
    }

    fn with_folders(existing_file: Option<File>, folders: Vec<Folder>) -> Self {
        Self {
            existing_file: Mutex::new(existing_file),
            folders: Mutex::new(folders),
            created_files: Mutex::new(Vec::new()),
            updated_files: Mutex::new(Vec::new()),
            created_versions: Mutex::new(Vec::new()),
        }
    }
}

impl UploadMetadataStore for MockUploadMetadataStore {
    async fn find_folder_by_id(
        &self,
        id: Uuid,
        _owner_id: Uuid,
    ) -> Result<Option<Folder>, UploadError> {
        Ok(self
            .folders
            .lock()
            .unwrap()
            .iter()
            .find(|folder| folder.id == id)
            .cloned())
    }

    async fn find_folder_by_id_unchecked(&self, id: Uuid) -> Result<Option<Folder>, UploadError> {
        Ok(self
            .folders
            .lock()
            .unwrap()
            .iter()
            .find(|folder| folder.id == id)
            .cloned())
    }

    async fn find_file_by_path(
        &self,
        _path: &str,
        _owner_id: Uuid,
    ) -> Result<Option<File>, UploadError> {
        Ok(self.existing_file.lock().unwrap().clone())
    }

    async fn create_file(&self, file: &File) -> Result<(), UploadError> {
        self.created_files.lock().unwrap().push(file.clone());
        Ok(())
    }

    async fn update_file(&self, file: &File) -> Result<(), UploadError> {
        self.updated_files.lock().unwrap().push(file.clone());
        *self.existing_file.lock().unwrap() = Some(file.clone());
        Ok(())
    }

    async fn create_file_version(
        &self,
        _file: &File,
        version: &FileVersion,
    ) -> Result<(), UploadError> {
        self.created_versions.lock().unwrap().push(version.clone());
        Ok(())
    }
}

struct MockEventStore {
    events: Mutex<Vec<Event>>,
}

impl MockEventStore {
    fn new() -> Self {
        Self {
            events: Mutex::new(Vec::new()),
        }
    }
}

impl FileEventStoreOps for MockEventStore {
    type Tx = ();

    async fn append(&self, event: &Event, _broadcaster: &EventBroadcaster) -> Result<()> {
        self.events.lock().unwrap().push(event.clone());
        Ok(())
    }

    async fn begin_transaction(&self) -> Result<Self::Tx> {
        Ok(())
    }

    async fn commit_transaction(&self, _tx: Self::Tx) -> Result<()> {
        Ok(())
    }

    async fn append_in_tx(&self, _tx: &mut Self::Tx, event: &Event) -> Result<()> {
        self.events.lock().unwrap().push(event.clone());
        Ok(())
    }
}

#[tokio::test]
async fn complete_upload_updates_existing_file_instead_of_creating_duplicate() {
    let owner_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();
    let session_id = Uuid::new_v4();

    let mut session = UploadSession::new(
        session_id,
        tenant_id,
        owner_id,
        None,
        "note1.md".to_string(),
        "text/markdown".to_string(),
        0,
        1024 * 1024,
        None,
    );
    session.status = UploadSessionStatus::InProgress;
    session.mark_chunk_received(0);

    let existing_file = File {
        id: Uuid::new_v4(),
        name: "note1.md".to_string(),
        path: "/note1.md".to_string(),
        content_hash: "old-hash".to_string(),
        size: 4,
        mime_type: "text/markdown".to_string(),
        parent_folder_id: None,
        owner_id,
        current_version: 1,
        created_at: Utc::now(),
        modified_at: Utc::now(),
        starred_at: None,
        deleted_at: None,
        tenant_id,
    };

    let repo = Arc::new(MockUploadRepo::new(session));
    let metadata_store = Arc::new(MockUploadMetadataStore::new(Some(existing_file.clone())));
    let event_store = Arc::new(MockEventStore::new());
    let service = UploadService::new(
        repo.clone(),
        Arc::new(MockUploadObjectStore),
        metadata_store.clone(),
        event_store.clone(),
        Arc::new(EventBroadcaster::new(16)),
    );

    let response = service.complete_upload(session_id, owner_id).await.unwrap();

    assert_eq!(response.file_id, existing_file.id);
    assert_eq!(
        response.content_hash,
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert!(metadata_store.created_files.lock().unwrap().is_empty());
    assert_eq!(metadata_store.updated_files.lock().unwrap().len(), 1);
    assert_eq!(metadata_store.created_versions.lock().unwrap().len(), 1);
    assert_eq!(
        repo.completed.lock().unwrap().as_slice(),
        &[(session_id, existing_file.id)]
    );

    let updated = metadata_store.updated_files.lock().unwrap()[0].clone();
    assert_eq!(updated.id, existing_file.id);
    assert_eq!(updated.current_version, 2);
    assert_eq!(updated.size, 0);

    let events = event_store.events.lock().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, EventType::FileModified);
    assert_eq!(
        events[0].payload["file_id"].as_str(),
        Some(existing_file.id.to_string().as_str())
    );
}

#[tokio::test]
async fn complete_upload_same_path_with_identical_content_is_a_no_op() {
    let owner_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();
    let session_id = Uuid::new_v4();
    let empty_hash = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string();

    let mut session = UploadSession::new(
        session_id,
        tenant_id,
        owner_id,
        None,
        "note1.md".to_string(),
        "text/markdown".to_string(),
        0,
        1024 * 1024,
        None,
    );
    session.status = UploadSessionStatus::InProgress;
    session.mark_chunk_received(0);

    let existing_file = File {
        id: Uuid::new_v4(),
        name: "note1.md".to_string(),
        path: "/note1.md".to_string(),
        content_hash: empty_hash.clone(),
        size: 0,
        mime_type: "text/markdown".to_string(),
        parent_folder_id: None,
        owner_id,
        current_version: 2,
        created_at: Utc::now(),
        modified_at: Utc::now(),
        starred_at: None,
        deleted_at: None,
        tenant_id,
    };

    let repo = Arc::new(MockUploadRepo::new(session));
    let metadata_store = Arc::new(MockUploadMetadataStore::new(Some(existing_file.clone())));
    let event_store = Arc::new(MockEventStore::new());
    let service = UploadService::new(
        repo.clone(),
        Arc::new(MockUploadObjectStore),
        metadata_store.clone(),
        event_store.clone(),
        Arc::new(EventBroadcaster::new(16)),
    );

    let response = service.complete_upload(session_id, owner_id).await.unwrap();

    assert_eq!(response.file_id, existing_file.id);
    assert_eq!(response.content_hash, empty_hash);
    assert!(metadata_store.created_files.lock().unwrap().is_empty());
    assert!(metadata_store.updated_files.lock().unwrap().is_empty());
    assert!(metadata_store.created_versions.lock().unwrap().is_empty());
    assert!(event_store.events.lock().unwrap().is_empty());
    assert_eq!(
        repo.completed.lock().unwrap().as_slice(),
        &[(session_id, existing_file.id)]
    );
}

#[tokio::test]
async fn complete_upload_updates_existing_nested_file_in_place() {
    let owner_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();
    let session_id = Uuid::new_v4();
    let parent = Folder::new_root_with_name("Uploads".to_string(), owner_id, tenant_id);
    let child = Folder::new_child_with_ancestors(
        "Nested".to_string(),
        format!("{}/Nested", parent.path),
        parent.id,
        parent.ancestor_ids.as_deref(),
        owner_id,
        tenant_id,
    );

    let mut session = UploadSession::new(
        session_id,
        tenant_id,
        owner_id,
        Some(child.id),
        "note1.md".to_string(),
        "text/markdown".to_string(),
        0,
        1024 * 1024,
        None,
    );
    session.status = UploadSessionStatus::InProgress;
    session.mark_chunk_received(0);

    let existing_file = File {
        id: Uuid::new_v4(),
        name: "note1.md".to_string(),
        path: format!("{}/note1.md", child.path),
        content_hash: "old-hash".to_string(),
        size: 4,
        mime_type: "text/markdown".to_string(),
        parent_folder_id: Some(child.id),
        owner_id,
        current_version: 5,
        created_at: Utc::now(),
        modified_at: Utc::now(),
        starred_at: None,
        deleted_at: None,
        tenant_id,
    };

    let repo = Arc::new(MockUploadRepo::new(session));
    let metadata_store = Arc::new(MockUploadMetadataStore::with_folders(
        Some(existing_file.clone()),
        vec![parent, child.clone()],
    ));
    let event_store = Arc::new(MockEventStore::new());
    let service = UploadService::new(
        repo.clone(),
        Arc::new(MockUploadObjectStore),
        metadata_store.clone(),
        event_store.clone(),
        Arc::new(EventBroadcaster::new(16)),
    );

    let response = service.complete_upload(session_id, owner_id).await.unwrap();

    assert_eq!(response.file_id, existing_file.id);
    assert!(metadata_store.created_files.lock().unwrap().is_empty());
    assert_eq!(metadata_store.updated_files.lock().unwrap().len(), 1);
    assert_eq!(metadata_store.created_versions.lock().unwrap().len(), 1);

    let updated = metadata_store.updated_files.lock().unwrap()[0].clone();
    assert_eq!(updated.path, format!("{}/note1.md", child.path));
    assert_eq!(updated.parent_folder_id, Some(child.id));
    assert_eq!(updated.current_version, 6);

    let events = event_store.events.lock().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, EventType::FileModified);
}

#[tokio::test]
async fn complete_upload_to_shared_folder_creates_version_not_duplicate() {
    let folder_owner = Uuid::new_v4();
    let uploader = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();
    let session_id = Uuid::new_v4();

    let shared_folder = Folder::new_root_with_name("Shared".to_string(), folder_owner, tenant_id);

    let mut session = UploadSession::new(
        session_id,
        tenant_id,
        uploader,
        Some(shared_folder.id),
        "note1.md".to_string(),
        "text/markdown".to_string(),
        0,
        1024 * 1024,
        None,
    );
    session.status = UploadSessionStatus::InProgress;
    session.mark_chunk_received(0);

    let existing_file = File {
        id: Uuid::new_v4(),
        name: "note1.md".to_string(),
        path: format!("{}/note1.md", shared_folder.path),
        content_hash: "old-hash".to_string(),
        size: 4,
        mime_type: "text/markdown".to_string(),
        parent_folder_id: Some(shared_folder.id),
        owner_id: folder_owner,
        current_version: 1,
        created_at: Utc::now(),
        modified_at: Utc::now(),
        starred_at: None,
        deleted_at: None,
        tenant_id,
    };

    let repo = Arc::new(MockUploadRepo::new(session));
    let metadata_store = Arc::new(MockUploadMetadataStore::with_folders(
        Some(existing_file.clone()),
        vec![shared_folder.clone()],
    ));
    let event_store = Arc::new(MockEventStore::new());
    let service = UploadService::new(
        repo.clone(),
        Arc::new(MockUploadObjectStore),
        metadata_store.clone(),
        event_store.clone(),
        Arc::new(EventBroadcaster::new(16)),
    );

    // Uploader (not owner) completes upload to shared folder
    let response = service.complete_upload(session_id, uploader).await.unwrap();

    // Should update existing file, not create a duplicate
    assert_eq!(response.file_id, existing_file.id);
    assert!(metadata_store.created_files.lock().unwrap().is_empty());
    assert_eq!(metadata_store.updated_files.lock().unwrap().len(), 1);
    assert_eq!(metadata_store.created_versions.lock().unwrap().len(), 1);

    let updated = metadata_store.updated_files.lock().unwrap()[0].clone();
    assert_eq!(updated.id, existing_file.id);
    assert_eq!(updated.current_version, 2);
    assert_eq!(updated.owner_id, folder_owner); // stays owned by folder owner

    let events = event_store.events.lock().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, EventType::FileModified);
}
