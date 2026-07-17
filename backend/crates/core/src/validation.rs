//! Shared validation and hashing utilities.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use bytes::Bytes;
use sha2::{Digest, Sha256};

/// Returns true if the IPv4 address is unspecified, loopback, private, link-local,
/// multicast, or part of the CGNAT range (100.64.0.0/10).
fn is_internal_ipv4(v4: &Ipv4Addr) -> bool {
    let octets = v4.octets();
    // CGNAT 100.64.0.0/10
    if octets[0] == 100 && (64..=127).contains(&octets[1]) {
        return true;
    }
    v4.is_unspecified()
        || v4.is_loopback()
        || v4.is_private()
        || v4.is_link_local()
        || v4.is_multicast()
}

/// Returns true if the IP address is unspecified, loopback, private, link-local,
/// multicast, unique-local, or an IPv4-mapped/compatible IPv6 address that
/// resolves to an internal IPv4 address.
pub fn is_internal_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_internal_ipv4(v4),
        IpAddr::V6(v6) => {
            if let Some(mapped_v4) = v6.to_ipv4() {
                return is_internal_ipv4(&mapped_v4);
            }
            v6.is_unspecified()
                || v6.is_loopback()
                || v6.is_unicast_link_local()
                || v6.is_multicast()
                || v6.is_unique_local()
        }
    }
}

/// Returns true when the operator has explicitly opted into allowing Mail
/// (IMAP/SMTP) connections to internal/private destinations. Off by default to
/// mitigate SSRF; self-hosted or air-gapped deployments may need it.
pub fn allow_internal_mail_servers() -> bool {
    std::env::var("RUSTSHARE_ALLOW_INTERNAL_MAIL_SERVERS").as_deref() == Ok("true")
}

async fn resolve_socket_addrs_internal(
    host: &str,
    port: u16,
    allow_internal: bool,
) -> Result<Vec<SocketAddr>, String> {
    if host.eq_ignore_ascii_case("localhost") {
        return Err("localhost is not allowed".to_string());
    }

    // For public-only mode, fast-reject internal IP literals before DNS.
    if !allow_internal {
        if let Ok(ip) = host.parse::<IpAddr>() {
            if is_internal_ip(&ip) {
                return Err(format!("{ip} is an internal address"));
            }
            return Ok(vec![SocketAddr::new(ip, port)]);
        }
    }

    let lookup = tokio::time::timeout(
        Duration::from_secs(5),
        tokio::net::lookup_host((host, port)),
    )
    .await
    .map_err(|_| "DNS lookup timed out".to_string())?
    .map_err(|e| format!("DNS lookup failed: {e}"))?;

    let addrs: Vec<SocketAddr> = lookup.collect();
    if addrs.is_empty() {
        return Err("DNS lookup returned no addresses".to_string());
    }

    if !allow_internal {
        for addr in &addrs {
            if is_internal_ip(&addr.ip()) {
                return Err(format!("{} resolves to an internal address", addr.ip()));
            }
        }
    }

    Ok(addrs)
}

/// Resolve a hostname to public socket addresses and reject internal/private
/// destinations to mitigate SSRF.
///
/// Rejects literal `localhost`, IP literals that are internal, and hostnames
/// whose DNS resolution returns only internal addresses. DNS lookup is capped at
/// 5 seconds.
pub async fn resolve_public_socket_addrs(host: &str, port: u16) -> Result<Vec<SocketAddr>, String> {
    resolve_socket_addrs_internal(host, port, false).await
}

/// Resolve socket addresses for an IMAP/SMTP host. Honors the public-only SSRF
/// guard unless `RUSTSHARE_ALLOW_INTERNAL_MAIL_SERVERS=true` is set.
pub async fn resolve_mail_server_socket_addrs(
    host: &str,
    port: u16,
) -> Result<Vec<SocketAddr>, String> {
    resolve_socket_addrs_internal(host, port, allow_internal_mail_servers()).await
}

/// Validate a file or folder name.
/// Returns Ok(()) if valid, or Err with a descriptive message.
pub fn validate_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Name cannot be empty".to_string());
    }
    if name.contains('/') {
        return Err("Name cannot contain forward slash (/)".to_string());
    }
    if name.contains('\0') {
        return Err("Name cannot contain null character".to_string());
    }
    Ok(())
}

/// Compute SHA-256 hash of byte content.
pub fn calculate_sha256(content: &Bytes) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content);
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Serializes tests that mutate the process-global
    /// `RUSTSHARE_ALLOW_INTERNAL_MAIL_SERVERS` variable; Cargo runs unit tests
    /// in parallel threads, so unsynchronized set/remove races make them flaky.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        ENV_LOCK.lock().unwrap_or_else(|err| err.into_inner())
    }

    #[test]
    fn allow_internal_mail_servers_defaults_to_false() {
        let _guard = env_lock();
        // The variable is not set in unit-test runs, so the guard must be off.
        std::env::remove_var("RUSTSHARE_ALLOW_INTERNAL_MAIL_SERVERS");
        assert!(!allow_internal_mail_servers());
    }

    #[test]
    fn allow_internal_mail_servers_activates_with_true() {
        let _guard = env_lock();
        std::env::set_var("RUSTSHARE_ALLOW_INTERNAL_MAIL_SERVERS", "true");
        assert!(allow_internal_mail_servers());
        std::env::remove_var("RUSTSHARE_ALLOW_INTERNAL_MAIL_SERVERS");
    }

    #[tokio::test]
    async fn resolve_public_socket_addrs_rejects_private_ip_literal() {
        let result = resolve_public_socket_addrs("10.0.0.1", 993).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("internal address"));
    }

    #[tokio::test]
    async fn resolve_mail_server_socket_addrs_allows_private_ip_when_enabled() {
        let _guard = env_lock();
        std::env::set_var("RUSTSHARE_ALLOW_INTERNAL_MAIL_SERVERS", "true");
        let result = resolve_mail_server_socket_addrs("10.0.0.1", 993).await;
        std::env::remove_var("RUSTSHARE_ALLOW_INTERNAL_MAIL_SERVERS");
        assert!(result.is_ok());
        let addrs = result.unwrap();
        assert_eq!(addrs.len(), 1);
        assert_eq!(addrs[0].ip().to_string(), "10.0.0.1");
        assert_eq!(addrs[0].port(), 993);
    }

    #[tokio::test]
    async fn resolve_mail_server_socket_addrs_rejects_localhost_even_when_enabled() {
        let _guard = env_lock();
        std::env::set_var("RUSTSHARE_ALLOW_INTERNAL_MAIL_SERVERS", "true");
        let result = resolve_mail_server_socket_addrs("localhost", 993).await;
        std::env::remove_var("RUSTSHARE_ALLOW_INTERNAL_MAIL_SERVERS");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("localhost"));
    }
}
