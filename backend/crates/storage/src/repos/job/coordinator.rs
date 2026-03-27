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
    
    /// Get reference to job repository for testing
    #[cfg(test)]
    pub fn job_repo(&self) -> &Arc<R> {
        &self.job_repo
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
                        // Release the claim
                        let _ = self.coord_store.release_job_claim(
                            &job.id.to_string(),
                            &self.worker_id,
                        ).await;
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
        
        info!("Job {} completed by worker {}", job_id, self.worker_id);
        Ok(())
    }
    
    /// Fail a job, with optional retry
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
            // Permanently failed
            job.status = JobStatus::Failed;
            job.completed_at = Some(chrono::Utc::now());
            error!(
                "Job {} failed permanently after {} retries: {}",
                job_id, job.retry_count, error_message
            );
        } else {
            // Retry
            job.status = JobStatus::Pending;
            job.worker_id = None;
            job.started_at = None;
            warn!(
                "Job {} failed, scheduling retry {}/{}",
                job_id, job.retry_count, job.max_retries
            );
        }
        
        self.job_repo
            .update_job(&job)
            .await
            .map_err(|e| CoordinatorError::Repository(e.to_string()))?;
        
        Ok(())
    }
}

/// Errors that can occur during job coordination
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
