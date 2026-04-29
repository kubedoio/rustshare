# Specification: Admin Modules and Templates

Status: Draft / Implementation Ready  
Owner: RustShare Core Team  
Related ADRs: ADR-0017, ADR-0018  

## 1. Purpose

The Admin Modules and Templates area allows workspace administrators to control which file-backed modules are available, where they appear in the WebUI, and which templates users can create from.

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
Default template
Sidebar order
Dashboard order
Last updated
Configure
```

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
- configure AI indexing
- configure audit logging

## 5. Enable module behavior

When an admin enables a module:

1. Set `module.enabled = true`.
2. Ensure root folder exists.
3. Ensure module metadata file exists, if required by renderer.
4. Append `module.enabled` audit event.
5. Refresh sidebar and dashboard registry views.

## 6. Disable module behavior

When an admin disables a module:

1. Set `module.enabled = false`.
2. Hide module from sidebar.
3. Hide module from dashboard.
4. Keep root folder and all user data.
5. Keep registry entry.
6. Append `module.disabled` audit event.

Disabling a module must never delete files.

## 7. Admin > Templates

The Templates page must show all system and custom templates.

Required columns:

```text
Template name
Template key
Module
Version
Renderer
Icon
Enabled
System/custom
Default for module
Created by
Last updated
Actions
```

## 8. Template actions

Admins can:

- create template
- edit custom template
- duplicate system or custom template
- delete custom template
- enable/disable template
- set template as module default

System templates must not be destructively edited unless a migration explicitly supports it.

## 9. Template creation form

Required fields:

```text
Template name
Template key
Description
Module
Renderer
Icon key
Create button label
Folder structure
Default files
Metadata schema
Form fields
AI indexing policy
Audit policy
```

For MVP, folder structure, default files, and metadata schema may be edited in a JSON editor with validation.

## 10. Form field types

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

## 11. Validation

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

## 12. Approved icon registry

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

## 13. Permissions

- Only admins can access `/admin/modules`.
- Only admins can access `/admin/templates`.
- Only admins can change module UI placement.
- Only admins can create/edit/delete templates.
- Normal users can use templates only if module policy allows.

## 14. Empty states

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
