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
/// ::1). Returns a refusal message — never the URL or its credentials — when
/// the destination is not provably disposable.
pub fn validate_disposable_database_url(database_url: &str) -> Result<(), String> {
    let parsed = url::Url::parse(database_url)
        .map_err(|error| format!("DATABASE_URL is not a valid URL: {error}"))?;
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
