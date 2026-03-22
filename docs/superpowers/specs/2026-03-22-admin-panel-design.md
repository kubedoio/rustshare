# Admin Panel — Design Spec

**Date:** 2026-03-22
**Status:** Approved
**Scope:** Sub-project 3 of the admin/UI enhancement roadmap. Covers the full admin panel: user management, group management (definition only), OIDC/SSO runtime config, SMTP config, webhooks, and a unified audit log.

---

## 1. Architecture Overview

### Approach
Option A — modular route group with typed config tables.

### Frontend
A new SvelteKit route group at `frontend/src/routes/admin/` with a dedicated layout (`+layout.svelte`) that:
- Reads `is_admin` from the current auth store
- Redirects non-admins to `/dashboard`
- Renders its own admin shell (sidebar with 5 tabs, minimal header with admin badge)

The existing `Sidebar` and `Header` components are **not** reused. The admin panel gets a visually distinct shell.

### Backend
All new endpoints live under `/api/v1/admin/...`, consistent with the frozen API contract. A `require_admin` extractor wraps all admin routes — validates the session cookie and asserts `is_admin = true`, returning 403 otherwise. The existing replication admin endpoints (`/api/v1/admin/replication/...`) remain untouched.

### Database
Five new migrations:
- `groups` + `group_members`
- `oidc_config`
- `smtp_config`
- `webhook_configs`
- `admin_actions`

One column addition:
- `users.disabled_at TIMESTAMPTZ` — nullable; non-null means account is disabled

---

## 2. Database Schema

### `groups`
```sql
id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
name         TEXT UNIQUE NOT NULL,
description  TEXT,
created_by   UUID REFERENCES users(id) ON DELETE SET NULL,
created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
updated_at   TIMESTAMPTZ NOT NULL DEFAULT now()
```

### `group_members`
```sql
id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
group_id    UUID NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
user_id     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
added_by    UUID REFERENCES users(id) ON DELETE SET NULL,
added_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
UNIQUE (group_id, user_id)
```

### `oidc_config`
Single-row table (enforced by `CHECK (id = '00000000-0000-0000-0000-000000000001'::uuid)`).
```sql
id                    UUID PRIMARY KEY,
enabled               BOOL NOT NULL DEFAULT false,
provider_name         TEXT,
client_id             TEXT,
client_secret_enc     TEXT,   -- encrypted with server-side key via crypto crate
issuer_url            TEXT,
scopes                TEXT[],
auto_provision_users  BOOL NOT NULL DEFAULT false,
updated_by            UUID REFERENCES users(id) ON DELETE SET NULL,
updated_at            TIMESTAMPTZ NOT NULL DEFAULT now()
```

### `smtp_config`
Single-row table (same fixed-UUID pattern).
```sql
id               UUID PRIMARY KEY,
enabled          BOOL NOT NULL DEFAULT false,
host             TEXT,
port             INT,
username         TEXT,
password_enc     TEXT,   -- encrypted
from_address     TEXT,
from_name        TEXT,
tls_mode         TEXT CHECK (tls_mode IN ('starttls', 'tls', 'none')),
updated_by       UUID REFERENCES users(id) ON DELETE SET NULL,
updated_at       TIMESTAMPTZ NOT NULL DEFAULT now()
```

### `webhook_configs`
```sql
id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
name         TEXT NOT NULL,
url          TEXT NOT NULL,
secret_enc   TEXT,   -- HMAC signing secret, encrypted
enabled      BOOL NOT NULL DEFAULT true,
events       TEXT[] NOT NULL,  -- e.g. ['file.uploaded', 'user.created']
created_by   UUID REFERENCES users(id) ON DELETE SET NULL,
created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
updated_at   TIMESTAMPTZ NOT NULL DEFAULT now()
```

Supported event types:
`file.uploaded`, `file.deleted`, `file.restored`, `folder.created`, `folder.deleted`,
`share.created`, `share.revoked`, `user.created`, `user.disabled`, `user.deleted`

### `admin_actions`
```sql
id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
actor_id      UUID REFERENCES users(id) ON DELETE SET NULL,
action_type   TEXT NOT NULL,
target_type   TEXT,     -- 'user' | 'group' | 'config' | 'webhook'
target_id     UUID,     -- nullable
detail        JSONB,    -- context-specific payload
performed_at  TIMESTAMPTZ NOT NULL DEFAULT now()
```

`action_type` values:
`user.created`, `user.disabled`, `user.enabled`, `user.deleted`, `user.quota_changed`,
`group.created`, `group.deleted`, `group.member_added`, `group.member_removed`,
`config.oidc_updated`, `config.smtp_updated`,
`webhook.created`, `webhook.updated`, `webhook.deleted`

### `users` table addition
```sql
ALTER TABLE users ADD COLUMN disabled_at TIMESTAMPTZ;
```
Login handler (`POST /api/v1/auth/login`) checks `disabled_at IS NOT NULL` and returns `403 Forbidden` with body `{"error": "account_disabled"}`.

---

## 3. Backend API Endpoints

All routes require `require_admin` extractor (403 if not admin, 401 if unauthenticated).

### Users

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/v1/admin/users` | List users. Query: `search`, `status` (active\|disabled), `page`, `per_page` |
| POST | `/api/v1/admin/users` | Create user. Body: `username`, `email`, `password`, `is_admin`, `storage_quota_bytes` |
| GET | `/api/v1/admin/users/:id` | User detail + derived `storage_used_bytes` |
| PATCH | `/api/v1/admin/users/:id` | Update `display_name`, `email`, `storage_quota_bytes`, `is_admin` |
| POST | `/api/v1/admin/users/:id/disable` | Set `disabled_at = now()`. Logs `user.disabled` to `admin_actions`. Invalidates all active sessions. |
| POST | `/api/v1/admin/users/:id/enable` | Set `disabled_at = NULL`. Logs `user.enabled`. |
| DELETE | `/api/v1/admin/users/:id` | Hard delete: purge files from object storage (async background job), delete DB rows. Logs `user.deleted`. |

### Groups

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/v1/admin/groups` | List groups with `member_count` |
| POST | `/api/v1/admin/groups` | Create group. Body: `name`, `description` |
| GET | `/api/v1/admin/groups/:id` | Group detail + member list |
| PATCH | `/api/v1/admin/groups/:id` | Update `name`, `description` |
| DELETE | `/api/v1/admin/groups/:id` | Delete group and all memberships |
| POST | `/api/v1/admin/groups/:id/members` | Add member. Body: `user_id` |
| DELETE | `/api/v1/admin/groups/:id/members/:user_id` | Remove member |

### OIDC/SSO Config

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/v1/admin/config/oidc` | Get config. `client_secret` returned as `"***"` if set |
| PUT | `/api/v1/admin/config/oidc` | Full update. Encrypts secret before storing. Logs `config.oidc_updated` |
| POST | `/api/v1/admin/config/oidc/test` | Fetch OIDC discovery URL; return success/error with detail |

### SMTP Config

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/v1/admin/config/smtp` | Get config. `password` redacted |
| PUT | `/api/v1/admin/config/smtp` | Full update. Logs `config.smtp_updated` |
| POST | `/api/v1/admin/config/smtp/test` | Send test email to the calling admin's email address |

### Webhooks

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/v1/admin/integrations/webhooks` | List webhooks |
| POST | `/api/v1/admin/integrations/webhooks` | Create webhook. Logs `webhook.created` |
| PATCH | `/api/v1/admin/integrations/webhooks/:id` | Update url, events, enabled, name. Logs `webhook.updated` |
| DELETE | `/api/v1/admin/integrations/webhooks/:id` | Delete. Logs `webhook.deleted` |
| POST | `/api/v1/admin/integrations/webhooks/:id/test` | Fire a `ping` event to the webhook URL |

### Audit Log

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/v1/admin/audit` | Unified log. Query: `type` (share_access\|security_event\|admin_action\|all), `user_id`, `from`, `to`, `page`, `per_page` |

The handler unions `share_access_log`, `user_security_events`, and `admin_actions`, normalises them into a common shape `{ id, occurred_at, actor_label, action_type, target_label, detail }`, and returns paginated results ordered by `occurred_at DESC`.

---

## 4. Frontend Routes & Components

### Routes (`frontend/src/routes/admin/`)

| Route | Description |
|-------|-------------|
| `/admin/` | Redirects to `/admin/users` |
| `/admin/+layout.svelte` | Admin shell: sidebar nav (Users, Groups, OIDC/SSO, Integrations, Audit), `is_admin` guard |
| `/admin/users/` | User list + create button |
| `/admin/users/[id]/` | User detail: edit form, disable/enable/delete actions |
| `/admin/groups/` | Group list + create button |
| `/admin/groups/[id]/` | Group detail + member management |
| `/admin/oidc/` | OIDC/SSO config form + test connection |
| `/admin/integrations/` | Tabbed: Webhooks | SMTP |
| `/admin/audit/` | Unified audit log with filter bar |

### Components (`frontend/src/lib/components/admin/`)

**Layout**
- `AdminLayout.svelte` — shell with sidebar, breadcrumb, admin badge

**Users**
- `UserTable.svelte` — paginated table, search input, status badge (active/disabled), action menu (edit, disable/enable, delete)
- `CreateUserModal.svelte` — fields: username, email, password, is_admin toggle, quota (GB input)
- `UserDetailForm.svelte` — edit quota/email/display name/is_admin; separate confirm dialogs for disable and delete

**Groups**
- `GroupTable.svelte` — list with member count, create/delete actions
- `GroupMemberList.svelte` — member list with `UserSearchInput` for adding, remove button per row
- `CreateGroupModal.svelte` — name + description fields

**Config**
- `OidcConfigForm.svelte` — issuer URL, client ID, client secret (masked), scopes, auto-provision toggle, test connection button with live status feedback
- `SmtpConfigForm.svelte` — host, port, TLS mode select, username, password (masked), from address/name, send test email button

**Webhooks**
- `WebhookList.svelte` — rows with enabled toggle, event type badges, test/edit/delete actions
- `CreateWebhookModal.svelte` — name, url, secret, event type checkboxes (one per supported event)

**Audit**
- `AuditTable.svelte` — columns: timestamp, actor, action type, target, detail (expandable row); filter bar: type select, user search, date-range picker

**Shared**
- `UserSearchInput.svelte` — debounced typeahead hitting `GET /api/v1/admin/users?search=...`. Reused in GroupMemberList and later in the share dialog (Sub-project 2).

---

## 5. Testing

### Backend integration tests (`backend/tests/`)

- **`admin_require_admin_test.rs`** — every `/api/v1/admin/...` route returns 401 for unauthenticated, 403 for non-admin session.
- **`admin_users_test.rs`** — full lifecycle: create → fetch → update quota → disable (verify login returns 403) → enable (verify login works) → hard delete (verify files purged and DB rows gone). Assert `admin_actions` row written for each mutating action.
- **`admin_groups_test.rs`** — create group → add member → assert uniqueness constraint on duplicate add → remove member → delete group.
- **`admin_config_oidc_test.rs`** — write config → read back (verify `client_secret` is `"***"`) → update → test-connection with mock HTTP. Verify `admin_actions` row written.
- **`admin_config_smtp_test.rs`** — write config → read back (password redacted) → update. Test-email endpoint (mock SMTP or verify handler logic unit-tested).
- **`admin_webhooks_test.rs`** — create → list → update events → test-fire (mock HTTP receptor asserting HMAC header) → delete.
- **`admin_audit_test.rs`** — generate events in all three sources, query with each filter combination (`type`, `user_id`, `from`/`to`), assert correct rows and pagination.

### Frontend unit tests (`frontend/src/lib/components/admin/__tests__/`)

- `UserTable.test.ts` — renders user rows, search filters list, status badge variants (active/disabled)
- `AuditTable.test.ts` — renders rows, expands detail, filter controls emit correct query params
- `OidcConfigForm.test.ts` — form validation, secret field masking, test-connection states (idle/loading/success/error)
- `CreateUserModal.test.ts` — field validation, submit calls correct API shape, modal closes on success
- `UserSearchInput.test.ts` — debounce fires after delay, renders results, emits selection

### E2E (Playwright, extending existing suite in `frontend/tests/`)

- Admin login → `/admin/users` → create user → verify in list
- Disable user → verify that user's login returns error → re-enable → verify login succeeds
- Create group → add member via typeahead → remove member → delete group
- Update OIDC config → reload page → verify values persisted
- Perform a user disable action → navigate to `/admin/audit` → verify row appears with correct actor, action type, and target

---

## 6. Out of Scope (Deferred)

- Sharing files/folders with groups — Sub-project 2
- File viewer / inline editor — Sub-project 1
- UI theme support — Sub-project 4
- Mobile admin access
- Role-based access control beyond the binary `is_admin` flag
- Webhook retry logic and delivery log (can be added later)
