# RustShare Public Preview UI Simplification Report

**Audience:** Engineering / product implementation team  
**Scope:** RustShare alpha UI simplification for Public Preview / Open Source Preview  
**Product frame:** RustShare is the durable company memory / artifact layer in Kubedo’s Company Memory Infrastructure direction.  
**Launch principle:** Minimum credible functionality. Simple, honest, useful.

---

## 1. Executive Summary

RustShare already has the right alpha surface for a public preview: Home, Folders, Notes, Meeting Notes, Standup Records, Brainstorming, Kanban, Decisions, and Shares.

The goal is **not** to remove these sections. The goal is to make each section minimally functional, understandable, and launch-safe.

For launch, RustShare should feel like:

> A lightweight company-memory workspace for durable files, notes, meeting records, standups, idea boards, kanban boards, decisions, and shares.

It should **not** feel like:

> Jira, Confluence, Notion, Trello, Miro, or a heavy workflow suite.

The highest-priority work is:

1. Hide internal implementation files and raw metadata from normal users.
2. Make all create actions open the newly created artifact immediately.
3. Replace native browser prompts with RustShare-native modals.
4. Simplify every module to the smallest useful public-preview version.
5. Clearly mark Brainstorming / Idea Boards as preview or experimental.
6. Fix Kanban New Board before showing Kanban publicly.

---

## 2. Global Public Preview UI Principles

### Keep

- File-backed workspace model
- Clean artifact records
- Simple creation flows
- Search if stable
- Basic sharing and permissions
- Basic activity history
- Lightweight templates
- “Open in Files” where useful

### Avoid

- Required metadata-heavy forms
- Workflow-heavy UI
- Native browser prompts
- Raw implementation files
- Raw UUIDs/internal IDs
- Trello/Jira-like Kanban complexity
- Miro-like Brainstorming complexity
- Formal ADR/approval bureaucracy in Decisions
- Complex external-sharing dashboards

### Global artifact rule

Internally, RustShare may store an artifact as:

```text
artifact-folder/
├── index.md
├── .rustshare.json
├── events.jsonl
└── attachments/
```

But users should normally see only:

```text
Artifact title
Type
Updated date
Relevant actions
```

Users should not see `.rustshare.json`, `events.jsonl`, UUIDs, raw metadata, or sidecar files in normal product views.

---

## 3. Final Section Launch Decisions

| Section | Decision | Launch condition |
|---|---|---|
| Home | Launch-visible but simplified | Show summary, recent artifacts, recent activity, quick actions |
| Folders | Launch-visible but simplified | Hide metadata and sidecar files |
| Notes | Launch-visible but simplified | Keep simple file-backed notes |
| Meeting Notes | Launch-visible but simplified | Show one clean record per meeting |
| Standup Records | Launch-visible but simplified | Fix create-and-open behavior |
| Brainstorming | Experimental / Preview | Hide advanced whiteboard/template behavior |
| Kanban | Launch-visible but simplified | Only if New Board is fixed |
| Decisions | Launch-visible but simplified | Replace browser prompt with modal |
| Shares | Launch-visible but simplified | Keep basic access/share management only |

---

## 4. Critical Launch Blockers

### 4.1 Hide internal implementation files globally

Must be hidden from normal user views:

```text
.rustshare.json
events.jsonl
raw internal IDs
sidecar metadata files
internal package structures
```

Applies to:

- Folders
- Meeting Notes
- Standup Records
- Brainstorming
- Kanban
- Decisions
- Shares
- Home recent artifacts/activity

---

### 4.2 Fix create-and-open behavior

After creating any artifact, the user should land directly inside the created artifact.

| User action | Required behavior |
|---|---|
| New note | Creates and opens note |
| New meeting note | Creates and opens meeting note |
| New standup | Creates and opens standup record |
| New board | Creates and opens board |
| New idea board | Creates and opens idea board |
| New decision | Creates and opens decision with template |
| New share | Starts guided share flow before creating package |

---

### 4.3 Replace native browser prompts

Native browser prompts are not launch-safe.

Required replacement:

- Use RustShare-native modals.
- Apply immediately to Decisions.
- Reuse modal pattern for New Board, New Idea Board, and New Share.

---

### 4.4 Fix Kanban New Board

Kanban can be public-preview visible only if:

- New Board works.
- New Board opens the created board immediately.
- Basic cards can be created and moved.

If this cannot be fixed before preview, hide Kanban or mark it internal-only.

---

### 4.5 Clarify Workspace root

Recommended structure:

```text
My Files
└── Workspace
    ├── Notes
    ├── Meeting Notes
    ├── Standup Records
    ├── Kanban
    ├── Brainstorming
    └── Decisions

Library
├── Shared
├── Starred
├── Photos
└── Trash
```

Photos should remain a Library smart view, not a Workspace module.

---

## 5. Section-by-Section Instructions

---

## 5.1 Home

### Launch decision

**Launch-visible but simplified**

### Purpose

Home should summarize the workspace and route users into recent work.

### Keep

- Workspace summary
- Recent artifacts
- Recent activity
- Quick actions
- Pinned folders if useful

### Simplify

- Remove detailed Kanban board previews.
- Hide or simplify Brainstorming thumbnails unless stable.
- Remove or relocate floating settings icon.
- Avoid full module previews.
- Use one consistent card style.

### Required actions

- Convert Home from module dashboard to workspace overview.
- Do not expose metadata filenames.
- Replace raw names with clean display names.
- Use clear sections:
  - Workspace overview
  - Recent artifacts
  - Recent activity
  - Quick actions
  - Pinned folders

### Empty state

```text
No recent artifacts yet. Create a note, meeting record, decision, or board to start building your workspace memory.
```

### Acceptance criteria

- User understands current workspace status in 5 seconds.
- User can quickly create or open recent artifacts.
- No heavy workflow preview dominates the page.

---

## 5.2 Folders

### Launch decision

**Launch-visible but simplified**

### Purpose

Folders is the reliable file and artifact structure.

### Keep

- My Files
- Shared
- Starred
- Photos
- Trash
- Workspace folder tree
- Upload
- New folder
- Sorting
- List/grid toggle if stable

### Simplify

- Hide `.rustshare.json`, `events.jsonl`, and sidecar metadata.
- Remove empty Status column unless meaningful.
- Avoid duplicate “My Files” concepts.
- Keep Photos under Library only.

### Main columns

```text
Name
Type
Size
Modified
```

### Required actions

- Make Workspace clear as the module artifact root.
- Ensure internal files are hidden by default.
- Show clean user-facing files and folders only.

### Empty state

```text
This folder is empty. Upload a file or create a folder to start organizing your workspace.
```

### Acceptance criteria

- Users never see internal metadata files.
- Workspace module folders are clear.
- Files and artifacts can be opened reliably.

---

## 5.3 Notes

### Launch decision

**Launch-visible but simplified**

### Purpose

Notes should provide simple free-form, file-backed writing.

### Keep

- Notes list
- New note
- Note detail view
- Edit mode
- Open in Files
- Search/filter if stable
- Basic markdown support

### Simplify

- Reduce toolbar to essentials.
- Keep export/download only if stable.
- Avoid document-suite complexity.
- De-emphasize `.md` extension where possible.
- Clarify save/autosave behavior.

### Required actions

- Show clean titles.
- Add one-line excerpt if feasible.
- Handle untitled notes gracefully.
- Show clear save state:
  - `Saved`
  - `Saving`
  - `All changes saved`

### Empty state

```text
No notes yet. Create your first note to capture ideas, documentation, or working knowledge.
```

### Acceptance criteria

- User can create, edit, save, and reopen a note.
- New note opens immediately.
- Editor feels simple, not like a full document suite.

---

## 5.4 Meeting Notes

### Launch decision

**Launch-visible but simplified**

### Purpose

Meeting Notes should create and preserve simple meeting records.

### Keep

- Meeting records list
- New meeting note
- Meeting detail view
- Open in Files
- One default template

### Simplify

- Hide internal files:
  - `.rustshare.json`
  - `events.jsonl`
  - raw `index.md` package structure
- Do not show artifact package internals.
- Avoid calendar, transcript, recording, or automation features.
- Rename “Create from Template” to “New meeting note”.

### Minimum template

```markdown
# Meeting Notes

Date:
People:

## Agenda

## Notes

## Decisions

## Next steps
```

### Required actions

- Show each meeting as one clean record.
- Opening a new meeting note should open the created note immediately.
- Meeting Notes page should show records, not raw folders.

### Empty state

```text
No meeting notes yet. Create a meeting note to capture agenda, discussion, decisions, and follow-up items.
```

### Acceptance criteria

- One meeting = one clean visible artifact.
- New meeting opens immediately.
- Internal package files are hidden.

---

## 5.5 Standup Records

### Launch decision

**Launch-visible but simplified**

### Purpose

Standup Records should capture simple daily updates.

### Keep

- New standup
- Standup list
- Standup detail/read view
- Edit
- Open in Files

### Simplify

- No sprint reporting.
- No analytics.
- No team-performance view.
- Hide metadata files.

### Minimum template

```markdown
# Standup — [Date]

## Yesterday

## Today

## Blockers

## Follow-up
```

### Required actions

- Fix New Standup flow:
  1. Click New Standup.
  2. Create file under `My Files / Workspace / Standup Records`.
  3. Open created record immediately.
- Use clean title format:
  - `Standup — May 7, 2026`
- Breadcrumb:
  - `Standup Records > Standup — May 7, 2026`
- Hide `.rustshare.json`.

### Empty state

```text
No standup records yet. Create a daily update to capture progress, blockers, and follow-up items.
```

### Acceptance criteria

- User creates a standup and lands inside it immediately.
- Standup record is readable and editable.
- No metadata files are exposed.

---

## 5.6 Brainstorming

### Launch decision

**Experimental / Preview**

### Purpose

Brainstorming should provide simple visual idea boards.

### Keep

- Board gallery
- New idea board
- Open board
- Rename board
- Delete board
- Autosave
- Open in Files

### Simplify

- Hide template choices.
- Hide Excalidraw external links.
- Hide advanced whiteboard menu items where possible.
- Replace “No Preview” with clean placeholder.
- Avoid full Miro-like positioning.

### Required actions

- Mark section as Preview or Experimental.
- Rename “New Board” to “New idea board”.
- New board modal asks only:
  - Board name
- Use one default type:
  - Blank idea board
- Open created board immediately.

### Preferred wording

Section name may remain:

```text
Brainstorming
```

Alternative future name:

```text
Idea Boards
```

Helper:

```text
Capture sketches, flows, and early ideas as visual workspace boards.
```

### Empty state

```text
No idea boards yet. Create a simple visual board to capture sketches, flows, or early thinking.
```

### Acceptance criteria

- Feature is clearly preview/experimental.
- User can create/open/save a basic board.
- External Excalidraw branding/links are not visible in normal UI.

---

## 5.7 Kanban

### Launch decision

**Launch-visible but simplified only after New Board is fixed**

If New Board remains broken: **Internal only for Public Preview**

### Purpose

Kanban should provide lightweight boards with movable cards.

### Keep

- Board list
- New board
- Open board
- Rename board
- Delete board
- Add card
- Move card between columns
- Edit card title
- Edit short description

### Simplify

- Hide attachments unless stable.
- Hide checklists unless stable.
- Hide activity if it exposes raw IDs.
- Avoid Jira/Trello-like details.
- Avoid workflow builder.
- Avoid duplicate priority labels.

### Minimum columns

Preferred:

```text
Backlog
In Progress
Done
```

Optional:

```text
Backlog
Ready
In Progress
Done
```

### Minimum card fields

```text
Title
Short description
Optional label
Optional assignee initials
```

### Required actions

- Fix New Board:
  1. Click New Board.
  2. Enter board name.
  3. Create board.
  4. Open new board immediately.
- Rename “Kanban Dashboard” to “Kanban”.
- Simplify card drawer.
- Remove internal IDs from activity.

### Empty state

```text
No boards yet. Create a lightweight board to organize work, ideas, or follow-up items.
```

### Acceptance criteria

- User can create and open board.
- User can create and move cards.
- Card details are simple.
- No Jira/Trello complexity dominates the UI.

---

## 5.8 Decisions

### Launch decision

**Launch-visible but simplified**

### Purpose

Decisions should create durable decision records with context and rationale.

### Keep

- Decision list
- New decision
- Open decision
- Edit decision
- Open in Files
- Export/download only if stable

### Simplify

- Replace browser prompt.
- Avoid ADR bureaucracy.
- Avoid approval workflows.
- Avoid complex status systems.
- Make it more than an empty Notes clone by inserting a basic template.

### Minimum template

```markdown
# Decision: [Title]

## Context

## Decision

## Reason

## Follow-up

## Date
```

### Required actions

- Replace browser prompt with RustShare modal.
- New decision opens immediately.
- Insert default decision template automatically.
- Show clean display title while keeping stable filename internally.
- Hide unstable export actions.

### Empty state

```text
No decisions yet. Create a decision record to preserve context, rationale, and follow-up.
```

### Acceptance criteria

- New decision uses modal.
- New decision opens immediately.
- Template is inserted.
- List shows clear title and summary.

---

## 5.9 Shares

### Launch decision

**Launch-visible but simplified**

### Purpose

Shares should show what has been shared, who can access it, and provide basic share actions.

### Keep

- Share list
- New share
- Open in Files
- Share details panel/modal
- Copy link if supported
- Revoke share if supported

### Simplify

- Avoid “public” wording unless public sharing is fully audited.
- Hide internal share package structure.
- Avoid vague names like `test`.
- Do not expose `files` / `README.md` internals.
- Avoid complex external collaboration management.

### Minimum share item fields

```text
Share name
File/folder type
Access level
People count or link status
Created/updated date
```

### Required actions

- New Share flow asks what item to share before creating package.
- Add simple share details modal/panel.
- Show access metadata.
- Hide internal package files.

### New share flow

```text
1. Choose item
2. Set access
3. Review and share
```

### Empty state

```text
No active shares. Share a file or folder when you are ready.
```

### Acceptance criteria

- User understands what is shared.
- User understands who can access it.
- User can revoke or manage share if supported.
- No internal package structure is exposed.

---

## 6. Implementation Priority

### Priority 0 — Blockers

1. Hide internal metadata files globally.
2. Fix create-and-open behavior.
3. Replace native browser prompts.
4. Fix Kanban New Board or hide Kanban.
5. Clarify Workspace root.

### Priority 1 — Core Launch UX

1. Simplify Home.
2. Stabilize Folders.
3. Simplify Notes editor/list.
4. Simplify Meeting Notes.
5. Fix Standup creation flow.
6. Simplify Decisions creation.
7. Simplify Shares list/details.

### Priority 2 — Preview / Experimental

1. Mark Brainstorming as Preview/Experimental.
2. Hide Excalidraw external links.
3. Simplify New idea board flow.

### Priority 3 — Polish

1. Empty states.
2. Consistent labels.
3. Breadcrumb cleanup.
4. Clean titles.
5. Consistent primary actions.
6. Mobile/responsive check.

---

## 7. Screenshot References

The uploaded screenshots are current-state references, not final design targets.

| File | Section |
|---|---|
| 01-home-overview.png | Home |
| 02-folders-files.png | Folders |
| 03-folders-with-photos-view.png | Folders / Photos |
| 04-notes.png | Notes |
| 05-meeting-notes.png | Meeting Notes |
| 06-standup-records.png | Standup Records |
| 07-brainstorming.png | Brainstorming |
| 08-kanban.png | Kanban |
| 09-decisions.png | Decisions |
| 10-shares.png | Shares |

Use them to verify what needs to be simplified, hidden, or stabilized.

---

## 8. Final Launch Checklist

### Global

- [ ] Internal metadata hidden globally
- [ ] Raw IDs hidden from normal UI
- [ ] Browser prompts replaced with RustShare modals
- [ ] Create-and-open behavior works globally
- [ ] Workspace folder root is clear
- [ ] Empty states added
- [ ] Labels and breadcrumbs cleaned
- [ ] Brainstorming marked Preview/Experimental
- [ ] No unstable features overclaimed

### Home

- [ ] Workspace overview simplified
- [ ] Recent artifacts shown
- [ ] Recent activity shown
- [ ] Quick actions shown
- [ ] Detailed Kanban/Brainstorming previews hidden

### Folders

- [ ] Metadata files hidden
- [ ] Clean columns shown
- [ ] Workspace tree visible
- [ ] Photos remains Library smart view
- [ ] Upload/New Folder work

### Notes

- [ ] New note works
- [ ] Note opens after creation
- [ ] Minimal editor shown
- [ ] Save state clear
- [ ] Clean note titles shown

### Meeting Notes

- [ ] New meeting note works
- [ ] Created meeting opens immediately
- [ ] Internal files hidden
- [ ] Meeting records shown as clean artifacts
- [ ] Default template inserted

### Standup Records

- [ ] New standup works
- [ ] Created standup opens immediately
- [ ] Clean title format used
- [ ] Correct breadcrumb shown
- [ ] Metadata hidden

### Brainstorming

- [ ] Marked Preview/Experimental
- [ ] New idea board works
- [ ] Created board opens immediately
- [ ] Excalidraw external links hidden
- [ ] No Preview placeholders improved

### Kanban

- [ ] New Board fixed
- [ ] New board opens immediately
- [ ] 3–4 simple columns
- [ ] Simple cards only
- [ ] Card drawer simplified
- [ ] Internal IDs hidden

### Decisions

- [ ] New decision modal implemented
- [ ] Browser prompt removed
- [ ] New decision opens immediately
- [ ] Default decision template inserted
- [ ] Clean decision titles shown

### Shares

- [ ] New share flow asks item first
- [ ] Share list shows access metadata
- [ ] Share details panel/modal added
- [ ] Copy/revoke actions visible if supported
- [ ] Internal package structure hidden

---

## 9. Final Recommendation

Launch RustShare with the existing module structure, but reduce each module to the smallest credible public-preview version.

The product should communicate:

```text
RustShare is a lightweight company-memory workspace.
It keeps files, notes, meetings, standups, idea boards, kanban boards, decisions, and shares as durable work context.
```

The technical team should prioritize:

```text
stability
clean artifact presentation
simple create/open/edit flows
honest preview scope
```

over adding new functionality.
