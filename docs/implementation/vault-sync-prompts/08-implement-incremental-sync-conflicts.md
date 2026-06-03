# Prompt 08: Implement Incremental Sync, Rename/Delete, and Conflict Safety

```text
You are extending RustShare Vault Sync plugin from manual sync to safer incremental sync.

Naming and product-positioning guardrail:
This feature must be implemented as RustShare Vault Sync with support for local Obsidian vaults. Do not call it an Obsidian Sync replacement, Obsidian Sync alternative, Obsidian Cloud, official Obsidian integration, or any phrase that implies affiliation, endorsement, certification, or replacement of Obsidian’s paid sync service. Use only public Obsidian plugin APIs and local vault files. Do not reverse engineer Obsidian Sync or use private Obsidian APIs. Do not use Obsidian logos, icons, brand colors, or commercial brand assets. Include the disclaimer: “Obsidian is a trademark of Dynalist Inc. RustShare is not affiliated with, endorsed by, or sponsored by Obsidian.”

Read first:
- SPEC-004-sync-engine-behavior.md
- CONTRACT-003-sync-state-machine.md
- CONTRACT-004-errors-conflicts-tombstones.md
- ADR-003-sync-protocol-revisions-conflicts.md

Task:
Implement:
1. File event listeners.
2. Debounced sync queue.
3. Periodic manifest polling.
4. Local delete -> server tombstone.
5. Remote delete -> safe local delete or conflict.
6. Rename event -> server rename endpoint.
7. Offline queue and retry.
8. 409 conflict handling.
9. Binary conflict handling.
10. Sync log view or simple log file.

Rules:
- Never retry 409 as blind overwrite.
- Never delete locally modified files because of remote tombstone without conflict handling.
- Never auto-merge binary files.
- Markdown auto-merge is out of scope unless separately approved.

Tests:
- Rename local file syncs as rename.
- Remote rename applies locally.
- Local delete creates tombstone.
- Remote delete removes unchanged local file.
- Remote delete + local edit creates conflict.
- Local edit + remote edit creates conflict.
- Offline edit syncs after reconnection.

Output:
- Implementation summary.
- Sync state machine notes.
- Tests run.
- Remaining production hardening items.
```
