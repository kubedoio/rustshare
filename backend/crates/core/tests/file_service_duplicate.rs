use std::sync::{Arc, Mutex};

use anyhow::Result;
use bytes::Bytes;
use chrono::Utc;
use rustshare_core::domain::{
    File, FileVersion, Folder, ReplicationJob, ReplicationState, Share, UserId,
};
use rustshare_core::events::{Event, EventBroadcaster, EventType};
use rustshare_core::services::{
    FileEventStoreOps, FileMetadataStoreOps, FileService, ObjectStoreOps, PermissionResolver,
    PermissionResolverOps,
};
use uuid::Uuid;

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
    async fn append(&self, event: &Event, _broadcaster: &EventBroadcaster) -> Result<()> {
        self.events.lock().unwrap().push(event.clone());
        Ok(())
    }
}

struct MockMetadataStore {
    existing_file: Mutex<Option<File>>,
    folders: Mutex<Vec<Folder>>,
    created_files: Mutex<Vec<File>>,
    updated_files: Mutex<Vec<File>>,
    created_versions: Mutex<Vec<FileVersion>>,
}

impl MockMetadataStore {
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

impl FileMetadataStoreOps for MockMetadataStore {
    async fn create_file(&self, file: &File) -> Result<()> {
        self.created_files.lock().unwrap().push(file.clone());
        Ok(())
    }

    async fn find_file_by_path(&self, _path: &str, _owner_id: Uuid) -> Result<Option<File>> {
        Ok(self.existing_file.lock().unwrap().clone())
    }

    async fn create_file_version(&self, version: &FileVersion) -> Result<()> {
        self.created_versions.lock().unwrap().push(version.clone());
        Ok(())
    }

    async fn find_folder_by_id(&self, id: Uuid) -> Result<Option<Folder>> {
        Ok(self
            .folders
            .lock()
            .unwrap()
            .iter()
            .find(|folder| folder.id == id)
            .cloned())
    }

    async fn find_file_by_id(&self, id: Uuid) -> Result<Option<File>> {
        let existing = self.existing_file.lock().unwrap().clone();
        Ok(existing.filter(|file| file.id == id))
    }

    async fn update_file(&self, file: &File) -> Result<()> {
        self.updated_files.lock().unwrap().push(file.clone());
        *self.existing_file.lock().unwrap() = Some(file.clone());
        Ok(())
    }

    async fn delete_file(&self, _id: Uuid) -> Result<()> {
        unreachable!()
    }

    async fn list_file_versions(&self, _file_id: Uuid) -> Result<Vec<FileVersion>> {
        Ok(Vec::new())
    }

    async fn find_file_version(&self, _file_id: Uuid, _version: i32) -> Result<Option<FileVersion>> {
        Ok(None)
    }

    async fn count_enabled_replication_targets(&self) -> Result<i64> {
        Ok(0)
    }

    async fn create_replication_job(&self, _job: &ReplicationJob) -> Result<()> {
        Ok(())
    }

    async fn update_file_version_replication_state(
        &self,
        _version_id: Uuid,
        _state: ReplicationState,
    ) -> Result<()> {
        Ok(())
    }
}

struct MockObjectStore {
    puts: Mutex<Vec<(String, Bytes)>>,
}

impl MockObjectStore {
    fn new() -> Self {
        Self {
            puts: Mutex::new(Vec::new()),
        }
    }
}

impl ObjectStoreOps for MockObjectStore {
    async fn put(&self, key: &str, data: Bytes) -> Result<()> {
        self.puts.lock().unwrap().push((key.to_string(), data));
        Ok(())
    }

    async fn exists(&self, _key: &str) -> Result<bool> {
        Ok(false)
    }

    async fn get_presigned_url(&self, _key: &str, _expiry_secs: u64) -> Result<String> {
        unreachable!()
    }

    async fn get(&self, _key: &str) -> Result<Bytes> {
        unreachable!()
    }

    async fn delete(&self, _key: &str) -> Result<()> {
        Ok(())
    }
}

struct MockPermissionOps {
    folders: Mutex<Vec<Folder>>,
}

impl MockPermissionOps {
    fn new() -> Self {
        Self {
            folders: Mutex::new(Vec::new()),
        }
    }

    fn with_folders(folders: Vec<Folder>) -> Self {
        Self {
            folders: Mutex::new(folders),
        }
    }
}

impl PermissionResolverOps for MockPermissionOps {
    async fn find_user_share(
        &self,
        _file_id: Option<Uuid>,
        _folder_id: Option<Uuid>,
        _recipient_user_id: UserId,
    ) -> Result<Option<Share>> {
        Ok(None)
    }

    async fn find_group_shares(
        &self,
        _file_id: Option<Uuid>,
        _folder_id: Option<Uuid>,
        _group_ids: &[Uuid],
    ) -> Result<Vec<Share>> {
        Ok(Vec::new())
    }

    async fn find_user_shares_for_folders(
        &self,
        _folder_ids: &[Uuid],
        _recipient_user_id: UserId,
    ) -> Result<Vec<Share>> {
        Ok(Vec::new())
    }

    async fn find_group_shares_for_folders(
        &self,
        _folder_ids: &[Uuid],
        _group_ids: &[Uuid],
    ) -> Result<Vec<Share>> {
        Ok(Vec::new())
    }

    async fn find_file_by_id(&self, _id: Uuid) -> Result<Option<File>> {
        Ok(None)
    }

    async fn find_folder_by_id(&self, id: Uuid) -> Result<Option<Folder>> {
        Ok(self
            .folders
            .lock()
            .unwrap()
            .iter()
            .find(|folder| folder.id == id)
            .cloned())
    }

    async fn get_user_group_ids(&self, _user_id: UserId) -> Result<Vec<Uuid>> {
        Ok(Vec::new())
    }
}

#[tokio::test]
async fn direct_upload_updates_existing_file_instead_of_creating_duplicate() {
    let owner_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();
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

    let event_store = Arc::new(MockEventStore::new());
    let metadata_store = Arc::new(MockMetadataStore::new(Some(existing_file.clone())));
    let object_store = Arc::new(MockObjectStore::new());
    let permission_resolver = Arc::new(PermissionResolver::new(Arc::new(MockPermissionOps::new())));
    let service = FileService::new(
        event_store.clone(),
        metadata_store.clone(),
        object_store,
        Arc::new(EventBroadcaster::new(16)),
        permission_resolver,
    );

    let uploaded = service
        .upload_file(
            owner_id,
            "note1.md".to_string(),
            None,
            Bytes::new(),
            "text/markdown".to_string(),
            tenant_id,
        )
        .await
        .unwrap();

    assert_eq!(uploaded.id, existing_file.id);
    assert_eq!(uploaded.current_version, 2);
    assert!(metadata_store.created_files.lock().unwrap().is_empty());
    assert_eq!(metadata_store.updated_files.lock().unwrap().len(), 1);
    assert_eq!(metadata_store.created_versions.lock().unwrap().len(), 1);

    let events = event_store.events.lock().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, EventType::FileModified);
    assert_eq!(
        events[0].payload["file_id"].as_str(),
        Some(existing_file.id.to_string().as_str())
    );
}

#[tokio::test]
async fn direct_upload_same_path_with_identical_content_is_a_no_op() {
    let owner_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();
    let content = Bytes::from_static(b"same content");
    let content_hash = "a636bd7cd42060a4d07fa1bfbcc010eb7794c2ba721e1e3e4c20335a15b66eaf";
    let existing_file = File {
        id: Uuid::new_v4(),
        name: "note1.md".to_string(),
        path: "/note1.md".to_string(),
        content_hash: content_hash.to_string(),
        size: content.len() as i64,
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

    let event_store = Arc::new(MockEventStore::new());
    let metadata_store = Arc::new(MockMetadataStore::new(Some(existing_file.clone())));
    let object_store = Arc::new(MockObjectStore::new());
    let permission_resolver = Arc::new(PermissionResolver::new(Arc::new(MockPermissionOps::new())));
    let service = FileService::new(
        event_store.clone(),
        metadata_store.clone(),
        object_store.clone(),
        Arc::new(EventBroadcaster::new(16)),
        permission_resolver,
    );

    let uploaded = service
        .upload_file(
            owner_id,
            "note1.md".to_string(),
            None,
            content,
            "text/markdown".to_string(),
            tenant_id,
        )
        .await
        .unwrap();

    assert_eq!(uploaded.id, existing_file.id);
    assert_eq!(uploaded.current_version, existing_file.current_version);
    assert!(metadata_store.created_files.lock().unwrap().is_empty());
    assert!(metadata_store.updated_files.lock().unwrap().is_empty());
    assert!(metadata_store.created_versions.lock().unwrap().is_empty());
    assert!(event_store.events.lock().unwrap().is_empty());
    assert_eq!(object_store.puts.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn direct_upload_updates_existing_nested_file_in_place() {
    let owner_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();
    let parent = Folder::new_root_with_name("Notes".to_string(), owner_id, tenant_id);
    let child = Folder::new_child_with_ancestors(
        "Projects".to_string(),
        format!("{}/Projects", parent.path),
        parent.id,
        parent.ancestor_ids.as_deref(),
        owner_id,
        tenant_id,
    );
    let existing_file = File {
        id: Uuid::new_v4(),
        name: "note1.md".to_string(),
        path: format!("{}/note1.md", child.path),
        content_hash: "old-hash".to_string(),
        size: 4,
        mime_type: "text/markdown".to_string(),
        parent_folder_id: Some(child.id),
        owner_id,
        current_version: 3,
        created_at: Utc::now(),
        modified_at: Utc::now(),
        starred_at: None,
        deleted_at: None,
        tenant_id,
    };

    let event_store = Arc::new(MockEventStore::new());
    let metadata_store = Arc::new(MockMetadataStore::with_folders(
        Some(existing_file.clone()),
        vec![parent.clone(), child.clone()],
    ));
    let object_store = Arc::new(MockObjectStore::new());
    let permission_resolver = Arc::new(PermissionResolver::new(Arc::new(
        MockPermissionOps::with_folders(vec![parent, child.clone()]),
    )));
    let service = FileService::new(
        event_store.clone(),
        metadata_store.clone(),
        object_store,
        Arc::new(EventBroadcaster::new(16)),
        permission_resolver,
    );

    let uploaded = service
        .upload_file(
            owner_id,
            "note1.md".to_string(),
            Some(child.id),
            Bytes::from_static(b"nested update"),
            "text/markdown".to_string(),
            tenant_id,
        )
        .await
        .unwrap();

    assert_eq!(uploaded.id, existing_file.id);
    assert_eq!(uploaded.path, format!("{}/note1.md", child.path));
    assert_eq!(uploaded.parent_folder_id, Some(child.id));
    assert_eq!(uploaded.current_version, 4);
    assert!(metadata_store.created_files.lock().unwrap().is_empty());
    assert_eq!(metadata_store.updated_files.lock().unwrap().len(), 1);
    assert_eq!(metadata_store.created_versions.lock().unwrap().len(), 1);

    let events = event_store.events.lock().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, EventType::FileModified);
}
