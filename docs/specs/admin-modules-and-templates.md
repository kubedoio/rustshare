# Specification: Admin Modules and Templates

Status: Draft / Implementation Ready  
Owner: RustShare Core Team  
Related ADRs: ADR-0017, ADR-0018, ADR-0019  

## 1. Purpose

The Admin Modules and Templates area allows workspace administrators to control which file-backed modules are available, where they appear in the WebUI, and which templates users can create from.

The Notes module is a special predefined module: it remains file-backed and user-facing as **Notes**, but its default note-taking mechanism must create OKF-compatible Markdown documents instead of plain ad-hoc Markdown files.

## 2. Admin routes

Required routes:

```text
/admin/modules
/admin/templates
```

Optional future routes:

```text
/admin/modules/:moduleKey
/admin/templates/:templateKey
/admin/modules/sidebar-order
/admin/modules/dashboard-order
```

## 3. Admin > Modules

The Modules page must show all predefined and custom modules.

Required table/list columns:

```text
Enabled
Sidebar
Dashboard
Icon
Module name
Description
Root path
Renderer
Document format
Default template
AI indexing
OKF enabled
Sidebar order
Dashboard order
Last updated
Configure
```

For legacy module definitions that do not expose `documentFormat` or `okf`, the Admin UI must show a safe fallback value such as `file-backed` / `not configured` instead of failing.

## 4. Module actions

Admins can:

- enable module
- disable module
- pin/unpin module from sidebar
- show/hide module on dashboard
- change sidebar order
- change dashboard order
- change icon from approved icon registry
- change root path
- choose default template
- configure renderer
- configure document format
- configure OKF behavior, when supported by the module
- configure AI indexing
- configure audit logging

## 5. Notes module OKF requirements

The predefined `notes` module must be represented in the Admin module registry as an OKF-backed note-taking module.

Normal users must still see the module as **Notes**. Do not rename the sidebar or dashboard label to “OKF Notes”.

Required predefined module shape:

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
  "auditPolicy": {
    "enabled": true,
    "events": [
      "note.created",
      "note.renamed",
      "note.updated",
      "note.deleted",
      "note.okf_migrated"
    ]
  },
  "okf": {
    "enabled": true,
    "conceptType": "Note",
    "frontmatterRequired": true,
    "preserveUnknownFields": true
  }
}
```

The Admin > Modules configuration view for Notes must make these properties visible:

- root path: `/Workspace/Notes`
- renderer: `okf-note`
- document format: `okf-markdown`
- default template: `template_default_okf_note`
- OKF concept type: `Note`
- frontmatter required: yes
- AI indexing source: OKF frontmatter + Markdown body
- permission-aware indexing: required

The Admin UI may allow toggling AI indexing for Notes, but it must not allow disabling the fact that Notes are stored as OKF-compatible documents once ADR-0019 is implemented.

## 6. Notes title and H1 behavior

The Notes module must separate note identity from Markdown content.

Required behavior:

- The top-left note name is the actual note/file/bundle name.
- The note name is editable independently through explicit Rename note behavior.
- The first H1 inside `note.md` is normal document content.
- Changing the first H1 must not rename the note file or folder.
- The first H1 or generated excerpt may be shown as an optional subtitle/description under the note name.
- Explicit Rename note updates the note/bundle name, `_rustshare/manifest.json`, and the OKF frontmatter `title`.

## 7. Enable module behavior

When an admin enables a module:

1. Set `module.enabled = true`.
2. Ensure root folder exists.
3. Ensure module metadata file exists, if required by renderer.
4. For the Notes module, ensure the OKF default template and OKF metadata configuration exist.
5. Append `module.enabled` audit event.
6. Refresh sidebar and dashboard registry views.

## 8. Disable module behavior

When an admin disables a module:

1. Set `module.enabled = false`.
2. Hide module from sidebar.
3. Hide module from dashboard.
4. Keep root folder and all user data.
5. Keep registry entry.
6. Append `module.disabled` audit event.

Disabling a module must never delete files.

For the Notes module, disabling the module must not delete OKF documents, note bundles, attachments, drawings, exports, manifests, or RAG/indexing metadata. It only hides the module entry points according to the module visibility rules.

## 9. Admin > Templates

The Templates page must show all system and custom templates.

Required columns:

```text
Template name
Template key
Module
Version
Renderer
Document format
Icon
Enabled
System/custom
Default for module
Created by
Last updated
Actions
```

## 10. Template actions

Admins can:

- create template
- edit custom template
- duplicate system or custom template
- delete custom template
- enable/disable template
- set template as module default

System templates must not be destructively edited unless a migration explicitly supports it.

The system default OKF note template must be treated as a system template. Admins may duplicate it to create custom OKF note templates, but custom templates must still produce valid OKF frontmatter when assigned to the Notes module.

## 11. Template creation form

Required fields:

```text
Template name
Template key
Description
Module
Renderer
Document format
Icon key
Create button label
Folder structure
Default files
Metadata schema
OKF frontmatter schema
Form fields
AI indexing policy
Audit policy
```

For MVP, folder structure, default files, metadata schema, and OKF frontmatter schema may be edited in a JSON editor with validation.

## 12. Default OKF note template

The default Notes template key must be:

```text
template_default_okf_note
```

It must create this bundle structure:

```text
note.md
attachments/
drawings/
exports/
_rustshare/manifest.json
```

`note.md` must include YAML frontmatter:

```markdown
---
type: Note
title: Untitled note
description: ""
resource: rustshare://workspace/<workspace-id>/notes/<note-id>
tags: []
timestamp: <created-at>
rustshare:
  module: notes
  source_kind: note
  source_id: <note-id>
  bundle_name: Untitled note
  main: note.md
  visibility: private
  acl_hash: <acl-hash>
  embedding_policy: allowed
  verification_status: draft
---

# Untitled

Start writing here.
```

The Markdown H1 is starter content only. It must not control note identity.

## 13. Form field types

Supported initial template form field types:

```text
text
textarea
date
datetime
select
multiselect
checkbox
user
users
tags
```

## 14. Validation

Template validation must enforce:

- template key is unique
- module key exists
- renderer is known or allowed to fallback to generic
- icon is in approved registry
- root path is valid
- folder paths are relative
- default files cannot escape module root
- metadata file names are reserved and controlled
- custom templates cannot overwrite system templates
- public templates cannot expose hidden metadata
- templates assigned to the Notes module must produce valid OKF frontmatter
- OKF templates must preserve unknown frontmatter fields during round-trip editing
- OKF templates must not make H1 content control file or folder naming

## 15. Approved icon registry

Initial approved icon keys:

```text
layout-dashboard
folder
file-text
sticky-note
calendar-days
clipboard-list
columns
git-branch
share-2
lock
globe
settings
```

No raw SVG or raw HTML is allowed in template definitions.

## 16. Permissions

- Only admins can access `/admin/modules`.
- Only admins can access `/admin/templates`.
- Only admins can change module UI placement.
- Only admins can create/edit/delete templates.
- Normal users can use templates only if module policy allows.
- RAG/indexing settings must not bypass RustShare permissions.

For Notes, permission-aware indexing is mandatory: OKF metadata can describe visibility, but RustShare permissions remain the source of truth for access control.

## 17. Empty states

If no custom templates exist:

```text
No custom templates yet.
Create a template to standardize how your team creates notes, meetings, boards, decisions, and shares.
```

If no modules are enabled:

```text
No modules are enabled.
Enable Notes, Meeting Notes, Kanban, Decisions, Standups, or Shares to add file-backed work areas to the dashboard.
```
