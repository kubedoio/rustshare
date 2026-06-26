# ADR-0019: Notes as OKF Documents

Status: Accepted  
Date: 2026-06-26  
Owner: RustShare Core Team  
Related: ADR-0017, ADR-0018, Issue #118

## Context

RustShare already treats modules as file-backed, admin-governed work areas. ADR-0017 defines the Module Registry and Template Registry as the source of truth for enabled modules, renderers, default templates, AI indexing policy, and audit policy. ADR-0018 defines that enabled modules drive sidebar, dashboard, routes, renderers, and summaries.

The current Notes implementation is still too Markdown-file-centric:

- Notes are described as first-class Markdown files with metadata sidecars.
- New folder-backed notes create a note bundle folder containing `note.md`, `attachments/`, `drawings/`, `exports/`, and `_rustshare/manifest.json`.
- `note.md` is currently the main note content file.
- The save path extracts the first H1 from the Markdown content and may rename the note bundle folder from that H1.
- The top-left note name is therefore not fully independent from document content.

This behavior is not compatible with the intended RustShare note model.

The note name, file/bundle identity, and document content heading must be separated:

- The top-left note name must represent the actual note/file/bundle name.
- The note name must be editable independently.
- The first H1 inside Markdown must remain normal document content.
- Changing the first H1 must not rename the note file or note bundle.

RustShare also needs a durable, portable, agent-readable format for future Company Memory and permission-aware RAG. Open Knowledge Format (OKF) is a strong fit because it is Markdown/YAML based, human-readable, file-friendly, and suitable as a canonical source format for RAG indexing.

## Decision

The RustShare Notes module will become an **OKF document module**.

New notes created through the Notes module must be created as OKF-compatible Markdown documents, not plain ad-hoc Markdown files.

RustShare will treat each note as one OKF concept document.

The preferred storage shape remains folder-backed:

```text
/Workspace/Notes/<note-name>/
├── note.md
├── attachments/
├── drawings/
├── exports/
└── _rustshare/
    └── manifest.json
```

`note.md` is the main OKF Markdown document and must include YAML frontmatter.

Example:

```markdown
---
type: Note
title: Customer onboarding checklist
description: Working note for onboarding tasks and follow-up actions.
resource: rustshare://workspace/<workspace-id>/notes/<note-id>
tags: []
timestamp: 2026-06-26T00:00:00Z
rustshare:
  module: notes
  source_kind: note
  source_id: <note-id>
  bundle_name: Customer onboarding checklist
  main: note.md
  visibility: private
  acl_hash: <acl-hash>
  embedding_policy: allowed
  verification_status: draft
---

# Working heading

The first H1 is document content. It is not the file name and must not rename the note.
```

## Identity and title rules

RustShare must separate four concepts:

| Concept | Source of truth | Behavior |
|---|---|---|
| Note identity | RustShare file/folder IDs | Stable internal identity. Never derived from H1. |
| Note/bundle name | Folder/file name plus manifest metadata | Shown in the top-left title area. Editable through Rename note. |
| OKF title | YAML frontmatter `title` | Mirrors the note display name by default and is updated by explicit note rename. |
| Markdown H1 | Markdown body content | Normal document content. Changing it must not rename the note. |

The top-left note title should show the actual note/bundle name. A first H1 or generated excerpt may be shown as an optional subtitle/description under the note name, but it must not control file identity.

## Admin module definition change

The predefined `notes` module must be updated from a generic Markdown note module to an OKF note-taking module.

Required module definition fields:

```json
{
  "key": "notes",
  "label": "Notes",
  "description": "Write OKF-compatible, file-backed notes for durable company memory.",
  "rootPath": "/Workspace/Notes",
  "renderer": "okf-note",
  "documentFormat": "okf-markdown",
  "defaultTemplate": "template_default_okf_note",
  "aiIndexingPolicy": {
    "enabled": true,
    "source": "okf-frontmatter-and-markdown",
    "permissionAware": true
  },
  "okf": {
    "enabled": true,
    "conceptType": "Note",
    "frontmatterRequired": true,
    "preserveUnknownFields": true
  }
}
```

The Admin > Modules UI must make this visible enough for administrators to understand that Notes are OKF-backed and RAG-ready, while still presenting the product simply as **Notes** to normal users.

The normal user-facing name remains **Notes**. Do not rename the module to “OKF Notes” in the sidebar or dashboard.

## Template change

Replace the default note template with `template_default_okf_note`.

The default template must create:

```text
note.md
attachments/
drawings/
exports/
_rustshare/manifest.json
```

`note.md` must contain valid OKF frontmatter.

The template must not place the note H1 in charge of the file name.

## Migration plan

Existing notes must be migrated safely and non-destructively.

### Legacy single-file Markdown notes

For existing plain `.md` notes:

1. Preserve the existing file content.
2. Add OKF frontmatter if missing.
3. Use the file name as the initial OKF `title`.
4. Use the first H1 only as optional subtitle/description if helpful.
5. Do not rename the file based on H1.
6. Preserve legacy sidecar metadata.

### Existing folder-backed notes

For existing note bundles:

1. Keep the current bundle folder.
2. Keep `note.md` as the main document.
3. Add or update OKF frontmatter in `note.md`.
4. Keep `_rustshare/manifest.json` as RustShare operational metadata.
5. Preserve attachments, drawings, and exports.
6. Stop the H1-based folder rename behavior.
7. Update manifest metadata so the explicit note name is the source of truth.

### Compatibility

During migration, RustShare must still open old notes that do not yet contain OKF frontmatter.

The editor may save them back as OKF documents after migration or explicit rewrite, but it must not destroy user content.

## RAG implication

The Notes module becomes the first native source of OKF concepts for RustShare RAG.

RAG indexing should consume Notes through the OKF layer:

```text
Notes module
  -> OKF note documents
  -> OKF concept registry
  -> structural chunks
  -> full-text / vector / graph indexes
  -> permission-aware RAG retrieval
```

The vector index is only a derived projection. The source of truth remains the RustShare note bundle plus OKF Markdown document and RustShare permissions.

## Why we made this decision

We made this decision because RustShare Notes should not remain generic Markdown files if RustShare is becoming durable company memory infrastructure.

Making Notes OKF-native gives RustShare:

- a clean foundation for permission-aware RAG;
- portable, human-readable company memory;
- a better bridge to Obsidian-style Markdown workflows;
- consistent AI indexing metadata from the moment a note is created;
- explicit ownership, visibility, and ACL metadata for RAG;
- a clear separation between file identity and document headings;
- a safer migration path from simple Markdown notes to structured knowledge documents.

This also fixes the current product issue where editing the first H1 can unexpectedly rename the note. That behavior is wrong for note-taking and dangerous for stable links, attachments, exports, citations, and future RAG provenance.

## Consequences

### Positive

- Notes become the first concrete OKF-backed RustShare module.
- RAG indexing starts from structured note metadata instead of guessing from plain Markdown.
- Note identity becomes stable and independent from document content.
- Note exports become more portable.
- Future OKF workspace export can include Notes without lossy conversion.

### Trade-offs

- The editor must preserve YAML frontmatter.
- The UI must decide whether frontmatter is shown, hidden, or edited through a metadata panel.
- Migration logic is required for existing Markdown notes.
- Tests must cover both legacy Markdown notes and OKF-native notes.

## Implementation requirements

- Add an OKF frontmatter parser/serializer for notes.
- Add an OKF note template.
- Update the predefined Notes module definition to use `renderer = okf-note` and `documentFormat = okf-markdown`.
- Update the note service so H1 changes never rename files or folders.
- Rename note only through explicit rename action.
- Update rename behavior so explicit rename updates the note/bundle name, manifest title, and OKF `title` frontmatter.
- Preserve unknown OKF frontmatter fields.
- Preserve Markdown body exactly except for controlled frontmatter updates.
- Keep `_rustshare/manifest.json` for RustShare operational metadata.
- Keep existing attachments/drawings/exports behavior.
- Ensure permission-aware RAG uses RustShare permissions before retrieving OKF note chunks.

## Acceptance criteria

- [ ] New Notes module items are OKF-compatible Markdown documents.
- [ ] `note.md` contains valid YAML frontmatter with `type: Note`.
- [ ] The top-left note name is editable independently from Markdown content.
- [ ] Changing the first H1 does not rename the note file or folder.
- [ ] Explicit Rename note updates the note/bundle name and OKF `title` frontmatter.
- [ ] Existing plain Markdown notes still open.
- [ ] Existing folder-backed notes still open.
- [ ] Migration adds OKF metadata without destroying content.
- [ ] Attachments, drawings, exports, and relative links remain valid.
- [ ] Admin > Modules shows the Notes module as OKF-backed through its module definition/configuration.
- [ ] RAG indexing can treat Notes as OKF concepts.
- [ ] Permission filtering happens before OKF note chunks are retrieved for RAG.

## Non-goals

- Do not rename the user-facing sidebar/dashboard module from **Notes** to **OKF Notes**.
- Do not force users to manually edit YAML frontmatter for normal note-taking.
- Do not make H1 a file-name controller.
- Do not replace RustShare permissions with OKF metadata.
- Do not make the vector index the source of truth.
