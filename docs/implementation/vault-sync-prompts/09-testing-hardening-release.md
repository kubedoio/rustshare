# Prompt 09: Testing, Hardening, and Release Checklist

```text
You are preparing RustShare Vault Sync for internal beta.

Naming and product-positioning guardrail:
This feature must be implemented as RustShare Vault Sync with support for local Obsidian vaults. Do not call it an Obsidian Sync replacement, Obsidian Sync alternative, Obsidian Cloud, official Obsidian integration, or any phrase that implies affiliation, endorsement, certification, or replacement of Obsidian’s paid sync service. Use only public Obsidian plugin APIs and local vault files. Do not reverse engineer Obsidian Sync or use private Obsidian APIs. Do not use Obsidian logos, icons, brand colors, or commercial brand assets. Include the disclaimer: “Obsidian is a trademark of Dynalist Inc. RustShare is not affiliated with, endorsed by, or sponsored by Obsidian.”

Read first:
- checklists/ACCEPTANCE_CRITERIA.md
- checklists/TEST_PLAN.md
- checklists/TERMINOLOGY_BLOCKLIST.md
- all ADR/SPEC/CONTRACT files

Task:
Run a full readiness review.

Backend tests:
- vault create/list/get
- manifest
- upload/download
- stale upload 409
- delete tombstone
- rename
- path traversal rejection
- tenant isolation
- token scope enforcement

Plugin tests:
- manual sync
- attachments
- ignored paths
- local/remote conflict
- delete conflict
- rename
- offline retry
- token invalid/expired

UI tests:
- Vaults section visible
- vault files separate from Workspace Notes
- attachment visibility
- Markdown preview safety
- source badges
- “Open in Obsidian” link

Compliance tests:
- forbidden terminology scan
- disclaimer present
- no Obsidian logo/assets
- API namespace is /api/vault-sync/v1

Output:
- Pass/fail checklist.
- Bugs found.
- Severity ranking.
- Required fixes before beta.
- Release notes draft using approved terminology only.
```
