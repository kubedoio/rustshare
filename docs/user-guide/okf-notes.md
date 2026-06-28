# OKF Notes in RustShare

RustShare stores Notes as **Open Knowledge Format (OKF)** documents. This guide explains what that means for you, how to create and edit notes, and how RustShare keeps your note identity stable when you rename, share, or sync files externally.

---

## What is an OKF note?

An OKF note is a folder-backed Markdown bundle, not a single file. The bundle lives under your workspace's Notes root:

```text
/Workspace/Notes/<note-name>/
├── note.md                         # Main OKF Markdown document
├── note.md.rustshare.json          # Visible sidecar (title, OKF id, ACL, conflict state)
├── attachments/                    # Attached files
├── drawings/                       # Excalidraw or drawing exports
├── exports/                        # Rendered exports of the note
└── _rustshare/
    └── manifest.json               # RustShare operational cache
```

The Markdown file `note.md` carries a YAML frontmatter block that makes the note machine-readable and RAG-ready. RustShare uses this frontmatter to track the note's stable identity, title, visibility, and indexing policy.

### Why a bundle?

- **Attachments and drawings stay with the note.** Moving or renaming the note keeps relative links valid.
- **Identity is separate from content.** The note has a stable id that does not change when you edit the title or the first heading.
- **External tools can read it.** The format is plain Markdown/YAML, so IDEs, sync clients, and Obsidian-style workflows can open `note.md` directly.

---

## Creating a note

1. Open the **Notes** module from the sidebar or dashboard.
2. Click **New note**.
3. RustShare creates a new bundle from the default OKF note template:
   - A folder named after your note title (default: `Untitled note`).
   - `note.md` with a valid OKF frontmatter block.
   - Empty `attachments/`, `drawings/`, `exports/`, and `_rustshare/` folders.
   - A visible sidecar, `note.md.rustshare.json`.

A fresh, unique OKF id is generated for the note when it is created. That id is written into the frontmatter and never changes.

---

## Editing a note

When you open a note, RustShare renders the Markdown body in the **rich editor**. The YAML frontmatter is preserved automatically; you do not need to see it for normal writing.

### Rich editor

- Edit the Markdown body as usual.
- Add headings, lists, attachments, and drawings.
- RustShare saves the body while keeping the frontmatter intact.

### Raw mode

If you need to edit the full Markdown source, including the YAML frontmatter:

1. Switch to **Raw** mode in the editor.
2. Edit the complete file.
3. Save.

Use Raw mode when you need to change metadata fields such as `description`, `tags`, or `resource`. Be careful not to modify fields that RustShare manages, especially `rustshare.id`.

---

## Title vs. first heading

RustShare separates four concepts that older note tools often confuse:

| Concept | What controls it | Behavior |
|---|---|---|
| Note identity | `rustshare.id` in frontmatter | Stable. Never changes unless the note is duplicated or imported. |
| Note/bundle name | Folder name and manifest | Shown as the note title in the UI. |
| OKF title | YAML frontmatter `title` | Mirrors the note display name. Updated by explicit rename. |
| Markdown H1 | Markdown body content | Normal document text. Changing it does **not** rename the note. |

### Renaming a note

To rename a note, use the **Rename note** action. RustShare updates:

- The bundle folder name.
- The OKF `title` in frontmatter.
- The manifest title in `_rustshare/manifest.json`.
- The visible sidecar.

### Editing the first heading does not rename the note

If you change the first `# Heading` in the Markdown body, only the document content changes. The note file, folder, and title stay the same. This keeps links, shares, attachments, and AI indexes stable.

---

## Sharing and AI search

### Private and public notes

Each note has a visibility setting:

- **Private** — Only you, and users or groups you explicitly share the note with, can read it.
- **Public** — The note can be opened by anyone who has the public share URL, typically `/p/note/<share_id>`.

To change visibility, use the share controls in the note UI. Toggling to Public generates a public share link. Toggling back to Private revokes public access.

### AI and RAG indexing

RustShare indexes OKF notes for AI retrieval. Indexing follows RustShare permissions:

- Owner access
- Direct user share
- Group share
- Public share

In addition, each note has an `embedding_policy` in the OKF frontmatter:

- `allowed` — The note may be included in AI vector and full-text indexes.
- `denied` — The note is excluded from AI indexing even if you have permission to read it.

RustShare permissions are always the source of truth for access control. OKF visibility metadata describes intent but does not grant access by itself.

---

## Conflicts and reconciliation

Because `note.md` is plain Markdown, you can edit it outside RustShare — in an IDE, a sync tool, or an Obsidian vault. RustShare reconciles those changes when it scans, syncs, or opens the note.

### When RustShare reconciles

RustShare compares three values:

1. The `title` in the YAML frontmatter.
2. The note bundle folder name.
3. The title stored in `_rustshare/manifest.json`.

If all three match, no action is needed. If one is newer and the others are unchanged, RustShare propagates the newest value.

### When a conflict happens

A conflict occurs when the YAML title and the folder name were both changed since the last known sync, and they no longer agree. RustShare:

- Shows a conflict banner in the note UI.
- Marks the note with a visible conflict state in the sidecar.
- Does not rename folders or rewrite files until you resolve it.
- Keeps the note readable.

### Resolving a conflict

The conflict banner offers three options:

- **Prefer YAML title** — Use the title from the frontmatter and rename the folder to match.
- **Prefer folder name** — Use the folder name as the title and update the frontmatter to match.
- **Custom title** — Enter a new title and update both the frontmatter and the folder name.

In every case, RustShare preserves `rustshare.id` and updates the manifest and sidecar.

### Duplicate id conflicts

If RustShare detects two notes with the same `rustshare.id`, it marks the duplicate as a `duplicate_id` conflict. This can happen when a note bundle is copied outside RustShare without generating a new id. Resolve the conflict by restoring the original note or by creating a fresh note with a new id.

---

## Do not edit `rustshare.id`

The `rustshare.id` field in the frontmatter is the stable identity of the note. It is used for:

- Internal file and folder references.
- Share links.
- Graph links between notes.
- RAG provenance and chunk ownership.
- Reconciliation across devices and external edits.

If you change or duplicate `rustshare.id`, RustShare may:

- Lose the link between the note and its indexes.
- Create a `duplicate_id` conflict.
- Break shares, attachments, and AI retrieval for the affected note.

Never edit `rustshare.id` manually. If you need a new note, create one through the Notes module so RustShare generates a fresh id.

---

## Appendix: OKF frontmatter reference

The following is a complete example of an OKF note frontmatter block. You do not need to memorize this for normal use; RustShare manages these fields for you.

```markdown
---
type: Note
title: Customer onboarding checklist
description: Working note for onboarding tasks and follow-up actions.
resource: rustshare://workspace/<workspace-id>/notes/<note-id>
tags:
  - onboarding
  - customers
timestamp: 2026-06-26T00:00:00Z
rustshare:
  id: <stable-note-uuid>
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

### Top-level fields

| Field | Purpose | Managed by |
|---|---|---|
| `type` | OKF concept type. Always `Note` for notes. | RustShare |
| `title` | Display title of the note. | RustShare; safe to edit in Raw mode |
| `description` | Short description or excerpt. | User |
| `resource` | Canonical RustShare resource URI. | RustShare |
| `tags` | User-defined tags. | User |
| `timestamp` | ISO 8601 timestamp of creation or last significant update. | RustShare |
| `rustshare` | RustShare operational metadata block. | RustShare |

### `rustshare` block fields

| Field | Purpose | Managed by |
|---|---|---|
| `id` | Stable note identity (UUID). | RustShare; **do not edit** |
| `module` | Source module. Always `notes`. | RustShare |
| `source_kind` | Kind of source artifact. Always `note`. | RustShare |
| `source_id` | Module-level source id. | RustShare |
| `bundle_name` | Name of the note bundle folder. | RustShare |
| `main` | Primary document file. Always `note.md`. | RustShare |
| `visibility` | Note visibility intent (`private` or `public`). | RustShare; toggle through share UI |
| `acl_hash` | Hash of the current ACL state. | RustShare |
| `embedding_policy` | AI indexing policy (`allowed` or `denied`). | RustShare; may be exposed in metadata UI |
| `verification_status` | Document verification state (e.g. `draft`). | RustShare |

### Preserved fields

RustShare preserves unknown top-level and `rustshare` fields during round-trip editing. You can add custom metadata for your own workflows, but RustShare will not interpret fields it does not recognize.

---

*Last updated: 2026-06-28*
