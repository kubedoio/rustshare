# OIDC Production Validation Checklist

> **Audience:** Operators and integrators configuring single sign-on (SSO) for RustShare  
> **Scope:** Web and mobile OIDC flows, identity provider compatibility, and claim validation

---

## Supported Flows

RustShare uses the `openidconnect` crate and supports the following flows:

| Flow | Use Case | PKCE | Details |
|------|----------|------|---------|
| **Authorization Code + PKCE** | Web browser login | Yes (SHA-256) | Server-side state stored in PostgreSQL; nonce verified |
| **Authorization Code + PKCE** | Mobile app login | Yes (SHA-256) | Mobile initiates `code_challenge`; backend exchanges code with `code_verifier` |

> **Note:** Implicit flow and Resource Owner Password Credentials flow are **not** supported.

### Required Configuration

Set these environment variables (or runtime config via admin API):

| Variable | Purpose |
|----------|---------|
| `OIDC_ISSUER_URL` | Provider discovery URL (e.g., `https://accounts.google.com`) |
| `OIDC_CLIENT_ID` | Client ID for web login |
| `OIDC_CLIENT_SECRET` | Client secret for web login |
| `OIDC_REDIRECT_URL` | Callback URL (e.g., `https://rustshare.example.com/api/v1/auth/oidc/callback`) |
| `OIDC_SCOPES` | Space-separated scopes (default: `openid profile email`) |

For mobile:

| Variable | Purpose |
|----------|---------|
| `OIDC_MOBILE_CLIENT_ID` | Separate client ID for mobile apps (optional but recommended) |
| `OIDC_MOBILE_CLIENT_SECRET` | Mobile client secret (if required by provider) |
| `OIDC_MOBILE_REDIRECT_URIS` | Comma-separated allowed redirect URIs for mobile |

---

## Provider Compatibility Matrix

### Okta

| Attribute | Status |
|-----------|--------|
| **Supported flows** | Authorization Code + PKCE (web), Authorization Code + PKCE (mobile) |
| **Tested scopes** | `openid`, `profile`, `email` |
| **Tested claims** | `email`, `email_verified` |
| **Discovery** | `.well-known/openid-configuration` |

**Configuration notes:**
- Create a **Web** application for browser login.
- Create a **Native** application for mobile login (enables PKCE by default).
- Add the callback URL (`/api/v1/auth/oidc/callback`) to **Sign-in redirect URIs**.
- Ensure the `email` claim is included in the ID token or returned via the `email` scope.

**Known issues:**
- None. Okta is fully standards-compliant with OIDC Discovery.

---

### Azure AD / Microsoft Entra ID

| Attribute | Status |
|-----------|--------|
| **Supported flows** | Authorization Code + PKCE (web), Authorization Code + PKCE (mobile) |
| **Tested scopes** | `openid`, `profile`, `email` |
| **Tested claims** | `email`, `email_verified` (emitted when `email` scope is requested) |
| **Discovery** | `https://login.microsoftonline.com/{tenant}/v2.0/.well-known/openid-configuration` |

**Configuration notes:**
- Register an application in **Azure Portal > App registrations**.
- Use the **v2.0** endpoint for OIDC Discovery.
- Add the RustShare callback URL to **Authentication > Web redirect URIs**.
- For mobile, add platform **Mobile and desktop applications** and register mobile redirect URIs.
- The `email` claim may be missing for personal Microsoft accounts unless explicitly requested; ensure the `email` scope is in `OIDC_SCOPES`.

**Known issues:**
- `email_verified` is not always present for Entra ID (work/school) accounts. RustShare treats a missing `email_verified` claim as acceptable, but rejects it explicitly if it is `false`.
- Group claims require additional configuration and are **not** used by RustShare.

---

### Google Workspace

| Attribute | Status |
|-----------|--------|
| **Supported flows** | Authorization Code + PKCE (web), Authorization Code + PKCE (mobile) |
| **Tested scopes** | `openid`, `profile`, `email` |
| **Tested claims** | `email`, `email_verified` |
| **Discovery** | `https://accounts.google.com/.well-known/openid-configuration` |

**Configuration notes:**
- Create OAuth 2.0 credentials in **Google Cloud Console > APIs & Services > Credentials**.
- Add the RustShare callback URL to **Authorized redirect URIs**.
- For mobile, use the **OAuth client ID (Android / iOS)** or a **Web** client with the mobile redirect URI.
- Google reliably returns `email_verified: true` for Google-managed addresses.

**Known issues:**
- Google does not support `localhost` redirect URIs in production; use a real domain or a configured local development domain.
- Ensure the Google account has a primary email address set; service accounts are not supported for end-user login.

---

### Keycloak

| Attribute | Status |
|-----------|--------|
| **Supported flows** | Authorization Code + PKCE (web), Authorization Code + PKCE (mobile) |
| **Tested scopes** | `openid`, `profile`, `email` |
| **Tested claims** | `email`, `email_verified` |
| **Discovery** | `https://keycloak.example.com/realms/{realm}/.well-known/openid-configuration` |

**Configuration notes:**
- Create a **OpenID Connect** client in the target realm.
- Set **Client authentication** to `On` for web (requires secret); for public mobile clients, set **Authentication flow** to `Standard flow` and disable **Client authentication**.
- Add valid redirect URIs including the RustShare callback and mobile schemes.
- Ensure **Email verified** user attribute is populated if you rely on `email_verified`.

**Known issues:**
- `email_verified` defaults to `false` in Keycloak unless explicitly set or mapped from an identity provider. If your Keycloak users are not verified, RustShare will reject logins unless you disable verification checks (not recommended).
- Clock skew between Keycloak and RustShare hosts can cause ID token validation failures. Ensure NTP is synchronized.

---

### Authentik

| Attribute | Status |
|-----------|--------|
| **Supported flows** | Authorization Code + PKCE (web), Authorization Code + PKCE (mobile) |
| **Tested scopes** | `openid`, `profile`, `email` |
| **Tested claims** | `email`, `email_verified` |
| **Discovery** | `https://authentik.example.com/application/o/{slug}/.well-known/openid-configuration` |

**Configuration notes:**
- Create an **OAuth2/OpenID Provider** in Authentik and bind it to an application.
- Set **Client type** to `Confidential` for web (requires secret) or `Public` for mobile.
- Add the RustShare callback URL to **Redirect URIs**.
- Under the provider settings, ensure **Include claims in ID token** is enabled if you need `email_verified` in the ID token.

**Known issues:**
- Authentik may not include `email_verified` in the ID token depending on the provider settings. If absent, RustShare accepts the login; if present and `false`, the login is rejected.
- Subpath hosting (e.g., `https://example.com/authentik/`) requires the `OIDC_ISSUER_URL` to match exactly the issuer value returned in discovery metadata.

---

## Claim Requirements

RustShare enforces the following ID token claims:

| Claim | Required | Validation |
|-------|----------|------------|
| `email` | **Yes** | Must be present and non-empty |
| `email_verified` | No | If present, must be `true`. Missing is treated as acceptable. |

The `sub` claim is not used as the RustShare user ID; instead, RustShare matches on `email` and provisions or links the local user account.

### User Provisioning

- If `auto_provision_users` is enabled (default: `true`), a new RustShare user is created on first OIDC login.
- If disabled, the login fails unless a local user with the same email already exists.

---

## Pre-Launch Validation Steps

Before enabling OIDC in production:

1. [ ] Configure `OIDC_ISSUER_URL`, `OIDC_CLIENT_ID`, `OIDC_CLIENT_SECRET`, and `OIDC_REDIRECT_URL` in `.env`.
2. [ ] Verify discovery metadata is reachable from the RustShare backend container:
   ```bash
   curl -fsS "${OIDC_ISSUER_URL}/.well-known/openid-configuration"
   ```
3. [ ] Complete a web login end-to-end and confirm session cookie is created.
4. [ ] Verify `/api/v1/me` returns the correct email and display name.
5. [ ] If mobile is in scope, complete a mobile authorization + token exchange flow.
6. [ ] Test login with an unverified email address and confirm it is rejected (if `email_verified` is `false`).
7. [ ] Test login with a user whose email does not exist locally and confirm provisioning behavior.
8. [ ] Document the chosen provider’s logout URL if you want to implement RP-initiated logout later.

---

## Troubleshooting

| Symptom | Likely Cause | Fix |
|---------|-------------|-----|
| `OIDC provider returned no ID token` | Provider is returning an access token only | Check scopes include `openid`; verify client configuration |
| `Invalid OIDC ID token` | Clock skew or nonce mismatch | Sync system clocks; ensure state/nonce are persisted and not replayed |
| `OIDC provider did not return an e-mail address` | `email` scope missing or claim not released | Add `email` to `OIDC_SCOPES`; check provider claim mappings |
| `OIDC provider returned an unverified e-mail address` | `email_verified: false` | Verify email in the identity provider, or adjust provider claim mapping |
| `Unknown OIDC state` | State expired or database cleaned up | State TTL is short; retry login flow |

---

## See Also

- [Deployment Guide](DEPLOYMENT.md)
- [Production Readiness](PRODUCTION_READINESS.md)
