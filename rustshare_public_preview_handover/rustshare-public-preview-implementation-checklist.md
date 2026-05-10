# RustShare Public Preview Implementation Checklist

## Priority 0 — Must Fix

- [ ] Hide `.rustshare.json` globally
- [ ] Hide `events.jsonl` globally
- [ ] Hide raw UUIDs/internal IDs
- [ ] Hide sidecar metadata files
- [ ] Replace browser-native prompts with RustShare modals
- [ ] Ensure all newly created artifacts open immediately
- [ ] Fix Kanban New Board or hide Kanban from public preview
- [ ] Make `My Files / Workspace` the clear root for module artifacts

## Create-and-Open Flows

- [ ] New note creates and opens note
- [ ] New meeting note creates and opens meeting note
- [ ] New standup creates and opens standup record
- [ ] New board creates and opens board
- [ ] New idea board creates and opens idea board
- [ ] New decision creates and opens decision with template
- [ ] New share starts guided flow before creating share package

## Home

- [ ] Simplify to workspace overview
- [ ] Show recent artifacts
- [ ] Show recent activity
- [ ] Show quick actions
- [ ] Hide detailed Kanban preview
- [ ] Hide rough Brainstorming thumbnails
- [ ] Remove or relocate floating settings icon

## Folders

- [ ] Hide internal metadata files
- [ ] Show columns: Name, Type, Size, Modified
- [ ] Remove empty Status column
- [ ] Show Workspace tree clearly
- [ ] Keep Shared, Starred, Photos, Trash as Library views
- [ ] Verify Upload works
- [ ] Verify New Folder works

## Notes

- [ ] Show clean note titles
- [ ] Add excerpt preview if feasible
- [ ] Clarify save/autosave state
- [ ] Reduce toolbar to essentials
- [ ] Keep export only if stable
- [ ] New note opens immediately

## Meeting Notes

- [ ] Show clean meeting records
- [ ] Rename Create from Template to New meeting note
- [ ] Insert default meeting template
- [ ] Created meeting opens immediately
- [ ] Hide internal artifact package files

## Standup Records

- [ ] New standup creates under Workspace / Standup Records
- [ ] Created standup opens immediately
- [ ] Use clean title: Standup — [Date]
- [ ] Breadcrumb uses Standup Records
- [ ] Hide metadata files

## Brainstorming

- [ ] Mark as Preview/Experimental
- [ ] Rename New Board to New idea board
- [ ] Remove template choices
- [ ] Hide Excalidraw external links
- [ ] Replace No Preview placeholders
- [ ] New idea board opens immediately

## Kanban

- [ ] Fix New Board
- [ ] New board opens immediately
- [ ] Rename Kanban Dashboard to Kanban
- [ ] Use 3–4 simple columns
- [ ] Simplify card drawer
- [ ] Hide attachments/checklists/activity unless stable
- [ ] Remove raw IDs

## Decisions

- [ ] Replace browser prompt with modal
- [ ] Insert default decision template
- [ ] Created decision opens immediately
- [ ] Show clean title
- [ ] Hide unstable export actions
- [ ] Avoid approval/ADR workflow complexity

## Shares

- [ ] New Share asks item first
- [ ] Add access metadata to share cards
- [ ] Add details modal/panel
- [ ] Add Copy link if supported
- [ ] Add Revoke share if supported
- [ ] Hide internal package structure
- [ ] Avoid “public” wording unless audited

## Final QA

- [ ] All modules understandable in 5 seconds
- [ ] All create flows tested
- [ ] No internal metadata visible
- [ ] No raw IDs visible
- [ ] No browser prompts
- [ ] Empty states present
- [ ] Breadcrumbs clean
- [ ] Mobile/responsive checked
