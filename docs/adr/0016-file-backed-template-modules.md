# ADR-0016: File-Backed Template Modules

Status: Accepted  
Date: TBD  
Owner: RustShare Core Team  
Related: ADR-0017, ADR-0018  

## Context

RustShare is evolving from a file-sharing application into a file-centric company memory and workspace platform.

The product direction requires permanent support for structured work areas such as:

- Notes
- Meeting Notes
- Standup Records
- Kanban Dashboard and Cards
- Decisions
- Public/Internal Shares

These features must not become disconnected mini-applications with separate data models. RustShare's product identity is that work artifacts remain files, folders, metadata, and event history.

The dashboard and module views may provide specialized experiences, but the source of truth must remain durable and portable.

## Decision

RustShare will implement these features as **File-Backed Template Modules**.

A Template Module is a permanent RustShare capability that combines:

```text
Module Definition
  visibility, routing, permissions, renderer, root path, UI placement

Template Definition
  reusable creation pattern, folder structure, default files, form fields, metadata schema

Module Instance
  actual folder/file-backed object created from a template

Renderer
  WebUI projection for module-specific behavior

Summary Provider
  dashboard summary logic for enabled modules
```

The module system must obey this permanent rule:

```text
Path = human organization
Metadata = machine state
Event log = historical truth
Index = fast search and rendering
Renderer = UI projection
```

## Architecture

### Module

A module is a product capability such as `notes`, `kanban`, or `decisions`.

A module defines:

- whether it is enabled
- where it lives in the workspace
- how it appears in the sidebar
- how it appears on the dashboard
- which renderer displays it
- which template is used by default
- who can use it
- whether it is indexed for AI/company memory
- whether audit logging is enabled

### Template

A template defines what gets created when the user creates an item.

A template defines:

- module key
- template key
- version
- folder structure
- default files
- metadata sidecar content
- creation form fields
- UI icon/label
- renderer hint

### Module instance

A module instance is the actual object created in the workspace.

Examples:

```text
/Notes/Projects/RustShare AI Integration.md
/Meetings/Engineering/2026/2026-04-30-template-modules/index.md
/Kanban/Product Launch/02-In-Progress/CARD-0001-improve-dashboard/index.md
/Decisions/Architecture/DEC-0001-file-backed-template-modules.md
/Shares/Public/Customer-Demo-Pack/README.md
```

### Metadata sidecars

Human-readable content and machine-readable state must be separated.

Recommended pattern:

```text
index.md                 human-readable content
.rustshare.json          machine-readable state
events.jsonl             audit/event history
attachments/             user files related to the object
```

For single-file notes and decisions, sidecars may be stored next to the Markdown file:

```text
DEC-0001-example.md
DEC-0001-example.rustshare.json
```

## Predefined modules

RustShare will ship with these predefined modules:

| Module | Key | Root path | Renderer |
|---|---|---|---|
| Notes | `notes` | `/Notes` | `notes` |
| Meeting Notes | `meeting-notes` | `/Meetings` | `meeting-notes` |
| Standup Records | `standups` | `/Standups` | `standups` |
| Kanban Dashboard | `kanban` | `/Kanban` | `kanban` |
| Decisions | `decisions` | `/Decisions` | `decisions` |
| Shares | `shares` | `/Shares` | `shares` |

## File layout examples

### Notes

```text
/Notes/Projects/RustShare AI Integration.md
/Notes/Projects/RustShare AI Integration.rustshare.json
```

### Meeting Notes

```text
/Meetings/Engineering/2026/2026-04-30-template-modules/
  index.md
  .rustshare.json
  events.jsonl
  attachments/
```

### Standup Records

```text
/Standups/Engineering/2026/04/2026-04-30.md
/Standups/Engineering/2026/04/2026-04-30.rustshare.json
```

### Kanban

```text
/Kanban/Product Launch/
  .rustshare-module.json
  00-Backlog/
  01-Ready/
  02-In-Progress/
  03-Review/
  04-Done/
```

A card may be represented as:

```text
/Kanban/Product Launch/02-In-Progress/CARD-0001-improve-dashboard/
  index.md
  .rustshare.json
  events.jsonl
  attachments/
```

### Decisions

```text
/Decisions/Architecture/DEC-0001-file-backed-template-modules.md
/Decisions/Architecture/DEC-0001-file-backed-template-modules.rustshare.json
```

### Shares

```text
/Shares/Internal/Engineering/RustShare-Preview-Pack/
  README.md
  .rustshare-share.json
  files/

/Shares/Public/Customer-Demo-Pack/
  README.md
  .rustshare-share.json
  files/
```

## Consequences

### Positive

- RustShare keeps a file-centric architecture.
- Modules become permanent, extensible platform capabilities.
- Users can sync/export/backup module data as files.
- AI/RAG indexing can operate on durable artifacts.
- Audit trails can be stored close to the object.
- Custom templates can be added without hardcoding new dashboard widgets.

### Negative / trade-offs

- Renderers must interpret file-backed objects correctly.
- Move/rename operations need event logging and metadata updates.
- S3/object-storage semantics may require copy/delete operations for moves.
- Template migrations need versioning and compatibility rules.

## Non-goals

- Do not build a proprietary database-first clone of Notion, Trello, or Confluence.
- Do not hardcode Notes/Kanban/Decision cards directly into the dashboard.
- Do not delete user files when disabling a module.
- Do not expose hidden `.rustshare*` metadata in public shares by default.
