//! Disposable-destination guard for database-backed tests.
//!
//! Shared between library unit tests (e.g. the auth login tests) and
//! integration tests (e.g. `tests/share_acl_propagation.rs`) so every
//! DB-backed test refuses — before connecting — any destination that is not
//! provably the disposable `rustshare_test` PostgreSQL on loopback.
//!
//! The guard reads the environment exactly as supplied to the test process.
//! It never loads a `.env` file: dotenv-importing in one library test leaks
//! process-wide and has silently pointed other tests at the application
//! database or flipped SSRF-related flags (issue #366).

/// Loopback hosts a disposable test database may live on.
const LOOPBACK_HOSTS: [&str; 3] = ["127.0.0.1", "localhost", "::1"];

/// Validate that `database_url` points at a disposable loopback test database
/// (`rustshare_test` or a `rustshare_test_*` name on 127.0.0.1, localhost, or
/// ::1) and that nothing in the URL can redirect the connection. Returns a
/// refusal message — never the URL or its credentials — when the destination
/// is not provably disposable.
///
/// The check is stricter than the URL's host and path: sqlx resolves the
/// effective destination from connection query parameters as well (`host`,
/// `hostaddr`, `port`, `dbname`, `user`, `password`, and socket paths), so
/// `…/rustshare_test?dbname=postgres` or `?host=192.0.2.1` would otherwise
/// pass a host/path check and still connect somewhere else. Supply a plain
/// URL; connection parameters are refused.
pub fn validate_disposable_database_url(database_url: &str) -> Result<(), String> {
    let parsed = url::Url::parse(database_url)
        .map_err(|error| format!("DATABASE_URL is not a valid URL: {error}"))?;
    if parsed.scheme() != "postgres" && parsed.scheme() != "postgresql" {
        return Err(format!(
            "DATABASE_URL scheme {:?} is not PostgreSQL (expected postgres or \
             postgresql); point DATABASE_URL at the disposable rustshare_test \
             database",
            parsed.scheme()
        ));
    }
    // The url crate keeps brackets on IPv6 hosts of non-special schemes such
    // as postgres:// (e.g. "[::1]"); normalize before the loopback check.
    let host = parsed
        .host_str()
        .unwrap_or_default()
        .trim_matches(['[', ']']);
    if !LOOPBACK_HOSTS.contains(&host) {
        return Err(format!(
            "DATABASE_URL host {host:?} is not loopback (expected 127.0.0.1, \
             localhost, or ::1); point DATABASE_URL at the disposable \
             rustshare_test database"
        ));
    }
    let database_name = parsed.path().trim_start_matches('/');
    if database_name != "rustshare_test" && !database_name.starts_with("rustshare_test_") {
        return Err(format!(
            "DATABASE_URL database {database_name:?} is not disposable (expected \
             rustshare_test or rustshare_test_*); refusing to target the \
             application database"
        ));
    }
    // Connection parameters can override every part checked above (host,
    // hostaddr, port, dbname, user, password, and unix-socket host paths), so
    // refuse them outright rather than trying to re-derive the destination.
    if let Some(query) = parsed.query() {
        // Names only: parameter values can carry credentials.
        let names: Vec<&str> = query
            .split('&')
            .map(|pair| pair.split('=').next().unwrap_or_default())
            .collect();
        return Err(format!(
            "DATABASE_URL must not carry connection parameters (found \
             {names:?}); they can redirect the connection away from the \
             validated disposable rustshare_test database. Supply a plain \
             postgres:// URL"
        ));
    }
    Ok(())
}

/// Read `DATABASE_URL` from the explicitly supplied environment — never from
/// a `.env` file — and validate it with [`validate_disposable_database_url`].
pub fn disposable_database_url_from_env() -> Result<String, String> {
    let database_url = std::env::var("DATABASE_URL").map_err(|_| {
        "DATABASE_URL is not set; database-backed tests require an explicitly \
         supplied disposable rustshare_test PostgreSQL URL and never fall back \
         to a .env file"
            .to_string()
    })?;
    validate_disposable_database_url(&database_url)?;
    Ok(database_url)
}
