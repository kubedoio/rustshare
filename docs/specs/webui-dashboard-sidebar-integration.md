# Specification: WebUI Dashboard and Sidebar Integration

Status: Draft / Implementation Ready  
Owner: RustShare Core Team  
Related ADR: ADR-0018  

## 1. Purpose

This specification defines how Template Modules appear in the RustShare WebUI.

Enabled modules must appear in:

- the left sidebar, if sidebar placement is enabled
- the dashboard module grid, if dashboard placement is enabled
- the dynamic module route `/modules/:moduleKey`

The dashboard and sidebar must be rendered from module definitions.

## 2. Current UI correction

Fix typo:

```text
Enabled file-backed work areas in this workspace.
```

Replace with:

```text
Enabled file-backed work areas in this workspace.
```

Remove notification/bell icon from the left sidebar.
Keep notification/bell icon in the top header.

## 3. Sidebar layout

Required layout:

```text
App mark / logo

Dashboard
My Files

Enabled module icons (canonical roots under /Workspace):
  Notes       → /Workspace/Notes
  Meeting Notes → /Workspace/Meetings
  Standups    → /Workspace/Standups
  Kanban      → /Workspace/Kanban
  Decisions   → /Workspace/Decisions
  Shares      → /Workspace/Shares

Bottom:
  Settings
  Admin, if user is admin
```

## 4. Sidebar rendering rules

A module appears in the sidebar only when:

```text
module.enabled = true
module.ui.sidebar.enabled = true
user has module access
```

Sorting:

```text
sort by module.ui.sidebar.order ascending
```

Each sidebar module icon must support:

- icon from approved registry
- tooltip/label
- route to `/modules/:moduleKey`
- active visual state
- permission-aware visibility

## 5. Dashboard layout

The dashboard must be renewed into this structure:

```text
Compact Workspace Summary
Workspace Modules Grid
Module Summary Sections
Recent Activity / Audit
```

## 6. Compact Workspace Summary

The current large Workspace Overview should be reduced into a smaller upper section.

Required data:

- total files
- shared items
- storage used
- quota limit
- enabled module count
- primary `+ New` action

Example layout:

```text
Workspace
22 files · 0 shared · 23.79 MB / 10 GB · 3 modules enabled     + New
```

## 7. Workspace Modules Grid

The dashboard must show enabled dashboard modules as cards.

A module appears in the grid only when:

```text
module.enabled = true
module.ui.dashboard.enabled = true
user has module access
```

Sorting:

```text
sort by module.ui.dashboard.order ascending
```

Each card shows:

- icon
- module name
- description
- root path
- compact summary
- primary action

## 8. Module card behavior

Clicking the card body opens:

```text
/modules/:moduleKey
```

Clicking the primary action executes:

- create from template
- open module
- open today's item
- manage shares
- or fallback generic action

Primary actions must be defined in `module.ui.dashboard.primaryAction`.

## 9. Module summary sections

Below the module grid, dashboard may render section summaries for enabled modules.

Required initial behavior:

| Module | Dashboard section |
|---|---|
| Notes | Recent Notes, first 4 |
| Meeting Notes | Recent Meetings, first 4 |
| Standups | Today's Standup + recent 4 |
| Kanban | Active Cards / Boards |
| Decisions | Recent Decisions, first 4 |
| Shares | Recent Shares and public/internal count |

If Notes is disabled, the Notes section must be hidden.

## 10. Empty states

If no modules are enabled:

```text
No workspace modules enabled yet.
Admins can enable Notes, Meeting Notes, Standups, Kanban, Decisions, and Shares from Admin > Modules.
```

If a module is enabled but has no content:

```text
No items yet.
Create the first item from this module's default template.
```

## 11. Responsive behavior

Desktop:

- sidebar visible
- compact summary full width
- modules grid 2-3 columns depending width
- summary sections below

Tablet:

- sidebar compact
- modules grid 2 columns

Mobile:

- sidebar collapses or becomes bottom/menu navigation
- modules grid 1 column
- module summaries remain readable

## 12. Visual design requirements

Match current RustShare style:

- warm off-white background
- subtle borders
- rounded cards
- restrained orange primary action buttons
- minimal enterprise feel
- no heavy glow
- no generic dark SaaS look
- no visual noise

## 13. Accessibility

- Sidebar icons must have accessible labels.
- Module cards must be keyboard reachable.
- Active module state must not rely on color alone.
- Tooltips must not be the only way to understand navigation.

## 14. Failure states

If module registry fails to load:

- show compact error state
- keep Dashboard/My Files accessible
- do not crash the entire shell

If a module summary provider fails:

- show generic fallback summary
- log error
- do not remove the module card
