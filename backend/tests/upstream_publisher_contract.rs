//! Test-only inventory for the future Option A NIP-29 publisher.
//!
//! These tests validate only deterministic contract fixtures. Every publisher
//! behavior below remains NOT IMPLEMENTED; passing this inventory is not
//! evidence that signing, persistence, retries, or publishing work.

use sha2::{Digest, Sha256};

const CANONICAL_REQUEST: &str = r#"{"attachments":[],"channel":"00000000-0000-4000-8000-000000000003","content":"contract fixture","intent":"channel_message","kind":9,"principal":"00000000-0000-4000-8000-000000000001","tenant":"00000000-0000-4000-8000-000000000002","workspace":"00000000-0000-4000-8000-000000000004"}"#;

const NOT_IMPLEMENTED: &[&str] = &[
    "current-principal-tenant-workspace-chat-entitlement",
    "ready-identity-key-generation-and-relay-mapping-generation",
    "typed-kind-9-intent-with-trusted-channel-binding",
    "deny-guest-read-only-revoked-and-cross-tenant",
    "atomic-scoped-idempotency-and-canonical-request-digest",
    "encrypted-immutable-preparation-and-signed-byte-retention",
    "crash-recovery-before-sign-after-sign-before-commit-and-after-commit",
    "lost-or-ambiguous-ack-exact-event-id-and-byte-reuse",
    "timestamp-expiry-and-stale-authorization-generation-denial",
    "relay-accepted-history-observed-and-recipient-delivered-are-distinct",
    "database-kms-failure-and-secret-body-audit-redaction",
];

#[test]
fn canonical_request_fixture_has_a_stable_sha256_digest() {
    let digest = Sha256::digest(CANONICAL_REQUEST.as_bytes());
    assert_eq!(
        hex::encode(digest),
        "f909fe0d3fca7fce5e794b3475c9aa0f14bfbc4bd14ec1541bf292accdc2d792"
    );
}

#[test]
fn every_future_publisher_behavior_is_explicitly_not_implemented() {
    assert_eq!(NOT_IMPLEMENTED.len(), 11);
    let mut cases = NOT_IMPLEMENTED.to_vec();
    cases.sort_unstable();
    cases.dedup();
    assert_eq!(cases.len(), NOT_IMPLEMENTED.len());
    assert!(NOT_IMPLEMENTED.iter().all(|case| !case.is_empty()));
}
