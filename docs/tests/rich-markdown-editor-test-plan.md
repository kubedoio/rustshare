# Test Plan: Rich Markdown Editor

## Load and Render

Load simple Markdown; render headings, paragraphs, lists, tables, code blocks, images and links.

## Edit and Save

Write user can edit; read-only user cannot; save updates Markdown; autosave works; manual save works; failed save preserves content.

## Formatting Round Trip

H1/H2/H3, bold, italic, underline, bullet list, numbered list, task list, blockquote, inline code, code block, horizontal rule, link, image, table.

## Slash Commands

`/` opens menu, filters, supports keyboard navigation, inserts expected blocks.

## Attachments

Image upload stores in attachments and inserts relative Markdown; file upload stores in attachments and inserts link/card; hidden metadata not listed; invalid filenames and path traversal rejected. Denied delete of a non-attachment file must not create an attachments folder or remove the unrelated file.

## Export

Printable view excludes chrome; PDF print flow opens; Markdown export works; images/tables render in printable view.

## Module Integration

Notes, Decisions, Meeting Notes and Markdown files use the editor. `.txt` still uses plain text editor.

## Security

Sanitize scripts, onclick, unsafe links; public share read-only; unauthorized edit/upload blocked; hidden files not exposed.
