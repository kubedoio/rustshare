use sqlx::PgPool;

/// Persistence queries for workflow configuration.
#[derive(Clone)]
pub struct WorkflowRepository {
    pool: PgPool,
}

impl WorkflowRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Preserve the installation-wide feature behavior used by the existing endpoint.
    pub async fn invite_enabled(&self) -> anyhow::Result<bool> {
        sqlx::query_scalar(
            "SELECT EXISTS(
                SELECT 1 FROM workflows
                WHERE key = 'invite_email' AND status = 'active'
            )",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(Into::into)
    }
}
