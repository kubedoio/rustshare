# Contract: Editor API

Use existing RustShare file APIs if sufficient. Otherwise implement document-level APIs.

## Get Document

`GET /api/editor/documents/:documentId`

Returns document content, metadata, attachments, permissions and revision.

## Save Content

`PUT /api/editor/documents/:documentId/content`

Request contains Markdown content, baseRevision, optional editorJson. Response returns ok, revision and updatedAt. Must detect conflict if revisions are supported.

## Upload Attachment

`POST /api/editor/documents/:documentId/attachments`

Returns sanitized attachment metadata.

## List Attachments

`GET /api/editor/documents/:documentId/attachments`

Must not return hidden/system files.

## Delete Attachment

`DELETE /api/editor/documents/:documentId/attachments/:attachmentId` optional for MVP.

## Errors

not_found, forbidden, validation_error, path_traversal, upload_too_large, unsupported_file_type, revision_conflict, save_failed.
