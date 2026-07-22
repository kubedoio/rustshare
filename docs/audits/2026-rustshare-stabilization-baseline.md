# RustShare Stabilization Baseline

Created as part of Phase 0 of the 2026 stabilization directive.

## Environment

| Item | Value |
|------|-------|
| Date | 2026-07-22 |
| Commit SHA | `8fb4ba059fa043887807bd7178096bc60e2011be` |
| OS | Linux rustshare 6.8.0-124-generic #124-Ubuntu SMP PREEMPT_DYNAMIC x86_64 |
| CPU | AMD EPYC 7401P 24-Core Processor, 16 logical cores visible |
| RAM | 15 GiB (13 GiB available at start) |
| Rust | rustc 1.95.0 (59807616e 2026-04-14), cargo 1.95.0 (f2d3ce0bd 2026-03-21) |
| Node.js | v22.22.2 |
| npm | 10.9.7 |
| hyperfine | not installed; using shell `time` |

## Workspace Structure

- Root workspace: `Cargo.toml` + `Cargo.lock`
- Backend workspace: `backend/Cargo.toml` + `backend/Cargo.lock`
- Both lockfiles present at baseline.

## Measurements

Commands were run from a `cargo clean` state where applicable. Timings recorded below as they complete.

### Rust workspace (root)

| Command | Exit code | Elapsed |
|---------|-----------|---------|
| `cargo clean` | 0 | 2 s |
| `cargo build --timings --workspace` | 0 | 190 s (3 m 10 s) |
| `cargo check --timings --workspace` | 0 | 78 s |
| `cargo test --workspace --lib --no-run` | 0 | 64 s |

### Rust workspace (backend, from `backend/`)

These commands exist because the repository currently has a separate backend workspace. They are recorded for comparison and will be removed after consolidation.

| Command | Exit code | Elapsed |
|---------|-----------|---------|
| `cargo build --timings` | 0 | 27 s (warm, after root build) |
| `cargo check --timings` | 0 | 19 s (warm) |
| `cargo test --all-features --lib --no-run` | 0 | 14 s (warm) |
| `cargo test --all-features --no-run` | 0 | 59 s (warm) |

### Frontend

| Command | Exit code | Elapsed |
|---------|-----------|---------|
| `npm ci` | 0 | 22 s |
| `npm run check` | 0 | 27 s |
| `npm run lint` | 0 | 103 s |
| `npm run test` | 0 | 65 s |
| `npm run build` | 0 | 23 s |

### Frontend production bundle sizes

| Metric | Value |
|--------|-------|
| Total client JS (uncompressed, all `_app/immutable` `.js` files) | ~8.55 MB |
| `frontend/build` total size | 24 MB |

### CI job structure (summary)

- `ci.yml` runs on every PR/push to `main` with no path filters.
- Rust compiling jobs: `clippy`, `test`, `build-release`, `root-workspace`, `sqlx-check`, `coverage`.
- Each DB-requiring job installs `sqlx-cli` and runs migrations independently.
- `integration-tests.yml` runs ignored backend integration tests when backend/vault paths change.
- `dependencies.yml` runs dependency checks on Cargo file changes.

See `docs/audits/2026-rustshare-bug-inventory.md` for full duplication analysis.

