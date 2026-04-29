# Test Plan: Template Modules System

Status: Draft / Implementation Ready  
Owner: RustShare Core Team  
Related ADRs: ADR-0016, ADR-0017, ADR-0018  

## 1. Registry tests

### Default module creation

Given an empty module registry, startup must create predefined module entries:

```text
notes
meeting-notes
standups
kanban
decisions
shares
```

### Preserve admin changes

Given an existing module registry with admin modifications, startup must not overwrite:

- enabled state
- sidebar visibility
- dashboard visibility
- order
- root path
- default template

### Default template creation

Given an empty template registry, startup must create predefined templates.

### Preserve custom templates

Given custom templates, startup must not delete or overwrite them.

## 2. Module lifecycle tests

### Enable module

When admin enables a module:

- module enabled state becomes true
- root folder is created if missing
- dashboard/sidebar can render it if UI flags allow
- audit event `module.enabled` is written

### Disable module

When admin disables a module:

- module enabled state becomes false
- module is hidden from dashboard
- module is hidden from sidebar
- files are not deleted
- audit event `module.disabled` is written

## 3. Template tests

### Create custom template

Admin can create a valid custom template.

Expected:

- template is saved
- template key is unique
- audit event `template.created` is written

### Duplicate template key

Attempting to create duplicate template key must fail.

### Invalid paths

Template paths containing any of the following must fail:

```text
../
/
.rustshare/system
```

### Invalid icon

Template or module icon outside approved registry must fail.

### Unknown renderer

Unknown renderer must not crash the system. It must fallback to generic renderer where allowed.

## 4. Create-from-template tests

When user creates object from template:

- module must be enabled
- user must have permission
- folder structure must be created
- default files must be created
- metadata sidecar must be created
- template version must be stored
- audit event `object.created.from_template` must be written

## 5. Kanban file-backed tests

Moving a Kanban card between columns must:

- move the card folder/file
- update metadata status
- append `object.moved` event
- refresh board index
- preserve stable object ID

## 6. Public share security tests

Public share rendering must not expose:

```text
.rustshare.json
.rustshare-share.json
.rustshare-module.json
events.jsonl
/.rustshare/system/*
```

## 7. Permission tests

- non-admin cannot configure modules
- non-admin cannot create/edit/delete templates
- user without module permission cannot open module
- disabled module route shows disabled state

## 8. Migration tests

When schema version changes:

- old registry loads safely
- migration preserves custom settings
- migration preserves custom templates
- unknown future fields are ignored or preserved according to migration policy
