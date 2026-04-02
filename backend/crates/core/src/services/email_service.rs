use lettre::{
    message::Mailbox,
    transport::smtp::authentication::Credentials,
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
}

pub struct EmailService {
    pool: PgPool,
    secret_key: SecretEncryptionKey,
}

impl EmailService {
    pub fn new(pool: PgPool, secret_key: SecretEncryptionKey) -> Self {
        Self { pool, secret_key }
    }

    pub async fn send_invite_email(
        &self,
        sender_name: &str,
        recipient_email: &str,
        invite_link: &str,
        subject_template: &str,
        body_template: &str,
    ) -> Result<(), EmailError> {
        let row = sqlx::query_as::<_, SmtpConfigRow>(
            "SELECT enabled, host, port, username, password_enc, from_address, from_name, tls_mode
             FROM smtp_config
             WHERE id = '00000000-0000-0000-0000-000000000002'"
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| EmailError::SmtpSendFailed(e.to_string()))?;

        let config = row.ok_or(EmailError::SmtpNotConfigured)?;
        if !config.enabled {
            return Err(EmailError::SmtpNotConfigured);
        }

        let host = config.host.ok_or(EmailError::SmtpNotConfigured)?;
        let port = config.port.ok_or(EmailError::SmtpNotConfigured)?;
        let from_address = config.from_address.ok_or(EmailError::SmtpNotConfigured)?;

        let from_name = config.from_name.as_deref().unwrap_or("RustShare");
        let from_mailbox: Mailbox = format!("{} <{}>", from_name, from_address)
            .parse()
            .map_err(|e| EmailError::SmtpSendFailed(format!("Invalid from address: {}", e)))?;

        let to_mailbox: Mailbox = recipient_email
            .parse()
            .map_err(|e| EmailError::SmtpSendFailed(format!("Invalid recipient address: {}", e)))?;

        let subject = subject_template
            .replace("{{sender_name}}", sender_name)
            .replace("{{recipient_name}}", recipient_email)
            .replace("{{invite_link}}", invite_link);

        let body = body_template
            .replace("{{sender_name}}", sender_name)
            .replace("{{recipient_name}}", recipient_email)
            .replace("{{invite_link}}", invite_link);

        let email = Message::builder()
            .from(from_mailbox)
            .to(to_mailbox)
            .subject(subject)
            .body(body)
            .map_err(|e| EmailError::SmtpSendFailed(e.to_string()))?;

        let mut builder = AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&host)
            .port(port as u16);

        if let Some(ref username) = config.username {
            let password = if let Some(ref enc) = config.password_enc {
                decrypt_secret(enc, &self.secret_key)
                    .map_err(|_| EmailError::DecryptFailed)?
            } else {
                String::new()
            };
            builder = builder.credentials(Credentials::new(username.clone(), password));
        }

        match config.tls_mode.as_deref() {
            Some("tls") => {
                builder = AsyncSmtpTransport::<Tokio1Executor>::relay(&host)
                    .map_err(|e| EmailError::SmtpSendFailed(e.to_string()))?
                    .port(port as u16);
                if let Some(ref username) = config.username {
                    let password = if let Some(ref enc) = config.password_enc {
                        decrypt_secret(enc, &self.secret_key)
                            .map_err(|_| EmailError::DecryptFailed)?
                    } else {
                        String::new()
                    };
                    builder = builder.credentials(Credentials::new(username.clone(), password));
                }
            }
            Some("starttls") => {
                builder = AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&host)
                    .map_err(|e| EmailError::SmtpSendFailed(e.to_string()))?
                    .port(port as u16);
                if let Some(ref username) = config.username {
                    let password = if let Some(ref enc) = config.password_enc {
                        decrypt_secret(enc, &self.secret_key)
                            .map_err(|_| EmailError::DecryptFailed)?
                    } else {
                        String::new()
                    };
                    builder = builder.credentials(Credentials::new(username.clone(), password));
                }
            }
            _ => {}
        }

        let transport = builder.build();

        transport
            .send(email)
            .await
            .map_err(|e| EmailError::SmtpSendFailed(e.to_string()))?;

        Ok(())
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
