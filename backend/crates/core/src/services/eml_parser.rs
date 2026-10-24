use chrono::{DateTime, Utc};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EmlParseError {
    #[error("Failed to parse .eml: {0}")]
    ParseFailed(String),
    #[error("Invalid message body")]
    InvalidBody,
    #[error("Date header could not be parsed: {0}")]
    InvalidDate(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedAddress {
    pub name: Option<String>,
    pub address: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedAttachment {
    pub filename: Option<String>,
    pub mime_type: String,
    pub size_bytes: usize,
    pub content_disposition: Option<String>,
    pub content_id: Option<String>,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedMail {
    pub message_id: Option<String>,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    pub subject: Option<String>,
    pub from: Option<ParsedAddress>,
    pub to: Vec<ParsedAddress>,
    pub cc: Vec<ParsedAddress>,
    pub bcc: Vec<ParsedAddress>,
    pub sent_at: Option<DateTime<Utc>>,
    pub body_text: Option<String>,
    pub body_html: Option<String>,
    pub attachments: Vec<ParsedAttachment>,
}
