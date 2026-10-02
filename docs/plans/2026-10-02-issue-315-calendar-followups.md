# Calendar Follow-ups: Day/Work-Week Views, Create-Event Hardening, Test Extension, and Working External Auth

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Each `## Task N:` block is a self-contained executor prompt.

**Goal:** Fix the three problems found in manual QA of the Calendar application (issue #315) — missing Day view / non-Mon–Fri week, create-event "JSON error" (stale bundle, plus hardening), and non-working Google connection (missing OAuth client credentials + wrong `RUSTSHARE_PUBLIC_URL`) — and extend the test suite so these classes of failure are caught in CI.

**Architecture:** Frontend work stays inside `CalendarApplicationView.svelte` plus a new extracted, unit-testable `$lib/calendar/view-range.ts`; the range API already accepts any window ≤ 366 days, so **no backend API change is needed** for Day/Work-week. Backend work adds only diagnostics (serde error logging), a validated public URL, an operator-facing provider status endpoint, and tests (pure unit + HTTP-level + DB-level). Google/MS connection itself already exists end-to-end; the blocker is deployment configuration plus missing guardrails/diagnostics.

**Tech Stack:** Rust 1.97 / Axum / SQLx (offline metadata) / PostgreSQL 16; Svelte 5 runes + TanStack Query (`$lib/query-compat`); vitest + @testing-library/svelte; `wiremock`-style hand-rolled mock provider servers in `backend/tests/`.

**Companion documents:**
- `docs/adr/0037-calendar-application-and-external-sync.md`
- `docs/specs/calendar-application-v1alpha1.md`
- `docs/contracts/calendar-application-api.md`
- `docs/plans/2026-10-01-issue-315-calendar-application.md` (the original executed plan)

---

## Background: verified findings (evidence, not speculation)

### F1 — "day view is missing / week should be Mon–Fri"

- `frontend/src/lib/components/apps/CalendarApplicationView.svelte:19` — `type CalendarView = 'month' | 'week' | 'agenda'`; switcher at `:447-456`, default `month`.
- Week range is Sunday-anchored: `windowRange` at `:50-62` uses `addDays(cursor, -cursor.getDay())` → **Sunday–Saturday**; `weekCells` (`:197-203`) is 7 cells; grids hardcode `grid-cols-7`.
- Week and agenda cells have **no create affordance** (only month cells do).
- The backend range contract already supports arbitrary windows: `handlers/calendar.rs:207-239`, `calendar_service.rs:1052-1072` (rejects `from >= to`, caps at `MAX_RANGE_WINDOW_DAYS = 366`), `docs/contracts/calendar-application-api.md:34-56`. **No API change required.**
- Docs that state the current view set (to be updated): `docs/adr/0037-*.md:149-150,284`, `docs/plans/2026-10-01-issue-315-calendar-application.md:9,68,398-399`.
- Interpretation: implement **Reading A** — add a **Day** view (single day) **and** make the week view a **Monday–Friday work week**, keeping Month (Sunday-first, unchanged) and Agenda. This satisfies both readings of the request; the toggle label for the week is `Work week`.

### F2 — "adding a meeting on the day gives JSON error"

Root-caused and reproduced against the live stack:

- Pre-fix bundle sent `timezone: editingEvent?.timezone ?? null` (`CalendarApplicationView.svelte:361` in the pre-fix commit). For create, that is `null`.
- Backend `CreateCalendarEventRequest.timezone: String` (`handlers/calendar.rs:79`) requires a non-null string; serde fails and `ValidatedJson` maps **any** deserialization error to `400 {"error":"Invalid JSON payload"}` (`handlers/validated_json.rs:35-37`) — exactly 32 bytes.
- Nginx access log on this host confirms the user's two attempts: `POST /api/v1/calendar/events` → `400 32` at 16:45:20Z and 16:45:56Z, referer `https://app.rustshare.io/apps/calendar`.
- Reproduction against the stack: the pre-fix payload → `400 {"error":"Invalid JSON payload"}`; the same payload with `"timezone":"Europe/Berlin"` → `201`.
- The code fix already landed in `78100d68` (HEAD has `timezone: editingEvent?.timezone ?? Intl.DateTimeFormat().resolvedOptions().timeZone`). The user's browser was still running the 13:20Z bundle; the backend container was rebuilt at 17:20Z. **A hard reload fixes the user's symptom.**
- Residual hardening gaps: no fallback if `Intl.DateTimeFormat().resolvedOptions().timeZone` is `undefined`; the backend discards the serde error (`map_err(|_err| …)`), so operators cannot see *why* a payload was rejected; no test exercises the **day-cell** create path or the exact UI payload shape.

### F3 — "Google addition is not working; there needs to be some token/auth mechanism"

Diagnosis on this deployment (verified):

- `RUSTSHARE_CALENDAR_GOOGLE_CLIENT_ID` / `_SECRET` and the Microsoft equivalents are **absent** from the running backend container; `RUSTSHARE_PUBLIC_URL` is **unset**, so it defaults to `http://localhost:5173` (`config.rs:22,155`) while the app is served at `https://app.rustshare.io`.
- Connect therefore returns **503** by design (`google_calendar.rs:111-118` → `calendar_service.rs:405-410` → `handlers/calendar.rs:824`), and the settings panel correctly shows "not configured on this deployment".
- Redirect URI is derived as `{public_url}/api/v1/calendar/oauth/google/callback` (`google_calendar.rs:125`; Outlook `outlook_calendar.rs:97`). With the default it points at a dead dev port — so even after adding credentials the flow would fail with `redirect_uri_mismatch` or redirect the browser to nothing. There is **no startup validation or warning** for this.
- DB hygiene: the deployment DB contains leftover `@test.local` Google sources from earlier test runs (5 google rows, `last_error = "google OAuth is not configured"`), i.e. integration tests have been pointed at the deployment database.
- Other real failure modes to handle: partial config (id without secret) is silently unconfigured; connect failures collapse to `?error=oauth_exchange` with no reason; no operator surface showing the effective redirect URI; Google "Testing" apps expire refresh tokens after 7 days; Outlook's authorize URL lacks `prompt=consent`, so re-consent can legitimately yield no refresh token.

**Staged auth plan (this document covers Stage 1; Stages 2–4 are separate follow-up plans, see "Deferred"):** Stage 1 = make the existing OAuth authorization-code flow work for operators (config validation + diagnostics + docs + DB hygiene + tests). Stage 2 = admin-UI provisioning of provider credentials (mirroring `admin/config/oidc`). Stage 3 = zero-console **ICS secret-URL** source (`kind = 'ical_url'`, encrypted URL, ETag cursor, SSRF-hardened fetch). Stage 4 = device-code flow (optional).

### F4 — Test coverage

- CI runs: `cargo test --workspace --all-features --lib` (in-source units), `Integration Tests` job (`cargo test --workspace --all-features -j 1 -- --ignored --test-threads=1`, `.github/workflows/integration-tests.yml:117,135`), and the frontend vitest job.
- Highest-value gaps (full matrix in Task 3/4/6): the **day-cell create path**; the UI payload shape vs `CreateCalendarEventRequest`; the create validation matrix; **HTTP-level** connect 503 mapping and the **entire OAuth callback handler**; redirect-URI derivation; view-range math (week/agenda/day); `calendar.ts` connect/disconnect/resync; settings-panel error paths; 413 upload cap; token-leak/ownership assertions on more endpoints.

---

## File structure

**Frontend**
- Create: `frontend/src/lib/calendar/view-range.ts` — pure range math + view definitions (testable without rendering).
- Create: `frontend/src/lib/calendar/view-range.test.ts`
- Modify: `frontend/src/lib/components/apps/CalendarApplicationView.svelte` — day view, Mon–Fri work week, create affordances, hardening.
- Modify: `frontend/src/lib/components/apps/CalendarApplicationView.test.ts`
- Modify: `frontend/src/lib/api/calendar.ts` (+ `calendar.test.ts`) — connect/disconnect/resync coverage.
- Modify: `frontend/src/lib/settings/CalendarSettingsPanel.test.ts` — error-path coverage.

**Backend**
- Modify: `backend/server/src/handlers/validated_json.rs` — log the discarded serde error (observability).
- Modify: `backend/server/src/config.rs` — validate/normalize `RUSTSHARE_PUBLIC_URL`; log effective calendar redirect URIs at startup.
- Modify: `backend/server/src/services/calendar_service.rs` — extract pure validators to `pub(crate)`; public provider-status helper.
- Modify: `backend/server/src/handlers/calendar.rs` — provider status endpoint (admin read-only) + config-error mapping for connect.
- Create: `backend/crates/core/src/domain/calendar_validation.rs` (if extraction is cleaner than `pub(crate)` fns) — only if Task 4 finds it necessary.
- Modify: `backend/server/src/services/google_calendar.rs`, `outlook_calendar.rs` — bind redirect URI validation; Outlook `prompt=consent`.
- Modify: `.env.example`, `backend/.env.example` — operator checklist.
- Tests: `backend/tests/support/calendar_harness.rs` (new shared harness), `backend/tests/calendar_api_test.rs`, `backend/tests/calendar_connect_test.rs` (new), plus unit tests in the touched sources.

**Docs**
- Modify: `docs/adr/0037-calendar-application-and-external-sync.md`, `docs/specs/calendar-application-v1alpha1.md`, `docs/contracts/calendar-application-api.md` (status endpoint + UI views note), `CHANGELOG.md`.
- Append an amendment section to `docs/plans/2026-10-01-issue-315-calendar-application.md`.

---

## Task 1: Create-event hardening and the day-cell regression test

**Files:**
- Modify: `frontend/src/lib/components/apps/CalendarApplicationView.svelte:342-389` (openEditor/handleSave/openCreate)
- Modify: `frontend/src/lib/components/apps/CalendarApplicationView.test.ts`
- Modify: `backend/server/src/handlers/validated_json.rs`
- Test: `backend/tests/calendar_api_test.rs`

- [ ] **Step 1: Write the failing day-cell create test**

Add to `CalendarApplicationView.test.ts` (mirrors the existing "New event" test, but drives the month-cell `+`):

```ts
it('creates an event from a day cell with that cell date and a concrete timezone', async () => {
  render(CalendarApplicationView, { props: { applicationId: 'io.elembra.calendar' } });
  await waitFor(() => expect(listEventsMock).toHaveBeenCalled());

  // October 2026 month grid: pick a deterministic day cell
  const cell = await screen.findByLabelText(/Create event on October 14, 2026/);
  await fireEvent.click(cell);

  await fireEvent.input(screen.getByLabelText('Title'), { target: { value: 'Board meeting' } });
  await fireEvent.submit(screen.getByRole('form'));

  await waitFor(() => expect(createEventMock).toHaveBeenCalledTimes(1));
  const payload = createEventMock.mock.calls[0][0];
  expect(payload.title).toBe('Board meeting');
  expect(payload.starts_at.startsWith('2026-10-14')).toBe(true);
  expect(typeof payload.timezone).toBe('string');
  expect(payload.timezone.length).toBeGreaterThan(0);
});
```

Run: `cd frontend && npx vitest run src/lib/components/apps/CalendarApplicationView.test.ts -t "creates an event from a day cell"`
Expected: PASS on HEAD (the bug is already fixed); if it FAILS, the pre-fix bundle is in the working tree — fix `handleSave` before continuing. This test exists to lock the behavior forever.

- [ ] **Step 2: Harden the timezone fallback**

In `CalendarApplicationView.svelte`, replace the inline expression with a helper near the other date helpers:

```ts
const BROWSER_TIMEZONE = (() => {
  const tz = Intl.DateTimeFormat().resolvedOptions().timeZone;
  return typeof tz === 'string' && tz.length > 0 ? tz : 'UTC';
})();
```

and use `timezone: editingEvent?.timezone ?? BROWSER_TIMEZONE` in `handleSave` (both create and update paths).

Add a test that stubs `Intl.DateTimeFormat` to return no timezone and asserts the `'UTC'` fallback:

```ts
it('falls back to UTC when the browser reports no timezone', async () => {
  const spy = vi
    .spyOn(Intl, 'DateTimeFormat')
    .mockReturnValue({ resolvedOptions: () => ({}) } as unknown as Intl.DateTimeFormat);
  render(CalendarApplicationView, { props: { applicationId: 'io.elembra.calendar' } });
  await fireEvent.click(await screen.findByRole('button', { name: /New event/i }));
  await fireEvent.input(screen.getByLabelText('Title'), { target: { value: 'TZ fallback' } });
  await fireEvent.submit(screen.getByRole('form'));
  await waitFor(() => expect(createEventMock).toHaveBeenCalledTimes(1));
  expect(createEventMock.mock.calls[0][0].timezone).toBe('UTC');
  spy.mockRestore();
});
```

Run: `cd frontend && npx vitest run src/lib/components/apps/CalendarApplicationView.test.ts`
Expected: all pass.

- [ ] **Step 3: Log the discarded serde reason in the backend**

In `backend/server/src/handlers/validated_json.rs`, keep the wire behavior (`400 {"error":"Invalid JSON payload"}`) but log the reason:

```rust
Json(value) => value,
Err(err) => {
    tracing::warn!(error = %err, "rejected request: invalid JSON payload");
    return Err(AppError::BadRequest("Invalid JSON payload".into()));
}
```

Add a backend test asserting the public body is unchanged while a malformed body is logged. Assert only the body in the test; assert the log via a captured `tracing` layer if one exists in the repo, otherwise assert the body and note the log is covered by manual observation.

- [ ] **Step 4: Backend contract tests for the UI payload shape**

Add to `backend/tests/calendar_api_test.rs`:

```rust
#[tokio::test]
#[ignore = "requires DATABASE_URL (see module docs)"]
async fn create_accepts_the_minimal_ui_payload() {
    // POST exactly what the UI sends: title, starts_at, ends_at, timezone,
    // description: null, location: null — no all_day, no rrule keys.
    // Expect 201 and the created event echoed back.
}

#[tokio::test]
#[ignore = "requires DATABASE_URL (see module docs)"]
async fn create_with_null_timezone_is_rejected_400() {
    // POST with "timezone": null -> 400 {"error":"Invalid JSON payload"}
    // Locks the contract that made the stale bundle fail loudly rather than silently.
}
```

Run: `set -a; . ./backend/.env; set +a; SQLX_OFFLINE=true S3_ENDPOINT=http://127.0.0.1:9009 RUSTFS_ENDPOINT=http://127.0.0.1:9009 cargo test -p rustshare-server --test calendar_api_test -- --ignored --test-threads=1`
Expected: pass.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/lib/components/apps/CalendarApplicationView.svelte frontend/src/lib/components/apps/CalendarApplicationView.test.ts backend/server/src/handlers/validated_json.rs backend/tests/calendar_api_test.rs
git commit -s -m "fix(calendar): harden event-create timezone and cover the day-cell path

Locks the day-cell create path with a regression test, falls back to UTC
when the browser reports no timezone, and logs the discarded serde reason
behind the generic 'Invalid JSON payload' 400 (issue #315 follow-up)."
```

---

## Task 2: Extract view-range math, add Day view and Mon–Fri work week

**Files:**
- Create: `frontend/src/lib/calendar/view-range.ts`
- Create: `frontend/src/lib/calendar/view-range.test.ts`
- Modify: `frontend/src/lib/components/apps/CalendarApplicationView.svelte`
- Modify: `frontend/src/lib/components/apps/CalendarApplicationView.test.ts`

- [ ] **Step 1: Write the failing range-math unit tests**

Create `frontend/src/lib/calendar/view-range.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { VIEW_OPTIONS, startOfWeekMonday, windowRange, shiftWindow } from './view-range';

const at = (s: string) => new Date(s);

describe('windowRange', () => {
  it('day view covers exactly the local day', () => {
    const { from, to } = windowRange('day', at('2026-10-14T15:00:00'));
    expect(from.getHours()).toBe(0);
    expect(to.getTime() - from.getTime()).toBe(24 * 3600 * 1000);
  });

  it('work week runs Monday 00:00 to Saturday 00:00 (5 days)', () => {
    const { from, to } = windowRange('week', at('2026-10-14T15:00:00')); // Wednesday
    expect(from.getDay()).toBe(1); // Monday
    expect(from.getDate()).toBe(12);
    expect(to.getDate()).toBe(17); // exclusive Saturday
    expect(to.getTime() - from.getTime()).toBe(5 * 24 * 3600 * 1000);
  });

  it('month view stays Sunday-anchored over 42 days', () => {
    const { from, to } = windowRange('month', at('2026-10-14T15:00:00'));
    expect(from.getDay()).toBe(0);
    expect(Math.round((to.getTime() - from.getTime()) / 86400000)).toBe(42);
  });

  it('agenda covers 30 days', () => {
    const { from, to } = windowRange('agenda', at('2026-10-14T15:00:00'));
    expect(Math.round((to.getTime() - from.getTime()) / 86400000)).toBe(30);
  });
});

describe('shiftWindow', () => {
  it('steps day/week/month/agenda by one period', () => {
    expect(shiftWindow('day', at('2026-10-14T10:00:00'), 1).getDate()).toBe(15);
    expect(shiftWindow('week', at('2026-10-14T10:00:00'), 1).getDate()).toBe(21);
    expect(shiftWindow('month', at('2026-10-14T10:00:00'), -1).getMonth()).toBe(8);
    expect(shiftWindow('agenda', at('2026-10-14T10:00:00'), 1).getDate()).toBe(13); // +30d
  });
});

describe('startOfWeekMonday', () => {
  it('is idempotent and lands on Monday for every weekday', () => {
    for (const d of ['2026-10-12', '2026-10-13', '2026-10-14', '2026-10-15', '2026-10-16', '2026-10-17', '2026-10-18']) {
      const monday = startOfWeekMonday(at(`${d}T12:00:00`));
      expect(monday.getDay()).toBe(1);
      expect(startOfWeekMonday(monday).getTime()).toBe(monday.getTime());
    }
  });
});

describe('VIEW_OPTIONS', () => {
  it('exposes day, work week, month and agenda with labels', () => {
    expect(VIEW_OPTIONS.map((v) => v.id)).toEqual(['day', 'week', 'month', 'agenda']);
    expect(VIEW_OPTIONS.map((v) => v.label)).toEqual(['Day', 'Work week', 'Month', 'Agenda']);
  });
});
```

Run: `cd frontend && npx vitest run src/lib/calendar/view-range.test.ts`
Expected: FAIL (module does not exist).

- [ ] **Step 2: Implement `view-range.ts`**

```ts
export type CalendarView = 'day' | 'week' | 'month' | 'agenda';

export const VIEW_OPTIONS: { id: CalendarView; label: string }[] = [
  { id: 'day', label: 'Day' },
  { id: 'week', label: 'Work week' },
  { id: 'month', label: 'Month' },
  { id: 'agenda', label: 'Agenda' },
];

export function startOfDay(date: Date): Date {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate());
}

export function addDays(date: Date, days: number): Date {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate() + days, date.getHours(), date.getMinutes(), date.getSeconds());
}

export function startOfWeekMonday(date: Date): Date {
  const day = startOfDay(date);
  const offset = (day.getDay() + 6) % 7; // Monday = 0
  return addDays(day, -offset);
}

export function windowRange(view: CalendarView, cursor: Date): { from: Date; to: Date } {
  const day = startOfDay(cursor);
  switch (view) {
    case 'day':
      return { from: day, to: addDays(day, 1) };
    case 'week': {
      const monday = startOfWeekMonday(day);
      return { from: monday, to: addDays(monday, 5) };
    }
    case 'month': {
      const first = new Date(day.getFullYear(), day.getMonth(), 1);
      const from = addDays(first, -first.getDay());
      return { from, to: addDays(from, 42) };
    }
    case 'agenda':
      return { from: day, to: addDays(day, 30) };
  }
}

export function shiftWindow(view: CalendarView, cursor: Date, direction: 1 | -1): Date {
  switch (view) {
    case 'day':
      return addDays(cursor, direction);
    case 'week':
      return addDays(cursor, 7 * direction);
    case 'month':
      return new Date(cursor.getFullYear(), cursor.getMonth() + direction, cursor.getDate());
    case 'agenda':
      return addDays(cursor, 30 * direction);
  }
}
```

Run: `cd frontend && npx vitest run src/lib/calendar/view-range.test.ts`
Expected: PASS.

- [ ] **Step 3: Wire the component to the module and add the Day view**

In `CalendarApplicationView.svelte`:
1. Delete the local `type CalendarView`, `windowRange`, `shiftWindow` and import them from `$lib/calendar/view-range` (keep `addDays`/`startOfDay` imports consistent; re-export if other code imports them from the component).
2. Drive the switcher from `VIEW_OPTIONS` (label + `aria-pressed={view === option.id}`), so the buttons read Day / Work week / Month / Agenda.
3. Week branch: `weekCells` becomes 5 cells from `startOfWeekMonday(cursor)`; the week header/body use `grid-cols-5` with labels derived from each cell's date (`['Mon','Tue','Wed','Thu','Fri']`).
4. `heading`: add a `day` branch — e.g. `Thu, Oct 2, 2026` via `toLocaleDateString(undefined, { weekday: 'short', month: 'short', day: 'numeric', year: 'numeric' })`; keep the week branch showing `Mon … – Fri …`.
5. Day view body: render a single-day column with an all-day lane at the top and hour rows 0–23. Position timed events absolutely. Add the helper `const eventKey = (event: CalendarEvent) => `${event.id}:${event.instance_start ?? ''}`;` (occurrences share the master's `id`, so `instance_start` is required in the key) and use it in every `{#each}` that lists events:

```svelte
{#each eventsOn(windowEvents, cursor) as event (eventKey(event))}
  {@const start = occurrenceStart(event)}
  {@const end = occurrenceEnd(event)}
  {@const topPct = (minutesFromMidnight(start, cursor) / 1440) * 100}
  {@const heightPct = Math.max(2, ((minutesFromMidnight(end, cursor) - minutesFromMidnight(start, cursor)) / 1440) * 100)}
  <button class="absolute left-0 right-0 ..." style={`top:${clamp(topPct, 0, 98)}%; height:${clamp(heightPct, 2, 100 - clamp(topPct, 0, 98))}%`} onclick={() => openDetail(event)}>
    {event.title}
  </button>
{/each}
```

with helpers `minutesFromMidnight(date, day)` (clamped to `[0, 1440]`) and `clamp(value, min, max)`.
6. Create affordances: add a `+` button per week cell (`aria-label="Create event on {formattedDate}"`) and per hour row in day view (`aria-label="Create event on {formattedDate} at {hour}:00"`), both calling `openCreate(date, hour?)`. Extend `openCreate(date?: Date, hour?: number)`: default 09:00–10:00, or `hour:00–hour+1:00` when `hour` is given.
7. Now-indicator: only when `view === 'day'` and `isToday(cursor)`; compute from `new Date()` and refresh with an interval that is cleaned up in the `$effect` return.

- [ ] **Step 4: Update and add component tests**

Keep the existing selectors working. Update the month-window test if the import path changed. Add these five, each asserting the exact requested window (literal ISO from/to for a pinned cursor) plus the visible outcome named in its title:

- `day view requests exactly one local day and renders one column` — switch to Day; assert the query's `from` is local midnight of the cursor and `to` is the next local midnight, and that the heading contains the weekday and date.
- `work week view requests Monday 00:00 through Saturday 00:00 and hides weekends` — switch to Work week; assert `from.getDay() === 1`, `to - from === 5 days`, the five headers read Mon–Fri, and no Sat/Sun header exists.
- `work week excludes a Sunday event` — fixture an event on the Sunday after the cursor week; assert its title is not rendered (while it is rendered in Month).
- `navigation steps by one day in day view and one week in work week` — click Next in each mode; assert the new `from` advances by 1 and 7 days respectively, staying on a Monday.
- `shows a create affordance in work-week cells and in day hour rows` — click the `Create event on …` button in a week cell and in a day hour row; assert the modal opens with that date (and the hour-slot start time for the hour row).

Run: `cd frontend && npx vitest run src/lib/components/apps/CalendarApplicationView.test.ts src/lib/calendar/view-range.test.ts && npm run check && npm run lint`
Expected: all pass, 0 svelte-check errors, prettier clean.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/lib/calendar frontend/src/lib/components/apps/CalendarApplicationView.svelte frontend/src/lib/components/apps/CalendarApplicationView.test.ts
git commit -s -m "feat(calendar): add Day view and Monday–Friday work week

Extracts view-range math into a unit-tested module, adds a single-day
hour grid with an all-day lane, converts the week view to a Mon–Fri work
week with create affordances, and covers the new ranges with tests
(issue #315 follow-up)."
```

---

## Task 3: Frontend coverage extension (API client, settings panel, view states)

**Files:**
- Modify: `frontend/src/lib/api/calendar.ts`, `frontend/src/lib/api/calendar.test.ts`
- Modify: `frontend/src/lib/settings/CalendarSettingsPanel.test.ts`
- Modify: `frontend/src/lib/components/apps/CalendarApplicationView.test.ts`

- [ ] **Step 1: Cover `connectSource`/`disconnectSource`/`resyncSource`**

Add to `calendar.test.ts` (mock `apiClient` exactly as the existing tests do):

```ts
it('connectSource requests the provider connect endpoint and returns the authorize URL', async () => {
  apiClient.get.mockResolvedValueOnce({
    authorize_url: 'https://accounts.google.com/o/oauth2/v2/auth?client_id=x&state=y'
  });
  const result = await calendarApi.connectSource('google');
  expect(apiClient.get).toHaveBeenCalledWith('/calendar/sources/google/connect');
  expect(result.authorize_url).toContain('accounts.google.com');
});

it('disconnectSource posts to the source disconnect endpoint', async () => {
  apiClient.post.mockResolvedValueOnce({});
  await calendarApi.disconnectSource('src-1');
  expect(apiClient.post).toHaveBeenCalledWith('/calendar/sources/src-1/disconnect');
});

it('disconnectSource also accepts the outlook kind', async () => {
  apiClient.post.mockResolvedValueOnce({});
  await calendarApi.disconnectSource('src-2');
  expect(apiClient.post).toHaveBeenCalledTimes(1);
});

it('resyncSource posts to the resync endpoint and propagates a 409', async () => {
  apiClient.post.mockResolvedValueOnce({});
  await calendarApi.resyncSource('src-1');
  expect(apiClient.post).toHaveBeenCalledWith('/calendar/sources/src-1/resync');

  apiClient.post.mockRejectedValueOnce(Object.assign(new Error('sync in progress'), { status: 409 }));
  await expect(calendarApi.resyncSource('src-1')).rejects.toMatchObject({ status: 409 });
});
```

If `calendarApi` does not yet expose these three methods, add them in `calendar.ts` first (they exist in the settings panel's usage — verify with `grep -n 'connectSource\|disconnectSource\|resyncSource' frontend/src/lib`). If `apiClient`'s mock lacks `post`, extend the mock factory at the top of the file the same way `mail.test.ts` does.

Run: `cd frontend && npx vitest run src/lib/api/calendar.test.ts`
Expected: PASS (add the methods to the mock setup first if the file's mock is strict).

- [ ] **Step 2: Settings-panel error-path matrix**

Add to `CalendarSettingsPanel.test.ts` — each test renders the panel with a mocked `$lib/api/calendar` and asserts rendered text and calls:

```ts
it('shows the not-configured state when connect returns 503', async () => {
  connectSourceMock.mockRejectedValueOnce(Object.assign(new Error('not configured'), { status: 503 }));
  render(CalendarSettingsPanel, { props: { applicationId: 'io.elembra.calendar' } });
  await fireEvent.click(await screen.findByRole('button', { name: /Connect Google/ }));
  expect(await screen.findByText(/not configured on this deployment/i)).toBeTruthy();
  expect(window.location.assign).not.toHaveBeenCalled();
});

it('navigates to the authorize URL on a successful connect', async () => {
  connectSourceMock.mockResolvedValueOnce({ authorize_url: 'https://accounts.google.com/o/oauth2/v2/auth?client_id=x' });
  render(CalendarSettingsPanel, { props: { applicationId: 'io.elembra.calendar' } });
  await fireEvent.click(await screen.findByRole('button', { name: /Connect Google/ }));
  await waitFor(() => expect(window.location.assign).toHaveBeenCalledWith(expect.stringContaining('accounts.google.com')));
});

it('shows a generic failure toast for a non-503 connect error', async () => {
  connectSourceMock.mockRejectedValueOnce(new Error('boom'));
  // click Connect, then expect the fixed failure copy (never the raw error message)
});

it('connects Outlook through the same flow', async () => {
  // assert GET /calendar/sources/outlook/connect is requested and the authorize URL is opened
});

it('shows a success toast after a resync', async () => {
  // resyncSource resolves -> assert the success copy appears
});

it('keeps the source list usable when disconnect fails', async () => {
  // disconnectSource rejects -> error toast shown, list still rendered, no unhandled rejection
});

it('offers Reconnect for a source parked in auth_required', async () => {
  // source.status === 'auth_required' -> a Reconnect button is rendered
});
```

For each `//` case above, the assertion is the visible outcome named in the test title (toast copy, button presence, call argument) — do not import the raw error text into the assertion.

Run: `cd frontend && npx vitest run src/lib/settings/CalendarSettingsPanel.test.ts`
Expected: PASS; fix any real defect the new tests surface (e.g. a missing error branch) rather than weakening the test.

- [ ] **Step 3: View-state tests (loading/error/omitted fields)**

Add to `CalendarApplicationView.test.ts`:

```ts
it('shows an error state with retry when the range query fails', async () => {
  listEventsMock.mockRejectedValueOnce(new Error('boom'));
  render(CalendarApplicationView, { props: { applicationId: 'io.elembra.calendar' } });
  expect(await screen.findByRole('button', { name: /retry/i })).toBeTruthy();
  await fireEvent.click(screen.getByRole('button', { name: /retry/i }));
  await waitFor(() => expect(listEventsMock).toHaveBeenCalledTimes(2));
});

it('renders an event whose source_kind is omitted', async () => {
  const { source_kind: _omitted, ...withoutKind } = eventAt('2026-10-14');
  listEventsMock.mockResolvedValueOnce({ events: [withoutKind] });
  render(CalendarApplicationView, { props: { applicationId: 'io.elembra.calendar' } });
  expect(await screen.findByText(withoutKind.title)).toBeTruthy();
});
```

Update the shared `eventAt` fixture helper to emit `…Z` timestamps (matching the wire contract) and to omit `source_kind` by default; add explicit fixtures where a source kind matters.

Run: `cd frontend && npm run check && npm run lint && npm run test`
Expected: green.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/lib/api frontend/src/lib/settings frontend/src/lib/components/apps
git commit -s -m "test(calendar): cover connect/disconnect/resync, panel error paths, and view states

Closes the frontend coverage gaps found in the QA follow-up review
(issue #315 follow-up)."
```

---

## Task 4: Backend unit coverage — validators, enums, range boundaries

**Files:**
- Modify: `backend/server/src/services/calendar_service.rs`
- Modify: `backend/crates/core/src/domain/calendar.rs`
- Test: in-source `#[cfg(test)]` modules

- [ ] **Step 1: Make validators unit-testable**

Change the visibility of `validate_timezone`, `validate_event_times`, `validate_rrule`, and the window check `fn validate_window(from, to)` in `calendar_service.rs` from private to `pub(crate)`, and extract the current inline range-window check out of the DB-bound `list_events` into that pure function (called by `list_events`).

Run: `SQLX_OFFLINE=true cargo check -p rustshare-server`
Expected: compiles.

- [ ] **Step 2: Write the failing unit tests**

Add to `calendar_service.rs`'s test module:

```rust
#[test]
fn validate_timezone_accepts_iana_and_rejects_unknown() {
    assert!(validate_timezone("Europe/Berlin").is_ok());
    assert!(validate_timezone("UTC").is_ok());
    assert!(validate_timezone("Mars/Olympus").is_err());
    assert!(validate_timezone("").is_err());
}

#[test]
fn validate_event_times_rejects_end_before_or_equal_start() {
    let start = "2026-10-14T09:00:00Z".parse().unwrap();
    assert!(validate_event_times(start, start, false).is_err());
    assert!(validate_event_times(start, "2026-10-14T08:00:00Z".parse().unwrap(), false).is_err());
    assert!(validate_event_times(start, "2026-10-14T10:00:00Z".parse().unwrap(), false).is_ok());
}

#[test]
fn validate_event_times_requires_whole_day_alignment_for_all_day() {
    let start = "2026-10-14T00:00:00Z".parse().unwrap();
    assert!(validate_event_times(start, "2026-10-16T00:00:00Z".parse().unwrap(), true).is_ok());
    assert!(validate_event_times(start, "2026-10-14T12:00:00Z".parse().unwrap(), true).is_err());
}

#[test]
fn validate_window_enforces_order_and_the_366_day_cap() {
    let from = "2026-01-01T00:00:00Z".parse().unwrap();
    assert!(validate_window(from, from).is_err());
    assert!(validate_window(from, "2026-01-01T00:00:01Z".parse().unwrap()).is_ok());
    assert!(validate_window(from, "2027-01-01T00:00:00Z".parse().unwrap()).is_ok());   // exactly 366
    assert!(validate_window(from, "2027-01-02T00:00:01Z".parse().unwrap()).is_err()); // 367
}
```

Run: `SQLX_OFFLINE=true cargo test -p rustshare-server --lib calendar`
Expected: PASS after the extraction.

- [ ] **Step 3: Enum round-trip tests**

Add to `backend/crates/core/src/domain/calendar.rs`:

```rust
#[test]
fn source_kind_round_trips_and_rejects_unknown() {
    for kind in [CalendarSourceKind::Internal, CalendarSourceKind::IcalImport, CalendarSourceKind::Google, CalendarSourceKind::Outlook] {
        assert_eq!(kind.as_str().parse::<CalendarSourceKind>().unwrap(), kind);
    }
    assert!("caldav".parse::<CalendarSourceKind>().is_err());
}
```

and equivalent tests for `CalendarSourceStatus`, `CalendarEventStatus`, `CalendarImportJobStatus`.

Run: `SQLX_OFFLINE=true cargo test -p rustshare-core --lib calendar`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add backend/server/src/services/calendar_service.rs backend/crates/core/src/domain/calendar.rs
git commit -s -m "test(calendar): unit-cover validators, enums, and range boundaries

Extracts the range-window check and widens validator visibility so the
event-input matrix runs without a database (issue #315 follow-up)."
```

---

## Task 5: Shared DB harness and mock provider (test infrastructure)

**Files:**
- Create: `backend/tests/support/mod.rs`, `backend/tests/support/calendar_harness.rs`, `backend/tests/support/mock_provider.rs`
- Modify: `backend/tests/calendar_api_test.rs`, `calendar_import_test.rs`, `calendar_google_sync_test.rs`, `calendar_outlook_sync_test.rs` (adopt the harness incrementally)

- [ ] **Step 1: Extract the duplicated harness**

Move the copy-pasted pieces (`setup_test_env`/`Harness`, `create_test_tenant`, `create_test_user`, `create_auth_token`, `cleanup_tenant`, `SERIAL` guard, `response_json`, the temp-file/multipart builder) into `backend/tests/support/calendar_harness.rs`, exposed as `pub mod support;` from each test target. Keep behavior identical; the goal is that new DB tests cost ~10 lines.

- [ ] **Step 2: Generalize the mock provider**

Merge `spawn_mock_google`/`spawn_mock_microsoft` into one `MockProvider` with pluggable paths, queued responses, failure injection, and hit counters (including `revoke_hits`). Provide constructors `MockProvider::google()` / `MockProvider::microsoft()`.

- [ ] **Step 3: Prove the harness by re-running all four suites**

Run: `set -a; . ./backend/.env; set +a; SQLX_OFFLINE=true S3_ENDPOINT=http://127.0.0.1:9009 RUSTFS_ENDPOINT=http://127.0.0.1:9009 cargo test -p rustshare-server --test calendar_api_test --test calendar_import_test --test calendar_google_sync_test --test calendar_outlook_sync_test -- --ignored --test-threads=1`
Expected: same totals as before the refactor (23 / 8 / 25 / 23).

- [ ] **Step 4: Guard against pointing tests at a deployment database**

Add a startup assertion in the harness: refuse to run when `DATABASE_URL` host is not localhost/`127.0.0.1`/`postgres` (env opt-out `RUSTSHARE_TEST_ALLOW_REMOTE_DB=1`), and document it in the module header. This prevents the `@test.local` fixture leakage found in the deployment DB.

- [ ] **Step 5: Commit**

```bash
git add backend/tests/support backend/tests/calendar_*_test.rs
git commit -s -m "test(calendar): shared DB harness and mock provider

Removes ~1000 lines of duplicated test scaffolding and blocks tests from
writing fixture rows into a deployment database (issue #315 follow-up)."
```

---

## Task 6: Make the connect flow work and observable (Stage 1 auth)

**Files:**
- Modify: `backend/server/src/config.rs`
- Modify: `backend/server/src/services/calendar_service.rs`, `google_calendar.rs`, `outlook_calendar.rs`
- Modify: `backend/server/src/handlers/calendar.rs`, `backend/server/src/routes.rs`
- Create: `backend/tests/calendar_connect_test.rs`
- Modify: `.env.example`, `backend/.env.example`

- [ ] **Step 1: Validate `RUSTSHARE_PUBLIC_URL` and log effective redirect URIs**

In `config.rs`: when any calendar provider client is configured (or always, at startup), validate `public_url`: must be an absolute `http`/`https` URL; reject the `http://localhost:5173` dev default in release builds unless `RUSTSHARE_ALLOW_DEV_PUBLIC_URL=1`; require `https` when the host is not localhost. On failure, refuse to start with a clear message naming the env var. Log, at `info`, the effective redirect URIs:

```rust
tracing::info!(google = %format!("{public_url}/api/v1/calendar/oauth/google/callback"),
               microsoft = %format!("{public_url}/api/v1/calendar/oauth/outlook/callback"),
               "calendar OAuth redirect URIs (register these verbatim in the provider console)");
```

Add `.env.example` entries and a commented checklist block near the existing calendar keys.

- [ ] **Step 2: Read-only provider-status endpoint**

Add `GET /api/v1/calendar/providers` (authenticated, tenant-enablement gated, read-only) returning:

```json
{ "public_url": "https://app.rustshare.io",
  "providers": [
    { "kind": "google", "configured": false, "redirect_uri": "https://app.rustshare.io/api/v1/calendar/oauth/google/callback" },
    { "kind": "outlook", "configured": false, "redirect_uri": "https://app.rustshare.io/api/v1/calendar/oauth/outlook/callback" }
  ] }
```

Never include client ids/secrets. Document it in `docs/contracts/calendar-application-api.md`.

- [ ] **Step 3: Surface connect failures with a reason**

Keep `?error=oauth_*` but add a `reason` code for the actionable cases (`redirect_uri`, `not_configured`, `denied`, `exchange`), and make the settings panel show a specific message plus a link to `/settings/apps/calendar` docs. Also set `prompt=consent` on the Outlook authorize URL (`outlook_calendar.rs:108-114`) so re-consent reliably returns a refresh token.

- [ ] **Step 4: HTTP-level connect/callback tests**

Create `backend/tests/calendar_connect_test.rs` using the Task 5 harness + mock provider. The full matrix with the exact assertions each test must make:

```rust
// 1. connect_unconfigured_returns_503_via_http
//    GET /api/v1/calendar/sources/google/connect with no client id configured.
//    assert: status == 503; body json.error mentions "not configured".
//
// 2. connect_configured_returns_authorize_url_with_derived_redirect_uri
//    Harness with public_url = "https://cal.example.test" and a mock Google client.
//    assert: status == 200; json.authorize_url contains "accounts.google.com";
//            url-decoded redirect_uri == "https://cal.example.test/api/v1/calendar/oauth/google/callback";
//            scope contains "calendar.readonly"; access_type=offline; prompt=consent.
//
// 3. callback_branch_matrix_redirects_with_reasons  (one test, five requests)
//    a) GET /api/v1/calendar/oauth/google/callback            -> 302 Location /settings/apps/calendar?error=oauth_state
//    b) ...?error=access_denied&state=<valid>                 -> 302 ...?error=oauth_denied
//    c) /oauth/outlook/callback?state=<google-state>&code=x   -> 302 ...?error=oauth_state   (kind mismatch)
//    d) exchange returns 400                                  -> 302 ...?error=oauth_exchange
//    e) exchange returns tokens                               -> 302 ...?connected=google
//    assert for each: status == 302, Location starts with the frontend path, response body is empty.
//
// 4. callback_response_never_contains_token_material
//    On the success case, assert the Location header contains neither the code nor any token
//    and that the response body has zero bytes.
//
// 5. provider_status_reports_configuration_and_redirect_uris
//    GET /api/v1/calendar/providers.
//    assert: 200; body.public_url == harness public_url; one entry per provider with
//            configured: false when unset, true with a mock client; redirect_uri derived
//            from public_url; no field containing a client secret.
//
// 6. outlook_authorize_url_requests_consent
//    assert the Microsoft authorize URL contains prompt=consent and scope includes Calendars.Read offline_access.
```

Run: `set -a; . ./backend/.env; set +a; SQLX_OFFLINE=true cargo test -p rustshare-server --test calendar_connect_test -- --ignored --test-threads=1`
Expected: pass.

- [ ] **Step 5: Commit**

```bash
git add backend .env.example
git commit -s -m "feat(calendar): validate public URL, expose provider status, cover connect flow

Refuses to boot with a dev public URL, logs the exact redirect URIs an
operator must register, adds a read-only provider-status endpoint, gives
connect failures actionable reasons, and tests the 503/302 paths at the
HTTP layer (issue #315 follow-up)."
```

---

## Task 7: Operator documentation and deployment hygiene

**Files:**
- Modify: `docs/specs/calendar-application-v1alpha1.md`, `docs/adr/0037-calendar-application-and-external-sync.md`, `CHANGELOG.md`
- Append: `docs/plans/2026-10-01-issue-315-calendar-application.md` (amendment section)
- Ops: clean the leaked `@test.local` sources in the dev/QA database

- [ ] **Step 1: Spec + ADR updates**

- Spec: add a **UI views** subsection (Day, Work week Mon–Fri, Month, Agenda, the window each requests, and that the range API is window-agnostic ≤366 days); make `RUSTSHARE_PUBLIC_URL` a hard prerequisite with the exact redirect URI pattern; document the provider-status endpoint.
- ADR-0037: update the frontend decision line (`:149-150`) and acceptance criterion (`:284`) to include the Day view and Mon–Fri work week; add a consequence noting that the OAuth flow requires operator-side client registration and that a non-public `RUSTSHARE_PUBLIC_URL` is now a startup error.

- [ ] **Step 2: Plan amendment**

Append to the executed plan a short `## Amendment (2026-10-02)` section recording: the Day/work-week change, the create-event root cause (stale bundle) and the hardening, the connect diagnosis and the config/validation work, and the reference to this follow-up plan.

- [ ] **Step 3: CHANGELOG**

Add under `[Unreleased] → Added/Changed`: Day view, Monday–Friday work week, provider-status endpoint; `Fixed`: event creation hardening, provider config validation. Reference the QA follow-up.

- [ ] **Step 4: Deployment hygiene (run against the dev/QA DB, not in a migration)**

```sql
-- Inspect first
SELECT id, kind, display_name, status, last_error FROM calendar_sources
WHERE display_name LIKE '%@test.local%' OR display_name LIKE 'Test Google%';
-- Then soft-delete the leftovers (keeps audit trail)
UPDATE calendar_sources SET is_enabled = false, deleted_at = now()
WHERE (display_name LIKE '%@test.local%' OR display_name LIKE 'Test Google%') AND deleted_at IS NULL;
```

Record the outcome in the PR discussion; do not ship this as a migration.

- [ ] **Step 5: Commit**

```bash
git add docs CHANGELOG.md
git commit -s -m "docs(calendar): document views, provider setup, and QA follow-up changes"
```

---

## Task 8: Operator end-to-end validation of Google sync (manual, this deployment)

**No code.** This is the runbook that proves Stage 1 works here; it is part of the deliverable.

- [ ] **Step 1: Set the deployment configuration** (in `.env`, then `docker compose up -d backend`):

```dotenv
RUSTSHARE_PUBLIC_URL=https://app.rustshare.io
RUSTSHARE_CALENDAR_GOOGLE_CLIENT_ID=<from Google Cloud Console>
RUSTSHARE_CALENDAR_GOOGLE_CLIENT_SECRET=<from Google Cloud Console>
RUSTSHARE_CALENDAR_SYNC_WORKER_ENABLED=true
```

- [ ] **Step 2: Create the Google Cloud OAuth client**: enable *Google Calendar API*; OAuth consent screen (External; add the connecting accounts as test users while in Testing); create an OAuth client of type **Web application** with authorized redirect URI exactly `https://app.rustshare.io/api/v1/calendar/oauth/google/callback`; scope `https://www.googleapis.com/auth/calendar.readonly`.

- [ ] **Step 3: Verify via the new endpoint**: `GET /api/v1/calendar/providers` reports `configured: true` and the redirect URI above.

- [ ] **Step 4: Connect and sync**: Settings → Apps → Calendar → *Connect Google Calendar* → consent → expect `?connected=google`; confirm the source shows `healthy` with `last_synced_at` and that events appear in Day/Work-week/Month views with source attribution.

- [ ] **Step 5: Negative checks**: revoke the app in the Google account → the source parks `auth_required` and the UI offers *Reconnect*; ensure no token material appears in logs or API responses.

- [ ] **Step 6: Record results** in the PR (including the 7-day refresh-token expiry caveat while the Google app remains in Testing).

---

## Deferred (separate follow-up plans, in priority order)

1. **Admin-UI provider credential provisioning** — mirror `admin/config/oidc` (`handlers/admin/config.rs`, routes at `routes.rs:973-998`): store client id/secret encrypted, mask secrets as `***`, validate the public URL, invalidate the runtime cache. Removes the need for shell access to configure providers.
2. **Zero-console ICS secret-URL source** (`kind = 'ical_url'`) — the highest-value self-hoster path (Google "secret address in iCal format", Microsoft published calendars). Requires: a `kind` CHECK migration, an encrypted `source_url_enc` column, an `ical_url` cursor kind, and a fetch worker with hardened SSRF controls (scheme allowlist, DNS resolution + private/link-local IP rejection, no redirects into private ranges, size/time caps). Includes tests for each SSRF failure mode.
3. **Device-code flow (RFC 8628)** — for deployments where a public redirect URI is impractical; needs a device-flow state table, polling UI, `slow_down` handling, and anti-phishing controls.
4. **Sync-cadence and observability polish** — expose `DEFAULT_SYNC_INTERVAL`, per-run sync history, and last-error reasons on a source-detail view.
5. **Remaining test backlog** (from the coverage analysis) — DB matrices for source CRUD via HTTP, import 413/MIME accept, lease-boundary and `LeaseLost` worker branches, token-leak assertions on update/delete envelopes, and cross-owner 404 across all verbs.

**Declined:** service-account/domain-wide delegation and Microsoft app-only credentials (they grant deployment-wide calendar reads and contradict the owner-only visibility model), and CalDAV (provider support for basic auth is gone; it remains a non-goal in the spec).

---

## Verification summary for the whole plan

```bash
# Rust
cargo fmt --all --check
SQLX_OFFLINE=true cargo clippy --workspace --all-targets --all-features -- -D warnings
SQLX_OFFLINE=true cargo test --workspace --all-features --lib
cargo sqlx prepare --workspace --check
# DB suites (dev/test DB only; the harness now refuses remote hosts)
set -a; . ./backend/.env; set +a; SQLX_OFFLINE=true S3_ENDPOINT=http://127.0.0.1:9009 RUSTFS_ENDPOINT=http://127.0.0.1:9009 \
  cargo test -p rustshare-server --test calendar_api_test --test calendar_import_test \
  --test calendar_google_sync_test --test calendar_outlook_sync_test --test calendar_connect_test \
  -- --ignored --test-threads=1
# Frontend
cd frontend && npm run check && npm run lint && npm run test && npm run build
```

Manual acceptance: Day view renders a single day with an hour grid; Work week shows Mon–Fri only; creating an event from a day cell succeeds; `GET /calendar/providers` reports the deployment's configuration; connecting Google end-to-end syncs events (Task 8).
