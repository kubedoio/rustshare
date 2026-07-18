use chrono::{DateTime, Utc};
use mailparse::{
    addrparse_header, parse_mail, DispositionType, MailAddr, MailHeader, MailHeaderMap,
    ParsedMail as MailParsedMail, SingleInfo,
};
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

pub struct EmlParser;

impl EmlParser {
    pub fn parse(bytes: &[u8]) -> Result<ParsedMail, EmlParseError> {
        let parsed = parse_mail(bytes).map_err(|e| EmlParseError::ParseFailed(e.to_string()))?;

        let message_id = header_value(&parsed, "Message-ID").map(trim_angle_brackets);
        let in_reply_to = header_value(&parsed, "In-Reply-To").map(trim_angle_brackets);
        let references = header_value(&parsed, "References")
            .map(|v| v.split_whitespace().map(trim_angle_brackets).collect())
            .unwrap_or_default();
        let subject = header_value(&parsed, "Subject");
        let sent_at = parse_date_header(&parsed)?;

        let from = parse_address(&parsed, "From")?;
        let to = parse_address_list(&parsed, "To")?;
        let cc = parse_address_list(&parsed, "Cc")?;
        let bcc = parse_address_list(&parsed, "Bcc")?;

        let mut body_text = None;
        let mut body_html = None;
        let mut attachments = Vec::new();

        collect_parts(&parsed, &mut body_text, &mut body_html, &mut attachments)?;

        Ok(ParsedMail {
            message_id,
            in_reply_to,
            references,
            subject,
            from,
            to,
            cc,
            bcc,
            sent_at,
            body_text,
            body_html,
            attachments,
        })
    }
}

fn header_value(parsed: &MailParsedMail, name: &str) -> Option<String> {
    parsed.headers.get_first_header(name).map(|h| h.get_value())
}

fn trim_angle_brackets(value: impl AsRef<str>) -> String {
    let value = value.as_ref();
    value
        .trim()
        .trim_start_matches('<')
        .trim_end_matches('>')
        .to_string()
}

fn parse_date_header(parsed: &MailParsedMail) -> Result<Option<DateTime<Utc>>, EmlParseError> {
    let Some(raw) = parsed.headers.get_first_header("Date") else {
        return Ok(None);
    };
    let value = raw.get_value();
    let timestamp =
        mailparse::dateparse(&value).map_err(|e| EmlParseError::InvalidDate(e.to_string()))?;
    DateTime::from_timestamp(timestamp, 0)
        .ok_or_else(|| EmlParseError::InvalidDate(value.clone()))
        .map(Some)
}

fn parse_address(
    parsed: &MailParsedMail,
    name: &str,
) -> Result<Option<ParsedAddress>, EmlParseError> {
    let addrs = parse_address_list(parsed, name)?;
    Ok(addrs.into_iter().next())
}

fn parse_address_list(
    parsed: &MailParsedMail,
    name: &str,
) -> Result<Vec<ParsedAddress>, EmlParseError> {
    let Some(header) = find_header(&parsed.headers, name) else {
        return Ok(Vec::new());
    };
    let list = addrparse_header(header).map_err(|e| EmlParseError::ParseFailed(e.to_string()))?;
    Ok(list.iter().flat_map(mail_addr_to_parsed).collect())
}

fn mail_addr_to_parsed(addr: &MailAddr) -> Vec<ParsedAddress> {
    match addr {
        MailAddr::Single(info) => vec![single_info_to_parsed(info)],
        MailAddr::Group(group) => group.addrs.iter().map(single_info_to_parsed).collect(),
    }
}

fn single_info_to_parsed(info: &SingleInfo) -> ParsedAddress {
    ParsedAddress {
        name: info.display_name.clone(),
        address: info.addr.clone(),
    }
}

fn find_header<'a>(headers: &'a [MailHeader], name: &str) -> Option<&'a MailHeader<'a>> {
    headers
        .iter()
        .find(|h| h.get_key().eq_ignore_ascii_case(name))
}

fn collect_parts(
    part: &MailParsedMail,
    body_text: &mut Option<String>,
    body_html: &mut Option<String>,
    attachments: &mut Vec<ParsedAttachment>,
) -> Result<(), EmlParseError> {
    if part.subparts.is_empty() {
        process_leaf_part(part, body_text, body_html, attachments)?;
    } else {
        for sub in &part.subparts {
            collect_parts(sub, body_text, body_html, attachments)?;
        }
    }
    Ok(())
}

fn process_leaf_part(
    part: &MailParsedMail,
    body_text: &mut Option<String>,
    body_html: &mut Option<String>,
    attachments: &mut Vec<ParsedAttachment>,
) -> Result<(), EmlParseError> {
    let cd = part.get_content_disposition();
    let content_id = header_value(part, "Content-ID").map(trim_angle_brackets);
    // A Content-ID alone does not make a part an attachment: multipart/related
    // messages may mark the root body part with a `start` Content-ID. Treat a
    // Content-ID part as an attachment only when it cannot fill the body slot
    // for its MIME type; otherwise the message would import with no readable
    // body and the body part would show up only as an attachment.
    let mimetype = part.ctype.mimetype.as_str();
    let body_slot_open = (mimetype == "text/plain" && body_text.is_none())
        || (mimetype == "text/html" && body_html.is_none());
    let is_attachment = cd.disposition == DispositionType::Attachment
        || cd.params.contains_key("filename")
        || part.ctype.params.contains_key("name")
        || (content_id.is_some() && !body_slot_open);

    if is_attachment {
        let filename = cd
            .params
            .get("filename")
            .cloned()
            .or_else(|| part.ctype.params.get("name").cloned());
        let content_disposition = header_value(part, "Content-Disposition");
        let data = part
            .get_body_raw()
            .map_err(|e| EmlParseError::ParseFailed(e.to_string()))?;
        attachments.push(ParsedAttachment {
            filename,
            mime_type: part.ctype.mimetype.clone(),
            size_bytes: data.len(),
            content_disposition,
            content_id,
            data,
        });
        return Ok(());
    }

    match part.ctype.mimetype.as_str() {
        "text/plain" if body_text.is_none() => {
            let body = part
                .get_body()
                .map_err(|e| EmlParseError::ParseFailed(e.to_string()))?;
            *body_text = Some(body);
        }
        "text/html" if body_html.is_none() => {
            let body = part
                .get_body()
                .map_err(|e| EmlParseError::ParseFailed(e.to_string()))?;
            *body_html = Some(body);
        }
        _ => {}
    }

    Ok(())
}
