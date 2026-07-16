use rustshare_core::services::eml_parser::EmlParser;

fn fixture(name: &str) -> String {
    std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/eml")
            .join(name),
    )
    .unwrap()
}

#[test]
fn parses_simple_plain_text_email() {
    let raw = fixture("simple_plain.eml");
    let parsed = EmlParser::parse(raw.as_bytes()).unwrap();

    assert_eq!(parsed.subject.as_deref(), Some("Hello World"));
    assert_eq!(parsed.message_id.as_deref(), Some("abc123@example.com"));
    assert_eq!(
        parsed.body_text.as_deref(),
        Some("This is a simple plain-text email.\n")
    );
    assert!(parsed.body_html.is_none());
    assert!(parsed.attachments.is_empty());
    assert_eq!(parsed.from.as_ref().unwrap().address, "sender@example.com");
    assert_eq!(parsed.to.len(), 1);
    assert_eq!(parsed.to[0].address, "recipient@example.com");
}

#[test]
fn parses_html_email_and_addresses() {
    let raw = fixture("simple_html.eml");
    let parsed = EmlParser::parse(raw.as_bytes()).unwrap();

    assert_eq!(parsed.subject.as_deref(), Some("HTML email"));
    assert_eq!(parsed.message_id.as_deref(), Some("html456@example.com"));
    assert!(parsed.body_text.is_none());
    assert!(parsed
        .body_html
        .as_deref()
        .unwrap()
        .contains("<p>This is an HTML email.</p>"));
    assert_eq!(parsed.from.as_ref().unwrap().name.as_deref(), Some("Alice"));
    assert_eq!(parsed.cc.len(), 1);
    assert_eq!(parsed.cc[0].name.as_deref(), Some("Carol"));
}

#[test]
fn parses_email_with_attachment() {
    let raw = fixture("with_attachment.eml");
    let parsed = EmlParser::parse(raw.as_bytes()).unwrap();

    assert_eq!(parsed.subject.as_deref(), Some("With attachment"));
    assert_eq!(parsed.attachments.len(), 1);
    let att = &parsed.attachments[0];
    assert_eq!(att.filename.as_deref(), Some("note.txt"));
    assert_eq!(att.mime_type, "text/plain");
    assert_eq!(String::from_utf8_lossy(&att.data), "attachment content");
}

#[test]
fn parses_unnamed_inline_content_id_part_as_attachment() {
    let raw = concat!(
        "MIME-Version: 1.0\r\n",
        "Content-Type: multipart/related; boundary=parts\r\n\r\n",
        "--parts\r\nContent-Type: text/html\r\n\r\n<img src=\"cid:logo\">\r\n",
        "--parts\r\nContent-Type: image/png\r\nContent-ID: <logo>\r\n",
        "Content-Transfer-Encoding: base64\r\n\r\naW1hZ2U=\r\n--parts--\r\n"
    );
    let parsed = EmlParser::parse(raw.as_bytes()).unwrap();

    assert_eq!(parsed.attachments[0].content_id.as_deref(), Some("logo"));
}
