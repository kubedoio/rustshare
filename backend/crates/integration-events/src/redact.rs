//! Redaction of secret-looking substrings in error messages.
//!
//! Used by the outbox store before persisting failure diagnostics (dead
//! letters, last-error columns) so operators can inspect failures without
//! exposing credentials (ADR-0031 / v1alpha1 dead-letter requirements).

/// Replacement marker for redacted substrings.
const REDACTED: &str = "[REDACTED]";

/// Redact secret-looking substrings in `message` and truncate the result to
/// at most `max_len` characters.
///
/// Redacted on sight (case-insensitive keywords):
/// * JWTs — `eyJ`-prefixed three-segment base64url runs;
/// * `Bearer <token>` values;
/// * `Authorization:` header values (rest of the line);
/// * AWS-style access key ids (`AKIA...` / `ASIA...`, 20 chars);
/// * `password=`, `secret=`, `token=`, `api_key=`, `access_key=`,
///   `aws_access_key_id=`, `client_secret=`, `passwd=` value pairs (values
///   run until `&`, `;`, whitespace, quotes or closing brackets).
///
/// Redaction runs before truncation so a secret in the middle of a long
/// message is still removed. Truncation happens on character boundaries.
/// Redaction prefers false positives (over-redacting) over leaking secrets.
pub fn redact_error(message: &str, max_len: usize) -> String {
    if max_len == 0 {
        return String::new();
    }
    let bytes = message.as_bytes();
    let mut out = String::with_capacity(bytes.len().min(max_len));
    let mut out_len = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        if out_len >= max_len {
            break;
        }
        // `match_secret` returns (total bytes the secret covers, bytes of the
        // match to keep verbatim). Keywords such as `Bearer `, `password=`
        // and `Authorization:` are kept for readability; the secret itself is
        // replaced.
        if let Some((consumed, keep)) = match_secret(message, i) {
            let prefix = &message[i..i + keep];
            if out_len + prefix.len() + REDACTED.len() <= max_len {
                out.push_str(prefix);
                out_len += prefix.len();
                out.push_str(REDACTED);
                out_len += REDACTED.len();
            } else {
                break;
            }
            i += consumed;
            continue;
        }
        let ch = message[i..].chars().next().unwrap();
        out.push(ch);
        out_len += 1;
        i += ch.len_utf8();
    }
    out
}

/// If a secret starts at byte offset `i` of `message`, return
/// `(consumed, keep)` — how many bytes the secret covers and how many of
/// those bytes (the leading keyword, e.g. `Bearer ` or `password=`) should
/// be emitted verbatim before the `[REDACTED]` marker.
fn match_secret(message: &str, i: usize) -> Option<(usize, usize)> {
    let rest = &message[i..];
    jwt_len(rest)
        .map(|consumed| (consumed, 0))
        .or_else(|| bearer_len(rest).map(|(consumed, keep)| (consumed, keep)))
        .or_else(|| authorization_len(rest).map(|(consumed, keep)| (consumed, keep)))
        .or_else(|| access_key_len(rest).map(|consumed| (consumed, 0)))
        .or_else(|| key_value_len(rest).map(|(consumed, keep)| (consumed, keep)))
}

fn is_base64url(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_'
}

/// Length of a `eyJ...`-style JWT: `eyJ` + header + `.` + payload + `.` +
/// signature, all base64url, with minimum segment lengths to avoid
/// false-positives on ordinary text such as `keyJSON`.
fn jwt_len(rest: &str) -> Option<usize> {
    let bytes = rest.as_bytes();
    if !bytes.starts_with(b"eyJ") {
        return None;
    }
    let mut pos = 3;
    let mut header_len = 0;
    while pos < bytes.len() && is_base64url(bytes[pos]) {
        pos += 1;
        header_len += 1;
    }
    if header_len < 8 {
        return None;
    }
    let mut total = pos;
    for _ in 0..2 {
        if bytes.get(pos) != Some(&b'.') {
            return None;
        }
        pos += 1;
        let mut segment_len = 0;
        while pos < bytes.len() && is_base64url(bytes[pos]) {
            pos += 1;
            segment_len += 1;
        }
        if segment_len < 4 {
            return None;
        }
        total = pos;
    }
    Some(total)
}

/// Minimum token length for `Bearer <token>` redaction; keeps prose like
/// "found bearer header" untouched while still catching real tokens, which
/// are long (JWTs, opaque OAuth tokens).
const MIN_BEARER_TOKEN_LEN: usize = 8;

/// Length of `Bearer <token>` (the whole value run) plus how many leading
/// bytes to keep (`Bearer `).
fn bearer_len(rest: &str) -> Option<(usize, usize)> {
    const PREFIX: &str = "bearer";
    let bytes = rest.as_bytes();
    if bytes.len() <= PREFIX.len() || !bytes[..PREFIX.len()].eq_ignore_ascii_case(PREFIX.as_bytes())
    {
        return None;
    }
    let after = bytes.get(PREFIX.len())?;
    if !after.is_ascii_whitespace() {
        return None;
    }
    let mut pos = PREFIX.len();
    while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
        pos += 1;
    }
    let value_start = pos;
    while pos < bytes.len() && !bytes[pos].is_ascii_whitespace() {
        pos += 1;
    }
    if pos - value_start < MIN_BEARER_TOKEN_LEN {
        return None; // no token, or a too-short one
    }
    Some((pos, value_start))
}

/// Length of an `Authorization:` header value (the rest of the line) plus how
/// many leading bytes to keep (the header name and any whitespace before the
/// value).
fn authorization_len(rest: &str) -> Option<(usize, usize)> {
    const PREFIX: &str = "authorization:";
    let bytes = rest.as_bytes();
    if bytes.len() < PREFIX.len() || !bytes[..PREFIX.len()].eq_ignore_ascii_case(PREFIX.as_bytes())
    {
        return None;
    }
    let mut value_start = PREFIX.len();
    while value_start < bytes.len() && bytes[value_start].is_ascii_whitespace() {
        value_start += 1;
    }
    let mut pos = value_start;
    while pos < bytes.len() && bytes[pos] != b'\n' {
        pos += 1;
    }
    Some((pos, value_start))
}

/// Length of a 20-character AWS-style access key id (`AKIA...` / `ASIA...`).
fn access_key_len(rest: &str) -> Option<usize> {
    let bytes = rest.as_bytes();
    if !(bytes.starts_with(b"AKIA") || bytes.starts_with(b"ASIA")) {
        return None;
    }
    let mut pos = 4;
    for _ in 0..16 {
        let byte = *bytes.get(pos)?;
        if !byte.is_ascii_uppercase() && !byte.is_ascii_digit() {
            return None;
        }
        pos += 1;
    }
    Some(pos)
}

/// Length of a `keyword=value` pair (`password=`, `secret=`, `token=`,
/// access-key keywords) plus how many leading bytes to keep (the keyword).
/// The value runs until `&`, `;`, whitespace, a quote or a closing bracket.
fn key_value_len(rest: &str) -> Option<(usize, usize)> {
    const KEYWORDS: [&str; 8] = [
        "password=",
        "client_secret=",
        "aws_access_key_id=",
        "access_key=",
        "api_key=",
        "secret=",
        "token=",
        "passwd=",
    ];
    let bytes = rest.as_bytes();
    let keyword = KEYWORDS.iter().find(|keyword| {
        bytes.len() >= keyword.len()
            && bytes[..keyword.len()].eq_ignore_ascii_case(keyword.as_bytes())
    })?;
    let mut pos = keyword.len();
    while pos < bytes.len() {
        let byte = bytes[pos];
        if byte == b'&'
            || byte == b';'
            || byte == b'\n'
            || byte == b'\r'
            || byte == b'"'
            || byte == b'\''
            || byte == b']'
            || byte == b'}'
            || byte.is_ascii_whitespace()
        {
            break;
        }
        pos += 1;
    }
    Some((pos, keyword.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const JWT: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U";

    #[test]
    fn redacts_jwt_runs() {
        let message = format!("rpc failed: {JWT} at call site");
        let out = redact_error(&message, 512);
        assert!(!out.contains("eyJhbGci"), "JWT leaked: {out}");
        assert!(out.contains("[REDACTED]"));
        assert!(out.contains("rpc failed:"));
        assert!(out.contains("at call site"));
    }

    #[test]
    fn redacts_bearer_tokens() {
        let message = "auth failed: Bearer abc123DEF456token".to_string();
        let out = redact_error(&message, 512);
        assert!(!out.contains("abc123DEF456token"), "token leaked: {out}");
        assert!(out.contains("Bearer [REDACTED]"));
    }

    #[test]
    fn redacts_authorization_headers() {
        let message = "Authorization: Basic dXNlcjpwYXNzd29yZA==\nnext line".to_string();
        let out = redact_error(&message, 512);
        assert!(
            !out.contains("dXNlcjpwYXNzd29yZA=="),
            "header leaked: {out}"
        );
        assert!(out.contains("Authorization: [REDACTED]"));
        assert!(out.contains("next line"));
    }

    #[test]
    fn redacts_key_value_pairs() {
        let out = redact_error("connection failed: password=sup3rSecret&user=bob", 512);
        assert!(!out.contains("sup3rSecret"), "password leaked: {out}");
        assert!(out.contains("password=[REDACTED]"));
        assert!(out.contains("user=bob"));

        let out = redact_error("failed with secret=abc123 token=xyz", 512);
        assert!(!out.contains("abc123"));
        assert!(!out.contains("xyz"));
        assert!(out.contains("secret=[REDACTED]"));
        assert!(out.contains("token=[REDACTED]"));

        let out = redact_error("ConnectionString=Server=x;Password=S3cret!;User=y", 512);
        assert!(!out.contains("S3cret!"), "password leaked: {out}");
        assert!(out.contains("Password=[REDACTED]"));

        let out = redact_error(
            "q?aws_access_key_id=AKIAIOSFODNN7EXAMPLE&region=us-east-1",
            512,
        );
        assert!(!out.contains("AKIAIOSFODNN7EXAMPLE"), "key leaked: {out}");
        assert!(out.contains("aws_access_key_id=[REDACTED]"));
        assert!(out.contains("region=us-east-1"));
    }

    #[test]
    fn redacts_bare_access_keys() {
        let out = redact_error("key AKIAIOSFODNN7EXAMPLE expired", 512);
        assert!(!out.contains("AKIAIOSFODNN7EXAMPLE"), "key leaked: {out}");
        assert!(out.contains("[REDACTED] expired"));
    }

    #[test]
    fn passes_through_innocuous_messages() {
        let message = "disk full: 42 bytes remaining on volume /data";
        assert_eq!(redact_error(message, 512), message);
    }

    #[test]
    fn truncates_to_max_len() {
        let message = "x".repeat(10_000);
        let out = redact_error(&message, 100);
        assert_eq!(out.chars().count(), 100);
        assert_eq!(out, "x".repeat(100));

        assert_eq!(redact_error(&message, 0), "");
    }

    #[test]
    fn truncation_respects_character_boundaries() {
        let message = "всё хорошо работает".repeat(1000);
        let out = redact_error(&message, 7);
        assert_eq!(out.chars().count(), 7);
    }

    #[test]
    fn redacts_then_truncates() {
        let message = format!("start {JWT} {}", "y".repeat(10_000));
        let out = redact_error(&message, 60);
        assert!(!out.contains("eyJhbGci"), "JWT leaked: {out}");
        assert!(out.contains("[REDACTED]"));
        assert!(out.chars().count() <= 60);
    }

    #[test]
    fn short_bearer_without_token_is_left_alone() {
        let out = redact_error("found bearer header", 512);
        assert_eq!(out, "found bearer header");
    }
}
