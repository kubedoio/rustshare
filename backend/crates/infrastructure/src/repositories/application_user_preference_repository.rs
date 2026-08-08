use rustshare_core::domain::{ApplicationUserPreference, UserId};
use sqlx::PgPool;

pub struct ApplicationUserPreferenceRepository {
    pool: PgPool,
}

impl ApplicationUserPreferenceRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn get_for_user(
        &self,
        user_id: UserId,
    ) -> anyhow::Result<Vec<ApplicationUserPreference>> {
        let rows = sqlx::query_as::<_, ApplicationUserPreference>(
            r#"
            SELECT user_id, application_id, enabled, created_at, updated_at
            FROM application_user_preferences
            WHERE user_id = $1
            ORDER BY application_id
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn get_for_user_and_application(
        &self,
        user_id: UserId,
        application_id: &str,
    ) -> anyhow::Result<Option<ApplicationUserPreference>> {
        let row = sqlx::query_as::<_, ApplicationUserPreference>(
            r#"
            SELECT user_id, application_id, enabled, created_at, updated_at
            FROM application_user_preferences
            WHERE user_id = $1 AND application_id = $2
            "#,
        )
        .bind(user_id)
        .bind(application_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row)
    }

    pub async fn set_enabled(
        &self,
        user_id: UserId,
        application_id: &str,
        enabled: bool,
    ) -> anyhow::Result<ApplicationUserPreference> {
        let row = sqlx::query_as::<_, ApplicationUserPreference>(
            r#"
            INSERT INTO application_user_preferences (user_id, application_id, enabled)
            VALUES ($1, $2, $3)
            ON CONFLICT (user_id, application_id)
            DO UPDATE SET enabled = $3, updated_at = NOW()
            RETURNING user_id, application_id, enabled, created_at, updated_at
            "#,
        )
        .bind(user_id)
        .bind(application_id)
        .bind(enabled)
        .fetch_one(&self.pool)
        .await?;

        Ok(row)
    }

    pub async fn seed_defaults(&self, user_id: UserId) -> anyhow::Result<()> {
        let defaults = [("notes", true), ("kanban", true), ("brainstorming", true)];

        for (key, enabled) in defaults {
            sqlx::query(
                r#"
                INSERT INTO application_user_preferences (user_id, application_id, enabled)
                VALUES ($1, $2, $3)
                ON CONFLICT (user_id, application_id) DO NOTHING
                "#,
            )
            .bind(user_id)
            .bind(key)
            .bind(enabled)
            .execute(&self.pool)
            .await?;
        }

        Ok(())
    }
}
