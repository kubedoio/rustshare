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
    pub body_html: Option<&'a str>,
    pub in_reply_to: Option<String>,
    pub references: Option<String>,
    pub attachments: Vec<SmtpAttachment>,
}
