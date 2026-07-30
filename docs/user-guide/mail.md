# Mail in RustShare

RustShare Mail lets you connect IMAP accounts, browse remote folders, import or archive messages into your workspace, and read both remote and imported mail in the WebUI.

This guide focuses on the privacy behavior of message rendering. For account setup and import/archive jobs, see the in-app Mail module.

---

## Reading messages

You can read mail in two places:

- **IMAP preview** — the message still lives on your mail server. RustShare fetches the body on demand and renders it in the preview pane.
- **Imported message page** — the message was copied into RustShare (via upload, selected import, or archive job) and is served from workspace storage.

In both cases, message HTML is sanitized before rendering: scripts, event handlers, forms, iframes, and unsafe URL schemes (such as `javascript:`) are always removed, whether or not you load remote images.

## Remote images are blocked by default

Opening a message never triggers requests to external image servers. Remote images (`http:`, `https:`, and protocol-relative `//` sources, including `srcset` candidates) are blocked by default, because loading them would tell the sender — via tracking pixels — when and where you opened the message.

When a message contains blocked images, a notice appears above the body:

> **Images were blocked to protect your privacy.** [Load remote images]

- Click **Load remote images** to load external images for that message only.
- The choice is per-message and is not remembered: switching to another message blocks images again. There is no global or per-sender allowlist.
- While images are loaded, a subtle "Remote images loaded" bar is shown; use **Block images** to revert.

Plain-text messages and messages without remote images show no notice.

## Embedded (cid:) images

Images embedded in the message itself (referenced as `cid:<content-id>` and carried as MIME parts) are not remote content and keep working without the explicit load action:

- In the **IMAP preview**, embedded images are served through the authenticated attachment download endpoint, matching the part by its Content-ID. If the referenced part is missing, the image is left as-is and simply renders broken.
- In **imported messages**, embedded images are rewritten to workspace file previews when the part was stored with the message.

## For integrators

The imported message-part endpoint reports and controls this behavior:

- `GET /api/v1/mail/messages/{id}/parts/{part_id}` sanitizes HTML parts and blocks remote images by default. When at least one image was blocked, the response carries the `X-Mail-Blocked-Remote-Images: 1` header.
- Append `?load_remote_images=true` to keep remote image sources in the sanitized HTML (per-request opt-in).
- The response always includes `Content-Security-Policy: sandbox`.
