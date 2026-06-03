# Prompt 01: Audit Current RustShare Readiness

```text
You are a senior engineer reviewing the current RustShare codebase before implementing RustShare Vault Sync.

Naming and product-positioning guardrail:
This feature must be implemented as RustShare Vault Sync with support for local Obsidian vaults. Do not call it an Obsidian Sync replacement, Obsidian Sync alternative, Obsidian Cloud, official Obsidian integration, or any phrase that implies affiliation, endorsement, certification, or replacement of Obsidian’s paid sync service. Use only public Obsidian plugin APIs and local vault files. Do not reverse engineer Obsidian Sync or use private Obsidian APIs. Do not use Obsidian logos, icons, brand colors, or commercial brand assets. Include the disclaimer: “Obsidian is a trademark of Dynalist Inc. RustShare is not affiliated with, endorsed by, or sponsored by Obsidian.”

Task:
Perform a read-only codebase audit. Do not modify code.

Read these documents first:
- docs/adr/ADR-001-vault-sync-product-scope.md
- docs/adr/ADR-002-storage-layout-and-file-identity.md
- docs/adr/ADR-003-sync-protocol-revisions-conflicts.md
- docs/adr/ADR-004-naming-trademark-positioning-guardrails.md
- docs/adr/ADR-006-filename-heading-separation.md
- docs/specs/SPEC-001-vault-sync-api-v1.md
- docs/contracts/CONTRACT-001-vault-sync-api-openapi.yaml

Audit goals:
1. Find existing file/storage models.
2. Find existing My Files and Workspace folder logic.
3. Find Notes implementation and title/H1 coupling.
4. Find API routing conventions.
5. Find auth/permission model.
6. Find frontend file browser and editor components.
7. Find indexing/search implementation.
8. Find tests and test framework.
9. Identify where Vault Sync should integrate.
10. Identify risks before coding.

Output:
- Current architecture summary.
- Affected files/modules.
- Implementation gap list.
- Recommended implementation sequence.
- Test strategy.
- Risks and unknowns.

Do not make code changes in this step.
```
