# RustShare Public Preview UI Review Resource

**Purpose:** This file compiles the section-by-section UI review guidance and sample image directions from the current RustShare alpha UI review conversation. It is intended to be uploaded into a new chat as context for producing a concise handover report, technical implementation prompts, or launch-scope UI tasks.

**Product frame:** RustShare is the durable company memory / artifact layer in Kubedo’s Company Memory Infrastructure direction.

**Launch principle:** Minimum credible functionality. Simple, honest, useful.

**Public Preview positioning:** RustShare should feel like a lightweight company-memory workspace, not like Jira, Confluence, Notion, Trello, or Miro.

---

## Global Launch Rules

1. Do not remove features by default.
2. Do not expand features for launch.
3. Decide what should be visible, simplified, hidden, marked experimental, or postponed.
4. Keep all modules connected to the file-backed workspace model.
5. Hide internal implementation details from normal users.
6. Avoid exposing `.rustshare.json`, `events.jsonl`, UUIDs, or raw metadata files.
7. After creating an artifact, open the created artifact immediately.
8. Avoid native browser prompts. Use RustShare-native modals.
9. Avoid workflow-heavy UI that makes RustShare feel like Jira, Confluence, Notion, Trello, or Miro.
10. Prefer clean records, readable titles, clear empty states, and stable file-backed behavior.

---

# Section Reviews

---

## 1. Home

### Quick judgment
**Keep but simplify**

### Simplest launch purpose
Home should show where recent company-memory artifacts are, what changed recently, and provide quick entry points into the main workspace sections.

### What works
- Shows lightweight previews for Kanban, Meetings, Notes, Brainstorming, and Shares.
- `/Workspace/...` labels reinforce the file-backed artifact model.
- Calm design, not too SaaS-heavy.
- Search bar is useful if reliable.
- Sidebar is minimal.

### Too much for launch
- Feels like a loose collection of module previews rather than a workspace summary.
- Kanban preview is too detailed and makes the product feel Jira/Trello-like.
- Brainstorming thumbnails feel rough and unfinished.
- “Recent visual decision boards” overlaps Brainstorming and Decisions.
- Floating settings icon feels misplaced.
- Some wording is inconsistent.

### Minimum launch version
Home should have only:
1. **Workspace summary**
2. **Recent artifacts**
3. **Quick actions**

Avoid full module previews on Home.

### Required changes
- Simplify Home from module dashboard to workspace overview.
- Hide detailed Kanban columns from Home.
- Hide or simplify Brainstorming thumbnails unless stable.
- Move or remove floating settings icon.
- Use one consistent card style.
- Do not expose `.rustshare.json` or metadata filenames.
- Replace raw names with clean display names.

### Wording recommendations
- **Workspace overview**
- **Recent artifacts**
- **Recent activity**
- **Quick actions**
- **Shared items**
- Empty state: “No recent artifacts yet. Create a note, meeting record, decision, or board to start building your workspace memory.”

### Relationship to other sections
Home should not be an independent module. It should summarize and route into Folders, Notes, Meeting Notes, Decisions, Kanban, Brainstorming, and Shares.

### Final launch decision
**Launch-visible but simplified**

### Sample image direction
A clean workspace overview with:
- Summary cards: total artifacts, updated this week, shared items, storage used
- Recent artifacts list
- Recent activity list
- Quick actions
- Pinned folders
- No detailed Kanban columns or full module previews

---

## 2. Folders

### Quick judgment
**Keep but simplify**

### Simplest launch purpose
Folders should be the reliable file and artifact structure where users can browse, upload, open, and organize RustShare workspace content.

### What works
- Core to RustShare’s identity.
- Left folder tree is useful.
- Table layout is practical.
- New folder, Upload, sorting, and view toggles are appropriate.
- Library section with Shared, Starred, Photos, and Trash is useful.
- Fits durable artifact layer positioning.

### Too much for launch
- Internal metadata files are visible.
- `*.rustshare.json` must not be shown.
- Module folders under My Files can confuse users unless Workspace is clearly the root.
- “Folders and files, tuned for quick scanning instead of dashboard theater” is not launch-safe copy.
- Status column is empty.
- Duplicate “My Files” concepts may confuse users.
- Pagination feels heavy but acceptable if visually quiet.

### Minimum launch version
Left sidebar:
- My Files
- Shared
- Starred
- Photos
- Trash

Workspace tree:
- My Files / Workspace / Notes
- My Files / Workspace / Meeting Notes
- My Files / Workspace / Standup Records
- My Files / Workspace / Kanban
- My Files / Workspace / Brainstorming
- My Files / Workspace / Decisions

Main file list:
- Name
- Type
- Size
- Modified

Basic actions:
- New folder
- Upload
- Sort
- List/grid toggle
- Open, rename, delete/move to trash

### Required changes
- Hide `.rustshare.json`, `events.jsonl`, and metadata files.
- Make Workspace the clear root for module artifacts.
- Remove empty Status column unless meaningful.
- Hide duplicate sidecar files.
- Clarify Photos as a Library smart view, not a Workspace module.
- Keep Photos under Library, not under Workspace.

### Wording recommendations
- Header: **Files** or **My Files**
- Helper: **Browse and organize files, folders, and workspace artifacts.**
- Empty folder: “This folder is empty. Upload a file or create a folder to start organizing your workspace.”

### Relationship to other sections
Folders is foundational and independent. Other modules are lightweight views over file-backed artifacts.

### Final launch decision
**Launch-visible but simplified**

### Sample image direction
A clean file manager with:
- Sidebar: My Files, Shared, Starred, Photos, Trash
- Workspace folder tree
- Main list showing only user-facing folders/files
- Hidden metadata files
- Clean path breadcrumbs
- No `.rustshare.json` visible

---

## 3. Notes

### Quick judgment
**Keep but simplify**

### Simplest launch purpose
Notes should let users create, browse, read, and edit simple file-backed notes inside the workspace.

### What works
- Strong core section.
- Fits company-memory positioning.
- List screen is understandable.
- Detail screen is clean.
- Markdown-oriented editing fits file-backed model.
- Open in Files is very good.
- Read/Edit distinction is useful if clear.

### Too much for launch
- Toolbar risks becoming too document-suite-like.
- Export options should stay only if stable.
- Messy filenames weaken credibility.
- List feels too much like raw file dump.
- Save/Read/Edit states are ambiguous.
- New note screen feels too empty.

### Minimum launch version
Notes list:
- Note title
- Last modified date
- Optional excerpt
- New note
- Open in Files
- Search/filter if stable

Note view:
- Title
- Content
- Edit
- Open in Files
- Export/download only if stable

Note edit:
- Basic markdown editor
- Minimal toolbar
- Clear autosave or save behavior

### Required changes
- Show clean note names.
- De-emphasize `.md` where possible.
- Add one-line preview/excerpt if possible.
- Handle untitled notes gracefully.
- Clarify autosave/save behavior.
- Reduce toolbar to essentials.
- Keep export only if reliable.

### Wording recommendations
- Helper: **Write and keep file-backed notes in your workspace.**
- Button: **New note**
- Empty state: “No notes yet. Create your first note to capture ideas, documentation, or working knowledge.”
- Save status: **All changes saved**

### Relationship to other sections
Notes is foundational. Meeting Notes, Standup Records, and Decisions should behave like specialized Notes views.

### Final launch decision
**Launch-visible but simplified**

### Sample image direction
A notes UI with:
- Notes list with clean titles and excerpts
- Note viewer with title, author/date metadata, content
- New note editor with minimal toolbar
- Clear Write/Preview or Edit/Read modes
- Open in Files action

---

## 4. Meeting Notes

### Quick judgment
**Keep but simplify**

### Simplest launch purpose
Meeting Notes should let users create and read simple meeting records with agenda, attendees, notes, decisions, and action items.

### What works
- Good default structure:
  - Meeting
  - Agenda
  - Attendees
  - Notes
  - Decisions
  - Action Items
- Open in Files is useful.
- Folder-backed artifact model is acceptable.

### Too much for launch
- User sees internal files:
  - `events.jsonl`
  - `.rustshare.json`
  - `index.md`
- “Create from Template” sounds too heavy.
- Landing page shows folders instead of meeting records.
- Internal artifact package leaks into UI.

### Minimum launch version
Meeting Notes should show one clean record per meeting:
- Meeting title
- Date
- Attendees count
- Last updated
- Open/Edit

A meeting artifact can be folder-backed internally, but the user sees one meeting note.

### Required changes
- Hide `.rustshare.json` and `events.jsonl`.
- Show meeting-note folder as a single artifact item.
- Rename **Create from Template** to **New meeting note**.
- Use one default meeting note template.
- Show meeting records directly on the Meeting Notes page.
- Opening a new meeting note should open the created note immediately.

### Wording recommendations
- Helper: **Record simple meeting notes, decisions, and follow-up items.**
- Button: **New meeting note**
- Section label: **Meeting records**
- Empty state: “No meeting notes yet. Create a meeting note to capture agenda, discussion, decisions, and follow-up items.”

### Relationship to other sections
Meeting Notes is a specialized Notes view backed by Folders.

### Final launch decision
**Launch-visible but simplified**

### Sample image direction
A meeting notes UI with:
- Meeting record list
- New meeting note modal/editor
- Meeting detail view with agenda, attendees, notes, decisions, action items
- Files view shows one clean artifact or hides internal files

---

## 5. Standup Records

### Quick judgment
**Keep but simplify**

### Simplest launch purpose
Standup Records should let users create and review simple daily updates: yesterday, today, blockers, and follow-up items.

### What works
- Good fit for lightweight company-memory artifacts.
- New Standup and Open in Files actions are understandable.
- Consistent with Notes and Meeting Notes.

### Too much for launch
- New Standup currently sends the user to My Files instead of opening the new standup.
- Metadata files are visible.
- Raw filenames are messy.
- Editor breadcrumb says Notes instead of Standup Records.
- Current flow requires too much navigation after creation.

### Minimum launch version
Standup Records:
- New standup
- List daily standup entries
- Open selected standup
- Edit selected standup
- Open in Files

Default template:
```markdown
# Standup — May 7, 2026

## Yesterday

## Today

## Blockers

## Follow-up
```

### Required changes
- Fix New Standup flow:
  - Click New Standup
  - Create file under My Files / Workspace / Standup Records
  - Open the created record immediately
- Hide `.rustshare.json`.
- Show clean titles like **Standup — May 7, 2026**.
- Use consistent naming: **Standup Records**.
- Breadcrumb should say **Standup Records > Standup — May 7, 2026**.

### Wording recommendations
- Helper: **Capture simple daily updates, blockers, and follow-up items.**
- Button: **New standup**
- Empty state: “No standup records yet. Create a daily update to capture progress, blockers, and follow-up items.”

### Relationship to other sections
Standup Records is a specialized Notes view backed by Folders.

### Final launch decision
**Launch-visible but simplified**

### Sample image direction
A standup UI with:
- Standup Records list
- New standup opens directly
- Editor with Yesterday / Today / Blockers / Follow-up
- Read view
- Files view with hidden metadata files

---

## 6. Brainstorming

### Quick judgment
**Mark as experimental/preview**

### Simplest launch purpose
Brainstorming should let users create and save simple visual idea boards inside the workspace.

### What works
- Board gallery with thumbnails is useful.
- File-backed path is aligned with artifact model.
- Editor appears functional.
- Autosave state is important.
- Open in Files remains useful.

### Too much for launch
- Feels close to a full Miro-like product.
- Helper text overlaps with Decisions.
- Template choices are too much:
  - Blank Board
  - Decision Making & Brainstorming
  - Meeting Whiteboard
- Excalidraw-branded menu links should not be visible:
  - GitHub
  - Follow us
  - Discord chat
- Advanced menu items should be hidden.
- Rough board names and No Preview cards feel unfinished.

### Minimum launch version
Brainstorming:
- List visual boards
- New idea board
- Open board
- Rename board
- Delete board
- Autosave board
- Open in Files

Creation should ask only:
- Board name
- Create board

Use one default type:
- Blank idea board

### Required changes
- Rename **New Board** to **New idea board**.
- Remove/hide template choices.
- Hide Excalidraw-branded links.
- Hide or simplify advanced menu items.
- Replace No Preview with clean placeholder.
- After New idea board, open the created board immediately.
- Mark as Preview/Experimental.

### Wording recommendations
- Consider section name: **Idea Boards**
- If keeping Brainstorming, helper: **Capture sketches, flows, and early ideas as visual workspace boards.**
- Button: **New idea board**
- Empty state: “No idea boards yet. Create a simple visual board to capture sketches, flows, or early thinking.”

### Relationship to other sections
Brainstorming is a visual artifact view connected to Folders. It should not merge with Decisions.

### Final launch decision
**Experimental/preview**

### Sample image direction
A Brainstorming preview UI with:
- Board gallery
- New idea board modal with only board name
- Simple whiteboard editor
- Clean thumbnail placeholders
- Open in Files
- No Excalidraw external links visible

---

## 7. Kanban

### Quick judgment
**Keep but simplify**

### Simplest launch purpose
Kanban should let users create simple file-backed boards with cards that can move between columns.

### What works
- Board overview is understandable.
- Board detail has familiar columns.
- Card drawer supports useful details.
- All Boards button is good.
- Description editor is useful.

### Too much for launch
- Card drawer is too Trello/Jira-like.
- Labels, assignees, attachments, checklists, and activity together are too much.
- Activity shows internal IDs.
- Five columns may be too much for minimum version.
- New Board is currently broken, which is a launch blocker.

### Minimum launch version
Kanban:
- Board list
- New board
- Open board
- Rename board
- Delete board
- Add card
- Move card between columns
- Edit card title
- Edit short description

Columns:
- Backlog
- In Progress
- Done

or at most:
- Backlog
- Ready
- In Progress
- Done

Card:
- Title
- Short description
- Optional label
- Optional assignee initials

### Required changes
- Fix New Board before launch.
- New Board flow:
  - Click New Board
  - Enter board name
  - Create board
  - Open the new board immediately
- Simplify card drawer.
- Hide/postpone attachments, checklists, and activity unless stable.
- Remove internal IDs from activity.
- Rename Kanban Dashboard to Kanban.
- Avoid duplicated priority labels.

### Wording recommendations
- Helper: **Organize lightweight work boards in your workspace.**
- Button: **New board**
- Empty state: “No boards yet. Create a lightweight board to organize work, ideas, or follow-up items.”
- New board modal:
  - Board name
  - e.g. Product launch checklist
  - Create board

### Relationship to other sections
Kanban is a lightweight board view over file-backed artifacts. It should not become Jira/Trello replacement.

### Final launch decision
**Launch-visible but simplified — only after fixing New Board**

If New Board remains broken, Kanban should be **Internal only**.

### Sample image direction
A Kanban UI with:
- Board list
- New board modal
- Board detail with 3–4 columns
- Simple card detail modal
- No Jira-like reporting
- No raw IDs
- No duplicated labels

---

## 8. Decisions

### Quick judgment
**Keep but simplify**

### Simplest launch purpose
Decisions should let users record simple, durable decision records with context, decision, reason, and follow-up.

### What works
- Strong fit for company-memory positioning.
- Simple list view.
- Reusing Notes editor is acceptable.
- Open in Files is useful.
- Filename pattern like `DEC-0001-title.md` is a good direction.

### Too much for launch
- Native browser prompt is not launch-safe.
- Detail page currently opens as mostly empty note.
- Editor feels like Notes clone without decision-specific structure.
- List does not show enough decision-specific information.
- Avoid full ADR bureaucracy.

### Minimum launch version
Decisions:
- List decision records
- New decision
- Open decision
- Edit decision
- Export/download if stable
- Open in Files

Default template:
```markdown
# Decision: [Title]

## Context

## Decision

## Reason

## Follow-up

## Date
```

### Required changes
- Replace native browser prompt with RustShare modal.
- After creating decision, open it immediately.
- Insert default decision template automatically.
- Show clean display title while keeping stable filename internally.
- Hide advanced export if unstable.

### Wording recommendations
- Helper: **Record important decisions with context and rationale.**
- Button: **New decision**
- Empty state: “No decisions yet. Create a decision record to preserve context, rationale, and follow-up.”

### Relationship to other sections
Decisions is a specialized Notes view backed by Folders. Brainstorming may lead to Decisions, but they should not merge.

### Final launch decision
**Launch-visible but simplified**

### Sample image direction
A Decisions UI with:
- Decision list with DEC numbers, title, summary
- New decision modal
- New decision opens with template
- Read/edit views
- Context, Decision, Reason, Follow-up, Date sections

---

## 9. Shares

### Quick judgment
**Keep but hide advanced parts**

### Simplest launch purpose
Shares should show files or folders that have been shared, who can access them, and allow basic share creation/removal.

### What works
- Empty state is clean.
- New Share and Open in Files are right actions.
- Simple cards are better than complex permission dashboard.
- File-backed share packages may work internally.

### Too much for launch
- “Public and internal share packages” is too much and security-sensitive.
- Public sharing implies serious security expectations.
- Vague share names like `test` are not useful.
- Internal share package structure leaks in Files:
  - `files`
  - `README.md`
- Most important share info is missing:
  - What is shared?
  - Who can access it?
  - Is it public or private?
  - Can I revoke it?

### Minimum launch version
Share list item should show:
- Share name
- File/folder type
- Access level
- People count or link status
- Created/updated date

Basic actions:
- Open
- Copy link
- View details
- Revoke share

Avoid complex permission models.

### Required changes
- Avoid “public” unless public sharing is fully audited and safe.
- Use meaningful share names.
- Add metadata to share cards.
- Add simple share details modal.
- Hide internal package structure in Shares UI.
- New Share flow should ask what to share before creating a package.

### Wording recommendations
- Helper: **Manage items shared from your workspace.**
- Button: **New share**
- Empty state: “No active shares. Share a file or folder when you are ready.”
- Modal flow:
  1. Choose item
  2. Set access
  3. Review and share

### Relationship to other sections
Shares is not a content module. It is a permission/access view over Folders and workspace artifacts.

### Final launch decision
**Launch-visible but simplified**

### Sample image direction
A Shares UI with:
- Share list
- Filters: All shares / Internal / Links
- Share details side panel
- People with access
- Activity
- Copy link
- Revoke share
- New Share stepper: choose item, set access, review and share

---

# Cross-Section Launch Decisions

| Section | Final decision |
|---|---|
| Home | Launch-visible but simplified |
| Folders | Launch-visible but simplified |
| Notes | Launch-visible but simplified |
| Meeting Notes | Launch-visible but simplified |
| Standup Records | Launch-visible but simplified |
| Brainstorming | Experimental/preview |
| Kanban | Launch-visible but simplified only after New Board is fixed |
| Decisions | Launch-visible but simplified |
| Shares | Launch-visible but simplified |

---

# Critical Launch Blockers

1. **Hide internal metadata files**
   - `.rustshare.json`
   - `events.jsonl`
   - raw internal IDs
   - sidecar metadata files

2. **Fix create-and-open behavior**
   - New note should open the new note.
   - New meeting note should open the new meeting note.
   - New standup should open the new standup record.
   - New board should create and open the board.
   - New decision should open the new decision with template.
   - New share should guide through item selection and access settings.

3. **Replace native browser prompts**
   - Decisions currently uses browser prompt.
   - Must use RustShare modal.

4. **Clarify module-folder relationship**
   - Folders are source of truth.
   - Modules are lightweight views over workspace artifacts.
   - Workspace should be the clear root:
     - My Files / Workspace / Notes
     - My Files / Workspace / Meeting Notes
     - My Files / Workspace / Standup Records
     - My Files / Workspace / Kanban
     - My Files / Workspace / Brainstorming
     - My Files / Workspace / Decisions

5. **Reduce workflow-heavy UI**
   - Avoid Trello/Jira behavior in Kanban.
   - Avoid Miro behavior in Brainstorming.
   - Avoid Confluence/Notion behavior in Notes/Meeting Notes.
   - Avoid ADR bureaucracy in Decisions.
   - Avoid complex permission dashboard in Shares.

---

# Suggested Public Preview Navigation

```text
Home
Folders
Notes
Meeting Notes
Standup Records
Brainstorming    [Preview]
Kanban
Decisions
Shares
```

Alternative naming if you want even clearer launch language:

```text
Home
Files
Notes
Meeting Notes
Standups
Idea Boards      [Preview]
Kanban
Decisions
Shares
```

---

# Recommended Workspace Folder Structure

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

**Photos should stay under Library as a smart view**, not as a Workspace module.

---

# Recommended Artifact Visibility Rule

Internally, RustShare may store an artifact as:

```text
artifact-folder/
├── index.md
├── .rustshare.json
├── events.jsonl
└── attachments/
```

But users should normally see:

```text
Artifact title
Type
Updated date
```

Not the internal implementation files.

---

# Suggested Unified Empty States

## Home
**No recent artifacts yet**  
Create a note, meeting record, decision, or board to start building your workspace memory.

## Folders
**This folder is empty**  
Upload a file or create a folder to start organizing your workspace.

## Notes
**No notes yet**  
Create your first note to capture ideas, documentation, or working knowledge.

## Meeting Notes
**No meeting notes yet**  
Create a meeting note to capture agenda, discussion, decisions, and follow-up items.

## Standup Records
**No standup records yet**  
Create a daily update to capture progress, blockers, and follow-up items.

## Brainstorming
**No idea boards yet**  
Create a simple visual board to capture sketches, flows, or early thinking.

## Kanban
**No boards yet**  
Create a lightweight board to organize work, ideas, or follow-up items.

## Decisions
**No decisions yet**  
Create a decision record to preserve context, rationale, and follow-up.

## Shares
**No active shares**  
Share a file or folder when you are ready.

---

# Suggested Sample Image Concepts Already Generated in Conversation

These mockup directions were generated during the conversation and can be recreated or refined later:

1. **Home mockup**
   - Workspace overview
   - Summary cards
   - Recent artifacts
   - Quick actions
   - Pinned folders

2. **Folders mockup**
   - Clean file manager
   - Library smart views
   - Workspace folder tree
   - Metadata hidden

3. **Notes mockup**
   - Notes list
   - Note detail
   - New note editor
   - Minimal markdown toolbar

4. **Meeting Notes mockup**
   - Meeting records list
   - New meeting note editor
   - Meeting detail view
   - Folder-backed artifact shown cleanly

5. **Standup Records mockup**
   - Standup list
   - Standup editor
   - Standup read view
   - Create action opens created record

6. **Brainstorming mockup**
   - Idea board gallery
   - New idea board modal
   - Simplified whiteboard editor
   - Preview/experimental feel

7. **Kanban mockup**
   - Board list
   - New board modal
   - Lightweight columns and cards
   - Simplified card details

8. **Decisions mockup**
   - Decision list
   - New decision modal
   - Decision template
   - Read/edit decision views

9. **Shares mockup**
   - Share list
   - New share flow
   - Share details panel
   - Copy link and revoke share actions

---

# Recommended Next Chat Prompt

Use this prompt in a new chat together with this file:

```text
We are preparing RustShare alpha UI for a Public Preview / Open Source Preview launch.

I uploaded a resource file containing section-by-section UI review decisions, launch simplification guidance, and sample image directions from a previous review conversation.

Please convert it into a concise handover report for the technical team.

The report should include:
1. Executive summary
2. Public Preview UI principles
3. Section-by-section launch decisions
4. Critical launch blockers
5. Required UI/UX changes by priority
6. Artifact visibility rules
7. Recommended workspace folder structure
8. Final launch scope
9. Optional appendix with sample screen directions

Keep it direct, technical-team friendly, and implementation-oriented.
Do not create implementation prompts yet.
```

---

# End of Resource File
