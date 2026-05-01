# Specification: Docmost-like Editor UX for RustShare

## Goal

Create a smooth, modern, Docmost-like editing experience without cloning Docmost or abandoning RustShare's file-backed model.

## UX Principles

- Read mode first for finished documents.
- Edit mode is one click away for users with write permission.
- Writing surface is calm and uncluttered.
- Toolbar is useful but restrained.
- Slash menu is the primary power-user entry point.
- Attachments feel native.
- Images insert inline smoothly.
- Save state is always visible.
- Export is easy to find.
- Raw Markdown remains available as fallback.

## Layout

Top bar: Back, title, path/module label, save status, read/edit toggle, export, more actions.

Main area: clean read mode or rich editor canvas.

Side/bottom panel: attachments, metadata, recent activity later.

## Toolbar

Heading selector, bold, italic, underline, bullet list, numbered list, task list, link, table, attachment, export/more.

## Slash Commands

Typing `/` at the beginning of a block opens the command menu.

Commands: Text, Heading 1, Heading 2, Heading 3, Bullet list, Numbered list, Task list, Quote, Code block, Table, Image, File attachment, Divider.

Keyboard: Arrow keys navigate, Enter selects, Escape closes, Cmd/Ctrl+B/I/K/S should work.

## Attachment UX

Drag image -> upload -> insert inline image with relative path.

Drag file -> upload -> insert file card/link with relative path.

Attachment panel shows filename, icon, size, open/download, insert/remove if allowed. Hidden/system files never appear.

## Export UX

Export menu: Markdown, Printable / Save as PDF. Later: HTML, DOCX, ZIP with attachments.

## Smoothness Requirements

The editor must not reinitialize on every keystroke. Autosave is debounced. Toolbar state updates without flicker. Uploads show pending/progress state. Switching read/edit should preserve scroll if feasible.

## Fallback UX

If Markdown cannot be safely parsed, show read mode, display warning, offer raw Markdown editor, and do not overwrite automatically.
