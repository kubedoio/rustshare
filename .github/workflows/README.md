# GitHub Actions Workflows

This directory contains the CI/CD workflows for RustShare.

## Workflows

| Workflow | File | Trigger | Purpose |
|----------|------|---------|---------|
| CI | `ci.yml` | PR/push to `main`; manual dispatch | PR format, clippy, Cargo Deny, and compose checks; tests, release build, SQLx prepare, and coverage on push or manual dispatch |
| Frontend CI | `frontend-ci.yml` | PR/push to `main` (frontend paths) | Typecheck, lint, tests, production build |
| Integration Tests | `integration-tests.yml` | Push to `main` (backend paths); manual dispatch | End-to-end tests with PostgreSQL + RustFS |
| Dependencies | `dependencies.yml` | Weekly + frontend package changes | Outdated npm dependency report, npm security audit (Rust advisories covered by `cargo deny` in `ci.yml`) |
| Pilot Release | `pilot-release.yml` | Push to `main`; manual dispatch | Build and validate pilot backend image |
| Release | `release.yml` | Tags / workflow dispatch | Build, sign, and publish release artifacts |

To run the expensive CI or integration workers for a PR, open the workflow in
Actions, choose **Run workflow**, and select the PR's head branch. These workers
also run automatically after changes reach `main`.

## Secrets

See [`docs/CI_SECRETS.md`](../../docs/CI_SECRETS.md) for the full list of
repository secrets, per-run generated credentials, and rotation guidance.

## Local validation

Install [`actionlint`](https://github.com/rhysd/actionlint) to validate workflow
syntax locally:

```bash
actionlint .github/workflows/*.yml
```

Secret scanning is handled by GitHub Advanced Security and repository-level settings.
