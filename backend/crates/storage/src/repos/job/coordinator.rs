//! Job coordinator for distributed job processing

use super::{Job, JobRepository, JobStatus};
use crate::coordination::CoordinationStore;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Job coordinator for managing job claims across distributed workers
pub struct JobCoordinator<R: JobRepository, C: CoordinationStore> {
    job_repo: Arc<R>,
    coord_store: Arc<C>,
    worker_id: String,
    lease_duration: Duration,
    heartbeat_interval: Duration,
}

impl<R: JobRepository, C: CoordinationStore> JobCoordinator<R, C> {
    /// Create a new job coordinator
    pub fn new(
        job_repo: Arc<R>,
        coord_store: Arc<C>,
        worker_id: String,
    ) -> Self {
        Self {
            job_repo,
            coord_store,
            worker_id,
            lease_duration: Duration::from_secs(300), // 5 minutes
            heartbeat_interval: Duration::from_secs(60), // 1 minute
        }
    }
    
    /// Configure lease duration
    pub fn with_lease_duration(mut self, duration: Duration) -> Self {
        self.lease_duration = duration;
        self
    }
    
    /// Configure heartbeat interval
    pub fn with_heartbeat_interval(mut self, interval: Duration) -> Self {
        self.heartbeat_interval = interval;
        self
    }
    
    /// Try to claim a job for processing
    ///
    /// Returns the job if successfully claimed, None if already claimed or no jobs available
    pub async fn claim_job(&self) -> Result<Option<Job>, CoordinatorError> {
        // Get pending jobs
        let pending = self.job_repo
            .get_pending_jobs(10)
            .await
            .map_err(|e| CoordinatorError::Repository(e.to_string()))?;
        
        for job in pending {
            // Try to claim via coordination store
            match self.coord_store.claim_job(
                &job.id.to_string(),
                &self.worker_id,
                self.lease_duration,
            ).await {
                Ok(true) => {
                    // Successfully claimed
                    info!("Worker {} claimed job {}", self.worker_id, job.id);
                    
                    // Update job status
                    let mut job = job;
                    job.status = JobStatus::Running;
                    job.worker_id = Some(self.worker_id.clone());
                    job.started_at = Some(chrono::Utc::now());
                    
                    if let Err(e) = self.job_repo.update_job(&job).await {
                        error!("Failed to update job status: {}", e);
                        // Release the claim - best effort
                        if let Err(release_err) = self.coord_store.release_job_claim(
                            &job.id.to_string(),
                            &self.worker_id,
                        ).await {
                            debug!(job_id = %job.id, error = %release_err, "failed to release job claim");
                        }
                        continue;
                    }
                    
                    return Ok(Some(job));
                }
                Ok(false) => {
                    // Job already claimed by another worker
                    debug!("Job {} already claimed", job.id);
                    continue;
                }
                Err(e) => {
                    warn!("Failed to claim job {}: {}", job.id, e);
                    continue;
                }
            }
        }
        
        Ok(None)
    }
    
    /// Send heartbeat for a job to extend the lease
    pub async fn heartbeat(&self, job_id: Uuid) -> Result<bool, CoordinatorError> {
        match self.coord_store.heartbeat_job(
            &job_id.to_string(),
            &self.worker_id,
            self.lease_duration,
        ).await {
            Ok(true) => {
                debug!("Heartbeat sent for job {}", job_id);
                Ok(true)
            }
            Ok(false) => {
                warn!("Lost claim on job {}", job_id);
                Ok(false)
            }
            Err(e) => {
                error!("Failed to send heartbeat for job {}: {}", job_id, e);
                Err(CoordinatorError::Coordination(e.to_string()))
            }
        }
    }
    
    /// Complete a job successfully
    pub async fn complete_job(
        &self,
        job_id: Uuid,
        result: serde_json::Value,
    ) -> Result<(), CoordinatorError> {
        // Release claim
        self.coord_store
            .release_job_claim(&job_id.to_string(), &self.worker_id)
            .await
            .map_err(|e| CoordinatorError::Coordination(e.to_string()))?;
        
        // Update job
        let mut job = self.job_repo
            .get_job(job_id)
            .await
            .map_err(|e| CoordinatorError::Repository(e.to_string()))?
            .ok_or(CoordinatorError::JobNotFound(job_id))?;
        
        job.status = JobStatus::Completed;
        job.completed_at = Some(chrono::Utc::now());
        job.payload = result;
        
        self.job_repo
            .update_job(&job)
            .await
            .map_err(|e| CoordinatorError::Repository(e.to_string()))?;
        
        info!("Job {} completed successfully", job_id);
        Ok(())
    }
    
    /// Mark a job as failed
    pub async fn fail_job(
        &self,
        job_id: Uuid,
        error_message: String,
    ) -> Result<(), CoordinatorError> {
        // Release claim
        self.coord_store
            .release_job_claim(&job_id.to_string(), &self.worker_id)
            .await
            .map_err(|e| CoordinatorError::Coordination(e.to_string()))?;
        
        // Get job
        let mut job = self.job_repo
            .get_job(job_id)
            .await
            .map_err(|e| CoordinatorError::Repository(e.to_string()))?
            .ok_or(CoordinatorError::JobNotFound(job_id))?;
        
        job.retry_count += 1;
        job.error_message = Some(error_message.clone());
        
        if job.retry_count >= job.max_retries {
            // Mark as permanently failed
            job.status = JobStatus::Failed;
            job.completed_at = Some(chrono::Utc::now());
            error!("Job {} failed permanently: {}", job_id, error_message);
        } else {
            // Reschedule for retry
            job.status = JobStatus::Pending;
            job.worker_id = None;
            job.started_at = None;
            job.scheduled_at = chrono::Utc::now() + chrono::Duration::seconds(60 * job.retry_count as i64);
            warn!("Job {} failed (attempt {}/{}), rescheduled", job_id, job.retry_count, job.max_retries);
        }
        
        self.job_repo
            .update_job(&job)
            .await
            .map_err(|e| CoordinatorError::Repository(e.to_string()))?;
        
        Ok(())
    }
    
    /// Cancel a job
    pub async fn cancel_job(&self, job_id: Uuid) -> Result<(), CoordinatorError> {
        // Release claim if we have it - best effort
        if let Err(e) = self.coord_store
            .release_job_claim(&job_id.to_string(), &self.worker_id)
            .await
        {
            debug!(job_id = %job_id, error = %e, "failed to release job claim during cancel");
        }
        
        // Get and update job
        let mut job = self.job_repo
            .get_job(job_id)
            .await
            .map_err(|e| CoordinatorError::Repository(e.to_string()))?
            .ok_or(CoordinatorError::JobNotFound(job_id))?;
        
        job.status = JobStatus::Cancelled;
        job.completed_at = Some(chrono::Utc::now());
        
        self.job_repo
            .update_job(&job)
            .await
            .map_err(|e| CoordinatorError::Repository(e.to_string()))?;
        
        info!("Job {} cancelled", job_id);
        Ok(())
    }
    
    /// Get the heartbeat interval
    pub fn heartbeat_interval(&self) -> Duration {
        self.heartbeat_interval
    }
}

/// Errors that can occur in job coordination
#[derive(Debug, thiserror::Error)]
pub enum CoordinatorError {
    #[error("Job not found: {0}")]
    JobNotFound(Uuid),
    
    #[error("Repository error: {0}")]
    Repository(String),
    
    #[error("Coordination error: {0}")]
    Coordination(String),
    
    #[error("Job already claimed")]
    AlreadyClaimed,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coordination::InMemoryCoordinationStore;
    use crate::metadata_v2::stores::LocalFsDocumentStore;
    use crate::metadata_v2::MetadataBackendConfig;
    use crate::repos::job::rustfs::RustFsJobRepository;
    use tempfile::TempDir;

    async fn create_test_coordinator() -> (JobCoordinator<RustFsJobRepository, InMemoryCoordinationStore>, TempDir) {
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
        
        let job_repo = Arc::new(RustFsJobRepository::new(
            doc_store,
            "apps/rustshare".to_string(),
            "test".to_string(),
        ));
        
        let coord_store: Arc<InMemoryCoordinationStore> = Arc::new(InMemoryCoordinationStore::new());
        
        let coordinator = JobCoordinator::new(
            job_repo,
            coord_store,
            "worker1".to_string(),
        );
        
        (coordinator, temp_dir)
    }

    fn create_test_job(id: Uuid) -> Job {
        Job {
            id,
            job_type: "replication".to_string(),
            resource_type: "file_version".to_string(),
            resource_id: Uuid::new_v4(),
            status: JobStatus::Pending,
            priority: 10,
            payload: serde_json::json!({"target": "s3"}),
            retry_count: 0,
            max_retries: 3,
            created_at: chrono::Utc::now(),
            scheduled_at: chrono::Utc::now(),
            started_at: None,
            completed_at: None,
            worker_id: None,
            error_message: None,
        }
    }

    #[tokio::test]
    async fn test_claim_job() {
        let (coordinator, _temp) = create_test_coordinator().await;
        let job = create_test_job(Uuid::new_v4());
        
        // Create job
        coordinator.job_repo.create_job(&job).await.unwrap();
        
        // Claim it
        let claimed = coordinator.claim_job().await.unwrap();
        assert!(claimed.is_some());
        
        let claimed_job = claimed.unwrap();
        assert_eq!(claimed_job.id, job.id);
        assert_eq!(claimed_job.status, JobStatus::Running);
        assert_eq!(claimed_job.worker_id, Some("worker1".to_string()));
    }

    #[tokio::test]
    async fn test_complete_job() {
        let (coordinator, _temp) = create_test_coordinator().await;
        let job = create_test_job(Uuid::new_v4());
        
        // Create and claim job
        coordinator.job_repo.create_job(&job).await.unwrap();
        let claimed = coordinator.claim_job().await.unwrap().unwrap();
        
        // Complete it
        coordinator.complete_job(claimed.id, serde_json::json!({"success": true}))
            .await
            .unwrap();
        
        // Verify
        let completed = coordinator.job_repo.get_job(job.id).await.unwrap().unwrap();
        assert_eq!(completed.status, JobStatus::Completed);
        assert!(completed.completed_at.is_some());
    }

    #[tokio::test]
    async fn test_fail_job_with_retry() {
        let (coordinator, _temp) = create_test_coordinator().await;
        let job = create_test_job(Uuid::new_v4());
        
        // Create and claim job
        coordinator.job_repo.create_job(&job).await.unwrap();
        let claimed = coordinator.claim_job().await.unwrap().unwrap();
        
        // Fail it
        coordinator.fail_job(claimed.id, "Network error".to_string())
            .await
            .unwrap();
        
        // Verify - should be pending again for retry
        let failed = coordinator.job_repo.get_job(job.id).await.unwrap().unwrap();
        assert_eq!(failed.status, JobStatus::Pending);
        assert_eq!(failed.retry_count, 1);
        assert!(failed.error_message.is_some());
    }

    #[tokio::test]
    async fn test_fail_job_permanently() {
        let (coordinator, _temp) = create_test_coordinator().await;
        let mut job = create_test_job(Uuid::new_v4());
        job.max_retries = 1;
        job.retry_count = 1; // Already failed once
        
        // Create and claim job
        coordinator.job_repo.create_job(&job).await.unwrap();
        let claimed = coordinator.claim_job().await.unwrap().unwrap();
        
        // Fail it again - should be permanent
        coordinator.fail_job(claimed.id, "Final error".to_string())
            .await
            .unwrap();
        
        // Verify - should be permanently failed
        let failed = coordinator.job_repo.get_job(job.id).await.unwrap().unwrap();
        assert_eq!(failed.status, JobStatus::Failed);
    }

    use std::sync::Arc;
    use crate::metadata_v2::MetadataDocumentStore;
}
