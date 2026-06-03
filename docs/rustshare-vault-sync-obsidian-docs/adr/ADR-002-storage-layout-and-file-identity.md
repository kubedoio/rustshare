# ADR-002: Storage Layout and File Identity

## Status

Accepted.

## Context

RustShare currently uses a Workspace folder model for internal modules such as Notes, Kanban, Meetings, Decisions, and Standups. Obsidian vault sync should not be mixed into internal RustShare Notes because Obsidian vaults have their own file/folder semantics, link behavior, attachment placement, and identity model.

## Decision

Store external vaults outside internal Workspace modules.

Preferred future-proof layout:

```text
My Files/
  Workspace/
    Notes/
    Kanban/
    Meetings/
    Decisions/
    Standups/

  Vaults/
    Obsidian/
      <vault-name>/
        *.md
        attachments/
        images/
        diagrams/
        templates/
        ...
```

Alternative acceptable layout if easier for current RustShare code:

```text
My Files/
  Obsidian/
    <vault-name>/
```

The preferred layout is `My Files/Vaults/Obsidian/<vault-name>` because it makes future adapters easier.

## File Identity Rules

File identity is path-based inside a vault:

```text
vault_id + relative_path
```

Examples:

```text
Architecture/RustShare.md
Meetings/2026-06-01.md
Attachments/diagram.png
```

RustShare must preserve:

```text
- filenames
- extensions
- Unicode characters
- spaces
- folder structure
- attachment paths
- case where the underlying storage supports it
```

RustShare must not automatically slugify or normalize user files inside the vault.

## Metadata Model

Each synced file must have server metadata outside the file body:

```json
{
  "file_id": "uuid",
  "vault_id": "uuid",
  "adapter": "obsidian_vault",
  "relative_path": "Architecture/RustShare.md",
  "content_type": "text/markdown",
  "sha256": "...",
  "size": 18420,
  "server_rev": 42,
  "mtime_client": 1760000000000,
  "mtime_server": 1760000000500,
  "deleted": false,
  "last_writer_device_id": "macbook-senol"
}
```

## Attachment Rule

Attachments are not hidden metadata. They are first-class files in the vault tree.

If a note embeds:

```markdown
![[diagram.png]]
```

RustShare must preserve the referenced file as a visible file.

## Acceptance Criteria

```text
- External vaults are not stored inside Workspace/Notes.
- Each vault has a stable vault_id.
- Each file is identified by vault_id + relative_path.
- Attachments appear in the RustShare file tree.
- Sync metadata is stored outside Markdown bodies.
- No automatic path rewriting except root vault folder collision handling.
```
