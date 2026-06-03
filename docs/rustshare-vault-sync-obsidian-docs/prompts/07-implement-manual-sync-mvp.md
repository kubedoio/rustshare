# Prompt 07: Implement Manual Sync MVP in Obsidian Plugin

```text
You are implementing the MVP manual sync behavior in the RustShare Vault Sync plugin.

Naming and product-positioning guardrail:
This feature must be implemented as RustShare Vault Sync with support for local Obsidian vaults. Do not call it an Obsidian Sync replacement, Obsidian Sync alternative, Obsidian Cloud, official Obsidian integration, or any phrase that implies affiliation, endorsement, certification, or replacement of Obsidian’s paid sync service. Use only public Obsidian plugin APIs and local vault files. Do not reverse engineer Obsidian Sync or use private Obsidian APIs. Do not use Obsidian logos, icons, brand colors, or commercial brand assets. Include the disclaimer: “Obsidian is a trademark of Dynalist Inc. RustShare is not affiliated with, endorsed by, or sponsored by Obsidian.”

Read first:
- SPEC-002-obsidian-vault-adapter-and-plugin-mvp.md
- SPEC-004-sync-engine-behavior.md
- CONTRACT-001-vault-sync-api-openapi.yaml
- CONTRACT-002-data-models-and-schemas.md

Task:
Implement manual sync.

Requirements:
1. Connect to RustShare URL.
2. Register or map current local vault.
3. Scan local vault files.
4. Ignore default sensitive paths.
5. Compute sha256 for files.
6. Fetch remote manifest.
7. Compare local files and remote manifest.
8. Upload local-only files.
9. Download remote-only files.
10. Update local sync state only after confirmed operations.
11. Show sync result.
12. Preserve Markdown byte-for-byte.
13. Preserve attachments as files.
14. Do not rewrite filenames or headings.

Conflict MVP:
- If local changed and remote changed, create conflict file.
- Do not attempt automatic merge.

Tests:
- Local .md uploads.
- Local attachment uploads.
- Remote .md downloads.
- Remote attachment downloads.
- Ignored paths are skipped.
- Conflict creates conflict file.
- Local sync state updates correctly.

Output:
- Implementation summary.
- Manual test instructions.
- Known limitations.
```
