# Prompt 00: Master Guardrails for All Implementation Work

Use this prompt at the beginning of every LLM coding session for this feature.

```text
You are working on RustShare. Implement RustShare Vault Sync with support for local Obsidian vault folders.

Naming and product-positioning guardrail:
This feature must be implemented as RustShare Vault Sync with support for local Obsidian vaults. Do not call it an Obsidian Sync replacement, Obsidian Sync alternative, Obsidian Cloud, official Obsidian integration, or any phrase that implies affiliation, endorsement, certification, or replacement of Obsidian’s paid sync service. Use only public Obsidian plugin APIs and local vault files. Do not reverse engineer Obsidian Sync or use private Obsidian APIs. Do not use Obsidian logos, icons, brand colors, or commercial brand assets. Include the disclaimer: “Obsidian is a trademark of Dynalist Inc. RustShare is not affiliated with, endorsed by, or sponsored by Obsidian.”

Safety requirements:
- Never silently overwrite user content.
- Every upload must include base_server_rev.
- Stale writes must return or handle 409 Conflict.
- Conflicts must create conflict files.
- Deletes must use tombstones.
- Rename should be first-class where possible.
- Attachments must remain visible files.
- Markdown must be preserved byte-for-byte unless the user explicitly edits it.
- File name and first H1 heading must be independent.
- Do not store sync metadata inside Markdown bodies by default.

Execution rules:
1. Read the relevant ADR, Spec, Contract, and checklist files first.
2. Inspect the existing code before changing anything.
3. Produce a short plan and list files to change.
4. Implement only the requested phase.
5. Add or update tests.
6. Run lint/typecheck/tests if available.
7. Summarize changes, risks, and remaining gaps.
```
