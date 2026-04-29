//! Retry manager with exponential backoff for sync operations

use std::future::Future;
use std::io;
use std::time::Duration;
use sync_domain::SyncError;
use tracing::{debug, warn};

/// Configuration for retry behavior
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_retries: u32,
    /// Base delay in seconds for exponential backoff
    pub base_delay_seconds: u64,
    /// Maximum delay in seconds between retries
    pub max_delay_seconds: u64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 10,
            base_delay_seconds: 1,
            max_delay_seconds: 300, // 5 minutes
        }
    }
}

impl RetryConfig {
    /// Create a new retry config with custom values
    pub fn new(max_retries: u32, base_delay_seconds: u64, max_delay_seconds: u64) -> Self {
        Self {
            max_retries,
            base_delay_seconds,
            max_delay_seconds,
        }
    }
}

/// Calculate the backoff delay for a given retry attempt
/// Uses exponential backoff: min(base * 2^retry_count, max_delay)
pub fn calculate_backoff_delay(retry_count: u32, base_seconds: u64, max_seconds: u64) -> Duration {
    // Calculate 2^retry_count, capping at u64::MAX to prevent overflow
    let exponential_factor = 2_u64.saturating_pow(retry_count);

    // Calculate base * 2^retry_count, saturating at u64::MAX
    let delay_seconds = base_seconds.saturating_mul(exponential_factor);

    // Take the minimum of calculated delay and max delay
    let final_delay = delay_seconds.min(max_seconds);

    Duration::from_secs(final_delay)
}

/// Get current Unix timestamp
pub fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Categories of error retryability
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCategory {
    /// Error is transient and can be retried
    Retryable,
    /// Error is permanent and should not be retried
    NotRetryable,
}

/// Determine if an error is retryable based on its type
pub fn is_retryable_error(error: &SyncError) -> ErrorCategory {
    match error {
        // Network errors are typically retryable
        SyncError::Network(_) => ErrorCategory::Retryable,

        // IO errors may be retryable depending on the error kind
        SyncError::Io(io_err) => is_retryable_io_error(io_err),

        // Auth errors are generally not retryable (credentials need to be fixed)
        SyncError::Auth(_) => ErrorCategory::NotRetryable,

        // Database errors - some may be retryable (lock contention), others not
        SyncError::Database(msg) => is_retryable_database_error(msg),

        // Conflicts need user intervention, not retries
        SyncError::Conflict(_) => ErrorCategory::NotRetryable,

        // Other errors - check the message for retryable patterns
        SyncError::Other(msg) => is_retryable_from_message(msg),
    }
}

/// Check if an IO error is retryable
fn is_retryable_io_error(error: &io::Error) -> ErrorCategory {
    match error.kind() {
        // Connection and network-related errors are retryable
        io::ErrorKind::ConnectionRefused
        | io::ErrorKind::ConnectionReset
        | io::ErrorKind::ConnectionAborted
        | io::ErrorKind::NotConnected
        | io::ErrorKind::BrokenPipe
        | io::ErrorKind::TimedOut
        | io::ErrorKind::Interrupted => ErrorCategory::Retryable,

        // Resource exhaustion might be temporary
        io::ErrorKind::WouldBlock => ErrorCategory::Retryable,

        // Permission and existence errors are not retryable
        io::ErrorKind::PermissionDenied
        | io::ErrorKind::NotFound
        | io::ErrorKind::AlreadyExists
        | io::ErrorKind::InvalidInput
        | io::ErrorKind::InvalidData
        | io::ErrorKind::UnexpectedEof
        | io::ErrorKind::OutOfMemory => ErrorCategory::NotRetryable,

        // Other/Uncategorized - check the error message
        _ => ErrorCategory::Retryable, // Conservative default for network-related uncategorized errors
    }
}

/// Check database error message for retryable patterns
fn is_retryable_database_error(msg: &str) -> ErrorCategory {
    let lower_msg = msg.to_lowercase();

    // Lock contention and busy errors are retryable
    if lower_msg.contains("busy")
        || lower_msg.contains("locked")
        || lower_msg.contains("lock")
        || lower_msg.contains("timeout")
        || lower_msg.contains("deadlock")
    {
        return ErrorCategory::Retryable;
    }

    // Corruption and constraint errors are not retryable
    if lower_msg.contains("corrupt")
        || lower_msg.contains("constraint")
        || lower_msg.contains("malformed")
    {
        return ErrorCategory::NotRetryable;
    }

    // Default to retryable for transient database issues
    ErrorCategory::Retryable
}

/// Check error message for retryable patterns (HTTP status codes, etc.)
fn is_retryable_from_message(msg: &str) -> ErrorCategory {
    let lower_msg = msg.to_lowercase();

    // 5xx server errors are retryable
    if lower_msg.contains("500")
        || lower_msg.contains("502")
        || lower_msg.contains("503")
        || lower_msg.contains("504")
        || lower_msg.contains("server error")
        || lower_msg.contains("service unavailable")
        || lower_msg.contains("gateway timeout")
        || lower_msg.contains("bad gateway")
    {
        return ErrorCategory::Retryable;
    }

    // 4xx client errors are not retryable (except 408 timeout, 429 rate limit)
    if lower_msg.contains("400")
        || lower_msg.contains("401")
        || lower_msg.contains("403")
        || lower_msg.contains("404")
        || lower_msg.contains("405")
        || lower_msg.contains("410")
        || lower_msg.contains("client error")
        || lower_msg.contains("bad request")
        || lower_msg.contains("unauthorized")
        || lower_msg.contains("forbidden")
        || lower_msg.contains("not found")
    {
        return ErrorCategory::NotRetryable;
    }

    // Rate limiting and timeouts are retryable
    if lower_msg.contains("429")
        || lower_msg.contains("408")
        || lower_msg.contains("rate limit")
        || lower_msg.contains("too many requests")
        || lower_msg.contains("request timeout")
    {
        return ErrorCategory::Retryable;
    }

    // DNS and resolution errors are retryable
    if lower_msg.contains("dns")
        || lower_msg.contains("resolve")
        || lower_msg.contains("lookup")
        || lower_msg.contains("name resolution")
    {
        return ErrorCategory::Retryable;
    }

    // Disk full is not retryable
    if lower_msg.contains("disk full")
        || lower_msg.contains("no space")
        || lower_msg.contains("quota exceeded")
    {
        return ErrorCategory::NotRetryable;
    }

    // Network/connection related messages are retryable
    if lower_msg.contains("timeout")
        || lower_msg.contains("connection")
        || lower_msg.contains("network")
        || lower_msg.contains("unreachable")
    {
        return ErrorCategory::Retryable;
    }

    // Default to retryable for unknown errors
    ErrorCategory::Retryable
}

/// Execute an operation with retry logic
///
/// # Arguments
/// * `config` - Retry configuration
/// * `operation_name` - Name of the operation for logging
/// * `operation` - Async closure that performs the operation
///
/// # Returns
/// * `Ok(T)` if the operation succeeds
/// * `Err(E)` if all retries are exhausted
///
/// # Example
/// ```rust
/// use sync_engine::retry::{RetryConfig, with_retry};
///
/// let config = RetryConfig::default();
/// let result = with_retry(&config, "upload file", || async {
///     // Your async operation here
///     Ok::<_, sync_domain::SyncError>(())
/// }).await;
/// ```
pub async fn with_retry<F, Fut, T, E>(
    config: &RetryConfig,
    operation_name: &str,
    operation: F,
) -> Result<T, E>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    let mut last_error: Option<E> = None;

    for attempt in 0..=config.max_retries {
        match operation().await {
            Ok(result) => {
                if attempt > 0 {
                    debug!("{} succeeded after {} retries", operation_name, attempt);
                }
                return Ok(result);
            }
            Err(error) => {
                let error_msg = error.to_string();

                // Check if we should retry this error
                // Note: We need to convert the error to check retryability
                // For SyncError, we can check directly. For other errors,
                // we treat them as retryable by default.
                let should_retry = attempt < config.max_retries;

                if !should_retry {
                    warn!(
                        "{} failed after {} attempts. Last error: {}",
                        operation_name,
                        attempt + 1,
                        error_msg
                    );
                    return Err(error);
                }

                let delay = calculate_backoff_delay(
                    attempt,
                    config.base_delay_seconds,
                    config.max_delay_seconds,
                );

                warn!(
                    "{} failed (attempt {}/{}): {}. Retrying in {:?}...",
                    operation_name,
                    attempt + 1,
                    config.max_retries + 1,
                    error_msg,
                    delay
                );

                tokio::time::sleep(delay).await;
                last_error = Some(error);
            }
        }
    }

    // This should not be reached, but just in case
    Err(last_error.expect("Last error should be set if all retries exhausted"))
}

/// Execute an operation with retry logic, specifically for SyncError
/// This version properly categorizes errors using is_retryable_error
pub async fn with_retry_sync<F, Fut, T>(
    config: &RetryConfig,
    operation_name: &str,
    operation: F,
) -> Result<T, SyncError>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T, SyncError>>,
{
    let mut last_error: Option<SyncError> = None;

    for attempt in 0..=config.max_retries {
        match operation().await {
            Ok(result) => {
                if attempt > 0 {
                    debug!("{} succeeded after {} retries", operation_name, attempt);
                }
                return Ok(result);
            }
            Err(error) => {
                let should_retry = match is_retryable_error(&error) {
                    ErrorCategory::Retryable => attempt < config.max_retries,
                    ErrorCategory::NotRetryable => false,
                };

                if !should_retry {
                    if attempt >= config.max_retries {
                        warn!(
                            "{} exhausted all {} retries. Last error: {}",
                            operation_name,
                            config.max_retries + 1,
                            error
                        );
                    } else {
                        warn!(
                            "{} encountered non-retryable error: {}",
                            operation_name, error
                        );
                    }
                    return Err(error);
                }

                let delay = calculate_backoff_delay(
                    attempt,
                    config.base_delay_seconds,
                    config.max_delay_seconds,
                );

                warn!(
                    "{} failed (attempt {}/{}): {}. Retrying in {:?}...",
                    operation_name,
                    attempt + 1,
                    config.max_retries + 1,
                    error,
                    delay
                );

                tokio::time::sleep(delay).await;
                last_error = Some(error);
            }
        }
    }

    // This should not be reached, but just in case
    Err(last_error.expect("Last error should be set if all retries exhausted"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backoff_calculation() {
        // Test exponential growth
        assert_eq!(calculate_backoff_delay(0, 1, 300), Duration::from_secs(1));
        assert_eq!(calculate_backoff_delay(1, 1, 300), Duration::from_secs(2));
        assert_eq!(calculate_backoff_delay(2, 1, 300), Duration::from_secs(4));
        assert_eq!(calculate_backoff_delay(3, 1, 300), Duration::from_secs(8));
        assert_eq!(calculate_backoff_delay(4, 1, 300), Duration::from_secs(16));

        // Test max delay cap
        assert_eq!(
            calculate_backoff_delay(10, 1, 300),
            Duration::from_secs(300) // Would be 1024, but capped at 300
        );

        // Test with different base
        assert_eq!(calculate_backoff_delay(0, 5, 100), Duration::from_secs(5));
        assert_eq!(calculate_backoff_delay(1, 5, 100), Duration::from_secs(10));
        assert_eq!(calculate_backoff_delay(2, 5, 100), Duration::from_secs(20));

        // Test max delay with higher base
        assert_eq!(
            calculate_backoff_delay(5, 10, 100),
            Duration::from_secs(100) // Would be 320, but capped at 100
        );
    }

    #[test]
    fn test_retryable_errors() {
        // Network errors should be retryable
        let network_error = SyncError::Network("connection failed".to_string());
        assert_eq!(is_retryable_error(&network_error), ErrorCategory::Retryable);

        // Auth errors should not be retryable
        let auth_error = SyncError::Auth("invalid token".to_string());
        assert_eq!(is_retryable_error(&auth_error), ErrorCategory::NotRetryable);

        // Conflict errors should not be retryable
        let conflict_error = SyncError::Conflict(std::path::PathBuf::from("/test"));
        assert_eq!(
            is_retryable_error(&conflict_error),
            ErrorCategory::NotRetryable
        );
    }

    #[test]
    fn test_io_error_categorization() {
        // Retryable IO errors
        let timeout_error = io::Error::new(io::ErrorKind::TimedOut, "timeout");
        let sync_error = SyncError::Io(timeout_error);
        assert_eq!(is_retryable_error(&sync_error), ErrorCategory::Retryable);

        let conn_refused = io::Error::new(io::ErrorKind::ConnectionRefused, "refused");
        let sync_error = SyncError::Io(conn_refused);
        assert_eq!(is_retryable_error(&sync_error), ErrorCategory::Retryable);

        // Non-retryable IO errors
        let not_found = io::Error::new(io::ErrorKind::NotFound, "file not found");
        let sync_error = SyncError::Io(not_found);
        assert_eq!(is_retryable_error(&sync_error), ErrorCategory::NotRetryable);

        let perm_denied = io::Error::new(io::ErrorKind::PermissionDenied, "access denied");
        let sync_error = SyncError::Io(perm_denied);
        assert_eq!(is_retryable_error(&sync_error), ErrorCategory::NotRetryable);
    }

    #[test]
    fn test_database_error_categorization() {
        // Retryable database errors
        let busy_error = SyncError::Database("database is locked".to_string());
        assert_eq!(is_retryable_error(&busy_error), ErrorCategory::Retryable);

        let deadlock_error = SyncError::Database("deadlock detected".to_string());
        assert_eq!(
            is_retryable_error(&deadlock_error),
            ErrorCategory::Retryable
        );

        // Non-retryable database errors
        let corrupt_error = SyncError::Database("database corrupt".to_string());
        assert_eq!(
            is_retryable_error(&corrupt_error),
            ErrorCategory::NotRetryable
        );

        let constraint_error = SyncError::Database("constraint failed".to_string());
        assert_eq!(
            is_retryable_error(&constraint_error),
            ErrorCategory::NotRetryable
        );
    }

    #[test]
    fn test_http_status_error_categorization() {
        // 5xx errors should be retryable
        let server_error = SyncError::Other("HTTP 503 Service Unavailable".to_string());
        assert_eq!(is_retryable_error(&server_error), ErrorCategory::Retryable);

        let bad_gateway = SyncError::Other("502 Bad Gateway".to_string());
        assert_eq!(is_retryable_error(&bad_gateway), ErrorCategory::Retryable);

        // 4xx errors should not be retryable
        let bad_request = SyncError::Other("HTTP 400 Bad Request".to_string());
        assert_eq!(
            is_retryable_error(&bad_request),
            ErrorCategory::NotRetryable
        );

        let not_found = SyncError::Other("404 Not Found".to_string());
        assert_eq!(is_retryable_error(&not_found), ErrorCategory::NotRetryable);

        let forbidden = SyncError::Other("403 Forbidden".to_string());
        assert_eq!(is_retryable_error(&forbidden), ErrorCategory::NotRetryable);

        // 408 and 429 should be retryable
        let timeout = SyncError::Other("408 Request Timeout".to_string());
        assert_eq!(is_retryable_error(&timeout), ErrorCategory::Retryable);

        let rate_limit = SyncError::Other("429 Rate Limit Exceeded".to_string());
        assert_eq!(is_retryable_error(&rate_limit), ErrorCategory::Retryable);
    }

    #[test]
    fn test_network_message_categorization() {
        // DNS errors should be retryable
        let dns_error = SyncError::Other("DNS lookup failed".to_string());
        assert_eq!(is_retryable_error(&dns_error), ErrorCategory::Retryable);

        // Connection errors should be retryable
        let conn_error = SyncError::Other("connection timeout".to_string());
        assert_eq!(is_retryable_error(&conn_error), ErrorCategory::Retryable);

        // Disk full should not be retryable
        let disk_full = SyncError::Other("disk full".to_string());
        assert_eq!(is_retryable_error(&disk_full), ErrorCategory::NotRetryable);
    }

    #[test]
    fn test_now_unix() {
        let now = now_unix();
        let system_now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Should be very close (within 1 second)
        assert!(now.abs_diff(system_now) <= 1);
    }

    #[test]
    fn test_retry_config_default() {
        let config = RetryConfig::default();
        assert_eq!(config.max_retries, 10);
        assert_eq!(config.base_delay_seconds, 1);
        assert_eq!(config.max_delay_seconds, 300);
    }

    #[test]
    fn test_retry_config_new() {
        let config = RetryConfig::new(5, 2, 60);
        assert_eq!(config.max_retries, 5);
        assert_eq!(config.base_delay_seconds, 2);
        assert_eq!(config.max_delay_seconds, 60);
    }

    #[tokio::test]
    async fn test_with_retry_sync_stops_on_non_retryable_errors() {
        let config = RetryConfig::new(5, 0, 0);
        let attempts = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let attempts_clone = attempts.clone();

        let result: Result<(), SyncError> = with_retry_sync(&config, "download file", move || {
            let attempts = attempts_clone.clone();
            async move {
                attempts.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Err(SyncError::Other(
                    "HTTP 404 Not Found for remote file".to_string(),
                ))
            }
        })
        .await;

        assert!(matches!(result, Err(SyncError::Other(_))));
        assert_eq!(attempts.load(std::sync::atomic::Ordering::SeqCst), 1);
    }
}
