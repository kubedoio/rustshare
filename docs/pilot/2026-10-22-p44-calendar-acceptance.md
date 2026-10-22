# P4.4 Calendar interoperability and Gate A acceptance evidence

Baseline: `3b55d1d2229454fcb80b3928012238a2042762d8`
Candidate: P4.4 interoperability closure (PR pending)
Environment: disposable PostgreSQL `rustshare_test_p44`, RustFS test bucket `rustshare-test-p44`, local HTTP server, and local Vite/Playwright browser.

This matrix records evidence gathered for #329 and the bounded historical-meeting journey. `Tested` means the named check was actually executed; a code path or an unexecuted test is not evidence of a pass.

## #329 Calendar acceptance matrix

| Acceptance criterion | Status | Evidence / bounded limitation |
| --- | --- | --- |
| Export one owned event as RFC 5545 iCalendar | **Tested** | `calendar_api_test` HTTP export checks ownership, headers, event fields, and an independent `ical` parser. |
| Import and export supported event types; repeat imports do not duplicate | **Tested** | `calendar_import_test` passed 15 tests, including supported single-event cases, stable UID mapping, duplicate prevention, and independent parse of exported data. |
| Preserve timezone semantics and DST wall-clock interpretation | **Tested** | Added an HTTP import/export/re-import test for `Europe/Berlin` during the 2026 fall DST overlap. It verifies the UTC instant and stored timezone survive export via `X-WR-TIMEZONE`; independent `ical` parsing is used. Existing parser tests cover TZID/VTIMEZONE conversion. |
| All-day events and supported recurrence round-trip | **Tested** | Existing import/export tests cover date-valued all-day events and UTC recurring masters. |
| Non-UTC recurrence and detached recurrence overrides | **Explicitly deferred** | Contract supports UTC recurring masters; non-UTC recurring masters and detached overrides are not exported as lossy data (override export returns conflict). No broader recurrence contract is claimed. |
| Stable UID handling and repeat-import idempotency | **Tested** | `calendar_import_test` verifies stable source UID mapping and that re-importing does not create duplicate events. |
| Malformed, unsupported, oversized, and event-count-bounded input | **Tested** | `calendar_import_test` exercised malformed/unsupported inputs, the 10 MiB file boundary, and the 10,000-event cap. |
| Private metadata is not exposed by export/import | **Tested** | Export assertions check private event metadata is not emitted; import allowlisting and unsupported-property tests cover input handling. |
| Owner-only visibility, cross-tenant isolation, and disabled-user behavior | **Tested** | HTTP Calendar API tests verify foreign-owner denial, disabled Calendar denial, and rejection of a previously valid token after the user is disabled. Meeting authorization tests also cover cross-tenant and disabled callers. Calendar remains owner-only; shared Calendar access is out of scope. |
| Export after access revocation | **Explicitly deferred / inapplicable** | Calendar has no event-share grant path, so there is no supported event grant/revoke transition to exercise. Owner-only and disabled-user export denial are tested above. Feed-token revocation remains untested because feeds are deferred. |
| Worker restart/recovery | **Tested** | `stale_running_import_is_reclaimed_after_worker_restart` marks an import job stale and starts a new worker, which reclaims and completes it. This simulates restart recovery against PostgreSQL; it is not a process/container crash drill. |
| Read-only subscription feeds | **Explicitly deferred** | ADR-0037 requires end-to-end bearer-token lifecycle, authorization, revocation, and URL/log redaction before implementation. Those deployment-bound controls are not demonstrated here. |
| CalDAV | **Explicitly deferred** | ADR-0037 defers server-side CalDAV absent a named pilot client and concrete interoperability requirement. |

## Executed HTTP and browser checks

PostgreSQL-backed HTTP checks run against the disposable database and RustFS test bucket:

- `calendar_api_test`: 28 tests passed; the single-event export test was rerun after adding the disabled-user assertion and passed.
- `calendar_import_test`: all 15 tests passed in the final combined run, including the timezone round-trip and stale-worker recovery cases.
- `calendar_google_sync_test`: 25 mock-provider tests passed.
- `calendar_outlook_sync_test`: 23 mock-provider tests passed.
- `meeting_actions_test`: 13 tests passed, including the event → meeting → decision/action → historical search/reopen path, permission grant/revoke, disabled caller, and cross-tenant denial.

The PR #395 Playwright test was executed against the seeded local environment. It passed after correcting its locators to match the rendered accessible roles. It verifies historical search → meeting reopen → originating Calendar event. It does **not** exercise Files/Notes linking or decision/action creation in the browser; those journey steps have database-backed HTTP integration coverage above. No live Google or Microsoft credentials were configured, so provider OAuth was not validated against external services.

## #327 follow-up assessment

- Internal Calendar event create/edit and Day/work-week/month/agenda rendering are present. The Calendar view has no drag/reschedule interaction; touch-friendly movement and responsive rescheduling are tracked in [#396](https://github.com/kubedoio/rustshare/issues/396).
- Google and Microsoft sync have mock-provider HTTP coverage (25 and 23 tests). OAuth app configuration is represented by server configuration/status and settings UI, but no real provider app credentials or redirect registration were available here, so deployment configuration and real OAuth remain unverified.
- The event-to-meeting HTTP path and meeting-to-event navigation passed. The seeded browser journey verifies historical search, reopen, and Calendar navigation; Files/Notes linking and Decision/Action authoring were verified by integration tests rather than a complete browser journey.
- The topbar’s removed legacy GET search call caused the browser journey to show “temporarily unavailable”; it now uses the existing permission-aware unified search endpoint. That endpoint does not return folder suggestions, so global folder suggestions remain a separate UI gap.
- Event-specific email context is tracked in [#397](https://github.com/kubedoio/rustshare/issues/397). Downstream Calendar consumption by other Applications remains a separate product question; no new Application or integration was added here.

## Gate A boundary

This evidence is local acceptance evidence, not institutional Gate A readiness. Gate A still requires evidence from the exact deployed candidate, including operational ownership, independent administrators, deployed secret/log-redaction controls, restore/recovery evidence, and the required permission-negative checks on that deployment. No production or pilot deployment was used for this record.
