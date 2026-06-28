# OKF-Native Notes Follow-Ups — Master Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement the child plans task-by-task. This document is the entry point; execute each child plan in the order below.

**Goal:** Close the four known gaps left after GitHub issue #118 (OKF-native Notes module) so the feature is production-ready for multi-user deployments with searchable, permission-aware notes.

**Architecture:** The work is split into four independent subsystems: (A) resolving real ACL principals for the AI index, (B) persisting the vector index, (C) exposing conflict-resolution actions in the UI, and (D) widening the OKF migration to standalone `.md` notes. Each subsystem has its own plan so it can be built, tested, and merged independently.

**Tech Stack:** Rust 1.95, Axum, PostgreSQL, SQLx, S3-compatible object storage, SvelteKit, TypeScript, Vitest.

---

## Child Plans

| # | Plan | Why it matters | Depends on |
|---|---|---|---|
| A | [ACL resolver integration](2026-06-27-okf-notes-acl-resolver-integration.md) | Search currently indexes every note with only `owner:{id}`. Group shares, direct shares, and public visibility are ignored by the RAG filter, leaking or hiding results. | None |
| B | [Persistent vector DB](2026-06-27-okf-notes-persistent-vector-db.md) | The index is in-memory and capped at 10k documents. Restarts wipe it and horizontal scaling is impossible. | Plan A (ACL shape must be stable before persisting) |
| C | [Frontend conflict UI](2026-06-27-okf-notes-frontend-conflict-ui.md) | Conflicts are only shown as a banner. Users cannot pick YAML/folder/custom resolution without editing files manually. | None |
| D | [Migration scope](2026-06-27-okf-notes-migration-scope.md) | The migration CLI only converts folder-backed bundles. Legacy single `.md` notes are skipped. | None |

## Recommended Execution Order

1. **Plan C** — smallest user-visible win, low risk.
2. **Plan A** — required before Plan B; changes the ACL contract that gets persisted.
3. **Plan B** — depends on Plan A.
4. **Plan D** — independent cleanup; can run in parallel with A/B/C.

## Cross-Cutting Conventions

- Every plan must keep `SQLX_OFFLINE=true cargo test --workspace --lib --bins` green.
- Frontend plans must keep `npm run check` and `npm run test` green.
- Do not change the OKF identity rule: `rustshare.id` is the stable identity; folder name, `title`, and H1 are display metadata.
- Keep changes minimal and focused; do not refactor unrelated code.

## Definition of Done for the Follow-Up Set

- [ ] `NoteAclPayload.read_acl` contains owner + all principals with read access.
- [ ] Search results respect shares and group membership.
- [ ] Indexed data survives server restart.
- [ ] Users can resolve note conflicts from the browser.
- [ ] Standalone legacy `.md` notes are migrated by the CLI.
- [ ] All new behavior is covered by tests.
