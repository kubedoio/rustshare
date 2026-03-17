# RustShare Phase 1: Foundation Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the foundational backend infrastructure including Cargo workspace, domain models, PostgreSQL event store, authentication system, and basic HTTP API for file operations.

**Architecture:** Event-sourced modular monolith with Cargo workspace structure. Core business logic separated from I/O. PostgreSQL stores events and projections. Axum handles HTTP/WebSocket. S3 client abstracts RustFS access.

**Tech Stack:**
- Rust 1.75+ with Cargo workspace
- Axum 0.7 (web framework)
- SQLx 0.7 (database with compile-time query checking)
- PostgreSQL 16
- Tokio (async runtime)
- AWS SDK for S3 (RustFS client)
- Argon2 (password hashing)
- JWT (authentication tokens)

**Phase 1 Scope:**
- ✅ Cargo workspace setup with crate structure
- ✅ Domain models (User, File, Folder, Share, Events)
- ✅ PostgreSQL schema and event store
- ✅ Authentication (admin bootstrapping, JWT, login/logout)
- ✅ Basic HTTP API (health check, auth endpoints)
- ✅ S3 client abstraction for RustFS
- ✅ Docker Compose development environment

**Out of Scope for Phase 1:**
- File upload/download HTTP endpoints (Phase 2)
- Real-time WebSocket sync
- Conflict detection
- File versioning
- WebDAV and S3-compatible protocols
- Frontend UI
- Thumbnails and previews
- Sharing functionality

---

## File Structure Overview

This plan creates the following structure:

```
rustshare/
├── backend/
│   ├── Cargo.toml                    # Workspace definition
│   ├── .env.example                  # Environment variables template
│   ├── crates/
│   │   ├── core/
│   │   │   ├── Cargo.toml
│   │   │   └── src/
│   │   │       ├── lib.rs
│   │   │       ├── domain/
│   │   │       │   ├── mod.rs
│   │   │       │   ├── user.rs       # User domain type
│   │   │       │   ├── file.rs       # File domain type
│   │   │       │   ├── folder.rs     # Folder domain type
│   │   │       │   └── share.rs      # Share domain type
│   │   │       ├── events/
│   │   │       │   ├── mod.rs
│   │   │       │   └── types.rs      # Event type definitions
│   │   │       └── services/
│   │   │           ├── mod.rs
│   │   │           ├── user_service.rs
│   │   │           └── file_service.rs
│   │   ├── storage/
│   │   │   ├── Cargo.toml
│   │   │   └── src/
│   │   │       ├── lib.rs
│   │   │       ├── event_store.rs    # Event persistence
│   │   │       ├── metadata.rs       # Query interface
│   │   │       └── object_store.rs   # S3/RustFS client
│   │   ├── auth/
│   │   │   ├── Cargo.toml
│   │   │   └── src/
│   │   │       ├── lib.rs
│   │   │       ├── password.rs       # Argon2 hashing
│   │   │       ├── jwt.rs            # Token generation/validation
│   │   │       └── session.rs        # Session management
│   │   └── protocols/
│   │       ├── Cargo.toml
│   │       └── src/
│   │           ├── lib.rs
│   │           └── http_api/
│   │               ├── mod.rs
│   │               ├── routes.rs     # Route definitions
│   │               ├── handlers.rs   # Request handlers
│   │               └── middleware.rs # Auth middleware
│   ├── server/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       └── main.rs               # Application entry point
│   └── migrations/
│       ├── 20260317000001_create_events_table.sql
│       ├── 20260317000002_create_users_table.sql
│       ├── 20260317000003_create_folders_table.sql
│       └── 20260317000004_create_files_table.sql
├── docker/
│   └── backend.Dockerfile
├── docker-compose.yml
└── docker-compose.dev.yml
```

---

## Task 1: Workspace and Project Setup

**Files:**
- Create: `backend/Cargo.toml`
- Create: `backend/.env.example`
- Create: `backend/.gitignore`
- Create: `.gitignore`

- [ ] **Step 1: Create root .gitignore**

```gitignore
# Rust
target/

# Environment
.env
.env.local

# IDE
.vscode/
.idea/
*.swp
*.swo

# OS
.DS_Store
Thumbs.db

# Docker
docker-compose.override.yml

# Logs
*.log

# Note: Cargo.lock IS committed for applications (not libraries)
# to ensure reproducible builds
```

- [ ] **Step 2: Create workspace Cargo.toml**

```toml
[workspace]
members = [
    "crates/core",
    "crates/storage",
    "crates/auth",
    "server",
]
resolver = "2"

# Note: crates/protocols will be added in Phase 4

[workspace.package]
version = "0.1.0"
edition = "2021"
rust-version = "1.75"
authors = ["RustShare Contributors"]
license = "Apache-2.0"

[workspace.dependencies]
# Async runtime
tokio = { version = "1.37", features = ["full"] }

# Web framework
axum = { version = "0.7", features = ["macros"] }
tower = "0.4"
tower-http = { version = "0.5", features = ["trace", "cors"] }

# Database
sqlx = { version = "0.7", features = ["runtime-tokio-rustls", "postgres", "uuid", "chrono", "json"] }

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# Crypto
argon2 = "0.5"
jsonwebtoken = "9"
rand = "0.8"

# S3 client
aws-sdk-s3 = "1.17"
aws-config = "1.1"

# Utilities
uuid = { version = "1.7", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
anyhow = "1.0"
thiserror = "1.0"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

# Testing
mockall = "0.12"
```

- [ ] **Step 3: Create .env.example**

```env
# Database
DATABASE_URL=postgres://rustshare:changeme@localhost:5432/rustshare

# RustFS (S3-compatible storage)
RUSTFS_ENDPOINT=http://localhost:9000
RUSTFS_ACCESS_KEY=rustfsadmin
RUSTFS_SECRET_KEY=rustfsadmin
RUSTFS_BUCKET=rustshare-data
RUSTFS_REGION=us-east-1

# Authentication
JWT_SECRET=change-me-in-production-use-strong-random-key

# Admin user (created on first boot if no users exist)
RUSTSHARE_ADMIN_EMAIL=admin@localhost
RUSTSHARE_ADMIN_PASSWORD=admin123

# Server
SERVER_HOST=0.0.0.0
SERVER_PORT=8080

# Logging
RUST_LOG=info,rustshare=debug
```

- [ ] **Step 4: Create backend .gitignore**

```gitignore
target/
.env
```

- [ ] **Step 5: Verify workspace structure**

Run: `cd backend && cargo check`
Expected: ERROR (no crates yet, but workspace should be valid)

- [ ] **Step 6: Commit**

```bash
git add .gitignore backend/Cargo.toml backend/.env.example backend/.gitignore
git commit -m "feat: initialize Rust workspace structure"
```

---

## Task 2: Core Domain Models

**Files:**
- Create: `backend/crates/core/Cargo.toml`
- Create: `backend/crates/core/src/lib.rs`
- Create: `backend/crates/core/src/domain/mod.rs`
- Create: `backend/crates/core/src/domain/user.rs`
- Create: `backend/crates/core/src/domain/file.rs`
- Create: `backend/crates/core/src/domain/folder.rs`
- Create: `backend/crates/core/src/domain/share.rs`

- [ ] **Step 1: Create core crate Cargo.toml**

```toml
[package]
name = "rustshare-core"
version.workspace = true
edition.workspace = true

[dependencies]
uuid.workspace = true
chrono.workspace = true
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
```

- [ ] **Step 2: Create lib.rs**

```rust
//! RustShare core domain models and business logic.
//!
//! This crate contains pure business logic with no I/O dependencies.

pub mod domain;
pub mod events;
// Note: services module will be added in Phase 2

pub use domain::*;
```

- [ ] **Step 3: Create domain/mod.rs**

```rust
//! Core domain types for RustShare.

pub mod user;
pub mod file;
pub mod folder;
pub mod share;

pub use user::User;
pub use file::File;
pub use folder::Folder;
pub use share::Share;
```

- [ ] **Step 4: Write user.rs**

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for a user
pub type UserId = Uuid;

/// User account in the system
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct User {
    pub id: UserId,
    pub email: String,
    pub password_hash: String,
    pub display_name: String,
    pub is_admin: bool,
    pub storage_quota: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl User {
    /// Create a new user with default values
    pub fn new(
        email: String,
        password_hash: String,
        display_name: String,
        is_admin: bool,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            email,
            password_hash,
            display_name,
            is_admin,
            storage_quota: 10 * 1024 * 1024 * 1024, // 10 GB default
            created_at: now,
            updated_at: now,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_user_has_default_quota() {
        let user = User::new(
            "test@example.com".to_string(),
            "hash".to_string(),
            "Test User".to_string(),
            false,
        );

        assert_eq!(user.email, "test@example.com");
        assert_eq!(user.storage_quota, 10 * 1024 * 1024 * 1024);
        assert!(!user.is_admin);
    }

    #[test]
    fn test_admin_user_creation() {
        let user = User::new(
            "admin@example.com".to_string(),
            "hash".to_string(),
            "Admin".to_string(),
            true,
        );

        assert!(user.is_admin);
    }
}
```

- [ ] **Step 5: Run tests**

Run: `cd backend && cargo test -p rustshare-core`
Expected: 2 tests passed

- [ ] **Step 6: Write folder.rs**

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::user::UserId;

/// Unique identifier for a folder
pub type FolderId = Uuid;

/// Folder in the file system
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Folder {
    pub id: FolderId,
    pub name: String,
    pub path: String,
    pub owner_id: UserId,
    pub parent_folder_id: Option<FolderId>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Folder {
    /// Create a new root folder
    pub fn new_root(name: String, owner_id: UserId) -> Self {
        let now = Utc::now();
        let path = format!("/{}", name);
        Self {
            id: Uuid::new_v4(),
            name,
            path,
            owner_id,
            parent_folder_id: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Create a new child folder
    pub fn new_child(
        name: String,
        owner_id: UserId,
        parent_folder_id: FolderId,
        parent_path: &str,
    ) -> Self {
        let now = Utc::now();
        let path = format!("{}/{}", parent_path, name);
        Self {
            id: Uuid::new_v4(),
            name,
            path,
            owner_id,
            parent_folder_id: Some(parent_folder_id),
            created_at: now,
            updated_at: now,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_root_folder_has_no_parent() {
        let owner_id = Uuid::new_v4();
        let folder = Folder::new_root("Documents".to_string(), owner_id);

        assert_eq!(folder.name, "Documents");
        assert_eq!(folder.path, "/Documents");
        assert_eq!(folder.parent_folder_id, None);
        assert_eq!(folder.owner_id, owner_id);
    }

    #[test]
    fn test_child_folder_has_correct_path() {
        let owner_id = Uuid::new_v4();
        let parent_id = Uuid::new_v4();
        let folder = Folder::new_child(
            "Projects".to_string(),
            owner_id,
            parent_id,
            "/Documents",
        );

        assert_eq!(folder.name, "Projects");
        assert_eq!(folder.path, "/Documents/Projects");
        assert_eq!(folder.parent_folder_id, Some(parent_id));
    }
}
```

- [ ] **Step 7: Run tests**

Run: `cd backend && cargo test -p rustshare-core`
Expected: 4 tests passed

- [ ] **Step 8: Write file.rs**

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{folder::FolderId, user::UserId};

/// Unique identifier for a file
pub type FileId = Uuid;

/// File in the system
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct File {
    pub id: FileId,
    pub name: String,
    pub path: String,
    pub size: i64,
    pub mime_type: String,
    pub content_hash: String,
    pub storage_key: String,
    pub owner_id: UserId,
    pub parent_folder_id: Option<FolderId>,
    pub current_version: i32,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
}

impl File {
    /// Create a new file
    pub fn new(
        name: String,
        parent_path: &str,
        size: i64,
        mime_type: String,
        content_hash: String,
        owner_id: UserId,
        parent_folder_id: Option<FolderId>,
    ) -> Self {
        let now = Utc::now();
        let path = if parent_path.ends_with('/') {
            format!("{}{}", parent_path, name)
        } else {
            format!("{}/{}", parent_path, name)
        };

        // Storage key is content-addressed: blobs/{hash}
        let storage_key = format!("blobs/{}", content_hash);

        Self {
            id: Uuid::new_v4(),
            name,
            path,
            size,
            mime_type,
            content_hash,
            storage_key,
            owner_id,
            parent_folder_id,
            current_version: 1,
            created_at: now,
            modified_at: now,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_file_has_version_one() {
        let owner_id = Uuid::new_v4();
        let file = File::new(
            "document.pdf".to_string(),
            "/Documents",
            1024,
            "application/pdf".to_string(),
            "abc123".to_string(),
            owner_id,
            None,
        );

        assert_eq!(file.name, "document.pdf");
        assert_eq!(file.path, "/Documents/document.pdf");
        assert_eq!(file.current_version, 1);
        assert_eq!(file.storage_key, "blobs/abc123");
    }

    #[test]
    fn test_file_path_construction() {
        let owner_id = Uuid::new_v4();
        let file = File::new(
            "report.txt".to_string(),
            "/Projects/Q1",
            512,
            "text/plain".to_string(),
            "def456".to_string(),
            owner_id,
            Some(Uuid::new_v4()),
        );

        assert_eq!(file.path, "/Projects/Q1/report.txt");
    }
}
```

- [ ] **Step 9: Write share.rs**

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{file::FileId, user::UserId};

/// Unique identifier for a share
pub type ShareId = Uuid;

/// Share permissions
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum SharePermissions {
    Read,
    ReadWrite,
}

/// Public share link for a file
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Share {
    pub id: ShareId,
    pub file_id: FileId,
    pub share_token: String,
    pub created_by: UserId,
    pub permissions: SharePermissions,
    pub password_hash: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub access_count: i32,
}

impl Share {
    /// Create a new share
    pub fn new(
        file_id: FileId,
        share_token: String,
        created_by: UserId,
        permissions: SharePermissions,
        password_hash: Option<String>,
        expires_at: Option<DateTime<Utc>>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            file_id,
            share_token,
            created_by,
            permissions,
            password_hash,
            expires_at,
            created_at: Utc::now(),
            access_count: 0,
        }
    }

    /// Check if share is expired
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            Utc::now() > expires_at
        } else {
            false
        }
    }

    /// Check if share requires password
    pub fn is_password_protected(&self) -> bool {
        self.password_hash.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_share_without_expiry_not_expired() {
        let share = Share::new(
            Uuid::new_v4(),
            "token123".to_string(),
            Uuid::new_v4(),
            SharePermissions::Read,
            None,
            None,
        );

        assert!(!share.is_expired());
        assert!(!share.is_password_protected());
    }

    #[test]
    fn test_share_with_password_is_protected() {
        let share = Share::new(
            Uuid::new_v4(),
            "token456".to_string(),
            Uuid::new_v4(),
            SharePermissions::Read,
            Some("hash".to_string()),
            None,
        );

        assert!(share.is_password_protected());
    }
}
```

- [ ] **Step 10: Run all core tests**

Run: `cd backend && cargo test -p rustshare-core`
Expected: 8 tests passed

- [ ] **Step 11: Commit**

```bash
git add backend/crates/core/
git commit -m "feat(core): add domain models for User, File, Folder, Share"
```

---

## Task 3: Event Definitions

**Files:**
- Create: `backend/crates/core/src/events/mod.rs`
- Create: `backend/crates/core/src/events/types.rs`

- [ ] **Step 1: Create events/mod.rs**

```rust
//! Event definitions for the event-sourced architecture.

pub mod types;

pub use types::*;
```

- [ ] **Step 2: Write event types**

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use uuid::Uuid;

use crate::domain::*;

/// Unique identifier for an event
pub type EventId = Uuid;

/// Event aggregate type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AggregateType {
    User,
    File,
    Folder,
    Share,
}

/// Event types in the system
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase", tag = "type")]
pub enum EventType {
    // User events
    UserCreated,
    UserUpdated,
    UserDeleted,

    // File events
    FileUploaded,
    FileModified,
    FileRenamed,
    FileMoved,
    FileDeleted,
    FileRestored,

    // Folder events
    FolderCreated,
    FolderRenamed,
    FolderMoved,
    FolderDeleted,

    // Share events
    ShareCreated,
    ShareRevoked,
    ShareUpdated,

    // Sync events
    ConflictDetected,
    ConflictResolved,
}

/// Event stored in the event store
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: EventId,
    pub event_type: EventType,
    pub aggregate_id: Uuid,
    pub aggregate_type: AggregateType,
    pub payload: JsonValue,
    pub user_id: UserId,
    pub timestamp: DateTime<Utc>,
    pub version: i32,
}

impl Event {
    /// Create a new event
    pub fn new(
        event_type: EventType,
        aggregate_id: Uuid,
        aggregate_type: AggregateType,
        payload: JsonValue,
        user_id: UserId,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            event_type,
            aggregate_id,
            aggregate_type,
            payload,
            user_id,
            timestamp: Utc::now(),
            version: 1,
        }
    }
}

/// File uploaded event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileUploadedPayload {
    pub file_id: FileId,
    pub name: String,
    pub path: String,
    pub size: i64,
    pub content_hash: String,
    pub storage_key: String,
    pub mime_type: String,
    pub owner_id: UserId,
}

/// Folder created event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderCreatedPayload {
    pub folder_id: FolderId,
    pub name: String,
    pub path: String,
    pub parent_folder_id: Option<FolderId>,
    pub owner_id: UserId,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_creation() {
        let user_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();
        let payload = serde_json::json!({
            "file_id": file_id.to_string(),
            "name": "test.txt"
        });

        let event = Event::new(
            EventType::FileUploaded,
            file_id,
            AggregateType::File,
            payload,
            user_id,
        );

        assert_eq!(event.event_type, EventType::FileUploaded);
        assert_eq!(event.aggregate_id, file_id);
        assert_eq!(event.version, 1);
    }

    #[test]
    fn test_event_type_serialization() {
        let event_type = EventType::FileUploaded;
        let json = serde_json::to_string(&event_type).unwrap();
        assert_eq!(json, r#"{"type":"FileUploaded"}"#);
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cd backend && cargo test -p rustshare-core`
Expected: 10 tests passed

- [ ] **Step 4: Commit**

```bash
git add backend/crates/core/src/events/
git commit -m "feat(core): add event type definitions for event sourcing"
```

---

## Task 4: Database Migrations

**Files:**
- Create: `backend/migrations/20260317000001_create_events_table.sql`
- Create: `backend/migrations/20260317000002_create_users_table.sql`
- Create: `backend/migrations/20260317000003_create_folders_table.sql`
- Create: `backend/migrations/20260317000004_create_files_table.sql`
- Create: `backend/migrations/20260317000005_create_shares_table.sql`

- [ ] **Step 1: Create events table migration**

```sql
-- Event store table (append-only)
CREATE TABLE events (
    id BIGSERIAL PRIMARY KEY,
    event_id UUID UNIQUE NOT NULL,
    event_type VARCHAR(100) NOT NULL,
    aggregate_id UUID NOT NULL,
    aggregate_type VARCHAR(50) NOT NULL,
    payload JSONB NOT NULL,
    user_id UUID NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    version INTEGER NOT NULL
);

-- Indexes for efficient querying
CREATE INDEX idx_events_aggregate ON events(aggregate_id, aggregate_type);
CREATE INDEX idx_events_timestamp ON events(timestamp);
CREATE INDEX idx_events_type ON events(event_type);
CREATE INDEX idx_events_user ON events(user_id);
```

- [ ] **Step 2: Create users table migration**

```sql
-- Users projection table
CREATE TABLE users (
    id UUID PRIMARY KEY,
    email VARCHAR(255) UNIQUE NOT NULL,
    password_hash VARCHAR(255) NOT NULL,
    display_name VARCHAR(255) NOT NULL,
    is_admin BOOLEAN NOT NULL DEFAULT FALSE,
    storage_quota BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index for email lookups (login)
CREATE INDEX idx_users_email ON users(email);
```

- [ ] **Step 3: Create folders table migration**

```sql
-- Folders projection table
CREATE TABLE folders (
    id UUID PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    path TEXT NOT NULL,
    owner_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    parent_folder_id UUID REFERENCES folders(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index for path lookups
CREATE INDEX idx_folders_path ON folders(path);
CREATE INDEX idx_folders_owner ON folders(owner_id);
CREATE INDEX idx_folders_parent ON folders(parent_folder_id);

-- Unique constraint: name must be unique within parent folder for same owner
CREATE UNIQUE INDEX idx_folders_unique_name ON folders(owner_id, parent_folder_id, name);
```

- [ ] **Step 4: Create files table migration**

```sql
-- Files projection table
CREATE TABLE files (
    id UUID PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    path TEXT NOT NULL,
    size BIGINT NOT NULL,
    mime_type VARCHAR(255) NOT NULL,
    content_hash VARCHAR(64) NOT NULL,
    storage_key VARCHAR(255) NOT NULL,
    owner_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    parent_folder_id UUID REFERENCES folders(id) ON DELETE CASCADE,
    current_version INTEGER NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    modified_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes
CREATE INDEX idx_files_path ON files(path);
CREATE INDEX idx_files_owner ON files(owner_id);
CREATE INDEX idx_files_parent ON files(parent_folder_id);
CREATE INDEX idx_files_hash ON files(content_hash);

-- File versions table
CREATE TABLE file_versions (
    id UUID PRIMARY KEY,
    file_id UUID NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    version_number INTEGER NOT NULL,
    content_hash VARCHAR(64) NOT NULL,
    storage_key VARCHAR(255) NOT NULL,
    size BIGINT NOT NULL,
    created_by UUID NOT NULL REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    change_description TEXT
);

-- Indexes
CREATE INDEX idx_file_versions_file ON file_versions(file_id, version_number DESC);
CREATE UNIQUE INDEX idx_file_versions_unique ON file_versions(file_id, version_number);
```

- [ ] **Step 5: Create shares table migration**

```sql
-- Shares projection table
CREATE TABLE shares (
    id UUID PRIMARY KEY,
    file_id UUID NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    share_token VARCHAR(255) UNIQUE NOT NULL,
    created_by UUID NOT NULL REFERENCES users(id),
    permissions VARCHAR(20) NOT NULL,
    password_hash VARCHAR(255),
    expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    access_count INTEGER NOT NULL DEFAULT 0
);

-- Index for token lookups (public access)
CREATE INDEX idx_shares_token ON shares(share_token);
CREATE INDEX idx_shares_file ON shares(file_id);
CREATE INDEX idx_shares_creator ON shares(created_by);
```

- [ ] **Step 6: Verify migrations are valid SQL**

Run: `cd backend && sqlx migrate info --database-url postgres://rustshare:changeme@localhost:5432/rustshare`
Expected: Lists 5 pending migrations (requires Docker PostgreSQL running)

- [ ] **Step 7: Commit**

```bash
git add backend/migrations/
git commit -m "feat(db): add PostgreSQL schema migrations for events and projections"
```

---

## Task 5: Storage Crate - Event Store

**Files:**
- Create: `backend/crates/storage/Cargo.toml`
- Create: `backend/crates/storage/src/lib.rs`
- Create: `backend/crates/storage/src/event_store.rs`
- Test: `backend/crates/storage/src/event_store.rs` (integration tests)

- [ ] **Step 1: Create storage Cargo.toml**

```toml
[package]
name = "rustshare-storage"
version.workspace = true
edition.workspace = true

[dependencies]
rustshare-core = { path = "../core" }
sqlx.workspace = true
tokio.workspace = true
uuid.workspace = true
chrono.workspace = true
serde.workspace = true
serde_json.workspace = true
anyhow.workspace = true
thiserror.workspace = true
tracing.workspace = true
```

- [ ] **Step 2: Create lib.rs**

```rust
//! Storage layer for RustShare.
//!
//! Handles persistence to PostgreSQL and RustFS.

pub mod event_store;
pub mod metadata;
pub mod object_store;

pub use event_store::EventStore;
```

- [ ] **Step 3: Write test for event store append**

```rust
use anyhow::Result;
use rustshare_core::events::*;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

/// Event store for append-only event log
pub struct EventStore {
    pool: PgPool,
}

impl EventStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Append a new event to the event store
    pub async fn append(&self, event: &Event) -> Result<()> {
        todo!("implement append")
    }

    /// Get all events for an aggregate
    pub async fn get_events(
        &self,
        aggregate_id: Uuid,
        aggregate_type: AggregateType,
    ) -> Result<Vec<Event>> {
        todo!("implement get_events")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustshare_core::events::{Event, EventType, AggregateType};
    use serde_json::json;

    async fn setup_test_db() -> PgPool {
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://rustshare:changeme@localhost:5432/rustshare".to_string());

        PgPool::connect(&database_url).await.unwrap()
    }

    #[tokio::test]
    #[ignore] // Requires database
    async fn test_append_and_retrieve_event() {
        let pool = setup_test_db().await;
        let store = EventStore::new(pool);

        let user_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();
        let payload = json!({
            "file_id": file_id.to_string(),
            "name": "test.txt",
            "size": 1024
        });

        let event = Event::new(
            EventType::FileUploaded,
            file_id,
            AggregateType::File,
            payload,
            user_id,
        );

        // Append event
        store.append(&event).await.unwrap();

        // Retrieve events
        let events = store.get_events(file_id, AggregateType::File).await.unwrap();

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, EventType::FileUploaded);
        assert_eq!(events[0].aggregate_id, file_id);
    }
}
```

- [ ] **Step 4: Run test to verify it fails**

Run: `cd backend && cargo test -p rustshare-storage test_append_and_retrieve_event -- --ignored`
Expected: FAIL with "not yet implemented: implement append"

- [ ] **Step 5: Implement append method**

Replace `append` method:

```rust
    /// Append a new event to the event store
    pub async fn append(&self, event: &Event) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO events (event_id, event_type, aggregate_id, aggregate_type, payload, user_id, timestamp, version)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
            event.id,
            serde_json::to_string(&event.event_type)?,
            event.aggregate_id,
            serde_json::to_string(&event.aggregate_type)?,
            event.payload,
            event.user_id,
            event.timestamp,
            event.version,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }
```

- [ ] **Step 6: Implement get_events method**

Replace `get_events` method:

```rust
    /// Get all events for an aggregate
    pub async fn get_events(
        &self,
        aggregate_id: Uuid,
        aggregate_type: AggregateType,
    ) -> Result<Vec<Event>> {
        let aggregate_type_str = serde_json::to_string(&aggregate_type)?;

        let rows = sqlx::query!(
            r#"
            SELECT event_id, event_type, aggregate_id, aggregate_type, payload, user_id, timestamp, version
            FROM events
            WHERE aggregate_id = $1 AND aggregate_type = $2
            ORDER BY timestamp ASC
            "#,
            aggregate_id,
            aggregate_type_str,
        )
        .fetch_all(&self.pool)
        .await?;

        let events = rows
            .into_iter()
            .map(|row| {
                Ok(Event {
                    id: row.event_id,
                    event_type: serde_json::from_str(&row.event_type)?,
                    aggregate_id: row.aggregate_id,
                    aggregate_type: serde_json::from_str(&row.aggregate_type)?,
                    payload: row.payload,
                    user_id: row.user_id,
                    timestamp: row.timestamp,
                    version: row.version,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(events)
    }
```

- [ ] **Step 7: Run test with database**

Note: This requires Docker Compose with PostgreSQL running and migrations applied.

Run:
```bash
cd backend
# Start PostgreSQL if not running
docker-compose up -d postgres
# Run migrations
sqlx migrate run --database-url postgres://rustshare:changeme@localhost:5432/rustshare
# Run test
cargo test -p rustshare-storage test_append_and_retrieve_event -- --ignored
```
Expected: PASS

- [ ] **Step 8: Commit**

```bash
git add backend/crates/storage/
git commit -m "feat(storage): implement event store with append and query"
```

---

## Task 6: Auth Crate - Password Hashing

**Files:**
- Create: `backend/crates/auth/Cargo.toml`
- Create: `backend/crates/auth/src/lib.rs`
- Create: `backend/crates/auth/src/password.rs`

- [ ] **Step 1: Write test for password hashing**

Create `backend/crates/auth/Cargo.toml`:

```toml
[package]
name = "rustshare-auth"
version.workspace = true
edition.workspace = true

[dependencies]
argon2.workspace = true
rand.workspace = true
thiserror.workspace = true
```

Create `backend/crates/auth/src/lib.rs`:

```rust
//! Authentication and authorization for RustShare.

pub mod password;
pub mod jwt;
// Note: session module will be added in Phase 2 for session management

pub use password::PasswordHasher;
pub use jwt::JwtManager;
```

Create `backend/crates/auth/src/password.rs`:

```rust
use argon2::{
    password_hash::{PasswordHash, PasswordHasher as _, PasswordVerifier, SaltString},
    Argon2,
};
use rand::rngs::OsRng;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PasswordError {
    #[error("Failed to hash password: {0}")]
    HashError(String),

    #[error("Invalid password")]
    InvalidPassword,
}

/// Password hasher using Argon2id
pub struct PasswordHasher;

impl PasswordHasher {
    /// Hash a password using Argon2id
    pub fn hash(password: &str) -> Result<String, PasswordError> {
        todo!("implement hash")
    }

    /// Verify a password against a hash
    pub fn verify(password: &str, hash: &str) -> Result<bool, PasswordError> {
        todo!("implement verify")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_password_hash_and_verify() {
        let password = "secure_password_123";
        let hash = PasswordHasher::hash(password).unwrap();

        assert!(PasswordHasher::verify(password, &hash).unwrap());
        assert!(!PasswordHasher::verify("wrong_password", &hash).unwrap());
    }

    #[test]
    fn test_different_hashes_for_same_password() {
        let password = "test123";
        let hash1 = PasswordHasher::hash(password).unwrap();
        let hash2 = PasswordHasher::hash(password).unwrap();

        // Hashes should be different due to different salts
        assert_ne!(hash1, hash2);

        // But both should verify
        assert!(PasswordHasher::verify(password, &hash1).unwrap());
        assert!(PasswordHasher::verify(password, &hash2).unwrap());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd backend && cargo test -p rustshare-auth`
Expected: FAIL with "not yet implemented"

- [ ] **Step 3: Implement hash method**

Replace `hash` method:

```rust
    /// Hash a password using Argon2id
    pub fn hash(password: &str) -> Result<String, PasswordError> {
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();

        let hash = argon2
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| PasswordError::HashError(e.to_string()))?;

        Ok(hash.to_string())
    }
```

- [ ] **Step 4: Implement verify method**

Replace `verify` method:

```rust
    /// Verify a password against a hash
    pub fn verify(password: &str, hash: &str) -> Result<bool, PasswordError> {
        let parsed_hash = PasswordHash::new(hash)
            .map_err(|e| PasswordError::HashError(e.to_string()))?;

        let argon2 = Argon2::default();

        match argon2.verify_password(password.as_bytes(), &parsed_hash) {
            Ok(()) => Ok(true),
            Err(_) => Ok(false),
        }
    }
```

- [ ] **Step 5: Run tests**

Run: `cd backend && cargo test -p rustshare-auth`
Expected: 2 tests passed

- [ ] **Step 6: Commit**

```bash
git add backend/crates/auth/
git commit -m "feat(auth): implement Argon2id password hashing"
```

---

## Task 7: Auth Crate - JWT Tokens

**Files:**
- Modify: `backend/crates/auth/Cargo.toml`
- Create: `backend/crates/auth/src/jwt.rs`

- [ ] **Step 1: Add JWT dependencies**

Add to `backend/crates/auth/Cargo.toml`:

```toml
[dependencies]
# ... existing deps ...
jsonwebtoken.workspace = true
serde.workspace = true
chrono.workspace = true
uuid.workspace = true
```

- [ ] **Step 2: Write test for JWT generation**

Create `backend/crates/auth/src/jwt.rs`:

```rust
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum JwtError {
    #[error("Failed to encode token: {0}")]
    EncodeError(String),

    #[error("Failed to decode token: {0}")]
    DecodeError(String),

    #[error("Token expired")]
    TokenExpired,
}

/// JWT claims
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: String,      // Subject (user ID)
    pub email: String,
    pub exp: i64,         // Expiration time
    pub iat: i64,         // Issued at
    pub iss: String,      // Issuer
}

/// JWT token manager
pub struct JwtManager {
    secret: String,
}

impl JwtManager {
    pub fn new(secret: String) -> Self {
        Self { secret }
    }

    /// Generate a JWT token for a user
    pub fn generate(&self, user_id: Uuid, email: String) -> Result<String, JwtError> {
        todo!("implement generate")
    }

    /// Validate and decode a JWT token
    pub fn validate(&self, token: &str) -> Result<Claims, JwtError> {
        todo!("implement validate")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_and_validate_token() {
        let secret = "test_secret_key_at_least_32_chars_long_for_security";
        let manager = JwtManager::new(secret.to_string());

        let user_id = Uuid::new_v4();
        let email = "test@example.com".to_string();

        let token = manager.generate(user_id, email.clone()).unwrap();
        let claims = manager.validate(&token).unwrap();

        assert_eq!(claims.sub, user_id.to_string());
        assert_eq!(claims.email, email);
        assert_eq!(claims.iss, "rustshare");
    }

    #[test]
    fn test_invalid_token_fails_validation() {
        let secret = "test_secret_key_at_least_32_chars_long_for_security";
        let manager = JwtManager::new(secret.to_string());

        let result = manager.validate("invalid.token.here");
        assert!(result.is_err());
    }
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cd backend && cargo test -p rustshare-auth test_generate_and_validate`
Expected: FAIL with "not yet implemented"

- [ ] **Step 4: Implement generate method**

Replace `generate` method:

```rust
    /// Generate a JWT token for a user
    pub fn generate(&self, user_id: Uuid, email: String) -> Result<String, JwtError> {
        let now = Utc::now();
        let expiration = now + Duration::hours(24);

        let claims = Claims {
            sub: user_id.to_string(),
            email,
            exp: expiration.timestamp(),
            iat: now.timestamp(),
            iss: "rustshare".to_string(),
        };

        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(self.secret.as_bytes()),
        )
        .map_err(|e| JwtError::EncodeError(e.to_string()))
    }
```

- [ ] **Step 5: Implement validate method**

Replace `validate` method:

```rust
    /// Validate and decode a JWT token
    pub fn validate(&self, token: &str) -> Result<Claims, JwtError> {
        let validation = Validation::default();

        let token_data = decode::<Claims>(
            token,
            &DecodingKey::from_secret(self.secret.as_bytes()),
            &validation,
        )
        .map_err(|e| JwtError::DecodeError(e.to_string()))?;

        Ok(token_data.claims)
    }
```

- [ ] **Step 6: Run tests**

Run: `cd backend && cargo test -p rustshare-auth`
Expected: 4 tests passed

- [ ] **Step 7: Commit**

```bash
git add backend/crates/auth/
git commit -m "feat(auth): implement JWT token generation and validation"
```

---

## Task 8: Storage Metadata Layer (User Queries)

**Files:**
- Create: `backend/crates/storage/src/metadata.rs`

- [ ] **Step 1: Write test for user creation**

```rust
use anyhow::Result;
use rustshare_core::domain::User;
use sqlx::PgPool;
use uuid::Uuid;

/// Metadata store for querying projection tables
pub struct MetadataStore {
    pool: PgPool,
}

impl MetadataStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Create a new user in the projection table
    pub async fn create_user(&self, user: &User) -> Result<()> {
        todo!("implement create_user")
    }

    /// Find user by email
    pub async fn find_user_by_email(&self, email: &str) -> Result<Option<User>> {
        todo!("implement find_user_by_email")
    }

    /// Find user by ID
    pub async fn find_user_by_id(&self, id: Uuid) -> Result<Option<User>> {
        todo!("implement find_user_by_id")
    }

    /// Check if any users exist (for admin bootstrapping)
    pub async fn has_users(&self) -> Result<bool> {
        todo!("implement has_users")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustshare_core::domain::User;

    async fn setup_test_db() -> PgPool {
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://rustshare:changeme@localhost:5432/rustshare".to_string());
        PgPool::connect(&database_url).await.unwrap()
    }

    #[tokio::test]
    #[ignore]
    async fn test_create_and_find_user() {
        let pool = setup_test_db().await;
        let store = MetadataStore::new(pool);

        let user = User::new(
            "test@example.com".to_string(),
            "hash123".to_string(),
            "Test User".to_string(),
            false,
        );

        store.create_user(&user).await.unwrap();

        let found = store.find_user_by_email("test@example.com").await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().email, "test@example.com");

        // Cleanup
        sqlx::query!("DELETE FROM users WHERE email = $1", "test@example.com")
            .execute(&store.pool)
            .await
            .unwrap();
    }
}
```

- [ ] **Step 2: Run test to verify failure**

Run: `cd backend && cargo test -p rustshare-storage test_create_and_find_user -- --ignored`
Expected: FAIL

- [ ] **Step 3: Implement create_user**

```rust
    pub async fn create_user(&self, user: &User) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO users (id, email, password_hash, display_name, is_admin, storage_quota, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
            user.id,
            user.email,
            user.password_hash,
            user.display_name,
            user.is_admin,
            user.storage_quota,
            user.created_at,
            user.updated_at,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }
```

- [ ] **Step 4: Implement query methods**

```rust
    pub async fn find_user_by_email(&self, email: &str) -> Result<Option<User>> {
        let row = sqlx::query_as!(
            User,
            r#"SELECT id, email, password_hash, display_name, is_admin, storage_quota, created_at, updated_at FROM users WHERE email = $1"#,
            email
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row)
    }

    pub async fn find_user_by_id(&self, id: Uuid) -> Result<Option<User>> {
        let row = sqlx::query_as!(
            User,
            r#"SELECT id, email, password_hash, display_name, is_admin, storage_quota, created_at, updated_at FROM users WHERE id = $1"#,
            id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row)
    }

    pub async fn has_users(&self) -> Result<bool> {
        let count: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM users")
            .fetch_one(&self.pool)
            .await?
            .unwrap_or(0);

        Ok(count > 0)
    }
```

- [ ] **Step 5: Update lib.rs exports**

Add to `backend/crates/storage/src/lib.rs`:

```rust
pub use metadata::MetadataStore;
```

- [ ] **Step 6: Run test**

Run: `cd backend && cargo test -p rustshare-storage test_create_and_find_user -- --ignored`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add backend/crates/storage/src/metadata.rs backend/crates/storage/src/lib.rs
git commit -m "feat(storage): implement metadata store for user queries"
```

---

## Task 9: Object Store (S3/RustFS Client)

**Files:**
- Create: `backend/crates/storage/src/object_store.rs`

- [ ] **Step 1: Add AWS SDK dependency**

Add to `backend/crates/storage/Cargo.toml`:

```toml
[dependencies]
# ... existing ...
aws-sdk-s3.workspace = true
aws-config.workspace = true
bytes = "1.5"
```

- [ ] **Step 2: Write object store trait and implementation**

```rust
use anyhow::Result;
use aws_sdk_s3::{Client as S3Client, primitives::ByteStream};
use bytes::Bytes;
use std::time::Duration;

/// Object storage abstraction for RustFS/S3
pub struct ObjectStore {
    client: S3Client,
    bucket: String,
}

impl ObjectStore {
    /// Create new object store
    pub async fn new(endpoint: String, region: String, bucket: String) -> Result<Self> {
        let config = aws_config::from_env()
            .endpoint_url(endpoint)
            .region(aws_config::Region::new(region))
            .load()
            .await;

        let client = S3Client::new(&config);

        Ok(Self { client, bucket })
    }

    /// Put object in storage
    pub async fn put(&self, key: &str, data: Bytes) -> Result<()> {
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .body(ByteStream::from(data))
            .send()
            .await?;

        Ok(())
    }

    /// Get object from storage
    pub async fn get(&self, key: &str) -> Result<Bytes> {
        let output = self.client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await?;

        let data = output.body.collect().await?;
        Ok(data.into_bytes())
    }

    /// Delete object from storage
    pub async fn delete(&self, key: &str) -> Result<()> {
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await?;

        Ok(())
    }

    /// Check if object exists
    pub async fn exists(&self, key: &str) -> Result<bool> {
        match self.client
            .head_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
        {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }
}
```

- [ ] **Step 3: Update lib.rs exports**

Add to `backend/crates/storage/src/lib.rs`:

```rust
pub use object_store::ObjectStore;
```

- [ ] **Step 4: Commit**

```bash
git add backend/crates/storage/
git commit -m "feat(storage): implement S3/RustFS object store client"
```

---

## Task 10: HTTP API Server Setup

**Files:**
- Create: `backend/server/Cargo.toml`
- Create: `backend/server/src/main.rs`

- [ ] **Step 1: Create server Cargo.toml**

```toml
[package]
name = "rustshare-server"
version.workspace = true
edition.workspace = true

[[bin]]
name = "rustshare-server"
path = "src/main.rs"

[dependencies]
rustshare-core = { path = "../crates/core" }
rustshare-storage = { path = "../crates/storage" }
rustshare-auth = { path = "../crates/auth" }

tokio.workspace = true
axum.workspace = true
tower.workspace = true
tower-http.workspace = true
sqlx.workspace = true
serde.workspace = true
serde_json.workspace = true
uuid.workspace = true
chrono.workspace = true
anyhow.workspace = true
tracing.workspace = true
tracing-subscriber.workspace = true
dotenv = "0.15"
```

- [ ] **Step 2: Write basic server main.rs**

```rust
use anyhow::Result;
use axum::{
    routing::{get, post},
    Json, Router,
};
use rustshare_auth::{JwtManager, PasswordHasher};
use rustshare_core::domain::User;
use rustshare_storage::{EventStore, MetadataStore, ObjectStore};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use tower_http::trace::TraceLayer;
use tracing::info;

/// Application state shared across handlers
#[derive(Clone)]
pub struct AppState {
    pub db_pool: PgPool,
    pub metadata_store: Arc<MetadataStore>,
    pub event_store: Arc<EventStore>,
    pub object_store: Arc<ObjectStore>,
    pub jwt_manager: Arc<JwtManager>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load environment variables
    dotenv::dotenv().ok();

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info,rustshare=debug".to_string()),
        )
        .init();

    info!("Starting RustShare server");

    // Connect to database
    let database_url = std::env::var("DATABASE_URL")?;
    let db_pool = PgPool::connect(&database_url).await?;

    info!("Connected to database");

    // Run migrations (path relative to workspace root)
    sqlx::migrate!("../migrations")
        .run(&db_pool)
        .await?;

    info!("Database migrations applied");

    // Initialize stores
    let metadata_store = Arc::new(MetadataStore::new(db_pool.clone()));
    let event_store = Arc::new(EventStore::new(db_pool.clone()));

    // Initialize object store
    let rustfs_endpoint = std::env::var("RUSTFS_ENDPOINT")?;
    let rustfs_region = std::env::var("RUSTFS_REGION")?;
    let rustfs_bucket = std::env::var("RUSTFS_BUCKET")?;

    let object_store = Arc::new(
        ObjectStore::new(rustfs_endpoint, rustfs_region, rustfs_bucket).await?,
    );

    info!("Object store initialized");

    // Initialize JWT manager
    let jwt_secret = std::env::var("JWT_SECRET")?;
    let jwt_manager = Arc::new(JwtManager::new(jwt_secret));

    // Bootstrap admin user if no users exist
    if !metadata_store.has_users().await? {
        let admin_email = std::env::var("RUSTSHARE_ADMIN_EMAIL")?;
        let admin_password = std::env::var("RUSTSHARE_ADMIN_PASSWORD")?;

        let password_hash = PasswordHasher::hash(&admin_password)?;
        let admin_user = User::new(
            admin_email.clone(),
            password_hash,
            "Administrator".to_string(),
            true,
        );

        metadata_store.create_user(&admin_user).await?;

        info!("Admin user created: {}", admin_email);
    }

    // Build application state
    let state = AppState {
        db_pool,
        metadata_store,
        event_store,
        object_store,
        jwt_manager,
    };

    // Build router
    let app = Router::new()
        .route("/health", get(health_check))
        .route("/api/auth/login", post(login))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    // Start server
    let host = std::env::var("SERVER_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port = std::env::var("SERVER_PORT").unwrap_or_else(|_| "8080".to_string());
    let addr = format!("{}:{}", host, port);

    info!("Server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

/// Health check endpoint
async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
    })
}

#[derive(Serialize)]
struct HealthResponse {
    status: String,
}

/// Login request
#[derive(Deserialize)]
struct LoginRequest {
    email: String,
    password: String,
}

/// Login response
#[derive(Serialize)]
struct LoginResponse {
    token: String,
    user: UserResponse,
}

#[derive(Serialize)]
struct UserResponse {
    id: String,
    email: String,
    display_name: String,
    is_admin: bool,
}

/// Login handler
async fn login(
    axum::extract::State(state): axum::extract::State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, (axum::http::StatusCode, String)> {
    // Find user
    let user = state
        .metadata_store
        .find_user_by_email(&req.email)
        .await
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| {
            (
                axum::http::StatusCode::UNAUTHORIZED,
                "Invalid credentials".to_string(),
            )
        })?;

    // Verify password
    let is_valid = PasswordHasher::verify(&req.password, &user.password_hash)
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if !is_valid {
        return Err((
            axum::http::StatusCode::UNAUTHORIZED,
            "Invalid credentials".to_string(),
        ));
    }

    // Generate JWT
    let token = state
        .jwt_manager
        .generate(user.id, user.email.clone())
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(LoginResponse {
        token,
        user: UserResponse {
            id: user.id.to_string(),
            email: user.email,
            display_name: user.display_name,
            is_admin: user.is_admin,
        },
    }))
}
```

- [ ] **Step 3: Build server**

Run: `cd backend && cargo build --bin rustshare-server`
Expected: Successful build

- [ ] **Step 4: Commit**

```bash
git add backend/server/
git commit -m "feat(server): add HTTP server with health check and login"
```

---

## Task 11: Docker Compose Setup

**Files:**
- Create: `docker-compose.yml`
- Create: `docker-compose.dev.yml`
- Create: `docker/backend.Dockerfile`

- [ ] **Step 1: Create docker-compose.yml**

```yaml
version: '3.8'

services:
  postgres:
    image: postgres:16-alpine
    environment:
      POSTGRES_DB: rustshare
      POSTGRES_USER: rustshare
      POSTGRES_PASSWORD: changeme
    volumes:
      - postgres_data:/var/lib/postgresql/data
    ports:
      - "5432:5432"
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U rustshare"]
      interval: 10s
      timeout: 5s
      retries: 5

  rustfs:
    image: minio/minio:latest
    environment:
      MINIO_ROOT_USER: rustfsadmin
      MINIO_ROOT_PASSWORD: rustfsadmin
    volumes:
      - rustfs_data:/data
    ports:
      - "9000:9000"
      - "9001:9001"
    command: server /data --console-address ":9001"
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:9000/minio/health/live"]
      interval: 10s
      timeout: 5s
      retries: 5

volumes:
  postgres_data:
  rustfs_data:
```

- [ ] **Step 2: Create backend.Dockerfile**

```dockerfile
FROM rust:1.76-alpine AS builder

WORKDIR /app

# Install build dependencies
RUN apk add --no-cache musl-dev openssl-dev

# Copy manifests
COPY backend/Cargo.toml backend/Cargo.lock ./

# Copy source
COPY backend/crates ./crates/
COPY backend/server ./server/
COPY backend/migrations ./migrations/

# Build
RUN cargo build --release --bin rustshare-server

# Runtime image
FROM alpine:3.19

RUN apk add --no-cache libgcc openssl ca-certificates

COPY --from=builder /app/target/release/rustshare-server /usr/local/bin/

CMD ["rustshare-server"]
```

- [ ] **Step 3: Create docker-compose.dev.yml**

```yaml
version: '3.8'

services:
  backend:
    build:
      context: .
      dockerfile: docker/backend.Dockerfile
    environment:
      DATABASE_URL: postgres://rustshare:changeme@postgres:5432/rustshare
      RUSTFS_ENDPOINT: http://rustfs:9000
      RUSTFS_ACCESS_KEY: rustfsadmin
      RUSTFS_SECRET_KEY: rustfsadmin
      RUSTFS_BUCKET: rustshare-data
      RUSTFS_REGION: us-east-1
      JWT_SECRET: dev-secret-key-change-in-production-12345
      RUSTSHARE_ADMIN_EMAIL: admin@localhost
      RUSTSHARE_ADMIN_PASSWORD: admin123
      SERVER_HOST: 0.0.0.0
      SERVER_PORT: 8080
      RUST_LOG: info,rustshare=debug
    depends_on:
      postgres:
        condition: service_healthy
      rustfs:
        condition: service_healthy
    ports:
      - "8080:8080"
```

- [ ] **Step 4: Test Docker Compose**

Run: `docker-compose -f docker-compose.yml -f docker-compose.dev.yml up -d`
Expected: All services start successfully

- [ ] **Step 5: Test health endpoint**

Run: `curl http://localhost:8080/health`
Expected: `{"status":"ok"}`

- [ ] **Step 6: Test login**

Run:
```bash
curl -X POST http://localhost:8080/api/auth/login \
  -H "Content-Type: application/json" \
  -d '{"email":"admin@localhost","password":"admin123"}'
```
Expected: JSON response with token and user info

- [ ] **Step 7: Commit**

```bash
git add docker-compose.yml docker-compose.dev.yml docker/
git commit -m "feat(docker): add Docker Compose setup for development"
```

---

## Task 12: README Documentation

**Files:**
- Create: `README.md`

- [ ] **Step 1: Write README**

```markdown
# RustShare

Personal/team file synchronization and sharing platform built with Rust.

## Phase 1: Foundation (Current)

✅ Core domain models
✅ Event-sourced architecture
✅ PostgreSQL database
✅ Authentication (Argon2id + JWT)
✅ Basic HTTP API
✅ S3/RustFS integration
✅ Docker Compose setup

## Quick Start

### Prerequisites

- Docker and Docker Compose
- Rust 1.75+ (for local development)

### Run with Docker Compose

1. Clone the repository:
```bash
git clone https://github.com/yourusername/rustshare.git
cd rustshare
```

2. Start services:
```bash
docker-compose -f docker-compose.yml -f docker-compose.dev.yml up -d
```

3. Check health:
```bash
curl http://localhost:8080/health
```

4. Login (default admin):
```bash
curl -X POST http://localhost:8080/api/auth/login \
  -H "Content-Type: application/json" \
  -d '{"email":"admin@localhost","password":"admin123"}'
```

### Access Services

- **Backend API:** http://localhost:8080
- **PostgreSQL:** localhost:5432
- **MinIO Console:** http://localhost:9001 (rustfsadmin / rustfsadmin)

### Local Development

1. Install dependencies:
```bash
cd backend
cargo build
```

2. Copy environment file:
```bash
cp .env.example .env
```

3. Start infrastructure:
```bash
docker-compose up -d postgres rustfs
```

4. Run migrations:
```bash
sqlx migrate run
```

5. Run server:
```bash
cargo run --bin rustshare-server
```

### Run Tests

```bash
# Unit tests
cargo test

# Integration tests (requires database)
docker-compose up -d postgres rustfs
cargo test -- --ignored
```

## Architecture

- **Modular Monolith:** Cargo workspace with separate crates
- **Event Sourcing:** All state changes stored as events
- **PostgreSQL:** Event store + projection tables
- **RustFS (MinIO):** S3-compatible object storage for file blobs

## Project Structure

```
backend/
├── crates/
│   ├── core/         # Domain models and business logic
│   ├── storage/      # Database and object storage
│   ├── auth/         # Authentication
│   └── protocols/    # HTTP/WebDAV/S3 adapters (future)
├── server/           # Main application
└── migrations/       # Database migrations
```

## Roadmap

### Phase 2: File Operations
- File upload/download with chunking
- Folder management
- File versioning
- Conflict detection

### Phase 3: Real-time Sync
- WebSocket sync protocol
- Change notifications
- Multi-device sync

### Phase 4: Protocols
- WebDAV support
- S3-compatible API

### Phase 5: Frontend
- SvelteKit web UI
- File browser
- Share management

## License

Apache 2.0
```

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: add README with quick start guide"
```

---

## Final Verification

- [ ] **Step 1: Clean build**

```bash
cd backend
cargo clean
cargo build --workspace
```

Expected: All crates build successfully

- [ ] **Step 2: Run all tests**

```bash
cargo test --workspace
```

Expected: All unit tests pass

- [ ] **Step 3: Integration test with Docker**

```bash
# Start services
docker-compose -f docker-compose.yml -f docker-compose.dev.yml up -d

# Wait for services to be ready
sleep 10

# Test health
curl http://localhost:8080/health

# Test login
curl -X POST http://localhost:8080/api/auth/login \
  -H "Content-Type: application/json" \
  -d '{"email":"admin@localhost","password":"admin123"}'

# Cleanup
docker-compose down
```

Expected: All endpoints respond correctly

- [ ] **Step 4: Final commit**

```bash
git add -A
git commit -m "feat: complete Phase 1 foundation implementation"
git tag v0.1.0-phase1
```

---

## Phase 1 Complete! 🎉

You now have a working foundation:
- ✅ Rust workspace with modular architecture
- ✅ Domain models for User, File, Folder, Share
- ✅ Event store for audit trail
- ✅ PostgreSQL with migrations
- ✅ Authentication with Argon2id + JWT
- ✅ HTTP API with health check and login
- ✅ S3/RustFS client for object storage
- ✅ Docker Compose development environment
- ✅ Admin user bootstrapping

**Next Steps:**
- Phase 2 will add file upload/download, folder management, and basic file operations
- Phase 3 will add WebSocket real-time sync and conflict detection
- Phase 4 will add WebDAV and S3-compatible protocols
- Phase 5 will add the SvelteKit frontend

**To proceed with Phase 2, create a new plan:** `docs/superpowers/plans/2026-03-17-rustshare-phase2-files.md`