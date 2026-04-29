# RustShare Development Guide

Get the full RustShare stack running locally and run the test suite in under 15 minutes.

---

## Prerequisites

| Tool | Version | Purpose | Install |
|------|---------|---------|---------|
| Rust | 1.95.0 | Backend, workspace crates | `rustup show` (reads `rust-toolchain.toml`) |
| Node.js | 22+ | Frontend build & tooling | [nodejs.org](https://nodejs.org) or `nvm` |
| Docker & Docker Compose | latest | PostgreSQL, RustFS (S3-compatible), nginx | [docker.com](https://docker.com) |
| sqlx-cli | latest | Database migrations | `cargo install sqlx-cli --features postgres` |
| PostgreSQL client | any | Optional, for ad-hoc DB inspection | `psql` or `pgcli` |

Optional but recommended:

```bash
cargo install cargo-outdated cargo-audit
```

---

## Quick Start

### 1. Clone and configure

```bash
git clone <repo-url> rustshare
cd rustshare
cp .env.example .env
```

### 2. Generate secrets

```bash
source ./scripts/pre-flight.sh
```

This generates strong secrets for `JWT_SECRET`, `RUSTSHARE_SECRET_ENCRYPTION_KEY`, `POSTGRES_PASSWORD`, and others, appending them to `.env`.

### 3. Start dev infrastructure

```bash
docker compose -f docker-compose.dev.yml up -d
```

This starts PostgreSQL 16 and RustFS (S3-compatible object storage).

### 4. Run database migrations

```bash
cd backend
sqlx migrate run
```

---

## Backend Development

### Run unit tests

```bash
cd backend
cargo test --all-features --lib
```

### Run quality checks

```bash
cargo fmt --check
cargo clippy --all-features -- -D warnings
cargo check
```

### Run migrations

```bash
cd backend
sqlx migrate run
```

Migrations live in `backend/migrations/` and use sqlx.

### Start the backend server locally

```bash
cd backend
cargo run --bin rustshare-server
```

The server binds to `SERVER_HOST:SERVER_PORT` (default `0.0.0.0:8080`).

### Workspace structure

| Path | Description |
|------|-------------|
| `backend/crates/core` | Domain models and business logic |
| `backend/crates/storage` | PostgreSQL and object-storage integration |
| `backend/crates/auth` | Password hashing, JWTs, session helpers |
| `backend/crates/crypto` | Encryption utilities |
| `backend/crates/infrastructure` | Repositories and persistence |
| `backend/server` | Axum HTTP and WebSocket server binary |

---

## Frontend Development

### Install dependencies

```bash
cd frontend
npm install
```

### Start dev server

```bash
npm run dev
```

The Vite dev server starts on `http://localhost:5173` by default.

### Run quality checks

```bash
npm run check      # svelte-check + TypeScript
npm run lint       # prettier + eslint
npm run test       # vitest unit tests
npm run build      # production build
```

### Run e2e tests

```bash
npm run test:e2e   # requires Playwright and a running backend
```

### Frontend stack

- **Framework:** SvelteKit (SPA mode) with Vite
- **Language:** TypeScript
- **Styling:** TailwindCSS + DaisyUI
- **Data fetching:** TanStack Query
- **Testing:** Vitest (unit), Playwright (e2e)

---

## Full Stack Development

Run the backend and frontend simultaneously against shared Docker infrastructure:

```bash
# Terminal 1 — infrastructure
docker compose -f docker-compose.dev.yml up -d postgres rustfs

# Terminal 2 — backend
cd backend
sqlx migrate run
cargo run --bin rustshare-server

# Terminal 3 — frontend
cd frontend
npm run dev
```

Access the app at `http://localhost:5173`. API requests are proxied to the backend via `VITE_API_URL`.

---

## Integration Tests

Integration and contract tests require live PostgreSQL and RustFS services. They are marked `#[ignored]` by default.

### Run all integration tests

```bash
# Start required services
docker compose -f docker-compose.dev.yml up -d postgres rustfs

# Run migrations
cd backend && sqlx migrate run

# Run ignored tests
cargo test --all-features -- --ignored
```

### Run specific contract tests

```bash
cargo test --test contracts -- --ignored
```

For deeper testing guidance, see [`backend/TESTING.md`](../backend/TESTING.md).

---

## Useful Commands

### Backend

| Command | Purpose |
|---------|---------|
| `cargo check` | Fast compile check |
| `cargo build` | Debug build |
| `cargo build --release` | Optimized release build |
| `cargo test --all-features --lib` | Unit tests only |
| `cargo test --all-features -- --ignored` | Integration + contract tests |
| `cargo fmt` | Format code |
| `cargo clippy --all-features -- -D warnings` | Lint (zero warnings policy) |
| `cargo doc --open` | Build and open docs |
| `sqlx migrate run` | Apply pending migrations |
| `sqlx migrate revert` | Revert last migration |

### Frontend

| Command | Purpose |
|---------|---------|
| `npm run dev` | Start dev server |
| `npm run build` | Production build |
| `npm run preview` | Preview production build |
| `npm run check` | TypeScript + Svelte check |
| `npm run check:watch` | Watch mode for checks |
| `npm run lint` | Prettier + ESLint |
| `npm run format` | Auto-format with Prettier |
| `npm run test` | Run Vitest unit tests |
| `npm run test:watch` | Vitest watch mode |
| `npm run test:e2e` | Playwright end-to-end tests |

### Docker

| Command | Purpose |
|---------|---------|
| `docker compose -f docker-compose.dev.yml up -d` | Start dev services |
| `docker compose -f docker-compose.dev.yml down` | Stop dev services |
| `docker compose -f docker-compose.dev.yml logs -f postgres` | Tail PostgreSQL logs |
| `docker compose -f docker-compose.dev.yml logs -f rustfs` | Tail RustFS logs |

---

## IDE Setup

### VS Code (recommended)

Install these extensions for the best experience:

| Extension | Purpose |
|-----------|---------|
| `rust-lang.rust-analyzer` | Rust language server, type hints, inline errors |
| `tamasfe.even-better-toml` | TOML support for `Cargo.toml` |
| `svelte.svelte-vscode` | Svelte language support |
| `dbaeumer.vscode-eslint` | ESLint integration |
| `esbenp.prettier-vscode` | Prettier formatting |
| `bradlc.vscode-tailwindcss` | TailwindCSS IntelliSense |
| `ms-playwright.playwright` | Playwright test runner |

### rust-analyzer

`rust-analyzer` should activate automatically when opening the workspace. It respects the pinned toolchain in `rust-toolchain.toml`.

### CLion / IntelliJ

Install the **Rust** plugin. Open the project at the repository root so the workspace `Cargo.toml` is detected.

---

## Troubleshooting

| Issue | Fix |
|-------|-----|
| `sqlx migrate run` fails with connection error | Ensure PostgreSQL is running: `docker compose -f docker-compose.dev.yml ps` |
| `cargo test -- --ignored` fails | Ensure both `postgres` and `rustfs` services are healthy |
| Frontend can't reach backend | Verify `VITE_API_URL` in `.env` points to the backend |
| Weak secret errors on startup | Run `source ./scripts/pre-flight.sh` to regenerate secrets |
| Port already in use | Check `SERVER_PORT` (8080) and dev server port (5173) are free |

---

For contribution guidelines, branch naming, and commit conventions, see [`CONTRIBUTING.md`](../CONTRIBUTING.md).
