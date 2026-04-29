# Test Plan: WebUI Module Integration

Status: Draft / Implementation Ready  
Owner: RustShare Core Team  
Related ADR: ADR-0018  

## 1. Sidebar tests

### Left sidebar notification removal

Expected:

- notification/bell icon is absent from the left sidebar
- notification/bell icon remains in the top header

### Enabled modules appear below folder icon

Given enabled modules with `ui.sidebar.enabled = true`, they appear below the My Files/folder icon.

### Disabled modules hidden

Given a disabled module, it does not appear in sidebar.

### Sidebar order

Modules are sorted by:

```text
ui.sidebar.order
```

### Active route state

When route is `/modules/notes`, the Notes sidebar icon is highlighted.

### Unauthorized modules hidden

Modules unavailable to the user must not appear in sidebar.

## 2. Dashboard tests

### Compact summary renders

Dashboard renders compact workspace summary with:

- total files
- shared items
- storage usage
- quota
- enabled module count
- `+ New` action

### Workspace Modules grid renders

Given enabled modules with `ui.dashboard.enabled = true`, cards appear in the grid.

### Disabled dashboard modules hidden

Disabled modules do not render as cards.

### Dashboard order

Module cards are sorted by:

```text
ui.dashboard.order
```

### Notes summary

If Notes is enabled, dashboard shows first 4 recent notes.

### Notes disabled

If Notes is disabled, Recent Notes section is hidden.

### Empty modules state

If no modules are enabled, dashboard shows:

```text
No workspace modules enabled yet.
Admins can enable Notes, Meeting Notes, Standups, Kanban, Decisions, and Shares from Admin > Modules.
```

### Typo fix

Dashboard text must read:

```text
Enabled file-backed work areas in this workspace.
```

## 3. Routing tests

### Dynamic module route

`/modules/notes` opens Notes module.

### Kanban route

`/modules/kanban` opens Kanban module.

### Unknown module

Unknown module key shows not found state.

### Disabled module route

Disabled module route shows module disabled state.

### Unauthorized module route

Unauthorized module route shows access denied state.

### Unknown renderer

Unknown renderer falls back to GenericModuleView.

## 4. Module page shell tests

Every module page shows:

- icon
- module name
- description
- root path
- primary action
- open root folder action
- recent activity

## 5. Summary provider tests

### Provider success

Module summary provider renders expected summary.

### Provider failure

If summary provider fails:

- module card still renders
- generic summary fallback is shown
- error is logged

## 6. Responsive tests

Desktop:

- sidebar visible
- modules grid multi-column

Tablet:

- compact sidebar
- modules grid two columns

Mobile:

- sidebar collapses or moves behind menu
- modules grid one column
- summaries remain readable

## 7. Accessibility tests

- sidebar icons have accessible labels
- module cards are keyboard navigable
- active state is not color-only
- primary actions are reachable by keyboard
- tooltips are not required to understand navigation
