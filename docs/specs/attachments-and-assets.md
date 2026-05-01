# Specification: Attachments and Assets

## Purpose

Define attachment behavior for the rich editor.

## Folder Layout

Folder-backed document:

```text
/Notes/Project-Brainstorm/
  index.md
  .rustshare.json
  /attachments/
    diagram.png
    brief.pdf
```

Single file document:

```text
/Documents/proposal.md
/Documents/proposal.attachments/
```

## Upload Rules

Allowed only with write permission and valid filenames. Reject path traversal, absolute paths, names beginning with `.rustshare`, files above size limits, unsupported MIME types if policy exists, and writes outside the attachments folder.

## Filename Sanitization

Strip path separators, reject `..`, reject absolute paths, reject hidden system metadata names, avoid overwrite by suffixing duplicate filenames.

## Image Insert

Upload image to attachments and insert `![diagram](./attachments/diagram.png)`.

## File Insert

Upload file to attachments and insert `[brief.pdf](./attachments/brief.pdf)` or a safe file-card node that serializes to a relative link.

## Public Share Rendering

Only render attachments referenced/allowed by the document. Do not expose metadata, editor cache, event logs or hidden files.
