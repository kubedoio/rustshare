use axum::{
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use governor::{
    clock::DefaultClock,
    state::{InMemoryState, NotKeyed},
    Quota, RateLimiter,
};
use std::{num::NonZeroU32, sync::Arc};

/// Rate limiter configuration for different endpoint types
#[derive(Clone)]
pub struct RateLimitConfig {
    pub share_session: Arc<RateLimiter<NotKeyed, InMemoryState, DefaultClock>>,
    pub share_info: Arc<RateLimiter<NotKeyed, InMemoryState, DefaultClock>>,
    pub share_download: Arc<RateLimiter<NotKeyed, InMemoryState, DefaultClock>>,
    pub authenticated: Arc<RateLimiter<NotKeyed, InMemoryState, DefaultClock>>,
}

impl RateLimitConfig {
    /// Create new rate limiter configuration with default quotas
    pub fn new() -> Self {
        Self {
            // 5 requests per minute for session creation (prevent brute-force password attacks)
            share_session: Arc::new(RateLimiter::direct(Quota::per_minute(
                NonZeroU32::new(5).unwrap(),
            ))),
            // 20 requests per minute for info endpoint
            share_info: Arc::new(RateLimiter::direct(Quota::per_minute(
                NonZeroU32::new(20).unwrap(),
            ))),
            // 10 requests per minute for downloads
            share_download: Arc::new(RateLimiter::direct(Quota::per_minute(
                NonZeroU32::new(10).unwrap(),
            ))),
            // 100 requests per minute for authenticated endpoints
            authenticated: Arc::new(RateLimiter::direct(Quota::per_minute(
                NonZeroU32::new(100).unwrap(),
            ))),
        }
    }
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Rate limiting middleware
///
/// Applies different rate limits based on endpoint path:
/// - POST /api/public/share/:token/session: 5 req/min (prevent brute-force)
/// - GET /api/public/share/:token/info: 20 req/min
/// - GET /api/public/share/:token/file: 10 req/min
/// - Authenticated share endpoints: 100 req/min
///
/// Returns 429 Too Many Requests when limit is exceeded.
pub async fn rate_limit_middleware(
    axum::extract::State(state): axum::extract::State<crate::AppState>,
    request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path();

    // Get client IP from connection info for logging
    let client_ip = request
        .extensions()
        .get::<std::net::SocketAddr>()
        .map(|addr| addr.ip().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    // Determine which rate limiter to use based on path
    let limiter = if path.contains("/api/public/share/") && path.ends_with("/session") {
        // Most restrictive: session creation (prevent password brute-force)
        Some(&state.rate_limit_config.share_session)
    } else if path.contains("/api/public/share/") && (path.ends_with("/info") || path.contains("/info?")) {
        Some(&state.rate_limit_config.share_info)
    } else if path.contains("/api/public/share/") && (path.ends_with("/file") || path.contains("/file?")) {
        Some(&state.rate_limit_config.share_download)
    } else if path.starts_with("/api/files/") && path.contains("/shares") {
        Some(&state.rate_limit_config.authenticated)
    } else {
        // No rate limiting for other endpoints
        None
    };

    // If no limiter applies, pass through
    let limiter = match limiter {
        Some(l) => l,
        None => return next.run(request).await,
    };

    // Check rate limit using token bucket algorithm
    match limiter.check() {
        Ok(_) => {
            // Request allowed
            next.run(request).await
        }
        Err(_) => {
            // Rate limit exceeded
            tracing::warn!(
                "Rate limit exceeded for IP: {} on path: {}",
                client_ip,
                path
            );
            (
                StatusCode::TOO_MANY_REQUESTS,
                Json(serde_json::json!({
                    "error": "Too many requests. Please try again later."
                })),
            )
                .into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limit_config_creation() {
        let config = RateLimitConfig::new();
        // Verify limiters are created and can check
        assert!(config.share_session.check().is_ok());
        assert!(config.share_info.check().is_ok());
        assert!(config.share_download.check().is_ok());
        assert!(config.authenticated.check().is_ok());
    }

    #[test]
    fn test_rate_limit_enforcement() {
        // Create a limiter with quota of 2 per minute
        let limiter = RateLimiter::direct(Quota::per_minute(NonZeroU32::new(2).unwrap()));

        // First two requests should succeed
        assert!(limiter.check().is_ok());
        assert!(limiter.check().is_ok());

        // Third should fail
        assert!(limiter.check().is_err());
    }

    #[test]
    fn test_rate_limit_different_quotas() {
        let config = RateLimitConfig::new();

        // Session endpoint should have strictest limit (5/min)
        for _ in 0..5 {
            assert!(config.share_session.check().is_ok());
        }
        assert!(config.share_session.check().is_err());

        // Info endpoint should have higher limit (20/min)
        for _ in 0..20 {
            assert!(config.share_info.check().is_ok());
        }
        assert!(config.share_info.check().is_err());
    }

    #[test]
    fn test_default_implementation() {
        let config = RateLimitConfig::default();
        assert!(config.share_session.check().is_ok());
    }
}
