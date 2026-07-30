use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use async_imap::types::{Fetch, Name, NameAttribute};
use async_trait::async_trait;
use base64::Engine;
use chrono::{DateTime, NaiveDate, Utc};
use futures_util::TryStreamExt;
use mailparse::{addrparse_header, MailAddr, MailHeaderMap};
use rustshare_core::domain::{MailSortOrder, MailTlsMode};
use rustshare_core::validation::{resolve_mail_server_socket_addrs, should_accept_invalid_certs};
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
    #[error("IMAP message {0} not found")]
    MessageNotFound(u32),
    #[error("UIDVALIDITY changed from {expected:?} to {actual:?}; selected UID {uid} is stale")]
    UidValidityMismatch {
        uid: u32,
        expected: Option<i64>,
        actual: Option<i64>,
    },
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
    pub display_name: String,
    pub delimiter: Option<String>,
    pub role: Option<String>,
    /// Unread/total message counts from IMAP STATUS. `None` when the server
    /// refused STATUS for this folder (e.g. `\Noselect` parents).
    pub unseen: Option<u32>,
    pub total: Option<u32>,
}

/// Unread/total message counts for a single folder, from IMAP STATUS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FolderStatus {
    pub unseen: Option<u32>,
    pub total: Option<u32>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ImapMessageSummary {
    pub uid: u32,
    pub subject: Option<String>,
    pub from_address: Option<String>,
    pub from_name: Option<String>,
    pub sent_at: Option<DateTime<Utc>>,
    pub size_bytes: i64,
    pub is_seen: bool,
    pub is_flagged: bool,
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
        // Reject internal/private destinations unless the operator explicitly
        // opted in (e.g. self-hosted or air-gapped mail servers). Pin the
        // resolved addresses so a second DNS lookup cannot rebind to an
        // internal address.
        let addrs = resolve_mail_server_socket_addrs(host, port)
            .await
            .map_err(|e| {
                ImapError::ConnectionFailed(format!("IMAP host failed SSRF validation: {e}"))
            })?;
        if addrs.is_empty() {
            return Err(ImapError::ConnectionFailed(
                "no addresses for IMAP host".to_string(),
            ));
        }
        // Relaxed certificate verification is only ever enabled for
        // internal/private destinations (e.g. self-signed intranet servers
        // reached by IP); public hosts always get full verification.
        let accept_invalid_certs = should_accept_invalid_certs(&addrs);
        if accept_invalid_certs {
            tracing::warn!(
                host = %host,
                port = %port,
                "accepting invalid TLS certificates for internal mail server connection"
            );
        }

        let mut last_error = None;
        for addr in addrs {
            match Self::connect_addr(host, port, addr, tls_mode, accept_invalid_certs).await {
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
        accept_invalid_certs: bool,
    ) -> Result<async_imap::Client<ImapStream>, ImapError> {
        match tls_mode {
            MailTlsMode::Tls => {
                let tcp_stream = tokio::time::timeout(DEFAULT_TIMEOUT, TcpStream::connect(addr))
                    .await
                    .map_err(|_| {
                        ImapError::ConnectionFailed("operation timed out".to_string())
                    })??;

                let connector = build_tls_connector(accept_invalid_certs)?;
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
            MailTlsMode::StartTls => {
                let tcp_stream = tokio::time::timeout(DEFAULT_TIMEOUT, TcpStream::connect(addr))
                    .await
                    .map_err(|_| {
                        ImapError::ConnectionFailed("operation timed out".to_string())
                    })??;
                let mut client = async_imap::Client::new(ImapStream::Plain(tcp_stream));
                tokio::time::timeout(
                    DEFAULT_TIMEOUT,
                    client.run_command_and_check_ok("STARTTLS", None),
                )
                .await
                .map_err(|_| ImapError::ConnectionFailed("operation timed out".to_string()))??;
                let stream = client.into_inner();
                let ImapStream::Plain(tcp_stream) = stream else {
                    return Err(ImapError::Tls(
                        "unexpected IMAP stream state after STARTTLS".to_string(),
                    ));
                };
                let connector = build_tls_connector(accept_invalid_certs)?;
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

        let mut folders = Vec::with_capacity(names.len());
        for name in names {
            let raw_name = name.name().to_string();
            let no_select = name
                .attributes()
                .iter()
                .any(|attribute| matches!(attribute, NameAttribute::NoSelect));
            // STATUS is refused for `\Noselect` hierarchy parents and may fail
            // for individual folders on some servers; degrade to `None` counts
            // instead of failing the whole listing.
            let status = if no_select {
                None
            } else {
                self.folder_status(&raw_name).await.ok()
            };
            let (unseen, total) = status
                .map(|status| (status.unseen, status.total))
                .unwrap_or((None, None));
            folders.push(MailFolder {
                display_name: decode_modified_utf7(&raw_name),
                role: folder_role(name.attributes()).map(str::to_string),
                name: raw_name,
                delimiter: name.delimiter().map(str::to_string),
                unseen,
                total,
            });
        }
        Ok(folders)
    }

    /// Fetch UNSEEN/MESSAGES counts for a folder via IMAP STATUS.
    pub async fn folder_status(&mut self, folder: &str) -> Result<FolderStatus, ImapError> {
        let mailbox = tokio::time::timeout(
            DEFAULT_TIMEOUT,
            self.session.status(folder, "(MESSAGES UNSEEN)"),
        )
        .await
        .map_err(|_| ImapError::CommandFailed("IMAP command timed out".to_string()))??;
        Ok(FolderStatus {
            unseen: mailbox.unseen,
            total: Some(mailbox.exists),
        })
    }

    pub async fn select_folder(&mut self, folder: &str) -> Result<Option<u32>, ImapError> {
        let mailbox = tokio::time::timeout(DEFAULT_TIMEOUT, self.session.select(folder))
            .await
            .map_err(|_| ImapError::CommandFailed("IMAP command timed out".to_string()))??;
        Ok(mailbox.uid_validity)
    }

    /// Fetch one date-sorted page of message summaries from a folder.
    pub async fn fetch_message_summaries(
        &mut self,
        folder: &str,
        limit: usize,
        cursor: Option<u32>,
        search: Option<&str>,
        sort: MailSortOrder,
    ) -> Result<(Option<u32>, Vec<ImapMessageSummary>), ImapError> {
        let uidvalidity = self.select_folder(folder).await?;

        let criteria = search.map_or_else(|| "ALL".to_string(), build_text_search_query);
        let uids = tokio::time::timeout(DEFAULT_TIMEOUT, self.session.uid_search(criteria))
            .await
            .map_err(|_| ImapError::CommandFailed("IMAP command timed out".to_string()))??;
        let uids: Vec<u32> = uids.into_iter().collect();
        if uids.is_empty() {
            return Ok((uidvalidity, Vec::new()));
        }

        // Fetch the raw header block instead of ENVELOPE: some servers (e.g.
        // Stalwart with UTF8=ACCEPT) return raw UTF-8 inside ENVELOPE quoted
        // strings, which the imap-proto parser rejects. BODY.PEEK[HEADER] is a
        // length-prefixed literal and parses regardless of content encoding.
        // ponytail: this fetches all matching headers because async-imap has no
        // SORT API; use server-side SORT when the connector exposes it.
        let mut summaries = Vec::with_capacity(uids.len());
        for chunk in uids.chunks(500) {
            let uid_set = chunk
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(",");
            let fetches = tokio::time::timeout(DEFAULT_TIMEOUT, async {
                self.session
                    .uid_fetch(uid_set, "(UID RFC822.SIZE FLAGS BODY.PEEK[HEADER])")
                    .await?
                    .try_collect::<Vec<Fetch>>()
                    .await
            })
            .await
            .map_err(|_| ImapError::CommandFailed("IMAP command timed out".to_string()))??;
            summaries.extend(fetches.into_iter().filter_map(summary_from_fetch));
        }
        sort_summaries_by_date(&mut summaries, sort);
        Ok((uidvalidity, select_summary_page(summaries, limit, cursor)))
    }

    pub async fn fetch_rfc822(
        &mut self,
        folder: &str,
        uid: u32,
        expected_uidvalidity: Option<i64>,
    ) -> Result<Vec<u8>, ImapError> {
        let uidvalidity = self.select_folder(folder).await?;
        if uidvalidity.map(i64::from) != expected_uidvalidity {
            return Err(ImapError::UidValidityMismatch {
                uid,
                expected: expected_uidvalidity,
                actual: uidvalidity.map(i64::from),
            });
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
            .ok_or(ImapError::MessageNotFound(uid))?;
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
            .ok_or(ImapError::MessageNotFound(uid))?;

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
        since: Option<NaiveDate>,
        before: Option<NaiveDate>,
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

    pub async fn append_message(&mut self, folder: &str, message: &[u8]) -> Result<(), ImapError> {
        tokio::time::timeout(
            DEFAULT_TIMEOUT,
            self.session.append(folder, None, None, message),
        )
        .await
        .map_err(|_| ImapError::CommandFailed("IMAP command timed out".to_string()))??;
        Ok(())
    }

    pub async fn mark_seen(&mut self, folder: &str, uid: u32, seen: bool) -> Result<(), ImapError> {
        self.select_folder(folder).await?;
        let query = if seen {
            "+FLAGS.SILENT (\\Seen)"
        } else {
            "-FLAGS.SILENT (\\Seen)"
        };
        tokio::time::timeout(
            DEFAULT_TIMEOUT,
            self.session.uid_store(uid.to_string(), query),
        )
        .await
        .map_err(|_| ImapError::CommandFailed("IMAP command timed out".to_string()))??
        .try_collect::<Vec<Fetch>>()
        .await
        .map_err(|e| ImapError::CommandFailed(format!("UID STORE failed: {e}")))?;
        Ok(())
    }

    pub async fn mark_flagged(
        &mut self,
        folder: &str,
        uid: u32,
        flagged: bool,
    ) -> Result<(), ImapError> {
        self.select_folder(folder).await?;
        let query = if flagged {
            "+FLAGS.SILENT (\\Flagged)"
        } else {
            "-FLAGS.SILENT (\\Flagged)"
        };
        tokio::time::timeout(
            DEFAULT_TIMEOUT,
            self.session.uid_store(uid.to_string(), query),
        )
        .await
        .map_err(|_| ImapError::CommandFailed("IMAP command timed out".to_string()))??
        .try_collect::<Vec<Fetch>>()
        .await
        .map_err(|e| ImapError::CommandFailed(format!("UID STORE failed: {e}")))?;
        Ok(())
    }

    /// Fetch the current `\Seen` / `\Flagged` state for a message.
    pub async fn fetch_message_flags(
        &mut self,
        folder: &str,
        uid: u32,
    ) -> Result<(bool, bool), ImapError> {
        self.select_folder(folder).await?;
        let fetches = tokio::time::timeout(DEFAULT_TIMEOUT, async {
            self.session
                .uid_fetch(uid.to_string(), "FLAGS")
                .await?
                .try_collect::<Vec<Fetch>>()
                .await
        })
        .await
        .map_err(|_| ImapError::CommandFailed("IMAP command timed out".to_string()))??;
        let fetch = fetches
            .into_iter()
            .next()
            .ok_or(ImapError::MessageNotFound(uid))?;
        let mut is_seen = false;
        let mut is_flagged = false;
        for flag in fetch.flags() {
            match flag {
                async_imap::types::Flag::Seen => is_seen = true,
                async_imap::types::Flag::Flagged => is_flagged = true,
                _ => {}
            }
        }
        Ok((is_seen, is_flagged))
    }

    pub async fn copy_message(
        &mut self,
        folder: &str,
        uid: u32,
        destination_folder: &str,
    ) -> Result<(), ImapError> {
        self.select_folder(folder).await?;
        tokio::time::timeout(
            DEFAULT_TIMEOUT,
            self.session.uid_copy(uid.to_string(), destination_folder),
        )
        .await
        .map_err(|_| ImapError::CommandFailed("IMAP command timed out".to_string()))??;
        Ok(())
    }

    pub async fn supports_uidplus(&mut self) -> Result<bool, ImapError> {
        let caps = tokio::time::timeout(DEFAULT_TIMEOUT, self.session.capabilities())
            .await
            .map_err(|_| ImapError::CommandFailed("IMAP command timed out".to_string()))?
            .map_err(|e| ImapError::CommandFailed(format!("CAPABILITY failed: {e}")))?;
        Ok(caps.has_str("UIDPLUS"))
    }

    pub async fn supports_move(&mut self) -> Result<bool, ImapError> {
        let caps = tokio::time::timeout(DEFAULT_TIMEOUT, self.session.capabilities())
            .await
            .map_err(|_| ImapError::CommandFailed("IMAP command timed out".to_string()))?
            .map_err(|e| ImapError::CommandFailed(format!("CAPABILITY failed: {e}")))?;
        Ok(caps.has_str("MOVE"))
    }

    pub async fn move_message(
        &mut self,
        folder: &str,
        uid: u32,
        destination_folder: &str,
    ) -> Result<(), ImapError> {
        self.select_folder(folder).await?;
        tokio::time::timeout(
            DEFAULT_TIMEOUT,
            self.session.uid_mv(uid.to_string(), destination_folder),
        )
        .await
        .map_err(|_| ImapError::CommandFailed("IMAP command timed out".to_string()))??;
        Ok(())
    }

    pub async fn delete_message(&mut self, folder: &str, uid: u32) -> Result<(), ImapError> {
        self.select_folder(folder).await?;
        if !self.supports_uidplus().await? {
            return Err(ImapError::CommandFailed(
                "Server does not support UIDPLUS; refusing unsafe mailbox-wide EXPUNGE".to_string(),
            ));
        }
        tokio::time::timeout(
            DEFAULT_TIMEOUT,
            self.session
                .uid_store(uid.to_string(), "+FLAGS.SILENT (\\Deleted)"),
        )
        .await
        .map_err(|_| ImapError::CommandFailed("IMAP command timed out".to_string()))??
        .try_collect::<Vec<Fetch>>()
        .await
        .map_err(|e| ImapError::CommandFailed(format!("UID STORE failed: {e}")))?;
        tokio::time::timeout(DEFAULT_TIMEOUT, self.session.uid_expunge(uid.to_string()))
            .await
            .map_err(|_| ImapError::CommandFailed("IMAP command timed out".to_string()))??
            .try_collect::<Vec<_>>()
            .await
            .map_err(|e| ImapError::CommandFailed(format!("UID EXPUNGE failed: {e}")))?;
        Ok(())
    }

    pub async fn logout(mut self) -> Result<(), ImapError> {
        tokio::time::timeout(DEFAULT_TIMEOUT, self.session.logout())
            .await
            .map_err(|_| ImapError::CommandFailed("IMAP command timed out".to_string()))??;
        Ok(())
    }
}

fn build_text_search_query(search: &str) -> String {
    let escaped = search
        .replace(['\r', '\n'], " ")
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    format!("TEXT \"{escaped}\"")
}

fn folder_role(attributes: &[NameAttribute<'_>]) -> Option<&'static str> {
    attributes.iter().find_map(|attribute| match attribute {
        NameAttribute::Archive => Some("archive"),
        NameAttribute::Drafts => Some("drafts"),
        NameAttribute::Sent => Some("sent"),
        NameAttribute::Trash => Some("trash"),
        _ => None,
    })
}

fn decode_modified_utf7(value: &str) -> String {
    let mut decoded = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find('&') {
        decoded.push_str(&rest[..start]);
        rest = &rest[start + 1..];
        let Some(end) = rest.find('-') else {
            decoded.push('&');
            decoded.push_str(rest);
            return decoded;
        };
        let encoded = &rest[..end];
        if encoded.is_empty() {
            decoded.push('&');
        } else {
            let base64 = encoded.replace(',', "/");
            let segment = base64::engine::general_purpose::STANDARD_NO_PAD
                .decode(base64)
                .ok()
                .filter(|bytes| bytes.len() % 2 == 0)
                .and_then(|bytes| {
                    String::from_utf16(
                        &bytes
                            .chunks_exact(2)
                            .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
                            .collect::<Vec<_>>(),
                    )
                    .ok()
                });
            decoded.push_str(segment.as_deref().unwrap_or(&rest[..=end]));
        }
        rest = &rest[end + 1..];
    }
    decoded.push_str(rest);
    decoded
}

/// Trait abstracting the IMAP operations required by archive jobs.
///
/// This allows archive job processing to be tested without a real IMAP server.
#[async_trait]
pub trait ImapArchiveSession: Send {
    async fn fetch_uids_by_date_range(
        &mut self,
        folder: &str,
        since: Option<NaiveDate>,
        before: Option<NaiveDate>,
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
        since: Option<NaiveDate>,
        before: Option<NaiveDate>,
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

/// Minimal IMAP session surface for guarded mailbox mutations, abstracted so
/// operations can be unit-tested with a mock session.
#[async_trait]
pub trait ImapMailboxSession: Send {
    async fn select_folder(&mut self, folder: &str) -> Result<Option<u32>, ImapError>;
    async fn mark_seen(&mut self, folder: &str, uid: u32, seen: bool) -> Result<(), ImapError>;
    async fn mark_flagged(
        &mut self,
        folder: &str,
        uid: u32,
        flagged: bool,
    ) -> Result<(), ImapError>;
    async fn folder_status(&mut self, folder: &str) -> Result<FolderStatus, ImapError>;
    async fn supports_move(&mut self) -> Result<bool, ImapError>;
    async fn move_message(
        &mut self,
        folder: &str,
        uid: u32,
        destination_folder: &str,
    ) -> Result<(), ImapError>;
    async fn copy_message(
        &mut self,
        folder: &str,
        uid: u32,
        destination_folder: &str,
    ) -> Result<(), ImapError>;
    async fn supports_uidplus(&mut self) -> Result<bool, ImapError>;
    async fn delete_message(&mut self, folder: &str, uid: u32) -> Result<(), ImapError>;
}

#[async_trait]
impl ImapMailboxSession for ImapSession {
    async fn select_folder(&mut self, folder: &str) -> Result<Option<u32>, ImapError> {
        ImapSession::select_folder(self, folder).await
    }

    async fn mark_seen(&mut self, folder: &str, uid: u32, seen: bool) -> Result<(), ImapError> {
        ImapSession::mark_seen(self, folder, uid, seen).await
    }

    async fn mark_flagged(
        &mut self,
        folder: &str,
        uid: u32,
        flagged: bool,
    ) -> Result<(), ImapError> {
        ImapSession::mark_flagged(self, folder, uid, flagged).await
    }

    async fn folder_status(&mut self, folder: &str) -> Result<FolderStatus, ImapError> {
        ImapSession::folder_status(self, folder).await
    }

    async fn supports_move(&mut self) -> Result<bool, ImapError> {
        ImapSession::supports_move(self).await
    }

    async fn move_message(
        &mut self,
        folder: &str,
        uid: u32,
        destination_folder: &str,
    ) -> Result<(), ImapError> {
        ImapSession::move_message(self, folder, uid, destination_folder).await
    }

    async fn copy_message(
        &mut self,
        folder: &str,
        uid: u32,
        destination_folder: &str,
    ) -> Result<(), ImapError> {
        ImapSession::copy_message(self, folder, uid, destination_folder).await
    }

    async fn supports_uidplus(&mut self) -> Result<bool, ImapError> {
        ImapSession::supports_uidplus(self).await
    }

    async fn delete_message(&mut self, folder: &str, uid: u32) -> Result<(), ImapError> {
        ImapSession::delete_message(self, folder, uid).await
    }
}

fn build_archive_search_query(since: Option<NaiveDate>, before: Option<NaiveDate>) -> String {
    let mut criteria = vec!["ALL".to_string()];
    if let Some(since) = since {
        criteria.push(format!("SINCE {}", since.format("%d-%b-%Y")));
    }
    if let Some(before) = before {
        criteria.push(format!("BEFORE {}", before.format("%d-%b-%Y")));
    }
    criteria.join(" ")
}

/// Certificate verifier that accepts any certificate. Only used for
/// internal/private mail servers, where self-signed certificates and
/// hostname mismatches (connecting by IP address) are common. Never used for
/// public destinations — see [`should_accept_invalid_certs`].
#[derive(Debug)]
struct NoCertVerifier;

impl tokio_rustls::rustls::client::danger::ServerCertVerifier for NoCertVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &tokio_rustls::rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[tokio_rustls::rustls::pki_types::CertificateDer<'_>],
        _server_name: &tokio_rustls::rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: tokio_rustls::rustls::pki_types::UnixTime,
    ) -> Result<tokio_rustls::rustls::client::danger::ServerCertVerified, tokio_rustls::rustls::Error>
    {
        Ok(tokio_rustls::rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &tokio_rustls::rustls::pki_types::CertificateDer<'_>,
        _dss: &tokio_rustls::rustls::DigitallySignedStruct,
    ) -> Result<
        tokio_rustls::rustls::client::danger::HandshakeSignatureValid,
        tokio_rustls::rustls::Error,
    > {
        Ok(tokio_rustls::rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &tokio_rustls::rustls::pki_types::CertificateDer<'_>,
        _dss: &tokio_rustls::rustls::DigitallySignedStruct,
    ) -> Result<
        tokio_rustls::rustls::client::danger::HandshakeSignatureValid,
        tokio_rustls::rustls::Error,
    > {
        Ok(tokio_rustls::rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<tokio_rustls::rustls::SignatureScheme> {
        tokio_rustls::rustls::crypto::aws_lc_rs::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

fn build_tls_connector(
    accept_invalid_certs: bool,
) -> Result<tokio_rustls::TlsConnector, ImapError> {
    let root_store = tokio_rustls::rustls::RootCertStore::from_iter(
        webpki_roots::TLS_SERVER_ROOTS.iter().cloned(),
    );
    let mut config = tokio_rustls::rustls::ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();
    if accept_invalid_certs {
        config
            .dangerous()
            .set_certificate_verifier(Arc::new(NoCertVerifier));
    }
    Ok(tokio_rustls::TlsConnector::from(Arc::new(config)))
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
    let header = fetch.header()?;
    let mut is_seen = false;
    let mut is_flagged = false;
    for flag in fetch.flags() {
        match flag {
            async_imap::types::Flag::Seen => is_seen = true,
            async_imap::types::Flag::Flagged => is_flagged = true,
            _ => {}
        }
    }
    summary_from_header_bytes(
        uid,
        header,
        i64::from(fetch.size.unwrap_or(0)),
        is_seen,
        is_flagged,
    )
}

/// Build a message summary from a raw RFC 5322 header block. Uses mailparse,
/// which handles both RFC 2047 encoded-words and raw UTF-8 header values, so
/// servers that return non-ASCII content (e.g. Stalwart) work the same as
/// servers that strictly encode (e.g. Gmail).
fn summary_from_header_bytes(
    uid: u32,
    header: &[u8],
    size_bytes: i64,
    is_seen: bool,
    is_flagged: bool,
) -> Option<ImapMessageSummary> {
    let (headers, _) = mailparse::parse_headers(header).ok()?;

    let subject = headers.get_first_value("Subject");

    let (from_name, from_address) = headers
        .get_first_header("From")
        .and_then(|from_header| addrparse_header(from_header).ok())
        .and_then(|addrs| addrs.iter().next().and_then(mail_addr_to_address))
        .map(|(name, address)| (name, Some(address)))
        .unwrap_or((None, None));

    let sent_at = headers
        .get_first_value("Date")
        .and_then(|d| parse_imap_date(&d));

    Some(ImapMessageSummary {
        uid,
        subject,
        from_address,
        from_name,
        sent_at,
        size_bytes,
        is_seen,
        is_flagged,
    })
}

fn select_summary_page(
    summaries: Vec<ImapMessageSummary>,
    limit: usize,
    cursor: Option<u32>,
) -> Vec<ImapMessageSummary> {
    let start = cursor
        .and_then(|cursor| summaries.iter().position(|summary| summary.uid == cursor))
        .map_or(0, |position| position + 1);
    summaries.into_iter().skip(start).take(limit).collect()
}

/// Order a mailbox by (Date header, UID) in the requested direction.
/// Messages without a parseable Date header are treated as the oldest; the
/// UID is the deterministic tiebreak.
fn sort_summaries_by_date(summaries: &mut [ImapMessageSummary], sort: MailSortOrder) {
    match sort {
        MailSortOrder::DateDesc => {
            summaries.sort_by_key(|summary| std::cmp::Reverse((summary.sent_at, summary.uid)));
        }
        MailSortOrder::DateAsc => {
            summaries.sort_by_key(|summary| (summary.sent_at, summary.uid));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modified_utf7_folder_name_decodes_without_changing_ascii() {
        assert_eq!(decode_modified_utf7("Entw&APw-rfe"), "Entwürfe");
        assert_eq!(decode_modified_utf7("Inbox &- Archive"), "Inbox & Archive");
    }

    #[test]
    fn special_use_folder_role_prefers_server_attributes() {
        assert_eq!(folder_role(&[NameAttribute::Archive]), Some("archive"));
        assert_eq!(folder_role(&[NameAttribute::Trash]), Some("trash"));
    }

    #[test]
    fn text_search_query_escapes_imap_quoted_strings() {
        assert_eq!(
            build_text_search_query("quarter \\\"report\"\r\n"),
            r#"TEXT "quarter \\\"report\"  ""#
        );
    }

    #[test]
    fn summary_parses_plain_ascii_header_block() {
        let header = b"Subject: Hello, world!\r\nFrom: \"John Doe\" <john@example.com>\r\nDate: Mon, 15 Aug 2022 10:30:00 +0000\r\n\r\n";
        let summary = summary_from_header_bytes(7, header, 1234, false, false).unwrap();
        assert_eq!(summary.uid, 7);
        assert_eq!(summary.subject, Some("Hello, world!".to_string()));
        assert_eq!(summary.from_name, Some("John Doe".to_string()));
        assert_eq!(summary.from_address, Some("john@example.com".to_string()));
        assert_eq!(summary.sent_at.unwrap().timestamp(), 1_660_559_400);
        assert_eq!(summary.size_bytes, 1234);
        assert!(!summary.is_seen);
        assert!(!summary.is_flagged);
    }

    #[test]
    fn summary_decodes_rfc2047_headers() {
        // Base64-encoded UTF-8 for "Привет" (Russian for "Hi").
        let header = b"Subject: =?UTF-8?B?0J/RgNC40LLQtdGC?=\r\nFrom: =?UTF-8?B?0J/RgNC40LLQtdGC?= <test@example.com>\r\n\r\n";
        let summary = summary_from_header_bytes(1, header, 0, true, false).unwrap();
        assert_eq!(summary.subject, Some("Привет".to_string()));
        assert_eq!(summary.from_name, Some("Привет".to_string()));
        assert_eq!(summary.from_address, Some("test@example.com".to_string()));
        assert!(summary.is_seen);
    }

    #[test]
    fn summary_carries_flagged_state() {
        let header = b"Subject: Hi\r\nFrom: <a@example.com>\r\n\r\n";
        let summary = summary_from_header_bytes(5, header, 0, false, true).unwrap();
        assert!(!summary.is_seen);
        assert!(summary.is_flagged);
    }

    #[test]
    fn summary_accepts_raw_utf8_headers() {
        // Stalwart (UTF8=ACCEPT) returns raw UTF-8 in ENVELOPE/headers, which
        // the ENVELOPE parser could not handle; the header block parser must.
        let header = "Subject: Legal update – no action needed 📃\r\nFrom: Finom Legal <hello@legal.finom.co>\r\nDate: Thu, 16 Jul 2026 10:09:51 +0000\r\n\r\n".as_bytes();
        let summary = summary_from_header_bytes(9719, header, 44747, true, false).unwrap();
        assert_eq!(
            summary.subject,
            Some("Legal update – no action needed 📃".to_string())
        );
        assert_eq!(summary.from_name, Some("Finom Legal".to_string()));
        assert_eq!(
            summary.from_address,
            Some("hello@legal.finom.co".to_string())
        );
    }

    #[test]
    fn summary_tolerates_missing_headers() {
        let summary =
            summary_from_header_bytes(3, b"X-Custom: 1\r\n\r\n", 0, false, false).unwrap();
        assert_eq!(summary.subject, None);
        assert_eq!(summary.from_name, None);
        assert_eq!(summary.from_address, None);
        assert_eq!(summary.sent_at, None);
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
        let since = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        let query = build_archive_search_query(Some(since), None);
        assert_eq!(query, "ALL SINCE 15-Jan-2024");
    }

    #[test]
    fn build_archive_search_query_before_only() {
        let before = NaiveDate::from_ymd_opt(2024, 6, 30).unwrap();
        let query = build_archive_search_query(None, Some(before));
        assert_eq!(query, "ALL BEFORE 30-Jun-2024");
    }

    #[test]
    fn build_archive_search_query_both_bounds() {
        let since = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let before = NaiveDate::from_ymd_opt(2024, 12, 31).unwrap();
        let query = build_archive_search_query(Some(since), Some(before));
        assert_eq!(query, "ALL SINCE 01-Jan-2024 BEFORE 31-Dec-2024");
    }

    fn summary_with(uid: u32, sent_at: Option<DateTime<Utc>>) -> ImapMessageSummary {
        ImapMessageSummary {
            uid,
            subject: None,
            from_address: None,
            from_name: None,
            sent_at,
            size_bytes: 0,
            is_seen: false,
            is_flagged: false,
        }
    }

    #[test]
    fn sort_summaries_desc_orders_by_date_then_uid_descending() {
        let ts = DateTime::from_timestamp(1_660_559_400, 0).unwrap();
        let older = DateTime::from_timestamp(1_660_000_000, 0).unwrap();
        let mut summaries = vec![
            summary_with(1, Some(older)),
            summary_with(2, Some(ts)),
            summary_with(3, Some(ts)),
        ];
        sort_summaries_by_date(&mut summaries, MailSortOrder::DateDesc);
        let uids: Vec<u32> = summaries.iter().map(|s| s.uid).collect();
        // Equal dates fall back to the higher UID first.
        assert_eq!(uids, vec![3, 2, 1]);
    }

    #[test]
    fn sort_summaries_asc_orders_by_date_then_uid_ascending() {
        let ts = DateTime::from_timestamp(1_660_559_400, 0).unwrap();
        let older = DateTime::from_timestamp(1_660_000_000, 0).unwrap();
        let mut summaries = vec![
            summary_with(3, Some(ts)),
            summary_with(1, Some(older)),
            summary_with(2, Some(ts)),
        ];
        sort_summaries_by_date(&mut summaries, MailSortOrder::DateAsc);
        let uids: Vec<u32> = summaries.iter().map(|s| s.uid).collect();
        assert_eq!(uids, vec![1, 2, 3]);
    }

    #[test]
    fn sort_summaries_treats_missing_date_as_oldest() {
        let ts = DateTime::from_timestamp(1_660_559_400, 0).unwrap();
        let mut summaries = vec![summary_with(1, None), summary_with(2, Some(ts))];
        sort_summaries_by_date(&mut summaries, MailSortOrder::DateDesc);
        assert_eq!(summaries[0].uid, 2);
        sort_summaries_by_date(&mut summaries, MailSortOrder::DateAsc);
        assert_eq!(summaries[0].uid, 1);
    }

    #[test]
    fn pagination_uses_the_complete_date_sorted_mailbox() {
        let newest = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        let middle = DateTime::from_timestamp(1_600_000_000, 0).unwrap();
        let oldest = DateTime::from_timestamp(1_500_000_000, 0).unwrap();
        let mut summaries = vec![
            summary_with(100, Some(oldest)),
            summary_with(1, Some(newest)),
            summary_with(50, Some(middle)),
        ];
        sort_summaries_by_date(&mut summaries, MailSortOrder::DateDesc);

        let first = select_summary_page(summaries.clone(), 2, None);
        assert_eq!(
            first.iter().map(|summary| summary.uid).collect::<Vec<_>>(),
            vec![1, 50]
        );
        let second = select_summary_page(summaries, 2, Some(50));
        assert_eq!(
            second.iter().map(|summary| summary.uid).collect::<Vec<_>>(),
            vec![100]
        );
    }
}
