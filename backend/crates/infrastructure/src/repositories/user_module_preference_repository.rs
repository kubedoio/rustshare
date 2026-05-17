use rustshare_core::domain::{UserId, UserModulePreference};
use sqlx::PgPool;

pub struct UserModulePreferenceRepository {
    pool: PgPool,
}

impl UserModulePreferenceRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn get_for_user(&self, user_id: UserId) -> anyhow::Result<Vec<UserModulePreference>> {
        let rows = sqlx::query_as::<_, UserModulePreference>(
            r#"
            SELECT user_id, module_key, enabled, created_at, updated_at
            FROM user_module_preferences
            WHERE user_id = $1
            ORDER BY module_key
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn get_for_user_and_module(
        &self,
        user_id: UserId,
        module_key: &str,
    ) -> anyhow::Result<Option<UserModulePreference>> {
        let row = sqlx::query_as::<_, UserModulePreference>(
            r#"
            SELECT user_id, module_key, enabled, created_at, updated_at
            FROM user_module_preferences
            WHERE user_id = $1 AND module_key = $2
            "#,
        )
        .bind(user_id)
        .bind(module_key)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row)
    }

    pub async fn set_enabled(
        &self,
        user_id: UserId,
        module_key: &str,
        enabled: bool,
    ) -> anyhow::Result<UserModulePreference> {
        let row = sqlx::query_as::<_, UserModulePreference>(
            r#"
            INSERT INTO user_module_preferences (user_id, module_key, enabled)
            VALUES ($1, $2, $3)
            ON CONFLICT (user_id, module_key)
            DO UPDATE SET enabled = $3, updated_at = NOW()
            RETURNING user_id, module_key, enabled, created_at, updated_at
            "#,
        )
        .bind(user_id)
        .bind(module_key)
        .bind(enabled)
        .fetch_one(&self.pool)
        .await?;

        Ok(row)
    }

    pub async fn seed_defaults(&self, user_id: UserId) -> anyhow::Result<()> {
        let defaults = [("notes", true), ("kanban", true), ("brainstorming", true)];

        for (key, enabled) in defaults {
            sqlx::query!(
                r#"
                INSERT INTO user_module_preferences (user_id, module_key, enabled)
                VALUES ($1, $2, $3)
                ON CONFLICT (user_id, module_key) DO NOTHING
                "#,
                user_id,
                key,
                enabled
            )
            .execute(&self.pool)
            .await?;
        }

        Ok(())
    }
}
