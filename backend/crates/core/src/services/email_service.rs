use lettre::{
    message::Mailbox, transport::smtp::authentication::Credentials, AsyncSmtpTransport,
    AsyncTransport, Message, Tokio1Executor,
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

        let builder = match config.tls_mode.as_deref() {
            Some("tls") => {
                let mut b = AsyncSmtpTransport::<Tokio1Executor>::relay(host)
                    .map_err(|e| EmailError::SmtpSendFailed(e.to_string()))?
                    .port(port);
                if let Some(c) = creds {
                    b = b.credentials(c);
                }
                b
            }
            Some("starttls") => {
                let mut b = AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(host)
                    .map_err(|e| EmailError::SmtpSendFailed(e.to_string()))?
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
