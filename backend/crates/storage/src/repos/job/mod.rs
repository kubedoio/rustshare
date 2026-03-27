//! Job repository and coordinator for zero-PostgreSQL job management

use async_trait::async_trait;
use thiserror::Error;
use uuid::Uuid;

pub mod rustfs;
pub mod coordinator;

pub use rustfs::RustFsJobRepository;
pub use coordinator::JobCoordinator;

/// Errors that can occur in job repository operations
#[derive(Debug, Error)]
pub enum JobRepositoryError {
    #[error("Job not found: {0}")]
    NotFound(Uuid),
    
    #[error("Job already claimed")]
    AlreadyClaimed,
    
    #[error("Storage error: {0}")]
    Storage(String),
    
    #[error("Concurrency conflict")]
    Conflict,
}

/// Job status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// Job data structure
#[derive(Debug, Clone)]
pub struct Job {
    pub id: Uuid,
    pub job_type: String,
    pub resource_type: String,
    pub resource_id: Uuid,
    pub status: JobStatus,
    pub priority: i32,
    pub payload: serde_json::Value,
    pub retry_count: u32,
    pub max_retries: u32,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub scheduled_at: chrono::DateTime<chrono::Utc>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub worker_id: Option<String>,
    pub error_message: Option<String>,
}

/// Query options for listing jobs
#[derive(Debug, Clone, Default)]
pub struct JobQuery {
    /// Filter by status
    pub status: Option<JobStatus>,
    /// Filter by job type
    pub job_type: Option<String>,
    /// Limit results
    pub limit: Option<usize>,
    /// Offset for pagination
    pub offset: Option<usize>,
}

/// Job repository trait
#[async_trait]
pub trait JobRepository: Send + Sync {
    /// Create a new job
    async fn create_job(&self, job: &Job) -> Result<(), JobRepositoryError>;
    
    /// Get job by ID
    async fn get_job(&self, id: Uuid) -> Result<Option<Job>, JobRepositoryError>;
    
    /// Update a job
    async fn update_job(&self, job: &Job) -> Result<(), JobRepositoryError>;
    
    /// Delete a job
    async fn delete_job(&self, id: Uuid) -> Result<(), JobRepositoryError>;
    
    /// Query jobs
    async fn query_jobs(&self, query: JobQuery) -> Result<Vec<Job>, JobRepositoryError>;
    
    /// Get pending jobs (sorted by priority)
    async fn get_pending_jobs(&self, limit: usize) -> Result<Vec<Job>, JobRepositoryError>;
    
    /// Get running jobs
    async fn get_running_jobs(&self) -> Result<Vec<Job>, JobRepositoryError>;
    
    /// Count jobs by status
    async fn count_jobs(&self, status: Option<JobStatus>) -> Result<usize, JobRepositoryError>;
}

/// Converts between Job and JobDocument
pub mod conversions {
    use super::*;
    use crate::metadata_v2::schemas::{JobDocument, JobStatus as DocJobStatus};
    
    fn from_core_status(status: JobStatus) -> DocJobStatus {
        match status {
            JobStatus::Pending => DocJobStatus::Pending,
            JobStatus::Running => DocJobStatus::Running,
            JobStatus::Completed => DocJobStatus::Completed,
            JobStatus::Failed => DocJobStatus::Failed,
            JobStatus::Cancelled => DocJobStatus::Cancelled,
        }
    }
    
    fn to_core_status(status: DocJobStatus) -> JobStatus {
        match status {
            DocJobStatus::Pending => JobStatus::Pending,
            DocJobStatus::Running => JobStatus::Running,
            DocJobStatus::Completed => JobStatus::Completed,
            DocJobStatus::Failed => JobStatus::Failed,
            DocJobStatus::Cancelled => JobStatus::Cancelled,
        }
    }
    
    /// Convert JobDocument to Job
    pub fn doc_to_job(doc: JobDocument) -> Job {
        Job {
            id: doc.id,
            job_type: format!("{:?}", doc.job_type),
            resource_type: doc.resource_type,
            resource_id: doc.resource_id,
            status: to_core_status(doc.status),
            priority: doc.priority,
            payload: doc.payload,
            retry_count: doc.retry_count,
            max_retries: doc.max_retries,
            created_at: doc.created_at,
            scheduled_at: doc.scheduled_at,
            started_at: doc.started_at,
            completed_at: doc.completed_at,
            worker_id: doc.worker_id,
            error_message: doc.error_message,
        }
    }
    
    /// Convert Job to JobDocument (simplified - would need job_type parsing)
    pub fn job_to_doc(job: &Job) -> JobDocument {
        JobDocument {
            schema_version: crate::metadata_v2::schemas::CURRENT_SCHEMA_VERSION,
            id: job.id,
            job_type: crate::metadata_v2::schemas::JobType::Replication, // Default
            resource_type: job.resource_type.clone(),
            resource_id: job.resource_id,
            status: from_core_status(job.status),
            priority: job.priority,
            payload: job.payload.clone(),
            result: None,
            retry_count: job.retry_count,
            max_retries: job.max_retries,
            created_at: job.created_at,
            scheduled_at: job.scheduled_at,
            started_at: job.started_at,
            completed_at: job.completed_at,
            error_message: job.error_message.clone(),
            worker_id: job.worker_id.clone(),
            version: 1,
        }
    }
}
