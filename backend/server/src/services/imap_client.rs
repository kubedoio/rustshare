use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use async_imap::types::{Fetch, Name};
use chrono::{DateTime, Utc};
use futures_util::TryStreamExt;
use rustshare_core::domain::MailTlsMode;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;

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
        match tls_mode {
            MailTlsMode::Tls => {
                let tcp_stream = TcpStream::connect((host, port)).await?;
                let connector = build_tls_connector()?;
                let server_name = host
                    .to_string()
                    .try_into()
                    .map_err(|e| ImapError::Tls(format!("invalid server name: {e}")))?;
                let tls_stream = connector.connect(server_name, tcp_stream).await?;
                Ok(async_imap::Client::new(ImapStream::Tls(tls_stream)))
            }
            MailTlsMode::None => {
                let tcp_stream = TcpStream::connect((host, port)).await?;
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
        let session = client
            .login(username, password)
            .await
            .map_err(|(err, _client)| ImapError::AuthenticationFailed(err.to_string()))?;
        Ok(Self { session })
    }

    pub async fn list_folders(&mut self) -> Result<Vec<MailFolder>, ImapError> {
        let names = self
            .session
            .list(None, Some("*"))
            .await?
            .try_collect::<Vec<Name>>()
            .await?;
        Ok(names
            .into_iter()
            .map(|name| MailFolder {
                name: name.name().to_string(),
                delimiter: name.delimiter().map(|d| d.to_string()),
            })
            .collect())
    }

    pub async fn select_folder(&mut self, folder: &str) -> Result<(), ImapError> {
        self.session.select(folder).await?;
        Ok(())
    }

    pub async fn fetch_message_summaries(
        &mut self,
        folder: &str,
        limit: usize,
    ) -> Result<Vec<ImapMessageSummary>, ImapError> {
        self.select_folder(folder).await?;

        let uids = self.session.uid_search("ALL").await?;
        let mut limited: Vec<u32> = uids.into_iter().take(limit).collect();
        limited.sort_unstable();
        if limited.is_empty() {
            return Ok(Vec::new());
        }

        let uid_set = limited
            .iter()
            .map(|u| u.to_string())
            .collect::<Vec<_>>()
            .join(",");

        let fetches = self
            .session
            .uid_fetch(uid_set, "(UID ENVELOPE RFC822.SIZE)")
            .await?
            .try_collect::<Vec<Fetch>>()
            .await?;

        Ok(fetches.into_iter().filter_map(summary_from_fetch).collect())
    }

    pub async fn fetch_rfc822(&mut self, folder: &str, uid: u32) -> Result<Vec<u8>, ImapError> {
        self.select_folder(folder).await?;

        let fetches = self
            .session
            .uid_fetch(uid.to_string(), "RFC822")
            .await?
            .try_collect::<Vec<Fetch>>()
            .await?;

        let fetch = fetches
            .into_iter()
            .next()
            .ok_or_else(|| ImapError::CommandFailed(format!("message {uid} not found")))?;

        fetch
            .body()
            .map(|bytes| bytes.to_vec())
            .ok_or_else(|| ImapError::CommandFailed(format!("message {uid} has no body")))
    }

    pub async fn logout(mut self) -> Result<(), ImapError> {
        self.session.logout().await?;
        Ok(())
    }
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

fn parse_imap_text(bytes: impl AsRef<[u8]>) -> Option<String> {
    Some(String::from_utf8_lossy(bytes.as_ref()).to_string())
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

    let subject = envelope.subject.as_ref().and_then(parse_imap_text);

    let (from_address, from_name) = envelope
        .from
        .as_ref()
        .and_then(|addrs| addrs.first())
        .map(|addr| {
            let address = addr
                .mailbox
                .as_ref()
                .and_then(parse_imap_text)
                .map(|m| format_address(&m, addr.host.as_deref()));
            let name = addr.name.as_ref().and_then(parse_imap_text);
            (address, name)
        })
        .unwrap_or((None, None));

    let sent_at = envelope
        .date
        .as_ref()
        .and_then(parse_imap_text)
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

fn format_address(mailbox: &str, host: Option<&[u8]>) -> String {
    match host {
        Some(h) => format!("{}@{}", mailbox, String::from_utf8_lossy(h)),
        None => mailbox.to_string(),
    }
}
