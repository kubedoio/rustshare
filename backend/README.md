# RustShare Backend

Rust-based file sharing backend with event sourcing, WebSocket real-time sync, and object storage.

## Architecture

The backend is organized as a Cargo workspace with the following crates:

- **rustshare-core**: Domain models, business logic, and services
- **rustshare-storage**: Database operations (PostgreSQL via SQLx)
- **rustshare-auth**: Authentication (Argon2id password hashing, JWT tokens)
- **rustshare-server**: HTTP/WebSocket server (Axum)

## Prerequisites

- Rust 1.70+
- PostgreSQL 15+
- S3-compatible object storage (AWS S3, MinIO, or local filesystem)
- Docker and Docker Compose (for development)

## Getting Started

### Development Setup

1. Start services:
```bash
docker-compose up -d
```

2. Run migrations:
```bash
cd backend
sqlx migrate run
```

3. Start server:
```bash
cd backend/server
cargo run
```

The server will start on `http://localhost:8080`.

### Environment Variables

Copy `.env.example` to `.env` and configure:

```bash
DATABASE_URL=postgres://rustshare:rustshare@localhost:5432/rustshare
JWT_SECRET=your-secret-key
OBJECT_STORAGE_ENDPOINT=http://localhost:9000
OBJECT_STORAGE_REGION=us-east-1
OBJECT_STORAGE_BUCKET=rustshare
OBJECT_STORAGE_ACCESS_KEY=minioadmin
OBJECT_STORAGE_SECRET_KEY=minioadmin
BROADCAST_CAPACITY=1000
```

## API Endpoints

### Authentication

- `POST /api/auth/login` - Login with email/password, returns JWT token
- `POST /api/auth/register` - Register new user (if enabled)

### Files

- `POST /api/files/upload` - Upload a new file
- `GET /api/files/:id` - Get file metadata
- `GET /api/files/:id/download` - Download file content
- `PUT /api/files/:id` - Update file content
- `GET /api/files/:id/versions` - List file versions
- `POST /api/files/:id/restore/:version_id` - Restore a previous version

### Folders

- `POST /api/folders` - Create folder
- `GET /api/folders/:id` - Get folder metadata
- `GET /api/folders/:id/contents` - List folder contents
- `GET /api/folders/:id/tree` - Get folder tree
- `PUT /api/folders/:id/rename` - Rename folder
- `PUT /api/folders/:id/move` - Move folder
- `DELETE /api/folders/:id` - Delete folder

### Health

- `GET /health` - Health check endpoint

## Phase 3A: Real-time Sync

WebSocket endpoint for real-time file/folder notifications.

**Endpoint:** `GET /api/sync`
**Auth:** JWT Bearer token in `Authorization` header during upgrade

**Client Protocol:**
- Connect with JWT token
- Optionally send `{"type":"sync","last_seen_event_id":"<uuid>"}` for catch-up
- Receive notifications: `{"event_id":"...","event_type":"FileUploaded",...}`

**Configuration:**
- `BROADCAST_CAPACITY`: Event buffer size per subscriber (default: 1000)

### Event Types

The WebSocket server broadcasts the following event types:

- `FileUploaded` - New file uploaded
- `FileModified` - File content updated
- `FileRestored` - File version restored
- `FolderCreated` - New folder created
- `FolderRenamed` - Folder name changed
- `FolderMoved` - Folder moved to new parent
- `FolderDeleted` - Folder deleted

### Testing

See `TESTING.md` for manual testing procedures.

## Development

### Running Tests

```bash
# Unit tests only (no database required)
cargo test --lib

# All tests (requires database)
cargo test

# Specific crate
cargo test -p rustshare-core

# With output
cargo test -- --nocapture
```

### Database Migrations

```bash
# Create new migration
sqlx migrate add migration_name

# Run migrations
sqlx migrate run

# Revert last migration
sqlx migrate revert
```

### Code Quality

```bash
# Format code
cargo fmt

# Lint
cargo clippy

# Check without building
cargo check
```

## Architecture Details

### Event Sourcing

All file and folder operations are recorded as immutable events in the `events` table. This provides:
- Complete audit trail
- Point-in-time recovery
- Real-time sync via event replay

### Real-time Sync

The WebSocket sync system uses:
- `tokio::sync::broadcast` for in-memory pub/sub
- Event replay from database for catch-up
- JWT authentication for secure connections
- Best-effort delivery with lag detection

### Object Storage

Files are stored in S3-compatible object storage:
- Content-addressable (SHA256 hashing)
- Automatic deduplication
- Version history support

## Deployment

### Docker Production Build

```bash
docker build -t rustshare-backend .
docker run -p 8080:8080 --env-file .env rustshare-backend
```

### Systemd Service

Example systemd unit file:

```ini
[Unit]
Description=RustShare Backend
After=network.target postgresql.service

[Service]
Type=simple
User=rustshare
WorkingDirectory=/opt/rustshare
EnvironmentFile=/opt/rustshare/.env
ExecStart=/opt/rustshare/rustshare-server
Restart=always

[Install]
WantedBy=multi-user.target
```

## License

See LICENSE file in repository root.
