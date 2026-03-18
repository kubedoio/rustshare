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
