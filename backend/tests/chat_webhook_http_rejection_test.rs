//! Integration test: HTTP webhook URLs are rejected (Task A3).
//!
//! Verifies that only HTTPS URLs are accepted for chat webhooks unless HTTP is
//! explicitly allowed for local development.

use rustshare_server::handlers::chat_integration::is_valid_chat_webhook_url;

#[test]
fn chat_webhook_http_rejection() {
    assert!(
        !is_valid_chat_webhook_url("http://example.com/webhook", false),
        "HTTP webhook URLs must be rejected in production"
    );
    assert!(
        is_valid_chat_webhook_url("https://example.com/webhook", false),
        "HTTPS webhook URLs must be accepted"
    );
    assert!(
        is_valid_chat_webhook_url("http://example.com/webhook", true),
        "HTTP webhook URLs may be allowed when explicitly enabled"
    );
    assert!(
        !is_valid_chat_webhook_url("ftp://example.com/webhook", false),
        "Non-HTTP(S) schemes must be rejected"
    );
    assert!(
        !is_valid_chat_webhook_url("not-a-url", false),
        "Malformed URLs must be rejected"
    );
}
