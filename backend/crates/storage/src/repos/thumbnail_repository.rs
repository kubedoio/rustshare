//! SQL-backed thumbnail metadata repository.

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use rustshare_core::{
    domain::{FileThumbnail, ThumbnailSize},
    services::ThumbnailRepository,
};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

pub struct ThumbnailRepositoryImpl {
    pool: PgPool,
}

impl ThumbnailRepositoryImpl {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(FromRow)]
struct ThumbnailRow {
    id: Uuid,
    file_id: Uuid,
    size: String,
    storage_path: String,
    content_type: String,
    generated_at: DateTime<Utc>,
}

impl TryFrom<ThumbnailRow> for FileThumbnail {
    type Error = anyhow::Error;

    fn try_from(row: ThumbnailRow) -> Result<Self> {
        Ok(Self {
            id: row.id,
            file_id: row.file_id,
            size: ThumbnailSize::try_from(row.size.as_str()).map_err(|e| anyhow!(e))?,
            storage_path: row.storage_path,
            content_type: row.content_type,
            generated_at: row.generated_at,
        })
    }
}

#[async_trait::async_trait]
impl ThumbnailRepository for ThumbnailRepositoryImpl {
    async fn get_thumbnail(
        &self,
        file_id: Uuid,
        size: ThumbnailSize,
    ) -> Result<Option<FileThumbnail>> {
        let row = sqlx::query_as::<_, ThumbnailRow>(
            r#"
            SELECT id, file_id, size, storage_path, content_type, generated_at
            FROM file_thumbnails
            WHERE file_id = $1 AND size = $2
            "#,
        )
        .bind(file_id)
        .bind(size.as_str())
        .fetch_optional(&self.pool)
        .await?;

        row.map(TryInto::try_into).transpose()
    }

    async fn get_file_metadata(&self, file_id: Uuid) -> Result<Option<(String, i64, String)>> {
        Ok(sqlx::query_as::<_, (String, i64, String)>(
            "SELECT name, size, content_hash FROM files WHERE id = $1",
        )
        .bind(file_id)
        .fetch_optional(&self.pool)
        .await?)
    }

    async fn save_thumbnail(
        &self,
        file_id: Uuid,
        size: ThumbnailSize,
        storage_path: &str,
        content_type: &str,
    ) -> Result<FileThumbnail> {
        let row = sqlx::query_as::<_, ThumbnailRow>(
            r#"
            INSERT INTO file_thumbnails (file_id, size, storage_path, content_type)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (file_id, size) DO UPDATE SET
                storage_path = EXCLUDED.storage_path,
                content_type = EXCLUDED.content_type,
                generated_at = NOW()
            RETURNING id, file_id, size, storage_path, content_type, generated_at
            "#,
        )
        .bind(file_id)
        .bind(size.as_str())
        .bind(storage_path)
        .bind(content_type)
        .fetch_one(&self.pool)
        .await?;

        row.try_into()
    }

    async fn delete_thumbnails(&self, file_id: Uuid) -> Result<()> {
        sqlx::query("DELETE FROM file_thumbnails WHERE file_id = $1")
            .bind(file_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thumbnail_row_maps_supported_varchar_sizes() {
        for (stored, expected) in [
            ("sm", ThumbnailSize::Sm),
            ("md", ThumbnailSize::Md),
            ("lg", ThumbnailSize::Lg),
        ] {
            let row = ThumbnailRow {
                id: Uuid::nil(),
                file_id: Uuid::nil(),
                size: stored.to_owned(),
                storage_path: "thumb.webp".to_owned(),
                content_type: "image/webp".to_owned(),
                generated_at: Utc::now(),
            };
            assert_eq!(FileThumbnail::try_from(row).unwrap().size, expected);
        }
    }

    #[test]
    fn thumbnail_row_rejects_unknown_varchar_size() {
        let row = ThumbnailRow {
            id: Uuid::nil(),
            file_id: Uuid::nil(),
            size: "unknown".to_owned(),
            storage_path: "thumb.webp".to_owned(),
            content_type: "image/webp".to_owned(),
            generated_at: Utc::now(),
        };

        assert!(FileThumbnail::try_from(row).is_err());
    }
}
