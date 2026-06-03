# Acceptance Criteria

> **Status:** Updated 2026-06-03 after industrial-grade review and remediation.

## Product and Naming

```text
- Feature is called RustShare Vault Sync.
- Obsidian is described only as local vault support/adapter/connector.
- Public documentation includes disclaimer.
- No forbidden customer-facing terminology is used.
```

Required disclaimer:

```text
Obsidian is a trademark of Dynalist Inc. RustShare is not affiliated with, endorsed by, or sponsored by Obsidian.
```

✅ **Verified:** Disclaimer present in `manifest.json`, all source files, and README.

---

## Storage

```text
- Vault files are stored outside Workspace/Notes.
- Preferred path: My Files/Vaults/Obsidian/<vault-name>.
- Attachments are visible files.
- Markdown is preserved byte-for-byte.
- Sync metadata is not injected into Markdown.
```

✅ **Implemented:** Content-addressed blob storage at `blobs/{sha256}`. Files stored via `ObjectStoreOps` trait.

---

## Filename / H1

```text
- Filename and first H1 are independent.
- Changing H1 does not rename file.
- Renaming file does not rewrite H1.
```

✅ **Implemented:** Path is preserved byte-for-word; no Markdown body rewriting.

---

## API

```text
- Namespace is /api/vault-sync/v1.
- Obsidian is represented as adapter = "obsidian_vault".
- All writes require base_server_rev.
- Stale writes return 409 Conflict.
- Delete creates tombstone.
- Rename is first-class.
- Manifest includes active files and tombstones.
```

✅ **Implemented:** All 9 endpoints operational. 409 returns structured JSON with `client_rev`, `current_rev`, `server_sha256`.

---

## Plugin

```text
- Plugin connects to RustShare.
- Plugin maps/creates vault.
- Plugin scans local vault.
- Plugin uploads Markdown and attachments.
- Plugin downloads Markdown and attachments.
- Plugin ignores sensitive paths by default.
- Plugin creates conflict files instead of overwriting.
```

✅ **Implemented:** All features present in `apps/obsidian-vault-sync/`. 64 unit tests passing.

---

## Security

```text
- Plugin does not store user password.
- Tokens are scoped.
- Tenant/user/vault authorization is enforced.
- Device ID is recorded.
- Audit events are emitted.
```

✅ **Implemented:** Token-based auth. Device registration with UUID. Tenant isolation via query binding.

⚠️ **Open gap:** Token generation UI does not yet exist in RustShare frontend. Users cannot self-service generate tokens.

---

## Beta Exit Criteria

```text
- Manual sync works reliably on a real vault copy.
- Incremental sync works for create/update/delete/rename.
- Conflict tests pass.
- No data-loss bug is open.
- Terminology scan passes.
- Internal documentation is complete.
```

| Criterion | Status | Notes |
|---|---|---|
| Manual sync on real vault | ⚠️ **Not yet tested** | Code complete; needs e2e smoke test with live backend |
| Incremental sync C/U/D/R | ⚠️ **Not yet tested** | Event listeners wired; needs live Obsidian + backend test |
| Conflict tests pass | ✅ **Passing** | 12 sync-engine tests including conflict scenarios |
| No data-loss bug open | ✅ **Clear** | 3 industrial-grade review passes, all critical findings fixed |
| Terminology scan passes | ✅ **Passing** | No forbidden terms in any customer-facing file |
| Internal documentation complete | ✅ **Complete** | ADRs, specs, contracts, checklists, execution plan all present |

---

## Known Limitations (MVP)

1. **No web UI for browsing vaults** — Phase 3 deferred to post-MVP. Plugin works standalone.
2. **No plugin marketplace submission** — Manual installation only.
3. **No token generation UI** — Requires frontend work in main RustShare app.
4. **Desktop only** — `isDesktopOnly: true` in manifest. Mobile support is post-MVP.
5. **Manifest capped at 10,000 entries** — Pagination via `?since_rev=` is documented as future work.
6. **In-memory rate limiting** — Multi-instance deployments need Redis-backed rate limiting.
7. **No background GC for orphaned blobs** — Documented as accepted trade-off; content-addressed deduplication minimizes impact.

---

## Industrial-Grade Review Summary

**Reviews conducted:** 3 passes
- Pass 1: Initial security & architecture review (20 findings)
- Pass 2: Phase 7 remediation (all findings fixed, validated, committed)
- Pass 3: Fresh re-review with subagents (29 additional findings across backend + plugin)

**All critical findings fixed:**
- Infinite conflict-copy loops (C5/C6)
- Manual/incremental sync race condition (C1)
- Device registration auth error hiding (C3)
- Non-network errors dropping operations forever (C11)
- Retry-After NaN causing immediate retry hammering (R8)
- Downloaded content not SHA-256 verified (S3)
- Device revocation race returning 500 instead of 403 (S1.2)
- Over-broad Database error catch mapping connection errors to 409 (C1.1)
- Rename check-then-act race condition (C2.1)
- Orphaned blob behavior documented with GC TODO (C1)

**Commits:**
- `7de7dd0` — Initial remediation
- `24462ce` — Critical plugin sync bugs
- `6e2d9fa` — Plugin reliability & state hygiene
- `138a5c9` — Backend correctness & security
