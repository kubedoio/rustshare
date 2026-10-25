# Mail-to-Calendar integration contract (M2.3)

Status: Proposed for issue #397
Date: 2026-10-25

## Design decision

Calendar owns a small `calendar_mail_links` relation. The existing Mail link
model is not extended: it has no Calendar-event target, and using its `meeting`
target would conflate Calendar events with folder-backed meeting notes. The
relationship stores the event UUID and the canonical Mail reference
`elembra://io.elembra.mail/message/{uuid}`. It does not copy message content,
recipients, credentials, or attachment metadata.

The foreign reference is an identifier only. Calendar endpoints parse and
validate it, then call the existing owner-scoped `MailService::get_message`
with the authenticated tenant and user. They also verify both Applications are
enabled for that tenant and retrieve the event through Calendar's existing
tenant/owner-scoped service. Mail is not registered in `SourceAuthorizer`; this
bounded integration does not change that contract or add another Mail
permission system.

## Relationship lifecycle

- One active association is allowed for a Calendar event in the same tenant
  and owner scope. The unique partial index makes retries idempotent.
- The relation records tenant, owner, event ID, canonical Mail reference,
  creation actor, creation time, and unlink time. Calendar event deletion and
  Mail message deletion remain authoritative; stale relations resolve as
  unavailable and disclose no foreign-resource metadata.
- Explicit unlink marks the relation inactive. Linking can be repeated after
  unlinking; it creates a new auditable active row.
- Disabling either Application prevents association creation and resolution.
  Re-enabling it makes the relation resolvable again only if both source rows
  still exist and remain owner-readable.
- Access is rechecked on every resolution. The relation never grants access.

## API shape

All operations require an authenticated session, enabled Mail and Calendar
Applications, and a message and event that `MailService` and
`CalendarService` both resolve for the caller's tenant and owner.

- `POST /api/v1/calendar/mail-links` with `{ "event_id": "uuid",
  "mail_message_ref": "elembra://io.elembra.mail/message/uuid" }` creates or
  returns the active association.
- `GET /api/v1/calendar/mail-links?message_id=uuid` returns active events for
  that currently authorized message.
- `GET /api/v1/calendar/events/{event_id}/mail-link` returns the authorized
  message ID for client navigation. It does not return message metadata.
- `DELETE /api/v1/calendar/mail-links/{link_id}` unlinks an association after
  reauthorizing both resources.

Invalid references, missing resources, and foreign-owned resources use
existence-hiding responses. A remote IMAP UID is never stored as a relationship
identity; clients may act only on a persisted `imported_message_id`.

## Event creation recovery

The existing Calendar create operation commits the event and its integration
outbox event atomically. Link creation is a separate idempotent operation. If
it fails after event creation, the UI reports that the event exists but the
relationship did not complete and offers retry using that event ID. It never
reports the whole operation as linked successfully.

The Calendar event contains only user-entered event fields. A Mail subject may
be prefilled as an editable title suggestion; the user confirms it through the
event create form. The form uses the browser's IANA time zone and local
date/time inputs so wall time and event zone stay aligned. Message bodies,
recipients, and attachments are not read by the integration.
