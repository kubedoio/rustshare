# OIDC Production Validation Checklist

Date: 2026-03-21

## Purpose

Validate Rustshare browser and mobile-oriented OIDC flows against the real launch identity provider before calling the web product launch-ready.

If the current pilot profile does not include OIDC at all, mark this checklist as `not in scope`, verify that the runtime does not advertise OIDC, and record that decision in the Phase 6 execution report.

Use this with:

- [API Contract Freeze](/Users/scolak/Projects/x/rustshare/docs/2026-03-21-api-contract-freeze.md)
- [Mobile OIDC Contract](/Users/scolak/Projects/x/rustshare/docs/2026-03-20-mobile-oidc-contract.md)
- [Phase 5 Launch Hardening Spec](/Users/scolak/Projects/x/rustshare/docs/2026-03-21-phase-5-launch-hardening-spec.md)

## Required Environment Inputs

Confirm all of these before testing:

- `OIDC_ISSUER_URL`
- `OIDC_CLIENT_ID`
- `OIDC_CLIENT_SECRET` if required by the provider
- `OIDC_REDIRECT_URL`
- `OIDC_SCOPES`
- `OIDC_LOGIN_LABEL`
- `OIDC_MOBILE_CLIENT_ID`
- `OIDC_MOBILE_CLIENT_SECRET` if required
- `OIDC_MOBILE_REDIRECT_URIS`

Also confirm:

- browser redirect URI is registered exactly
- mobile redirect URI allowlist matches the actual app callback scheme
- the provider is configured to return verified e-mail where expected

## Browser Validation

### Positive path

1. Open the Rustshare web app logged out.
2. Confirm the login page advertises OIDC correctly.
3. Start OIDC login.
4. Complete provider sign-in.
5. Confirm the browser returns to Rustshare successfully.
6. Confirm `/api/v1/me` resolves as the signed-in user.
7. Confirm a secure HTTP-only session cookie is present.
8. Confirm file browsing works after login.
9. Confirm logout clears the web session.

Expected result:

- browser OIDC login succeeds
- session cookie is issued by Rustshare
- logout ends access cleanly

### Failure paths

Test and record the observed behavior for:

- unregistered redirect URI
- provider user without verified e-mail
- nonce mismatch
- expired or already-used authorization code
- provider downtime or discovery failure

Expected result:

- the user gets a clear failure outcome
- Rustshare does not issue a session
- logs are sufficient to diagnose the failure

## Mobile PKCE Validation

### Positive path

1. Use the aligned standalone mobile client or a controlled PKCE test harness.
2. Call `POST /api/v1/auth/oidc/mobile/authorize`.
3. Confirm the returned provider authorization URL is valid.
4. Complete provider sign-in.
5. Return through the registered mobile callback URI.
6. Call `POST /api/v1/auth/oidc/mobile/exchange`.
7. Confirm Rustshare returns a bearer token and user object.
8. Confirm authenticated mobile API calls succeed with that token.

Expected result:

- authorize returns a valid provider URL
- exchange succeeds only with matching verifier and nonce
- token grants access to `/api/v1` routes

### Failure paths

Test and record:

- redirect URI not in `OIDC_MOBILE_REDIRECT_URIS`
- wrong `code_verifier`
- wrong `nonce`
- expired code
- reused code

Expected result:

- Rustshare rejects the exchange
- no token is issued
- failure mode is diagnosable from logs and response

## Provisioning Checks

Confirm behavior for:

- existing user by e-mail
- new user auto-provision
- admin flag handling
- display-name mapping

Expected result:

- users are mapped or created exactly once
- no duplicate accounts for the same verified e-mail

## Launch Sign-Off Criteria

OIDC is ready for launch only when:

- browser login/logout has passed against the real IdP
- mobile PKCE authorize/exchange has passed against the real IdP
- expected failure paths are recorded
- final launch values for redirect URIs and scopes are documented
- any provider-specific caveats are written down explicitly
