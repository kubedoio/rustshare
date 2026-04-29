# ADR-0017: Template Registry and Admin Governance

Status: Accepted  
Date: TBD  
Owner: RustShare Core Team  
Related: ADR-0016, ADR-0018  

## Context

Template Modules must be permanent and manageable by workspace administrators.

RustShare needs a governance layer so admins can:

- enable or disable modules
- control whether modules appear in the sidebar
- control whether modules appear on the dashboard
- define ordering
- select icons from a safe icon registry
- choose default templates
- create custom templates
- configure root paths and renderers
- define AI indexing and audit policies

The system must remain data-driven. The WebUI must render from registry definitions, not from hardcoded module lists.

## Decision

RustShare will implement two registries:

```text
Module Registry
  permanent capabilities and their runtime/admin/UI configuration

Template Registry
  reusable file/folder creation patterns for modules
```

The registries must be persisted as RustShare system metadata.

Recommended default paths:

```text
/.rustshare/system/modules/modules.json
/.rustshare/system/templates/templates.json
```

If RustShare already has a system metadata storage layer, that layer may be used, but the data model must remain compatible with these documents.

## Module governance

Admins can configure predefined and custom modules.

Each module supports:

- enabled/disabled
- sidebar visibility
- dashboard visibility
- sidebar order
- dashboard order
- icon key
- label
- root path
- renderer
- default template
- permission policy
- AI indexing policy
- audit policy

Disabling a module must hide it from dashboard/sidebar, but must not delete files.

## Template governance

Admins can create and manage templates.

Each template supports:

- name
- key
- module key
- version
- renderer hint
- icon key
- create button label
- form fields
- folder structure
- default files
- metadata schema
- AI indexing policy
- audit policy

System templates may be duplicated but must not be destructively overwritten by custom templates.

## Default predefined modules

The system must ensure these module definitions exist on workspace initialization:

```text
notes
meeting-notes
standups
kanban
decisions
shares
```

Missing predefined modules should be created.
Existing admin changes must not be overwritten.

## Startup behavior

On startup or workspace initialization:

1. Load module registry.
2. Load template registry.
3. Ensure predefined modules exist.
4. Ensure predefined templates exist.
5. Preserve admin changes.
6. Ensure enabled module root paths exist.
7. Do not enable all modules automatically unless default policy says so.
8. Do not delete data from disabled or removed modules.

## Admin routes

Recommended admin routes:

```text
/admin/modules
/admin/templates
```

Optional future routes:

```text
/admin/modules/sidebar-order
/admin/modules/dashboard-order
/admin/modules/:moduleKey
/admin/templates/:templateKey
```

## Permissions

- Only admins can configure modules.
- Only admins can create/edit/delete templates.
- Normal users can use enabled modules only if module policy allows.
- Disabled modules must not appear in navigation.
- Disabled module routes must show a clear disabled state.

## Security rules

- Template UI definitions must not accept raw HTML.
- Template UI definitions must not accept raw SVG.
- Icons must be selected from an approved icon registry.
- Default files must not escape the module root path.
- Public share rendering must not expose hidden RustShare metadata.
- Custom templates must not overwrite system templates.

## Consequences

### Positive

- Admins can shape the RustShare workspace without code changes.
- WebUI navigation can be data-driven.
- Custom templates can be added safely.
- System defaults remain upgradeable.

### Trade-offs

- Registry migration logic is required.
- Module and template validation must be strict.
- UI needs fallback behavior for unknown renderers.
