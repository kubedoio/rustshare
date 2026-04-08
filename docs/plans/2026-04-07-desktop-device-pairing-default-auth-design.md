# Desktop Device Pairing Default Auth Design

## Summary

RustShare Desktop should stop asking users to manually supply API tokens during setup.

The default login flow should be device pairing:

1. `rustshare-desktop login` creates a short-lived pairing request.
2. The CLI prints a full approval URL.
3. The CLI tells the user the URL is valid for 5 minutes.
4. The CLI tells the user to open the URL from an already authenticated RustShare web UI session.
5. The user approves the device in the web app.
6. The desktop polls for approval, receives the final device token, and stores it in Keychain.

This keeps the user experience aligned with modern device authorization flows while preserving explicit user approval and short-lived enrollment semantics.

## Goals

- Make device pairing the default desktop login mechanism.
- Eliminate the need for ordinary users to manually create or paste raw API tokens.
- Support a one-click approval URL that can be opened in the browser.
- Keep the approval link valid for 5 minutes by default.
- Require approval from an authenticated RustShare web UI session.
- Store the final device token in secure platform storage on macOS.

## Non-Goals

- Shipping a full GUI `.app` onboarding flow in this change.
- Reworking the sync engine authentication model beyond what is required for desktop pairing.
- Introducing OAuth device flow compatibility with external identity providers.
- Changing the existing manual Settings-based code approval flow unless needed for consistency.

## Current State

The repository already contains most of the necessary pieces, but they are not connected into one coherent flow.

### Backend

The backend already exposes:

- `POST /api/v1/auth/device/request`
- `POST /api/v1/auth/device/poll`
- `POST /api/v1/auth/device/approve`
- `GET /api/v1/auth/device/qr-info`

Relevant file:

- `backend/server/src/handlers/device_auth.rs`

The backend already stores pending pair requests with expiration and only issues the real device token after approval.

### Frontend

The frontend already contains:

- a device pairing page at `frontend/src/routes/device/+page.svelte`
- API helpers in `frontend/src/lib/api/auth.ts`
- a settings UI that can approve pairing by `user_code`

What is missing is a dedicated approval-link route that handles a desktop-originated URL.

### Desktop

The active desktop binary lives in:

- `apps/desktop/src/main.rs`

The current CLI still uses:

- `rustshare-desktop login <TOKEN>`

There is also an unused pairing implementation in:

- `apps/desktop/src/api/auth.rs`

That pairing module is not currently wired into the active CLI path, and the desktop auth storage is split between two approaches:

- `platform::TokenStore`
- the keyring logic in `apps/desktop/src/api/auth.rs`

## Product Decision

The desktop should use link-based device pairing as the default login path.

The experience should be:

```text
Approve this device in RustShare:
https://rustshare.example.com/device/approve?device_code=...

This approval link is valid for 5 minutes.
Open it from a browser where you are already signed in to RustShare.

Waiting for approval...
```

Optional browser auto-open is helpful, but printing the link is mandatory so the user can copy it to another browser or machine.

## Chosen Flow

### 1. Desktop requests pairing

The desktop calls `POST /api/v1/auth/device/request`.

The backend returns:

- `user_code`
- `device_code`
- `expires_in`
- `verification_uri`
- `verification_uri_complete`

Example:

```json
{
  "user_code": "ABCD1234",
  "device_code": "Q0xJRU5ULVNIT1JULUxJVkVELVRPS0VO",
  "expires_in": 300,
  "verification_uri": "https://rustshare.example.com/device/approve",
  "verification_uri_complete": "https://rustshare.example.com/device/approve?device_code=Q0xJRU5ULVNIT1JULUxJVkVELVRPS0VO"
}
```

`device_code` remains a short-lived pairing secret, not the final device token.

### 2. Desktop displays approval instructions

The CLI prints:

- the full approval link
- the 5-minute validity window
- a warning to open it from an authenticated RustShare web UI session

The CLI then polls `POST /api/v1/auth/device/poll`.

### 3. User opens approval link in browser

The browser lands on a new frontend route:

- `/device/approve?device_code=...`

This route must:

- require an authenticated session
- preserve the original approval URL across login redirect if necessary
- validate missing or obviously malformed query state
- show a dedicated approval screen
- call the backend approval endpoint

### 4. Web UI approves the pending pairing request

The backend should accept link-based approval by `device_code`.

There are two acceptable ways to model this:

1. Extend `POST /api/v1/auth/device/approve` to accept either `user_code` or `device_code`.
2. Add a dedicated `POST /api/v1/auth/device/approve-link`.

Recommendation:

- extend the existing `device_approve` contract to accept exactly one of `user_code` or `device_code`

Why:

- one conceptual approval action
- keeps the manual code-entry flow alive
- avoids multiplying endpoints for the same operation

### 5. Desktop receives the real device token

After approval, the existing poll endpoint returns:

- `status = approved`
- the final device token

The desktop stores that token in Keychain and uses it for daemon, status, and sync operations.

## Contract Changes

## Backend request response

`POST /api/v1/auth/device/request` should be extended to include:

- `verification_uri`
- `verification_uri_complete`

The backend should build these using instance URL logic rather than forcing the desktop to guess route structure.

## Backend approval request

Current shape:

```json
{ "user_code": "ABCD1234" }
```

Proposed shape:

```json
{ "user_code": "ABCD1234" }
```

or

```json
{ "device_code": "..." }
```

Validation rule:

- exactly one of `user_code` or `device_code` must be present

## UX Requirements

### Desktop CLI

`rustshare-desktop login` should:

- start pairing
- print the approval link
- print the expiration warning
- explain that the link must be opened in an authenticated RustShare browser session
- wait for approval
- confirm success when token is stored

### Web approval page

The `/device/approve` page should:

- explain what the user is approving
- require sign-in
- show if the link is expired or invalid
- confirm approval clearly
- avoid exposing the final token in the browser

### Manual fallback

The existing manual code-entry flow in Settings should remain available as a fallback for:

- browsers where deep-link copy/paste is inconvenient
- troubleshooting
- operator workflows

It is not the primary path anymore.

## Security Considerations

- The approval link is only a short-lived pairing token, not the final auth token.
- Default TTL remains 300 seconds.
- Approval requires an authenticated web session.
- Polling remains rate-limited.
- Pair requests are deleted after success or expiry.
- The final device token is only issued after successful user approval.
- Desktop stores the real token in Keychain, not plaintext config.

## Desktop Implementation Strategy

The desktop currently has duplicated auth paths. We should converge on one.

Recommendation:

- keep `platform::TokenStore` as the storage abstraction
- adapt the desktop pairing logic to use that abstraction
- update `apps/desktop/src/main.rs` to call the pairing flow by default
- retain a token-based fallback behind an explicit flag such as `login --token`

This is lower risk than carrying two different keyring conventions forward.

## Frontend Implementation Strategy

Add a dedicated route:

- `frontend/src/routes/device/approve/+page.svelte`

This route should:

- read `device_code` from query params
- check whether the user is authenticated
- redirect to login if needed
- present the approval UI
- approve via the backend
- display success, invalid, or expired states

The existing `/device` page should also be updated so any generated QR or deep-link URL points at this same approval route consistently.

## Backend Implementation Strategy

Update:

- `backend/server/src/handlers/device_auth.rs`

to:

- include verification URLs in the request response
- support approval by `device_code`
- preserve current approval-by-`user_code` behavior

No new table should be required. The existing `device_pair_requests` table is already the correct authority for pending enrollment state.

## Testing Strategy

### Backend

Add tests for:

- request response includes verification URIs
- approval rejects requests with neither field
- approval rejects requests with both fields
- approval by `device_code` succeeds for valid pending request
- expired request cannot be approved by link

### Frontend

Add tests for:

- `/device/approve` renders invalid state without `device_code`
- authenticated approval submits `device_code`
- unauthenticated users are redirected through login
- expired approval shows a retry-friendly message

### Desktop

Add tests for:

- `login` default path triggers pairing flow rather than raw token input
- approval instructions include full URL
- output mentions 5-minute validity
- output tells the user to open the link from an authenticated web UI session
- approved token is stored through the shared token storage path

## Rollout Notes

- Keep `login --token` for admin/debug use during transition.
- Update macOS installation docs only after the CLI behavior is implemented.
- If the web approval route lands before the desktop CLI change, it is still safe because it only adds a new approval surface.

## Result

After this change, RustShare Desktop onboarding becomes:

- simpler for end users
- more secure than manual token handling
- better aligned with the backend and frontend pairing model that already exists in the repository

This change should be treated as the canonical desktop authentication path for Phase 1.
