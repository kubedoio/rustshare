# Production Workflow System Design

**Date:** 2026-04-02  
**Status:** Approved  
**Scope:** Move the MVP workflow/invite system to a production-grade backend-driven implementation with database persistence, SMTP email delivery, cryptographic invite tokens, and real user registration via invite acceptance.

---

## 1. Goals

- Replace client-side `localStorage` workflow storage with a PostgreSQL-backed `workflows` table.
- Replace client-side base64 invite tokens with cryptographically random, database-stored `invite_tokens`.
- Deliver invite emails via SMTP using the existing `smtp_config` admin configuration.
- Ensure the topbar invite button is only visible when the `invite_email` workflow is active.
- Make the invite acceptance page (`/invite/[token]`) actually create user accounts in the backend.
- Prevent enabling the invite workflow unless SMTP is properly configured.
- Add a `[+ New]` button in the admin workflow UI as scaffolding for future workflow types.

---

## 2. Non-Goals

- Background email queue/worker (out of scope; emails sent inline).
- Full "create custom workflow" feature (the `[+ New]` button shows "coming soon").
- Non-invite workflow triggers (announcements, T&C changes, share workflows) — schema supports them, but only `invite_email` is wired end-to-end.
- Email templating engine beyond simple variable substitution.

---

## 3. Database Schema

### 3.1 `workflows`

```sql
CREATE TABLE workflows (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    key VARCHAR(50) NOT NULL,
    name VARCHAR(100) NOT NULL,
    trigger_type VARCHAR(50) NOT NULL,
    status VARCHAR(20) NOT NULL DEFAULT 'draft' CHECK (status IN ('active', 'draft')),
    subject TEXT,
    body TEXT,
    terms_enabled BOOLEAN NOT NULL DEFAULT false,
    terms_text TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_by UUID REFERENCES users(id) ON DELETE SET NULL,
    UNIQUE (tenant_id, key)
);

CREATE INDEX idx_workflows_tenant ON workflows(tenant_id);
CREATE INDEX idx_workflows_key ON workflows(key);
```

**Migration behavior:** Pre-seed one row on creation:
- `key` = `'invite_email'`
- `name` = `'Invite Email'`
- `trigger_type` = `'manual'`
- `status` = `'draft'` (must be explicitly enabled by an admin)
- `subject` = `"You've been invited to RustShare"`
- `body` = default multi-line invite template with `{{recipient_name}}`, `{{sender_name}}`, `{{invite_link}}` placeholders
- `terms_enabled` = `true`
- `terms_text` = default Terms of Service / Privacy Policy text

### 3.2 `invite_tokens`

```sql
CREATE TABLE invite_tokens (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    token VARCHAR(64) NOT NULL,
    sender_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    recipient_email VARCHAR(255) NOT NULL,
    workflow_id UUID NOT NULL REFERENCES workflows(id) ON DELETE RESTRICT,
    expires_at TIMESTAMPTZ NOT NULL,
    used_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (tenant_id, token)
);

CREATE INDEX idx_invite_tokens_token ON invite_tokens(token);
CREATE INDEX idx_invite_tokens_sender ON invite_tokens(sender_id, created_at);
CREATE INDEX idx_invite_tokens_tenant ON invite_tokens(tenant_id);
```

**Token generation:** 32 cryptographically random bytes from `rand::thread_rng()`, hex-encoded to 64 characters. No base64 client encoding.

**Expiry:** 7 days from creation (`NOW() + INTERVAL '7 days'`).

---

## 4. Backend Architecture

### 4.1 New Dependencies

Add to `backend/crates/core/Cargo.toml` (since `EmailService` lives in `rustshare_core`):

```toml
lettre = { version = "0.11", default-features = false, features = ["builder", "smtp-transport", "tokio1-rustls-tls", "pool"] }
```

Also add `lettre` to `backend/server/Cargo.toml` if the server crate constructs `EmailService` directly:

```toml
lettre = { workspace = true }
```

### 4.2 New Service: `rustshare_core::services::email_service`

Create `backend/crates/core/src/services/email_service.rs`:

- **`EmailService`** struct holding a `PgPool` reference.
- **`async fn send_invite_email(...)`**:
  1. Load the singleton `smtp_config` row.
  2. Verify `enabled = true` and required fields (`host`, `port`, `from_address`) are present.
  3. Decrypt `password_enc` via `rustshare_crypto::decrypt_secret` if present.
  4. Build SMTP transport via `lettre::AsyncSmtpTransport::<Tokio1Executor>`.
  5. Substitute `{{recipient_name}}`, `{{sender_name}}`, `{{invite_link}}` in workflow body/subject.
  6. Send message. Return `Result<(), EmailError>`.

**Error variants:** `SmtpNotConfigured`, `SmtpSendFailed(String)`, `DecryptFailed`.

### 4.3 New Admin Handler: `backend/server/src/handlers/admin/workflows.rs`

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/v1/admin/workflows` | List all workflows |
| GET | `/api/v1/admin/workflows/{id}` | Get single workflow by ID |
| PUT | `/api/v1/admin/workflows/{id}` | Update editable fields (subject, body, terms_enabled, terms_text, status) |
| POST | `/api/v1/admin/workflows/{id}/enable` | Set status to `active` after SMTP validation |
| POST | `/api/v1/admin/workflows/{id}/disable` | Set status to `draft` |

**Enable validation rules:**
- Workflow key must be `invite_email` (only supported trigger for now).
- `smtp_config` must exist with `enabled = true`.
- `host`, `port`, `from_address` must all be non-null.
- If any rule fails, return `400 Bad Request` with a specific error message.

**Audit logging:** Every mutating admin action (`update`, `enable`, `disable`) writes a row to `admin_actions` with `action_type` = `workflow.updated`, `workflow.enabled`, or `workflow.disabled`.

### 4.4 New Invite Handlers: `backend/server/src/handlers/invites.rs`

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| POST | `/api/v1/invites` | Yes | Create invite token and send email |
| GET | `/api/v1/invites/{token}` | No | Validate token, return sender + workflow config |
| POST | `/api/v1/invites/{token}/accept` | No | Register user from invite |

Additionally, add a lightweight public feature-flags endpoint:

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| GET | `/api/v1/features` | Yes | Returns `{ "invite_enabled": boolean }` based on the active `invite_email` workflow for the current tenant |

#### POST `/api/v1/invites`

Request body:
```json
{
  "recipient_email": "colleague@company.com",
  "origin": "https://app.rustshare.io"
}
```

`origin` is the public frontend URL used to build the invite link. It should be the browser's `window.location.origin` on the client. If omitted, the backend may fall back to a configured base URL.

**Invite link construction:** `invite_link = format!("{}/invite/{}", origin, token)`

Logic:
1. Authenticate caller.
2. Fetch the `invite_email` workflow. If missing or `status != 'active'`, return `422 Unprocessable Entity`.
3. Generate random 64-char hex token.
4. Insert `invite_tokens` row with `expires_at = NOW() + 7 days`.
5. Build `invite_link = "{origin}/invite/{token}"`.
6. Call `EmailService::send_invite_email(...)`.
7. **On email failure:** delete the inserted token row and return `502 Bad Gateway` with SMTP detail.
8. **On success:** return:
```json
{
  "token": "abc123...",
  "invite_link": "https://.../invite/abc123...",
  "expires_at": "2026-04-09T21:40:49Z"
}
```

#### GET `/api/v1/invites/{token}`

Logic:
1. Look up token in `invite_tokens`.
2. If not found, expired, used, or revoked → return `410 Gone`.
3. Load associated workflow.
4. Return:
```json
{
  "sender_name": "Alice",
  "recipient_email": "bob@example.com",
  "subject": "...",
  "body": "...",
  "terms_enabled": true,
  "terms_text": "...",
  "expires_at": "..."
}
```

#### POST `/api/v1/invites/{token}/accept`

Request body:
```json
{
  "display_name": "Bob",
  "email": "bob@example.com",
  "password": "secure-password",
  "terms_accepted": true
}
```

Logic:
1. Look up token. Validate not found/expired/used/revoked → `410 Gone`.
2. Verify `email` matches `recipient_email` → `400` if mismatch.
3. Validate `password` length ≥ 8 → `400` if too short.
4. If `terms_enabled` is true and `terms_accepted` is false → `400`.
5. Check if `email` already exists in `users` → `409 Conflict`.
6. Hash password with `rustshare_crypto::PasswordHasher`.
7. Insert user into `users` table (reuse existing auth repo patterns).
8. Mark token `used_at = NOW()`.
9. Return the created user object:
```json
{
  "id": "uuid",
  "email": "bob@example.com",
  "display_name": "Bob",
  "created_at": "..."
}
```

### 4.5 Routing

In `backend/server/src/main.rs`, add:

```rust
// Admin workflows
.route("/api/v1/admin/workflows", get(handlers::admin::workflows::list_workflows))
.route("/api/v1/admin/workflows/{id}", get(handlers::admin::workflows::get_workflow))
.route("/api/v1/admin/workflows/{id}", put(handlers::admin::workflows::update_workflow))
.route("/api/v1/admin/workflows/{id}/enable", post(handlers::admin::workflows::enable_workflow))
.route("/api/v1/admin/workflows/{id}/disable", post(handlers::admin::workflows::disable_workflow))

// Public invites
.route("/api/v1/invites", post(handlers::invites::create_invite))
.route("/api/v1/invites/{token}", get(handlers::invites::get_invite))
.route("/api/v1/invites/{token}/accept", post(handlers::invites::accept_invite))
```

---

## 5. Frontend Changes

### 5.1 New API Module: `frontend/src/lib/api/workflows.ts`

```typescript
export const listWorkflows = () => apiClient.get<{ workflows: Workflow[] }>('/admin/workflows');
export const getWorkflow = (id: string) => apiClient.get<Workflow>(`/admin/workflows/${id}`);
export const updateWorkflow = (id: string, data: WorkflowUpdateRequest) =>
  apiClient.put<Workflow>(`/admin/workflows/${id}`, data);
export const enableWorkflow = (id: string) =>
  apiClient.post<Workflow>(`/admin/workflows/${id}/enable`);
export const disableWorkflow = (id: string) =>
  apiClient.post<Workflow>(`/admin/workflows/${id}/disable`);
```

### 5.2 New API Module: `frontend/src/lib/api/invites.ts`

```typescript
export const createInvite = (data: { recipient_email: string }) =>
  apiClient.post<{ token: string; invite_link: string; expires_at: string }>('/invites', data);
export const getInvite = (token: string) =>
  apiClient.get<InviteDetail>(`/invites/${token}`);
export const acceptInvite = (token: string, data: AcceptInviteRequest) =>
  apiClient.post<User>(`/invites/${token}/accept`, data);
```

### 5.3 `Topbar.svelte`

- On mount, fetch `GET /api/v1/features` (lightweight public endpoint).
- Determine if `invite_enabled` is `true`.
- **If inactive:** do not render the invite button or its container at all.
- **If active:** render the existing invite popup UI.
- Replace local `generateInviteLink` logic with `POST /api/v1/invites` call, passing `window.location.origin` as `origin`.
- Show loading state during API call, then display the returned `invite_link`.
- Handle email send failure with a user-friendly error message.

### 5.4 `admin/workflows/+page.svelte`

- Replace `localStorage` load/save with API calls to `workflows.ts`.
- Add a `[+ New]` button at the top of the workflow list.
- Clicking `[+ New]` opens a modal: title "New Workflow", body "More workflow types coming soon.", primary action disabled.
- Status toggle now calls `enableWorkflow` / `disableWorkflow` with loading spinners.
- If enable fails due to SMTP missing, show inline alert: "SMTP must be configured before this workflow can be enabled."
- Preview mode continues to show rendered subject/body/terms using the same variable substitution logic client-side.

### 5.5 `invite/[token]/+page.svelte`

- On mount, call `GET /api/v1/invites/{token}`.
- Replace local `DEFAULT_WORKFLOW` and `localStorage` lookup with the API response.
- Form submission calls `POST /api/v1/invites/{token}/accept`.
- On success, show existing success state and redirect to `/login` after 2.5s.
- Error handling:
  - `410 Gone` → show invalid/expired token UI (already exists).
  - `409 Conflict` → show "This email already has an account. Please sign in instead."
  - `400` validation errors → display inline below the form.

---

## 6. Security & Encryption

- **Invite tokens:** 256-bit random values (32 bytes hex-encoded), stored in Postgres. Not guessable. Not derived from user data.
- **SMTP password:** Already encrypted at rest via `rustshare_crypto::encrypt_secret`. Decrypted only in-memory during `EmailService::send_invite_email`.
- **Token lifecycle:** Tokens are single-use (`used_at`), time-bounded (`expires_at`), and revocable (`revoked_at`).
- **Registration validation:** Password minimum 8 characters. Email must match the invited address. Terms acceptance enforced when configured.

---

## 7. Error Handling Reference

| Scenario | HTTP Status | Response Body Pattern |
|----------|-------------|----------------------|
| SMTP not configured | `400` | `{ "error": "SMTP must be configured and enabled before this workflow can be activated" }` |
| SMTP send failure | `502` | `{ "error": "Failed to send invite email: <smtp detail>" }` |
| Invite workflow inactive | `422` | `{ "error": "Invite workflow is not active" }` |
| Token expired/used/revoked | `410` | `{ "error": "Invite link has expired or already been used" }` |
| Email mismatch on accept | `400` | `{ "error": "Email does not match the invited address" }` |
| Password too short | `400` | `{ "error": "Password must be at least 8 characters" }` |
| Terms not accepted | `400` | `{ "error": "You must accept the Terms & Conditions" }` |
| Email already exists | `409` | `{ "error": "An account with this email already exists" }` |

---

## 8. Testing Strategy

### 8.1 Backend Integration Tests

**`backend/tests/admin_workflows_test.rs`**
- `test_list_workflows` — assert the pre-seeded `invite_email` workflow is returned.
- `test_update_workflow` — update subject/body/terms, read back, assert changes persisted.
- `test_enable_workflow_requires_smtp` — enable without SMTP config → assert `400`.
- `test_enable_workflow_with_smtp` — seed SMTP config, enable → assert `status = active`.
- `test_disable_workflow` — enable then disable → assert `status = draft`.
- `test_audit_log_written` — assert `admin_actions` row exists after each mutation.

**`backend/tests/invites_test.rs`**
- `test_create_invite_workflow_inactive` — create invite when workflow is draft → `422`.
- `test_create_invite_sends_email` — seed SMTP config + enable workflow, create invite → assert token row exists, email sent (use a test SMTP sink or `lettre` stub transport).
- `test_get_invite_valid` — create invite, fetch by token → assert sender info and workflow config.
- `test_get_invite_expired` — manually set `expires_at` in the past → `410`.
- `test_accept_invite_creates_user` — accept valid invite → assert user created in DB, password hashes correctly, token `used_at` set.
- `test_accept_invite_email_mismatch` — accept with wrong email → `400`.
- `test_accept_invite_terms_required` — workflow with `terms_enabled=true`, accept without `terms_accepted=true` → `400`.
- `test_accept_invite_already_used` — accept twice → second request `410`.
- `test_accept_invite_duplicate_email` — create a user directly, then accept invite with same email → `409`.

### 8.2 Frontend Tests

No new automated unit tests are required. The existing Svelte component patterns and API client are well-established. Manual dogfooding of the full flow is sufficient for this scope.

---

## 9. Migration Plan

1. **Run migrations:**
   - `backend/migrations/20260402000001_create_workflows.sql`
   - `backend/migrations/20260402000002_create_invite_tokens.sql`
2. **Backfill:** The `workflows` migration pre-seeds the `invite_email` row with default template content for the default tenant.
3. **Frontend cutover:** Once the backend APIs are live, the frontend stops reading/writing `localStorage` key `rs_workflows`.
4. **Cleanup:** After deployment is stable, remove any orphaned `rs_workflows` entries from users' browsers (not required, but harmless).

---

## 10. Open Questions / Future Work

- **Announcement workflows:** Trigger on `login` event. Will require hooking into the auth session creation flow.
- **T&C change workflows:** Trigger on `login` when `terms_text` has been updated since the user's last acceptance.
- **Share workflows:** Trigger on `share_created` event.
- **Email queue:** If email volume increases, migrate `EmailService` to write to a queue table and process via a background worker.
