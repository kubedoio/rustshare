# RustShare Vault Sync with Obsidian Vault Support — Document Pack

This ZIP contains ADRs, specs, contracts, implementation prompts, and checklists for implementing **RustShare Vault Sync** with support for local Obsidian vault folders.

The feature must be implemented as a generic RustShare vault synchronization capability with an Obsidian vault adapter. It must not be positioned as an official Obsidian product, an Obsidian cloud service, or a replacement for Obsidian’s paid sync service.

> Obsidian is a trademark of Dynalist Inc. RustShare is not affiliated with, endorsed by, or sponsored by Obsidian.

## Important Naming Correction

Earlier drafts may have used `/api/obsidian-sync/v1`. This pack intentionally changes the preferred API namespace to:

```text
/api/vault-sync/v1
```

Obsidian-specific behavior is represented as an adapter:

```json
{
  "adapter": "obsidian_vault"
}
```

This keeps the architecture generic, safer, and future-proof.

## ZIP Contents

```text
adr/
  ADR-001-vault-sync-product-scope.md
  ADR-002-storage-layout-and-file-identity.md
  ADR-003-sync-protocol-revisions-conflicts.md
  ADR-004-naming-trademark-positioning-guardrails.md
  ADR-005-security-auth-device-management.md
  ADR-006-filename-heading-separation.md

spec/
  SPEC-001-vault-sync-api-v1.md
  SPEC-002-obsidian-vault-adapter-and-plugin-mvp.md
  SPEC-003-rustshare-storage-ui-indexing.md
  SPEC-004-sync-engine-behavior.md
  SPEC-005-naming-framing-compliance.md

contracts/
  CONTRACT-001-vault-sync-api-openapi.yaml
  CONTRACT-002-data-models-and-schemas.md
  CONTRACT-003-sync-state-machine.md
  CONTRACT-004-errors-conflicts-tombstones.md

prompts/
  00-master-guardrails.md
  01-audit-current-rustshare-readiness.md
  02-add-adr-spec-contracts-to-repo.md
  03-implement-rustshare-storage-and-metadata.md
  04-implement-vault-sync-api.md
  05-implement-rustshare-ui-indexing.md
  06-create-obsidian-plugin-skeleton.md
  07-implement-manual-sync-mvp.md
  08-implement-incremental-sync-conflicts.md
  09-testing-hardening-release.md

checklists/
  ACCEPTANCE_CRITERIA.md
  TEST_PLAN.md
  TERMINOLOGY_BLOCKLIST.md
```

## Recommended LLM Execution Order

### Phase 0 — Read-only analysis

Run:

```text
prompts/00-master-guardrails.md
prompts/01-audit-current-rustshare-readiness.md
```

Goal: make your LLM inspect the current RustShare codebase without changing code. It should produce a gap report, list affected modules, and identify existing Files/Notes/Workspace APIs.

Do not allow coding in this phase.

### Phase 1 — Put decisions into the repository

Run:

```text
prompts/02-add-adr-spec-contracts-to-repo.md
```

Goal: add the ADRs, specs, and contracts into the RustShare repository before implementation starts.

Expected result:

```text
docs/adr/
docs/specs/
docs/contracts/
```

### Phase 2 — RustShare backend foundation

Run:

```text
prompts/03-implement-rustshare-storage-and-metadata.md
prompts/04-implement-vault-sync-api.md
```

Goal: implement server-side support first.

Required before plugin work:

```text
- My Files/Vaults/Obsidian/<vault-name> or My Files/Obsidian/<vault-name>
- source_type / adapter_type metadata
- manifest endpoint
- upload/download/delete/rename endpoints
- server revisions
- stale upload conflict response
```

### Phase 3 — RustShare UI and indexing

Run:

```text
prompts/05-implement-rustshare-ui-indexing.md
```

Goal: make synced vaults visible and usable in RustShare without mixing them into internal RustShare Notes.

### Phase 4 — Obsidian plugin MVP

Run in a separate plugin repository/package:

```text
prompts/06-create-obsidian-plugin-skeleton.md
prompts/07-implement-manual-sync-mvp.md
```

Goal: build a desktop-first plugin with login/settings, vault mapping, manual sync, upload/download, and basic status display.

### Phase 5 — Incremental sync and conflict safety

Run:

```text
prompts/08-implement-incremental-sync-conflicts.md
```

Goal: add event-based sync, debounce queue, tombstones, rename support, conflict files, and offline retry.

### Phase 6 — Test and release hardening

Run:

```text
prompts/09-testing-hardening-release.md
```

Goal: enforce acceptance criteria, integration tests, conflict tests, UI tests, terminology review, and release checklist.

## How to Guide Your LLM

Use this pattern for every step:

```text
1. Read the relevant ADR, Spec, Contract, and checklist files.
2. Inspect the existing implementation.
3. Produce a short implementation plan.
4. List files to change.
5. Implement only this phase.
6. Add or update tests.
7. Run tests/lint/typecheck.
8. Summarize changes, risks, and remaining work.
```

Do not let the LLM implement backend and plugin together in one prompt. That is too broad and increases data-loss risk.

## Mandatory Quality Gates

Before each merge:

```text
- No forbidden naming/framing terms in UI or docs, except in the blocklist/spec sections.
- No use of Obsidian logo, icon, or brand styling.
- No private or reverse-engineered Obsidian APIs.
- Filename and first H1 heading are independent.
- Uploads require base_server_rev.
- Stale uploads return 409 Conflict.
- Conflicts create conflict files instead of overwriting user content.
- Attachments remain visible files.
- Delete operations use tombstones.
- Rename is first-class where possible.
- All Markdown is preserved byte-for-byte unless the user edits it.
```

## Recommended First Milestone

Do not start with “full compatibility.” Start with:

```text
Safe manual sync of one local Obsidian vault into RustShare Vault Sync storage.
```

That means:

```text
Obsidian local vault
  -> RustShare Vault Sync plugin
  -> /api/vault-sync/v1
  -> My Files/Vaults/Obsidian/<vault-name>
```
