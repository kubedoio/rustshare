//! Chat integration trait implementations for storage types.

use anyhow::Result;
use rustshare_core::domain::{File, Folder, Share, SharePermissions, UserId};
use rustshare_core::events::{Event, EventBroadcaster};
use rustshare_core::services::{
    ChatEventStoreOps as EventStoreOps, ChatMetadataStoreOps as MetadataStoreOps,
};
use uuid::Uuid;

use crate::{EventStore, MetadataStore};
use sqlx::Row;

fn permission_from_db_value(value: &str) -> SharePermissions {
    // Keep in sync with MetadataStore::permission_from_db_value.
    // Duplicated here so this module can remain self-contained.
    match value {
        "Edit" => SharePermissions::Edit,
        "Admin" => SharePermissions::Admin,
        _ => SharePermissions::View,
    }
}

#[allow(async_fn_in_trait)]
impl MetadataStoreOps for MetadataStore {
    async fn get_share_by_token(&self, token: &str, tenant_id: Uuid) -> Result<Option<Share>> {
        self.get_share_by_token(token, tenant_id).await
    }

    async fn get_share_by_token_unscoped(&self, token: &str) -> Result<Option<Share>> {
        self.get_share_by_token_unscoped(token).await
    }

    async fn find_file_by_id(&self, id: Uuid, owner_id: Uuid) -> Result<Option<File>> {
        self.find_file_by_id(id, owner_id).await
    }

    async fn find_folder_by_id(&self, id: Uuid, owner_id: Uuid) -> Result<Option<Folder>> {
        self.find_folder_by_id(id, owner_id).await
    }

    async fn get_user_shares(&self, user_id: UserId) -> Result<Vec<Share>> {
        let rows = sqlx::query(
            r#"
            SELECT
                s.id,
                s.file_id,
                s.folder_id,
                s.share_token,
                s.recipient_user_id,
                s.recipient_group_id,
                s.created_by,
                s.permissions,
                s.password_hash,
                s.expires_at,
                s.upload_only,
                s.access_count,
                s.created_at,
                s.revoked_at,
                s.tenant_id
            FROM shares s
            WHERE s.created_by = $1
              AND s.revoked_at IS NULL
            ORDER BY s.created_at DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool().clone())
        .await?;

        let mut shares = Vec::with_capacity(rows.len());
        for row in rows {
            let permissions = permission_from_db_value(row.try_get("permissions")?);
            shares.push(Share {
                id: row.try_get("id")?,
                file_id: row.try_get("file_id")?,
                folder_id: row.try_get("folder_id")?,
                share_token: row.try_get("share_token")?,
                recipient_user_id: row.try_get("recipient_user_id")?,
                recipient_group_id: row.try_get("recipient_group_id")?,
                created_by: row.try_get("created_by")?,
                permissions,
                password_hash: row.try_get("password_hash")?,
                expires_at: row.try_get("expires_at")?,
                upload_only: row.try_get("upload_only")?,
                access_count: row.try_get("access_count")?,
                created_at: row.try_get("created_at")?,
                revoked_at: row.try_get("revoked_at")?,
                tenant_id: row.try_get("tenant_id")?,
            });
        }

        Ok(shares)
    }
}

#[allow(async_fn_in_trait)]
impl EventStoreOps for EventStore {
    async fn append(&self, event: &Event, broadcaster: &EventBroadcaster) -> Result<()> {
        self.append(event, broadcaster).await
    }
}
