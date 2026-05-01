# ADR-0020: Canonical Markdown with Optional Editor JSON Cache

## Status

Proposed / Permanent Architecture Candidate

## Context

Modern rich editors often use structured JSON as their internal document model. Markdown is human-readable and portable, but it cannot represent every advanced editor feature perfectly without extensions.

RustShare must balance:

- modern rich editing;
- durable file-backed storage;
- portability;
- compatibility with external tools;
- clean AI/RAG indexing;
- future extension support.

## Decision

RustShare will use Markdown as the canonical document source for rich text documents.

An optional editor JSON file may be stored as a cache or fidelity-preserving auxiliary representation, but it must never be the only source of truth.

## Storage Model

Folder-backed rich document:

```text
/{ModuleRoot}/{DocumentSlug}/
  index.md
  index.editor.json        optional
  .rustshare.json
  /attachments/
```

Single Markdown file opened from file browser:

```text
/path/to/document.md
/path/to/document.rustshare.json       optional, if needed
/path/to/document.attachments/         optional, if needed
```

## Source of Truth

```text
index.md or document.md = canonical source
index.editor.json = optional editor cache
.rustshare.json = machine metadata
attachments/ = real file assets
```

## When to Write Editor JSON

RustShare may write `index.editor.json` when:

- editor startup speed benefits from cached JSON;
- custom nodes need extra fidelity;
- a conversion round-trip would otherwise lose non-critical editor state.

But RustShare must be able to reconstruct the editor from Markdown alone for standard documents.

## Required Markdown Compatibility

The editor must preserve:

- headings
- paragraphs
- bold
- italic
- underline, if representable with HTML or Markdown extension
- bullet lists
- numbered lists
- task lists
- blockquotes
- inline code
- fenced code blocks
- links
- images
- tables
- horizontal rules

## Custom Blocks

For custom blocks, prefer Markdown-compatible representations:

```markdown
:::rustshare-attachment
path: ./attachments/spec.pdf
title: spec.pdf
:::
```

or standard Markdown links where possible:

```markdown
[spec.pdf](./attachments/spec.pdf)
```

For MVP, use standard Markdown wherever possible.

## Rationale

Markdown as canonical source supports:

- external editing;
- Git-style diffs;
- backup;
- export;
- portability;
- AI indexing;
- long-term durability.

Editor JSON-only storage would damage RustShare's file-centric product identity.

## Consequences

Positive:

- content is portable;
- external users can edit Markdown outside RustShare;
- documents can be indexed by simple pipelines;
- export is easier.

Negative:

- rich editor features must be carefully mapped to Markdown;
- some complex blocks may require custom syntax;
- round-trip tests are mandatory.

## Acceptance Criteria

- Editing and saving a document updates Markdown.
- The document remains readable outside RustShare.
- Editor JSON cache can be deleted without losing the document.
- Markdown round-trip tests exist for supported formatting.
- Unsupported advanced blocks degrade gracefully.
