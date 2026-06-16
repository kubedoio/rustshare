//! Integration test: chat integration admin handlers require `AdminUser` (Task A5).
//!
//! This compile-time test verifies that the admin chat-integration handlers are
//! typed to accept the `AdminUser` extractor. Runtime 403/200 behavior is
//! enforced by the `AdminUser` extractor itself and is covered by the
//! `admin_require_admin_test` DB integration tests.

use std::future::Future;

use axum::{extract::State, Json};
use rustshare_server::{
    handlers::{
        chat_integration::{list_chat_webhooks, register_chat_webhook, RegisterWebhookRequest},
        extractors::AdminUser,
    },
    AppState,
};

#[test]
fn chat_integration_admin_authorization() {
    fn assert_list_requires_admin<H, Fut>(_handler: H)
    where
        H: Fn(State<AppState>, AdminUser) -> Fut,
        Fut: Future,
    {
    }
    assert_list_requires_admin(list_chat_webhooks);

    fn assert_register_requires_admin<H, Fut>(_handler: H)
    where
        H: Fn(State<AppState>, AdminUser, Json<RegisterWebhookRequest>) -> Fut,
        Fut: Future,
    {
    }
    assert_register_requires_admin(register_chat_webhook);
}
