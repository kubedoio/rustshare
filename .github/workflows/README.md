# GitHub Actions Workflows

This directory contains the CI/CD workflows for RustShare.

## Workflows

| Workflow | File | Trigger | Purpose |
|----------|------|---------|---------|
| CI | `ci.yml` | PR/push to `main` | Format, clippy, tests, SQLx prepare check, secret scan |
| Frontend CI | `frontend-ci.yml` | PR/push to `main` (frontend paths) | Typecheck, lint, tests, production build |
| Integration Tests | `integration-tests.yml` | PR/push to `main` (backend paths) | End-to-end tests with PostgreSQL + RustFS |
| Dependencies | `dependencies.yml` | Weekly + Cargo changes | Outdated dependency report, security audit |
| Pilot Release | `pilot-release.yml` | PR/push to `main` | Build and validate pilot backend image |
| Release | `release.yml` | Tags / workflow dispatch | Build, sign, and publish release artifacts |

## Secrets

See [`docs/CI_SECRETS.md`](../../docs/CI_SECRETS.md) for the full list of
repository secrets, per-run generated credentials, and rotation guidance.

## Local validation

Install [`actionlint`](https://github.com/rhysd/actionlint) to validate workflow
syntax locally:

```bash
actionlint .github/workflows/*.yml
```

Run the same secret scanner used in CI:

```bash
./scripts/secret-scan.sh
```
