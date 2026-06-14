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

## 2. Required module fields

Canonical serialized field names (API response / database):

```text
id
module_key
display_name
description
enabled
root_path
renderer
default_template
icon
schema_version
permissions
ai_indexing
audit
ui_config
created_at
updated_at
tenant_id
```

Compatibility aliases (legacy docs / older contracts):

| Canonical (snake_case) | Legacy alias (camelCase) |
|---|---|
| `module_key` | `key` |
| `display_name` | `displayName` |
| `root_path` | `rootPath` |
| `default_template` | `defaultTemplate` |
| `schema_version` | `schemaVersion` |
| `ai_indexing` | `aiIndexing` |
| `ui_config` | `ui` |

The API serializes modules using snake_case. Frontend types and normalizers must use the canonical snake_case names.

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
meetings
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

## 7. Module root policy

Canonical module roots:

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

New writes must use the canonical `/Workspace/<Module>` path. Legacy paths are supported for read-fallback during migration.

## 8. Path safety rules

Folder and file paths in templates:

- must be relative
- must not contain `..`
- must not start with `/`
- must not escape module root
- must not target system registry files

## 9. File-backed object contract

Every created object must have:

- stable object ID
- human-readable content file or folder
- machine-readable metadata
- creation audit event
- template version reference

Recommended metadata fields (canonical snake_case):

```json
{
  "id": "object_01HX...",
  "type": "kanban.card",
  "title": "Improve dashboard",
  "template": "template_default_kanban_card",
  "template_version": "1.0",
  "created_at": "2026-04-30T00:00:00Z",
  "updated_at": "2026-04-30T00:00:00Z"
}
```

Compatibility alias: `templateVersion`, `createdAt`, `updatedAt` are accepted during deserialization but canonical output uses snake_case.

## 10. Invariants

- Disabled modules are not shown on dashboard.
- Disabled modules are not shown in sidebar.
- Disabled modules keep their data.
- Public share renderer does not expose hidden metadata.
- Unknown renderer falls back to generic renderer.
- Template creation writes audit events.
- Move operations update path, metadata, and event log.

## 10. Legacy module root policy

Modules may exist at legacy root paths or at canonical workspace paths (e.g. `/Workspace/Notes`). The following policy governs all module services unless explicitly overridden and documented:

**Legacy roots:**
```text
/Notes
/Meetings
/Standups
/Kanban
/Decisions
/Brainstorming
/Shares
```

**Canonical roots:**
```text
/Workspace/Notes
/Workspace/Meetings
/Workspace/Standups
/Workspace/Kanban
/Workspace/Decisions
/Workspace/Brainstorming
/Workspace/Shares
```

1. **Read compatibility** — Queries must remain visible for data stored at legacy roots. Listing operations must include both legacy and canonical paths.
2. **Write canonical** — New items created from templates or module-specific services must be written exclusively under the canonical `/Workspace/<Module>` root. Services must not write new data to legacy roots.
3. **No duplicate roots** — When ensuring a module root folder exists, the service must locate or create the folder under `/Workspace`. If the canonical folder already exists, it must be reused. Creation of a duplicate root is prohibited.
4. **Explicit exceptions** — Any feature that requires write access to a legacy root must declare the exception in both this contract and `docs/specs/template-modules-system.md`.
