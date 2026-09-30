# Elembra Beta — Tester Guide

Welcome to the Elembra beta, and thank you for helping us build it. This
guide tells you what to expect, what to test, and how to report what you
find.

## What you are testing

Elembra is a self-hosted workspace for durable team knowledge: **Files**
(upload/download, versioning, trash/restore, sharing), **Notes** (Markdown
editor), **Chat** (Buzz-backed channels), and **Memory/Search + Ask**
(permission-aware search and cited Q&A when an AI provider is configured).

You are using a **hosted beta instance operated by us**. It is not your
production system and must not become one.

## Ground rules

1. **Do not put real secrets or production data in the beta.** Assume
   anything you upload can be read by the operators during debugging and may
   be lost on a major incident or reset.
2. **Beta software breaks.** Features change without notice. We announce
   breaking changes in the weekly digest.
3. **Your data is yours**: you can export/download your files at any time,
   and it is deleted when you leave the beta (tell us and we offboard you).
4. **Security issues go private.** Never report suspected vulnerabilities in
   public — see `SECURITY.md` in the repository for the private channel.

## What to test (in rough priority order)

1. **Daily driver flows**: upload, organize, rename, move, delete, restore
   files; create and edit notes; search for your own content.
2. **Sharing**: internal shares with colleagues (View vs Edit), public
   links, upload-only links, and — importantly — **revocation**: after a
   share is revoked, is access really gone?
3. **Permissions**: make sure people can only see what they should. If you
   ever see something you should not have access to, treat it as Sev-1 (see
   below).
4. **Chat**: channels, identity unlock, attachments, and whether chat
   messages later show up in search.
5. **Ask** (if enabled): ask questions about your files/notes and check
   whether the citations actually point at content you can open.
6. **Rough edges**: slow operations, confusing UI, confusing errors,
   browser quirks, keyboard navigation, dark mode.

## How to report

Open an issue in the repository using the feedback template
(`docs/beta/feedback-template.md`). The short version of a useful report:

- **What you tried to do**
- **What happened instead** (exact error text is gold)
- **What you expected**
- **Browser and OS**
- **Roughly when** (so we can correlate with logs)

Remove passwords, tokens, and private URLs before posting.

### Severity levels

| Level | Meaning | Example | Our target |
|---|---|---|---|
| Sev-1 | Data loss, cross-tenant/cross-user leak, instance unusable | You see someone else's files | Ack ≤ 4h (contact us directly) |
| Sev-2 | Core flow broken with no workaround | Uploads fail for a file type | First response ≤ 2 business days |
| Sev-3 | Broken with workaround, or bad UX | Confusing error message | Triaged weekly |

## What we commit to during the beta

- **SLA**: Sev-1 acknowledged within 4 hours; everything else gets a first
  response within 2 business days.
- **Weekly digest**: what shipped, what broke, what we are working on.
- **No silent resets**: if we ever need to reset data, you get at least 72h
  notice and an export window.
- **Honest answers**: if something is a known limitation, we will say so
  (see the known-limitations list).

## Known limitations (day-1 summary)

See `known-limitations.md` in this folder for the current list. Headlines:
single-server deployment (brief maintenance windows), web-only (no
desktop/mobile apps), chat reply/thread UI and some chat device management
are unfinished, and Obsidian vault sync is not part of this beta.

## When the beta ends

We will announce the end date at least two weeks ahead. You get an export
window for your data, and we delete it after the window closes (unless you
join the next phase).
