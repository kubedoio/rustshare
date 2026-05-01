# Specification: Module Integration

## Shared Editor Rule

All modules that edit Markdown content use the shared Rich Markdown Editor.

## Notes

`/Notes/{noteSlug}/index.md`, `.rustshare.json`, `attachments/`. New/open note uses `MarkdownDocumentPage`.

## Meeting Notes

`/Meetings/{team}/{date-meeting-slug}/index.md`, `.rustshare.json`, `events.jsonl`, `attachments/`. Meeting template includes attendees, agenda, notes, decisions, action items.

## Decisions

`/Decisions/{category}/{decisionSlug}/index.md`, `.rustshare.json`, `attachments/`. Metadata stores status, owner, approvers and links. Content stores context, decision, alternatives and consequences.

## Standups

Daily folder/file uses rich editor or simplified Markdown form.

## Kanban Cards

Card detail uses editor for `index.md`; card metadata remains separate.

## Brainstorming

README.md uses editor; board.excalidraw remains separate.

## File Browser

Opening `.md` uses rich editor/viewer. `.txt` keeps plain text editor. Raw Markdown fallback should exist where feasible.
