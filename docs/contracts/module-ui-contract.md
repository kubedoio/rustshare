# Contract: Module WebUI Definition

Status: Draft / Implementation Ready  
Owner: RustShare Core Team  
Related ADR: ADR-0018  

## 1. Purpose

This contract defines the required WebUI fields for modules and templates.

The WebUI must render sidebar icons, dashboard cards, module pages, and creation actions from registry data.

## 2. Module UI contract

```json
{
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
  }
}
```

## 3. Sidebar UI fields

Required:

```text
enabled
order
icon
label
```

Rules:

- `enabled` controls sidebar visibility.
- `order` controls position below My Files.
- `icon` must be approved icon key.
- `label` must be text only.

## 4. Dashboard UI fields

Required:

```text
enabled
order
cardTitle
cardDescription
summaryMode
maxItems
primaryAction
```

Rules:

- `enabled` controls dashboard card visibility.
- `order` controls card order.
- `cardTitle` and `cardDescription` must be text only.
- `summaryMode` must be supported or fallback to generic.
- `maxItems` should default to 4.

## 5. Primary action contract

Supported actions:

```text
open-module
create-from-template
open-root-folder
open-todays-item
manage-shares
generic-create
```

Example:

```json
{
  "label": "New Note",
  "action": "create-from-template",
  "template": "template_default_note"
}
```

## 6. Summary modes

Supported initial summary modes:

```text
recent-items
today-status
kanban-active-cards
share-counts
generic-file-summary
```

Fallback:

```text
generic-file-summary
```

## 7. Module page layout values

Supported initial layouts:

```text
list-grid
kanban-board
gallery-grid
calendar-list
standup-today
decision-registry
share-manager
generic-file-list
```

Unknown layout must fallback to:

```text
generic-file-list
```

## 8. Page config compatibility alias

The canonical page config key is `page`. The legacy alias `modulePage` is still accepted during normalization but must be mapped to `page` on output.

Example canonical shape:

```json
{
  "page": {
    "enabled": true,
    "route": "/modules/notes",
    "renderer": "notes",
    "layout": "list-grid",
    "emptyStateTitle": "No notes yet",
    "emptyStateDescription": "Create your first file-backed note.",
    "emptyStateAction": "New Note",
    "primaryAction": {
      "label": "New Note",
      "action": "create-from-template",
      "template": "template_default_note"
    },
    "searchPlaceholder": "Search notes...",
    "filterLabel": "All notes",
    "sortLabel": "Modified",
    "itemSingular": "note",
    "itemPlural": "notes"
  }
}
```

Legacy alias accepted during input:

```json
{
  "modulePage": {
    "layout": "list-grid",
    "emptyStateTitle": "No notes yet",
    "emptyStateDescription": "Create your first file-backed note.",
    "emptyStateAction": "New Note"
  }
}
```

## 9. Template UI contract

```json
{
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
        }
      ]
    }
  }
}
```

## 10. Form field contract

Required field properties:

```text
key
label
type
required
```

Supported types:

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

## 11. Approved icon registry

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
lightbulb
```

> **Note:** `lightbulb` is used by the Brainstorming module default but is currently missing from the backend runtime registry. This is a known gap tracked in the consistency backlog.

No raw SVG, raw HTML, external image URL, or scriptable icon payload is allowed.

## 12. Widget type registry

Canonical widget types:

```text
kanban-summary
decisions-meetings-summary
latest-notes
active-shares
recent-brainstorm-boards
recent-items
shares-overview
generic-module-summary
```

Unknown widget type must fallback to:

```text
generic-module-summary
```

## 13. Rendering invariants

- Sidebar modules are rendered from enabled registry entries.
- Dashboard modules are rendered from enabled registry entries.
- Dashboard cards are not hardcoded.
- Sidebar module icons are not hardcoded.
- Disabled modules are hidden.
- Unauthorized modules are hidden.
- Active module state is shown when route matches `/modules/:moduleKey`.
- Summary provider failure must fallback to generic summary.
