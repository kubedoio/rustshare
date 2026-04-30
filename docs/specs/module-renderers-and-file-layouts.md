# Specification: Module Renderers and File Layouts

Status: Draft / Implementation Ready  
Owner: RustShare Core Team  
Related ADRs: ADR-0016, ADR-0018  

## 1. Purpose

Module renderers provide specialized WebUI views over file-backed RustShare modules.

The renderer must not become the source of truth. It is only a projection over files, folders, metadata, event logs, and indexes.

## 2. Renderer resolution

When opening `/modules/:moduleKey`:

1. Load module definition by `moduleKey`.
2. Check `module.enabled`.
3. Check user permission.
4. Resolve `module.renderer`.
5. Render specialized renderer if available.
6. Otherwise render `GenericModuleView`.

## 3. Required renderers

Initial specialized renderers:

```text
NotesModuleView
MeetingNotesModuleView
StandupsModuleView
KanbanModuleView
DecisionsModuleView
SharesModuleView
GenericModuleView
```

Renderer keys:

```text
notes
meetings
standups
kanban
decisions
shares
generic-template
```

## 4. Common module page shell

Every module page must show:

- module icon
- module name
- module description
- root path
- primary action
- secondary action: Open root folder
- recent activity
- empty state if no items exist

## 5. Notes layout

Root path:

```text
/Notes
```

Recommended item layout:

```text
/Notes/Projects/RustShare AI Integration.md
/Notes/Projects/RustShare AI Integration.rustshare.json
```

Renderer behavior:

- list recent notes
- support create note
- open note editor
- show tags, modified time, visibility
- allow root folder navigation

Dashboard summary:

- first 4 recent notes

## 6. Meeting Notes layout

Root path:

```text
/Meetings
```

Recommended item layout:

```text
/Meetings/Engineering/2026/2026-04-30-template-modules/
  index.md
  .rustshare.json
  events.jsonl
  attachments/
```

Renderer behavior:

- list recent meeting notes
- create meeting from template
- show date/team/participants
- link decisions and action items
- allow attachments

Dashboard summary:

- first 4 recent meeting notes

## 7. Standup Records layout

Root path:

```text
/Standups
```

Recommended item layout:

```text
/Standups/Engineering/2026/04/2026-04-30.md
/Standups/Engineering/2026/04/2026-04-30.rustshare.json
```

Renderer behavior:

- show today's standup status
- create today's standup
- list recent records
- extract blockers and follow-ups

Dashboard summary:

- today's status
- first 4 recent records

## 8. Kanban layout

Root path:

```text
/Kanban
```

Board layout:

```text
/Kanban/Product Launch/
  .rustshare-module.json
  00-Backlog/
  01-Ready/
  02-In-Progress/
  03-Review/
  04-Done/
```

Card layout:

```text
/Kanban/Product Launch/02-In-Progress/CARD-0001-improve-dashboard/
  index.md
  .rustshare.json
  events.jsonl
  attachments/
```

Renderer behavior:

- show boards
- open board
- show columns from folders
- show cards from folders/files
- drag card between columns
- moving card updates path, metadata, and event log

Dashboard summary:

- active boards
- first 4 active cards

## 9. Decisions layout

Root path:

```text
/Decisions
```

Recommended item layout:

```text
/Decisions/Architecture/DEC-0001-file-backed-template-modules.md
/Decisions/Architecture/DEC-0001-file-backed-template-modules.rustshare.json
```

Renderer behavior:

- list recent decisions
- filter by status
- create decision
- link meetings/cards/files
- show accepted/superseded status

Dashboard summary:

- first 4 recent decisions

## 10. Shares layout

Root path:

```text
/Shares
```

Recommended layout:

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

Renderer behavior:

- manage internal shares
- manage public shares
- show expiration
- show download/upload policy
- never expose hidden metadata publicly

Dashboard summary:

- internal share count
- public share count
- first 4 recent shares

## 11. GenericModuleView

Used when no specialized renderer exists.

Generic view shows:

- module title
- description
- root path
- list of files/folders under root
- primary create-from-template action
- recent modified items
- empty state

The generic renderer is required for custom templates and future modules.
