# Desktop Device Pairing Default Auth Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make device pairing the default `rustshare-desktop login` flow, using a 5-minute approval link that must be opened from an authenticated RustShare web session.

**Architecture:** Extend the backend device-pairing contract to return a complete approval URL and accept approval by `device_code`, add a dedicated frontend `/device/approve` route for browser approval, and rewire the desktop CLI to use pairing-first auth with one shared token storage path.

**Tech Stack:** Rust, Axum, SQLx, SvelteKit, Vitest, Cargo, macOS Keychain via `keyring`

---

### Task 1: Freeze the backend pairing contract with tests

**Files:**
- Modify: `backend/server/src/handlers/device_auth.rs`
- Test: `backend/server/src/handlers/device_auth.rs`
- Reference: `backend/server/src/main.rs`

**Step 1: Write the failing backend tests**

Add tests in `backend/server/src/handlers/device_auth.rs` for:

- `device_request` returns `verification_uri`
- `device_request` returns `verification_uri_complete`
- approval request rejects payloads with neither `user_code` nor `device_code`
- approval request rejects payloads with both `user_code` and `device_code`

Suggested test skeleton:

```rust
#[tokio::test]
async fn device_request_includes_verification_uri_fields() {
    // call handler and assert response fields are present
}

#[test]
fn approve_request_requires_exactly_one_identifier() {
    // deserialize/validate request and assert error conditions
}
```

**Step 2: Run the targeted test command to verify failure**

Run:

```bash
cargo test -p rustshare-server device_request_includes_verification_uri_fields -- --nocapture
```

Expected:

- FAIL because the response struct does not yet include the new URI fields

**Step 3: Implement the response contract**

Update the request response struct to include:

```rust
pub struct DeviceRequestResponse {
    pub user_code: String,
    pub device_code: String,
    pub expires_in: i64,
    pub verification_uri: String,
    pub verification_uri_complete: String,
}
```

Build the URIs from the instance URL and the frontend approval path:

```rust
let verification_uri = format!("{}/device/approve", instance_url);
let verification_uri_complete =
    format!("{}/device/approve?device_code={}", instance_url, device_code);
```

**Step 4: Re-run the targeted tests**

Run:

```bash
cargo test -p rustshare-server device_request_includes_verification_uri_fields -- --nocapture
```

Expected:

- PASS

**Step 5: Commit**

```bash
git add backend/server/src/handlers/device_auth.rs
git commit -m "test: lock device pairing request contract"
```

### Task 2: Add link-based approval on the backend

**Files:**
- Modify: `backend/server/src/handlers/device_auth.rs`
- Reference: `backend/server/src/main.rs`
- Test: `backend/server/src/handlers/device_auth.rs`

**Step 1: Write the failing test for approval by `device_code`**

Add a handler-level test that verifies approval succeeds when `device_code` is provided for a valid pending request.

Suggested request model:

```rust
#[derive(Deserialize)]
pub struct DeviceApproveRequest {
    pub user_code: Option<String>,
    pub device_code: Option<String>,
}
```

Validation helper:

```rust
fn approval_lookup_mode(req: &DeviceApproveRequest) -> Result<ApprovalLookupMode, ApiError> {
    // exactly one of user_code or device_code
}
```

**Step 2: Run the targeted test to verify failure**

Run:

```bash
cargo test -p rustshare-server approve_request_requires_exactly_one_identifier -- --nocapture
```

Expected:

- FAIL because the request model only supports `user_code`

**Step 3: Implement the approval-path expansion**

Update `device_approve` so it:

- accepts either `user_code` or `device_code`
- rejects both/none
- looks up the correct pending request
- preserves existing expiry and already-approved behavior

Do not issue the final device token in `device_approve`; keep token issuance in `device_poll`.

**Step 4: Run targeted tests**

Run:

```bash
cargo test -p rustshare-server device_auth -- --nocapture
```

Expected:

- PASS for the updated handler tests

**Step 5: Commit**

```bash
git add backend/server/src/handlers/device_auth.rs
git commit -m "feat: support device approval by pairing link"
```

### Task 3: Add backend contract coverage for expiry and one-time use

**Files:**
- Modify: `backend/server/src/handlers/device_auth.rs`
- Test: `backend/tests/contracts/device_pairing_contract.rs`

**Step 1: Write failing contract coverage**

Add or extend contract tests for:

- expired `device_code` cannot be approved
- successful approval still issues token only from poll
- pair request is deleted after approved poll completes

Suggested contract-level assertions:

```rust
assert_eq!(poll_response.status, "approved");
assert!(token.len() > 20);
assert!(pair_request_deleted);
```

**Step 2: Run the relevant contract test**

Run:

```bash
cargo test -p rustshare-server device_pairing_contract -- --ignored --nocapture
```

Expected:

- FAIL or remain incomplete until the contract logic is aligned

**Step 3: Adjust backend implementation only as needed**

Keep changes minimal:

- preserve current poll behavior
- ensure cleanup still happens
- ensure link approval cannot resurrect expired requests

**Step 4: Re-run the contract test**

Run:

```bash
cargo test -p rustshare-server device_pairing_contract -- --ignored --nocapture
```

Expected:

- PASS or at minimum targeted coverage passes in the updated environment

**Step 5: Commit**

```bash
git add backend/server/src/handlers/device_auth.rs backend/tests/contracts/device_pairing_contract.rs
git commit -m "test: cover pairing link expiry and completion"
```

### Task 4: Add the frontend approval-link route

**Files:**
- Create: `frontend/src/routes/device/approve/+page.svelte`
- Create: `frontend/src/routes/device/approve/__tests__/page.test.ts`
- Modify: `frontend/src/lib/api/auth.ts`
- Reference: `frontend/src/routes/login/+page.svelte`

**Step 1: Write the failing frontend tests**

Add tests for:

- missing `device_code` shows invalid-link state
- authenticated user can submit approval
- expired approval shows a clear error

Suggested test shape:

```ts
it('shows invalid state when device_code is missing', async () => {
  // render page and assert message
});

it('submits device_code approval for authenticated users', async () => {
  // mock approve API, click approve, assert payload
});
```

**Step 2: Run the targeted frontend test**

Run:

```bash
cd frontend && npm test -- src/routes/device/approve/__tests__/page.test.ts
```

Expected:

- FAIL because the route does not exist yet

**Step 3: Implement the approval page**

The page should:

- read `device_code` from the query string
- redirect unauthenticated users to `/login` with a return target
- present an approval CTA when the query is valid
- call `approveDevicePairingByDeviceCode(deviceCode)`
- show success, invalid, and expired states

Add a dedicated client helper:

```ts
export async function approveDevicePairingByDeviceCode(device_code: string) {
  return apiClient.post<{ device_name: string }>('/auth/device/approve', { device_code });
}
```

**Step 4: Run the targeted test**

Run:

```bash
cd frontend && npm test -- src/routes/device/approve/__tests__/page.test.ts
```

Expected:

- PASS

**Step 5: Commit**

```bash
git add frontend/src/routes/device/approve/+page.svelte frontend/src/routes/device/approve/__tests__/page.test.ts frontend/src/lib/api/auth.ts
git commit -m "feat: add device approval link page"
```

### Task 5: Make the existing `/device` flow use the same approval-link contract

**Files:**
- Modify: `frontend/src/routes/device/+page.svelte`
- Modify: `frontend/src/lib/api/auth.ts`
- Test: `frontend/src/routes/device/approve/__tests__/page.test.ts`

**Step 1: Write the failing assertion**

Add or extend a test asserting generated URLs target:

- `/device/approve?device_code=...`

and not a route that does not exist.

**Step 2: Run the targeted test**

Run:

```bash
cd frontend && npm test -- src/routes/device/approve/__tests__/page.test.ts
```

Expected:

- FAIL if the page still depends on inconsistent route semantics

**Step 3: Align the route contract**

Update the existing device page to:

- consume `verification_uri_complete` if available
- stop rebuilding the URL client-side if the backend already returns it
- keep the visual countdown consistent with the 5-minute TTL

**Step 4: Run the targeted test**

Run:

```bash
cd frontend && npm test -- src/routes/device/approve/__tests__/page.test.ts
```

Expected:

- PASS

**Step 5: Commit**

```bash
git add frontend/src/routes/device/+page.svelte frontend/src/lib/api/auth.ts
git commit -m "refactor: align device pairing links across web flows"
```

### Task 6: Rewire desktop auth so pairing is the default login path

**Files:**
- Modify: `apps/desktop/src/main.rs`
- Modify: `apps/desktop/src/lib.rs`
- Modify: `apps/desktop/src/api/mod.rs`
- Modify: `apps/desktop/src/api/auth.rs`
- Modify: `apps/desktop/src/config.rs`
- Test: `apps/desktop/src/api/auth.rs`

**Step 1: Write the failing desktop tests**

Add tests for:

- approval instructions include `verification_uri_complete`
- instructions mention 5-minute validity
- instructions tell the user to open the link from an authenticated web UI session

Suggested helper extraction:

```rust
fn pairing_instructions(url: &str, expires_in: i64) -> String {
    format!(
        "Approve this device in RustShare:\n{}\n\nThis approval link is valid for {} minutes.\nOpen it from a browser where you are already signed in to RustShare.\n",
        url,
        expires_in / 60
    )
}
```

**Step 2: Run the desktop targeted test**

Run:

```bash
cargo test -p rustshare-desktop pairing_instructions -- --nocapture
```

Expected:

- FAIL because the helper and flow do not exist yet

**Step 3: Implement the CLI behavior**

Change the CLI shape from:

```rust
Login { token: String }
```

to something like:

```rust
Login {
    #[arg(long)]
    token: Option<String>,
    #[arg(long, default_value_t = false)]
    open_browser: bool,
}
```

Behavior:

- if `--token` is passed, keep legacy admin/debug login
- otherwise run pairing
- print the approval URL and instructions
- optionally call `open <url>` on macOS when `--open-browser` is set or by default if desired
- poll until approval

Also expose the desktop API modules by uncommenting the relevant exports in `apps/desktop/src/lib.rs`.

**Step 4: Run desktop tests**

Run:

```bash
cargo test -p rustshare-desktop -- --nocapture
```

Expected:

- PASS

**Step 5: Commit**

```bash
git add apps/desktop/src/main.rs apps/desktop/src/lib.rs apps/desktop/src/api/mod.rs apps/desktop/src/api/auth.rs apps/desktop/src/config.rs
git commit -m "feat: make desktop login pairing-first"
```

### Task 7: Unify desktop token storage

**Files:**
- Modify: `apps/desktop/src/main.rs`
- Modify: `apps/desktop/src/api/auth.rs`
- Modify: `crates/platform/src/lib.rs`
- Test: `apps/desktop/src/api/auth.rs`

**Step 1: Write the failing storage-path test**

Add a test or helper-level assertion ensuring both login and daemon/status commands resolve credentials from the same abstraction.

Suggested helper:

```rust
fn desktop_token_store() -> TokenStore {
    TokenStore::new("rustshare")
}
```

**Step 2: Run the targeted test**

Run:

```bash
cargo test -p rustshare-desktop desktop_token_store -- --nocapture
```

Expected:

- FAIL because storage remains split between `platform::TokenStore` and local keyring code

**Step 3: Implement the unification**

Pick one storage path and use it everywhere.

Recommendation:

- keep `platform::TokenStore`
- stop storing device auth separately under `rustshare-desktop/device_token`
- persist tokens keyed by stable device ID through the same storage path already consumed by the daemon

**Step 4: Run the desktop tests**

Run:

```bash
cargo test -p rustshare-desktop -- --nocapture
```

Expected:

- PASS

**Step 5: Commit**

```bash
git add apps/desktop/src/main.rs apps/desktop/src/api/auth.rs crates/platform/src/lib.rs
git commit -m "refactor: unify desktop token storage"
```

### Task 8: Update the macOS installation documentation to the new auth flow

**Files:**
- Modify: `apps/desktop/docs/distribution/macos-client-installation.md`
- Modify: `apps/desktop/docs/distribution/build-and-package.md`

**Step 1: Write the doc delta**

Replace token-creation instructions with:

- pairing-first login
- approval link explanation
- 5-minute validity warning
- requirement to open the link from an authenticated web UI session
- explicit note that users do not manually generate tokens in the normal flow

Include an admin/debug note for `login --token`.

**Step 2: Sanity-check the docs**

Run:

```bash
sed -n '1,260p' apps/desktop/docs/distribution/macos-client-installation.md
```

Expected:

- the auth instructions match the implemented CLI behavior

**Step 3: Commit**

```bash
git add apps/desktop/docs/distribution/macos-client-installation.md apps/desktop/docs/distribution/build-and-package.md
git commit -m "docs: document pairing-first desktop login"
```

### Task 9: Run end-to-end verification across backend, frontend, and desktop

**Files:**
- Verify only

**Step 1: Run backend tests**

```bash
cargo test -p rustshare-server device_auth -- --nocapture
```

Expected:

- PASS

**Step 2: Run desktop tests**

```bash
cargo test -p rustshare-desktop -- --nocapture
```

Expected:

- PASS

**Step 3: Run frontend tests**

```bash
cd frontend && npm test -- src/routes/device/approve/__tests__/page.test.ts
```

Expected:

- PASS

**Step 4: Run frontend type checks**

```bash
cd frontend && npm run check
```

Expected:

- PASS

**Step 5: Manual smoke test**

Run:

```bash
cargo run -p rustshare-desktop -- --server http://localhost:8080 login
```

Expected:

- CLI prints the approval URL
- CLI says it is valid for 5 minutes
- CLI says to open it from an authenticated RustShare web UI session
- after approval in browser, the CLI reports success

**Step 6: Commit the final verification pass**

```bash
git add .
git commit -m "chore: verify pairing-first desktop auth flow"
```
