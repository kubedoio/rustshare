//! Integration test: HTTP webhook URLs are rejected (Task A3).
//!
//! Verifies that only HTTPS URLs are accepted for chat webhooks unless HTTP is
//! explicitly allowed for local development.

use rustshare_core::services::validate_chat_webhook_url;

#[tokio::test]
async fn chat_webhook_http_rejection() {
    assert!(
        validate_chat_webhook_url("http://1.1.1.1/webhook", false)
            .await
            .is_err(),
        "HTTP webhook URLs must be rejected in production"
    );
    assert!(
        validate_chat_webhook_url("https://1.1.1.1/webhook", false)
            .await
            .is_ok(),
        "HTTPS webhook URLs must be accepted"
    );
    assert!(
        validate_chat_webhook_url("http://1.1.1.1/webhook", true)
            .await
            .is_ok(),
        "HTTP webhook URLs may be allowed when explicitly enabled"
    );
    assert!(
        validate_chat_webhook_url("ftp://1.1.1.1/webhook", false)
            .await
            .is_err(),
        "Non-HTTP(S) schemes must be rejected"
    );
    assert!(
        validate_chat_webhook_url("not-a-url", false).await.is_err(),
        "Malformed URLs must be rejected"
    );
}
