//! DB-backed integration suite for the Calendar Application .ics import
//! (issue #315, Task 2 of the calendar implementation plan).
//!
//! Covers: multipart upload → `202` with a pending job, background worker
//! processing to `completed`, range queries returning the imported events
//! (TZID converted to UTC, all-day spans), and idempotent re-import: a
//! second upload of the identical file creates no duplicate rows.
//!
//! DB-backed and `#[ignore]`d; run against the dev database (migrations
//! applied) with `--test-threads=1`:
//!
//!   set -a; . ./backend/.env; set +a; SQLX_OFFLINE=true \
//!     cargo test -p rustshare-server --test calendar_import_test -- \
//!       --ignored --test-threads=1
//!
//! Every test takes the shared `SERIAL` guard and cleans up exactly the rows
//! it created under fresh tenants.

use std::io::BufReader;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use rustshare_server::state::AppState;
use serde_json::Value;
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

mod support;

use support::calendar_harness::*;

const ICS_FIXTURE: &str = "\
BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//RustShare//Test//EN
BEGIN:VEVENT
UID:import-minimal-1
DTSTART:20261005T140000Z
DTEND:20261005T150000Z
SUMMARY:Minimal meeting
END:VEVENT
BEGIN:VEVENT
UID:import-tzid-1
DTSTART;TZID=Europe/Berlin:20261006T140000
DTEND;TZID=Europe/Berlin:20261006T150000
SUMMARY:Berlin meeting
END:VEVENT
BEGIN:VEVENT
UID:import-allday-1
DTSTART;VALUE=DATE:20261007
DTEND;VALUE=DATE:20261008
SUMMARY:All day off
END:VEVENT
BEGIN:VEVENT
UID:import-recur-1
DTSTART:20261008T140000Z
DTEND:20261008T150000Z
RRULE:FREQ=WEEKLY;COUNT=3
SUMMARY:Weekly sync
END:VEVENT
BEGIN:VEVENT
UID:import-recur-1
RECURRENCE-ID:20261015T140000Z
DTSTART:20261015T160000Z
DTEND:20261015T170000Z
SUMMARY:Weekly sync moved
END:VEVENT
BEGIN:VTODO
UID:import-todo-1
SUMMARY:A task that must be skipped
END:VTODO
END:VCALENDAR
";

/// POST an arbitrary multipart body to the import endpoint.
async fn upload_multipart(
    app: &axum::Router<()>,
    token: &str,
    body: Vec<u8>,
) -> (StatusCode, Value) {
    support::calendar_harness::upload_multipart(
        app,
        "/api/v1/calendar/import",
        token,
        IMPORT_BOUNDARY,
        body,
    )
    .await
}

async fn upload_ics(
    app: &axum::Router<()>,
    token: &str,
    filename: &str,
    content: &str,
) -> (StatusCode, Value) {
    upload_multipart(
        app,
        token,
        multipart_file_body(IMPORT_BOUNDARY, filename, content),
    )
    .await
}

async fn spawn_import_worker(state: &AppState) {
    rustshare_server::calendar_import_worker::spawn_calendar_import_worker(
        Arc::clone(&state.metadata_store),
        Arc::clone(&state.outbox_store),
        state.shutdown_tx.subscribe(),
        rustshare_server::calendar_import_worker::CalendarImportWorkerConfig {
            poll_interval: std::time::Duration::from_millis(250),
            max_concurrent_jobs: 2,
            stale_threshold: std::time::Duration::from_secs(300),
        },
    );
}

/// Poll `GET /api/v1/calendar/import-jobs/{id}` until the job reaches a
/// terminal state; callers assert the expected status.
async fn wait_for_job(app: &axum::Router<()>, token: &str, job_id: Uuid) -> Value {
    wait_for_job_with_timeout(app, token, job_id, std::time::Duration::from_secs(30)).await
}

async fn wait_for_job_with_timeout(
    app: &axum::Router<()>,
    token: &str,
    job_id: Uuid,
    timeout: std::time::Duration,
) -> Value {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/calendar/import-jobs/{job_id}"))
                    .header("Authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let (status, body) = response_json(response).await;
        assert_eq!(status, StatusCode::OK);
        let status = body["status"].as_str().unwrap();
        if matches!(status, "completed" | "failed" | "cancelled") {
            return body;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "import job {job_id} did not finish in time"
        );
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

/// Poll the outbox until the worker's best-effort `imported.v1` envelope shows
/// up. The worker commits the job status before publishing, so the envelope may
/// lag the terminal status; assert only once it lands (or the deadline passes).
async fn wait_for_imported_envelope(pool: &PgPool, tenant_id: Uuid) -> Value {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let envelope = sqlx::query_scalar::<_, Value>(
            "SELECT event_json FROM integration_outbox \
             WHERE tenant_id = $1 AND event_type = 'io.elembra.calendar.event.imported.v1' \
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(tenant_id)
        .fetch_optional(pool)
        .await
        .expect("query integration_outbox");
        if let Some(envelope) = envelope {
            return envelope;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "imported.v1 envelope was not published in time"
        );
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

async fn count_imported_events(state: &AppState, tenant_id: Uuid) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM calendar_events WHERE tenant_id = $1 AND external_uid IS NOT NULL",
    )
    .bind(tenant_id)
    .fetch_one(&state.db_pool)
    .await
    .expect("count imported events")
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn ics_upload_import_and_reimport_is_idempotent() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env_without_calendar_outbox().await;
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let user = create_test_user(&state, "calendar_import", tenant_id).await;
    configure_calendar(&state, tenant_id, user.id, true).await;
    let token = create_auth_token(&state, user.id, tenant_id);
    let app = build_app(state.clone());
    spawn_import_worker(&state).await;

    // First upload: accepted with a pending job.
    let (status, body) = upload_ics(&app, &token, "export.ics", ICS_FIXTURE).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let job_id = Uuid::parse_str(body["job_id"].as_str().unwrap()).unwrap();
    let source_id = Uuid::parse_str(body["source_id"].as_str().unwrap()).unwrap();
    assert_eq!(body["status"], "pending");

    // The worker picks it up and completes it.
    let job = wait_for_job(&app, &token, job_id).await;
    assert_eq!(job["status"], "completed");
    assert_eq!(job["failed_events"], 0);
    assert_eq!(job["total_events"], 5);
    assert_eq!(job["processed_events"], 5);

    // The completed run published one imported.v1 envelope with counts and
    // the source ResourceRef — identifiers/counts only, never titles. The
    // worker commits the status before the best-effort publish, so poll.
    let envelope = wait_for_imported_envelope(&state.db_pool, tenant_id).await;
    assert_eq!(envelope["data"]["processed_events"], 5);
    assert_eq!(envelope["data"]["total_events"], 5);
    assert_eq!(envelope["elembraResource"]["resourceType"], "source");
    assert_eq!(
        envelope["elembraResource"]["resourceId"],
        source_id.to_string()
    );
    assert!(
        envelope["data"].get("title").is_none(),
        "import events must not carry titles"
    );

    // Range query returns the imported events with converted times.
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/calendar/events?from=2026-10-01T00:00:00Z&to=2026-10-31T00:00:00Z")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, body) = response_json(response).await;
    assert_eq!(status, StatusCode::OK);
    let events = body["events"].as_array().unwrap();
    let titles: Vec<&str> = events
        .iter()
        .map(|event| event["title"].as_str().unwrap())
        .collect();
    assert!(titles.contains(&"Minimal meeting"));
    assert!(titles.contains(&"Berlin meeting"));
    assert!(titles.contains(&"All day off"));
    // The weekly master expands (2 in-window instances; the third is
    // overridden) and the override row is returned as stored.
    assert!(titles.contains(&"Weekly sync"));
    assert!(titles.contains(&"Weekly sync moved"));

    // TZID event is stored as the correct UTC instant (14:00 CEST = 12:00Z).
    let berlin = events
        .iter()
        .find(|event| event["title"] == "Berlin meeting")
        .unwrap();
    assert_eq!(berlin["starts_at"], "2026-10-06T12:00:00Z");
    assert_eq!(berlin["timezone"], "Europe/Berlin");
    // All-day event is a UTC-midnight span.
    let all_day = events
        .iter()
        .find(|event| event["title"] == "All day off")
        .unwrap();
    assert_eq!(all_day["starts_at"], "2026-10-07T00:00:00Z");
    assert_eq!(all_day["ends_at"], "2026-10-08T00:00:00Z");
    assert_eq!(all_day["all_day"], true);
    // Imported events are read-only mirrors of the file.
    assert!(events.iter().all(|event| event["read_only"] == true));

    // Row count after the first import.
    let first_count = count_imported_events(&state, tenant_id).await;
    assert_eq!(first_count, 5);

    // Re-upload the identical file (same filename reuses the source) and let
    // the worker process it: no duplicate rows appear.
    let (status, body) = upload_ics(&app, &token, "export.ics", ICS_FIXTURE).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(body["source_id"], source_id.to_string());
    let second_job = wait_for_job(
        &app,
        &token,
        Uuid::parse_str(body["job_id"].as_str().unwrap()).unwrap(),
    )
    .await;
    assert_eq!(second_job["status"], "completed");
    assert_eq!(second_job["total_events"], 5);
    assert_eq!(second_job["processed_events"], 5);

    let second_count = count_imported_events(&state, tenant_id).await;
    assert_eq!(
        first_count, second_count,
        "re-import must not duplicate rows"
    );

    // The job list shows both runs.
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/calendar/import-jobs")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, body) = response_json(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["jobs"].as_array().unwrap().len(), 2);

    cleanup_outbox(&state.db_pool, tenant_id).await;
    cleanup_tenant(&state.db_pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn imported_recurring_utc_event_exports_with_uid_times_and_rrule() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env_without_calendar_outbox().await;
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let user = create_test_user(&state, "calendar_import_export", tenant_id).await;
    configure_calendar(&state, tenant_id, user.id, true).await;
    let token = create_auth_token(&state, user.id, tenant_id);
    let app = build_app(state.clone());
    spawn_import_worker(&state).await;

    let ics = "\
BEGIN:VCALENDAR
VERSION:2.0
BEGIN:VEVENT
UID: item\\,weekly\\;source\\\\partner@example.test\x20
DTSTART:20261008T140000Z
DTEND:20261008T150000Z
RRULE:FREQ=WEEKLY;COUNT=3
SUMMARY:Issue 329 weekly sync
END:VEVENT
END:VCALENDAR
";
    let (status, body) = upload_ics(&app, &token, "issue-329.ics", ics).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let job = wait_for_job(
        &app,
        &token,
        Uuid::parse_str(body["job_id"].as_str().unwrap()).unwrap(),
    )
    .await;
    assert_eq!(job["status"], "completed");
    assert_eq!(job["failed_events"], 0);
    assert_eq!(job["processed_events"], 1);

    let event_id = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM calendar_events \
         WHERE tenant_id = $1 AND owner_id = $2 \
           AND external_uid = $3 \
           AND recurrence_id IS NULL",
    )
    .bind(tenant_id)
    .bind(user.id)
    .bind(" item,weekly;source\\partner@example.test ")
    .fetch_one(&state.db_pool)
    .await
    .expect("find imported recurring event");

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/calendar/events/{event_id}/export"))
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let exported = String::from_utf8(
        axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("read exported calendar")
            .to_vec(),
    )
    .expect("exported calendar should be UTF-8");

    let properties: Vec<&str> = exported.lines().collect();
    assert!(properties.contains(&"UID: item\\,weekly\\;source\\\\partner@example.test\x20"));

    let mut parser = ical::IcalParser::new(BufReader::new(exported.as_bytes()));
    let calendar = parser
        .next()
        .expect("one exported VCALENDAR")
        .expect("independent parser accepts exported VCALENDAR");
    assert!(parser.next().is_none(), "exactly one exported VCALENDAR");
    assert_eq!(calendar.events.len(), 1, "one exported VEVENT");
    let exported_event = &calendar.events[0];
    let property_value = |name: &str| {
        exported_event
            .properties
            .iter()
            .find(|property| property.name == name)
            .and_then(|property| property.value.as_deref())
            .unwrap_or_else(|| panic!("exported event is missing {name}"))
    };
    assert_eq!(property_value("DTSTART"), "20261008T140000Z");
    assert_eq!(property_value("DTEND"), "20261008T150000Z");
    assert_eq!(property_value("RRULE"), "FREQ=WEEKLY;COUNT=3");

    cleanup_tenant(&state.db_pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn unsupported_ics_fields_are_reported_without_importing_or_disclosing_values() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env_without_calendar_outbox().await;
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let user = create_test_user(&state, "calendar_import_unsupported", tenant_id).await;
    configure_calendar(&state, tenant_id, user.id, true).await;
    let token = create_auth_token(&state, user.id, tenant_id);
    let app = build_app(state.clone());
    spawn_import_worker(&state).await;

    let ics = "\
BEGIN:VCALENDAR
VERSION:2.0
BEGIN:VEVENT
UID:issue-329-unsupported@example.test
DTSTART:20261008T140000Z
DTEND:20261008T150000Z
ORGANIZER:mailto:private@example.test
ATTENDEE;CN=Private:mailto:private@example.test
SUMMARY:Meeting with unsupported participants
END:VEVENT
END:VCALENDAR
";
    let (status, body) = upload_ics(&app, &token, "unsupported.ics", ics).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let job = wait_for_job(
        &app,
        &token,
        Uuid::parse_str(body["job_id"].as_str().unwrap()).unwrap(),
    )
    .await;

    assert_eq!(job["status"], "completed");
    assert_eq!(job["total_events"], 1);
    assert_eq!(job["processed_events"], 0);
    assert_eq!(job["failed_events"], 1);
    let error = job["last_error"].as_str().expect("actionable import error");
    assert!(error.contains("ATTENDEE, ORGANIZER"), "error was {error}");
    assert!(
        error.contains("event was not imported"),
        "error was {error}"
    );
    assert!(!error.contains("private@example.test"));

    let imported_count = sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM calendar_events \
         WHERE tenant_id = $1 AND owner_id = $2 \
           AND external_uid = 'issue-329-unsupported@example.test'",
    )
    .bind(tenant_id)
    .bind(user.id)
    .fetch_one(&state.db_pool)
    .await
    .expect("count events with unsupported properties");
    assert_eq!(imported_count, 0);

    let range_ics = "\
BEGIN:VCALENDAR
VERSION:2.0
BEGIN:VEVENT
UID:issue-329-range@example.test
DTSTART:20261008T140000Z
DTEND:20261008T150000Z
RRULE:FREQ=WEEKLY;COUNT=3
SUMMARY:Weekly meeting
END:VEVENT
BEGIN:VEVENT
UID:issue-329-range@example.test
RECURRENCE-ID:20261015T140000Z
RECURRENCE-ID;RANGE=THISANDFUTURE:20261015T140000Z
DTSTART:20261015T160000Z
DTEND:20261015T170000Z
SUMMARY:Changed and following meetings
END:VEVENT
END:VCALENDAR
";
    let (status, body) = upload_ics(&app, &token, "range.ics", range_ics).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let range_job = wait_for_job(
        &app,
        &token,
        Uuid::parse_str(body["job_id"].as_str().unwrap()).unwrap(),
    )
    .await;

    assert_eq!(range_job["status"], "completed");
    assert_eq!(range_job["total_events"], 2);
    assert_eq!(range_job["processed_events"], 1);
    assert_eq!(range_job["failed_events"], 1);
    let range_error = range_job["last_error"]
        .as_str()
        .expect("unsupported RANGE diagnostic");
    assert!(range_error.contains("multiple RECURRENCE-ID properties"));
    assert!(range_error.contains("RECURRENCE-ID RANGE"));
    assert!(range_error.contains("event was not imported"));
    assert!(!range_error.contains("THISANDFUTURE"));

    let imported_range_overrides = sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM calendar_events \
         WHERE tenant_id = $1 AND owner_id = $2 \
           AND external_uid = 'issue-329-range@example.test' \
           AND recurrence_id IS NOT NULL",
    )
    .bind(tenant_id)
    .bind(user.id)
    .fetch_one(&state.db_pool)
    .await
    .expect("count imported RANGE overrides");
    assert_eq!(imported_range_overrides, 0);

    cleanup_tenant(&state.db_pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn structurally_invalid_ics_fails_the_job() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env_without_calendar_outbox().await;
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let user = create_test_user(&state, "calendar_import_bad", tenant_id).await;
    configure_calendar(&state, tenant_id, user.id, true).await;
    let token = create_auth_token(&state, user.id, tenant_id);
    let app = build_app(state.clone());
    spawn_import_worker(&state).await;

    let (status, body) = upload_ics(&app, &token, "broken.ics", "not a calendar").await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let job = wait_for_job(
        &app,
        &token,
        Uuid::parse_str(body["job_id"].as_str().unwrap()).unwrap(),
    )
    .await;
    assert_eq!(job["status"], "failed");
    assert!(job["last_error"].is_string());

    assert_eq!(count_imported_events(&state, tenant_id).await, 0);

    cleanup_tenant(&state.db_pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn single_event_upsert_failure_does_not_fail_the_job() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env_without_calendar_outbox().await;
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let user = create_test_user(&state, "calendar_import_onebad", tenant_id).await;
    configure_calendar(&state, tenant_id, user.id, true).await;
    let token = create_auth_token(&state, user.id, tenant_id);
    let app = build_app(state.clone());
    spawn_import_worker(&state).await;

    // The first event violates the ends_at > starts_at table CHECK; the
    // second is well-formed. Per-component semantics: the job still
    // completes and the good event is imported.
    let ics = "\
BEGIN:VCALENDAR
BEGIN:VEVENT
UID:bad-window-1
DTSTART:20261005T150000Z
DTEND:20261005T140000Z
SUMMARY:Backwards window
END:VEVENT
BEGIN:VEVENT
UID:good-after-bad-1
DTSTART:20261006T140000Z
DTEND:20261006T150000Z
SUMMARY:Good event
END:VEVENT
END:VCALENDAR
";

    let (status, body) = upload_ics(&app, &token, "onebad.ics", ics).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let job = wait_for_job(
        &app,
        &token,
        Uuid::parse_str(body["job_id"].as_str().unwrap()).unwrap(),
    )
    .await;

    // The bad row counts as its own failure; the job itself completes.
    assert_eq!(job["status"], "completed");
    assert_eq!(job["total_events"], 2);
    assert_eq!(job["failed_events"], 1);
    assert_eq!(job["processed_events"], 1);
    assert!(job["last_error"].is_string());

    // Only the good event was persisted.
    assert_eq!(count_imported_events(&state, tenant_id).await, 1);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/calendar/events?from=2026-10-01T00:00:00Z&to=2026-10-31T00:00:00Z")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, body) = response_json(response).await;
    assert_eq!(status, StatusCode::OK);
    let events = body["events"].as_array().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["title"], "Good event");

    cleanup_outbox(&state.db_pool, tenant_id).await;
    cleanup_tenant(&state.db_pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn non_ics_upload_is_rejected() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env_without_calendar_outbox().await;
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let user = create_test_user(&state, "calendar_import_type", tenant_id).await;
    configure_calendar(&state, tenant_id, user.id, true).await;
    let token = create_auth_token(&state, user.id, tenant_id);
    let app = build_app(state.clone());

    let body = multipart_file_body_with_type(
        "calendar-import-boundary",
        "notes.txt",
        "text/plain",
        "not a calendar",
    );
    let (status, _) = upload_multipart(&app, &token, body).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    cleanup_tenant(&state.db_pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn oversized_ics_upload_is_rejected_before_creating_import_state() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env_without_calendar_outbox().await;
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let user = create_test_user(&state, "calendar_import_oversize", tenant_id).await;
    configure_calendar(&state, tenant_id, user.id, true).await;
    let token = create_auth_token(&state, user.id, tenant_id);
    let app = build_app(state.clone());

    // Keep the request one byte over the documented 10 MiB ICS field cap.
    let oversized_ics = "x".repeat(10 * 1024 * 1024 + 1);
    let (status, body) = upload_ics(&app, &token, "oversized.ics", &oversized_ics).await;

    // Unknown fields are ignored semantically, but still count toward the
    // route's total 11 MiB multipart request bound.
    let oversized_ignored_field = "x".repeat(11 * 1024 * 1024);
    let oversized_request = format!(
        "--{IMPORT_BOUNDARY}\r\n\
         Content-Disposition: form-data; name=\"file\"; filename=\"small.ics\"\r\n\
         Content-Type: text/calendar\r\n\r\n\
         {ICS_FIXTURE}\r\n\
         --{IMPORT_BOUNDARY}\r\n\
         Content-Disposition: form-data; name=\"ignored\"\r\n\r\n\
         {oversized_ignored_field}\r\n\
         --{IMPORT_BOUNDARY}--\r\n"
    );
    let (request_status, request_body) =
        upload_multipart(&app, &token, oversized_request.into_bytes()).await;

    let import_state: (i64, i64) = sqlx::query_as(
        "SELECT \
            (SELECT count(*) FROM calendar_import_jobs WHERE tenant_id = $1), \
            (SELECT count(*) FROM calendar_sources WHERE tenant_id = $1)",
    )
    .bind(tenant_id)
    .fetch_one(&state.db_pool)
    .await
    .expect("count import state after oversized upload");

    cleanup_tenant(&state.db_pool, tenant_id).await;

    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE, "response: {body}");
    assert_eq!(
        request_status,
        StatusCode::PAYLOAD_TOO_LARGE,
        "response: {request_body}"
    );
    assert_eq!(import_state, (0, 0));
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn duplicate_multipart_fields_are_rejected() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env_without_calendar_outbox().await;
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let user = create_test_user(&state, "calendar_import_dup", tenant_id).await;
    configure_calendar(&state, tenant_id, user.id, true).await;
    let token = create_auth_token(&state, user.id, tenant_id);
    let app = build_app(state.clone());
    let boundary = "calendar-import-boundary";

    let two_files = format!(
        "--{boundary}\r\n\
         Content-Disposition: form-data; name=\"file\"; filename=\"a.ics\"\r\n\
         Content-Type: text/calendar\r\n\
         \r\n\
         {ICS_FIXTURE}\r\n\
         --{boundary}\r\n\
         Content-Disposition: form-data; name=\"file\"; filename=\"b.ics\"\r\n\
         Content-Type: text/calendar\r\n\
         \r\n\
         {ICS_FIXTURE}\r\n\
         --{boundary}--\r\n"
    );
    let (status, _) = upload_multipart(&app, &token, two_files.into_bytes()).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "duplicate file field");

    let source_id = Uuid::new_v4();
    let two_sources = format!(
        "--{boundary}\r\n\
         Content-Disposition: form-data; name=\"file\"; filename=\"c.ics\"\r\n\
         Content-Type: text/calendar\r\n\
         \r\n\
         {ICS_FIXTURE}\r\n\
         --{boundary}\r\n\
         Content-Disposition: form-data; name=\"source_id\"\r\n\
         \r\n\
         {source_id}\r\n\
         --{boundary}\r\n\
         Content-Disposition: form-data; name=\"source_id\"\r\n\
         \r\n\
         {source_id}\r\n\
         --{boundary}--\r\n"
    );
    let (status, _) = upload_multipart(&app, &token, two_sources.into_bytes()).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "duplicate source_id field");

    cleanup_outbox(&state.db_pool, tenant_id).await;
    cleanup_tenant(&state.db_pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn source_id_must_belong_to_the_caller_and_be_ical_import() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env_without_calendar_outbox().await;
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let owner = create_test_user(&state, "calendar_import_owner", tenant_id).await;
    let other = create_test_user(&state, "calendar_import_other", tenant_id).await;
    configure_calendar(&state, tenant_id, owner.id, true).await;
    let owner_token = create_auth_token(&state, owner.id, tenant_id);
    let other_token = create_auth_token(&state, other.id, tenant_id);
    let app = build_app(state.clone());

    let source_id: Uuid = sqlx::query_scalar(
        "INSERT INTO calendar_sources (tenant_id, owner_id, kind, display_name, created_at, updated_at) \
         VALUES ($1, $2, 'ical_import', 'Owner import', NOW(), NOW()) RETURNING id",
    )
    .bind(tenant_id)
    .bind(owner.id)
    .fetch_one(&state.db_pool)
    .await
    .expect("create ical_import source");

    // Another user in the same tenant cannot target that source: not found.
    let body = format!(
        "--calendar-import-boundary\r\n\
         Content-Disposition: form-data; name=\"file\"; filename=\"d.ics\"\r\n\
         Content-Type: text/calendar\r\n\
         \r\n\
         {ICS_FIXTURE}\r\n\
         --calendar-import-boundary\r\n\
         Content-Disposition: form-data; name=\"source_id\"\r\n\
         \r\n\
         {}\r\n\
         --calendar-import-boundary--\r\n",
        source_id
    );
    let (status, _) = upload_multipart(&app, &other_token, body.into_bytes()).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // An `internal` source is not a valid import target: bad request.
    let internal = state
        .calendar_service
        .ensure_internal_source(tenant_id, owner.id)
        .await
        .expect("internal source");
    let body = format!(
        "--calendar-import-boundary\r\n\
         Content-Disposition: form-data; name=\"file\"; filename=\"e.ics\"\r\n\
         Content-Type: text/calendar\r\n\
         \r\n\
         {ICS_FIXTURE}\r\n\
         --calendar-import-boundary\r\n\
         Content-Disposition: form-data; name=\"source_id\"\r\n\
         \r\n\
         {}\r\n\
         --calendar-import-boundary--\r\n",
        internal.id
    );
    let (status, _) = upload_multipart(&app, &owner_token, body.into_bytes()).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    cleanup_outbox(&state.db_pool, tenant_id).await;
    cleanup_tenant(&state.db_pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn cancelled_import_job_does_not_publish_completion_event() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env_without_calendar_outbox().await;
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let user = create_test_user(&state, "calendar_import_cancel", tenant_id).await;
    configure_calendar(&state, tenant_id, user.id, true).await;
    let token = create_auth_token(&state, user.id, tenant_id);
    let app = build_app(state.clone());

    // No worker runs, so the job stays pending; cancel it while a worker would
    // have been processing it.
    let (status, body) = upload_ics(&app, &token, "cancel.ics", ICS_FIXTURE).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let job_id = Uuid::parse_str(body["job_id"].as_str().unwrap()).unwrap();

    let mut job = state
        .metadata_store
        .get_calendar_import_job(tenant_id, user.id, job_id)
        .await
        .expect("load job")
        .expect("job exists");
    sqlx::query("UPDATE calendar_import_jobs SET status = 'cancelled' WHERE id = $1")
        .bind(job_id)
        .execute(&state.db_pool)
        .await
        .expect("cancel job");
    // The worker still holds the job as running in memory.
    job.status = "running".to_string();

    let outcome = rustshare_server::services::ical_import::process_import_job(
        &state.metadata_store,
        &state.outbox_store,
        &job,
    )
    .await
    .expect("processing succeeds");
    assert!(outcome.processed_events > 0);

    let published = sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM integration_outbox \
         WHERE tenant_id = $1 AND event_type = 'io.elembra.calendar.event.imported.v1'",
    )
    .bind(tenant_id)
    .fetch_one(&state.db_pool)
    .await
    .expect("count outbox rows");
    assert_eq!(
        published, 0,
        "a job that lost the running race must not publish a success event"
    );

    let stored_status =
        sqlx::query_scalar::<_, String>("SELECT status FROM calendar_import_jobs WHERE id = $1")
            .bind(job_id)
            .fetch_one(&state.db_pool)
            .await
            .expect("load job status");
    assert_eq!(stored_status, "cancelled");

    cleanup_outbox(&state.db_pool, tenant_id).await;
    cleanup_tenant(&state.db_pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn huge_duration_is_a_component_failure_without_stalling_the_job() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env_without_calendar_outbox().await;
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let user = create_test_user(&state, "calendar_import_duration", tenant_id).await;
    configure_calendar(&state, tenant_id, user.id, true).await;
    let token = create_auth_token(&state, user.id, tenant_id);
    let app = build_app(state.clone());
    spawn_import_worker(&state).await;

    // A DURATION that would overflow `chrono` must be rejected per component:
    // the job still completes (the worker task must not panic and strand the
    // job running, which the stale reset would requeue forever).
    let ics = "\
BEGIN:VCALENDAR
BEGIN:VEVENT
UID:huge-duration-1
DTSTART:20261005T140000Z
DURATION:P1000000000000D
SUMMARY:Absurd duration
END:VEVENT
BEGIN:VEVENT
UID:good-duration-1
DTSTART:20261006T140000Z
DURATION:PT2H
SUMMARY:Sane duration
END:VEVENT
END:VCALENDAR
";

    let (status, body) = upload_ics(&app, &token, "duration.ics", ics).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let job = wait_for_job(
        &app,
        &token,
        Uuid::parse_str(body["job_id"].as_str().unwrap()).unwrap(),
    )
    .await;

    assert_eq!(job["status"], "completed");
    assert_eq!(job["total_events"], 2);
    assert_eq!(job["failed_events"], 1);
    assert_eq!(job["processed_events"], 1);
    assert_eq!(count_imported_events(&state, tenant_id).await, 1);

    cleanup_outbox(&state.db_pool, tenant_id).await;
    cleanup_tenant(&state.db_pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn event_limit_fails_the_job_without_persisting_partial_import() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env_without_calendar_outbox().await;
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let user = create_test_user(&state, "calendar_import_limit", tenant_id).await;
    configure_calendar(&state, tenant_id, user.id, true).await;
    let token = create_auth_token(&state, user.id, tenant_id);
    let app = build_app(state.clone());
    spawn_import_worker(&state).await;

    // Exceed the parser's event cap by one while staying below the existing
    // multipart byte limit. Since the whole file must parse before upserting,
    // this hard failure must leave the calendar unchanged.
    let mut ics = String::from("BEGIN:VCALENDAR\n");
    for index in 0..=10_000 {
        ics.push_str(&format!(
            "BEGIN:VEVENT\nUID:limit-{index}\nDTSTART:20261005T140000Z\nDTEND:20261005T150000Z\nEND:VEVENT\n"
        ));
    }
    ics.push_str("END:VCALENDAR\n");

    let (status, body) = upload_ics(&app, &token, "over-limit.ics", &ics).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let job = wait_for_job(
        &app,
        &token,
        Uuid::parse_str(body["job_id"].as_str().unwrap()).unwrap(),
    )
    .await;

    assert_eq!(job["status"], "failed");
    assert_eq!(
        job["last_error"],
        "calendar file is unreadable: calendar exceeds the 10000-VEVENT import limit"
    );
    assert_eq!(count_imported_events(&state, tenant_id).await, 0);

    cleanup_tenant(&state.db_pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn maximum_sized_calendar_import_completes_and_persists_every_event() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env_without_calendar_outbox().await;
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let user = create_test_user(&state, "calendar_import_max", tenant_id).await;
    configure_calendar(&state, tenant_id, user.id, true).await;
    let token = create_auth_token(&state, user.id, tenant_id);
    let app = build_app(state.clone());
    spawn_import_worker(&state).await;

    let mut ics = String::from("BEGIN:VCALENDAR\nVERSION:2.0\n");
    for index in 0..10_000 {
        ics.push_str(&format!(
            "BEGIN:VEVENT\nUID:maximum-{index}\nDTSTART:20261005T140000Z\nDTEND:20261005T150000Z\nEND:VEVENT\n"
        ));
    }
    ics.push_str("END:VCALENDAR\n");
    assert!(ics.len() < 10 * 1024 * 1024);
    let payload_bytes = ics.len();

    let started_at = std::time::Instant::now();
    let (status, body) = upload_ics(&app, &token, "maximum.ics", &ics).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let job = wait_for_job_with_timeout(
        &app,
        &token,
        Uuid::parse_str(body["job_id"].as_str().unwrap()).unwrap(),
        std::time::Duration::from_secs(300),
    )
    .await;
    let elapsed = started_at.elapsed();
    let imported_events = count_imported_events(&state, tenant_id).await;

    assert_eq!(job["status"], "completed");
    assert_eq!(job["total_events"], 10_000);
    assert_eq!(job["processed_events"], 10_000);
    assert_eq!(job["failed_events"], 0);
    assert_eq!(imported_events, 10_000);
    eprintln!(
        "MAX_CALENDAR_IMPORT events=10000 payload_bytes={payload_bytes} elapsed_ms={} persisted_events={imported_events}",
        elapsed.as_millis(),
    );

    cleanup_tenant(&state.db_pool, tenant_id).await;
}
