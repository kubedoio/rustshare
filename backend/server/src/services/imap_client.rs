use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use async_imap::types::{Fetch, Name};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures_util::TryStreamExt;
use mailparse::{addrparse_header, parse_header, MailAddr};
use rustshare_core::domain::MailTlsMode;
use rustshare_core::validation::resolve_public_socket_addrs;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_MAIL_BODY_SIZE_BYTES: usize = 25 * 1024 * 1024;

/// Underlying transport for an IMAP session.
///
/// Supports both TLS-wrapped and plain TCP streams so that the public API can
/// expose a single, non-generic [`ImapSession`] regardless of the configured
/// [`MailTlsMode`].
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum ImapStream {
    Tls(tokio_rustls::client::TlsStream<TcpStream>),
    Plain(TcpStream),
}

impl AsyncRead for ImapStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            ImapStream::Tls(s) => Pin::new(s).poll_read(cx, buf),
            ImapStream::Plain(s) => Pin::new(s).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for ImapStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        match self.get_mut() {
            ImapStream::Tls(s) => Pin::new(s).poll_write(cx, buf),
            ImapStream::Plain(s) => Pin::new(s).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            ImapStream::Tls(s) => Pin::new(s).poll_flush(cx),
            ImapStream::Plain(s) => Pin::new(s).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            ImapStream::Tls(s) => Pin::new(s).poll_shutdown(cx),
            ImapStream::Plain(s) => Pin::new(s).poll_shutdown(cx),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ImapError {
    #[error("IMAP connection failed: {0}")]
    ConnectionFailed(String),
    #[error("IMAP TLS error: {0}")]
    Tls(String),
    #[error("IMAP authentication failed: {0}")]
    AuthenticationFailed(String),
    #[error("IMAP command failed: {0}")]
    CommandFailed(String),
    #[error("IMAP message {uid} size {size} bytes exceeds maximum allowed {max} bytes")]
    MessageTooLarge { uid: u32, size: usize, max: usize },
}

impl From<std::io::Error> for ImapError {
    fn from(err: std::io::Error) -> Self {
        ImapError::ConnectionFailed(err.to_string())
    }
}

impl From<async_imap::error::Error> for ImapError {
    fn from(err: async_imap::error::Error) -> Self {
        ImapError::CommandFailed(err.to_string())
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct MailFolder {
    pub name: String,
    pub delimiter: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ImapMessageSummary {
    pub uid: u32,
    pub subject: Option<String>,
    pub from_address: Option<String>,
    pub from_name: Option<String>,
    pub sent_at: Option<DateTime<Utc>>,
    pub size_bytes: i64,
}

pub struct ImapClient;

pub struct ImapSession {
    session: async_imap::Session<ImapStream>,
}

impl ImapClient {
    pub async fn connect(
        host: &str,
        port: u16,
        tls_mode: MailTlsMode,
    ) -> Result<async_imap::Client<ImapStream>, ImapError> {
        // Reject internal/private destinations and pin the resolved addresses so
        // a second DNS lookup cannot rebind to an internal address.
        let addrs = resolve_public_socket_addrs(host, port).await.map_err(|e| {
            ImapError::ConnectionFailed(format!("IMAP host failed SSRF validation: {e}"))
        })?;
        if addrs.is_empty() {
            return Err(ImapError::ConnectionFailed(
                "no addresses for IMAP host".to_string(),
            ));
        }

        let mut last_error = None;
        for addr in addrs {
            match Self::connect_addr(host, port, addr, tls_mode).await {
                Ok(client) => return Ok(client),
                Err(e) => {
                    tracing::warn!(
                        host = %host,
                        port = %port,
                        addr = %addr,
                        error = %e,
                        "IMAP connection attempt failed"
                    );
                    last_error = Some(e);
                }
            }
        }
        Err(last_error.unwrap_or_else(|| {
            ImapError::ConnectionFailed("no addresses for IMAP host".to_string())
        }))
    }

    async fn connect_addr(
        host: &str,
        _port: u16,
        addr: std::net::SocketAddr,
        tls_mode: MailTlsMode,
    ) -> Result<async_imap::Client<ImapStream>, ImapError> {
        match tls_mode {
            MailTlsMode::Tls => {
                let tcp_stream = tokio::time::timeout(DEFAULT_TIMEOUT, TcpStream::connect(addr))
                    .await
                    .map_err(|_| {
                        ImapError::ConnectionFailed("operation timed out".to_string())
                    })??;

                let connector = build_tls_connector()?;
                // Keep the original hostname for TLS certificate verification.
                let server_name = host
                    .to_string()
                    .try_into()
                    .map_err(|e| ImapError::Tls(format!("invalid server name: {e}")))?;
                let tls_stream = tokio::time::timeout(
                    DEFAULT_TIMEOUT,
                    connector.connect(server_name, tcp_stream),
                )
                .await
                .map_err(|_| ImapError::ConnectionFailed("operation timed out".to_string()))??;
                Ok(async_imap::Client::new(ImapStream::Tls(tls_stream)))
            }
            MailTlsMode::None => {
                tracing::warn!(
                    "IMAP connection to {} will transmit credentials in plaintext",
                    addr
                );
                let tcp_stream = tokio::time::timeout(DEFAULT_TIMEOUT, TcpStream::connect(addr))
                    .await
                    .map_err(|_| {
                        ImapError::ConnectionFailed("operation timed out".to_string())
                    })??;
                Ok(async_imap::Client::new(ImapStream::Plain(tcp_stream)))
            }
            MailTlsMode::StartTls => Err(ImapError::Tls(
                "STARTTLS not supported in this phase".to_string(),
            )),
        }
    }
}

impl ImapSession {
    pub async fn login(
        client: async_imap::Client<ImapStream>,
        username: &str,
        password: &str,
    ) -> Result<Self, ImapError> {
        let session = tokio::time::timeout(DEFAULT_TIMEOUT, client.login(username, password))
            .await
            .map_err(|_| ImapError::ConnectionFailed("operation timed out".to_string()))?
            .map_err(|(err, _client)| ImapError::AuthenticationFailed(err.to_string()))?;
        Ok(Self { session })
    }

    pub async fn list_folders(&mut self) -> Result<Vec<MailFolder>, ImapError> {
        let names = tokio::time::timeout(DEFAULT_TIMEOUT, async {
            self.session
                .list(None, Some("*"))
                .await?
                .try_collect::<Vec<Name>>()
                .await
        })
        .await
        .map_err(|_| ImapError::CommandFailed("IMAP command timed out".to_string()))??;

        Ok(names
            .into_iter()
            .map(|name| MailFolder {
                name: name.name().to_string(),
                delimiter: name.delimiter().map(|d| d.to_string()),
            })
            .collect())
    }

    pub async fn select_folder(&mut self, folder: &str) -> Result<Option<u32>, ImapError> {
        let mailbox = tokio::time::timeout(DEFAULT_TIMEOUT, self.session.select(folder))
            .await
            .map_err(|_| ImapError::CommandFailed("IMAP command timed out".to_string()))??;
        Ok(mailbox.uid_validity)
    }

    pub async fn fetch_message_summaries(
        &mut self,
        folder: &str,
        limit: usize,
    ) -> Result<(Option<u32>, Vec<ImapMessageSummary>), ImapError> {
        let uidvalidity = self.select_folder(folder).await?;

        let uids = tokio::time::timeout(DEFAULT_TIMEOUT, self.session.uid_search("ALL"))
            .await
            .map_err(|_| ImapError::CommandFailed("IMAP command timed out".to_string()))??;
        // Sort newest-first (highest UID) so the limit returns a stable,
        // deterministic slice instead of an arbitrary HashSet subset.
        let mut all_uids: Vec<u32> = uids.into_iter().collect();
        all_uids.sort_unstable_by(|a, b| b.cmp(a));
        let limited: Vec<u32> = all_uids.into_iter().take(limit).collect();
        if limited.is_empty() {
            return Ok((uidvalidity, Vec::new()));
        }

        let uid_set = limited
            .iter()
            .map(|u| u.to_string())
            .collect::<Vec<_>>()
            .join(",");

        let fetches = tokio::time::timeout(DEFAULT_TIMEOUT, async {
            self.session
                .uid_fetch(uid_set, "(UID ENVELOPE RFC822.SIZE)")
                .await?
                .try_collect::<Vec<Fetch>>()
                .await
        })
        .await
        .map_err(|_| ImapError::CommandFailed("IMAP command timed out".to_string()))??;

        Ok((
            uidvalidity,
            fetches.into_iter().filter_map(summary_from_fetch).collect(),
        ))
    }

    pub async fn fetch_rfc822(
        &mut self,
        folder: &str,
        uid: u32,
        expected_uidvalidity: Option<i64>,
    ) -> Result<Vec<u8>, ImapError> {
        let uidvalidity = self.select_folder(folder).await?;
        if uidvalidity.map(i64::from) != expected_uidvalidity {
            return Err(ImapError::CommandFailed(format!(
                "UIDVALIDITY changed from {:?} to {:?}; selected UID {} is stale",
                expected_uidvalidity,
                uidvalidity.map(i64::from),
                uid
            )));
        }

        // Fetch the advertised size first so we reject oversized messages
        // before transferring the full body across the wire.
        let size_fetches = tokio::time::timeout(DEFAULT_TIMEOUT, async {
            self.session
                .uid_fetch(uid.to_string(), "RFC822.SIZE")
                .await?
                .try_collect::<Vec<Fetch>>()
                .await
        })
        .await
        .map_err(|_| ImapError::CommandFailed("IMAP command timed out".to_string()))??;

        let size_fetch = size_fetches
            .into_iter()
            .next()
            .ok_or_else(|| ImapError::CommandFailed(format!("message {uid} not found")))?;
        let size = size_fetch.size.unwrap_or(0) as usize;
        if size > MAX_MAIL_BODY_SIZE_BYTES {
            return Err(ImapError::MessageTooLarge {
                uid,
                size,
                max: MAX_MAIL_BODY_SIZE_BYTES,
            });
        }

        // Use BODY.PEEK[] so the import does not set the remote \Seen flag.
        let fetches = tokio::time::timeout(DEFAULT_TIMEOUT, async {
            self.session
                .uid_fetch(uid.to_string(), "BODY.PEEK[]")
                .await?
                .try_collect::<Vec<Fetch>>()
                .await
        })
        .await
        .map_err(|_| ImapError::CommandFailed("IMAP command timed out".to_string()))??;

        let fetch = fetches
            .into_iter()
            .next()
            .ok_or_else(|| ImapError::CommandFailed(format!("message {uid} not found")))?;

        let body = fetch
            .body()
            .ok_or_else(|| ImapError::CommandFailed(format!("message {uid} has no body")))?;
        if body.len() > MAX_MAIL_BODY_SIZE_BYTES {
            return Err(ImapError::MessageTooLarge {
                uid,
                size: body.len(),
                max: MAX_MAIL_BODY_SIZE_BYTES,
            });
        }
        Ok(body.to_vec())
    }

    pub async fn fetch_uids_by_date_range(
        &mut self,
        folder: &str,
        since: Option<DateTime<Utc>>,
        before: Option<DateTime<Utc>>,
    ) -> Result<(Option<u32>, Vec<u32>), ImapError> {
        let uid_validity = self
            .select_folder(folder)
            .await?
            .ok_or_else(|| ImapError::CommandFailed("Missing UIDVALIDITY".to_string()))?;

        if let (Some(since), Some(before)) = (since, before) {
            if since >= before {
                return Err(ImapError::CommandFailed(
                    "archive_since must be before archive_before".to_string(),
                ));
            }
        }

        let query = build_archive_search_query(since, before);

        let uids = tokio::time::timeout(DEFAULT_TIMEOUT, self.session.uid_search(query))
            .await
            .map_err(|_| ImapError::CommandFailed("IMAP command timed out".to_string()))?
            .map_err(|e| ImapError::CommandFailed(format!("UID SEARCH failed: {e}")))?;

        let mut uids: Vec<u32> = uids.into_iter().collect();
        uids.sort_unstable();
        Ok((Some(uid_validity), uids))
    }

    pub async fn logout(mut self) -> Result<(), ImapError> {
        tokio::time::timeout(DEFAULT_TIMEOUT, self.session.logout())
            .await
            .map_err(|_| ImapError::CommandFailed("IMAP command timed out".to_string()))??;
        Ok(())
    }
}

/// Trait abstracting the IMAP operations required by archive jobs.
///
/// This allows archive job processing to be tested without a real IMAP server.
#[async_trait]
pub trait ImapArchiveSession: Send {
    async fn fetch_uids_by_date_range(
        &mut self,
        folder: &str,
        since: Option<DateTime<Utc>>,
        before: Option<DateTime<Utc>>,
    ) -> Result<(Option<u32>, Vec<u32>), ImapError>;

    async fn fetch_rfc822(
        &mut self,
        folder: &str,
        uid: u32,
        expected_uidvalidity: Option<i64>,
    ) -> Result<Vec<u8>, ImapError>;
}

#[async_trait]
impl ImapArchiveSession for ImapSession {
    async fn fetch_uids_by_date_range(
        &mut self,
        folder: &str,
        since: Option<DateTime<Utc>>,
        before: Option<DateTime<Utc>>,
    ) -> Result<(Option<u32>, Vec<u32>), ImapError> {
        ImapSession::fetch_uids_by_date_range(self, folder, since, before).await
    }

    async fn fetch_rfc822(
        &mut self,
        folder: &str,
        uid: u32,
        expected_uidvalidity: Option<i64>,
    ) -> Result<Vec<u8>, ImapError> {
        ImapSession::fetch_rfc822(self, folder, uid, expected_uidvalidity).await
    }
}

fn build_archive_search_query(
    since: Option<DateTime<Utc>>,
    before: Option<DateTime<Utc>>,
) -> String {
    let mut criteria = vec!["ALL".to_string()];
    if let Some(since) = since {
        criteria.push(format!("SINCE {}", since.format("%d-%b-%Y")));
    }
    if let Some(before) = before {
        criteria.push(format!("BEFORE {}", before.format("%d-%b-%Y")));
    }
    criteria.join(" ")
}

fn build_tls_connector() -> Result<tokio_rustls::TlsConnector, ImapError> {
    let root_store = tokio_rustls::rustls::RootCertStore::from_iter(
        webpki_roots::TLS_SERVER_ROOTS.iter().cloned(),
    );
    let config = tokio_rustls::rustls::ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();
    Ok(tokio_rustls::TlsConnector::from(Arc::new(config)))
}

fn decode_header_text(key: &str, raw: &[u8]) -> Option<String> {
    let raw_str = String::from_utf8_lossy(raw);
    let header_line = format!("{}: {}\r\n", key, raw_str);
    let bytes = header_line.as_bytes();
    let (header, _idx) = parse_header(bytes).ok()?;
    Some(header.get_value())
}

fn parse_address(raw: &[u8]) -> Option<(Option<String>, String)> {
    let raw_str = String::from_utf8_lossy(raw);
    let header_line = format!("From: {}\r\n", raw_str);
    let bytes = header_line.as_bytes();
    let (header, _idx) = parse_header(bytes).ok()?;
    let addrs = addrparse_header(&header).ok()?;
    addrs.iter().next().and_then(mail_addr_to_address)
}

fn mail_addr_to_address(addr: &MailAddr) -> Option<(Option<String>, String)> {
    match addr {
        MailAddr::Single(info) => Some((info.display_name.clone(), info.addr.clone())),
        MailAddr::Group(group) => group
            .addrs
            .first()
            .map(|info| (info.display_name.clone(), info.addr.clone())),
    }
}

fn parse_imap_date(value: &str) -> Option<DateTime<Utc>> {
    let trimmed = value.trim();
    DateTime::parse_from_rfc2822(trimmed)
        .ok()
        .or_else(|| DateTime::parse_from_str(trimmed, "%d %b %Y %H:%M:%S %z").ok())
        .map(|dt| dt.with_timezone(&Utc))
}

fn summary_from_fetch(fetch: Fetch) -> Option<ImapMessageSummary> {
    let uid = fetch.uid?;
    let envelope = fetch.envelope()?;

    let subject = envelope
        .subject
        .as_ref()
        .and_then(|b| decode_header_text("Subject", b));

    let (from_name, from_address) = envelope
        .from
        .as_ref()
        .and_then(|addrs| addrs.first())
        .and_then(|addr| {
            parse_address(&address_to_bytes(addr)).map(|(name, address)| (name, Some(address)))
        })
        .unwrap_or((None, None));

    let sent_at = envelope
        .date
        .as_ref()
        .and_then(|b| decode_header_text("Date", b))
        .and_then(|d| parse_imap_date(&d));

    let size_bytes = i64::from(fetch.size.unwrap_or(0));

    Some(ImapMessageSummary {
        uid,
        subject,
        from_address,
        from_name,
        sent_at,
        size_bytes,
    })
}

fn address_to_bytes(addr: &async_imap::imap_proto::types::Address<'_>) -> Vec<u8> {
    let name = addr.name.as_deref().map(String::from_utf8_lossy);
    let mailbox = addr
        .mailbox
        .as_deref()
        .map(String::from_utf8_lossy)
        .unwrap_or_default();
    let host = addr
        .host
        .as_deref()
        .map(String::from_utf8_lossy)
        .unwrap_or_default();

    if mailbox.is_empty() {
        return Vec::new();
    }

    let email = if host.is_empty() {
        mailbox.to_string()
    } else {
        format!("{}@{}", mailbox, host)
    };

    match name {
        Some(n) if !n.is_empty() => format!("{} <{}>", n, email).into_bytes(),
        _ => format!("<{}>", email).into_bytes(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn decode_plain_ascii_subject() {
        let raw = b"Hello, world!";
        assert_eq!(
            decode_header_text("Subject", raw),
            Some("Hello, world!".to_string())
        );
    }

    #[test]
    fn decode_rfc2047_subject() {
        // Base64-encoded UTF-8 for "Привет" (Russian for "Hi").
        let raw = b"=?UTF-8?B?0J/RgNC40LLQtdGC?=";
        assert_eq!(
            decode_header_text("Subject", raw),
            Some("Привет".to_string())
        );
    }

    #[test]
    fn parse_address_with_display_name() {
        let raw = b"\"John Doe\" <john@example.com>";
        let (name, address) = parse_address(raw).unwrap();
        assert_eq!(name, Some("John Doe".to_string()));
        assert_eq!(address, "john@example.com");
    }

    #[test]
    fn parse_address_with_encoded_display_name() {
        // Base64-encoded UTF-8 for "Привет".
        let raw = b"=?UTF-8?B?0J/RgNC40LLQtdGC?= <test@example.com>";
        let (name, address) = parse_address(raw).unwrap();
        assert_eq!(name, Some("Привет".to_string()));
        assert_eq!(address, "test@example.com");
    }

    #[test]
    fn parse_address_without_display_name() {
        let raw = b"jane@example.com";
        let (name, address) = parse_address(raw).unwrap();
        assert_eq!(name, None);
        assert_eq!(address, "jane@example.com");
    }

    #[test]
    fn parse_rfc2822_date() {
        let parsed = parse_imap_date("Mon, 15 Aug 2022 10:30:00 +0000").unwrap();
        assert_eq!(parsed.timestamp(), 1_660_559_400);
    }

    #[test]
    fn parse_fallback_date_format() {
        let parsed = parse_imap_date("15 Aug 2022 10:30:00 +0000").unwrap();
        assert_eq!(parsed.timestamp(), 1_660_559_400);
    }

    #[test]
    fn build_archive_search_query_open_range() {
        let query = build_archive_search_query(None, None);
        assert_eq!(query, "ALL");
    }

    #[test]
    fn build_archive_search_query_since_only() {
        let since = Utc.with_ymd_and_hms(2024, 1, 15, 0, 0, 0).unwrap();
        let query = build_archive_search_query(Some(since), None);
        assert_eq!(query, "ALL SINCE 15-Jan-2024");
    }

    #[test]
    fn build_archive_search_query_before_only() {
        let before = Utc.with_ymd_and_hms(2024, 6, 30, 23, 59, 59).unwrap();
        let query = build_archive_search_query(None, Some(before));
        assert_eq!(query, "ALL BEFORE 30-Jun-2024");
    }

    #[test]
    fn build_archive_search_query_both_bounds() {
        let since = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let before = Utc.with_ymd_and_hms(2024, 12, 31, 0, 0, 0).unwrap();
        let query = build_archive_search_query(Some(since), Some(before));
        assert_eq!(query, "ALL SINCE 01-Jan-2024 BEFORE 31-Dec-2024");
    }
}
