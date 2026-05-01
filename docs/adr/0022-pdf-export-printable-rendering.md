# ADR-0022: PDF Export via Printable Rendering First

## Status

Proposed / Permanent Architecture Candidate

## Context

Users expect rich notes and documents to be exportable to PDF.

A high-quality PDF export system can become complex:

- page layout;
- headers/footers;
- images;
- tables;
- code blocks;
- page breaks;
- server-side rendering;
- permissions;
- long-running jobs.

Docmost-like products often need PDF export, but RustShare should not block the editor MVP on a full server-side PDF rendering system.

## Decision

RustShare will implement PDF export in two phases.

### Phase 1: Browser Print / Save as PDF

RustShare renders a clean `PrintableDocumentView` with a dedicated print stylesheet and triggers the browser print dialog.

The user can save as PDF from the browser.

### Phase 2: Optional Server-Side PDF Export

Later, RustShare may implement server-side PDF generation using a controlled rendering engine, such as headless Chromium, if needed for automated exports or shareable PDF URLs.

## Printable View Rules

Printable view must exclude:

- sidebar
- top navigation
- editor toolbar
- edit controls
- admin controls
- comments panel, unless explicitly included
- hidden metadata
- internal debug data

Printable view should include:

- document title
- document path or module label, if configured
- content
- images
- tables
- code blocks
- optional footer with export date
- optional version/revision metadata later

## Rationale

Browser print is lightweight, reliable enough for MVP, and avoids adding a heavy backend service too early.

A dedicated print stylesheet is still required. Printing the editor UI directly is unacceptable.

## Consequences

Positive:

- fast MVP delivery;
- no headless browser backend required;
- less security surface;
- simpler implementation.

Negative:

- browser-specific rendering differences;
- no server-generated PDF URL in MVP;
- limited programmatic control.

## Acceptance Criteria

- User can open printable view.
- Printable view renders document content without editor chrome.
- Browser print dialog can be triggered.
- Images, tables and code blocks render acceptably.
- Export requires read permission.
- Hidden metadata is not rendered.
