# ADR-0019: Shared Rich Markdown Editor for RustShare

## Status

Proposed / Permanent Architecture Candidate

## Context

RustShare is evolving from a file-sharing application into a file-backed workspace and company memory system.

Several RustShare modules require text authoring:

- Notes
- Meeting Notes
- Standup Records
- Decision Records
- Markdown files opened from the file browser
- README files inside folders/modules
- Kanban card details
- Brainstorming board README notes
- future documentation/wiki-style pages

A plain textarea or raw Markdown editor is not sufficient for the intended user experience. The desired experience is closer to Docmost, Notion, or Confluence: rich editing, slash commands, attachments, inline images, headings, tables, code blocks, task lists, smooth read/edit flow, and export capabilities.

However, RustShare must remain file-centric. It must not become a closed proprietary document database.

## Decision

RustShare will implement a shared **Rich Markdown Editor** as a reusable application component.

The editor will be used across RustShare modules and Markdown files.

The canonical storage format will remain Markdown.

The editor will provide a Docmost-like authoring experience while preserving RustShare's durable file-backed model.

## Architecture Rule

```text
Rich editor = user experience
Markdown file = source of truth
Attachments folder = asset storage
Metadata sidecar = machine state
Optional editor JSON = cache/acceleration, not canonical truth
```

## Scope

The editor must support:

- rich read mode
- edit mode
- headings
- bold / italic / underline
- bullet lists
- numbered lists
- task lists
- blockquotes
- inline code
- code blocks
- tables
- links
- images
- file attachments
- drag-and-drop uploads
- slash commands
- minimal toolbar
- autosave
- manual save
- Markdown import/export
- printable view
- browser PDF export flow
- permission-aware editing

## Non-goals for first implementation

The first implementation will not include:

- real-time collaboration
- comments
- collaborative cursors
- advanced page history diff UI
- AI writing assistant
- server-side PDF rendering
- DOCX export
- full Docmost feature parity
- proprietary document-only storage

These can be added later.

## Rationale

A single shared editor prevents each module from creating its own incompatible editing experience.

This decision improves:

- user experience
- module consistency
- file portability
- future AI/RAG indexing
- exportability
- product credibility
- implementation maintainability

## Consequences

Positive:

- Notes, Decisions, Meetings, Standups, Kanban cards and Markdown files share one editor.
- Documents remain usable outside RustShare.
- Attachments remain normal files.
- The product gains a modern knowledge-work experience.

Negative / risks:

- Rich editors are complex.
- Markdown serialization can lose fidelity for advanced custom blocks.
- Attachments introduce security and path-handling risks.
- PDF export needs careful print styling.
- Tiptap/Markdown conversion may require adapters and tests.

## Implementation Direction

Use a rich editor engine such as Tiptap/ProseMirror if compatible with the RustShare frontend stack.

If RustShare uses Svelte, isolate the Tiptap integration behind Svelte components and editor adapters.

Do not scatter editor logic across modules.

Required reusable components:

- `RichMarkdownEditor`
- `RichMarkdownViewer`
- `MarkdownDocumentPage`
- `EditorToolbar`
- `SlashCommandMenu`
- `AttachmentPanel`
- `PrintableDocumentView`

## Acceptance Criteria

- A Markdown file can be opened in read mode.
- A user with write permission can switch to edit mode.
- The editor can save Markdown back to the original file.
- Basic formatting persists to Markdown.
- Attachments are stored as files.
- Inline images are stored in the attachments folder and referenced by relative Markdown paths.
- The editor is reusable by Notes, Decisions and Meeting Notes.
- Printable view excludes editor chrome.
- Hidden metadata files are not exposed.
