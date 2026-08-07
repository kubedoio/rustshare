# Security Incident Runbook

> **Audience:** Operators and security responders  
> **Scope:** RustShare self-hosted Docker Compose deployments  
> **Goal:** Contain security incidents, rotate exposed credentials, revoke unauthorized sessions, and isolate affected tenants.

---

## 1. Incident Severity Levels

| Level | Examples | Response Time |
|-------|----------|---------------|
| **Critical** | Compromised `JWT_SECRET`, `RUSTSHARE_SECRET_ENCRYPTION_KEY`, host root access, full database exfiltration | Immediate |
| **High** | Exposed admin password, leaked webhook secret, unauthorized tenant data access | Within 1 hour |
| **Medium** | Suspicious login patterns, individual user account compromise | Within 4 hours |
| **Low** | Lost device, expired token cleanup | Within 24 hours |

---

## 2. Immediate Containment Checklist

1. **Stop the bleeding**
   - Rotate any secret suspected to be exposed (see [Secret Rotation](#3-secret-rotation)).
   - Revoke active sessions for affected users (see [Session Revocation](#4-session-revocation)).
   - Disable compromised accounts (see [Isolate a Tenant or User](#5-isolate-a-tenant-or-user)).

2. **Preserve evidence**
   - Capture application logs from the backend and Nginx containers.
   - Export relevant rows from `user_security_events`, `share_access_log`, and `admin_actions`.
   - Do not delete containers or volumes until the incident is understood.

3. **Verify containment**
   - Confirm rotated secrets are in use: restart the backend and check `/health/ready`.
   - Confirm revoked sessions return `401 Unauthorized`.
   - Confirm disabled users cannot log in.

4. **Communicate**
   - Notify affected tenants/users according to your incident-response policy.
   - Document the timeline and actions taken.

---

## 3. Secret Rotation

### 3.1 Rotate `JWT_SECRET`

**Impact:** All existing browser and API sessions are invalidated. Users must log in again.

1. Generate a new secret:
   ```bash
   openssl rand -base64 32
   ```
2. Update `JWT_SECRET` in `.env`.
3. Restart the backend:
   ```bash
   docker compose up -d --force-recreate backend
   ```
4. Verify `/health/ready` returns `200`.
5. Instruct users to log in again.

### 3.2 Rotate `RUSTSHARE_SECRET_ENCRYPTION_KEY`

**Impact:** Data encrypted with the old key cannot be decrypted without it. **Do not discard the old key until all data is re-encrypted.**

1. Generate a new key:
   ```bash
   openssl rand -base64 32
   ```
2. Add the new key to `.env` as `RUSTSHARE_SECRET_ENCRYPTION_KEY`.
3. Keep the old key available (e.g., `RUSTSHARE_SECRET_ENCRYPTION_KEY_PREVIOUS`) for decryption.
4. Re-encrypt existing data using a maintenance script or planned re-encryption job.
5. After re-encryption is verified, remove the old key from `.env`.
6. Restart the backend.

> There is no automated re-encryption job yet. Plan a maintenance window and write a one-off script if the volume of encrypted data is large.

### 3.3 Rotate PostgreSQL Password

1. Generate a new password:
   ```bash
   openssl rand -hex 32
   ```
2. Update `POSTGRES_PASSWORD` and `DATABASE_URL` in `.env`.
3. Update the password in PostgreSQL:
   ```bash
   docker compose exec postgres psql -U rustshare -c "ALTER USER rustshare WITH PASSWORD '<new-password>';"
   ```
4. Restart the stack:
   ```bash
   docker compose up -d --force-recreate
   ```
5. Verify `/health/ready`.

### 3.4 Rotate RustFS / S3 Credentials

1. Generate a new access key and secret:
   ```bash
   openssl rand -hex 32
   ```
2. Update RustFS root credentials via the RustFS console or `mc` CLI.
3. Update `RUSTFS_ROOT_USER`, `RUSTFS_ROOT_PASSWORD`, `AWS_ACCESS_KEY_ID`, and `AWS_SECRET_ACCESS_KEY` in `.env`.
4. Restart the stack.
5. Verify upload and download work.

### 3.5 Rotate `OIDC_CLIENT_SECRET`

1. Rotate the secret in your identity provider.
2. Update `OIDC_CLIENT_SECRET` in `.env`.
3. Restart the backend.
4. Test the OIDC login flow end-to-end.

### 3.6 Rotate `RUSTSHARE_CHAT_WEBHOOK_SECRET`

1. Generate a new secret:
   ```bash
   openssl rand -base64 32
   ```
2. Update `RUSTSHARE_CHAT_WEBHOOK_SECRET` in `.env`.
3. Update the secret in the remote chat integration (e.g., Mattermost, Slack, Teams).
4. Re-register webhook URLs if needed:
   ```bash
   curl -X POST https://<your-domain>/api/v1/admin/integrations/chat/webhooks \
     -H "Authorization: Bearer <admin-token>" \
     -H "Content-Type: application/json" \
     -d '{"url": "https://chat.example.com/webhook"}'
   ```
5. Restart the backend.

### 3.7 Rotate `METRICS_API_TOKEN`

1. Generate a new token:
   ```bash
   openssl rand -base64 32
   ```
2. Update `METRICS_API_TOKEN` in `.env`.
3. Update your Prometheus scrape configuration.
4. Restart the backend.

---

## 4. Session Revocation

### 4.1 User Self-Revocation

Authenticated users can list and revoke their own browser sessions:

```bash
# List sessions
curl -H "Cookie: rustshare_session=<session-cookie>" \
  https://<your-domain>/api/v1/me/sessions

# Revoke a session
curl -X DELETE \
  -H "Cookie: rustshare_session=<session-cookie>" \
  -H "X-Rustshare-Csrf: <csrf-token>" \
  https://<your-domain>/api/v1/me/sessions/<session-id>
```

> Users cannot revoke their current session through this endpoint; they must use **Sign Out** instead.

### 4.2 Admin Revocation by Disabling a User

Disabling a user immediately terminates all browser sessions and revokes all device tokens:

```bash
curl -X POST \
  -H "Authorization: Bearer <admin-token>" \
  https://<your-domain>/api/v1/admin/users/<user-id>/disable
```

### 4.3 Mass Session Invalidation via `JWT_SECRET` Rotation

Rotating `JWT_SECRET` invalidates all bearer-token and cookie-based sessions. See [Rotate `JWT_SECRET`](#31-rotate-jwt_secret).

---

## 5. Isolate a Tenant or User

### 5.1 Suspend a Compromised User

1. Disable the user (see [Admin Revocation](#42-admin-revocation-by-disabling-a-user)).
2. Review the user's recent activity in `user_security_events` and `admin_actions`.
3. Revoke any public shares created by the user:
   ```sql
   UPDATE shares SET revoked_at = NOW() WHERE created_by = '<user-id>';
   ```
4. Notify the user and your security team.

### 5.2 Contain a Cross-Tenant Access Issue

If you suspect a tenant boundary has been crossed:

1. Identify the affected tenant(s) from `user_security_events` and `share_access_log`.
2. Rotate `JWT_SECRET` to force global re-authentication.
3. Review public share tokens for the affected tenant(s):
   ```sql
   SELECT id, share_token, file_id, folder_id, created_by, created_at
   FROM shares
   WHERE tenant_id = '<tenant-id>' AND revoked_at IS NULL;
   ```
4. Revoke suspicious shares:
   ```sql
   UPDATE shares SET revoked_at = NOW() WHERE id = '<share-id>';
   ```
5. Disable compromised users.
6. Rotate `RUSTSHARE_CHAT_WEBHOOK_SECRET` if chat-integration events may have crossed tenants.
7. Open a follow-up issue to review repository `tenant_id` filtering for the affected feature area.

### 5.3 Compromised Webhook Endpoint

If an external chat webhook endpoint is compromised:

1. Rotate `RUSTSHARE_CHAT_WEBHOOK_SECRET` immediately.
2. List registered webhooks:
   ```bash
   curl -H "Authorization: Bearer <admin-token>" \
     https://<your-domain>/api/v1/admin/integrations/chat/webhooks
   ```
3. Remove untrusted URLs.
4. Re-register only trusted HTTPS endpoints.
5. Review recent `share_access_log` and `user_security_events` for unauthorized activity.

---

## 6. Post-Incident Verification

After containment:

- [ ] Rotated secrets are active and the stack is healthy (`/health/ready`).
- [ ] Revoked sessions return `401`.
- [ ] Disabled users cannot authenticate.
- [ ] Suspicious shares are revoked.
- [ ] Application logs show no further unauthorized access.
- [ ] A backup was taken after the incident for forensic preservation.
- [ ] A restore drill passes:
  ```bash
  ./scripts/run-restore-drill.sh /mnt/backups/rustshare/<latest>
  ```

---

## 7. Reporting

If the incident may affect users outside your organization, report it responsibly:

1. **GitHub Private Security Advisories:**  
   [https://github.com/kubedoio/rustshare/security/advisories/new](https://github.com/kubedoio/rustshare/security/advisories/new)

2. **Email fallback:** `security@rustshare.io` with `[SECURITY]` in the subject.

See [SECURITY.md](../../SECURITY.md) for the full policy.

---

## See Also

- [Security Model](../security-model.md)
- [Backup/Restore Runbook](backup-restore.md)
- [Deployment Guide](../DEPLOYMENT.md)
- [CI/CD Secrets Reference](../CI_SECRETS.md)
- [Production Readiness](../PRODUCTION_READINESS.md)
