# Elembra Beta — Known Limitations

The honest list. Every tester receives this at onboarding; it is updated in
the weekly digest when things change. Items leave this list only when they
are actually fixed, not when we hope they will be.

## Deployment and operations

- **Single server.** The beta runs on one host with Docker Compose. There
  are brief maintenance windows for upgrades; we announce them in the
  digest with at least 24h notice (72h if a data reset were ever needed).
- **No high availability, no multi-region, no zero-downtime upgrades.**
- Backups run daily at most (recovery point up to 24h old). Do not treat the
  beta as the only copy of anything you care about.

## Product scope

- **Web-only.** There are no supported desktop or mobile clients in this
  beta. The desktop CLI exists in the repository but is not part of the
  beta.
- **Obsidian vault sync is not included** in this beta.
- **Calendar is new in this beta** (customer requirement): events with
  attendees and month/week/list views are in scope; its depth beyond that
  (recurrence, ICS import/export, cross-application attachments) depends on
  the signed-off specification — we will tell you exactly what is included
  at kickoff.
- **Chat reply/thread composer** is not finished yet — replies exist in the
  protocol but the composer UI is deferred.
- **Chat device/key administration** is limited: browser-held identity with
  explicit lock/unlock; account-managed device administration is deferred.

## Behavior you might notice

- **Search freshness**: new notes and chat messages become searchable
  through a durable event pipeline; a short delay (seconds, occasionally
  minutes under load) is expected.
- **Ask quality**: cited answers depend on the configured AI provider; when
  no provider is configured, Ask degrades cleanly instead of pretending.
  Citations always point at content you are authorized to open — if a
  citation ever opens something you shouldn't see, that is Sev-1.
- **Upload-only public links** deliberately hide folder contents; that is
  intended behavior, not a bug.
- The interface is English-only for now.

## Security and data

- Before you receive access, the beta instance must have recorded, in its
  launch gate (`docs/releases/beta-gate.yaml`): exact-image vulnerability
  scanning with 0 Critical / 0 High, CodeQL static analysis, and an internal
  security review including an adversarial cross-tenant campaign. External
  penetration testing is scheduled for the GA track and is not part of this
  beta.
- Do not store real secrets or production data (see the tester guide).
- Security issues must be reported privately per `SECURITY.md`.

## Reporting something not on this list

If you hit behavior that is not listed here, report it — the feedback
template (`feedback-template.md`) is the fastest way. Even "this list is
wrong about X" is useful feedback.
