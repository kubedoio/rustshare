# RustShare Client Integration Checklist

Date: 2026-03-21

Use this checklist for any new web, mobile, or desktop client work.

Primary contract reference:

- [API Contract Freeze](/Users/scolak/Projects/x/rustshare/docs/2026-03-21-api-contract-freeze.md)

## Required Rules

- use `/api/v1/...` for all new API integrations
- use `GET /api/ws` for realtime
- do not introduce new dependencies on unversioned `/api/...` aliases
- do not introduce new dependencies on removed websocket aliases such as `/api/v1/ws`
- do not introduce new dependencies on removed websocket aliases such as `/api/sync`

## Browser Web Checklist

- use secure HTTP-only cookie sessions
- do not persist JWTs in local storage for browser auth
- send CSRF header on unsafe requests
- bootstrap session state from `GET /api/v1/me`
- use `GET /api/v1/auth/config` to discover enabled login methods
- tolerate unknown websocket event types

## Mobile / Desktop Checklist

- use bearer-token auth for non-browser clients
- use `POST /api/v1/auth/oidc/mobile/authorize`
- use `POST /api/v1/auth/oidc/mobile/exchange`
- use `GET /api/v1/me` as the account bootstrap endpoint
- use `GET /api/ws` for realtime
- support cookie-less token auth on websocket upgrade via `Authorization` or `?token=`
- tolerate additive JSON fields and unknown websocket event types

## Realtime Checklist

- treat `/api/ws` as the canonical endpoint
- authenticate with browser cookie, `Authorization: Bearer`, or `?token=...`
- do not assume only the currently known event types will ever exist
- ignore unknown events safely
- handle reconnection without assuming missed events were not emitted

## Operator / Internal Boundary

These are not general client-contract routes:

- `/api/v1/admin/replication/jobs`
- `/api/v1/admin/replication/summary`
- `/api/v1/admin/replication/targets`

Use them only for internal admin tooling and operator surfaces.

## Before Shipping a New Client Slice

- verify the route list against [API Contract Freeze](/Users/scolak/Projects/x/rustshare/docs/2026-03-21-api-contract-freeze.md)
- verify realtime targets `/api/ws`
- verify no compatibility-only routes were introduced
- verify auth behavior matches the correct client type
