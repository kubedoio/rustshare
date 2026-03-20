# RustShare

A secure, high-performance file sharing and synchronization platform built with Rust. RustShare provides Dropbox-like functionality with support for public share links, user-to-user sharing, real-time synchronization via WebSocket, and robust permission management.

## ✨ Features

### Core Functionality
- **File Management**: Upload, download, rename, move, and delete files with versioning support
- **Folder Operations**: Create hierarchical folder structures with full CRUD operations
- **Public Sharing**: Generate secure share links with optional password protection and expiration dates
- **User-to-User Sharing**: Share files and folders directly with other users via email
- **Permission Management**: Granular permissions (View, Edit, Admin) with folder inheritance
- **Real-Time Sync**: WebSocket-based synchronization for multi-device support
- **Notifications**: Persistent notifications for share events and permission changes

### Security
- **Argon2id Password Hashing**: Industry-standard password security
- **JWT Authentication**: Stateless authentication with token-based access
- **Rate Limiting**: Protection against brute force and DoS attacks
- **Optimistic Locking**: Conflict detection using ETags and file hashes
- **Permission Inheritance**: Efficient permission resolution with caching

### Infrastructure
- **Event Sourcing**: Complete audit trail of all state changes
- **S3-Compatible Storage**: Flexible storage backend (MinIO/AWS S3)
- **PostgreSQL Database**: Robust data persistence with ACID guarantees
- **Docker Support**: Complete containerized deployment
- **Reverse Proxy Ready**: X-Forwarded-For support for rate limiting

## 🏗️ Architecture

RustShare is built as a modular monolith using Cargo workspaces:

- **Domain-Driven Design**: Clear separation of business logic from infrastructure
- **Event Sourcing**: All state changes stored as immutable events
- **CQRS Pattern**: Separate read models (projections) for query optimization
- **Repository Pattern**: Abstract data access layer
- **Service Layer**: Business logic orchestration

### Technology Stack

- **Language**: Rust 1.75+
- **Web Framework**: Axum (async web framework)
- **Database**: PostgreSQL 15+ with sqlx
- **Object Storage**: MinIO (S3-compatible)
- **Authentication**: JWT with Argon2id
- **Real-Time**: WebSocket with Tokio
- **Serialization**: Serde with JSON
- **Testing**: 150+ tests with coverage

## 🚀 Quick Start

### Prerequisites

- Docker and Docker Compose
- Rust 1.75+ (for local development)
- PostgreSQL 15+ (for local development)

### Option 1: Docker Compose (Recommended)

The fastest way to get RustShare running:

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

- **Backend API**: http://localhost:8080
- **Health Check**: http://localhost:8080/health
- **PostgreSQL**: localhost:5432 (user: rustshare, password: rustshare_dev, db: rustshare)
- **MinIO Console**: http://localhost:9001 (credentials: rustfsadmin / rustfsadmin)

### Default Credentials

- **Admin User**: admin@localhost / admin123

### Option 2: Local Development

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

# All tests
cargo test --all-features
```

## 📖 Configuration

RustShare uses environment variables for configuration. See `.env.example` for all available options.

### Key Environment Variables

```bash
# Database
DATABASE_URL=postgres://rustshare:rustshare_dev@localhost:5432/rustshare

# Object Storage (S3/MinIO)
STORAGE_ENDPOINT=http://localhost:9000
STORAGE_ACCESS_KEY=rustfsadmin
STORAGE_SECRET_KEY=rustfsadmin
STORAGE_BUCKET=rustshare-files
STORAGE_REGION=us-east-1

# Authentication
JWT_SECRET=your-secret-key-change-in-production
JWT_EXPIRY_HOURS=24

# Server
SERVER_HOST=0.0.0.0
SERVER_PORT=8080

# Rate Limiting
RUSTSHARE_RATE_LIMIT_AUTH_LOGIN_PER_MINUTE=10
RUSTSHARE_RATE_LIMIT_OIDC_LOGIN_PER_MINUTE=30
RUSTSHARE_RATE_LIMIT_SHARE_SESSION_PER_MINUTE=5
RUSTSHARE_RATE_LIMIT_SHARE_INFO_PER_MINUTE=30
RUSTSHARE_RATE_LIMIT_SHARE_DOWNLOAD_PER_MINUTE=30
RUSTSHARE_RATE_LIMIT_SHARE_UPLOAD_PER_MINUTE=20
RUSTSHARE_RATE_LIMIT_AUTHENTICATED_SHARE_ADMIN_PER_MINUTE=120

# Storage Quotas
DEFAULT_USER_QUOTA_GB=10
```

## 🔌 API Documentation

### Authentication

**Login**
```bash
POST /api/auth/login
Content-Type: application/json

{
  "email": "user@example.com",
  "password": "password123"
}

Response:
{
  "token": "eyJ0eXAiOiJKV1QiLCJhbGc...",
  "user": {
    "id": "uuid",
    "email": "user@example.com",
    "display_name": "User Name"
  }
}
```

### Files

**Upload File**
```bash
POST /api/files/upload?folder_id={folder_id}
Authorization: Bearer {token}
Content-Type: multipart/form-data

file: [binary data]

Response:
{
  "id": "uuid",
  "name": "document.pdf",
  "size": 1024000,
  "mime_type": "application/pdf",
  "current_version": 1,
  "created_at": "2026-03-18T12:00:00Z"
}
```

**Download File**
```bash
GET /api/files/{file_id}/download
Authorization: Bearer {token}

Response: 302 Redirect to presigned S3 URL
```

**List File Versions**
```bash
GET /api/files/{file_id}/versions
Authorization: Bearer {token}

Response:
[
  {
    "version_number": 2,
    "size": 1024000,
    "content_hash": "sha256:abc...",
    "created_at": "2026-03-18T12:00:00Z"
  }
]
```

### Folders

**Create Folder**
```bash
POST /api/folders
Authorization: Bearer {token}
Content-Type: application/json

{
  "name": "My Documents",
  "parent_folder_id": "uuid-or-null"
}

Response:
{
  "id": "uuid",
  "name": "My Documents",
  "path": "/My Documents",
  "parent_folder_id": null,
  "created_at": "2026-03-18T12:00:00Z"
}
```

**Get Folder Contents**
```bash
GET /api/folders/{folder_id}/contents
Authorization: Bearer {token}

Response:
{
  "folders": [...],
  "files": [...]
}
```

### Public Sharing

**Create Share Link**
```bash
POST /api/files/{file_id}/shares
Authorization: Bearer {token}
Content-Type: application/json

{
  "permissions": "View",
  "password": "optional-password",
  "expires_at": "2026-04-18T12:00:00Z"
}

Response:
{
  "id": "uuid",
  "share_token": "abc123xyz",
  "permissions": "View",
  "password_protected": true,
  "expires_at": "2026-04-18T12:00:00Z",
  "share_url": "http://localhost:8080/api/public/share/abc123xyz"
}
```

**Access Shared File**
```bash
GET /api/public/share/{token}/info

Response:
{
  "file_name": "document.pdf",
  "file_size": 1024000,
  "password_protected": true,
  "expires_at": "2026-04-18T12:00:00Z"
}

POST /api/public/share/{token}/session
Content-Type: application/json

{
  "password": "optional-password"
}

Response:
{
  "session_token": "jwt-token-for-download"
}

GET /api/public/share/{token}/file
Authorization: Bearer {session_token}

Response: 302 Redirect to presigned S3 URL
```

### User-to-User Sharing

**Share File with User**
```bash
POST /api/user-shares/files/{file_id}
Authorization: Bearer {token}
Content-Type: application/json

{
  "recipient_email": "colleague@example.com",
  "permissions": "Edit"
}

Response:
{
  "id": "uuid",
  "file_id": "uuid",
  "recipient_user_id": "uuid",
  "permissions": "Edit",
  "created_at": "2026-03-18T12:00:00Z"
}
```

**Share Folder with User**
```bash
POST /api/user-shares/folders/{folder_id}
Authorization: Bearer {token}
Content-Type: application/json

{
  "recipient_email": "colleague@example.com",
  "permissions": "Admin"
}
```

**List Received Shares**
```bash
GET /api/user-shares/received
Authorization: Bearer {token}

Response:
[
  {
    "share_id": "uuid",
    "resource_type": "File",
    "resource_id": "uuid",
    "resource_name": "document.pdf",
    "owner_email": "owner@example.com",
    "permissions": "Edit",
    "created_at": "2026-03-18T12:00:00Z"
  }
]
```

**Update Share Permissions**
```bash
PUT /api/user-shares/{share_id}/permission
Authorization: Bearer {token}
Content-Type: application/json

{
  "recipient_email": "colleague@example.com",
  "new_permissions": "Admin"
}
```

### Notifications

**List Notifications**
```bash
GET /api/notifications?unread_only=true
Authorization: Bearer {token}

Response:
[
  {
    "id": "uuid",
    "notification_type": "ShareReceived",
    "resource_type": "File",
    "resource_id": "uuid",
    "message": "admin@localhost shared 'document.pdf' with you",
    "read": false,
    "created_at": "2026-03-18T12:00:00Z"
  }
]
```

**Mark Notification as Read**
```bash
PUT /api/notifications/{notification_id}/read
Authorization: Bearer {token}
```

### WebSocket Sync

**Connect to Sync Stream**
```javascript
const ws = new WebSocket('ws://localhost:8080/api/sync');
ws.onopen = () => {
  ws.send(JSON.stringify({
    type: 'Auth',
    token: 'your-jwt-token'
  }));
};

ws.onmessage = (event) => {
  const syncEvent = JSON.parse(event.data);
  // Handle events: FileUploaded, FileModified, FileDeleted, etc.
};
```

**Event Types**:
- `FileUploaded`, `FileModified`, `FileRenamed`, `FileMoved`, `FileDeleted`, `FileRestored`
- `FolderCreated`, `FolderRenamed`, `FolderMoved`, `FolderDeleted`
- `ShareCreated`, `ShareRevoked`, `ShareUpdated`

## 🏗️ Project Structure

```
rustshare/
├── backend/
│   ├── crates/
│   │   ├── core/                 # Domain models and business logic
│   │   │   ├── domain/          # Domain entities (File, Folder, Share, User)
│   │   │   ├── events/          # Event types and broadcasting
│   │   │   └── services/        # Business logic services
│   │   ├── infrastructure/       # Infrastructure implementations
│   │   │   ├── repositories/    # Database access layer
│   │   │   └── event_store/     # Event persistence
│   │   ├── storage/              # Object storage (S3/MinIO)
│   │   ├── auth/                 # Authentication and authorization
│   │   └── protocols/            # Future: WebDAV, S3 API
│   ├── server/                   # Main application server
│   │   ├── handlers/            # HTTP route handlers
│   │   ├── middleware/          # Rate limiting, auth, etc.
│   │   └── websocket/           # WebSocket sync implementation
│   └── migrations/               # Database migrations
├── docker/                       # Dockerfiles
├── docs/                         # Documentation and design specs
└── README.md
```

## 🧪 Development

### Running Migrations

```bash
cd backend
sqlx migrate run

# Revert last migration
sqlx migrate revert
```

### Database Schema

The database uses event sourcing with projections:

- `events` - Immutable event log (source of truth)
- `users` - User accounts with quotas
- `files` - File metadata projections
- `folders` - Folder hierarchy
- `file_versions` - Version history
- `shares` - Both public and user-to-user shares
- `notifications` - Persistent notification queue

### Testing Strategy

- **Unit Tests**: Domain logic and services
- **Integration Tests**: Database and storage operations (marked with `#[ignore]`)
- **Property Tests**: Invariant checking with proptest
- **Coverage**: Aim for 80%+ coverage on business logic

### Code Quality

```bash
# Format code
cargo fmt

# Lint
cargo clippy -- -D warnings

# Check for security vulnerabilities
cargo audit
```

## 🐳 Docker Deployment

### Quick Start with Docker Compose

The easiest way to deploy RustShare in production:

1. **Clone the repository**
```bash
git clone https://github.com/yourusername/rustshare.git
cd rustshare
```

2. **Configure environment**
```bash
cp .env.example .env
```

3. **Update production settings** in `.env`:
```bash
# CRITICAL: Generate a strong JWT secret
openssl rand -base64 32

# Edit .env and set:
# - JWT_SECRET (use generated value above)
# - DATABASE_URL (update password)
# - STORAGE credentials (update MinIO access keys)
# - ORIGIN (your production domain)
```

4. **Update docker-compose.yml**:
   - Change PostgreSQL password (POSTGRES_PASSWORD)
   - Update MinIO credentials (MINIO_ROOT_USER/PASSWORD)

5. **Start all services**
```bash
docker-compose up -d
```

6. **Verify deployment**
```bash
# Check all services are running
docker-compose ps

# Test backend health
curl http://localhost/health

# View logs
docker-compose logs -f
```

7. **Access the application**
   - Frontend: http://localhost
   - Backend API: http://localhost/api
   - MinIO Console: http://localhost:9001

### Services Architecture

The Docker Compose stack includes:

| Service | Description | Ports | Internal URL |
|---------|-------------|-------|--------------|
| **Frontend** | SvelteKit web UI | 3000 | http://frontend:3000 |
| **Backend** | Rust API server | 8080 | http://backend:8080 |
| **Nginx** | Reverse proxy & load balancer | 80 | - |
| **PostgreSQL** | Database | 5432 | postgres:5432 |
| **MinIO** | S3-compatible storage | 9000, 9001 | http://rustfs:9000 |

All services are connected via Docker network and communicate internally using service names.

### Environment Variables

See `.env.example` for all available configuration options. Key variables:

#### Backend Configuration
```bash
# Database
DATABASE_URL=postgres://rustshare:changeme@postgres:5432/rustshare

# Storage
STORAGE_ENDPOINT=http://rustfs:9000
STORAGE_ACCESS_KEY=rustfsadmin
STORAGE_SECRET_KEY=rustfsadmin
STORAGE_BUCKET=rustshare-files
STORAGE_REGION=us-east-1

# Security
JWT_SECRET=change-this-secret-in-production
JWT_EXPIRY_HOURS=24

# Server
SERVER_HOST=0.0.0.0
SERVER_PORT=8080

# Limits
RUSTSHARE_RATE_LIMIT_AUTH_LOGIN_PER_MINUTE=10
RUSTSHARE_RATE_LIMIT_OIDC_LOGIN_PER_MINUTE=30
RUSTSHARE_RATE_LIMIT_SHARE_SESSION_PER_MINUTE=5
RUSTSHARE_RATE_LIMIT_SHARE_INFO_PER_MINUTE=30
RUSTSHARE_RATE_LIMIT_SHARE_DOWNLOAD_PER_MINUTE=30
RUSTSHARE_RATE_LIMIT_SHARE_UPLOAD_PER_MINUTE=20
RUSTSHARE_RATE_LIMIT_AUTHENTICATED_SHARE_ADMIN_PER_MINUTE=120
DEFAULT_USER_QUOTA_GB=10
```

#### Frontend Configuration
```bash
# SvelteKit origin (set to your domain in production)
ORIGIN=http://localhost:3000

# API endpoints (use nginx proxy in production)
VITE_API_URL=http://localhost/api
VITE_WS_URL=ws://localhost/api
```

### Production Deployment Checklist

Before deploying to production:

- [ ] **Security**
  - [ ] Generate strong JWT_SECRET: `openssl rand -base64 32`
  - [ ] Update database password in docker-compose.yml and DATABASE_URL
  - [ ] Change MinIO credentials (MINIO_ROOT_USER/PASSWORD in docker-compose.yml)
  - [ ] Update STORAGE_ACCESS_KEY and STORAGE_SECRET_KEY to match MinIO
  - [ ] Remove or change default admin credentials

- [ ] **SSL/TLS**
  - [ ] Set up reverse proxy with HTTPS (Caddy, Traefik, or external nginx)
  - [ ] Configure SSL certificates (Let's Encrypt recommended)
  - [ ] Update ORIGIN to use https://your-domain.com
  - [ ] Update VITE_API_URL and VITE_WS_URL to use wss:// for WebSocket

- [ ] **Domain & Network**
  - [ ] Point your domain to server IP
  - [ ] Configure firewall rules (allow 80, 443; block 5432, 9000 from public)
  - [ ] Set up monitoring for port 80 availability

- [ ] **Storage & Capacity**
  - [ ] Adjust DEFAULT_USER_QUOTA_GB based on your storage capacity
  - [ ] Configure volume mounts for persistent data
  - [ ] Set up automated backups for PostgreSQL
  - [ ] Set up automated backups for MinIO data

- [ ] **Performance & Reliability**
  - [ ] Tune auth/public-share rate limits based on expected traffic and password policy
  - [ ] Set RUST_LOG=info (disable debug logging)
  - [ ] Configure health check monitoring
  - [ ] Set up log aggregation (e.g., ELK stack, Loki)

- [ ] **Maintenance**
  - [ ] Document backup/restore procedures
  - [ ] Set up automated database migrations
  - [ ] Plan for updates and rollbacks
  - [ ] Configure alerting for service failures

### SSL/TLS Setup with Caddy (Recommended)

Caddy provides automatic HTTPS with Let's Encrypt:

1. **Create Caddyfile**:
```caddy
files.example.com {
    reverse_proxy nginx:80
}
```

2. **Update docker-compose.yml**:
```yaml
caddy:
  image: caddy:2-alpine
  ports:
    - "80:80"
    - "443:443"
  volumes:
    - ./Caddyfile:/etc/caddy/Caddyfile
    - caddy_data:/data
    - caddy_config:/config
  depends_on:
    - nginx
```

3. **Update environment**:
```bash
ORIGIN=https://files.example.com
VITE_API_URL=https://files.example.com/api
VITE_WS_URL=wss://files.example.com/api
```

### Backup & Restore

#### Database Backup
```bash
scripts/backup-stack.sh
scripts/verify-backup-bundle.sh backups/<timestamp>
scripts/restore-stack.sh backups/<timestamp>
scripts/post-restore-smoke.sh
```

#### Included Artifacts
```bash
backups/<timestamp>/postgres.sql.gz
backups/<timestamp>/rustfs-data.tar.gz
backups/<timestamp>/config.tar.gz
backups/<timestamp>/manifest.env
```

Detailed recovery steps:
- [docs/2026-03-20-backup-restore-runbook.md](docs/2026-03-20-backup-restore-runbook.md)
- [docs/2026-03-20-restore-drill-checklist.md](docs/2026-03-20-restore-drill-checklist.md)

### Monitoring & Logs

```bash
# View logs for all services
docker-compose logs -f

# View specific service logs
docker-compose logs -f backend
docker-compose logs -f frontend

# Check service health
docker-compose ps

# Restart a service
docker-compose restart backend

# Stop all services
docker-compose down

# Stop and remove volumes (WARNING: deletes all data)
docker-compose down -v
```

### Troubleshooting

**Backend won't start:**
- Check DATABASE_URL is correct
- Verify PostgreSQL is running: `docker-compose ps postgres`
- Check backend logs: `docker-compose logs backend`

**Frontend can't connect to backend:**
- Verify VITE_API_URL matches your nginx configuration
- Check nginx is running: `docker-compose ps nginx`
- Test backend directly: `curl http://localhost:8080/health`

**Storage errors:**
- Verify MinIO is running: `docker-compose ps rustfs`
- Check MinIO credentials match between docker-compose.yml and .env
- Access MinIO console: http://localhost:9001

**WebSocket connection fails:**
- Check VITE_WS_URL protocol (ws:// or wss://)
- Verify nginx websocket proxy configuration
- Check browser console for connection errors

### Scaling & Performance

For high-traffic deployments:

1. **Database optimization**:
   - Use connection pooling (already configured in backend)
   - Enable PostgreSQL query caching
   - Add read replicas for read-heavy workloads

2. **Storage optimization**:
   - Use AWS S3 instead of MinIO for better scalability
   - Enable CloudFront CDN for static assets
   - Configure S3 lifecycle policies for old versions

3. **Application scaling**:
   - Run multiple backend instances with load balancing
   - Use Redis for session storage and caching
   - Enable nginx caching for static content

4. **Monitoring**:
   - Set up Prometheus metrics collection
   - Use Grafana for visualization
   - Configure alerting for critical metrics

## 🚢 Deployment

### Production Configuration

1. **Use strong secrets**: Generate secure JWT_SECRET and database passwords
2. **Enable TLS**: Use reverse proxy (nginx/Caddy) for HTTPS
3. **Configure storage**: Use AWS S3 or properly secured MinIO
4. **Set quotas**: Adjust DEFAULT_USER_QUOTA_GB based on capacity
5. **Rate limiting**: Tune based on expected traffic
6. **Monitoring**: Set up logging and health check monitoring

### Manual Docker Build

```bash
# Build production image
docker build -f docker/backend.Dockerfile -t rustshare:latest .

# Run with production compose file
docker-compose up -d

# Check logs
docker-compose logs -f backend
```

### Reverse Proxy Example (nginx)

```nginx
upstream rustshare {
    server localhost:8080;
}

server {
    listen 443 ssl http2;
    server_name files.example.com;

    ssl_certificate /path/to/cert.pem;
    ssl_certificate_key /path/to/key.pem;

    location / {
        proxy_pass http://rustshare;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }

    location /api/sync {
        proxy_pass http://rustshare;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
    }
}
```

## 📚 Architecture Decisions

## 📚 Architecture Decisions

### Why Event Sourcing?
- Complete audit trail for compliance
- Natural fit for file versioning
- Enables temporal queries ("show me files as of yesterday")
- Facilitates debugging and replay

### Why Modular Monolith?
- Simpler deployment than microservices
- Clear module boundaries for future extraction
- Easier development and testing
- Lower operational overhead

### Why Rust?
- Memory safety without garbage collection
- Excellent async performance with Tokio
- Strong type system prevents bugs
- Great ecosystem for web services

## 🗺️ Roadmap

### ✅ Phase 1: Foundation (Complete)
- Core domain models
- Event sourcing architecture
- PostgreSQL database
- Authentication (Argon2id + JWT)
- S3/RustFS integration

### ✅ Phase 2: File Operations (Complete)
- File upload/download with chunking
- Folder management
- File versioning
- Conflict detection with optimistic locking

### ✅ Phase 3A: Public Sharing (Complete)
- Share link generation
- Password protection
- Expiration dates
- Anonymous access

### ✅ Phase 3B: Real-Time Sync (Complete)
- WebSocket sync protocol
- Event broadcasting
- Multi-device synchronization

### ✅ Phase 3A: User-to-User Sharing (Complete)
- Direct user sharing via email
- Granular permissions (View/Edit/Admin)
- Folder sharing with inheritance
- Permission caching
- Notifications system

### 🔄 Phase 4: Frontend (In Progress)
- SvelteKit web UI
- File browser interface
- Real-time updates
- Share management UI
- Mobile responsive design

### 📋 Phase 5: Advanced Features (Planned)
- Full-text search
- Trash and recovery
- Activity feed
- Collaborative editing
- Mobile apps (iOS/Android)

### 🔌 Phase 6: Protocol Support (Planned)
- WebDAV server
- S3-compatible API
- Desktop sync clients

## 🤝 Contributing

Contributions are welcome! Please follow these guidelines:

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Write tests for your changes
4. Ensure all tests pass (`cargo test`)
5. Run `cargo fmt` and `cargo clippy`
6. Commit your changes (`git commit -m 'feat: add amazing feature'`)
7. Push to the branch (`git push origin feature/amazing-feature`)
8. Open a Pull Request

### Commit Message Convention

Follow [Conventional Commits](https://www.conventionalcommits.org/):
- `feat:` New features
- `fix:` Bug fixes
- `docs:` Documentation changes
- `test:` Test additions/modifications
- `refactor:` Code refactoring
- `chore:` Build process or tooling changes

## 📄 License

Copyright 2026 RustShare Contributors

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.

See [LICENSE](LICENSE) file for details.

## 🙏 Acknowledgments

- Built with [Axum](https://github.com/tokio-rs/axum) web framework
- Database access via [sqlx](https://github.com/launchbadge/sqlx)
- Object storage with [MinIO](https://min.io/) / [AWS S3](https://aws.amazon.com/s3/)
- Inspired by Dropbox, Nextcloud, and other file sharing platforms

## 📞 Support

- **Issues**: [GitHub Issues](https://github.com/senolcolak/rustshare/issues)
- **Discussions**: [GitHub Discussions](https://github.com/senolcolak/rustshare/discussions)
- **Documentation**: [docs/](docs/)

---

**RustShare** - Secure file sharing built with Rust 🦀
