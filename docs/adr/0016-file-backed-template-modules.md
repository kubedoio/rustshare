# ADR-0016: File-Backed Template Modules

Status: Accepted for file-backed content semantics; **superseded by ADR-0030 for platform/Application boundaries and Module terminology**  
Date: TBD  
Owner: RustShare Core Team  
Related: ADR-0017, ADR-0018, ADR-0030

## 2026-08-07 supersession note

ADR-0030 replaces `Module` as Elembra's top-level product/modularity abstraction with `Application`. The current `Module` API, registry and `/modules` terminology are migration sources, not permanent compatibility contracts.

The durable content-design principles in this ADR remain valid for Applications/features whose natural source of truth is file-backed—especially Notes, Decisions, Meetings, Standups and appropriate Kanban records. In particular, the separation of human-readable content, machine metadata, historical events and rebuildable indexes remains intentional.

Do **not** interpret this ADR as requiring every Elembra Application to be file-backed. Mail, Chat/Buzz, Memory, Agents, Connectors and future Applications own domain-specific state according to the canonical Elembra architecture.

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

For the file-backed structured-work features covered by this ADR, RustShare/Elembra will implement the durable content model described below. The historical term **File-Backed Template Module** remains useful when reading existing code and migrations, but ADR-0030 defines the target Application architecture.

```text
Module/Application Definition
  visibility, routing, permissions, renderer, root path, UI placement

Template Definition
  reusable creation pattern, folder structure, default files, form fields, metadata schema

Content Instance
  actual folder/file-backed object created from a template

Renderer
  WebUI projection for specialized behavior

Summary Provider
  dashboard summary logic
```

The file-backed content model obeys this permanent rule:

```text
Path = human organization
Metadata = machine state
Event log = historical truth
Index = fast search and rendering
Renderer = UI projection
```

## Architecture

### Application / file-backed feature

In the target architecture an Application is the product/domain boundary defined by ADR-0030. A file-backed feature such as Notes may define:

- whether it is enabled
- where its content lives in the workspace
- how it appears in the sidebar
- how it appears on the dashboard
- which renderer displays it
- which template is used by default
- who can use it
- whether/how it publishes to Elembra Memory
- whether audit logging is enabled

Existing `Module` records carrying these values are migrated into Application manifests/configuration and are not retained as a second permanent registry.

### Template

A template defines what gets created when the user creates an item.

A template defines:

- owning Application/feature
- template key
- version
- folder structure
- default files
- metadata sidecar content
- creation form fields
- UI icon/label
- renderer hint

### Content instance

A content instance is the actual object created in the workspace.

Examples:

```text
/Notes/Projects/Elembra AI Integration.md
/Meetings/Engineering/2026/2026-04-30-architecture/index.md
/Kanban/Product Launch/02-In-Progress/CARD-0001-improve-dashboard/index.md
/Decisions/Architecture/DEC-0001-application-model.md
/Shares/Public/Customer-Demo-Pack/README.md
```

### Metadata sidecars

Human-readable content and machine-readable state must be separated.

Recommended pattern:

```text
index.md                 human-readable content
.elembra.json            machine-readable state (target naming)
events.jsonl             local/domain history where appropriate
attachments/             user files related to the object
```

Existing `.rustshare*` sidecars are migration inputs. Their renaming/migration must preserve user data and should be performed deliberately rather than through a blind global rename.

For single-file notes and decisions, sidecars may be stored next to the Markdown file.

## Historical predefined modules

The existing RustShare implementation ships/predefines module records such as:

| Historical Module | Key | Typical root | Renderer |
|---|---|---|---|
| Notes | `notes` | `/Notes` or `/Workspace/Notes` | notes/okf-note |
| Meeting Notes | `meetings` | `/Meetings` | meetings |
| Standup Records | `standups` | `/Standups` | standups |
| Kanban | `kanban` | `/Kanban` | kanban |
| Decisions | `decisions` | `/Decisions` | decisions |
| Shares | `shares` | `/Shares` | shares |

These rows do not define the permanent Elembra Application taxonomy. Migration decisions are governed by ADR-0030 and `docs/migrations/rustshare-to-elembra-cutover.md`.

## File layout examples

### Notes

```text
/Notes/Projects/Elembra AI Integration.md
/Notes/Projects/Elembra AI Integration.elembra.json
```

### Meeting Notes

```text
/Meetings/Engineering/2026/2026-04-30-architecture/
  index.md
  .elembra.json
  events.jsonl
  attachments/
```

### Standup Records

```text
/Standups/Engineering/2026/04/2026-04-30.md
/Standups/Engineering/2026/04/2026-04-30.elembra.json
```

### Kanban

```text
/Kanban/Product Launch/
  .elembra-app.json
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
  .elembra.json
  events.jsonl
  attachments/
```

### Decisions

```text
/Decisions/Architecture/DEC-0001-application-model.md
/Decisions/Architecture/DEC-0001-application-model.elembra.json
```

### Shares

Existing share/file relationships should be evaluated during the Files Application migration. A share is primarily a Files authorization/domain concept; it need not become an independent top-level Application merely because the legacy Module registry contains `shares`.

## Consequences

### Positive

- File-backed knowledge stays durable, portable and exportable.
- Specialized UI remains a projection rather than the source of truth.
- AI/Memory indexing can operate on durable source artifacts with provenance.
- Human-readable content remains separated from machine state.
- The new Application architecture can retain good data semantics without making all Applications file-backed.

### Negative / trade-offs

- Renderers must interpret file-backed objects correctly.
- Move/rename operations need event logging and metadata updates.
- S3/object-storage semantics may require copy/delete operations for moves.
- Template migrations need explicit versioning.
- Existing `.rustshare*` metadata requires a safe naming/data migration if changed.

## Non-goals

- Do not make every Elembra Application file-backed.
- Do not build a proprietary database-first clone of Notion, Trello, or Confluence for content naturally represented as durable files.
- Do not retain the legacy Module API/registry as a permanent compatibility layer.
- Do not delete user files when disabling an Application.
- Do not expose hidden metadata in public shares by default.
