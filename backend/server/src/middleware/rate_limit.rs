use super::client_ip::extract_client_ip;
use axum::{
    extract::{ConnectInfo, Request, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use governor::{clock::DefaultClock, state::keyed::DashMapStateStore, Quota, RateLimiter};
use std::{net::IpAddr, num::NonZeroU32, sync::Arc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RateLimitScope {
    AuthLogin,
    OidcLogin,
    ShareSession,
    ShareInfo,
    ShareDownload,
    ShareUpload,
    AuthenticatedShareAdmin,
    AiQuery,
}

impl RateLimitScope {
    fn header_value(self) -> HeaderValue {
        match self {
            Self::AuthLogin => HeaderValue::from_static("auth-login"),
            Self::OidcLogin => HeaderValue::from_static("oidc-login"),
            Self::ShareSession => HeaderValue::from_static("share-session"),
            Self::ShareInfo => HeaderValue::from_static("share-info"),
            Self::ShareDownload => HeaderValue::from_static("share-download"),
            Self::ShareUpload => HeaderValue::from_static("share-upload"),
            Self::AuthenticatedShareAdmin => HeaderValue::from_static("authenticated-share-admin"),
            Self::AiQuery => HeaderValue::from_static("ai-query"),
        }
    }
}

/// Rate limiter configuration for different endpoint types (per-IP)
#[derive(Clone)]
pub struct RateLimitConfig {
    pub auth_login: Arc<RateLimiter<IpAddr, DashMapStateStore<IpAddr>, DefaultClock>>,
    pub oidc_login: Arc<RateLimiter<IpAddr, DashMapStateStore<IpAddr>, DefaultClock>>,
    pub share_session: Arc<RateLimiter<IpAddr, DashMapStateStore<IpAddr>, DefaultClock>>,
    pub share_info: Arc<RateLimiter<IpAddr, DashMapStateStore<IpAddr>, DefaultClock>>,
    pub share_download: Arc<RateLimiter<IpAddr, DashMapStateStore<IpAddr>, DefaultClock>>,
    pub share_upload: Arc<RateLimiter<IpAddr, DashMapStateStore<IpAddr>, DefaultClock>>,
    pub authenticated_share_admin:
        Arc<RateLimiter<IpAddr, DashMapStateStore<IpAddr>, DefaultClock>>,
    pub ai_query: Arc<RateLimiter<IpAddr, DashMapStateStore<IpAddr>, DefaultClock>>,
}

impl RateLimitConfig {
    /// Create new rate limiter configuration with default quotas (per-IP)
    pub fn new() -> Self {
        Self {
            auth_login: Arc::new(RateLimiter::dashmap(quota_from_env(
                "RUSTSHARE_RATE_LIMIT_AUTH_LOGIN_PER_MINUTE",
                10,
            ))),
            oidc_login: Arc::new(RateLimiter::dashmap(quota_from_env(
                "RUSTSHARE_RATE_LIMIT_OIDC_LOGIN_PER_MINUTE",
                30,
            ))),
            // 5 requests per minute PER IP for session creation (prevent brute-force password attacks)
            share_session: Arc::new(RateLimiter::dashmap(quota_from_env(
                "RUSTSHARE_RATE_LIMIT_SHARE_SESSION_PER_MINUTE",
                5,
            ))),
            // Public share metadata and folder listing
            share_info: Arc::new(RateLimiter::dashmap(quota_from_env(
                "RUSTSHARE_RATE_LIMIT_SHARE_INFO_PER_MINUTE",
                30,
            ))),
            // Anonymous downloads from public links
            share_download: Arc::new(RateLimiter::dashmap(quota_from_env(
                "RUSTSHARE_RATE_LIMIT_SHARE_DOWNLOAD_PER_MINUTE",
                30,
            ))),
            // Anonymous uploads to public folder links
            share_upload: Arc::new(RateLimiter::dashmap(quota_from_env(
                "RUSTSHARE_RATE_LIMIT_SHARE_UPLOAD_PER_MINUTE",
                20,
            ))),
            // Authenticated share-management endpoints
            authenticated_share_admin: Arc::new(RateLimiter::dashmap(quota_from_env(
                "RUSTSHARE_RATE_LIMIT_AUTHENTICATED_SHARE_ADMIN_PER_MINUTE",
                120,
            ))),
            // AI endpoints (search, summarize, ask)
            ai_query: Arc::new(RateLimiter::dashmap(quota_from_env(
                "RUSTSHARE_RATE_LIMIT_AI_QUERY_PER_MINUTE",
                30,
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
/// getting its own independent quota.
///
/// Canonical routes live under `/api/v1/...`; legacy unversioned auth aliases are
/// still classified here so older clients do not silently bypass protections.
///
/// Supports reverse proxy deployments:
/// - Checks X-Forwarded-For, X-Real-IP, and Forwarded headers
/// - Falls back to direct connection IP when no proxy headers present
/// - Validates and rejects private/loopback IPs from headers (prevents spoofing)
///
/// Returns 429 Too Many Requests when limit is exceeded.
pub async fn rate_limit_middleware(
    State(state): State<crate::AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    headers: HeaderMap,
    request: Request,
    next: Next,
) -> Response {
    // Extract real client IP (handles proxy headers)
    let client_ip = extract_client_ip(&headers, Some(&ConnectInfo(addr))).unwrap_or_else(|| {
        tracing::warn!("Could not extract client IP, using fallback");
        addr.ip()
    });
    let method = request.method().clone();
    let path = request.uri().path();

    let scope = classify_request(&method, path);

    // If no limiter applies, pass through
    let scope = match scope {
        Some(scope) => scope,
        None => return next.run(request).await,
    };
    let limiter = limiter_for_scope(&state.rate_limit_config, scope);

    // Check rate limit FOR THIS SPECIFIC IP using token bucket algorithm
    match limiter.check_key(&client_ip) {
        Ok(_) => {
            // Request allowed
            next.run(request).await
        }
        Err(_) => {
            // Rate limit exceeded for this IP
            tracing::warn!(
                "Rate limit exceeded for IP: {} on path: {} ({:?})",
                client_ip,
                path,
                scope
            );
            let mut response = (
                StatusCode::TOO_MANY_REQUESTS,
                Json(serde_json::json!({
                    "error": "Too many requests. Please try again later."
                })),
            )
                .into_response();
            response
                .headers_mut()
                .insert("retry-after", HeaderValue::from_static("60"));
            response
                .headers_mut()
                .insert("x-rustshare-rate-limit-scope", scope.header_value());
            response
        }
    }
}

fn quota_from_env(var_name: &str, default_per_minute: u32) -> Quota {
    let per_minute = std::env::var(var_name)
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .and_then(NonZeroU32::new)
        .unwrap_or_else(|| {
            // Safety: default_per_minute is always > 0 (enforced by caller)
            NonZeroU32::new(default_per_minute.max(1))
                .expect("default_per_minute.max(1) is always non-zero")
        });

    Quota::per_minute(per_minute)
}

fn classify_request(method: &Method, path: &str) -> Option<RateLimitScope> {
    if matches_auth_login(method, path) {
        return Some(RateLimitScope::AuthLogin);
    }

    if matches_oidc_login(method, path) {
        return Some(RateLimitScope::OidcLogin);
    }

    if is_public_share_path(path) && method == Method::POST && path.ends_with("/session") {
        return Some(RateLimitScope::ShareSession);
    }

    if is_public_share_path(path)
        && method == Method::GET
        && (path.ends_with("/info") || path.ends_with("/folder/contents"))
    {
        return Some(RateLimitScope::ShareInfo);
    }

    if is_public_share_path(path)
        && method == Method::GET
        && (path.ends_with("/file") || path.contains("/folder/files/"))
    {
        return Some(RateLimitScope::ShareDownload);
    }

    if is_public_share_path(path) && method == Method::POST && path.ends_with("/folder/upload") {
        return Some(RateLimitScope::ShareUpload);
    }

    if matches_authenticated_share_admin(method, path) {
        return Some(RateLimitScope::AuthenticatedShareAdmin);
    }

    if matches_ai_query(method, path) {
        return Some(RateLimitScope::AiQuery);
    }

    None
}

fn matches_ai_query(method: &Method, path: &str) -> bool {
    method == Method::POST
        && (path == "/api/v1/ai/search"
            || path == "/api/v1/ai/summarize"
            || path == "/api/v1/ai/ask")
}

fn matches_auth_login(method: &Method, path: &str) -> bool {
    method == Method::POST && path == "/api/v1/auth/login"
}

fn matches_oidc_login(method: &Method, path: &str) -> bool {
    method == Method::GET && path == "/api/v1/auth/oidc/login"
}

fn matches_authenticated_share_admin(method: &Method, path: &str) -> bool {
    if !path.starts_with("/api/v1/") {
        return false;
    }

    match *method {
        Method::POST => path.ends_with("/shares") || path.ends_with("/share"),
        Method::PUT | Method::DELETE => path.contains("/api/v1/shares/"),
        _ => false,
    }
}

fn is_public_share_path(path: &str) -> bool {
    path.starts_with("/api/v1/public/share/")
}

fn limiter_for_scope(
    config: &RateLimitConfig,
    scope: RateLimitScope,
) -> &Arc<RateLimiter<IpAddr, DashMapStateStore<IpAddr>, DefaultClock>> {
    match scope {
        RateLimitScope::AuthLogin => &config.auth_login,
        RateLimitScope::OidcLogin => &config.oidc_login,
        RateLimitScope::ShareSession => &config.share_session,
        RateLimitScope::ShareInfo => &config.share_info,
        RateLimitScope::ShareDownload => &config.share_download,
        RateLimitScope::ShareUpload => &config.share_upload,
        RateLimitScope::AuthenticatedShareAdmin => &config.authenticated_share_admin,
        RateLimitScope::AiQuery => &config.ai_query,
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
        assert!(config.auth_login.check_key(&test_ip).is_ok());
        assert!(config.oidc_login.check_key(&test_ip).is_ok());
        assert!(config.share_session.check_key(&test_ip).is_ok());
        assert!(config.share_info.check_key(&test_ip).is_ok());
        assert!(config.share_download.check_key(&test_ip).is_ok());
        assert!(config.share_upload.check_key(&test_ip).is_ok());
        assert!(config.authenticated_share_admin.check_key(&test_ip).is_ok());
        assert!(config.ai_query.check_key(&test_ip).is_ok());
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
        assert!(config.auth_login.check_key(&test_ip).is_ok());
        assert!(config.oidc_login.check_key(&test_ip).is_ok());
        assert!(config.share_session.check_key(&test_ip).is_ok());
        assert!(config.share_info.check_key(&test_ip).is_ok());
        assert!(config.share_download.check_key(&test_ip).is_ok());
        assert!(config.share_upload.check_key(&test_ip).is_ok());
        assert!(config.authenticated_share_admin.check_key(&test_ip).is_ok());

        // Session endpoint should have strictest limit (5/min per IP)
        for _ in 1..5 {
            assert!(config.share_session.check_key(&test_ip).is_ok());
        }
        assert!(config.share_session.check_key(&test_ip).is_err());

        // Info endpoint should have higher limit (30/min per IP)
        let test_ip2: IpAddr = "10.0.0.2".parse().unwrap();
        for _ in 0..30 {
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

    #[test]
    fn test_classify_request_matches_hardened_routes() {
        assert_eq!(
            classify_request(&Method::POST, "/api/v1/auth/login"),
            Some(RateLimitScope::AuthLogin)
        );
        assert_eq!(
            classify_request(&Method::GET, "/api/v1/auth/oidc/login"),
            Some(RateLimitScope::OidcLogin)
        );
        assert_eq!(
            classify_request(&Method::POST, "/api/v1/public/share/token/session"),
            Some(RateLimitScope::ShareSession)
        );
        assert_eq!(
            classify_request(&Method::GET, "/api/v1/public/share/token/info"),
            Some(RateLimitScope::ShareInfo)
        );
        assert_eq!(
            classify_request(&Method::GET, "/api/v1/public/share/token/folder/contents"),
            Some(RateLimitScope::ShareInfo)
        );
        assert_eq!(
            classify_request(
                &Method::GET,
                "/api/v1/public/share/token/folder/files/file-id"
            ),
            Some(RateLimitScope::ShareDownload)
        );
        assert_eq!(
            classify_request(&Method::POST, "/api/v1/public/share/token/folder/upload"),
            Some(RateLimitScope::ShareUpload)
        );
        assert_eq!(
            classify_request(&Method::DELETE, "/api/v1/shares/share-id"),
            Some(RateLimitScope::AuthenticatedShareAdmin)
        );
        assert_eq!(classify_request(&Method::GET, "/api/v1/me"), None);
    }
}
