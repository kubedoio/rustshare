use crate::validation::resolve_public_socket_addrs;
use lettre::{
    address::Envelope,
    message::{
        header::{ContentDisposition, ContentType},
        Mailbox, MultiPart, SinglePart,
    },
    transport::smtp::{
        authentication::Credentials,
        client::{Tls, TlsParameters},
    },
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};
use rustshare_crypto::{decrypt_secret, SecretEncryptionKey};
use sqlx::PgPool;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EmailError {
    #[error("SMTP is not configured or not enabled")]
    SmtpNotConfigured,
    #[error("Failed to decrypt SMTP password")]
    DecryptFailed,
    #[error("Failed to send email: {0}")]
    SmtpSendFailed(String),
    #[error("Invalid SMTP TLS mode: {0}. Must be 'tls' or 'starttls'")]
    InvalidTlsMode(String),
}

pub struct EmailService {
    pool: PgPool,
    secret_key: SecretEncryptionKey,
}

pub struct OutboundEmail<'a> {
    pub sender_name: &'a str,
    pub sender_email: &'a str,
    pub recipients: &'a [String],
    pub cc: &'a [String],
    pub bcc: &'a [String],
    pub subject: &'a str,
    pub body: &'a str,
}

#[derive(Clone)]
pub struct SmtpAttachment {
    pub filename: String,
    pub mime_type: String,
    pub content: Vec<u8>,
}

pub struct OutboundMailMessage<'a> {
    pub recipients: &'a [String],
    pub cc: &'a [String],
    pub bcc: &'a [String],
    pub subject: &'a str,
    pub body: &'a str,
    pub in_reply_to: Option<String>,
    pub references: Option<String>,
    pub attachments: Vec<SmtpAttachment>,
}

impl EmailService {
    pub fn new(pool: PgPool, secret_key: SecretEncryptionKey) -> Self {
        Self { pool, secret_key }
    }

    /// Send a test email to `recipient_email` using the stored SMTP
    /// configuration. Used by the admin "Send Test Email" action.
    pub async fn send_test_email(&self, recipient_email: &str) -> Result<(), EmailError> {
        let config = self.load_config().await?;
        let from_address = config
            .from_address
            .as_deref()
            .ok_or(EmailError::SmtpNotConfigured)?;
        let from_name = config.from_name.as_deref().unwrap_or("RustShare");
        let from_mailbox: Mailbox = format!("{} <{}>", from_name, from_address)
            .parse()
            .map_err(|e| EmailError::SmtpSendFailed(format!("Invalid from address: {}", e)))?;

        let to_mailbox: Mailbox = recipient_email
            .parse()
            .map_err(|e| EmailError::SmtpSendFailed(format!("Invalid recipient address: {}", e)))?;

        let email = Message::builder()
            .from(from_mailbox)
            .to(to_mailbox)
            .subject("RustShare SMTP Test")
            .body(String::from(
                "This is a test email from RustShare.\n\nYour SMTP configuration is working correctly.",
            ))
            .map_err(|e| EmailError::SmtpSendFailed(e.to_string()))?;

        let transport = self.build_transport(&config).await?;
        transport
            .send(email)
            .await
            .map_err(|e| EmailError::SmtpSendFailed(e.to_string()))?;

        Ok(())
    }

    pub async fn send_invite_email(
        &self,
        sender_name: &str,
        recipient_name: &str,
        recipient_email: &str,
        invite_link: &str,
        subject_template: &str,
        body_template: &str,
    ) -> Result<(), EmailError> {
        let config = self.load_config().await?;
        let from_address = config
            .from_address
            .as_deref()
            .ok_or(EmailError::SmtpNotConfigured)?;

        let from_name = config.from_name.as_deref().unwrap_or("RustShare");
        let from_mailbox: Mailbox = format!("{} <{}>", from_name, from_address)
            .parse()
            .map_err(|e| EmailError::SmtpSendFailed(format!("Invalid from address: {}", e)))?;

        let to_mailbox: Mailbox = recipient_email
            .parse()
            .map_err(|e| EmailError::SmtpSendFailed(format!("Invalid recipient address: {}", e)))?;

        let subject = subject_template
            .replace("{{sender_name}}", sender_name)
            .replace("{{recipient_name}}", recipient_name)
            .replace("{{invite_link}}", invite_link);

        let body = body_template
            .replace("{{sender_name}}", sender_name)
            .replace("{{recipient_name}}", recipient_name)
            .replace("{{invite_link}}", invite_link);

        let email = Message::builder()
            .from(from_mailbox)
            .to(to_mailbox)
            .subject(subject)
            .body(body)
            .map_err(|e| EmailError::SmtpSendFailed(e.to_string()))?;

        let transport = self.build_transport(&config).await?;
        transport
            .send(email)
            .await
            .map_err(|e| EmailError::SmtpSendFailed(e.to_string()))?;

        Ok(())
    }

    pub async fn send_user_email(&self, email: OutboundEmail<'_>) -> Result<(), EmailError> {
        if email.recipients.is_empty() && email.cc.is_empty() && email.bcc.is_empty() {
            return Err(EmailError::SmtpSendFailed(
                "At least one recipient is required".to_string(),
            ));
        }

        let config = self.load_config().await?;
        let from_address = config
            .from_address
            .as_deref()
            .ok_or(EmailError::SmtpNotConfigured)?;
        let reply_to: Mailbox = format_mailbox(email.sender_name, email.sender_email)?;
        let mut builder = Message::builder()
            .from(format_mailbox(
                config.from_name.as_deref().unwrap_or("RustShare"),
                from_address,
            )?)
            .reply_to(reply_to)
            .subject(email.subject);

        for recipient in email.recipients {
            builder = builder.to(parse_mailbox(recipient)?);
        }
        for recipient in email.cc {
            builder = builder.cc(parse_mailbox(recipient)?);
        }
        for recipient in email.bcc {
            builder = builder.bcc(parse_mailbox(recipient)?);
        }

        let email = builder
            .multipart(
                MultiPart::alternative()
                    .singlepart(SinglePart::plain(email.body.to_string()))
                    .singlepart(SinglePart::html(format!(
                        "<pre style=\"white-space:pre-wrap;font-family:system-ui,sans-serif\">{}</pre>",
                        html_escape(email.body)
                    ))),
            )
            .map_err(|e| EmailError::SmtpSendFailed(e.to_string()))?;

        self.build_transport(&config)
            .await?
            .send(email)
            .await
            .map_err(|e| EmailError::SmtpSendFailed(e.to_string()))?;

        Ok(())
    }

    pub async fn send_user_email_via_smtp(
        &self,
        smtp: &crate::domain::MailSmtpSettings,
        email: OutboundMailMessage<'_>,
    ) -> Result<Vec<u8>, EmailError> {
        if !smtp.is_enabled {
            return Err(EmailError::SmtpNotConfigured);
        }
        if email.recipients.is_empty() && email.cc.is_empty() && email.bcc.is_empty() {
            return Err(EmailError::SmtpSendFailed(
                "At least one recipient is required".to_string(),
            ));
        }

        let email_msg = build_outbound_smtp_message(smtp, email)?;

        let row = SmtpConfigRow {
            enabled: smtp.is_enabled,
            host: Some(smtp.host.clone()),
            port: Some(smtp.port),
            username: Some(smtp.username.clone()),
            password_enc: Some(smtp.password_enc.clone()),
            from_address: Some(smtp.from_address.clone()),
            from_name: smtp.from_name.clone(),
            tls_mode: Some(smtp.tls_mode.clone()),
        };

        let bytes = email_msg.formatted();
        self.build_transport(&row)
            .await?
            .send(email_msg)
            .await
            .map_err(|e| EmailError::SmtpSendFailed(e.to_string()))?;

        Ok(bytes)
    }

    pub fn build_raw_eml(
        &self,
        smtp: &crate::domain::MailSmtpSettings,
        email: OutboundMailMessage<'_>,
    ) -> Result<Vec<u8>, EmailError> {
        let msg = build_outbound_smtp_message(smtp, email)?;
        Ok(msg.formatted())
    }

    async fn load_config(&self) -> Result<SmtpConfigRow, EmailError> {
        let row = sqlx::query_as::<_, SmtpConfigRow>(
            "SELECT enabled, host, port, username, password_enc, from_address, from_name, tls_mode
             FROM smtp_config
             WHERE id = '00000000-0000-0000-0000-000000000002'",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| EmailError::SmtpSendFailed(format!("Database error: {}", e)))?;

        let config = row.ok_or(EmailError::SmtpNotConfigured)?;
        if !config.enabled {
            return Err(EmailError::SmtpNotConfigured);
        }
        Ok(config)
    }

    async fn build_transport(
        &self,
        config: &SmtpConfigRow,
    ) -> Result<AsyncSmtpTransport<Tokio1Executor>, EmailError> {
        let host = config
            .host
            .as_deref()
            .ok_or(EmailError::SmtpNotConfigured)?;
        let port = config.port.ok_or(EmailError::SmtpNotConfigured)?;
        let port = u16::try_from(port).map_err(|_| {
            EmailError::SmtpSendFailed(format!("SMTP port {} is out of range", port))
        })?;
        let connection_host = validate_smtp_host(host, port).await?;

        let creds = config
            .username
            .as_ref()
            .map(|username| {
                let password = if let Some(ref enc) = config.password_enc {
                    decrypt_secret(enc, &self.secret_key).map_err(|_| EmailError::DecryptFailed)
                } else {
                    Ok(String::new())
                }?;
                Ok(Credentials::new(username.clone(), password))
            })
            .transpose()?;

        let builder = match config
            .tls_mode
            .as_deref()
            .map(|s| s.to_lowercase())
            .as_deref()
        {
            Some("tls") => {
                let tls = TlsParameters::new(host.to_string())
                    .map_err(|e| EmailError::SmtpSendFailed(e.to_string()))?;
                let mut b = AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(
                    connection_host.clone(),
                )
                .tls(Tls::Wrapper(tls))
                .port(port);
                if let Some(c) = creds {
                    b = b.credentials(c);
                }
                b
            }
            Some("starttls") => {
                let tls = TlsParameters::new(host.to_string())
                    .map_err(|e| EmailError::SmtpSendFailed(e.to_string()))?;
                let mut b = AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(
                    connection_host.clone(),
                )
                .tls(Tls::Required(tls))
                .port(port);
                if let Some(c) = creds {
                    b = b.credentials(c);
                }
                b
            }
            Some("none") => {
                let mut b =
                    AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(connection_host)
                        .port(port);
                if let Some(c) = creds {
                    b = b.credentials(c);
                }
                b
            }
            other => {
                return Err(EmailError::InvalidTlsMode(
                    other.unwrap_or("(not set)").to_string(),
                ));
            }
        };

        Ok(builder.build())
    }
}

async fn validate_smtp_host(host: &str, port: u16) -> Result<String, EmailError> {
    if cfg!(debug_assertions)
        && std::env::var("RUSTSHARE_ALLOW_INTERNAL_SMTP_FOR_TESTS").as_deref() == Ok("true")
    {
        return Ok(host.to_string());
    }

    let addrs = resolve_public_socket_addrs(host, port).await.map_err(|e| {
        EmailError::SmtpSendFailed(format!("SMTP host failed SSRF validation: {e}"))
    })?;
    addrs
        .first()
        .map(|addr| addr.ip().to_string())
        .ok_or_else(|| EmailError::SmtpSendFailed("SMTP host resolved no addresses".to_string()))
}

fn build_outbound_smtp_message(
    smtp: &crate::domain::MailSmtpSettings,
    email: OutboundMailMessage<'_>,
) -> Result<Message, EmailError> {
    let from_mailbox: Mailbox =
        format_mailbox(smtp.from_name.as_deref().unwrap_or(""), &smtp.from_address)?;
    let mut envelope_to = Vec::new();
    let envelope_from = from_mailbox.email.clone();
    let mut builder = Message::builder()
        .from(from_mailbox.clone())
        .subject(email.subject);

    if let Some(ref reply_to_addr) = smtp.reply_to {
        if !reply_to_addr.trim().is_empty() {
            builder = builder.reply_to(parse_mailbox(reply_to_addr)?);
        }
    }

    if let Some(ref in_reply_to) = email.in_reply_to {
        if !in_reply_to.trim().is_empty() {
            builder = builder.in_reply_to(in_reply_to.clone());
        }
    }

    if let Some(ref references) = email.references {
        if !references.trim().is_empty() {
            builder = builder.references(references.clone());
        }
    }

    for recipient in email.recipients {
        let mailbox = parse_mailbox(recipient)?;
        envelope_to.push(mailbox.email.clone());
        builder = builder.to(mailbox);
    }
    for recipient in email.cc {
        let mailbox = parse_mailbox(recipient)?;
        envelope_to.push(mailbox.email.clone());
        builder = builder.cc(mailbox);
    }
    for recipient in email.bcc {
        envelope_to.push(parse_mailbox(recipient)?.email);
    }

    let envelope = Envelope::new(Some(envelope_from), envelope_to)
        .map_err(|e| EmailError::SmtpSendFailed(e.to_string()))?;
    builder = builder.envelope(envelope);

    let alternative = MultiPart::alternative()
        .singlepart(SinglePart::plain(email.body.to_string()))
        .singlepart(SinglePart::html(format!(
            "<pre style=\"white-space:pre-wrap;font-family:system-ui,sans-serif\">{}</pre>",
            html_escape(email.body)
        )));

    if email.attachments.is_empty() {
        builder.multipart(alternative)
    } else {
        let mut multipart = MultiPart::mixed().multipart(alternative);
        for attachment in email.attachments {
            let ct = ContentType::parse(&attachment.mime_type)
                .unwrap_or_else(|_| ContentType::parse("application/octet-stream").unwrap());
            let part = SinglePart::builder()
                .header(ct)
                .header(ContentDisposition::attachment(&attachment.filename))
                .body(attachment.content);
            multipart = multipart.singlepart(part);
        }
        builder.multipart(multipart)
    }
    .map_err(|e| EmailError::SmtpSendFailed(e.to_string()))
}

fn parse_mailbox(address: &str) -> Result<Mailbox, EmailError> {
    address
        .parse()
        .map_err(|e| EmailError::SmtpSendFailed(format!("Invalid email address: {e}")))
}

fn format_mailbox(name: &str, address: &str) -> Result<Mailbox, EmailError> {
    let address = address
        .parse()
        .map_err(|e| EmailError::SmtpSendFailed(format!("Invalid email address: {e}")))?;
    let display_name = if name.trim().is_empty() {
        None
    } else {
        Some(name.to_string())
    };
    Ok(Mailbox::new(display_name, address))
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[derive(sqlx::FromRow)]
struct SmtpConfigRow {
    enabled: bool,
    host: Option<String>,
    port: Option<i32>,
    username: Option<String>,
    password_enc: Option<String>,
    from_address: Option<String>,
    from_name: Option<String>,
    tls_mode: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::MailSmtpSettings;
    use chrono::Utc;
    use uuid::Uuid;

    fn smtp_settings() -> MailSmtpSettings {
        MailSmtpSettings {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            owner_id: Uuid::new_v4(),
            mail_account_id: Uuid::new_v4(),
            host: "smtp.example.com".to_string(),
            port: 587,
            username: "sender@example.com".to_string(),
            password_enc: "encrypted".to_string(),
            tls_mode: "starttls".to_string(),
            from_address: "sender@example.com".to_string(),
            from_name: Some("Sender".to_string()),
            reply_to: None,
            sent_folder: None,
            is_enabled: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn outbound_smtp_message_keeps_bcc_out_of_headers() {
        let smtp = smtp_settings();
        let bcc = ["blind@example.com".to_string()];
        let to = ["to@example.com".to_string()];
        let msg = build_outbound_smtp_message(
            &smtp,
            OutboundMailMessage {
                recipients: &to,
                cc: &[],
                bcc: &bcc,
                subject: "Subject",
                body: "Body",
                in_reply_to: None,
                references: None,
                attachments: vec![],
            },
        )
        .expect("message should build");

        let raw = String::from_utf8(msg.formatted()).expect("message is utf8");
        assert!(!raw.contains("Bcc:"));
        assert!(!raw.contains("blind@example.com"));
        assert!(msg.envelope().to().iter().any(|addr| {
            let value: &str = addr.as_ref();
            value == "blind@example.com"
        }));
    }

    #[tokio::test]
    async fn validate_smtp_host_rejects_localhost() {
        let err = validate_smtp_host("127.0.0.1", 25)
            .await
            .expect_err("loopback SMTP host should be rejected");
        assert!(err.to_string().contains("internal address"));
    }
}
