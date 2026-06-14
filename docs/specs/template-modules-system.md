# Specification: Template Modules System

Status: Draft / Implementation Ready  
Owner: RustShare Core Team  
Related ADRs: ADR-0016, ADR-0017, ADR-0018  

## 1. Purpose

The Template Modules System provides a permanent way to define file-backed RustShare work areas.

Examples:

- Notes
- Meeting Notes
- Standup Records
- Kanban Dashboard
- Decisions
- Public/Internal Shares

A module is not a separate mini-app. A module is a structured file-backed capability rendered by the WebUI.

## 2. Core concepts

### Module Definition

Controls capability, visibility, root path, renderer, permissions, and WebUI placement.

### Template Definition

Controls what files, folders, metadata, and form fields are created.

### Module Renderer

Controls how a module is displayed in the WebUI.

### Module Summary Provider

Controls what compact dashboard summary is shown for a module.

### Module Instance

The actual folder/file-backed object created from a template.

## 3. Permanent invariant

```text
Module registry decides what appears.
Template registry decides what gets created.
Renderer decides how it looks.
Files/folders store the real object.
Metadata stores machine state.
Event log stores history.
Index stores fast searchable projections.
```

## 4. Predefined modules

```json
[
  {
    "key": "notes",
    "displayName": "Notes",
    "rootPath": "/Workspace/Notes",
    "renderer": "notes"
  },
  {
    "key": "meetings",
    "displayName": "Meeting Notes",
    "rootPath": "/Workspace/Meetings",
    "renderer": "meetings"
  },
  {
    "key": "standups",
    "displayName": "Standup Records",
    "rootPath": "/Workspace/Standups",
    "renderer": "standups"
  },
  {
    "key": "kanban",
    "displayName": "Kanban Dashboard",
    "rootPath": "/Workspace/Kanban",
    "renderer": "kanban"
  },
  {
    "key": "decisions",
    "displayName": "Decisions",
    "rootPath": "/Workspace/Decisions",
    "renderer": "decisions"
  },
  {
    "key": "shares",
    "displayName": "Shares",
    "rootPath": "/Workspace/Shares",
    "renderer": "shares"
  }
]
```

## 5. Module Definition

Required shape:

```json
{
  "id": "module_notes",
  "key": "notes",
  "displayName": "Notes",
  "description": "Capture file-backed notes and reusable knowledge.",
  "enabled": true,
  "rootPath": "/Workspace/Notes",
  "renderer": "notes",
  "defaultTemplate": "template_default_note",
  "icon": "sticky-note",
  "schemaVersion": "1.0",
  "permissions": {
    "adminCanConfigure": true,
    "workspaceMembersCanUse": true,
    "allowPublicShare": false,
    "allowInternalShare": true
  },
  "ui": {
    "sidebar": {
      "enabled": true,
      "order": 30,
      "icon": "sticky-note",
      "label": "Notes"
    },
    "dashboard": {
      "enabled": true,
      "order": 10,
      "cardTitle": "Notes",
      "cardDescription": "Recent file-backed notes.",
      "summaryMode": "recent-items",
      "maxItems": 4,
      "primaryAction": {
        "label": "New Note",
        "action": "create-from-template",
        "template": "template_default_note"
      }
    },
    "modulePage": {
      "layout": "list-grid",
      "emptyStateTitle": "No notes yet",
      "emptyStateDescription": "Create your first file-backed note.",
      "emptyStateAction": "New Note"
    }
  },
  "aiIndexing": {
    "enabled": true
  },
  "audit": {
    "enabled": true
  }
}
```

## Dashboard Summary Semantics

Module summaries are computed by the backend from the module root path and the
requesting user's current permissions.

Rules:

- Include owned files/folders under the module root.
- Include files/folders visible through active user shares, active group shares,
  or inherited folder shares.
- Exclude revoked or expired shares.
- Exclude hidden/system metadata from user-facing recent item lists.
- For Kanban, the `kanban-overview` summary returns recent cards plus a
  `boards` array. Directly shared board folders must appear in that `boards`
  array even when the canonical `/Workspace/Kanban` root is not shared.

## 6. Template Definition

Required shape:

```json
{
  "id": "template_default_meeting_note",
  "key": "default-meeting-note",
  "name": "Default Meeting Note",
  "description": "Creates a structured meeting note.",
  "module": "meetings",
  "version": "1.0",
  "renderer": "meeting-note",
  "ui": {
    "createLabel": "New Meeting Note",
    "icon": "calendar-days",
    "form": {
      "fields": [
        {
          "key": "title",
          "label": "Meeting title",
          "type": "text",
          "required": true
        },
        {
          "key": "team",
          "label": "Team",
          "type": "text",
          "required": false
        },
        {
          "key": "date",
          "label": "Date",
          "type": "date",
          "required": true
        }
      ]
    }
  },
  "folderStructure": [
    "attachments"
  ],
  "defaultFiles": [
    {
      "path": "index.md",
      "content": "# {{title}}\n\nDate: {{date}}\nTeam: {{team}}\n\n## Attendees\n\n## Agenda\n\n## Notes\n\n## Decisions\n\n## Action Items\n- [ ] "
    },
    {
      "path": ".rustshare.json",
      "contentType": "application/json"
    }
  ],
  "metadataSchema": {
    "type": "meeting.note",
    "fields": {
      "title": "string",
      "team": "string",
      "date": "date",
      "participants": "array",
      "linkedDecisions": "array",
      "linkedCards": "array"
    }
  }
}
```

## 7. Module root policy

Canonical module root paths use the `/Workspace/<Module>` prefix.

```text
/Workspace/Notes
/Workspace/Meetings
/Workspace/Standups
/Workspace/Kanban
/Workspace/Decisions
/Workspace/Brainstorming
/Workspace/Shares
```

Legacy compatibility alias (read-only):

```text
/Notes
/Meetings
/Standups
/Kanban
/Decisions
/Brainstorming
/Shares
```

**Legacy Root Policy:**

1. **Read compatibility** — Queries must remain visible for data stored at legacy roots. Listing operations must include both legacy and canonical paths.
2. **Write canonical** — New items created from templates or module-specific services must be written exclusively under the canonical `/Workspace/<Module>` root. Services must not write new data to legacy roots.
3. **No duplicate roots** — When ensuring a module root folder exists, the service must locate or create the folder under `/Workspace`. If the canonical folder already exists, it must be reused. Creation of a duplicate root is prohibited.
4. **Explicit exceptions** — Any feature that requires write access to a legacy root must declare the exception in both this spec and `docs/contracts/template-module-contract.md`.

New data must always write to the canonical `/Workspace/<Module>` path. Legacy paths are supported for read-fallback during migration but must not be used for new writes.

## 8. Registry persistence

Recommended default paths:

```text
/.rustshare/system/modules/modules.json
/.rustshare/system/templates/templates.json
```

## 9. Startup behavior

On application startup or workspace initialization:

1. Load registry files.
2. Validate schema version.
3. Ensure predefined modules exist.
4. Ensure predefined templates exist.
5. Preserve admin changes.
6. Ensure enabled module root folders exist under the canonical `/Workspace/<Module>` path.
7. Before creating a module root folder, verify the canonical path does not already exist. Reuse the existing folder; do not create a duplicate.
8. Do not delete disabled module data.
8. Do not overwrite custom templates.

## 10. Create-from-template flow

When a user creates an item from a template:

1. Validate user permission.
2. Validate module enabled state.
3. Load template.
4. Validate form input.
5. Resolve target path to the canonical `/Workspace/<Module>` subtree.
6. Create folder structure.
7. Render default files.
8. Create metadata sidecar.
9. Append audit event.
10. Refresh relevant indexes and dashboard summaries.

## 11. Audit events

Required event types:

```text
module.enabled
module.disabled
module.ui.updated
template.created
template.updated
template.deleted
object.created.from_template
object.moved
object.archived
share.created
share.updated
share.expired
```
