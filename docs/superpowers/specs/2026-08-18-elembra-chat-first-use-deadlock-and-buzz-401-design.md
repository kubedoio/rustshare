# Elembra Chat first-use deadlock + Buzz 401 hardening

## Problem statement

Two independent regressions in the Elembra Chat alpha deployment:

1. **First-use deadlock (critical).** `GET /api/v1/applications/chat/status` only returns an active `mapping` when the caller also has an active `binding`. In a fresh deployment an admin enables Chat, auto-provisioning creates the workspace/community mapping, but a normal user has no binding yet. The status endpoint therefore reports `mapping: null`, the UI shows "Chat is being configured for this workspace", and the `BindingPanel` (the only path to create a binding) is never rendered.

2. **Buzz bootstrap 401.** The alpha/dogfood deployment can reach the Buzz relay but `GET /api/v1/relay/community` returns `401 Unauthorized`. This happens when the trusted-service identity used by Elembra (`RUSTSHARE_CHAT_BRIDGE_SECRET_KEY`) does not match the identity the relay trusts (`RELAY_TRUSTED_SERVICE_PUBKEYS` / `BUZZ_RELAY_OWNER_PUBKEY`). Current alpha compose makes this easy to misconfigure because `RUSTSHARE_CHAT_BRIDGE_SECRET_KEY` can silently diverge from `BUZZ_SERVICE_SK`, and there is no validation that the configured keys represent one keypair.

## Root causes

1. In `backend/server/src/handlers/chat_app.rs`, `mapping_info` is built inside `if let (Some(mapping), Some(binding)) = (&mapping, &binding)`. No binding means `mapping_info` stays `None`, even when an active mapping exists.
2. `docker-compose.alpha.yml` sets:
   ```yaml
   RUSTSHARE_CHAT_BRIDGE_SECRET_KEY: ${RUSTSHARE_CHAT_BRIDGE_SECRET_KEY:-${BUZZ_SERVICE_SK:-}}
   ```
   A stale explicit value of `RUSTSHARE_CHAT_BRIDGE_SECRET_KEY` wins over a regenerated `BUZZ_SERVICE_SK` without warning. The relay's `RELAY_TRUSTED_SERVICE_PUBKEYS` is driven from `BUZZ_RELAY_OWNER_PUBKEY`, which must correspond to the bridge secret — but nothing validates that correspondence.
3. The admin Chat page (`frontend/src/routes/admin/applications/chat/+page.svelte`) always renders "Set up automatically", encouraging re-provisioning when a mapping already exists.
4. Bootstrap failures surface generic strings (`relay discovery failed: Unauthorized`) rather than operator-actionable diagnostics.

## Design

### 1. Backend status semantics

Refactor `chat_status` in `backend/server/src/handlers/chat_app.rs` so:

- `mapping_info` is built directly from `mapping` when `mapping.active` is true, independent of `binding`.
- `binding` is mapped independently.
- `admission_active` requires active mapping + active binding + active admission.
- Inactive mappings remain hidden (existence-hiding).
- Tenant/workspace isolation and all other security guarantees are unchanged.

Conceptual shape:
```rust
let mapping_info = mapping
    .as_ref()
    .filter(|m| m.active)
    .map(|m| CommunityMappingInfo { community_id: m.community_id.clone(), relay_url: m.relay_url.clone() });

let mut admission_active = false;
if let (Some(mapping), Some(binding)) = (&mapping, &binding) {
    if mapping.active && binding.status == BindingStatus::Active {
        admission_active = chat_identity
            .active_admission(...)
            .await?;
    }
}
```

### 2. Backend regression tests

Extend `backend/tests/chat_app_read_test.rs` with a new test for the exact first-use state:

- Chat enabled
- active mapping exists
- no binding
- no admission

Expected: `chat_enabled == true`, `mapping != None`, `binding == None`, `admission_active == false`.

Also assert the existing/security states remain correct:

1. no mapping → `mapping == None`
2. inactive mapping → `mapping == None`
3. active mapping + active binding but no admission → mapping visible, binding visible, `admission_active == false`
4. fully configured → mapping visible, binding visible, `admission_active == true`
5. tenant B must never see tenant A's mapping

### 3. Frontend regression tests

Strengthen `frontend/src/lib/components/chat/ChatApplicationView.test.ts` with two explicit, distinct states:

- Workspace genuinely has no mapping: expects "Chat is being configured for this workspace."
- Workspace has mapping but user is not bound: expects `BindingPanel` / "Set up Chat".

The second state must never display the configuring message.

### 4. Admin Chat UX

Update `frontend/src/routes/admin/applications/chat/+page.svelte`:

- No mapping → offer "Set up automatically" and "Connect existing Chat deployment".
- Active mapping exists → show "Connected ✓" with diagnostic fields, hide/disable the automatic provisioning button.
- If a separate diagnostics/re-verification action is useful, name it "Verify relay connection".
- Never silently overwrite an existing mapping.
- Preserve ADR-0036 conflict semantics.

Add/update tests in `frontend/src/routes/admin/applications/chat/page.test.ts`.

### 5. Actionable 401 diagnostics

When bootstrap or relay verification receives `401 Unauthorized`, surface an operator-useful message that does not leak secrets, private keys, or raw auth headers. Example copy:

> Buzz rejected Elembra's service identity. Verify that the public key corresponding to `RUSTSHARE_CHAT_BRIDGE_SECRET_KEY` / `BUZZ_SERVICE_SK` is configured in the relay's `RELAY_TRUSTED_SERVICE_PUBKEYS`, then recreate the backend and relay containers.

Backend logs should include safe context to distinguish:

- relay unreachable
- 401 service identity rejected
- invalid relay response/signature
- stale response
- mapping conflict

Map `BuzzAuthorityError::Unauthorized` to a dedicated diagnostic category in `ChatBootstrapError` and propagate it through the admin provision endpoint.

### 6. Alpha configuration validation

Create `frontend/scripts/alpha-validate-buzz-config.mjs` (Node) using the existing `@noble/curves/secp256k1.js` dependency:

1. Validate all relevant values are 64 lowercase hex.
2. Derive the x-only public key from `BUZZ_SERVICE_SK`.
3. Check it equals `BUZZ_RELAY_OWNER_PUBKEY`.
4. Check `RUSTSHARE_CHAT_BRIDGE_SECRET_KEY == BUZZ_SERVICE_SK` when both are supplied.
5. Derive the relay public key from `BUZZ_RELAY_PRIVATE_KEY`.
6. Optionally verify `BUZZ_RELAY_PUBKEY` if present.
7. Fail with clear actionable messages; never print private secrets.

Wire it into `scripts/pre-flight.sh` so the alpha/dogfood validation path runs it when the Buzz variables are present. Wire it into `scripts/run-alpha-dogfood.sh` as a pre-check.

### 7. Docker Compose secret source

Change `docker-compose.alpha.yml` backend environment so that the alpha deployment has exactly one canonical bridge service secret source:

```yaml
backend:
  environment:
    RUSTSHARE_CHAT_BRIDGE_SECRET_KEY: ${BUZZ_SERVICE_SK:?BUZZ_SERVICE_SK must be set (run node frontend/scripts/alpha-gen-buzz-keys.mjs)}
```

This makes a stale explicit `RUSTSHARE_CHAT_BRIDGE_SECRET_KEY` impossible in the alpha override; production/manual deployments continue using the generic backend variable unchanged outside alpha.

### 8. Documentation

Update `docs/runbooks/elembra-alpha.md` to:

- Distinguish the five states: Application enabled, Community mapped, User identity bound, Admission active, Relay trusted-service authentication healthy.
- Explain that `Chat enabled` is not equivalent to `Chat ready`.
- Document the new validation script and the single canonical alpha bridge secret source.
- Add the 401 diagnostic to the common-failure table.

## Testing plan

1. Add backend regression test; watch it fail against current `chat_status`.
2. Fix `chat_status`; watch the regression test pass; run the full `chat_app_read_test` suite.
3. Add frontend `ChatApplicationView` regression tests; verify current component already passes or adjust.
4. Add/update admin Chat page tests for the connected-state UX.
5. Add Node validation script tests if a test harness exists; otherwise test manually with valid/invalid `.env` snippets.
6. Run backend formatting/lint/unit tests.
7. Run frontend typecheck, lint, and tests.
8. If the full dogfood environment is available, run `scripts/run-alpha-dogfood.sh` and verify the first-use state machine end to end.

## Files expected to change

- `backend/server/src/handlers/chat_app.rs`
- `backend/tests/chat_app_read_test.rs`
- `frontend/src/lib/components/chat/ChatApplicationView.test.ts`
- `frontend/src/routes/admin/applications/chat/+page.svelte`
- `frontend/src/routes/admin/applications/chat/page.test.ts`
- `backend/server/src/services/chat_bootstrap.rs`
- `backend/server/src/handlers/admin/applications.rs`
- `frontend/scripts/alpha-validate-buzz-config.mjs` (new)
- `scripts/pre-flight.sh`
- `scripts/run-alpha-dogfood.sh`
- `docker-compose.alpha.yml`
- `docs/runbooks/elembra-alpha.md`

## Architecture preservation

- Buzz remains Chat source of truth.
- No Buzz private database reads.
- No Elembra Chat ACL tables.
- No NIP-98 bypass.
- `RELAY_TRUSTED_SERVICE_PUBKEYS` is not weakened.
- No automatic trust of unauthenticated discovery responses.
- No human Chat signing keys stored server-side.
- Mapping, binding, and admission remain separate concepts.
- Existing mappings are never silently overwritten.
- Fail closed on authorization failures.
- Files/Memory/Search/Ask security guarantees unchanged.
