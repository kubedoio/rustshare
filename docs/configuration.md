# RustShare Configuration Reference

Complete environment variable reference for operators deploying or self-hosting RustShare.

---

## Required Variables

These variables must be set before the application will start. Run `scripts/pre-flight.sh` to auto-generate strong values for most secrets.

| Variable | Description | Example | Required? |
|----------|-------------|---------|-----------|
| `POSTGRES_PASSWORD` | PostgreSQL password | `openssl rand -hex 24` | **Yes** |
| `DATABASE_URL` | PostgreSQL connection string | `postgres://rustshare:password@postgres:5432/rustshare` | **Yes** |
| `JWT_SECRET` | Signing key for JWT tokens | `openssl rand -base64 32` | **Yes** |
| `RUSTSHARE_SECRET_ENCRYPTION_KEY` | Encryption key for sensitive data at rest | `openssl rand -base64 32` | **Yes** |
| `STORAGE_ACCESS_KEY` | S3-compatible storage access key | `openssl rand -hex 16` | **Yes** |
| `STORAGE_SECRET_KEY` | S3-compatible storage secret key | `openssl rand -hex 32` | **Yes** |
| `AWS_ACCESS_KEY_ID` | AWS/S3 access key (used by backend SDK) | Same as `STORAGE_ACCESS_KEY` | **Yes** |
| `AWS_SECRET_ACCESS_KEY` | AWS/S3 secret key (used by backend SDK) | Same as `STORAGE_SECRET_KEY` | **Yes** |
| `STORAGE_ENDPOINT` | S3-compatible storage endpoint | `http://rustfs:9000` | **Yes** |
| `STORAGE_BUCKET` | S3 bucket name for file storage | `rustshare-files` | **Yes** |
| `STORAGE_REGION` | S3 region | `us-east-1` | **Yes** |
| `ORIGIN` | SvelteKit origin for CSRF protection | `https://files.example.com` | **Yes** |
| `VITE_API_URL` | Backend API URL (browser perspective) | `http://localhost/api` | **Yes** |
| `VITE_WS_URL` | WebSocket URL for real-time sync | `ws://localhost/api` | **Yes** |

---

## Database

| Variable | Description | Default | Required? |
|----------|-------------|---------|-----------|
| `POSTGRES_PASSWORD` | PostgreSQL password. Must be strong; used by both the postgres service and `DATABASE_URL`. | — | **Yes** |
| `DATABASE_URL` | Full PostgreSQL connection string. Auto-constructed by `pre-flight.sh` if empty. | — | **Yes** |

**Docker-internal alias:** The backend also accepts `RUSTFS_ENDPOINT` as an alias for the object-store endpoint when talking to RustFS inside Docker.

---

## Object Storage

RustShare supports any S3-compatible store (RustFS, MinIO, AWS S3, etc.).

| Variable | Description | Default | Required? |
|----------|-------------|---------|-----------|
| `STORAGE_ENDPOINT` | S3 endpoint URL. Use `http://rustfs:9000` inside Docker; `http://localhost:9000` for local development. | `http://rustfs:9000` | **Yes** |
| `STORAGE_ACCESS_KEY` | S3 access key. | — | **Yes** |
| `STORAGE_SECRET_KEY` | S3 secret key. | — | **Yes** |
| `STORAGE_BUCKET` | Bucket name for stored files. | `rustshare-files` | **Yes** |
| `STORAGE_REGION` | S3 region. Use `us-east-1` for RustFS/MinIO. | `us-east-1` | **Yes** |
| `AWS_ACCESS_KEY_ID` | AWS SDK access key. Typically set to the same value as `STORAGE_ACCESS_KEY`. | — | **Yes** |
| `AWS_SECRET_ACCESS_KEY` | AWS SDK secret key. Typically set to the same value as `STORAGE_SECRET_KEY`. | — | **Yes** |

**Docker-internal variables:**

| Variable | Description | Default |
|----------|-------------|---------|
| `RUSTFS_ENDPOINT` | Internal Docker endpoint for RustFS | `http://rustfs:9000` |
| `RUSTFS_PUBLIC_ENDPOINT` | Public-facing RustFS endpoint | `http://localhost:9000` |
| `RUSTFS_REGION` | RustFS region | `us-east-1` |
| `RUSTFS_BUCKET` | RustFS bucket | `rustshare-files` |
| `RUSTFS_ACCESS_KEY` | RustFS access key (Docker alias) | — |
| `RUSTFS_SECRET_KEY` | RustFS secret key (Docker alias) | — |

---

## Authentication & Security

| Variable | Description | Default | Required? |
|----------|-------------|---------|-----------|
| `JWT_SECRET` | Secret key used to sign and verify JWT tokens. **Must be changed from any default.** | — | **Yes** |
| `RUSTSHARE_SECRET_ENCRYPTION_KEY` | AES-256-GCM key for encrypting secrets at rest (API keys, passwords). **Must be strong.** | — | **Yes** |
| `JWT_EXPIRY_HOURS` | JWT token lifetime in hours. | `24` | No |
| `PASSWORD_LOGIN_ENABLED` | Enable username/password login. Set to `false` to force OIDC-only. | `true` | No |

**Session / cookie configuration:**

| Variable | Description | Default |
|----------|-------------|---------|
| `SESSION_TTL_SECONDS` | Session time-to-live in seconds. | — |
| `SESSION_USE_REVOCATION_CACHE` | Enable in-memory session revocation cache. | — |
| `SESSION_COOKIE_SECURE` | Set `Secure` flag on session cookies. | — |
| `SESSION_COOKIE_NAME` | Name of the session cookie. | — |
| `SESSION_COOKIE_SAME_SITE` | `SameSite` policy for session cookies (`Strict`, `Lax`, `None`). | — |

---

## Server

| Variable | Description | Default | Required? |
|----------|-------------|---------|-----------|
| `SERVER_HOST` | Interface to bind. Use `0.0.0.0` for all interfaces; `127.0.0.1` for localhost only. | `0.0.0.0` | No |
| `SERVER_PORT` | HTTP server port. | `8080` | No |
| `FRONTEND_DIST_DIR` | Path to the compiled frontend SPA for static serving. | `/app/frontend-build` | No |
| `RUSTSHARE_PUBLIC_URL` | Public base URL of the server (used in share links, device auth). | `http://localhost:8080` | No |
| `BROADCAST_CAPACITY` | Internal WebSocket event broadcast channel capacity. | — | No |

---

## Rate Limiting

All rates are **per IP address per minute** using a token-bucket algorithm.

| Variable | Description | Default |
|----------|-------------|---------|
| `RUSTSHARE_RATE_LIMIT_AUTH_LOGIN_PER_MINUTE` | Password login attempts | `10` |
| `RUSTSHARE_RATE_LIMIT_OIDC_LOGIN_PER_MINUTE` | OIDC login attempts | `30` |
| `RUSTSHARE_RATE_LIMIT_SHARE_SESSION_PER_MINUTE` | Public share session creation (brute-force protection) | `5` |
| `RUSTSHARE_RATE_LIMIT_SHARE_INFO_PER_MINUTE` | Public share metadata / folder listing | `30` |
| `RUSTSHARE_RATE_LIMIT_SHARE_DOWNLOAD_PER_MINUTE` | Anonymous downloads from public links | `30` |
| `RUSTSHARE_RATE_LIMIT_SHARE_UPLOAD_PER_MINUTE` | Anonymous uploads to public folder links | `20` |
| `RUSTSHARE_RATE_LIMIT_AUTHENTICATED_SHARE_ADMIN_PER_MINUTE` | Authenticated share management (create/update/delete) | `120` |
| `RUSTSHARE_RATE_LIMIT_AI_QUERY_PER_MINUTE` | AI search / summarize / ask endpoints | `30` |

**Legacy variables** (recognized but not actively used by current backend):

| Variable | Description | Default |
|----------|-------------|---------|
| `RATE_LIMIT_REQUESTS_PER_MINUTE` | Legacy global rate limit placeholder | `60` |
| `RATE_LIMIT_BURST` | Legacy burst capacity placeholder | `100` |

---

## Quotas

| Variable | Description | Default |
|----------|-------------|---------|
| `RUSTSHARE_DEFAULT_STORAGE_QUOTA_BYTES` | Default per-user storage quota in bytes. | `10737418240` (10 GB) |
| `DEFAULT_USER_QUOTA_GB` | Legacy quota in GB (recognized but prefer bytes). | `10` |
| `MAX_UPLOAD_SIZE_MB` | Maximum single-file upload size in MB. | `5000` |

---

## Frontend / CORS

| Variable | Description | Example | Required? |
|----------|-------------|---------|-----------|
| `ORIGIN` | SvelteKit origin for CSRF protection. **Must be your production domain in production.** | `http://localhost:3000` | **Yes** |
| `VITE_API_URL` | Backend API base URL from the browser's perspective. | `http://localhost/api` | **Yes** |
| `VITE_WS_URL` | WebSocket endpoint URL from the browser's perspective. | `ws://localhost/api` | **Yes** |

---

## Admin & Demo

| Variable | Description | Default | Required? |
|----------|-------------|---------|-----------|
| `RUSTSHARE_ADMIN_USERNAME` | Default admin username created on first boot. | `admin` | No |
| `RUSTSHARE_ADMIN_EMAIL` | Default admin email created on first boot. | `admin@localhost` | No |
| `RUSTSHARE_ADMIN_PASSWORD` | Default admin password created on first boot. If empty, a random password is generated and printed to logs. | — | No |
| `RUSTSHARE_DEMO_VIEWER_USERNAME` | Demo viewer account username. | `viewer` | No |
| `RUSTSHARE_DEMO_VIEWER_EMAIL` | Demo viewer account email. | `viewer@localhost` | No |
| `RUSTSHARE_DEMO_VIEWER_PASSWORD` | Demo viewer account password. | — | No |
| `RUSTSHARE_DEMO_VIEWER_DISPLAY_NAME` | Demo viewer display name. | `Viewer User` | No |
| `RUSTFS_ROOT_USER` | RustFS root user (must match `STORAGE_ACCESS_KEY`). | — | **Yes** |
| `RUSTFS_ROOT_PASSWORD` | RustFS root password (must match `STORAGE_SECRET_KEY`). | — | **Yes** |

---

## Logging

| Variable | Description | Default |
|----------|-------------|---------|
| `RUST_LOG` | Rust tracing/log filter. Use `info` for production; `debug` or `info,rustshare=debug` for development. | `info,rustshare=debug` |

Examples:

```bash
RUST_LOG=info                    # Production
RUST_LOG=debug                   # Verbose development
RUST_LOG=info,rustshare=debug    # Default: info globally, debug for RustShare crates
RUST_LOG=error                   # Quiet: errors only
```

---

## Metadata Backend

| Variable | Description | Default |
|----------|-------------|---------|
| `RUSTSHARE_METADATA_BACKEND` | Metadata storage backend. Options: `postgres`, `rustfs`, `dual_write`, `rustfs_reads`, `localfs`. | `postgres` |
| `RUSTSHARE_METADATA_PREFIX` | Object key prefix for metadata in object storage. | `apps/rustshare` |
| `RUSTSHARE_METADATA_NAMESPACE` | Metadata namespace for multi-tenancy. | `default` |
| `RUSTSHARE_METADATA_CACHE` | Enable in-memory metadata cache (`true`/`false`). | `true` |
| `RUSTSHARE_LOCALFS_PATH` | Filesystem path for `localfs` backend (development only). | `./local-metadata` |

**Migration stages:**

- `postgres` — PostgreSQL only (legacy, default).
- `rustfs` — Object storage only (new system).
- `dual_write` — Write to both; read from PostgreSQL (migration phase).
- `rustfs_reads` — Write to both; read from RustFS (validation phase).
- `localfs` — Local filesystem (**development only**).

---

## OIDC / SSO

| Variable | Description | Default |
|----------|-------------|---------|
| `OIDC_ISSUER_URL` | OIDC issuer URL (e.g., `https://auth.example.com`). | — |
| `OIDC_CLIENT_ID` | OIDC client ID for web login. | — |
| `OIDC_CLIENT_SECRET` | OIDC client secret for web login. | — |
| `OIDC_REDIRECT_URL` | OIDC redirect URL for web login (e.g., `https://files.example.com/api/v1/auth/oidc/callback`). | — |
| `OIDC_LOGIN_LABEL` | Button label shown on the login page (e.g., "Sign in with SSO"). | — |
| `OIDC_SCOPES` | Space-separated OIDC scopes. | `openid profile email` |
| `OIDC_MOBILE_CLIENT_ID` | Separate OIDC client ID for mobile apps. | — |
| `OIDC_MOBILE_CLIENT_SECRET` | Separate OIDC client secret for mobile apps. | — |
| `OIDC_MOBILE_REDIRECT_URIS` | Comma-separated allowed redirect URIs for mobile auth. | — |

---

## SCIM Provisioning

| Variable | Description | Default |
|----------|-------------|---------|
| `RUSTSHARE_SCIM_BEARER_TOKEN` | Bearer token for SCIM v1/v2 API authentication. | — |
| `RUSTSHARE_SCIM_BASE_URL` | Base URL used in SCIM resource URLs. | `http://localhost:8080` |

---

## AI Features

| Variable | Description | Default |
|----------|-------------|---------|
| `RUSTSHARE_AI_ENABLED` | Enable AI endpoints (search, summarize, ask). | — |
| `RUSTSHARE_UPLOAD_STORE_PATH` | Path for temporary upload document storage. | — |

---

## Generating Secrets

Use `scripts/pre-flight.sh` to automatically generate and validate all required secrets:

```bash
source ./scripts/pre-flight.sh
```

This will:

1. Back up your current `.env`.
2. Generate cryptographically strong values for any missing or weak secrets.
3. Auto-construct `DATABASE_URL` if empty.
4. Export all variables for `docker compose`.

To generate a single secret manually:

```bash
# For JWT_SECRET and RUSTSHARE_SECRET_ENCRYPTION_KEY
openssl rand -base64 32

# For POSTGRES_PASSWORD
openssl rand -hex 24
```

---

## Security Warnings

### ⚠️ Must change before production

The following variables **must** be set to strong, unique values. The server refuses to start with weak or missing credentials:

| Variable | Why it matters |
|----------|----------------|
| `JWT_SECRET` | Compromise allows token forgery and account takeover. |
| `RUSTSHARE_SECRET_ENCRYPTION_KEY` | Compromise exposes all encrypted secrets (API keys, passwords) at rest. |
| `POSTGRES_PASSWORD` | Compromise exposes all application data. |
| `STORAGE_ACCESS_KEY` / `STORAGE_SECRET_KEY` | Compromise allows unrestricted access to all stored files. |
| `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` | Same as above; used by the S3 SDK. |
| `RUSTFS_ROOT_PASSWORD` | Root access to the object storage backend. |

### ⚠️ Dangerous if weak

| Variable | Risk |
|----------|------|
| `RUSTSHARE_ADMIN_PASSWORD` | If left empty, a random password is generated on first boot and printed to logs. Retrieve it immediately and change it. |
| `RUSTSHARE_DEMO_VIEWER_PASSWORD` | If set, creates a publicly accessible demo account. Use a strong password or leave empty. |
| `RUSTSHARE_SCIM_BEARER_TOKEN` | If leaked, allows provisioning/unprovisioning of users via SCIM. |

### Production deployment checklist

- [ ] Generate strong `JWT_SECRET` (`openssl rand -base64 32`)
- [ ] Generate strong `RUSTSHARE_SECRET_ENCRYPTION_KEY` (`openssl rand -base64 32`)
- [ ] Update `POSTGRES_PASSWORD` and ensure `DATABASE_URL` reflects it
- [ ] Change RustFS credentials (`RUSTFS_ROOT_USER` / `RUSTFS_ROOT_PASSWORD`)
- [ ] Update `STORAGE_ACCESS_KEY` and `STORAGE_SECRET_KEY`
- [ ] Set `ORIGIN` to your production domain (e.g., `https://files.example.com`)
- [ ] Configure SSL/TLS with a reverse proxy
- [ ] Adjust `RUSTSHARE_DEFAULT_STORAGE_QUOTA_BYTES` based on capacity
- [ ] Set `RUST_LOG=info` for production
- [ ] Configure firewall rules
- [ ] Set up database backups
- [ ] Set up RustFS/S3 backups
- [ ] Plan metadata migration if moving from `postgres` to `rustfs` backend
