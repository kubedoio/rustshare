use crate::client::ApiClient;
use file_ops;
use anyhow::Result;
use std::path::{Path, PathBuf};
use sync_domain::{LocalEntry, RemoteEntry};
use uuid::Uuid;
use tracing::{info, error};

pub struct SyncWorker {
    client: ApiClient,
}

impl SyncWorker {
    pub fn new(client: ApiClient) -> Self {
        Self { client }
    }

    pub async fn upload(&self, local: &LocalEntry, remote_root: Uuid) -> Result<()> {
        info!("Uploading {}...", local.path.display());
        // In Phase 1, we'd use the resumable upload flow:
        // 1. Create session (POST /api/v1/uploads/sessions)
        // 2. Upload chunks (PUT /api/v1/uploads/sessions/{id}/chunks/{index})
        // 3. Complete session (POST /api/v1/uploads/sessions/{id}/complete)
        Ok(())
    }

    pub async fn download(&self, remote: &RemoteEntry, local_dest: &Path) -> Result<()> {
        info!("Downloading {} -> {}...", remote.name, local_dest.display());
        let response = self.client.download_file(remote.id).await?;
        let mut stream = response.bytes_stream();
        
        let mut temp_path = local_dest.to_path_buf();
        temp_path.set_extension("rs_tmp");
        
        // In a real implementation:
        // 1. Write stream to temp_path
        // 2. Verify hash
        // 3. atomic_rename(temp_path, local_dest)
        
        Ok(())
    }
}
