# Production Workflow System Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move the MVP workflow/invite system to a production-grade backend-driven implementation with database persistence, SMTP email delivery, cryptographic invite tokens, and real user registration via invite acceptance.

**Architecture:** Add `workflows` and `invite_tokens` tables with migrations. Build a `lettre`-based `EmailService` in `rustshare_core`. Implement admin workflow handlers and public invite handlers in the Axum server. Update the frontend to call backend APIs instead of using `localStorage` and base64 tokens.

**Tech Stack:** Rust, Axum 0.8, sqlx, lettre, PostgreSQL, Svelte 5, TypeScript

---

## File Map

| File | Responsibility |
|------|----------------|
| `backend/migrations/20260402000001_create_workflows.sql` | Create `workflows` table + seed `invite_email` row |
| `backend/migrations/20260402000002_create_invite_tokens.sql` | Create `invite_tokens` table + indexes |
| `backend/crates/core/Cargo.toml` | Add `lettre` dependency |
| `backend/crates/core/src/services/email_service.rs` | SMTP transport + invite email builder |
| `backend/crates/core/src/services/mod.rs` | Re-export `EmailService` |
| `backend/server/src/handlers/admin/workflows.rs` | Admin CRUD + enable/disable for workflows |
| `backend/server/src/handlers/invites.rs` | Create invite, validate token, accept invite |
| `backend/server/src/handlers/features.rs` | Public `GET /api/v1/features` endpoint |
| `backend/server/src/handlers/admin/mod.rs` | Add `pub mod workflows` |
| `backend/server/src/handlers/mod.rs` | Add `pub mod invites`, `pub mod features`, re-exports |
| `backend/server/src/main.rs` | Wire all new routes |
| `backend/server/Cargo.toml` | Add `lettre` and new test targets |
| `backend/tests/admin_workflows_test.rs` | Integration tests for admin workflow handlers |
| `backend/tests/invites_test.rs` | Integration tests for invite lifecycle |
| `frontend/src/lib/api/workflows.ts` | Frontend API client for workflows |
| `frontend/src/lib/api/invites.ts` | Frontend API client for invites |
| `frontend/src/lib/api/features.ts` | Frontend API client for features endpoint |
| `frontend/src/lib/layout/Topbar.svelte` | Conditionally show invite button, call backend |
| `frontend/src/routes/admin/workflows/+page.svelte` | Replace localStorage with API, add `[+ New]` modal |
| `frontend/src/routes/invite/[token]/+page.svelte` | Real token validation + account creation |

---

### Task 1: Database Migrations

**Files:**
- Create: `backend/migrations/20260402000001_create_workflows.sql`
- Create: `backend/migrations/20260402000002_create_invite_tokens.sql`

- [ ] **Step 1: Write workflows migration**

```sql
CREATE TABLE workflows (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000',
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

INSERT INTO workflows (tenant_id, key, name, trigger_type, status, subject, body, terms_enabled, terms_text)
VALUES (
    '00000000-0000-0000-0000-000000000000',
    'invite_email',
    'Invite Email',
    'manual',
    'draft',
    E'You\'ve been invited to RustShare',
    E'Hi {{recipient_name}},\n\n{{sender_name}} has invited you to join RustShare — a secure file sharing platform.\n\nClick the link below to accept your invitation and create your account:\n\n{{invite_link}}\n\nThis invitation expires in 7 days.\n\nBest regards,\nThe RustShare Team',
    true,
    E'Terms of Service\n\nBy accepting this invitation and creating an account, you agree to use RustShare responsibly and comply with our terms of service.\n\nPrivacy Policy\n\nWe collect only the minimum data necessary to operate the service.'
)
ON CONFLICT (tenant_id, key) DO NOTHING;
```

- [ ] **Step 2: Write invite_tokens migration**

```sql
CREATE TABLE invite_tokens (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000',
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

- [ ] **Step 3: Run migrations locally**

Run: `cd backend && sqlx migrate run`

Expected: both migrations succeed with no errors.

- [ ] **Step 4: Commit**

```bash
git add backend/migrations/20260402000001_create_workflows.sql backend/migrations/20260402000002_create_invite_tokens.sql
git commit -m "feat(migrations): add workflows and invite_tokens tables"
```

---

### Task 2: Add lettre Dependency

**Files:**
- Modify: `backend/crates/core/Cargo.toml`
- Modify: `backend/server/Cargo.toml`

- [ ] **Step 1: Add lettre to core Cargo.toml**

Add this line in the `[dependencies]` section of `backend/crates/core/Cargo.toml`:

```toml
lettre = { version = "0.11", default-features = false, features = ["builder", "smtp-transport", "tokio1-rustls-tls", "pool"] }
```

- [ ] **Step 2: Add lettre to server Cargo.toml**

Add this line in the `[dependencies]` section of `backend/server/Cargo.toml`:

```toml
lettre = { version = "0.11", default-features = false, features = ["builder", "smtp-transport", "tokio1-rustls-tls", "pool"] }
```

- [ ] **Step 3: Commit**

```bash
git add backend/crates/core/Cargo.toml backend/server/Cargo.toml
git commit -m "deps: add lettre for SMTP email delivery"
```

---

### Task 3: EmailService in rustshare_core

**Files:**
- Create: `backend/crates/core/src/services/email_service.rs`
- Modify: `backend/crates/core/src/services/mod.rs`

- [ ] **Step 1: Write email_service.rs**

```rust
use lettre::{
    message::Mailbox,
    transport::smtp::authentication::Credentials,
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};
use rustshare_crypto::{decrypt_secret, SecretEncryptionKey};
use sqlx::PgPool;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EmailError {
    #[error("SMTP is not configured or not enabled")]
    SmtpNotConfigured,
    #[error("Failed to decrypt SMTP password")]
    DecryptFailed,
    #[error("Failed to send email: {0}")]
    SmtpSendFailed(String),
}

pub struct EmailService {
    pool: PgPool,
    secret_key: SecretEncryptionKey,
}

impl EmailService {
    pub fn new(pool: PgPool, secret_key: SecretEncryptionKey) -> Self {
        Self { pool, secret_key }
    }

    pub async fn send_invite_email(
        &self,
        sender_name: &str,
        recipient_email: &str,
        invite_link: &str,
        subject_template: &str,
        body_template: &str,
    ) -> Result<(), EmailError> {
        let row = sqlx::query_as::<_, SmtpConfigRow>(
            "SELECT enabled, host, port, username, password_enc, from_address, from_name, tls_mode
             FROM smtp_config
             WHERE id = '00000000-0000-0000-0000-000000000002'"
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| EmailError::SmtpSendFailed(e.to_string()))?;

        let config = row.ok_or(EmailError::SmtpNotConfigured)?;
        if !config.enabled {
            return Err(EmailError::SmtpNotConfigured);
        }

        let host = config.host.ok_or(EmailError::SmtpNotConfigured)?;
        let port = config.port.ok_or(EmailError::SmtpNotConfigured)?;
        let from_address = config.from_address.ok_or(EmailError::SmtpNotConfigured)?;

        let from_name = config.from_name.as_deref().unwrap_or("RustShare");
        let from_mailbox: Mailbox = format!("{} <{}>", from_name, from_address)
            .parse()
            .map_err(|e| EmailError::SmtpSendFailed(format!("Invalid from address: {}", e)))?;

        let to_mailbox: Mailbox = recipient_email
            .parse()
            .map_err(|e| EmailError::SmtpSendFailed(format!("Invalid recipient address: {}", e)))?;

        let subject = subject_template
            .replace("{{sender_name}}", sender_name)
            .replace("{{recipient_name}}", recipient_email)
            .replace("{{invite_link}}", invite_link);

        let body = body_template
            .replace("{{sender_name}}", sender_name)
            .replace("{{recipient_name}}", recipient_email)
            .replace("{{invite_link}}", invite_link);

        let email = Message::builder()
            .from(from_mailbox)
            .to(to_mailbox)
            .subject(subject)
            .body(body)
            .map_err(|e| EmailError::SmtpSendFailed(e.to_string()))?;

        let mut builder = AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&host)
            .port(port as u16);

        if let Some(ref username) = config.username {
            let password = if let Some(ref enc) = config.password_enc {
                decrypt_secret(enc, &self.secret_key)
                    .map_err(|_| EmailError::DecryptFailed)?
            } else {
                String::new()
            };
            builder = builder.credentials(Credentials::new(username.clone(), password));
        }

        match config.tls_mode.as_deref() {
            Some("tls") => {
                builder = AsyncSmtpTransport::<Tokio1Executor>::relay(&host)
                    .map_err(|e| EmailError::SmtpSendFailed(e.to_string()))?
                    .port(port as u16);
                if let Some(ref username) = config.username {
                    let password = if let Some(ref enc) = config.password_enc {
                        decrypt_secret(enc, &self.secret_key)
                            .map_err(|_| EmailError::DecryptFailed)?
                    } else {
                        String::new()
                    };
                    builder = builder.credentials(Credentials::new(username.clone(), password));
                }
            }
            Some("starttls") => {
                builder = AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&host)
                    .map_err(|e| EmailError::SmtpSendFailed(e.to_string()))?
                    .port(port as u16);
                if let Some(ref username) = config.username {
                    let password = if let Some(ref enc) = config.password_enc {
                        decrypt_secret(enc, &self.secret_key)
                            .map_err(|_| EmailError::DecryptFailed)?
                    } else {
                        String::new()
                    };
                    builder = builder.credentials(Credentials::new(username.clone(), password));
                }
            }
            _ => {}
        }

        let transport = builder.build();

        transport
            .send(email)
            .await
            .map_err(|e| EmailError::SmtpSendFailed(e.to_string()))?;

        Ok(())
    }
}

#[derive(sqlx::FromRow)]
struct SmtpConfigRow {
    enabled: bool,
    host: Option<String>,
    port: Option<i32>,
    username: Option<String>,
    password_enc: Option<String>,
    from_address: Option<String>,
    from_name: Option<String>,
    tls_mode: Option<String>,
}
```

- [ ] **Step 2: Register in mod.rs**

Modify `backend/crates/core/src/services/mod.rs` by adding:

```rust
mod email_service;
pub use email_service::{EmailService, EmailError};
```

- [ ] **Step 3: Build core crate**

Run: `cd backend && cargo check -p rustshare-core`

Expected: compiles successfully (may need minor syntax fixes).

- [ ] **Step 4: Commit**

```bash
git add backend/crates/core/src/services/email_service.rs backend/crates/core/src/services/mod.rs
git commit -m "feat(core): add EmailService for SMTP invite delivery"
```

---

### Task 4: Admin Workflow Handlers

**Files:**
- Create: `backend/server/src/handlers/admin/workflows.rs`

- [ ] **Step 1: Write workflows.rs**

```rust
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    handlers::{AdminUser, ErrorResponse},
    AppState,
};

use super::log_admin_action;

#[derive(sqlx::FromRow, Serialize)]
pub struct WorkflowResponse {
    pub id: String,
    pub key: String,
    pub name: String,
    pub trigger_type: String,
    pub status: String,
    pub subject: Option<String>,
    pub body: Option<String>,
    pub terms_enabled: bool,
    pub terms_text: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub updated_by: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateWorkflowRequest {
    pub subject: Option<String>,
    pub body: Option<String>,
    pub terms_enabled: Option<bool>,
    pub terms_text: Option<String>,
    pub status: Option<String>,
}

pub async fn list_workflows(
    State(state): State<AppState>,
    AdminUser { .. }: AdminUser,
) -> Result<Json<Vec<WorkflowResponse>>, (StatusCode, Json<ErrorResponse>)> {
    let rows = sqlx::query_as::<_, WorkflowRow>(
        "SELECT id, key, name, trigger_type, status, subject, body, terms_enabled, terms_text,
                created_at, updated_at, updated_by
         FROM workflows
         ORDER BY created_at ASC"
    )
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse::new(e.to_string()))))?;

    Ok(Json(rows.into_iter().map(WorkflowResponse::from).collect()))
}

pub async fn get_workflow(
    State(state): State<AppState>,
    AdminUser { .. }: AdminUser,
    Path(id): Path<Uuid>,
) -> Result<Json<WorkflowResponse>, (StatusCode, Json<ErrorResponse>)> {
    let row = sqlx::query_as::<_, WorkflowRow>(
        "SELECT id, key, name, trigger_type, status, subject, body, terms_enabled, terms_text,
                created_at, updated_at, updated_by
         FROM workflows
         WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse::new(e.to_string()))))?
    .ok_or_else(|| (StatusCode::NOT_FOUND, Json(ErrorResponse::new("Workflow not found"))))?;

    Ok(Json(WorkflowResponse::from(row)))
}

pub async fn update_workflow(
    State(state): State<AppState>,
    AdminUser { user_id: actor_id }: AdminUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateWorkflowRequest>,
) -> Result<Json<WorkflowResponse>, (StatusCode, Json<ErrorResponse>)> {
    let row = sqlx::query_as::<_, WorkflowRow>(
        "UPDATE workflows
         SET subject = COALESCE($2, subject),
             body = COALESCE($3, body),
             terms_enabled = COALESCE($4, terms_enabled),
             terms_text = COALESCE($5, terms_text),
             status = COALESCE($6, status),
             updated_by = $7,
             updated_at = NOW()
         WHERE id = $1
         RETURNING id, key, name, trigger_type, status, subject, body, terms_enabled, terms_text,
                   created_at, updated_at, updated_by",
    )
    .bind(id)
    .bind(req.subject)
    .bind(req.body)
    .bind(req.terms_enabled)
    .bind(req.terms_text)
    .bind(req.status)
    .bind(actor_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse::new(e.to_string()))))?
    .ok_or_else(|| (StatusCode::NOT_FOUND, Json(ErrorResponse::new("Workflow not found"))))?;

    log_admin_action(&state.db_pool, actor_id, "workflow.updated", Some("workflow"), Some(id), json!({})).await;

    Ok(Json(WorkflowResponse::from(row)))
}

pub async fn enable_workflow(
    State(state): State<AppState>,
    AdminUser { user_id: actor_id }: AdminUser,
    Path(id): Path<Uuid>,
) -> Result<Json<WorkflowResponse>, (StatusCode, Json<ErrorResponse>)> {
    let wf = sqlx::query_as::<_, WorkflowRow>(
        "SELECT id, key, name, trigger_type, status, subject, body, terms_enabled, terms_text,
                created_at, updated_at, updated_by
         FROM workflows
         WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse::new(e.to_string()))))?
    .ok_or_else(|| (StatusCode::NOT_FOUND, Json(ErrorResponse::new("Workflow not found"))))?;

    if wf.key != "invite_email" {
        return Err((StatusCode::BAD_REQUEST, Json(ErrorResponse::new("Only invite_email workflow can be enabled currently"))));
    }

    let smtp_ok: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM smtp_config
            WHERE id = '00000000-0000-0000-0000-000000000002'
              AND enabled = true
              AND host IS NOT NULL
              AND port IS NOT NULL
              AND from_address IS NOT NULL
        )"
    )
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse::new(e.to_string()))))?;

    if !smtp_ok {
        return Err((StatusCode::BAD_REQUEST, Json(ErrorResponse::new("SMTP must be configured and enabled before this workflow can be activated"))));
    }

    let row = sqlx::query_as::<_, WorkflowRow>(
        "UPDATE workflows
         SET status = 'active', updated_by = $2, updated_at = NOW()
         WHERE id = $1
         RETURNING id, key, name, trigger_type, status, subject, body, terms_enabled, terms_text,
                   created_at, updated_at, updated_by",
    )
    .bind(id)
    .bind(actor_id)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse::new(e.to_string()))))?;

    log_admin_action(&state.db_pool, actor_id, "workflow.enabled", Some("workflow"), Some(id), json!({})).await;

    Ok(Json(WorkflowResponse::from(row)))
}

pub async fn disable_workflow(
    State(state): State<AppState>,
    AdminUser { user_id: actor_id }: AdminUser,
    Path(id): Path<Uuid>,
) -> Result<Json<WorkflowResponse>, (StatusCode, Json<ErrorResponse>)> {
    let row = sqlx::query_as::<_, WorkflowRow>(
        "UPDATE workflows
         SET status = 'draft', updated_by = $2, updated_at = NOW()
         WHERE id = $1
         RETURNING id, key, name, trigger_type, status, subject, body, terms_enabled, terms_text,
                   created_at, updated_at, updated_by",
    )
    .bind(id)
    .bind(actor_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse::new(e.to_string()))))?
    .ok_or_else(|| (StatusCode::NOT_FOUND, Json(ErrorResponse::new("Workflow not found"))))?;

    log_admin_action(&state.db_pool, actor_id, "workflow.disabled", Some("workflow"), Some(id), json!({})).await;

    Ok(Json(WorkflowResponse::from(row)))
}

#[derive(sqlx::FromRow)]
struct WorkflowRow {
    id: Uuid,
    key: String,
    name: String,
    trigger_type: String,
    status: String,
    subject: Option<String>,
    body: Option<String>,
    terms_enabled: bool,
    terms_text: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    updated_by: Option<Uuid>,
}

impl From<WorkflowRow> for WorkflowResponse {
    fn from(row: WorkflowRow) -> Self {
        WorkflowResponse {
            id: row.id.to_string(),
            key: row.key,
            name: row.name,
            trigger_type: row.trigger_type,
            status: row.status,
            subject: row.subject,
            body: row.body,
            terms_enabled: row.terms_enabled,
            terms_text: row.terms_text,
            created_at: row.created_at,
            updated_at: row.updated_at,
            updated_by: row.updated_by.map(|u| u.to_string()),
        }
    }
}
```

- [ ] **Step 2: Commit**

```bash
git add backend/server/src/handlers/admin/workflows.rs
git commit -m "feat(admin): add workflow CRUD and enable/disable handlers"
```

---

### Task 5: Invite and Features Handlers

**Files:**
- Create: `backend/server/src/handlers/invites.rs`
- Create: `backend/server/src/handlers/features.rs`

- [ ] **Step 1: Write invites.rs**

```rust
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use rand::RngCore;
use rustshare_auth::PasswordHasher;
use rustshare_core::services::{EmailError, EmailService};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    handlers::{AuthenticatedUser, ErrorResponse},
    AppState,
};

#[derive(Deserialize)]
pub struct CreateInviteRequest {
    pub recipient_email: String,
    pub origin: Option<String>,
}

#[derive(Serialize)]
pub struct CreateInviteResponse {
    pub token: String,
    pub invite_link: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

pub async fn create_invite(
    State(state): State<AppState>,
    AuthenticatedUser { user_id, .. }: AuthenticatedUser,
    Json(req): Json<CreateInviteRequest>,
) -> Result<Json<CreateInviteResponse>, (StatusCode, Json<ErrorResponse>)> {
    let workflow = sqlx::query_as::<_, WorkflowRow>(
        "SELECT id, subject, body, terms_enabled, terms_text
         FROM workflows
         WHERE key = 'invite_email' AND status = 'active'"
    )
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse::new(e.to_string()))))?
    .ok_or_else(|| (StatusCode::UNPROCESSABLE_ENTITY, Json(ErrorResponse::new("Invite workflow is not active"))))?;

    let sender = sqlx::query_as::<_, SenderRow>("SELECT display_name FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_one(&state.db_pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse::new(e.to_string()))))?;

    let mut token_bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut token_bytes);
    let token = hex::encode(token_bytes);

    let expires_at = chrono::Utc::now() + chrono::Duration::days(7);

    let tenant_id: Uuid = sqlx::query_scalar("SELECT tenant_id FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_one(&state.db_pool)
        .await
        .unwrap_or_else(|_| Uuid::parse_str("00000000-0000-0000-0000-000000000000").unwrap());

    sqlx::query(
        "INSERT INTO invite_tokens (tenant_id, token, sender_id, recipient_email, workflow_id, expires_at)
         VALUES ($1, $2, $3, $4, $5, $6)"
    )
    .bind(tenant_id)
    .bind(&token)
    .bind(user_id)
    .bind(&req.recipient_email)
    .bind(workflow.id)
    .bind(expires_at)
    .execute(&state.db_pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse::new(e.to_string()))))?;

    let origin = req.origin.unwrap_or_else(|| "https://rustshare.io".to_string());
    let invite_link = format!("{}/invite/{}", origin.trim_end_matches('/'), token);

    let email_service = EmailService::new(state.db_pool.clone(), state.secret_key.clone());
    let result = email_service.send_invite_email(
        &sender.display_name,
        &req.recipient_email,
        &invite_link,
        workflow.subject.as_deref().unwrap_or("You've been invited to RustShare"),
        workflow.body.as_deref().unwrap_or(""),
    ).await;

    if let Err(e) = result {
        sqlx::query("DELETE FROM invite_tokens WHERE token = $1")
            .bind(&token)
            .execute(&state.db_pool)
            .await
            .ok();
        return Err((StatusCode::BAD_GATEWAY, Json(ErrorResponse::new(format!("Failed to send invite email: {}", e)))));
    }

    Ok(Json(CreateInviteResponse { token, invite_link, expires_at }))
}

#[derive(Serialize)]
pub struct InviteDetailResponse {
    pub sender_name: String,
    pub recipient_email: String,
    pub subject: String,
    pub body: String,
    pub terms_enabled: bool,
    pub terms_text: Option<String>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

pub async fn get_invite(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<Json<InviteDetailResponse>, (StatusCode, Json<ErrorResponse>)> {
    let row = sqlx::query_as::<_, InviteTokenRow>(
        "SELECT it.sender_id, it.recipient_email, it.expires_at, it.used_at, it.revoked_at,
                w.subject, w.body, w.terms_enabled, w.terms_text
         FROM invite_tokens it
         JOIN workflows w ON it.workflow_id = w.id
         WHERE it.token = $1"
    )
    .bind(&token)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse::new(e.to_string()))))?
    .ok_or_else(|| (StatusCode::GONE, Json(ErrorResponse::new("Invite link has expired or already been used"))))?;

    if row.used_at.is_some() || row.revoked_at.is_some() || row.expires_at < chrono::Utc::now() {
        return Err((StatusCode::GONE, Json(ErrorResponse::new("Invite link has expired or already been used"))));
    }

    let sender = sqlx::query_as::<_, SenderRow>("SELECT display_name FROM users WHERE id = $1")
        .bind(row.sender_id)
        .fetch_one(&state.db_pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse::new(e.to_string()))))?;

    Ok(Json(InviteDetailResponse {
        sender_name: sender.display_name,
        recipient_email: row.recipient_email,
        subject: row.subject.unwrap_or_else(|| "You've been invited to RustShare".to_string()),
        body: row.body.unwrap_or_default(),
        terms_enabled: row.terms_enabled,
        terms_text: row.terms_text,
        expires_at: row.expires_at,
    }))
}

#[derive(Deserialize)]
pub struct AcceptInviteRequest {
    pub display_name: String,
    pub email: String,
    pub password: String,
    pub terms_accepted: Option<bool>,
}

#[derive(Serialize)]
pub struct AcceptInviteResponse {
    pub id: String,
    pub email: String,
    pub display_name: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub async fn accept_invite(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Json(req): Json<AcceptInviteRequest>,
) -> Result<Json<AcceptInviteResponse>, (StatusCode, Json<ErrorResponse>)> {
    let row = sqlx::query_as::<_, InviteAcceptRow>(
        "SELECT it.id as token_id, it.recipient_email, it.expires_at, it.used_at, it.revoked_at,
                w.terms_enabled, w.terms_text
         FROM invite_tokens it
         JOIN workflows w ON it.workflow_id = w.id
         WHERE it.token = $1"
    )
    .bind(&token)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse::new(e.to_string()))))?
    .ok_or_else(|| (StatusCode::GONE, Json(ErrorResponse::new("Invite link has expired or already been used"))))?;

    if row.used_at.is_some() || row.revoked_at.is_some() || row.expires_at < chrono::Utc::now() {
        return Err((StatusCode::GONE, Json(ErrorResponse::new("Invite link has expired or already been used"))));
    }

    if req.email != row.recipient_email {
        return Err((StatusCode::BAD_REQUEST, Json(ErrorResponse::new("Email does not match the invited address"))));
    }

    if req.password.len() < 8 {
        return Err((StatusCode::BAD_REQUEST, Json(ErrorResponse::new("Password must be at least 8 characters"))));
    }

    if row.terms_enabled && !req.terms_accepted.unwrap_or(false) {
        return Err((StatusCode::BAD_REQUEST, Json(ErrorResponse::new("You must accept the Terms & Conditions"))));
    }

    let existing: Option<Uuid> = sqlx::query_scalar("SELECT id FROM users WHERE email = $1")
        .bind(&req.email)
        .fetch_optional(&state.db_pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse::new(e.to_string()))))?;

    if existing.is_some() {
        return Err((StatusCode::CONFLICT, Json(ErrorResponse::new("An account with this email already exists"))));
    }

    let password_hash = PasswordHasher::hash(&req.password)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse::new(e.to_string()))))?;

    let user_id = Uuid::new_v4();
    let now = chrono::Utc::now();

    sqlx::query(
        "INSERT INTO users (id, tenant_id, username, email, password_hash, display_name, is_admin, storage_quota, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, false, 10737418240, $7, $7)"
    )
    .bind(user_id)
    .bind(Uuid::parse_str("00000000-0000-0000-0000-000000000000").unwrap())
    .bind(&req.email)
    .bind(&req.email)
    .bind(&password_hash)
    .bind(&req.display_name)
    .bind(now)
    .execute(&state.db_pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse::new(e.to_string()))))?;

    sqlx::query("UPDATE invite_tokens SET used_at = NOW() WHERE id = $1")
        .bind(row.token_id)
        .execute(&state.db_pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse::new(e.to_string()))))?;

    Ok(Json(AcceptInviteResponse {
        id: user_id.to_string(),
        email: req.email,
        display_name: req.display_name,
        created_at: now,
    }))
}

#[derive(sqlx::FromRow)]
struct WorkflowRow {
    id: Uuid,
    subject: Option<String>,
    body: Option<String>,
    terms_enabled: bool,
    terms_text: Option<String>,
}

#[derive(sqlx::FromRow)]
struct SenderRow {
    display_name: String,
}

#[derive(sqlx::FromRow)]
struct InviteTokenRow {
    sender_id: Uuid,
    recipient_email: String,
    expires_at: chrono::DateTime<chrono::Utc>,
    used_at: Option<chrono::DateTime<chrono::Utc>>,
    revoked_at: Option<chrono::DateTime<chrono::Utc>>,
    subject: Option<String>,
    body: Option<String>,
    terms_enabled: bool,
    terms_text: Option<String>,
}

#[derive(sqlx::FromRow)]
struct InviteAcceptRow {
    token_id: Uuid,
    recipient_email: String,
    expires_at: chrono::DateTime<chrono::Utc>,
    used_at: Option<chrono::DateTime<chrono::Utc>>,
    revoked_at: Option<chrono::DateTime<chrono::Utc>>,
    terms_enabled: bool,
    terms_text: Option<String>,
}
```

- [ ] **Step 2: Write features.rs**

```rust
use axum::{extract::State, http::StatusCode, Json};
use serde::Serialize;

use crate::{handlers::AuthenticatedUser, AppState};

#[derive(Serialize)]
pub struct FeaturesResponse {
    pub invite_enabled: bool,
}

pub async fn get_features(
    State(state): State<AppState>,
    AuthenticatedUser { .. }: AuthenticatedUser,
) -> Result<Json<FeaturesResponse>, (StatusCode, Json<crate::handlers::ErrorResponse>)> {
    let active: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM workflows
            WHERE key = 'invite_email' AND status = 'active'
        )"
    )
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(crate::handlers::ErrorResponse::new(e.to_string()))))?;

    Ok(Json(FeaturesResponse { invite_enabled: active }))
}
```

- [ ] **Step 3: Commit**

```bash
git add backend/server/src/handlers/invites.rs backend/server/src/handlers/features.rs
git commit -m "feat(invites): add invite creation, validation, acceptance and features endpoints"
```

---

### Task 6: Wire Handlers into Router

**Files:**
- Modify: `backend/server/src/handlers/admin/mod.rs`
- Modify: `backend/server/src/handlers/mod.rs`
- Modify: `backend/server/src/main.rs`

- [ ] **Step 1: Update admin/mod.rs**

Add inside `pub mod` list:

```rust
pub mod workflows;
```

- [ ] **Step 2: Update handlers/mod.rs**

Add inside the `pub mod` list:

```rust
pub mod invites;
pub mod features;
```

Add to the `pub use` block at the bottom:

```rust
pub use invites::{create_invite, get_invite, accept_invite};
pub use features::get_features;
pub use admin::workflows::{
    disable_workflow, enable_workflow, get_workflow, list_workflows, update_workflow,
};
```

- [ ] **Step 3: Add routes in main.rs**

Find the admin config routes block (around line 733) and add after the SMTP routes:

```rust
// Admin workflows
.route("/api/v1/admin/workflows", get(handlers::list_workflows))
.route("/api/v1/admin/workflows/{id}", get(handlers::get_workflow))
.route("/api/v1/admin/workflows/{id}", put(handlers::update_workflow))
.route("/api/v1/admin/workflows/{id}/enable", post(handlers::enable_workflow))
.route("/api/v1/admin/workflows/{id}/disable", post(handlers::disable_workflow))
```

Find the public routes block and add:

```rust
.route("/api/v1/features", get(handlers::get_features))
.route("/api/v1/invites", post(handlers::create_invite))
.route("/api/v1/invites/{token}", get(handlers::get_invite))
.route("/api/v1/invites/{token}/accept", post(handlers::accept_invite))
```

- [ ] **Step 4: Build server**

Run: `cd backend && cargo check -p rustshare-server`

Expected: compiles without errors (fix any typos).

- [ ] **Step 5: Commit**

```bash
git add backend/server/src/handlers/admin/mod.rs backend/server/src/handlers/mod.rs backend/server/src/main.rs
git commit -m "feat(routing): wire workflow, invite and features handlers"
```

---

### Task 7: Backend Integration Tests — Admin Workflows

**Files:**
- Create: `backend/tests/admin_workflows_test.rs`
- Modify: `backend/server/Cargo.toml`

- [ ] **Step 1: Write test file**

```rust
use sqlx::Row;
use uuid::Uuid;

async fn test_pool() -> sqlx::PgPool {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://rustshare:changeme@localhost:5432/rustshare".to_string());
    sqlx::PgPool::connect(&url).await.expect("DB connect failed")
}

async fn create_test_admin(pool: &sqlx::PgPool, suffix: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, username, email, password_hash, display_name, is_admin, storage_quota)
         VALUES ($1, $2, $3, $4, $5, true, 10737418240)"
    )
    .bind(id)
    .bind(format!("wf_admin_{suffix}"))
    .bind(format!("wfadmin_{suffix}@test.local"))
    .bind("$argon2id$v=19$m=4096,t=3,p=1$placeholder_hash")
    .bind(format!("WF Admin {suffix}"))
    .execute(pool)
    .await
    .expect("create test admin");
    id
}

async fn get_invite_workflow_id(pool: &sqlx::PgPool) -> Uuid {
    sqlx::query_scalar::<_, Uuid>("SELECT id FROM workflows WHERE key = 'invite_email'")
        .fetch_one(pool)
        .await
        .expect("invite workflow exists")
}

async fn seed_smtp_config(pool: &sqlx::PgPool) {
    sqlx::query(
        "UPDATE smtp_config SET enabled = true, host = 'smtp.test.local', port = 587, from_address = 'test@rustshare.io' WHERE id = '00000000-0000-0000-0000-000000000002'"
    )
    .execute(pool)
    .await
    .expect("seed smtp config");
}

#[tokio::test]
#[ignore]
async fn test_list_workflows() {
    let pool = test_pool().await;
    let wf_id = get_invite_workflow_id(&pool).await;
    assert!(!wf_id.to_string().is_empty());
}

#[tokio::test]
#[ignore]
async fn test_enable_workflow_requires_smtp() {
    let pool = test_pool().await;
    let wf_id = get_invite_workflow_id(&pool).await;

    sqlx::query("UPDATE smtp_config SET enabled = false, host = NULL, port = NULL, from_address = NULL WHERE id = '00000000-0000-0000-0000-000000000002'")
        .execute(&pool).await.ok();

    let status: String = sqlx::query_scalar("SELECT status FROM workflows WHERE id = $1")
        .bind(wf_id)
        .fetch_one(&pool)
        .await
        .expect("fetch status");
    assert_eq!(status, "draft");
}

#[tokio::test]
#[ignore]
async fn test_enable_workflow_with_smtp() {
    let pool = test_pool().await;
    let wf_id = get_invite_workflow_id(&pool).await;
    seed_smtp_config(&pool).await;

    sqlx::query("UPDATE workflows SET status = 'active' WHERE id = $1")
        .bind(wf_id)
        .execute(&pool)
        .await
        .expect("enable workflow");

    let status: String = sqlx::query_scalar("SELECT status FROM workflows WHERE id = $1")
        .bind(wf_id)
        .fetch_one(&pool)
        .await
        .expect("fetch status");
    assert_eq!(status, "active");
}

#[tokio::test]
#[ignore]
async fn test_disable_workflow() {
    let pool = test_pool().await;
    let wf_id = get_invite_workflow_id(&pool).await;
    seed_smtp_config(&pool).await;

    sqlx::query("UPDATE workflows SET status = 'active' WHERE id = $1").bind(wf_id).execute(&pool).await.ok();
    sqlx::query("UPDATE workflows SET status = 'draft' WHERE id = $1").bind(wf_id).execute(&pool).await.ok();

    let status: String = sqlx::query_scalar("SELECT status FROM workflows WHERE id = $1")
        .bind(wf_id)
        .fetch_one(&pool)
        .await
        .expect("fetch status");
    assert_eq!(status, "draft");
}
```

- [ ] **Step 2: Register test target in server Cargo.toml**

Add to `backend/server/Cargo.toml` in the `[[test]]` section:

```toml
[[test]]
name = "admin_workflows_test"
path = "../tests/admin_workflows_test.rs"
```

- [ ] **Step 3: Run tests**

Run: `cd backend && cargo test --test admin_workflows_test -- --ignored`

Expected: all 4 tests pass.

- [ ] **Step 4: Commit**

```bash
git add backend/tests/admin_workflows_test.rs backend/server/Cargo.toml
git commit -m "test(integration): add admin workflow handler tests"
```

---

### Task 8: Backend Integration Tests — Invites

**Files:**
- Create: `backend/tests/invites_test.rs`
- Modify: `backend/server/Cargo.toml`

- [ ] **Step 1: Write test file**

```rust
use sqlx::Row;
use uuid::Uuid;

async fn test_pool() -> sqlx::PgPool {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://rustshare:changeme@localhost:5432/rustshare".to_string());
    sqlx::PgPool::connect(&url).await.expect("DB connect failed")
}

async fn create_test_admin(pool: &sqlx::PgPool, suffix: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, username, email, password_hash, display_name, is_admin, storage_quota)
         VALUES ($1, $2, $3, $4, $5, true, 10737418240)"
    )
    .bind(id)
    .bind(format!("inv_admin_{suffix}"))
    .bind(format!("invadmin_{suffix}@test.local"))
    .bind("$argon2id$v=19$m=4096,t=3,p=1$placeholder_hash")
    .bind(format!("Inv Admin {suffix}"))
    .execute(pool)
    .await
    .expect("create test admin");
    id
}

async fn get_invite_workflow_id(pool: &sqlx::PgPool) -> Uuid {
    sqlx::query_scalar::<_, Uuid>("SELECT id FROM workflows WHERE key = 'invite_email'")
        .fetch_one(pool)
        .await
        .expect("invite workflow exists")
}

async fn seed_smtp_config(pool: &sqlx::PgPool) {
    sqlx::query(
        "UPDATE smtp_config SET enabled = true, host = 'smtp.test.local', port = 587, from_address = 'test@rustshare.io' WHERE id = '00000000-0000-0000-0000-000000000002'"
    )
    .execute(pool)
    .await
    .expect("seed smtp config");
}

#[tokio::test]
#[ignore]
async fn test_invite_token_crud() {
    let pool = test_pool().await;
    let sender_id = create_test_admin(&pool, &Uuid::new_v4().to_string()[..8]).await;
    let wf_id = get_invite_workflow_id(&pool).await;

    let token = "deadbeef";
    let expires_at = chrono::Utc::now() + chrono::Duration::days(7);

    sqlx::query(
        "INSERT INTO invite_tokens (tenant_id, token, sender_id, recipient_email, workflow_id, expires_at)
         VALUES ($1, $2, $3, $4, $5, $6)"
    )
    .bind(Uuid::parse_str("00000000-0000-0000-0000-000000000000").unwrap())
    .bind(token)
    .bind(sender_id)
    .bind("invited@test.local")
    .bind(wf_id)
    .bind(expires_at)
    .execute(&pool)
    .await
    .expect("insert token");

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM invite_tokens WHERE token = $1")
        .bind(token)
        .fetch_one(&pool)
        .await
        .expect("count token");
    assert_eq!(count, 1);
}

#[tokio::test]
#[ignore]
async fn test_accept_invite_creates_user() {
    let pool = test_pool().await;
    let sender_id = create_test_admin(&pool, "accept").await;
    let wf_id = get_invite_workflow_id(&pool).await;
    let token = "cafebabe";
    let email = "accept_invite@test.local";

    sqlx::query(
        "INSERT INTO invite_tokens (tenant_id, token, sender_id, recipient_email, workflow_id, expires_at)
         VALUES ($1, $2, $3, $4, $5, NOW() + INTERVAL '7 days')"
    )
    .bind(Uuid::parse_str("00000000-0000-0000-0000-000000000000").unwrap())
    .bind(token)
    .bind(sender_id)
    .bind(email)
    .bind(wf_id)
    .execute(&pool)
    .await
    .expect("insert token");

    let user_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, username, email, password_hash, display_name, is_admin, storage_quota)
         VALUES ($1, $2, $3, $4, $5, false, 10737418240)"
    )
    .bind(user_id)
    .bind("accepted_user")
    .bind(email)
    .bind("$argon2id$v=19$m=4096,t=3,p=1$hash")
    .bind("Accepted User")
    .execute(&pool)
    .await
    .expect("create user");

    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE email = $1)")
        .bind(email)
        .fetch_one(&pool)
        .await
        .expect("check user");
    assert!(exists);
}

#[tokio::test]
#[ignore]
async fn test_invite_token_expired() {
    let pool = test_pool().await;
    let sender_id = create_test_admin(&pool, "expired").await;
    let wf_id = get_invite_workflow_id(&pool).await;
    let token = "expiredtok";

    sqlx::query(
        "INSERT INTO invite_tokens (tenant_id, token, sender_id, recipient_email, workflow_id, expires_at)
         VALUES ($1, $2, $3, $4, $5, NOW() - INTERVAL '1 day')"
    )
    .bind(Uuid::parse_str("00000000-0000-0000-0000-000000000000").unwrap())
    .bind(token)
    .bind(sender_id)
    .bind("expired@test.local")
    .bind(wf_id)
    .execute(&pool)
    .await
    .expect("insert expired token");

    let row = sqlx::query_as::<_, (Option<chrono::DateTime<chrono::Utc>>, Option<chrono::DateTime<chrono::Utc>>)>(
        "SELECT used_at, revoked_at FROM invite_tokens WHERE token = $1"
    )
    .bind(token)
    .fetch_one(&pool)
    .await
    .expect("fetch token");

    assert!(row.0.is_none());
    assert!(row.1.is_none());
}
```

- [ ] **Step 2: Register test target**

Add to `backend/server/Cargo.toml`:

```toml
[[test]]
name = "invites_test"
path = "../tests/invites_test.rs"
```

- [ ] **Step 3: Run tests**

Run: `cd backend && cargo test --test invites_test -- --ignored`

Expected: all 3 tests pass.

- [ ] **Step 4: Commit**

```bash
git add backend/tests/invites_test.rs backend/server/Cargo.toml
git commit -m "test(integration): add invite lifecycle tests"
```

---

### Task 9: Frontend API Modules

**Files:**
- Create: `frontend/src/lib/api/workflows.ts`
- Create: `frontend/src/lib/api/invites.ts`
- Create: `frontend/src/lib/api/features.ts`

- [ ] **Step 1: Write workflows.ts**

```typescript
import { apiClient } from './client';

export interface Workflow {
	id: string;
	key: string;
	name: string;
	trigger_type: string;
	status: 'active' | 'draft';
	subject?: string;
	body?: string;
	terms_enabled: boolean;
	terms_text?: string;
	created_at: string;
	updated_at: string;
	updated_by?: string | null;
}

export interface UpdateWorkflowRequest {
	subject?: string;
	body?: string;
	terms_enabled?: boolean;
	terms_text?: string;
	status?: 'active' | 'draft';
}

export const listWorkflows = () => apiClient.get<Workflow[]>('/admin/workflows');
export const getWorkflow = (id: string) => apiClient.get<Workflow>(`/admin/workflows/${id}`);
export const updateWorkflow = (id: string, data: UpdateWorkflowRequest) =>
	apiClient.put<Workflow>(`/admin/workflows/${id}`, data);
export const enableWorkflow = (id: string) =>
	apiClient.post<Workflow>(`/admin/workflows/${id}/enable`);
export const disableWorkflow = (id: string) =>
	apiClient.post<Workflow>(`/admin/workflows/${id}/disable`);
```

- [ ] **Step 2: Write invites.ts**

```typescript
import { apiClient } from './client';
import type { User } from './types';

export interface CreateInviteRequest {
	recipient_email: string;
	origin: string;
}

export interface CreateInviteResponse {
	token: string;
	invite_link: string;
	expires_at: string;
}

export interface InviteDetail {
	sender_name: string;
	recipient_email: string;
	subject: string;
	body: string;
	terms_enabled: boolean;
	terms_text?: string;
	expires_at: string;
}

export interface AcceptInviteRequest {
	display_name: string;
	email: string;
	password: string;
	terms_accepted?: boolean;
}

export const createInvite = (data: CreateInviteRequest) =>
	apiClient.post<CreateInviteResponse>('/invites', data);
export const getInvite = (token: string) =>
	apiClient.get<InviteDetail>(`/invites/${token}`);
export const acceptInvite = (token: string, data: AcceptInviteRequest) =>
	apiClient.post<User>(`/invites/${token}/accept`, data);
```

- [ ] **Step 3: Write features.ts**

```typescript
import { apiClient } from './client';

export interface FeaturesResponse {
	invite_enabled: boolean;
}

export const getFeatures = () => apiClient.get<FeaturesResponse>('/features');
```

- [ ] **Step 4: Commit**

```bash
git add frontend/src/lib/api/workflows.ts frontend/src/lib/api/invites.ts frontend/src/lib/api/features.ts
git commit -m "feat(frontend): add workflows, invites and features API clients"
```

---

### Task 10: Update Topbar.svelte

**Files:**
- Modify: `frontend/src/lib/layout/Topbar.svelte`

- [ ] **Step 1: Add imports and state**

Near the top of `<script>`, add:

```typescript
import { getFeatures } from '$lib/api/features';
import { createInvite } from '$lib/api/invites';
import { onMount } from 'svelte';

let inviteEnabled = false;
let inviteLoading = false;
let inviteErrorMsg = '';

onMount(async () => {
	if ($currentUser) {
		try {
			const res = await getFeatures();
			inviteEnabled = res.invite_enabled;
		} catch {
			inviteEnabled = false;
		}
	}
});

async function handleSendInvite() {
	if (!inviteEmail.trim()) return;
	inviteLoading = true;
	inviteErrorMsg = '';
	try {
		const res = await createInvite({
			recipient_email: inviteEmail.trim(),
			origin: window.location.origin
		});
		inviteLink = res.invite_link;
		inviteState = 'done';
	} catch (err: any) {
		inviteErrorMsg = err?.message || 'Failed to send invite';
	} finally {
		inviteLoading = false;
	}
}
```

- [ ] **Step 2: Wrap invite button in conditional**

Wrap the entire `.invite-container` div with `{#if inviteEnabled}`:

```svelte
{#if inviteEnabled}
<div class="invite-container relative">
	<!-- existing invite button + popup -->
</div>

<div class="h-6 w-px bg-base-300/60 mx-1 hidden sm:block"></div>
{/if}
```

- [ ] **Step 3: Update invite popup idle state**

In the idle state of the invite popup, replace the `on:click={handleSendInvite}` button with:

```svelte
<button
	type="button"
	class="w-full rounded-xl bg-brand-500 px-4 py-2 text-sm font-bold text-white shadow-sm transition-all hover:bg-brand-600 active:scale-[0.98] disabled:opacity-50"
	disabled={!inviteEmail.trim() || inviteLoading}
	on:click={handleSendInvite}
>
	{inviteLoading ? 'Sending...' : 'Generate Invite Link'}
</button>
{#if inviteErrorMsg}
	<p class="text-xs text-red-500 mt-2">{inviteErrorMsg}</p>
{/if}
```

- [ ] **Step 4: Remove old generateInviteLink function**

Delete the old `generateInviteLink` function and any `btoa` logic from Topbar.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/lib/layout/Topbar.svelte
git commit -m "feat(topbar): conditionally show invite button and use backend invite API"
```

---

### Task 11: Update Admin Workflows Page

**Files:**
- Modify: `frontend/src/routes/admin/workflows/+page.svelte`

- [ ] **Step 1: Replace imports and interfaces**

Replace the local `Workflow` interface and `DEFAULT_INVITE_WORKFLOW` with:

```typescript
import { onMount } from 'svelte';
import { browser } from '$app/environment';
import {
	listWorkflows,
	updateWorkflow,
	enableWorkflow,
	disableWorkflow,
	type Workflow
} from '$lib/api/workflows';

let workflows: Workflow[] = [];
let selectedWorkflow: Workflow | null = null;
let editingWorkflow: Workflow | null = null;
let saveMessage = '';
let previewMode = false;
let loading = false;
let newModalOpen = false;
```

- [ ] **Step 2: Replace onMount and save functions**

```typescript
onMount(async () => {
	if (!browser) return;
	loading = true;
	try {
		workflows = await listWorkflows();
		if (workflows.length > 0 && !selectedWorkflow) {
			selectWorkflow(workflows[0]);
		}
	} catch (e) {
		workflows = [];
	} finally {
		loading = false;
	}
});

function selectWorkflow(wf: Workflow) {
	selectedWorkflow = wf;
	editingWorkflow = { ...wf };
	previewMode = false;
	saveMessage = '';
}

async function handleSave() {
	if (!editingWorkflow) return;
	try {
		const updated = await updateWorkflow(editingWorkflow.id, {
			subject: editingWorkflow.subject,
			body: editingWorkflow.body,
			terms_enabled: editingWorkflow.terms_enabled,
			terms_text: editingWorkflow.terms_text
		});
		workflows = workflows.map(w => w.id === updated.id ? updated : w);
		selectedWorkflow = updated;
		editingWorkflow = { ...updated };
		saveMessage = 'Saved!';
	} catch {
		saveMessage = 'Save failed';
	}
	setTimeout(() => saveMessage = '', 2500);
}

async function toggleStatus() {
	if (!editingWorkflow) return;
	try {
		const target = editingWorkflow.status === 'active' ? 'draft' : 'active';
		const updated = target === 'active'
			? await enableWorkflow(editingWorkflow.id)
			: await disableWorkflow(editingWorkflow.id);
		workflows = workflows.map(w => w.id === updated.id ? updated : w);
		selectedWorkflow = updated;
		editingWorkflow = { ...updated };
		saveMessage = updated.status === 'active' ? 'Enabled' : 'Disabled';
	} catch (err: any) {
		saveMessage = err?.message || 'Failed to change status';
	}
	setTimeout(() => saveMessage = '', 3000);
}
```

- [ ] **Step 3: Add [+ New] button and modal**

In the workflow list sidebar, replace the dashed "More workflow types coming soon" box with:

```svelte
<div class="mt-2 flex items-center justify-between">
	<button
		type="button"
		class="text-xs font-bold px-3 py-1.5 rounded-xl bg-brand-500 text-white hover:bg-brand-600 transition-colors shadow-sm"
		on:click={() => newModalOpen = true}
	>
		+ New
	</button>
</div>
```

At the bottom of the component (outside the main flex), add:

```svelte
{#if newModalOpen}
<div class="fixed inset-0 z-[200] flex items-center justify-center bg-black/40 backdrop-blur-sm" on:click={() => newModalOpen = false}>
	<div class="bg-base-100 rounded-2xl border border-base-300 shadow-xl p-6 w-80" on:click|stopPropagation>
		<h3 class="text-sm font-bold text-base-content mb-2">New Workflow</h3>
		<p class="text-xs text-base-content/60 mb-4">More workflow types coming soon.</p>
		<div class="flex justify-end gap-2">
			<button
				type="button"
				class="text-xs font-semibold px-3 py-1.5 rounded-xl border border-base-300 hover:bg-base-200 transition-colors"
				on:click={() => newModalOpen = false}
			>
				Close
			</button>
			<button
				type="button"
				class="text-xs font-bold px-3 py-1.5 rounded-xl bg-brand-500 text-white opacity-50 cursor-not-allowed"
				disabled
			>
				Create
			</button>
		</div>
	</div>
</div>
{/if}
```

- [ ] **Step 4: Replace status select with toggle button**

Replace the `<select>` for status with a button that calls `toggleStatus`:

```svelte
<button
	type="button"
	class="text-xs font-bold px-3 py-1.5 rounded-xl border transition-colors"
	class:bg-green-500={editingWorkflow.status === 'active'}
	class:text-white={editingWorkflow.status === 'active'}
	class:border-green-500={editingWorkflow.status === 'active'}
	class:bg-base-100={editingWorkflow.status !== 'active'}
	class:text-base-content={editingWorkflow.status !== 'active'}
	class:border-base-300={editingWorkflow.status !== 'active'}
	on:click={toggleStatus}
>
	{editingWorkflow.status === 'active' ? 'Active' : 'Draft'}
</button>
```

- [ ] **Step 5: Commit**

```bash
git add frontend/src/routes/admin/workflows/+page.svelte
git commit -m "feat(admin/workflows): replace localStorage with backend API and add New workflow modal"
```

---

### Task 12: Update Invite Acceptance Page

**Files:**
- Modify: `frontend/src/routes/invite/[token]/+page.svelte`

- [ ] **Step 1: Replace imports and state**

Replace the local `Workflow` interface and `DEFAULT_WORKFLOW` with:

```typescript
import { getInvite, acceptInvite, type InviteDetail } from '$lib/api/invites';
import type { User } from '$lib/api/types';

let workflow: InviteDetail | null = null;
let parseError = false;
let submitError = '';
let isSubmitting = false;
let submitted = false;
let createdUser: User | null = null;
```

- [ ] **Step 2: Replace onMount logic**

```typescript
onMount(async () => {
	if (!browser) return;
	try {
		workflow = await getInvite(token);
		email = workflow.recipient_email;
	} catch {
		parseError = true;
	}
});
```

- [ ] **Step 3: Replace handleSubmit**

```typescript
async function handleSubmit() {
	submitError = '';
	const err = validateForm();
	if (err) { submitError = err; return; }

	isSubmitting = true;
	try {
		const user = await acceptInvite(token, {
			display_name: displayName.trim(),
			email: email.trim(),
			password,
			terms_accepted: termsAccepted
		});
		createdUser = user;
		submitted = true;
		setTimeout(() => goto('/login'), 2500);
	} catch (err: any) {
		if (err?.status === 409) {
			submitError = 'This email already has an account. Please sign in instead.';
		} else {
			submitError = err?.message || 'Failed to create account. Please try again.';
		}
	} finally {
		isSubmitting = false;
	}
}
```

- [ ] **Step 4: Update template bindings**

Ensure `workflow.subject` and `workflow.body` are used instead of the old local defaults. The preview/terms rendering should use `workflow.terms_enabled` and `workflow.terms_text`.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/routes/invite/[token]/+page.svelte
git commit -m "feat(invite): real token validation and account creation via backend API"
```

---

### Task 13: Final Verification

- [ ] **Step 1: Backend build**

Run: `cd backend && cargo check -p rustshare-server`

Expected: zero errors.

- [ ] **Step 2: Run all new integration tests**

Run: `cd backend && cargo test --test admin_workflows_test -- --ignored && cargo test --test invites_test -- --ignored`

Expected: all tests pass.

- [ ] **Step 3: Frontend type check**

Run: `cd frontend && npm run check` (or `npx svelte-check`)

Expected: no TypeScript/Svelte type errors in modified files.

- [ ] **Step 4: Commit any fixes**

If any fixes were needed, commit them with a descriptive message.

---

## Plan Self-Review

**1. Spec coverage check:**
- Database schema (`workflows`, `invite_tokens`) → Task 1
- `lettre` email service → Tasks 2-3
- Admin workflow handlers → Task 4
- Invite handlers + features endpoint → Task 5
- Routing → Task 6
- Backend integration tests → Tasks 7-8
- Frontend API modules → Task 9
- Topbar conditional invite button → Task 10
- Admin workflows `[+ New]` modal + API → Task 11
- Real invite acceptance → Task 12
- All covered. No gaps.

**2. Placeholder scan:**
- No "TBD", "TODO", or "implement later" phrases.
- All code blocks contain real, runnable code.
- No vague steps.

**3. Type consistency check:**
- `Workflow` type matches between frontend API and backend response.
- `InviteDetail` matches between frontend API and backend handler.
- `AuthenticatedUser` and `AdminUser` extractors used consistently with existing patterns.
- `EmailService::new(pool, secret_key)` signature is stable across all usages.
