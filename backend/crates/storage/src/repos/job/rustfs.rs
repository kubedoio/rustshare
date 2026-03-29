//! RustFS-backed job repository implementation

use super::*;
use crate::metadata_v2::{
    schemas::{
        JobDocument, JobQueueIndex, JobRef, JobStatus as DocJobStatus,
        CURRENT_SCHEMA_VERSION,
    },
    MetadataDocumentStore, MetadataDocumentStoreExt, PutOptions,
};
use crate::repos::PathBuilder;
use async_trait::async_trait;
use std::sync::Arc;
use uuid::Uuid;

/// RustFS-backed job repository
pub struct RustFsJobRepository {
    doc_store: Arc<dyn MetadataDocumentStore>,
    path_builder: PathBuilder,
}

impl RustFsJobRepository {
    /// Create a new RustFS job repository
    pub fn new(doc_store: Arc<dyn MetadataDocumentStore>, base_prefix: String, namespace: String) -> Self {
        Self {
            doc_store,
            path_builder: PathBuilder::new(base_prefix, namespace),
        }
    }
    
    /// Get or create queue index
    async fn get_or_create_index(&self) -> Result<JobQueueIndex, JobRepositoryError> {
        let index_path = self.path_builder.queue_index_path();
        
        match self.doc_store.get::<JobQueueIndex>(&index_path).await {
            Ok(Some((index, _))) => Ok(index),
            Ok(None) => Ok(JobQueueIndex::new(self.path_builder.namespace().to_string())),
            Err(e) => Err(JobRepositoryError::Storage(e.to_string())),
        }
    }
    
    /// Save queue index
    async fn save_index(&self, index: &JobQueueIndex) -> Result<(), JobRepositoryError> {
        let index_path = self.path_builder.queue_index_path();
        
        self.doc_store
            .put(&index_path, index, PutOptions::default())
            .await
            .map_err(|e| JobRepositoryError::Storage(e.to_string()))?;
        
        Ok(())
    }
    
    /// Update index when job is created
    async fn add_to_index(&self, job: &JobDocument) -> Result<(), JobRepositoryError> {
        let mut index = self.get_or_create_index().await?;
        
        let job_ref = JobRef {
            job_id: job.id,
            job_type: job.job_type,
            resource_type: job.resource_type.clone(),
            resource_id: job.resource_id,
            priority: job.priority,
            created_at: job.created_at,
        };
        
        index.add_pending(job_ref);
        self.save_index(&index).await
    }
    
    /// Update index when job status changes
    async fn update_index_status(
        &self,
        job_id: Uuid,
        old_status: JobStatus,
        new_status: JobStatus,
    ) -> Result<(), JobRepositoryError> {
        let mut index = self.get_or_create_index().await?;
        
        match (old_status, new_status) {
            (JobStatus::Pending, JobStatus::Running) => {
                index.mark_running(job_id);
            }
            (JobStatus::Running, JobStatus::Completed) |
            (JobStatus::Running, JobStatus::Failed) |
            (JobStatus::Running, JobStatus::Cancelled) => {
                index.mark_completed(job_id);
            }
            _ => {}
        }
        
        self.save_index(&index).await
    }
}

#[async_trait]
impl JobRepository for RustFsJobRepository {
    async fn create_job(&self, job: &Job) -> Result<(), JobRepositoryError> {
        let doc = super::conversions::job_to_doc(job);
        let path = self.path_builder.job_path(job.id);
        
        // Store job
        self.doc_store
            .put(&path, &doc, PutOptions::default())
            .await
            .map_err(|e| JobRepositoryError::Storage(e.to_string()))?;
        
        // Update index if pending
        if job.status == JobStatus::Pending {
            self.add_to_index(&doc).await?;
        }
        
        Ok(())
    }
    
    async fn get_job(&self, id: Uuid) -> Result<Option<Job>, JobRepositoryError> {
        let path = self.path_builder.job_path(id);
        
        match self.doc_store.get::<JobDocument>(&path).await {
            Ok(Some((doc, _))) => Ok(Some(super::conversions::doc_to_job(doc))),
            Ok(None) => Ok(None),
            Err(e) => Err(JobRepositoryError::Storage(e.to_string())),
        }
    }
    
    async fn update_job(&self, job: &Job) -> Result<(), JobRepositoryError> {
        let old_job = self.get_job(job.id).await?;
        let doc = super::conversions::job_to_doc(job);
        let path = self.path_builder.job_path(job.id);
        
        // Store updated job
        self.doc_store
            .put(&path, &doc, PutOptions::default())
            .await
            .map_err(|e| JobRepositoryError::Storage(e.to_string()))?;
        
        // Update index if status changed
        if let Some(old) = old_job {
            if old.status != job.status {
                self.update_index_status(job.id, old.status, job.status).await?;
            }
        }
        
        Ok(())
    }
    
    async fn delete_job(&self, id: Uuid) -> Result<(), JobRepositoryError> {
        let path = self.path_builder.job_path(id);
        
        // Get job for index update
        let job = self.get_job(id).await?;
        
        // Delete job
        self.doc_store
            .delete(&path)
            .await
            .map_err(|e| JobRepositoryError::Storage(e.to_string()))?;
        
        // Update index
        if let Some(job) = job {
            let mut index = self.get_or_create_index().await?;
            index.remove_job(id);
            self.save_index(&index).await?;
        }
        
        Ok(())
    }
    
    async fn query_jobs(&self, query: JobQuery) -> Result<Vec<Job>, JobRepositoryError> {
        // Get all jobs from index
        let index = self.get_or_create_index().await?;
        
        // Collect all job references
        let mut job_refs: Vec<JobRef> = Vec::new();
        
        if query.status.is_none() || query.status == Some(JobStatus::Pending) {
            job_refs.extend(index.pending.clone());
        }
        if query.status.is_none() || query.status == Some(JobStatus::Running) {
            job_refs.extend(index.running.clone());
        }
        if query.status.is_none() {
            job_refs.extend(index.completed_recent.clone());
        }
        
        // Apply filters
        if let Some(job_type) = &query.job_type {
            job_refs.retain(|j| format!("{:?}", j.job_type) == *job_type);
        }
        
        // Apply pagination
        let offset = query.offset.unwrap_or(0);
        let limit = query.limit.unwrap_or(usize::MAX);
        
        let job_refs: Vec<JobRef> = job_refs
            .into_iter()
            .skip(offset)
            .take(limit)
            .collect();
        
        // Fetch full documents in parallel
        let paths: Vec<String> = job_refs
            .iter()
            .map(|r| self.path_builder.job_path(r.job_id))
            .collect();
        let path_refs: Vec<&str> = paths.iter().map(|s| s.as_str()).collect();
        
        let mut jobs = Vec::new();
        if !path_refs.is_empty() {
            match self.doc_store.get_multi::<JobDocument>(&path_refs).await {
                Ok(results) => {
                    for (_, doc, _) in results {
                        jobs.push(super::conversions::doc_to_job(doc));
                    }
                }
                Err(e) => return Err(JobRepositoryError::Storage(e.to_string())),
            }
        }
        
        Ok(jobs)
    }
    
    async fn get_pending_jobs(&self, limit: usize) -> Result<Vec<Job>, JobRepositoryError> {
        let index = self.get_or_create_index().await?;
        
        let pending_refs: Vec<&JobRef> = index.pending.iter().take(limit).collect();
        let paths: Vec<String> = pending_refs
            .iter()
            .map(|r| self.path_builder.job_path(r.job_id))
            .collect();
        let path_refs: Vec<&str> = paths.iter().map(|s| s.as_str()).collect();
        
        let mut jobs = Vec::new();
        if !path_refs.is_empty() {
            match self.doc_store.get_multi::<JobDocument>(&path_refs).await {
                Ok(results) => {
                    for (_, doc, _) in results {
                        let job = super::conversions::doc_to_job(doc);
                        if job.status == JobStatus::Pending {
                            jobs.push(job);
                        }
                    }
                }
                Err(e) => return Err(JobRepositoryError::Storage(e.to_string())),
            }
        }
        
        Ok(jobs)
    }
    
    async fn get_running_jobs(&self) -> Result<Vec<Job>, JobRepositoryError> {
        let index = self.get_or_create_index().await?;
        
        let paths: Vec<String> = index
            .running
            .iter()
            .map(|r| self.path_builder.job_path(r.job_id))
            .collect();
        let path_refs: Vec<&str> = paths.iter().map(|s| s.as_str()).collect();
        
        let mut jobs = Vec::new();
        if !path_refs.is_empty() {
            match self.doc_store.get_multi::<JobDocument>(&path_refs).await {
                Ok(results) => {
                    for (_, doc, _) in results {
                        let job = super::conversions::doc_to_job(doc);
                        if job.status == JobStatus::Running {
                            jobs.push(job);
                        }
                    }
                }
                Err(e) => return Err(JobRepositoryError::Storage(e.to_string())),
            }
        }
        
        Ok(jobs)
    }
    
    async fn count_jobs(&self, status: Option<JobStatus>) -> Result<usize, JobRepositoryError> {
        let index = self.get_or_create_index().await?;
        
        let count = match status {
            Some(JobStatus::Pending) => index.pending.len(),
            Some(JobStatus::Running) => index.running.len(),
            None => index.pending.len() + index.running.len() + index.completed_recent.len(),
            _ => 0,
        };
        
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata_v2::stores::LocalFsDocumentStore;
    use crate::metadata_v2::MetadataBackendConfig;
    use tempfile::TempDir;

    async fn create_test_repository() -> (RustFsJobRepository, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config = MetadataBackendConfig {
            base_prefix: "test".to_string(),
            namespace: "default".to_string(),
            enable_optimistic_concurrency: true,
            fallback_to_leases: true,
        };
        
        let doc_store: Arc<dyn MetadataDocumentStore> = Arc::new(
            LocalFsDocumentStore::new(temp_dir.path().to_path_buf(), config)
        );
        
        let repo = RustFsJobRepository::new(
            doc_store,
            "apps/rustshare".to_string(),
            "test".to_string(),
        );
        
        (repo, temp_dir)
    }

    fn create_test_job(id: Uuid, priority: i32) -> Job {
        Job {
            id,
            job_type: "replication".to_string(),
            resource_type: "file_version".to_string(),
            resource_id: Uuid::new_v4(),
            status: JobStatus::Pending,
            priority,
            payload: serde_json::json!({"target": "s3"}),
            retry_count: 0,
            max_retries: 3,
            created_at: chrono::Utc::now(),
            scheduled_at: chrono::Utc::now(),
            started_at: None,
            completed_at: None,
            worker_id: None,
            error_message: None,
            tenant_id: Uuid::nil(),
        }
    }

    #[tokio::test]
    async fn test_create_and_get_job() {
        let (repo, _temp) = create_test_repository().await;
        let job = create_test_job(Uuid::new_v4(), 10);
        
        repo.create_job(&job).await.unwrap();
        
        let found = repo.get_job(job.id).await.unwrap().unwrap();
        assert_eq!(found.priority, 10);
        assert_eq!(found.status, JobStatus::Pending);
    }

    #[tokio::test]
    async fn test_get_pending_jobs() {
        let (repo, _temp) = create_test_repository().await;
        
        // Create jobs with different priorities
        for i in 0..5 {
            let job = create_test_job(Uuid::new_v4(), i * 10);
            repo.create_job(&job).await.unwrap();
        }
        
        let pending = repo.get_pending_jobs(10).await.unwrap();
        assert_eq!(pending.len(), 5);
        
        // Should be sorted by priority (highest first)
        assert_eq!(pending[0].priority, 40);
        assert_eq!(pending[4].priority, 0);
    }

    #[tokio::test]
    async fn test_update_job_status() {
        let (repo, _temp) = create_test_repository().await;
        let mut job = create_test_job(Uuid::new_v4(), 10);
        
        repo.create_job(&job).await.unwrap();
        
        // Update to running
        job.status = JobStatus::Running;
        job.worker_id = Some("worker1".to_string());
        job.started_at = Some(chrono::Utc::now());
        
        repo.update_job(&job).await.unwrap();
        
        let found = repo.get_job(job.id).await.unwrap().unwrap();
        assert_eq!(found.status, JobStatus::Running);
        assert_eq!(found.worker_id, Some("worker1".to_string()));
    }

    #[tokio::test]
    async fn test_count_jobs() {
        let (repo, _temp) = create_test_repository().await;
        
        for _ in 0..3 {
            let job = create_test_job(Uuid::new_v4(), 10);
            repo.create_job(&job).await.unwrap();
        }
        
        assert_eq!(repo.count_jobs(None).await.unwrap(), 3);
        assert_eq!(repo.count_jobs(Some(JobStatus::Pending)).await.unwrap(), 3);
        assert_eq!(repo.count_jobs(Some(JobStatus::Running)).await.unwrap(), 0);
    }
}
