# Fix Shared Section Display Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Fix two issues:
1. `/shares` page doesn't show group shares (only shows public shares with tokens)
2. When viewing a shared directory, users should see share information

**Architecture:** 
- Backend: Create new method to get ALL shares (public + user + group) for the shares page
- Frontend: Update Share type to handle optional tokens, add share type indicators

---

## Task 1: Create Backend Method to Get All User Shares

**Files:**
- Modify: `backend/crates/storage/src/metadata.rs`

**Step 1: Add get_user_all_shares method**

Add a new method after `get_user_public_shares`:

```rust
/// Get all active shares created by a specific user (public, user, and group shares).
pub async fn get_user_all_shares(&self, user_id: Uuid) -> Result<Vec<OwnedPublicShare>> {
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
            s.tenant_id,
            COALESCE(s.file_id, s.folder_id) AS resource_id,
            CASE
                WHEN s.file_id IS NOT NULL THEN 'file'
                ELSE 'folder'
            END AS resource_type,
            COALESCE(f.name, fo.name) AS resource_name
        FROM shares s
        LEFT JOIN files f ON f.id = s.file_id
        LEFT JOIN folders fo ON fo.id = s.folder_id
        WHERE s.created_by = $1
          AND s.revoked_at IS NULL
        ORDER BY s.created_at DESC
        "#,
    )
    .bind(user_id)
    .fetch_all(&self.pool)
    .await?;

    let mut shares = Vec::with_capacity(rows.len());
    for row in rows {
        let permissions_str: String = row.try_get("permissions")?;
        let permissions = Self::permission_from_db_value(&permissions_str);

        shares.push(OwnedPublicShare {
            share: Share {
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
            },
            resource_id: row.try_get("resource_id")?,
            resource_type: row.try_get("resource_type")?,
            resource_name: row.try_get("resource_name")?,
        });
    }

    Ok(shares)
}
```

**Step 2: Check compilation**

```bash
cd /Users/scolak/Projects/x/rustshare/backend && cargo check --package rustshare-storage 2>&1 | head -20
```

**Step 3: Commit**

```bash
git add backend/crates/storage/src/metadata.rs
git commit -m "feat: add get_user_all_shares to include group shares"
```

---

## Task 2: Update list_user_shares Handler

**Files:**
- Modify: `backend/server/src/handlers/shares.rs`

**Step 1: Update OwnedShareResponse to make share_token optional**

```rust
#[derive(Serialize)]
pub struct OwnedShareResponse {
    pub id: Uuid,
    pub resource_id: Uuid,
    pub resource_type: String,
    pub resource_name: String,
    pub share_token: Option<String>,  // Changed from String to Option<String>
    pub permissions: SharePermissions,
    pub password_protected: bool,
    pub access_count: i32,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    // Add share type indicators
    pub recipient_user_id: Option<Uuid>,
    pub recipient_group_id: Option<Uuid>,
}
```

**Step 2: Update list_user_shares to use new method**

Change the handler to use `get_user_all_shares`:

```rust
pub async fn list_user_shares(
    State(state): State<AppState>,
    AuthenticatedUser { user_id, .. }: AuthenticatedUser,
) -> Result<Json<Vec<OwnedShareResponse>>, (StatusCode, String)> {
    let shares = state
        .metadata_store
        .get_user_all_shares(user_id)  // Use new method
        .await
        .map_err(|error| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to list shares: {error}"),
            )
        })?;

    let response = shares
        .into_iter()
        .map(|entry| {
            let share = entry.share;

            OwnedShareResponse {
                id: share.id,
                resource_id: entry.resource_id,
                resource_type: entry.resource_type,
                resource_name: entry.resource_name,
                share_token: share.share_token,  // Now Optional
                permissions: share.permissions,
                password_protected: share.password_hash.is_some(),
                access_count: share.access_count,
                expires_at: share.expires_at,
                created_at: share.created_at,
                recipient_user_id: share.recipient_user_id,
                recipient_group_id: share.recipient_group_id,
            }
        })
        .collect();

    Ok(Json(response))
}
```

**Step 3: Check compilation**

```bash
cd /Users/scolak/Projects/x/rustshare/backend && cargo check --package rustshare-server 2>&1 | head -20
```

**Step 4: Commit**

```bash
git add backend/server/src/handlers/shares.rs
git commit -m "feat: update list_user_shares to include all share types"
```

---

## Task 3: Update Frontend Share Type

**Files:**
- Modify: `frontend/src/lib/api/types.ts`

**Step 1: Update Share interface**

Change from:
```typescript
export interface Share {
	id: string;
	resource_id: string;
	resource_type: 'file' | 'folder';
	resource_name?: string;
	share_token: string;  // Required
	permissions: 'View' | 'Edit' | 'Admin';
	upload_only: boolean;
	password_protected: boolean;
	access_count: number;
	expires_at: string | null;
	created_at: string;
	created_by?: string;
	// Share type indicators
	recipient_user_id?: string | null;
	recipient_group_id?: string | null;
}
```

To:
```typescript
export interface Share {
	id: string;
	resource_id: string;
	resource_type: 'file' | 'folder';
	resource_name?: string;
	share_token: string | null;  // Optional - null for group/user shares
	permissions: 'View' | 'Edit' | 'Admin';
	upload_only: boolean;
	password_protected: boolean;
	access_count: number;
	expires_at: string | null;
	created_at: string;
	created_by?: string;
	// Share type indicators
	recipient_user_id?: string | null;
	recipient_group_id?: string | null;
}

// Helper type for share classification
export type ShareType = 'public' | 'user' | 'group';

export function getShareType(share: Share): ShareType {
	if (share.recipient_group_id) return 'group';
	if (share.recipient_user_id) return 'user';
	return 'public';
}
```

**Step 2: Commit**

```bash
git add frontend/src/lib/api/types.ts
git commit -m "feat: make share_token optional, add share type helpers"
```

---

## Task 4: Update Shares Page UI

**Files:**
- Modify: `frontend/src/routes/(app)/shares/+page.svelte`

**Step 1: Add share type display**

Import the helper:
```typescript
import { getShareType, type ShareType } from '$lib/api/types';
```

Add a function to get share type label:
```typescript
function getShareTypeLabel(share: Share): string {
	const type = getShareType(share);
	switch (type) {
		case 'group': return 'Group Share';
		case 'user': return 'Shared with User';
		case 'public': return 'Public Link';
	}
}

function getShareTypeIcon(share: Share): typeof Globe {
	const type = getShareType(share);
	switch (type) {
		case 'group': return Users;
		case 'user': return User;
		case 'public': return Globe;
	}
}
```

**Step 2: Update the share list display**

Find the share list rendering and add share type badges. Look for the `{#each shares as share}` block and add type indicators.

**Step 3: Handle missing share tokens**

For group/user shares without tokens, don't show the "Copy Link" button or show "No public link".

**Step 4: Commit**

```bash
git add frontend/src/routes/(app)/shares/+page.svelte
git commit -m "feat: show share types in shares page"
```

---

## Task 5: Show Share Info in File Browser

**Files:**
- Modify: `frontend/src/lib/files/FileExplorer.svelte` or relevant file browser component

**Step 1: Add API to get shares for current folder**

Check if there's already an API to list shares for a folder. If not, add one:

```typescript
// In shares.ts
export async function listFolderShares(folderId: string): Promise<Share[]> {
	return apiClient.get<Share[]>(`/folders/${folderId}/shares`);
}
```

**Step 2: Show share indicator in file browser**

For folders that have shares, show a share icon/badge in the file browser.

**Step 3: Commit**

```bash
git add frontend/
git commit -m "feat: show share indicators in file browser"
```

---

## Task 6: Final Verification

**Step 1: Check backend compilation**

```bash
cd /Users/scolak/Projects/x/rustshare/backend && cargo check 2>&1 | tail -5
```

**Step 2: Check frontend compilation**

```bash
cd /Users/scolak/Projects/x/rustshare/frontend && npm run check 2>&1 | tail -10
```

**Step 3: Run tests**

```bash
cd /Users/scolak/Projects/x/rustshare/backend && cargo test share 2>&1 | tail -20
```

---

## Summary

| Task | Description |
|------|-------------|
| 1 | Add `get_user_all_shares` method in metadata.rs |
| 2 | Update `list_user_shares` handler to use new method |
| 3 | Update frontend Share type with optional token |
| 4 | Update shares page UI with share type badges |
| 5 | Add share indicators in file browser |
| 6 | Final verification |

**Estimated effort:** 30-45 minutes CC+gstack
