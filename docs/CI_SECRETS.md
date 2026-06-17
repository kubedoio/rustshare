# CI/CD Secrets Reference

This document lists the secrets and environment variables used by RustShare's
GitHub Actions workflows, explains whether they should be configured as
repository secrets or generated per-run, and provides rotation guidance.

> Repository administrators can configure secrets at **Settings → Secrets and
> variables → Actions**.

---

## Repository secrets (optional but recommended)

These secrets are used by workflows when they are set. If they are not set, the
workflows generate equivalent per-run values automatically. Setting them is
recommended if you want deterministic test credentials or if your organization
requires all secrets to come from the GitHub secret store.

| Secret | Used by | Purpose | Rotation |
|--------|---------|---------|----------|
| `CI_JWT_SECRET` | `pilot-release.yml` | JWT signing key for pilot smoke-test stack | Rotate on suspected compromise or quarterly. |
| `CI_ENCRYPTION_KEY` | `pilot-release.yml` | Encryption key for sensitive data in pilot stack | Rotate with `CI_JWT_SECRET`. |

These secrets are base64-encoded 32-byte values. Generate them with:

```bash
openssl rand -base64 32
```

---

## Per-run generated secrets

The following credentials are generated fresh for every workflow run and never
leave the ephemeral runner. They do **not** need to be configured as repository
secrets, but they must remain dynamic in the workflow files.

| Variable | Generated in | Purpose |
|----------|--------------|---------|
| `JWT_SECRET` | `integration-tests.yml` | JWT signing key for integration tests |
| `RUSTSHARE_SECRET_ENCRYPTION_KEY` | `integration-tests.yml` | Encryption key for integration tests |
| `RUSTSHARE_ADMIN_PASSWORD` | `integration-tests.yml` | Bootstrap admin password for integration tests |
| `RUSTFS_ROOT_USER` / `RUSTFS_ROOT_PASSWORD` | `integration-tests.yml` | RustFS root credentials for integration tests |
| `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` | `integration-tests.yml` | S3-compatible credentials (matches RustFS root credentials) |
| `POSTGRES_PASSWORD` | `ci.yml`, `dependencies.yml`, `integration-tests.yml` | Ephemeral PostgreSQL password |

Generation uses `openssl rand` or run-scoped GitHub context expressions
(`github.run_id`, `github.run_attempt`, `github.job`). Do not replace these
with hardcoded values.

---

## GitHub token

| Secret | Used by | Purpose |
|--------|---------|---------|
| `GITHUB_TOKEN` | `pilot-release.yml` | Authenticate to GitHub Container Registry for image publish |

`GITHUB_TOKEN` is automatically provided by GitHub Actions. No manual rotation
is required; it is scoped to the workflow run.

---

## Dev-only / non-secret configuration

The following are **not secrets**. They are local-development defaults or
public configuration values and may remain as plain text in workflow files.

| Variable | Example | Notes |
|----------|---------|-------|
| `DATABASE_URL` | `postgres://rustshare:...` | Contains a per-run generated password |
| `RUSTFS_ENDPOINT` | `http://127.0.0.1:9000` | Local service endpoint |
| `RUSTFS_PUBLIC_ENDPOINT` | `http://127.0.0.1:9000` | Local service endpoint |
| `RUSTFS_REGION` | `us-east-1` | Region label for RustFS |
| `RUSTFS_BUCKET` | `rustshare-files` | Bucket name |
| `RUSTSHARE_ADMIN_EMAIL` | `admin@localhost` | Default admin email in local dev |
| `RUSTSHARE_ADMIN_USERNAME` | `admin` | Default admin username in local dev |
| `PASSWORD_LOGIN_ENABLED` | `true` | Feature flag |

---

## Rotation guidance

1. **Repository secrets** (`CI_JWT_SECRET`, `CI_ENCRYPTION_KEY`):
   - Generate a new value with `openssl rand -base64 32`.
   - Update the secret in **Settings → Secrets and variables → Actions**.
   - Re-run the most recent workflow on `main` to confirm it still passes.
   - Delete the old secret value from any local copies or notes.

2. **Per-run generated secrets**:
   - No rotation needed; they are unique to each workflow run and discarded
     when the runner is cleaned up.

3. **Production secrets** (see `.env.example` and `docs/DEPLOYMENT.md`):
   - Rotate `JWT_SECRET`, `RUSTSHARE_SECRET_ENCRYPTION_KEY`, PostgreSQL
     password, and RustFS/S3 credentials before any production deployment.
   - Use `scripts/pre-flight.sh` to generate a fresh set of production secrets.

---

## Security checks

- The `secret-scan` job in `ci.yml` runs `scripts/secret-scan.sh` on every PR
  and push to `main`. It blocks merges if it detects hardcoded secrets in
  workflow files, Docker Compose files, environment examples, or shell scripts.
- Do **not** add real secrets to test fixtures or documentation. Use
  deterministic fake values and add them to `.secret-scan-allowlist` if the
  scanner flags them.
