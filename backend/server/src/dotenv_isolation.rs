//! Child-process dotenv-isolation regression tests (issue #366).
//!
//! Library tests must never import a `.env` file: `dotenvy::dotenv()` walks
//! parent directories, so one test helper that dotenv-loads can silently pull
//! an unrelated checkout's `.env` into the shared test-process environment —
//! pointing DB tests at the application database and flipping SSRF-related
//! flags that other tests read (the environmental failures in
//! `handlers::auth` and `handlers::chat_identity` at base).
//!
//! These tests re-invoke the current test binary as child processes with a
//! controlled cwd (a temp dir that may hold a synthetic `.env`) and a
//! controlled environment. The parent process env and cwd are never mutated.
//! The refusal and parity drivers below must fail if `dotenvy::dotenv()` is
//! ever restored to the auth test helper.

use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

/// Marker var written into the synthetic `.env`; its value must never appear
/// in a child's environment or output.
const MARKER_VAR: &str = "RUSTSHARE_DOTENV_ISOLATION_MARKER";
const MARKER_VALUE: &str = "dotenv-marker-value-must-stay-unimported";

/// Chat provisioning var written into the synthetic `.env`. Imported into a
/// child, it would flip `config::tests::chat_provisioning_auto_without_relay_url_fails`.
const DECOY_RELAY_VAR: &str = "RUSTSHARE_CHAT_BOOTSTRAP_RELAY_URL";
const DECOY_RELAY_URL: &str = "wss://dotenv.example.test";

/// SSRF dev flag written into the synthetic `.env`; imported into a child it
/// flips the chat_identity relay-rejection test (the base environmental
/// failure this module pins down).
const DECOY_LOCAL_RELAY_FLAG: &str = "RUSTSHARE_CHAT_ALLOW_LOCAL_RELAY";

/// Credential-looking decoy `DATABASE_URL` in the synthetic `.env`. It must
/// never be contacted (loopback port 1 refuses instantly) and its password
/// must never be printed.
const DECOY_DATABASE_URL: &str =
    "postgres://dotenv_decoy_user:dotenv_decoy_password@127.0.0.1:1/rustshare_test";
const DECOY_PASSWORD: &str = "dotenv_decoy_password";

/// Set on child processes only; gates the strict probe assertions so the
/// probe is a no-op in normal (parent) test runs, where an ambient shell may
/// legitimately export some of these vars.
const CHILD_SENTINEL_VAR: &str = "RUSTSHARE_DOTENV_ISOLATION_CHILD";

const CHILD_TIMEOUT: Duration = Duration::from_secs(120);

/// libtest substring filter matching exactly the seven DB-backed login tests.
const LOGIN_TEST_FILTER: &str = "handlers::auth::tests::login_";
/// The Argon2 timing-ratio test is excluded from child runs: it is
/// load-sensitive and the parent process may be running the same test
/// concurrently. It remains covered by the parent's `--ignored` run.
const LOGIN_TIMING_TEST_PATH: &str = "handlers::auth::tests::login_timing_attack_resistance";
const CONFIG_TEST_PATH: &str = "config::tests::chat_provisioning_auto_without_relay_url_fails";
const CHAT_IDENTITY_TEST_PATH: &str =
    "handlers::chat_identity::tests::relay_url_rejects_private_or_credentialed_targets";
const PROBE_TEST_PATH: &str = "dotenv_isolation::probe_rejects_imported_dotenv_values";
const REFUSAL_TEST_PATH: &str = "handlers::auth::tests::login_without_tenant_id_still_works";

/// Write the synthetic `.env` that must stay unimported.
fn write_synthetic_env_file(dir: &Path) {
    std::fs::write(
        dir.join(".env"),
        format!(
            "{MARKER_VAR}={MARKER_VALUE}\n{DECOY_RELAY_VAR}={DECOY_RELAY_URL}\n{DECOY_LOCAL_RELAY_FLAG}=true\nDATABASE_URL={DECOY_DATABASE_URL}\n"
        ),
    )
    .expect("write synthetic .env");
}

struct ChildRun {
    output: String,
    success: bool,
}

/// Re-invoke this test binary in a child process. The child's environment is
/// pre-cleaned of every var the synthetic `.env` defines, so any presence in
/// the child can only come from a dotenv import; `mutate` applies the
/// per-scenario env/cwd adjustments.
fn run_child(
    test_args: &[&str],
    cwd: &Path,
    timeout: Duration,
    mutate: impl FnOnce(&mut Command),
) -> ChildRun {
    let exe = std::env::current_exe().expect("locate current test binary");
    let mut command = Command::new(exe);
    command
        .args(test_args)
        .arg("--nocapture")
        .arg("--include-ignored")
        .arg("--test-threads=1")
        .current_dir(cwd)
        .env_remove(MARKER_VAR)
        .env_remove(DECOY_RELAY_VAR)
        .env_remove(DECOY_LOCAL_RELAY_FLAG)
        .env(CHILD_SENTINEL_VAR, "1");
    mutate(&mut command);
    // Capture the pipes explicitly: `spawn` inherits them by default, which
    // would leave `wait_with_output` with nothing to collect. The child
    // output stays small (test-harness panic/summary lines), so polling
    // `try_wait` before reading cannot deadlock on a full pipe buffer.
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let child = command.spawn().expect("spawn child test binary");
    let output = wait_with_timeout(child, timeout, test_args);
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    ChildRun {
        success: output.status.success(),
        output: text,
    }
}

fn wait_with_timeout(mut child: Child, timeout: Duration, label: &[&str]) -> Output {
    let deadline = Instant::now() + timeout;
    loop {
        if child.try_wait().expect("poll child").is_some() {
            return child.wait_with_output().expect("collect child output");
        }
        if Instant::now() >= deadline {
            // Kill first, then collect: the panic must carry the child's own
            // output (otherwise a hung child reports only its filter label and
            // the reason for the hang is lost).
            let _ = child.kill();
            let collected = child
                .wait_with_output()
                .map(|output| {
                    format!(
                        "\n--- child stdout ---\n{}\n--- child stderr ---\n{}",
                        excerpt(&output.stdout),
                        excerpt(&output.stderr)
                    )
                })
                .unwrap_or_default();
            panic!("child test binary timed out after {timeout:?}: {label:?}{collected}");
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// Bound captured child output so a timeout panic stays readable.
fn excerpt(bytes: &[u8]) -> String {
    const LIMIT: usize = 4_000;
    let text = String::from_utf8_lossy(bytes);
    if text.len() <= LIMIT {
        return text.into_owned();
    }
    let mut end = LIMIT;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}… [truncated]", &text[..end])
}

/// Assert the synthetic `.env` stayed unimported and unprinted: no value from
/// it may appear in the child output.
fn assert_no_dotenv_leak(run: &ChildRun) {
    for needle in [MARKER_VALUE, DECOY_RELAY_URL, DECOY_PASSWORD] {
        assert!(
            !run.output.contains(needle),
            "child output leaked `{needle}` from the synthetic .env:\n{}",
            run.output
        );
    }
}

// ---------------------------------------------------------------------------
// Guard unit tests: pure URL validation, no live services, no children.
// ---------------------------------------------------------------------------

#[test]
fn guard_accepts_disposable_loopback_test_urls() {
    for url in [
        "postgres://u:p@127.0.0.1:15432/rustshare_test",
        "postgres://u:p@127.0.0.1:15432/rustshare_test_i366",
        "postgres://u:p@localhost:5432/rustshare_test",
        "postgres://u:p@[::1]:5432/rustshare_test",
    ] {
        assert!(
            crate::test_guard::validate_disposable_database_url(url).is_ok(),
            "{url} must be accepted"
        );
    }
}

#[test]
fn guard_rejects_non_test_database_names() {
    for url in [
        "postgres://u:p@127.0.0.1:5432/rustshare", // application database
        "postgres://u:p@127.0.0.1:5432/rustshare_testing", // near-miss prefix
        "postgres://u:p@127.0.0.1:5432/postgres",
        "postgres://u:p@127.0.0.1:5432/",
    ] {
        let error = crate::test_guard::validate_disposable_database_url(url)
            .expect_err("non-test database name must be rejected before connecting");
        assert!(error.contains("not disposable"), "{error}");
    }
}

#[test]
fn guard_rejects_non_loopback_hosts() {
    for url in [
        "postgres://u:p@203.0.113.10:5432/rustshare_test",
        "postgres://u:p@db.internal.test:5432/rustshare_test",
        "postgres://u:p@10.0.0.5:5432/rustshare_test",
        // Empty host: sqlx would fall back to a unix socket.
        "postgres:///rustshare_test",
        // Percent-encoded socket path: sqlx percent-decodes the host and then
        // treats a leading `/` as a unix socket, so this must stay refused.
        "postgres://%2Fvar%2Frun%2Fpostgresql/rustshare_test",
    ] {
        let error = crate::test_guard::validate_disposable_database_url(url)
            .expect_err("non-loopback host must be rejected before connecting");
        assert!(error.contains("not loopback"), "{error}");
    }
}

#[test]
fn guard_rejection_messages_never_print_credentials() {
    let error = crate::test_guard::validate_disposable_database_url(
        "postgres://dotenv_decoy_user:dotenv_decoy_password@203.0.113.10:5432/rustshare_test",
    )
    .expect_err("remote host must be rejected");
    assert!(
        error.contains("203.0.113.10"),
        "host helps diagnosis: {error}"
    );
    assert!(
        !error.contains(DECOY_PASSWORD),
        "leaked credential: {error}"
    );

    let error = crate::test_guard::validate_disposable_database_url(&format!(
        "postgres://u:p@127.0.0.1:15432/rustshare_test?{DECOY_PASSWORD}=1"
    ))
    .expect_err("malformed connection parameter must be refused");
    assert!(
        !error.contains(DECOY_PASSWORD),
        "credential-shaped parameter name leaked: {error}"
    );

    let error = crate::test_guard::validate_disposable_database_url(
        "postgres://dotenv_decoy_user:dotenv_decoy_password@not a url",
    )
    .expect_err("malformed URL must be rejected");
    assert!(
        !error.contains(DECOY_PASSWORD),
        "leaked credential: {error}"
    );
}

#[test]
fn guard_rejects_connection_parameter_redirects() {
    // sqlx honours these as connection parameters, so a host/path check alone
    // would let them redirect the connection (or use a unix socket).
    for url in [
        "postgres://u:p@127.0.0.1:15432/rustshare_test?dbname=postgres",
        "postgres://u:p@127.0.0.1:15432/rustshare_test?host=192.0.2.1",
        "postgres://u:p@127.0.0.1:15432/rustshare_test?hostaddr=192.0.2.1",
        "postgres://u:p@127.0.0.1:15432/rustshare_test?host=/var/run/postgresql",
        "postgres://u:p@127.0.0.1:15432/rustshare_test?port=5439&dbname=rustshare",
        "postgres://u:p@127.0.0.1:15432/rustshare_test?sslmode=disable",
    ] {
        let error = crate::test_guard::validate_disposable_database_url(url)
            .expect_err("connection parameters must be refused before connecting");
        assert!(error.contains("connection parameters"), "{error}");
    }

    let error = crate::test_guard::validate_disposable_database_url(
        "postgres://u:p@127.0.0.1:15432/rustshare_test?password=dotenv_decoy_password",
    )
    .expect_err("connection parameters must be refused");
    assert!(
        !error.contains(DECOY_PASSWORD),
        "leaked credential: {error}"
    );

    // A malformed component with no `=` is not a parameter name: the value
    // must not be echoed either.
    let error = crate::test_guard::validate_disposable_database_url(&format!(
        "postgres://u:p@127.0.0.1:15432/rustshare_test?{DECOY_PASSWORD}"
    ))
    .expect_err("connection parameters must be refused");
    assert!(
        !error.contains(DECOY_PASSWORD),
        "leaked credential: {error}"
    );

    // Even malformed names must stay out of diagnostics.
    let error = crate::test_guard::validate_disposable_database_url(
        "postgres://u:p@127.0.0.1:15432/rustshare_test?___=1",
    )
    .expect_err("connection parameters must be refused");
    assert!(
        error.contains("connection parameters") && !error.contains("___"),
        "delimiter-only names must be redacted: {error}"
    );
}

#[test]
fn guard_rejects_non_postgres_schemes() {
    for url in [
        "mysql://u:p@127.0.0.1:15432/rustshare_test",
        "http://u:p@127.0.0.1:15432/rustshare_test",
    ] {
        let error = crate::test_guard::validate_disposable_database_url(url)
            .expect_err("non-PostgreSQL scheme must be refused before connecting");
        assert!(error.contains("not PostgreSQL"), "{error}");
    }
}

// ---------------------------------------------------------------------------
// Child-process drivers (never mutate the parent process env or cwd).
// ---------------------------------------------------------------------------

/// With no DATABASE_URL in the child environment, the login test must refuse
/// in setup — before connecting — even though the child cwd holds a `.env`
/// that supplies a (decoy) DATABASE_URL, and no `.env` value may be imported
/// or printed. Fails if `dotenvy::dotenv()` is restored to the auth helper:
/// the child then loads the decoy and spends the helper's 60s acquire timeout
/// retrying the refused connection before panicking with the connect error,
/// so this driver allows for that bound.
#[test]
fn child_refuses_missing_database_url_despite_env_file() {
    let temp = tempfile::tempdir().expect("temp dir with synthetic .env");
    write_synthetic_env_file(temp.path());

    let refusal = run_child(
        &["--exact", REFUSAL_TEST_PATH],
        temp.path(),
        Duration::from_secs(90),
        |command| {
            command.env_remove("DATABASE_URL");
        },
    );
    assert!(
        !refusal.success,
        "login test must refuse without DATABASE_URL:\n{}",
        refusal.output
    );
    assert!(
        refusal
            .output
            .contains("refusing to run database-backed login tests"),
        "expected the pre-connect guard refusal, got:\n{}",
        refusal.output
    );
    assert!(
        !refusal
            .output
            .contains("Failed to connect to test database"),
        "the guard must refuse before any connection attempt:\n{}",
        refusal.output
    );
    assert_no_dotenv_leak(&refusal);

    let probe = run_child(
        &["--exact", PROBE_TEST_PATH],
        temp.path(),
        Duration::from_secs(30),
        |command| {
            command.env_remove("DATABASE_URL");
        },
    );
    assert!(
        probe.success,
        "probe must pass with a synthetic .env in the child cwd:\n{}",
        probe.output
    );
    assert_no_dotenv_leak(&probe);
}

/// The config parity test and the chat_identity relay-rejection test must
/// pass identically whether or not the child cwd contains a synthetic `.env`;
/// the chat_identity test is the exact test that failed environmentally at
/// base once the `.env` flag leaked into the shared process env.
#[test]
fn child_config_and_chat_identity_results_match_with_and_without_env_file() {
    let with_env = tempfile::tempdir().expect("temp dir with synthetic .env");
    write_synthetic_env_file(with_env.path());
    let without_env = tempfile::tempdir().expect("plain temp dir");

    for test_path in [CONFIG_TEST_PATH, CHAT_IDENTITY_TEST_PATH] {
        let with = run_child(
            &["--exact", test_path],
            with_env.path(),
            CHILD_TIMEOUT,
            |_| {},
        );
        let without = run_child(
            &["--exact", test_path],
            without_env.path(),
            CHILD_TIMEOUT,
            |_| {},
        );
        assert!(
            with.success,
            "{test_path} failed with a synthetic .env in the child cwd:\n{}",
            with.output
        );
        assert!(
            without.success,
            "{test_path} failed without a .env:\n{}",
            without.output
        );
        assert_no_dotenv_leak(&with);
        assert_no_dotenv_leak(&without);
    }
}

/// With an explicit disposable DATABASE_URL, the DB-backed login tests must
/// pass identically from a directory containing the synthetic `.env` and one
/// without — proving the supplied URL is used (the `.env` decoy points at a
/// closed port) and no unrelated `.env` keys are imported. The load-sensitive
/// Argon2 timing test is skipped here (see [`LOGIN_TIMING_TEST_PATH`]).
#[tokio::test]
#[ignore = "requires disposable PostgreSQL (rustshare_test on loopback) via DATABASE_URL; run with -- --ignored"]
async fn child_login_suite_matches_with_and_without_env_file() {
    let database_url =
        crate::test_guard::disposable_database_url_from_env().unwrap_or_else(|message| {
            panic!("dotenv_isolation login parity requires a disposable DATABASE_URL: {message}")
        });

    let with_env = tempfile::tempdir().expect("temp dir with synthetic .env");
    write_synthetic_env_file(with_env.path());
    let without_env = tempfile::tempdir().expect("plain temp dir");

    for dir in [with_env.path(), without_env.path()] {
        let run = run_child(
            &[LOGIN_TEST_FILTER, "--skip", LOGIN_TIMING_TEST_PATH],
            dir,
            CHILD_TIMEOUT,
            |command| {
                command.env("DATABASE_URL", &database_url);
            },
        );
        assert!(
            run.success,
            "login suite must pass against the supplied disposable DATABASE_URL (cwd {}):\n{}",
            dir.display(),
            run.output
        );
        assert!(
            run.output.contains("6 passed; 0 failed"),
            "expected the six non-timing login tests to run and pass:\n{}",
            run.output
        );
        assert_no_dotenv_leak(&run);

        let probe = run_child(
            &["--exact", PROBE_TEST_PATH],
            dir,
            Duration::from_secs(30),
            |command| {
                command.env("DATABASE_URL", &database_url);
            },
        );
        assert!(probe.success, "probe must stay green:\n{}", probe.output);
    }
}

/// Runs in child processes only (gated by [`CHILD_SENTINEL_VAR`]): asserts
/// none of the synthetic `.env` values were imported into the child
/// environment. With a supplied database, exercise auth setup in this same
/// process: a separate child cannot observe another child's environment.
/// Without the sentinel it is a deliberate no-op.
#[tokio::test]
async fn probe_rejects_imported_dotenv_values() {
    if std::env::var_os(CHILD_SENTINEL_VAR).is_none() {
        return;
    }
    if std::env::var_os("DATABASE_URL").is_some() {
        let pool = crate::handlers::auth::tests::test_db_pool().await;
        pool.close().await;
    }
    for var in [MARKER_VAR, DECOY_RELAY_VAR, DECOY_LOCAL_RELAY_FLAG] {
        assert!(
            std::env::var_os(var).is_none(),
            "child environment imported `{var}` from a .env file; library tests \
             must never dotenv-load (issue #366)"
        );
    }
}
