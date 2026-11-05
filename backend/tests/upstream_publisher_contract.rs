//! Test-only inventory for the future Option A NIP-29 publisher.
//!
//! These tests validate deterministic contract fixtures only. Every publisher
//! behavior remains NOT IMPLEMENTED; passing tests do not prove signing,
//! persistence, retries, or publishing work.

use serde::Serialize;
use sha2::{Digest, Sha256};

const CANONICAL_REQUEST: &str = r#"{"attachments":[],"channel":"00000000-0000-4000-8000-000000000003","content":"contract fixture","intent":"channel_message","kind":9,"principal":"00000000-0000-4000-8000-000000000001","tenant":"00000000-0000-4000-8000-000000000002","workspace":"00000000-0000-4000-8000-000000000004"}"#;

#[derive(Clone, Serialize)]
struct RequestFixture {
    attachments: Vec<&'static str>,
    channel: &'static str,
    content: &'static str,
    intent: &'static str,
    kind: u16,
    principal: &'static str,
    tenant: &'static str,
    workspace: &'static str,
}

fn request_fixture() -> RequestFixture {
    RequestFixture {
        attachments: Vec::new(),
        channel: "00000000-0000-4000-8000-000000000003",
        content: "contract fixture",
        intent: "channel_message",
        kind: 9,
        principal: "00000000-0000-4000-8000-000000000001",
        tenant: "00000000-0000-4000-8000-000000000002",
        workspace: "00000000-0000-4000-8000-000000000004",
    }
}

fn digest(request: &RequestFixture) -> String {
    hex::encode(Sha256::digest(
        serde_json::to_vec(request).expect("fixture serialization succeeds"),
    ))
}

#[test]
fn canonical_request_fixture_is_stable_and_scope_bound() {
    let request = request_fixture();
    assert_eq!(serde_json::to_string(&request).unwrap(), CANONICAL_REQUEST);
    let expected_digest = digest(&request);
    assert_eq!(
        expected_digest,
        "f909fe0d3fca7fce5e794b3475c9aa0f14bfbc4bd14ec1541bf292accdc2d792"
    );
    assert_eq!(expected_digest, digest(&request));

    let mut variants = Vec::new();
    let mut changed = request.clone();
    changed.tenant = "00000000-0000-4000-8000-000000000005";
    variants.push(changed);
    let mut changed = request.clone();
    changed.workspace = "00000000-0000-4000-8000-000000000006";
    variants.push(changed);
    let mut changed = request.clone();
    changed.principal = "00000000-0000-4000-8000-000000000007";
    variants.push(changed);
    let mut changed = request.clone();
    changed.channel = "00000000-0000-4000-8000-000000000008";
    variants.push(changed);
    let mut changed = request.clone();
    changed.content = "different content";
    variants.push(changed);
    let mut changed = request.clone();
    changed.intent = "private_channel_dm";
    variants.push(changed);
    let mut changed = request.clone();
    changed.kind = 10;
    variants.push(changed);
    let mut changed = request.clone();
    changed.attachments = vec!["00000000-0000-4000-8000-000000000009"];
    variants.push(changed);
    assert!(variants
        .iter()
        .all(|variant| digest(variant) != expected_digest));
}

#[derive(Clone, Eq, PartialEq)]
struct IdempotencyScope {
    tenant: &'static str,
    workspace: &'static str,
    principal: &'static str,
    identity: &'static str,
    identity_generation: i64,
    mapping: &'static str,
    mapping_generation: i64,
    channel: &'static str,
    key: &'static str,
}

fn idempotency_scope() -> IdempotencyScope {
    IdempotencyScope {
        tenant: "00000000-0000-4000-8000-000000000002",
        workspace: "00000000-0000-4000-8000-000000000004",
        principal: "00000000-0000-4000-8000-000000000001",
        identity: "00000000-0000-4000-8000-000000000010",
        identity_generation: 3,
        mapping: "00000000-0000-4000-8000-000000000011",
        mapping_generation: 7,
        channel: "00000000-0000-4000-8000-000000000003",
        key: "request-1",
    }
}

#[test]
fn idempotency_scope_fixture_binds_subject_identity_mapping_and_channel() {
    let scope = idempotency_scope();
    let mut variants = Vec::new();
    let mut changed = scope.clone();
    changed.tenant = "00000000-0000-4000-8000-000000000005";
    variants.push(changed);
    let mut changed = scope.clone();
    changed.workspace = "00000000-0000-4000-8000-000000000006";
    variants.push(changed);
    let mut changed = scope.clone();
    changed.principal = "00000000-0000-4000-8000-000000000007";
    variants.push(changed);
    let mut changed = scope.clone();
    changed.identity = "00000000-0000-4000-8000-000000000012";
    variants.push(changed);
    let mut changed = scope.clone();
    changed.identity_generation += 1;
    variants.push(changed);
    let mut changed = scope.clone();
    changed.mapping = "00000000-0000-4000-8000-000000000013";
    variants.push(changed);
    let mut changed = scope.clone();
    changed.mapping_generation += 1;
    variants.push(changed);
    let mut changed = scope.clone();
    changed.channel = "00000000-0000-4000-8000-000000000008";
    variants.push(changed);
    let mut changed = scope.clone();
    changed.key = "request-2";
    variants.push(changed);
    assert!(variants.iter().all(|variant| variant != &scope));
}

const NOT_IMPLEMENTED: &[&str] = &[
    "current-principal-tenant-workspace-chat-entitlement",
    "ready-identity-key-generation-and-relay-mapping-generation",
    "typed-kind-9-intent-with-trusted-channel-binding",
    "deny-guest-read-only-revoked-wrong-channel-and-cross-tenant",
    "atomic-scoped-idempotency-and-canonical-request-digest",
    "encrypted-immutable-preparation-and-signed-byte-retention",
    "crash-recovery-before-sign-after-sign-before-commit-and-after-commit",
    "lost-or-ambiguous-ack-exact-event-id-and-byte-reuse",
    "timestamp-expiry-and-stale-authorization-generation-denial",
    "relay-accepted-history-observed-and-recipient-delivered-are-distinct",
    "database-kms-failure-and-secret-body-audit-redaction",
];

#[test]
fn publisher_contract_inventory_is_explicitly_not_implemented() {
    assert_eq!(NOT_IMPLEMENTED.len(), 11);
    let mut cases = NOT_IMPLEMENTED.to_vec();
    cases.sort_unstable();
    cases.dedup();
    assert_eq!(cases.len(), NOT_IMPLEMENTED.len());
    assert!(NOT_IMPLEMENTED.iter().all(|case| !case.is_empty()));
}
