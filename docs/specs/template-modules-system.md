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
    "rootPath": "/Notes",
    "renderer": "notes"
  },
  {
    "key": "meeting-notes",
    "displayName": "Meeting Notes",
    "rootPath": "/Meetings",
    "renderer": "meeting-notes"
  },
  {
    "key": "standups",
    "displayName": "Standup Records",
    "rootPath": "/Standups",
    "renderer": "standups"
  },
  {
    "key": "kanban",
    "displayName": "Kanban Dashboard",
    "rootPath": "/Kanban",
    "renderer": "kanban"
  },
  {
    "key": "decisions",
    "displayName": "Decisions",
    "rootPath": "/Decisions",
    "renderer": "decisions"
  },
  {
    "key": "shares",
    "displayName": "Shares",
    "rootPath": "/Shares",
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
  "rootPath": "/Notes",
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

## 6. Template Definition

Required shape:

```json
{
  "id": "template_default_meeting_note",
  "key": "default-meeting-note",
  "name": "Default Meeting Note",
  "description": "Creates a structured meeting note.",
  "module": "meeting-notes",
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

## 7. Registry persistence

Recommended default paths:

```text
/.rustshare/system/modules/modules.json
/.rustshare/system/templates/templates.json
```

## 8. Startup behavior

On application startup or workspace initialization:

1. Load registry files.
2. Validate schema version.
3. Ensure predefined modules exist.
4. Ensure predefined templates exist.
5. Preserve admin changes.
6. Ensure enabled module root folders exist.
7. Do not delete disabled module data.
8. Do not overwrite custom templates.

## 9. Create-from-template flow

When a user creates an item from a template:

1. Validate user permission.
2. Validate module enabled state.
3. Load template.
4. Validate form input.
5. Resolve target path.
6. Create folder structure.
7. Render default files.
8. Create metadata sidecar.
9. Append audit event.
10. Refresh relevant indexes and dashboard summaries.

## 10. Audit events

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
