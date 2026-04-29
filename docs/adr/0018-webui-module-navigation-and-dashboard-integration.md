# ADR-0018: WebUI Module Navigation and Dashboard Integration

Status: Accepted  
Date: TBD  
Owner: RustShare Core Team  
Related: ADR-0016, ADR-0017  

## Context

The current RustShare WebUI has:

- top header with search, notification bell, appearance control, and user menu
- left sidebar with primary navigation icons
- workspace overview card
- notes area
- workspace modules area, currently not fully populated

The product requirement is to make enabled Template Modules visible and usable from the WebUI.

When a module is enabled, users should see:

- a small icon in the left sidebar below the folder/My Files icon, if the module is sidebar-pinned
- a module card in the dashboard grid, if the module is dashboard-enabled
- a module page when clicking the sidebar icon or dashboard card
- a compact summary in the dashboard

The notification bell must be removed from the left sidebar and kept only in the top header.

## Decision

RustShare WebUI will render module navigation and dashboard content from the Module Registry.

The WebUI must not hardcode module cards or sidebar module icons.

Permanent rule:

```text
Enabled module definitions drive sidebar, dashboard, routes, renderers, and summaries.
```

## Sidebar integration

The left sidebar will contain:

```text
App mark / logo

Primary navigation:
  Dashboard
  My Files

Enabled module navigation:
  Notes
  Meeting Notes
  Standups
  Kanban
  Decisions
  Shares

Bottom:
  Settings
  Admin, if user is admin
```

Only modules matching all conditions appear in the sidebar:

```text
module.enabled = true
module.ui.sidebar.enabled = true
user has module access
```

Sidebar modules must be sorted by:

```text
module.ui.sidebar.order
```

The left sidebar notification/bell icon must be removed.
The top header notification bell must remain.

## Dashboard integration

The dashboard will be redesigned around a compact operational layout:

```text
Compact Workspace Summary
Enabled Modules Grid
Module Summary Sections
Recent Activity / Audit
```

The old large Workspace Overview card should be reduced into a compact upper section.

The compact summary should show:

- total files
- shared items
- storage used
- quota
- enabled module count
- primary `+ New` action

Below it, the dashboard renders `Workspace Modules` as a grid of enabled dashboard modules.

Only modules matching all conditions appear in the dashboard:

```text
module.enabled = true
module.ui.dashboard.enabled = true
user has module access
```

Dashboard modules must be sorted by:

```text
module.ui.dashboard.order
```

## Module summaries

Each module can have a summary provider.

Initial summary providers:

| Module | Summary behavior |
|---|---|
| Notes | first 4 recent notes |
| Meeting Notes | first 4 recent meeting notes |
| Standups | today's status plus first 4 recent records |
| Kanban | active boards plus first 4 active cards |
| Decisions | first 4 recent decisions |
| Shares | public/internal count plus first 4 recent shares |

If no summary provider exists, the WebUI must render a generic file-backed summary.

Generic summary:

- total items under root path
- last modified item
- root path
- primary create action, if available

## Module routing

Prefer one dynamic route:

```text
/modules/:moduleKey
```

Route resolution:

1. Load module by `moduleKey`.
2. If not found, show not found state.
3. If disabled, show module disabled state.
4. If unauthorized, show access denied state.
5. Resolve renderer from `module.renderer`.
6. Render specialized renderer if available.
7. Otherwise render `GenericModuleView`.

## Module page shell

Every module page must have a common shell:

- icon
- module name
- description
- root path
- primary action
- secondary action: open root folder
- recent activity

Specialized renderers may add module-specific content.

## WebUI definitions in module manifests

Module manifests must include WebUI placement configuration:

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

## Consequences

### Positive

- Sidebar and dashboard become extensible.
- Enabling a module immediately changes the workspace UI.
- Custom modules/templates can use generic renderers.
- The dashboard becomes operational instead of storage-statistic-heavy.

### Trade-offs

- Module registry loading must be available early in WebUI boot.
- Sidebar rendering must support permissions and active route state.
- Dashboard summary providers require module-specific adapters.
- Unknown renderers require a safe generic fallback.

## Non-goals

- Do not implement module navigation as hardcoded icons.
- Do not keep duplicate notification bells in top header and left sidebar.
- Do not show disabled modules in dashboard or sidebar.
- Do not make the dashboard empty when only one module is enabled.
