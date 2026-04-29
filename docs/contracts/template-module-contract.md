# Contract: Template Module Registry

Status: Draft / Implementation Ready  
Owner: RustShare Core Team  
Related ADRs: ADR-0016, ADR-0017, ADR-0018  

## 1. Module contract

A module definition must satisfy this contract.

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

## 2. Required module fields

```text
id
key
displayName
description
enabled
rootPath
renderer
defaultTemplate
schemaVersion
permissions
ui
aiIndexing
audit
```

## 3. Module key rules

Module keys must:

- be lowercase
- use hyphen-separated words
- be unique within workspace
- not contain slashes
- not contain spaces
- not begin with a dot

Valid:

```text
notes
meeting-notes
standups
kanban
decisions
shares
```

Invalid:

```text
Meeting Notes
../notes
.rustshare
kanban/board
```

## 4. Template contract

A template definition must satisfy this contract.

```json
{
  "id": "template_default_kanban_board",
  "key": "default-kanban-board",
  "name": "Default Kanban Board",
  "description": "Creates a standard Kanban board folder structure.",
  "module": "kanban",
  "version": "1.0",
  "renderer": "kanban",
  "ui": {
    "createLabel": "New Kanban Board",
    "icon": "columns",
    "form": {
      "fields": [
        {
          "key": "title",
          "label": "Board title",
          "type": "text",
          "required": true
        }
      ]
    }
  },
  "folderStructure": [
    "00-Backlog",
    "01-Ready",
    "02-In-Progress",
    "03-Review",
    "04-Done"
  ],
  "defaultFiles": [
    {
      "path": "README.md",
      "content": "# {{title}}\n\nThis board is file-backed."
    },
    {
      "path": ".rustshare-module.json",
      "contentType": "application/json"
    }
  ],
  "metadataSchema": {
    "type": "kanban.board",
    "fields": {
      "title": "string",
      "owner": "string",
      "statusColumns": "array"
    }
  }
}
```

## 5. Required template fields

```text
id
key
name
module
version
renderer
ui
folderStructure
defaultFiles
metadataSchema
```

## 6. Template key rules

Template keys must:

- be lowercase
- use hyphen-separated words
- be unique within workspace
- not contain slashes
- not contain spaces
- not begin with a dot

## 7. Path safety rules

Folder and file paths in templates:

- must be relative
- must not contain `..`
- must not start with `/`
- must not escape module root
- must not target system registry files

## 8. File-backed object contract

Every created object must have:

- stable object ID
- human-readable content file or folder
- machine-readable metadata
- creation audit event
- template version reference

Recommended metadata fields:

```json
{
  "id": "object_01HX...",
  "type": "kanban.card",
  "title": "Improve dashboard",
  "template": "template_default_kanban_card",
  "templateVersion": "1.0",
  "createdAt": "2026-04-30T00:00:00Z",
  "updatedAt": "2026-04-30T00:00:00Z"
}
```

## 9. Invariants

- Disabled modules are not shown on dashboard.
- Disabled modules are not shown in sidebar.
- Disabled modules keep their data.
- Public share renderer does not expose hidden metadata.
- Unknown renderer falls back to generic renderer.
- Template creation writes audit events.
- Move operations update path, metadata, and event log.
