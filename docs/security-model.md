# RustShare Security Model

> **Status:** Production-readiness gap closure complete — Workstreams A–F  
> **Scope:** Backend, frontend, and deployment runtime  
> **Last updated:** 2026-06-18

---

## 1. Threat Model

### What RustShare Protects Against

| Threat | Mitigation |
|--------|------------|
| Unauthorized file access | JWT session cookies, ACL checks on every download, share-token validation, tenant-scoped queries |
| Cross-tenant data access | Repository-level `tenant_id` filtering; optional `X-Tenant-ID` validation/tenant derivation for anonymous public routes; tenant-scoped share-session JWT claims |
| Credential stuffing / brute force | Argon2id password hashing, per-IP rate limiting on login |
| Session hijacking | HttpOnly cookies, server-side session records, CSRF tokens for mutations |
| Data exposure at rest | AES-256-GCM encryption of sensitive fields via `RUSTSHARE_SECRET_ENCRYPTION_KEY` |
| Share-link abuse | Optional password protection, expiry dates, access logging, rate limiting |
| Cross-site scripting (XSS) | HTML escaping in markdown rendering, `javascript:` URL stripping, CSP-friendly headers |
| Cross-site request forgery (CSRF) | CSRF protection enforced on cookie-authenticated mutation routes |
| SQL injection | `sqlx` compile-time query checks; parameterized queries only |
| Webhook spoofing / tampering | HMAC-SHA256 signatures over raw request bodies; HTTPS-only webhook registration |
| Insecure session cookies | Session and CSRF cookies default to `Secure`; explicit opt-out required for HTTP development |
| Object-store blob corruption | Content-addressed `blobs/{sha256}` uploads and app-mediated downloads are SHA-256 verified |

### What RustShare Does NOT Protect Against

| Limitation | Notes |
|------------|-------|
| Compromised host OS | If the server is rooted, encryption keys and memory are exposed. |
| Network interception without TLS | TLS termination is the operator's responsibility (reverse proxy). |
| Malicious file contents | Virus scanning is out of scope for the core platform. |
| Social engineering | Share-link passwords can be forwarded by legitimate recipients. |
| Insider threats from admins | Admins have broad access; audit logs help detect misuse but cannot prevent it. |

> **Honest disclaimer:** RustShare has not undergone an external penetration test. The security posture is based on code review, standard Rust practices, automated dependency auditing, and the production-readiness gap closure work.

---

## 2. Authentication

### Browser Sessions (Primary)

- **Mechanism:** JWT embedded in a secure, `HttpOnly`, `SameSite=Lax` cookie.
- **Storage:** Session records are persisted in `user_sessions` (PostgreSQL) and include `expires_at`, `ip_address`, `user_agent`, and `tenant_id`.
- **Expiry:** Controlled by `JWT_EXPIRY_HOURS` (default: 24 hours).
- **Refresh:** Not yet implemented as a separate refresh-token flow. Users must re-authenticate after expiry.
- **Revocation:** Sessions can be deleted individually (`DELETE /api/v1/me/sessions/:id`) or globally by changing passwords / disabling the user.
- **CSRF:** All cookie-authenticated mutation routes require a valid CSRF token.
- **Secure flag:** Session and CSRF cookies default to `Secure` (HTTPS-only). Local HTTP development must explicitly opt out via `RUSTSHARE_SESSION_COOKIE_SECURE=false` (or the legacy `SESSION_COOKIE_SECURE=false`).

### Bearer Tokens (API / Mobile / Desktop)

- Some endpoints accept `Authorization: Bearer <token>` or `?token=<token>` for non-browser clients.
- These tokens are standard JWTs signed with `JWT_SECRET`.
- Mobile and desktop clients use PKCE-based OIDC flows where available.

### OIDC Integration

- RustShare supports SSO via standard OIDC (`authorization_code` flow).
- Configuration:
  - `OIDC_ISSUER_URL`
  - `OIDC_CLIENT_ID`
  - `OIDC_CLIENT_SECRET`
  - `OIDC_REDIRECT_URL`
- Mobile-specific OIDC values (`OIDC_MOBILE_CLIENT_ID`, `OIDC_MOBILE_REDIRECT_URIS`) support PKCE for native apps.
- **Limitation:** OIDC has been tested locally but not validated end-to-end against every intended production identity provider. See [PRODUCTION_READINESS.md](PRODUCTION_READINESS.md).

---

## 3. Authorization

### Permission Model

RustShare uses a hybrid ACL model:

| Level | Mechanism |
|-------|-----------|
| **Owner** | Full CRUD on owned files and folders. |
| **Internal share** | `shares` table row linking `recipient_user_id` to a file or folder with `View`, `Edit`, or `Admin` permission. |
| **Group share** | Shares can target groups; recipients inherit access. |
| **Public link** | `share_token` grants anonymous access with optional password and expiry. Upload-only links are restricted to folders. |

### ACL Checks

- Every file/folder API endpoint validates that the caller is the owner, a recipient, or accessing via a valid public share session.
- Share tokens are looked up in the database on every request; there is no blanket "allow all" path.
- Revoked shares (`revoked_at IS NOT NULL`) are immediately rejected.

### Admin Route Enforcement

- All routes under `/api/v1/admin/*` require the `AdminUser` extractor (`is_admin = true`).
- Anonymous and non-admin requests are rejected with `401 Unauthorized` or `403 Forbidden`.
- This applies to user management, groups, templates, modules, workflows, config (OIDC/SMTP/security), audit logs, chat integration admin endpoints, and replication admin endpoints.

---

## 4. Multi-Tenant Isolation

### Active Control: Repository-Level Filtering

Tenant isolation is enforced primarily at the repository and service layers:

- Tenant-scoped tables include `tenant_id` in `WHERE` clauses for reads, writes, updates, and deletes.
- Affected domains: files, folders, shares, notifications, vaults, vault files, templates, modules, user sessions, and permission-resolver lookups.
- Contract tests prove that tenant B cannot get, list, update, or delete tenant A's objects.

### Anonymous Public Routes

- Unauthenticated public-share routes accept optional `X-Tenant-ID`.
- If `X-Tenant-ID` is present, it must match the share's tenant; otherwise the request is rejected.
- If `X-Tenant-ID` is omitted, the tenant is derived from the globally unique share token before share lookups continue.
- Public chat-unfurl routes can use `X-Tenant-ID` to scope tenant resolution.
- Share token resolution is scoped to the effective tenant; a token from tenant A will not resolve when `X-Tenant-ID` specifies tenant B.

### Share-Session JWT

- When a public share session is created, `tenant_id` is embedded in the share-session JWT claims.
- All subsequent share-session routes use that claim to scope share lookups to the issuing tenant.

### RLS Defense-in-Depth

- A previous PostgreSQL RLS context middleware was removed because it set `app.current_tenant_id` / `app.current_user_id` on a connection that was returned to the pool before handler queries ran.
- RLS may be reintroduced in the future only if it can be applied on the same connection that executes handler queries (e.g., per-request connection pinning or an explicit `SET` on every acquired connection via `before_acquire`).

### Password Login Tenant Scoping

- Password login accepts an optional `tenant_id`.
- When `tenant_id` is provided, lookup is tenant-scoped and case-insensitive.
- When omitted for backward compatibility, unscoped login rejects ambiguous emails that exist in multiple tenants.
- The users table enforces per-tenant, case-insensitive email uniqueness.

### Object-Store Integrity

- File bytes are stored under content-addressed keys: `blobs/{sha256}`.
- `ObjectStore::put`, `put_if_absent`, `put_from_path`, and `put_from_path_if_absent` verify bytes against the key before upload.
- Path uploads copy verified bytes to a private temporary source before upload, avoiding path-swap or mutable-source races.
- `ObjectStore::get` verifies downloaded blob bytes before returning them.
- `ObjectStore::get_stream` verifies content-addressed blob streams and reports checksum mismatches at EOF. Because integrity is only known at EOF, verified blob streams do not advertise `Content-Length`.
- User-facing file downloads are served through backend streaming endpoints instead of presigned object-store URLs so authorization, audit behavior, response headers, and integrity checks remain enforced by RustShare.

---

## 5. Encryption

### Data at Rest

- **Key:** `RUSTSHARE_SECRET_ENCRYPTION_KEY` (AES-256-GCM).
- **Scope:** Sensitive metadata fields (e.g., share passwords, encryption key material) and selected user secrets.
- **Storage:** The encryption key is held in server memory only; it is never written to the database or object store.
- **Rotation:** Changing `RUSTSHARE_SECRET_ENCRYPTION_KEY` requires a re-encryption job (not yet automated). Back up the old key until all data encrypted with it has been re-encrypted, or you will lose access to stored secrets.

### Data in Transit

- RustShare **does not terminate TLS internally**.
- The operator **must** place Nginx (or another reverse proxy) in front of the backend and configure TLS certificates.
- Internal Docker network traffic between Nginx, backend, PostgreSQL, and RustFS is unencrypted by default. In a single-host deployment this is standard; for multi-host or zero-trust networks, overlay encryption (e.g., WireGuard, VPC-level encryption) is recommended.

---

## 6. Secrets Management

### Required Secrets

| Secret | Purpose | Generation |
|--------|---------|------------|
| `JWT_SECRET` | Signs session JWTs | `openssl rand -base64 32` |
| `RUSTSHARE_SECRET_ENCRYPTION_KEY` | Encrypts data at rest | `openssl rand -base64 32` |
| `POSTGRES_PASSWORD` | Database access | `openssl rand -hex 32` |
| `RUSTFS_ROOT_USER` / `RUSTFS_ROOT_PASSWORD` | Object storage admin | Strong random strings |
| `STORAGE_ACCESS_KEY` / `STORAGE_SECRET_KEY` | S3 API credentials | Strong random strings |
| `OIDC_CLIENT_SECRET` | OIDC RP authentication | Provided by IdP |
| `RUSTSHARE_CHAT_WEBHOOK_SECRET` | Webhook signing secret | `openssl rand -base64 32` |
| `METRICS_API_TOKEN` | Bearer token for Prometheus `/metrics` endpoint | `openssl rand -base64 32` |

### Secret Hygiene

- Run `./scripts/pre-flight.sh` before first deployment to auto-generate strong values.
- **Never** use the placeholder values from `.env.example` in production. The backend will refuse to start with known weak defaults.
- Store secrets in a vault or encrypted environment file. Do not commit `.env` to version control.
- Rotate secrets on a schedule or after personnel changes or suspected compromise. Rotation procedures are documented in the [Security Incident Runbook](runbooks/security-incident.md).

### Rotation Impact Summary

| Secret | Impact |
|--------|--------|
| `JWT_SECRET` | All sessions invalidated; users must re-authenticate. |
| `RUSTSHARE_SECRET_ENCRYPTION_KEY` | Existing encrypted data unreadable without old key until re-encrypted. |
| `POSTGRES_PASSWORD` | Brief outage while stack restarts. |
| RustFS / S3 credentials | Object storage inaccessible until all clients are updated. |
| `OIDC_CLIENT_SECRET` | SSO login fails until IdP and backend are aligned. |
| `RUSTSHARE_CHAT_WEBHOOK_SECRET` | Incoming webhook signatures fail until the remote integration is updated. |
| `METRICS_API_TOKEN` | Prometheus scrapes fail until the new token is configured. |

---

## 7. Webhook Security

### Incoming Webhooks

- Chat integration webhooks are verified with HMAC-SHA256 over the raw request body.
- Expected signature formats:
  - `v1=<hex>` for simple signatures.
  - `t=<timestamp>,v1=<hex>` for timestamped signatures (replay protection).
- Verification uses constant-time comparison to prevent timing attacks.
- Missing or invalid signatures return `401 Unauthorized` before the request body is deserialized.

### Outgoing Webhooks

- Events dispatched by RustShare include `X-RustShare-Signature` and `X-RustShare-Event` headers.
- Webhook URLs must use HTTPS in production.
- HTTP URLs are rejected unless the deployment is a debug build or `RUSTSHARE_ALLOW_HTTP_WEBHOOKS` is explicitly set to `"true"` or `"1"` (dev-only).

### Webhook Registration and SSRF Hardening

- Registration is restricted to admin users via `/api/v1/admin/integrations/chat/webhooks`.
- The endpoint validates the URL scheme and rejects non-HTTPS URLs in production.
- URLs are validated against SSRF payloads before registration:
  - Rejected hosts: `localhost`, loopback (`127.0.0.0/8`, `::1`), private IPv4 (`10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`), link-local (`169.254.0.0/16`, `fe80::/64`), multicast (`224.0.0.0/4`, `ff00::/8`), CGNAT (`100.64.0.0/10`), and IPv4-mapped IPv6 addresses (`::ffff:127.0.0.1`, `::ffff:10.0.0.1`, etc.).
  - Hostnames are resolved and every resolved IP address is checked; DNS lookups time out after 5 seconds.
  - DNS failures and internal IP resolutions are logged server-side but surfaced to API clients only as a generic "Invalid webhook URL" error.
- To mitigate DNS rebinding, the same SSRF validation is re-run immediately before every outgoing webhook dispatch.

### Replay Protection

- Incoming chat webhook events must include a timestamped HMAC-SHA256 signature (`t=<timestamp>,v1=<hex>`).
- After signature verification, the event timestamp is checked against the current time.
- `RUSTSHARE_WEBHOOK_MAX_AGE_SECONDS` controls the allowed window (default: 300 seconds).
- Events with timestamps in the future or older than the configured window are rejected with `401 Unauthorized` so that replay failures are not distinguishable from signature failures.

---

## 8. Network Security

### Default Docker Compose Topology

```text
┌─────────────────────────────────────────┐
│           Docker Network                │
│  ┌────────┐  ┌────────┐  ┌─────────┐  │
│  │  Nginx │──│ Backend│──│PostgreSQL│  │
│  └────────┘  └───┬────┘  └─────────┘  │
│                  │                     │
│              ┌───┴───┐                 │
│              │ RustFS│                 │
│              └───────┘                 │
└─────────────────────────────────────────┘
```

- Only **Nginx** exposes ports to the host (`80`, and `443` when TLS is configured).
- PostgreSQL (`5432`) and RustFS (`9000/9001`) are exposed in the default `docker-compose.yml` for local development convenience. **In production, remove or firewall these port mappings.**
- The backend (`8080`) should not be reachable directly from the public internet; all traffic should flow through Nginx.

### Nginx as Security Boundary

The included `docker/nginx.conf` sets:

- `X-Frame-Options: SAMEORIGIN`
- `X-Content-Type-Options: nosniff`
- `X-XSS-Protection: 1; mode=block`
- `client_max_body_size 0` (size limits enforced by the backend, not Nginx)
- WebSocket upgrade headers for `/api/` routes

> **Production hardening:** Add `Strict-Transport-Security`, configure a restrictive CSP, and consider rate limiting at the Nginx layer as a first line of defense.

---

## 9. File Upload Security

| Control | Implementation |
|---------|----------------|
| Size limits | `MAX_UPLOAD_SIZE_MB` (default: 5000 MB); public uploads limited to `MAX_PUBLIC_UPLOAD_SIZE` (100 MB); resumable chunk uploads limited to `MAX_CHUNK_SIZE` (100 MB). |
| Content-Type | Stored on upload but not strictly validated against file magic bytes. |
| Filename sanitization | Path traversal prevention; filenames normalized before storage. `Content-Disposition` parameters strip control characters, backslashes, and quotes. |
| Virus scanning | **Out of scope.** Integrate ClamAV or a cloud scanning API via a post-upload hook if required. |
| Anonymous uploads | Allowed only via upload-only folder share links; attribution is tracked via share session IDs. |

---

## 10. Rate Limiting

Rate limiting is applied per IP address and can be backed by Redis (distributed) or in-memory (single-node).

| Route Category | Default Limit | Purpose |
|----------------|---------------|---------|
| Auth login | 10 / minute | Brute-force protection |
| OIDC login | 30 / minute | SSO abuse protection |
| Share session creation | 5 / minute | Slow down token scanning |
| Share info / download | 30 / minute | Prevent hot-link abuse |
| Share upload | 20 / minute | Limit anonymous upload spam |
| Authenticated share admin | 120 / minute | Normal user sharing operations |

Configuration is via environment variables prefixed with `RUSTSHARE_RATE_LIMIT_`.

---

## 11. Audit Logging

### Audit Taxonomy

RustShare maintains durable audit evidence for all security-sensitive operations. Events are stored in append-only tables and are queryable via the admin audit endpoint (`GET /api/v1/admin/audit`).

#### Security Events

Stored in `user_security_events`.

| Event | Event Type | Actor | Detail |
|-------|------------|-------|--------|
| Login | `login` | User ID | IP, user-agent, session ID |
| Logout | `logout` | User ID | Session ID |
| Failed authentication | `login_failed` | User ID (or `NULL`) | IP, user-agent, reason |
| Permission denied | `permission_denied` | User ID | Resource type, resource ID, action attempted |
| Password changed | `password_changed` | User ID | IP, user-agent |
| Session revoked | `session_revoked` | User ID | IP, user-agent, session metadata |
| User disabled | `user.disabled` | Admin ID | Reason |

#### File Events

Stored in the `events` table (event-sourced aggregate log) with `aggregate_type = 'file'`.

| Event | Event Type | Aggregate | Durable Payload |
|-------|------------|-----------|-----------------|
| Upload | `FileUploaded` | File ID | Name, path, size, hash, owner, actor info |
| Download | *(via share_access_log for public links)* | File/Share ID | IP, user-agent, session |
| Replace / new version | `FileModified` | File ID | Old/new version, old/new hash, size delta |
| Rename | `FileRenamed` | File ID | Old/new name, old/new path |
| Move | `FileMoved` | File ID | Old/new parent folder, old/new path |
| Delete | `FileDeleted` | File ID | File name, folder ID |
| Restore | `FileRestored` | File ID | Restored-from version, hash, size |

#### Share Events

Stored in both the `events` table (event-sourced) and `share_access_log` (projection).

| Event | Event Type | Destination | Durable Payload |
|-------|------------|-------------|-----------------|
| Share created | `ShareCreated` | `events` | Share ID, file/folder ID, token, permissions, expiry |
| Share revoked | `ShareRevoked` | `events` | Share ID, file/folder ID, revoked-by user |
| Public link accessed (allowed) | `download` / `upload` / `browse` | `share_access_log` | IP, user-agent, actor type, session ID, success=true |
| Public link accessed (denied) | `download` / `upload` / `session_create` | `share_access_log` | IP, user-agent, actor type, session ID, success=false |
| Share permission changed | `SharePermissionChanged` | `events` | Old/new permissions, changed-by user |

#### Admin Actions

Stored in `admin_actions`.

| Event | Event Type | Actor | Target | Detail |
|-------|------------|-------|--------|--------|
| User created | `user.created` | Admin ID | User ID | Created-by, initial quota |
| User disabled | `user.disabled` | Admin ID | User ID | Reason |
| Config updated | `config.*` | Admin ID | Tenant/system | Changed keys, old/new values |

### What Is Logged

| Action | Destination |
|--------|-------------|
| Login / logout / failed auth | `user_security_events` table |
| Password changes | `user_security_events` table |
| Session revocation | `user_security_events` table |
| User disabled / password changed by admin | `user_security_events` + `admin_actions` tables |
| File uploaded / modified / renamed / moved / deleted / restored | `events` table (aggregate log) |
| Share created / revoked / permission changed | `events` table + `share_access_log` |
| Public share accessed / downloaded / uploaded (allowed) | `share_access_log` (with IP, user-agent, actor type) |
| Public share access denied (revoked, expired, bad password) | `share_access_log` (infrastructure ready; handler integration gap) |
| Replication failures | `replication_attempts` table + application logs |

### Log Locations

- **Application logs:** Container stdout/stderr, captured by Docker logging driver.
- **Nginx logs:** `/var/log/nginx/access.log` and `error.log` inside the Nginx container.
- **PostgreSQL logs:** Container stdout or configured log file.
- **RustFS logs:** Container stdout and `/logs` volume.
- **Audit tables:** `events`, `user_security_events`, `share_access_log`, `admin_actions` — all in PostgreSQL.

> **Note:** Centralized log aggregation (e.g., Loki, ELK, CloudWatch) is not included in the default deployment. Operators should configure this according to their monitoring stack.

> **Durability guarantee:** Audit events in `events`, `user_security_events`, `share_access_log`, and `admin_actions` are committed in the same database transaction as the operation they describe (or in a subsequent retryable write). They are retained indefinitely unless explicitly purged by an operator.

---

## 12. Vulnerability Reporting

If you discover a security issue, please report it responsibly:

1. **GitHub Private Security Advisories (preferred):**  
   [https://github.com/kubedoio/rustshare/security/advisories/new](https://github.com/kubedoio/rustshare/security/advisories/new)

2. **Email fallback:** `security@rustshare.io` with `[SECURITY]` in the subject.

See [SECURITY.md](../SECURITY.md) for the full policy, disclosure timeline, and supported versions.

---

## 13. Security Checklist (Pre-Deployment)

Use this checklist before exposing RustShare to the internet.

- [ ] Run `./scripts/pre-flight.sh` to generate strong secrets.
- [ ] Change **all** default passwords and keys (`JWT_SECRET`, `RUSTSHARE_SECRET_ENCRYPTION_KEY`, PostgreSQL, RustFS, S3 credentials).
- [ ] Configure TLS termination at Nginx or a load balancer.
- [ ] Set `ORIGIN` to your production domain (enables strict CSRF validation).
- [ ] Remove or firewall direct access to PostgreSQL (`5432`) and RustFS (`9000/9001`).
- [ ] Disable `PASSWORD_LOGIN_ENABLED` if you intend to use OIDC exclusively.
- [ ] Configure OIDC with a production identity provider and test the full flow.
- [ ] Disable dev-only overrides (`RUSTSHARE_ALLOW_HTTP_WEBHOOKS`, `localfs` metadata backend).
- [ ] Verify rate-limiting behavior under load.
- [ ] Run `./scripts/backup-stack.sh` and confirm the bundle is restorable.
- [ ] Review Nginx security headers and add HSTS / CSP as appropriate.
- [ ] Enable automated dependency updates (Dependabot is already configured).
- [ ] Set up log aggregation and alerting for auth failures and replication errors.
- [ ] Document your secret rotation procedure and store `.env` in a secrets manager.

---

## See Also

- [SECURITY.md](../SECURITY.md)
- [Production Readiness](PRODUCTION_READINESS.md)
- [Deployment Guide](DEPLOYMENT.md)
- [Security Incident Runbook](runbooks/security-incident.md)
- [Backup/Restore Runbook](runbooks/backup-restore.md)
- [Troubleshooting](troubleshooting.md)
