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
