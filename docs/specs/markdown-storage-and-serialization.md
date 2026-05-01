# Specification: Markdown Storage and Serialization

## Canonical Storage

Markdown is canonical: `index.md` or `document.md`.

Optional cache: `index.editor.json`.

## Load Flow

Resolve document path, check read permission, load Markdown, load metadata, optionally load editor JSON cache, validate cache against source hash, parse Markdown into editor state, render read/edit mode.

## Save Flow

Serialize editor state to Markdown, validate payload, save Markdown file, optionally save editor JSON cache, update metadata, emit audit event if available, return revision metadata.

## Cache Validity

Editor cache must include source file, source hash, editor schema version and generatedAt. If source hash changes outside RustShare, ignore cache.

## Markdown Mapping

Heading -> `#`; bold -> `**`; italic -> `*`; underline -> `<u>` or extension; task list -> `- [ ]`; code block -> fenced code; image -> `![alt](./attachments/img.png)`; table -> GFM table; attachment -> standard link or custom block.

## Unsupported Syntax

Unsupported Markdown must not be destroyed. If unsafe to round-trip, load read mode and offer raw Markdown editing.
