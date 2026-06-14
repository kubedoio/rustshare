# ADR-006: Filename and Markdown Heading Separation

## Status

Accepted.

## Context

RustShare Notes previously tied the note file name to the first H1 heading inside the Markdown content. This behavior is incompatible with file-based vault workflows. In local Markdown vaults, the file path/name is the primary identity of the note. The first H1 heading is document content and must not rename the file.

This rule is especially important for Obsidian vault support because internal links, attachments, backlinks, and rename behavior are path-based.

## Decision

RustShare must separate file identity from Markdown document headings.

The visible file/note name in RustShare must be the actual filename/path. The first H1 inside the Markdown body must remain normal content.

Changing this:

```markdown
# New Heading
```

must not rename this:

```text
Architecture/RustShare.md
```

## UI Behavior

RustShare may optionally show the first H1 or a generated summary as a subtitle/description under the filename.

Example:

```text
RustShare.md
Architecture plan for RustShare Vault Sync
```

But the title and subtitle must be clearly separate.

## Implementation Constraints

```text
- Do not infer filename from first H1.
- Do not rename file when H1 changes.
- Do not rewrite H1 when file is renamed.
- Do not modify unrelated Files, Notes, or routing behavior.
- Keep existing editor layout unless a specific UI task changes it.
```

## Acceptance Criteria

```text
- Top-left name is editable independently as the actual file name.
- First H1 remains Markdown body content.
- Changing the H1 does not rename the file.
- Renaming the file does not rewrite the first H1.
- Obsidian vault files preserve their original filenames and headings.
```
