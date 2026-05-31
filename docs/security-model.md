# RustShare Security Model

> **Status:** Late MVP / pre-release  
> **Scope:** Backend, frontend, and deployment runtime  
> **Last updated:** 2026-04-29

---

## 1. Threat Model

### What RustShare Protects Against

| Threat | Mitigation |
|--------|------------|
| Unauthorized file access | JWT session cookies, ACL checks on every download, share-token validation |
| Credential stuffing / brute force | Argon2id password hashing, per-IP rate limiting on login |
| Session hijacking | HttpOnly cookies, server-side session records, CSRF tokens for mutations |
| Data exposure at rest | AES-256-GCM encryption of sensitive fields via `RUSTSHARE_SECRET_ENCRYPTION_KEY` |
| Share-link abuse | Optional password protection, expiry dates, access logging, rate limiting |
| Cross-site scripting (XSS) | HTML escaping in markdown rendering, `javascript:` URL stripping, CSP-friendly headers |
| Cross-site request forgery (CSRF) | CSRF protection enforced on cookie-authenticated mutation routes |
| SQL injection | `sqlx` compile-time query checks; parameterized queries only |

### What RustShare Does NOT Protect Against

| Limitation | Notes |
|------------|-------|
| Compromised host OS | If the server is rooted, encryption keys and memory are exposed. |
| Network interception without TLS | TLS termination is the operator's responsibility (reverse proxy). |
| Malicious file contents | Virus scanning is out of scope for the core platform. |
| Social engineering | Share-link passwords can be forwarded by legitimate recipients. |
| Insider threats from admins | Admins have broad access; audit logs help detect misuse but cannot prevent it. |

> **Honest disclaimer:** RustShare has not undergone an external penetration test. The security posture is based on code review, standard Rust practices, and automated dependency auditing.

---

## 2. Authentication

### Browser Sessions (Primary)

- **Mechanism:** JWT embedded in a secure, `HttpOnly`, `SameSite=Lax` cookie.
- **Storage:** Session records are persisted in `user_sessions` (PostgreSQL) and include `expires_at`, `ip_address`, and `user_agent`.
- **Expiry:** Controlled by `JWT_EXPIRY_HOURS` (default: 24 hours).
- **Refresh:** Not yet implemented as a separate refresh-token flow. Users must re-authenticate after expiry.
- **Revocation:** Sessions can be deleted individually (`DELETE /api/v1/me/sessions/:id`) or globally by changing passwords.
- **CSRF:** All cookie-authenticated mutation routes require a valid CSRF token.

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
- **Limitation:** OIDC has been tested locally but not yet validated end-to-end against every intended production identity provider. See [PRODUCTION_READINESS.md](PRODUCTION_READINESS.md).

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

---

## 4. Encryption

### Data at Rest

- **Key:** `RUSTSHARE_SECRET_ENCRYPTION_KEY` (AES-256-GCM).
- **Scope:** Sensitive metadata fields (e.g., share passwords, encryption key material) and selected user secrets.
- **Storage:** The encryption key is held in server memory only; it is never written to the database or object store.
- **Rotation:** Changing `RUSTSHARE_SECRET_ENCRYPTION_KEY` requires a re-encryption job (not yet automated). Plan downtime or a maintenance window.

### Data in Transit

- RustShare **does not terminate TLS internally**.
- The operator **must** place Nginx (or another reverse proxy) in front of the backend and configure TLS certificates.
- Internal Docker network traffic between Nginx, backend, PostgreSQL, and RustFS is unencrypted by default. In a single-host deployment this is standard; for multi-host or zero-trust networks, overlay encryption (e.g., WireGuard, VPC-level encryption) is recommended.

---

## 5. Secrets Management

### Required Secrets

| Secret | Purpose | Generation |
|--------|---------|------------|
| `JWT_SECRET` | Signs session JWTs | `openssl rand -base64 32` |
| `RUSTSHARE_SECRET_ENCRYPTION_KEY` | Encrypts data at rest | `openssl rand -base64 32` |
| `POSTGRES_PASSWORD` | Database access | `openssl rand -base64 24` |
| `RUSTFS_ROOT_USER` / `RUSTFS_ROOT_PASSWORD` | Object storage admin | Strong random strings |
| `STORAGE_ACCESS_KEY` / `STORAGE_SECRET_KEY` | S3 API credentials | Strong random strings |
| `OIDC_CLIENT_SECRET` | OIDC RP authentication | Provided by IdP |

### Secret Hygiene

- Run `./scripts/pre-flight.sh` before first deployment to auto-generate strong values.
- **Never** use the placeholder values from `.env.example` in production. The backend will refuse to start with known weak defaults.
- Store secrets in a vault or encrypted environment file. Do not commit `.env` to version control.
- Rotate secrets on a schedule or after personnel changes. Rotation procedures for `JWT_SECRET` and `RUSTSHARE_SECRET_ENCRYPTION_KEY` should be planned during deployment design.

---

## 6. Network Security

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

## 7. File Upload Security

| Control | Implementation |
|---------|----------------|
| Size limits | `MAX_UPLOAD_SIZE_MB` (default: 5000 MB); enforced by backend middleware. |
| Content-Type | Stored on upload but not strictly validated against file magic bytes. |
| Filename sanitization | Basic path traversal prevention; filenames are normalized before storage. |
| Virus scanning | **Out of scope.** Integrate ClamAV or a cloud scanning API via a post-upload hook if required. |
| Anonymous uploads | Allowed only via upload-only folder share links; attribution is tracked via share session IDs. |

---

## 8. Rate Limiting

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

## 9. Audit Logging

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

> **Note on denied access:** The infrastructure to log denied public share access attempts exists (`share_access_log` supports `success = false`), but handler-level integration for revoked, expired, and password-failure denials is a documented gap tracked by contract test S-11.

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

## 10. Vulnerability Reporting

If you discover a security issue, please report it responsibly:

1. **GitHub Private Security Advisories (preferred):**  
   [https://github.com/kubedoio/rustshare/security/advisories/new](https://github.com/kubedoio/rustshare/security/advisories/new)

2. **Email fallback:** `security@rustshare.io` with `[SECURITY]` in the subject.

See [SECURITY.md](../SECURITY.md) for the full policy, disclosure timeline, and supported versions.

---

## 11. Security Checklist (Pre-Deployment)

Use this checklist before exposing RustShare to the internet.

- [ ] Run `./scripts/pre-flight.sh` to generate strong secrets.
- [ ] Change **all** default passwords and keys (`JWT_SECRET`, `RUSTSHARE_SECRET_ENCRYPTION_KEY`, PostgreSQL, RustFS, S3 credentials).
- [ ] Configure TLS termination at Nginx or a load balancer.
- [ ] Set `ORIGIN` to your production domain (enables strict CSRF validation).
- [ ] Remove or firewall direct access to PostgreSQL (`5432`) and RustFS (`9000/9001`).
- [ ] Disable `PASSWORD_LOGIN_ENABLED` if you intend to use OIDC exclusively.
- [ ] Configure OIDC with a production identity provider and test the full flow.
- [ ] Verify rate-limiting behavior under load.
- [ ] Run `./scripts/backup-stack.sh` and confirm the bundle is restorable.
- [ ] Review Nginx security headers and add HSTS / CSP as appropriate.
- [ ] Enable automated dependency updates (Dependabot is already configured).
- [ ] Set up log aggregation and alerting for auth failures and replication errors.
- [ ] Document your secret rotation procedure.

---

## See Also

- [SECURITY.md](../SECURITY.md)
- [Production Readiness](PRODUCTION_READINESS.md)
- [Deployment Guide](DEPLOYMENT.md)
- [Troubleshooting](troubleshooting.md)
