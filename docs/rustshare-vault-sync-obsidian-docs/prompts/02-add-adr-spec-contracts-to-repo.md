# Prompt 02: Add ADR, Spec, and Contract Documents to Repository

```text
You are adding the RustShare Vault Sync design documents to the repository.

Naming and product-positioning guardrail:
This feature must be implemented as RustShare Vault Sync with support for local Obsidian vaults. Do not call it an Obsidian Sync replacement, Obsidian Sync alternative, Obsidian Cloud, official Obsidian integration, or any phrase that implies affiliation, endorsement, certification, or replacement of Obsidian’s paid sync service. Use only public Obsidian plugin APIs and local vault files. Do not reverse engineer Obsidian Sync or use private Obsidian APIs. Do not use Obsidian logos, icons, brand colors, or commercial brand assets. Include the disclaimer: “Obsidian is a trademark of Dynalist Inc. RustShare is not affiliated with, endorsed by, or sponsored by Obsidian.”

Task:
Create or update the following folders:
- docs/adr/
- docs/specs/
- docs/contracts/
- docs/checklists/

Add the ADR, Spec, Contract, and checklist files from the provided document pack.

Requirements:
- Preserve the naming guardrails exactly.
- Use /api/vault-sync/v1 as the API namespace.
- Use adapter = "obsidian_vault" for Obsidian vault support.
- Include the non-affiliation disclaimer in public-facing docs.
- Do not create implementation code yet.

Output:
- List of files added.
- Confirmation that forbidden naming is only present in compliance/blocklist contexts.
- Any repo documentation index files updated.
```
