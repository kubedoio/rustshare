# Mobile OIDC Contract

Date: 2026-03-20

## Purpose

Rustshare mobile clients use OIDC Authorization Code + PKCE without reusing the web cookie flow.
The backend remains the contract owner for allowed redirect URIs, user provisioning, and Rustshare
API token issuance.

## Endpoints

- `POST /api/v1/auth/oidc/mobile/authorize`
- `POST /api/v1/auth/oidc/mobile/exchange`

Legacy aliases also exist under `/api/auth/oidc/mobile/*`.

## Environment Variables

- `OIDC_ISSUER_URL`
- `OIDC_SCOPES`
- `OIDC_MOBILE_CLIENT_ID`
- `OIDC_MOBILE_CLIENT_SECRET` (optional for public clients)
- `OIDC_MOBILE_REDIRECT_URIS`

`OIDC_MOBILE_REDIRECT_URIS` accepts a comma-separated allowlist, for example:

```text
rustshare://auth/callback,https://mobile.example.com/callback
```

## Authorize Request

`POST /api/v1/auth/oidc/mobile/authorize`

```json
{
  "redirect_uri": "rustshare://auth/callback",
  "code_challenge": "<pkce challenge>",
  "state": "<opaque mobile state>",
  "nonce": "<opaque mobile nonce>"
}
```

Response:

```json
{
  "authorization_url": "https://issuer.example/authorize?... "
}
```

Backend rules:

- `redirect_uri` must exactly match one configured allowlist value.
- `redirect_uri` must parse as a valid URL.
- `code_challenge`, `state`, and `nonce` must be non-empty.

## Exchange Request

`POST /api/v1/auth/oidc/mobile/exchange`

```json
{
  "code": "<authorization code>",
  "code_verifier": "<pkce verifier>",
  "redirect_uri": "rustshare://auth/callback",
  "nonce": "<same nonce used at authorize time>"
}
```

Response:

```json
{
  "token": "<rustshare bearer token>",
  "expires_in": 86400,
  "user": {
    "id": "<uuid>",
    "email": "user@example.com",
    "display_name": "User Example",
    "is_admin": false
  }
}
```

Backend rules:

- The backend performs provider discovery from `OIDC_ISSUER_URL`.
- The backend exchanges the authorization code with PKCE.
- The backend validates the ID token nonce and verified e-mail when provided.
- Users are looked up by e-mail and provisioned automatically if missing.
- The backend returns a Rustshare JWT bearer token for current mobile API usage.

## Scope Boundary

This contract is for mobile-only auth bootstrap. It does not change:

- web OIDC login and callback flow
- web session cookies
- public share session flows
- WebSocket session behavior

## Known Follow-Up

The current mobile token returned here is the existing Rustshare bearer token. A future hardening
pass can replace it with a dedicated mobile session or refresh-token model without changing the
authorize endpoint shape.
