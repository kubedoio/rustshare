# Specification: Rich Markdown Editor System

## Purpose

Implement a reusable Docmost-like rich Markdown editor for RustShare while preserving RustShare's file-backed architecture.

The editor must provide a smooth rich writing experience for notes, Markdown files, meeting notes, decisions, standups, Kanban card details, README files, and future workspace modules.

## Product Principles

```text
One editor core.
Many module integrations.
Markdown remains canonical.
Attachments remain files.
Editor JSON is optional cache.
PDF export starts with printable rendering.
```

## Required Components

- `RichMarkdownEditor`: editable rich document canvas.
- `RichMarkdownViewer`: sanitized read-mode renderer.
- `MarkdownDocumentPage`: route/page wrapper with read/edit, title, status and actions.
- `AttachmentPanel`: lists/uploads/inserts attachments.
- `SlashCommandMenu`: `/` menu for block insertion.
- `EditorToolbar`: minimal formatting toolbar.
- `PrintableDocumentView`: print/PDF output.

## Supported Documents

Folder-backed document:

```text
/{ModuleRoot}/{DocumentSlug}/
  index.md
  .rustshare.json
  /attachments/
  index.editor.json optional
```

Single file document:

```text
/path/to/file.md
/path/to/file.attachments/ optional
```

## Required Formatting Features

MVP: H1/H2/H3, bold, italic, underline, paragraph, bullet list, numbered list, task list, blockquote, inline code, fenced code block, horizontal rule, link, image, table, attachment card/link.

Later: callouts, mentions, internal links, embeds, comments, page history diff, collaborative cursors, AI assist.

## Save Model

Manual save and debounced autosave both serialize editor state to Markdown and save it as canonical source. Optional editor JSON cache may be saved, but it must not be required.

Save status values: `saved`, `saving`, `unsaved`, `error`.

## Editor Engine

Recommended: Tiptap/ProseMirror, isolated behind an adapter. If RustShare uses Svelte, use a Svelte wrapper and keep editor-specific logic out of modules.

## Raw Markdown Escape Hatch

Provide or preserve raw Markdown editing when feasible, to recover unsupported syntax and debug conversion issues.

## Acceptance Criteria

- Same editor can edit Notes, Decisions and Meeting Notes.
- Markdown remains canonical.
- Attachments are file-backed.
- Permission checks are respected.
- Hidden metadata is not exposed.
