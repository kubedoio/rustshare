//! Integration test: Content-Disposition header escaping for public share downloads (Task A2).
//!
//! Verifies that filenames containing quotes are correctly escaped in both the
//! legacy `filename` parameter and the RFC 5987 `filename*` parameter.

use rustshare_server::handlers::public_shares::build_content_disposition;

#[test]
fn public_share_download_header_escaped() {
    let file_name = "report\".txt";
    let header = build_content_disposition(file_name);

    assert!(
        header.contains("filename=\"report\\\".txt\""),
        "legacy filename parameter must escape embedded quotes: {}",
        header
    );
    assert!(
        header.contains("filename*=UTF-8''report%22.txt"),
        "RFC 5987 filename* parameter must URL-encode embedded quotes: {}",
        header
    );
}

#[test]
fn public_share_download_header_newline_sanitized() {
    let file_name = "line\nfeed.txt";
    let header = build_content_disposition(file_name);

    assert!(
        !header.contains('\n'),
        "legacy filename must not contain raw newline: {}",
        header
    );
    assert!(
        header.contains("filename=\"linefeed.txt\""),
        "legacy filename must strip newline: {}",
        header
    );
    assert!(
        header.contains("filename*=UTF-8''line%0Afeed.txt"),
        "RFC 5987 filename* must percent-encode newline: {}",
        header
    );
}

#[test]
fn public_share_download_header_carriage_return_sanitized() {
    let file_name = "car\rriage.txt";
    let header = build_content_disposition(file_name);

    assert!(
        !header.contains('\r'),
        "legacy filename must not contain raw carriage return: {}",
        header
    );
    assert!(
        header.contains("filename=\"carriage.txt\""),
        "legacy filename must strip carriage return: {}",
        header
    );
    assert!(
        header.contains("filename*=UTF-8''car%0Driage.txt"),
        "RFC 5987 filename* must percent-encode carriage return: {}",
        header
    );
}

#[test]
fn public_share_download_header_backslash_escaped() {
    let file_name = "path\\to\\file.txt";
    let header = build_content_disposition(file_name);

    assert!(
        header.contains("filename=\"path\\\\to\\\\file.txt\""),
        "legacy filename parameter must escape backslashes: {}",
        header
    );
    assert!(
        header.contains("filename*=UTF-8''path%5Cto%5Cfile.txt"),
        "RFC 5987 filename* must percent-encode backslashes: {}",
        header
    );
}

#[test]
fn public_share_download_header_control_chars_sanitized() {
    let file_name = "foo\x01bar\x7fbaz.txt";
    let header = build_content_disposition(file_name);

    assert!(
        !header.bytes().any(|b| b < 0x20 || b == 0x7f),
        "legacy filename must not contain control characters: {}",
        header
    );
    assert!(
        header.contains("filename=\"foobarbaz.txt\""),
        "legacy filename must strip all control characters: {}",
        header
    );
    assert!(
        header.contains("filename*=UTF-8''foo%01bar%7Fbaz.txt"),
        "RFC 5987 filename* must percent-encode control characters: {}",
        header
    );
}

#[test]
fn public_share_download_header_unicode_preserved() {
    let file_name = "résumé.pdf";
    let header = build_content_disposition(file_name);

    assert!(
        header.contains("filename=\"résumé.pdf\""),
        "legacy filename must preserve unicode: {}",
        header
    );
    assert!(
        header.contains("filename*=UTF-8''r%C3%A9sum%C3%A9.pdf"),
        "RFC 5987 filename* must percent-encode unicode: {}",
        header
    );
}
