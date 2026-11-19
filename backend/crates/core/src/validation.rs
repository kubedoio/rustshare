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

/// Returns true when every resolved address is internal/private.
pub fn all_addrs_internal(addrs: &[SocketAddr]) -> bool {
    !addrs.is_empty() && addrs.iter().all(|addr| is_internal_ip(&addr.ip()))
}

/// Decide whether certificate verification may be relaxed for a mail server
/// connection. Relaxed verification is only ever allowed when internal mail
/// servers are explicitly enabled and every resolved address is
/// internal/private — public destinations always get full verification.
pub fn should_accept_invalid_certs(
    addrs: &[SocketAddr],
    allow_internal_mail_servers: bool,
    accept_invalid_certs: bool,
) -> bool {
    allow_internal_mail_servers && accept_invalid_certs && all_addrs_internal(addrs)
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
    allow_internal: bool,
) -> Result<Vec<SocketAddr>, String> {
    resolve_socket_addrs_internal(host, port, allow_internal).await
}

/// Resolve socket addresses for a Chat relay URL. Honors the public-only SSRF
/// guard unless `RUSTSHARE_CHAT_ALLOW_LOCAL_RELAY=true` is set (dev/dogfooding
/// deployments that run the relay on the same host). Unlike the mail path, the
/// literal `localhost` host is allowed under the flag so a same-host relay can
/// be mapped through the API.
pub async fn resolve_chat_relay_socket_addrs(
    host: &str,
    port: u16,
    allow_internal: bool,
) -> Result<Vec<SocketAddr>, String> {
    if allow_internal && host.eq_ignore_ascii_case("localhost") {
        return Ok(vec![SocketAddr::new(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            port,
        )]);
    }
    resolve_socket_addrs_internal(host, port, allow_internal).await
}

/// Resolve a configured Chat relay URL. A private destination is allowed only
/// when the complete URL equals the explicitly configured deployment-owned
/// relay URL. The legacy flag remains available for development fixtures, but
/// it cannot turn arbitrary private URLs into trusted Chat relays.
pub async fn resolve_chat_relay_socket_addrs_for_url(
    relay_url: &str,
    allow_internal: bool,
    deployment_relay_url: Option<&str>,
) -> Result<Vec<SocketAddr>, String> {
    let parsed =
        url::Url::parse(relay_url).map_err(|error| format!("invalid relay URL: {error}"))?;
    if !matches!(parsed.scheme(), "ws" | "wss")
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err("Chat relay URL must be a credential-free ws:// or wss:// URL".into());
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| "Chat relay URL must include a host".to_string())?;
    let port = parsed
        .port_or_known_default()
        .ok_or_else(|| "Chat relay URL must include a valid port".to_string())?;
    let deployment_owned = deployment_relay_url
        .and_then(|value| url::Url::parse(value).ok())
        .is_some_and(|value| value == parsed);
    if (deployment_owned || allow_internal) && host.eq_ignore_ascii_case("localhost") {
        return Ok(vec![SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port)]);
    }
    resolve_socket_addrs_internal(host, port, deployment_owned || allow_internal).await
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

/// Escape a user-supplied pattern for use inside a Postgres `LIKE`/`ILIKE`
/// pattern (`%`/`_`/`\` become literals; `\` is the default escape char).
pub fn escape_ilike(input: &str) -> String {
    input
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_ilike_escapes_wildcards() {
        assert_eq!(escape_ilike("50%_off"), "50\\%\\_off");
        assert_eq!(escape_ilike("a\\b"), "a\\\\b");
        assert_eq!(escape_ilike("plain query"), "plain query");
        assert_eq!(escape_ilike("100%"), "100\\%");
        assert_eq!(escape_ilike("a_b"), "a\\_b");
    }

    #[tokio::test]
    async fn resolve_public_socket_addrs_rejects_private_ip_literal() {
        let result = resolve_public_socket_addrs("10.0.0.1", 993).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("internal address"));
    }

    #[tokio::test]
    async fn resolve_mail_server_socket_addrs_allows_private_ip_when_enabled() {
        let result = resolve_mail_server_socket_addrs("10.0.0.1", 993, true).await;
        assert!(result.is_ok());
        let addrs = result.unwrap();
        assert_eq!(addrs.len(), 1);
        assert_eq!(addrs[0].ip().to_string(), "10.0.0.1");
        assert_eq!(addrs[0].port(), 993);
    }

    #[tokio::test]
    async fn resolve_mail_server_socket_addrs_rejects_localhost_even_when_enabled() {
        let result = resolve_mail_server_socket_addrs("localhost", 993, true).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("localhost"));
    }

    #[tokio::test]
    async fn chat_relay_rejects_localhost_by_default() {
        let result = resolve_chat_relay_socket_addrs("localhost", 7447, false).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn chat_relay_allows_localhost_when_flag_set() {
        let result = resolve_chat_relay_socket_addrs("localhost", 7447, true).await;
        assert!(result.is_ok());
        let addrs = result.unwrap();
        assert_eq!(addrs.len(), 1);
        assert!(addrs[0].ip().is_loopback());
        assert_eq!(addrs[0].port(), 7447);
    }

    #[tokio::test]
    async fn chat_relay_allows_private_ip_when_flag_set() {
        let result = resolve_chat_relay_socket_addrs("10.0.0.7", 7447, true).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn configured_chat_relay_allows_only_the_exact_private_url() {
        assert!(resolve_chat_relay_socket_addrs_for_url(
            "ws://localhost:7447",
            false,
            Some("ws://localhost:7447")
        )
        .await
        .is_ok());
        assert!(resolve_chat_relay_socket_addrs_for_url(
            "ws://127.0.0.1:7447",
            false,
            Some("ws://localhost:7447")
        )
        .await
        .is_err());
    }

    #[test]
    fn all_addrs_internal_requires_non_empty_and_all_private() {
        let private: SocketAddr = "10.5.199.84:993".parse().unwrap();
        let loopback: SocketAddr = "127.0.0.1:993".parse().unwrap();
        let public: SocketAddr = "142.250.74.5:993".parse().unwrap();

        assert!(all_addrs_internal(&[private]));
        assert!(all_addrs_internal(&[private, loopback]));
        assert!(!all_addrs_internal(&[public]));
        assert!(!all_addrs_internal(&[private, public]));
        assert!(!all_addrs_internal(&[]));
    }

    #[tokio::test]
    async fn should_accept_invalid_certs_composes_all_gates() {
        let private: SocketAddr = "10.5.199.84:993".parse().unwrap();
        let public: SocketAddr = "142.250.74.5:993".parse().unwrap();

        // Internal servers enabled + default setting: private relaxes, public
        // never does.
        assert!(should_accept_invalid_certs(&[private], true, true));
        assert!(!should_accept_invalid_certs(&[public], true, true));
        assert!(!should_accept_invalid_certs(&[private, public], true, true));

        // Explicit "never" restores strict verification even for private hosts.
        assert!(!should_accept_invalid_certs(&[private], true, false));

        // Without the internal-servers opt-in, nothing relaxes.
        assert!(!should_accept_invalid_certs(&[private], false, true));
    }
}
