use axum::{
    extract::{ConnectInfo, Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use governor::{
    clock::DefaultClock,
    state::keyed::DashMapStateStore,
    Quota, RateLimiter,
};
use std::{net::IpAddr, num::NonZeroU32, sync::Arc};

/// Rate limiter configuration for different endpoint types (per-IP)
#[derive(Clone)]
pub struct RateLimitConfig {
    pub share_session: Arc<RateLimiter<IpAddr, DashMapStateStore<IpAddr>, DefaultClock>>,
    pub share_info: Arc<RateLimiter<IpAddr, DashMapStateStore<IpAddr>, DefaultClock>>,
    pub share_download: Arc<RateLimiter<IpAddr, DashMapStateStore<IpAddr>, DefaultClock>>,
    pub authenticated: Arc<RateLimiter<IpAddr, DashMapStateStore<IpAddr>, DefaultClock>>,
}

impl RateLimitConfig {
    /// Create new rate limiter configuration with default quotas (per-IP)
    pub fn new() -> Self {
        Self {
            // 5 requests per minute PER IP for session creation (prevent brute-force password attacks)
            share_session: Arc::new(RateLimiter::dashmap(Quota::per_minute(
                NonZeroU32::new(5).unwrap(),
            ))),
            // 20 requests per minute PER IP for info endpoint
            share_info: Arc::new(RateLimiter::dashmap(Quota::per_minute(
                NonZeroU32::new(20).unwrap(),
            ))),
            // 10 requests per minute PER IP for downloads
            share_download: Arc::new(RateLimiter::dashmap(Quota::per_minute(
                NonZeroU32::new(10).unwrap(),
            ))),
            // 100 requests per minute PER IP for authenticated endpoints
            authenticated: Arc::new(RateLimiter::dashmap(Quota::per_minute(
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

/// Rate limiting middleware (per-IP)
///
/// Applies different rate limits based on endpoint path, with each IP address
/// getting its own independent quota:
/// - POST /api/public/share/:token/session: 5 req/min per IP (prevent brute-force)
/// - GET /api/public/share/:token/info: 20 req/min per IP
/// - GET /api/public/share/:token/file: 10 req/min per IP
/// - Authenticated share endpoints: 100 req/min per IP
///
/// Returns 429 Too Many Requests when limit is exceeded.
pub async fn rate_limit_middleware(
    State(state): State<crate::AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    request: Request,
    next: Next,
) -> Response {
    let client_ip = addr.ip();
    let path = request.uri().path();

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

    // Check rate limit FOR THIS SPECIFIC IP using token bucket algorithm
    match limiter.check_key(&client_ip) {
        Ok(_) => {
            // Request allowed
            next.run(request).await
        }
        Err(_) => {
            // Rate limit exceeded for this IP
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
        // Verify limiters are created with keyed state
        let test_ip: IpAddr = "127.0.0.1".parse().unwrap();
        assert!(config.share_session.check_key(&test_ip).is_ok());
        assert!(config.share_info.check_key(&test_ip).is_ok());
        assert!(config.share_download.check_key(&test_ip).is_ok());
        assert!(config.authenticated.check_key(&test_ip).is_ok());
    }

    #[tokio::test]
    async fn test_per_ip_rate_limit_enforcement() {
        // Create a limiter with quota of 2 per minute PER IP
        let limiter = RateLimiter::dashmap(Quota::per_minute(NonZeroU32::new(2).unwrap()));

        let ip1: IpAddr = "192.168.1.1".parse().unwrap();
        let ip2: IpAddr = "192.168.1.2".parse().unwrap();

        // IP1: First two requests should succeed
        assert!(limiter.check_key(&ip1).is_ok());
        assert!(limiter.check_key(&ip1).is_ok());

        // IP1: Third should fail (quota exhausted)
        assert!(limiter.check_key(&ip1).is_err());

        // IP2: Should still have full quota (independent of IP1)
        assert!(limiter.check_key(&ip2).is_ok());
        assert!(limiter.check_key(&ip2).is_ok());
        assert!(limiter.check_key(&ip2).is_err()); // IP2's third fails
    }

    #[test]
    fn test_rate_limit_different_quotas() {
        let config = RateLimitConfig::new();
        let test_ip: IpAddr = "10.0.0.1".parse().unwrap();

        // Each limiter should have independent quota per IP
        assert!(config.share_session.check_key(&test_ip).is_ok());
        assert!(config.share_info.check_key(&test_ip).is_ok());
        assert!(config.share_download.check_key(&test_ip).is_ok());
        assert!(config.authenticated.check_key(&test_ip).is_ok());

        // Session endpoint should have strictest limit (5/min per IP)
        for _ in 1..5 {
            assert!(config.share_session.check_key(&test_ip).is_ok());
        }
        assert!(config.share_session.check_key(&test_ip).is_err());

        // Info endpoint should have higher limit (20/min per IP)
        let test_ip2: IpAddr = "10.0.0.2".parse().unwrap();
        for _ in 0..20 {
            assert!(config.share_info.check_key(&test_ip2).is_ok());
        }
        assert!(config.share_info.check_key(&test_ip2).is_err());
    }

    #[test]
    fn test_default_implementation() {
        let config = RateLimitConfig::default();
        let test_ip: IpAddr = "172.16.0.1".parse().unwrap();
        assert!(config.share_session.check_key(&test_ip).is_ok());
    }
}
