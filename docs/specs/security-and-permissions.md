# Specification: Security and Permissions

## Permission Rules

Read permission: view, print, export, view allowed attachments.

Write permission: edit, save, upload attachments, update editor cache.

Public share: read-only by default.

## Sanitization

Rendered Markdown/HTML must sanitize scripts, event handlers, unsafe links and unsafe raw HTML.

## Attachment Security

Validate filename, path, MIME/type if available, size, permissions and target folder. Prevent path traversal and hidden metadata exposure.

## Link Security

Reject/sanitize unsafe schemes such as `javascript:`. Use safe external link attributes where appropriate.

## Extension Security

Slash commands execute known actions only. Custom nodes/extensions must not inject unsanitized HTML.
