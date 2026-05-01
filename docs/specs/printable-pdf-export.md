# Specification: Printable View and PDF Export

## MVP Model

Render `PrintableDocumentView`, apply print stylesheet, call browser print, user saves as PDF.

## Printable View Includes

Title, document content, images, tables, code blocks, links, optional path and export date.

## Printable View Excludes

Sidebar, app header, toolbar, buttons, edit controls, admin controls, hidden metadata and attachment panel unless explicitly configured.

## Export UI

Export menu: Markdown, Save as PDF.

## Later

Server-side PDF export may be added later, but is not MVP.
