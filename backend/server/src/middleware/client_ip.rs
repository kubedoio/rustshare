use axum::{extract::ConnectInfo, http::HeaderMap};
use std::net::{IpAddr, SocketAddr};

/// Extract real client IP address, considering proxy headers
///
/// Priority order:
/// 1. X-Forwarded-For (leftmost non-private IP)
/// 2. X-Real-IP
/// 3. Forwarded header (RFC 7239)
/// 4. ConnectInfo (direct connection)
///
/// Security: Rejects private/loopback IPs from headers to prevent spoofing
pub fn extract_client_ip(
    headers: &HeaderMap,
    connect_info: Option<&ConnectInfo<SocketAddr>>,
) -> Option<IpAddr> {
    // Check X-Forwarded-For header (most common)
    if let Some(xff) = headers.get("x-forwarded-for") {
        if let Ok(xff_str) = xff.to_str() {
            // XFF format: "client, proxy1, proxy2". The rightmost entry is the
            // one appended by the immediate proxy from $remote_addr (and nginx
            // in this deployment overwrites the header entirely with
            // `$remote_addr`), so a client cannot forge it.
            if let Some(client_ip) = xff_str.rsplit(',').next() {
                if let Ok(ip) = client_ip.trim().parse::<IpAddr>() {
                    // Validate it's not a private IP (prevents spoofing)
                    if !is_private_ip(&ip) {
                        tracing::debug!("Extracted client IP from X-Forwarded-For: {}", ip);
                        return Some(ip);
                    } else {
                        tracing::debug!("Rejected private IP from X-Forwarded-For: {}", ip);
                    }
                }
            }
        }
    }

    // Check X-Real-IP header (nginx, Cloudflare)
    if let Some(real_ip) = headers.get("x-real-ip") {
        if let Ok(ip_str) = real_ip.to_str() {
            if let Ok(ip) = ip_str.parse::<IpAddr>() {
                if !is_private_ip(&ip) {
                    tracing::debug!("Extracted client IP from X-Real-IP: {}", ip);
                    return Some(ip);
                } else {
                    tracing::debug!("Rejected private IP from X-Real-IP: {}", ip);
                }
            }
        }
    }

    // Check standard Forwarded header (RFC 7239)
    if let Some(forwarded) = headers.get("forwarded") {
        if let Ok(fwd_str) = forwarded.to_str() {
            // Format: "for=192.0.2.60;proto=https;by=203.0.113.43"
            if let Some(for_part) = fwd_str.split(';').find(|s| s.trim().starts_with("for=")) {
                let ip_part = for_part.trim_start_matches("for=").trim();
                // Remove quotes. RFC 7239 requires IPv6 literals to be in
                // brackets, so parse the bracketed form before any port.
                let ip_part = ip_part.trim_matches('"');
                let ip_str = if let Some(rest) = ip_part.strip_prefix('[') {
                    // [2001:db8::1] or [2001:db8::1]:8080
                    rest.split(']').next().unwrap_or("")
                } else if ip_part.parse::<IpAddr>().is_ok() {
                    ip_part
                } else {
                    // IPv4 with port: 192.0.2.60:8080
                    ip_part.split(':').next().unwrap_or(ip_part)
                };
                if let Ok(ip) = ip_str.parse::<IpAddr>() {
                    if !is_private_ip(&ip) {
                        tracing::debug!("Extracted client IP from Forwarded header: {}", ip);
                        return Some(ip);
                    } else {
                        tracing::debug!("Rejected private IP from Forwarded header: {}", ip);
                    }
                }
            }
        }
    }

    // Fallback to direct connection IP
    if let Some(ConnectInfo(addr)) = connect_info {
        tracing::debug!("Using direct connection IP: {}", addr.ip());
        return Some(addr.ip());
    }

    tracing::warn!("Could not extract client IP from any source");
    None
}

/// Check if IP is in private/reserved range (prevents header spoofing)
fn is_private_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(ipv4) => {
            // Private ranges: 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16
            // Loopback: 127.0.0.0/8
            ipv4.is_private() || ipv4.is_loopback() || ipv4.is_link_local()
        }
        IpAddr::V6(ipv6) => {
            // Private: fc00::/7, Loopback: ::1
            ipv6.is_loopback() || ipv6.is_unicast_link_local() || is_ipv6_private(ipv6)
        }
    }
}

/// Check if IPv6 address is in unique local address (ULA) range
fn is_ipv6_private(ipv6: &std::net::Ipv6Addr) -> bool {
    let bytes = ipv6.octets();
    // fc00::/7 (unique local addresses)
    (bytes[0] & 0xfe) == 0xfc
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn test_extract_from_x_forwarded_for() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_static("203.0.113.1, 198.51.100.1"),
        );

        // The rightmost entry is the one appended by the immediate trusted
        // proxy; client-supplied entries on the left must not win.
        let ip = extract_client_ip(&headers, None);
        assert_eq!(ip, Some("198.51.100.1".parse().unwrap()));
    }

    #[test]
    fn test_extract_from_x_forwarded_for_single_ip() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", HeaderValue::from_static("203.0.113.42"));

        let ip = extract_client_ip(&headers, None);
        assert_eq!(ip, Some("203.0.113.42".parse().unwrap()));
    }

    #[test]
    fn test_extract_from_x_real_ip() {
        let mut headers = HeaderMap::new();
        headers.insert("x-real-ip", HeaderValue::from_static("203.0.113.42"));

        let ip = extract_client_ip(&headers, None);
        assert_eq!(ip, Some("203.0.113.42".parse().unwrap()));
    }

    #[test]
    fn test_extract_from_forwarded_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "forwarded",
            HeaderValue::from_static("for=203.0.113.60;proto=https;by=203.0.113.43"),
        );

        let ip = extract_client_ip(&headers, None);
        assert_eq!(ip, Some("203.0.113.60".parse().unwrap()));
    }

    #[test]
    fn test_extract_from_forwarded_header_with_quotes() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "forwarded",
            HeaderValue::from_static("for=\"203.0.113.60\""),
        );

        let ip = extract_client_ip(&headers, None);
        assert_eq!(ip, Some("203.0.113.60".parse().unwrap()));
    }

    #[test]
    fn test_reject_private_ip_in_xff() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", HeaderValue::from_static("192.168.1.1"));

        let ip = extract_client_ip(&headers, None);
        assert_eq!(ip, None); // Should reject private IP
    }

    #[test]
    fn test_xff_takes_rightmost_entry() {
        // The rightmost entry is the one appended by the immediate proxy from
        // $remote_addr; client-supplied entries on the left must not win.
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_static("8.8.8.8, 203.0.113.60"),
        );

        let ip = extract_client_ip(&headers, None);
        assert_eq!(ip, Some("203.0.113.60".parse().unwrap()));
    }

    #[test]
    fn test_forwarded_header_ipv6_with_port() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "forwarded",
            HeaderValue::from_static(r#"for="[2001:db8::1]:8080""#),
        );

        let ip = extract_client_ip(&headers, None);
        assert_eq!(ip, Some("2001:db8::1".parse().unwrap()));
    }

    #[test]
    fn test_reject_loopback_in_xff() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", HeaderValue::from_static("127.0.0.1"));

        let ip = extract_client_ip(&headers, None);
        assert_eq!(ip, None); // Should reject loopback
    }

    #[test]
    fn test_reject_private_ip_in_x_real_ip() {
        let mut headers = HeaderMap::new();
        headers.insert("x-real-ip", HeaderValue::from_static("10.0.0.1"));

        let ip = extract_client_ip(&headers, None);
        assert_eq!(ip, None); // Should reject private IP
    }

    #[test]
    fn test_fallback_to_connect_info() {
        let headers = HeaderMap::new();
        let addr: SocketAddr = "203.0.113.50:8080".parse().unwrap();
        let connect_info = ConnectInfo(addr);

        let ip = extract_client_ip(&headers, Some(&connect_info));
        assert_eq!(ip, Some("203.0.113.50".parse().unwrap()));
    }

    #[test]
    fn test_fallback_to_connect_info_allows_private() {
        // Direct connections can be private (e.g., local development)
        let headers = HeaderMap::new();
        let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        let connect_info = ConnectInfo(addr);

        let ip = extract_client_ip(&headers, Some(&connect_info));
        assert_eq!(ip, Some("127.0.0.1".parse().unwrap()));
    }

    #[test]
    fn test_xff_priority_over_x_real_ip() {
        // X-Forwarded-For should take priority over X-Real-IP
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", HeaderValue::from_static("203.0.113.1"));
        headers.insert("x-real-ip", HeaderValue::from_static("203.0.113.2"));

        let ip = extract_client_ip(&headers, None);
        assert_eq!(ip, Some("203.0.113.1".parse().unwrap()));
    }

    #[test]
    fn test_x_real_ip_priority_over_forwarded() {
        // X-Real-IP should take priority over Forwarded
        let mut headers = HeaderMap::new();
        headers.insert("x-real-ip", HeaderValue::from_static("203.0.113.2"));
        headers.insert("forwarded", HeaderValue::from_static("for=203.0.113.3"));

        let ip = extract_client_ip(&headers, None);
        assert_eq!(ip, Some("203.0.113.2".parse().unwrap()));
    }

    #[test]
    fn test_forwarded_priority_over_connect_info() {
        // Forwarded header should take priority over ConnectInfo
        let mut headers = HeaderMap::new();
        headers.insert("forwarded", HeaderValue::from_static("for=203.0.113.3"));
        let addr: SocketAddr = "203.0.113.50:8080".parse().unwrap();
        let connect_info = ConnectInfo(addr);

        let ip = extract_client_ip(&headers, Some(&connect_info));
        assert_eq!(ip, Some("203.0.113.3".parse().unwrap()));
    }

    #[test]
    fn test_is_private_ip_v4() {
        assert!(is_private_ip(&"192.168.1.1".parse().unwrap()));
        assert!(is_private_ip(&"10.0.0.1".parse().unwrap()));
        assert!(is_private_ip(&"172.16.0.1".parse().unwrap()));
        assert!(is_private_ip(&"172.31.255.255".parse().unwrap()));
        assert!(is_private_ip(&"127.0.0.1".parse().unwrap()));
        assert!(is_private_ip(&"169.254.1.1".parse().unwrap())); // Link-local

        assert!(!is_private_ip(&"203.0.113.1".parse().unwrap()));
        assert!(!is_private_ip(&"8.8.8.8".parse().unwrap()));
        assert!(!is_private_ip(&"1.1.1.1".parse().unwrap()));
    }

    #[test]
    fn test_is_private_ip_v6() {
        assert!(is_private_ip(&"::1".parse().unwrap())); // Loopback
        assert!(is_private_ip(&"fe80::1".parse().unwrap())); // Link-local
        assert!(is_private_ip(&"fc00::1".parse().unwrap())); // ULA
        assert!(is_private_ip(&"fd00::1".parse().unwrap())); // ULA

        assert!(!is_private_ip(&"2001:4860:4860::8888".parse().unwrap())); // Google DNS
    }

    #[test]
    fn test_no_headers_no_connect_info() {
        let headers = HeaderMap::new();
        let ip = extract_client_ip(&headers, None);
        assert_eq!(ip, None);
    }

    #[test]
    fn test_malformed_xff_header() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", HeaderValue::from_static("not-an-ip"));

        let ip = extract_client_ip(&headers, None);
        assert_eq!(ip, None);
    }

    #[test]
    fn test_ipv6_in_xff() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_static("2001:4860:4860::8888"),
        );

        let ip = extract_client_ip(&headers, None);
        assert_eq!(ip, Some("2001:4860:4860::8888".parse().unwrap()));
    }

    #[test]
    fn test_xff_with_spaces() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_static("  203.0.113.1  ,  198.51.100.1  "),
        );

        // Rightmost entry wins (see test_extract_from_x_forwarded_for).
        let ip = extract_client_ip(&headers, None);
        assert_eq!(ip, Some("198.51.100.1".parse().unwrap()));
    }
}
