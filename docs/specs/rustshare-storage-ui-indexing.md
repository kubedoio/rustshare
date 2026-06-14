# SPEC-003: RustShare Storage, UI, and Indexing

## Purpose

Define how RustShare should display, index, and manage externally synchronized vaults.

## Storage Location

Preferred:

```text
My Files/Vaults/Obsidian/<vault-name>/
```

Acceptable temporary layout:

```text
My Files/Obsidian/<vault-name>/
```

## UI Navigation

RustShare should show a separate area:

```text
Vaults
  Obsidian
    <vault-name>
```

Do not mix synced vault files into:

```text
Workspace/Notes
```

## File Badges

Files under synced vaults should show source metadata:

```text
Source: Vault Sync
Adapter: Obsidian vault
Last synced: <timestamp>
Last device: <device name>
Server revision: <rev>
```

## Open in Client Link

For Obsidian vaults, RustShare may show:

```text
Open in Obsidian
```

Use Obsidian URI only as a local convenience link. This must not imply official affiliation.

Example format:

```text
obsidian://open?vault=<vault-name>&file=<relative-path>
```

## Markdown Preview

RustShare preview should preserve and safely render:

```text
- Markdown headings
- YAML frontmatter display or folding
- [[wikilinks]] as internal file links where possible
- ![[embedded attachments]] as preview where possible
- Markdown links
- tags
- code blocks
```

Preview must not rewrite the source file.

## Indexing

Index these fields:

```text
filename
relative_path
Markdown text
headings
frontmatter keys/values
tags
wikilinks
outlinks
attachments
modified time
vault_id
adapter_type
```

## Search Behavior

RustShare global search may include vault files, but results should indicate source:

```text
Vault Sync / Obsidian vault / <vault-name>
```

## Acceptance Criteria

```text
- Synced vault files are visually separate from Workspace Notes.
- Attachments are visible in the file tree.
- Search can find Markdown content from synced vaults.
- Preview supports common Obsidian-style link/embed syntax without modifying files.
- File name and first H1 remain independent.
- UI follows naming guardrails.
```
