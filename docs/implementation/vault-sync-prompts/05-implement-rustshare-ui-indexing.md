# Prompt 05: Implement RustShare UI and Indexing for Vault Sync

```text
You are implementing RustShare frontend and indexing support for RustShare Vault Sync.

Naming and product-positioning guardrail:
This feature must be implemented as RustShare Vault Sync with support for local Obsidian vaults. Do not call it an Obsidian Sync replacement, Obsidian Sync alternative, Obsidian Cloud, official Obsidian integration, or any phrase that implies affiliation, endorsement, certification, or replacement of Obsidian’s paid sync service. Use only public Obsidian plugin APIs and local vault files. Do not reverse engineer Obsidian Sync or use private Obsidian APIs. Do not use Obsidian logos, icons, brand colors, or commercial brand assets. Include the disclaimer: “Obsidian is a trademark of Dynalist Inc. RustShare is not affiliated with, endorsed by, or sponsored by Obsidian.”

Read first:
- SPEC-003-rustshare-storage-ui-indexing.md
- SPEC-005-naming-framing-compliance.md
- ADR-004
- ADR-006

Task:
Add UI support for synced external vaults.

Requirements:
1. Show a separate Vaults area:
   - Vaults / Obsidian / <vault-name>

2. Do not mix synced vault files into Workspace/Notes.

3. Add file metadata display:
   - Source: Vault Sync
   - Adapter: Obsidian vault
   - Last synced
   - Last device
   - Server revision

4. Add “Open in Obsidian” link for files under Obsidian vault adapter.

5. Markdown preview should preserve content and support display of:
   - headings
   - frontmatter
   - [[wikilinks]]
   - ![[embedded attachments]] where possible
   - Markdown links
   - tags

6. Search/indexing should include vault files with clear source badge.

7. Ensure filename and first H1 heading are independent in UI.

Tests:
- Vault appears separately.
- Attachments visible.
- Search finds vault Markdown.
- Preview does not rewrite source.
- UI labels follow naming guardrails.

Output:
- Files changed.
- Screenshots or UI summary if possible.
- Tests run.
- Remaining UI limitations.
```
