# Prompt 03: Implement RustShare Storage and Metadata Foundation

```text
You are implementing the RustShare backend storage foundation for RustShare Vault Sync.

Naming and product-positioning guardrail:
This feature must be implemented as RustShare Vault Sync with support for local Obsidian vaults. Do not call it an Obsidian Sync replacement, Obsidian Sync alternative, Obsidian Cloud, official Obsidian integration, or any phrase that implies affiliation, endorsement, certification, or replacement of Obsidian’s paid sync service. Use only public Obsidian plugin APIs and local vault files. Do not reverse engineer Obsidian Sync or use private Obsidian APIs. Do not use Obsidian logos, icons, brand colors, or commercial brand assets. Include the disclaimer: “Obsidian is a trademark of Dynalist Inc. RustShare is not affiliated with, endorsed by, or sponsored by Obsidian.”

Read first:
- ADR-001
- ADR-002
- ADR-003
- ADR-006
- SPEC-001
- CONTRACT-002

Task:
Implement the backend storage and metadata model for external vaults.

Requirements:
1. Add vault metadata model:
   - vault_id
   - tenant_id
   - owner_user_id
   - name
   - adapter = "obsidian_vault"
   - root_path
   - server_rev
   - timestamps

2. Add file metadata model:
   - file_id
   - vault_id
   - relative_path
   - content_type
   - sha256
   - size
   - server_rev
   - mtime_server
   - deleted/deleted_at
   - last_writer_device_id

3. Create storage layout:
   Preferred: My Files/Vaults/Obsidian/<vault-name>/
   Acceptable if required by existing code: My Files/Obsidian/<vault-name>/

4. Ensure vault files are not placed under Workspace/Notes.

5. Ensure filename and first Markdown H1 are independent.

6. Keep attachments as visible files.

Tests:
- Can create vault metadata.
- Can create file metadata.
- Relative paths are preserved.
- Unicode filenames are preserved.
- H1 change does not rename file.
- File rename does not rewrite H1.

Output:
- Files changed.
- Migration notes if database changes are needed.
- Tests added.
- Known limitations.
```
