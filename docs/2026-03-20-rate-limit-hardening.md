# Rate Limit Hardening

Rustshare now applies per-IP rate limiting to the highest-risk public and authentication routes.

Covered routes:
- `POST /api/auth/login` and `/api/v1/auth/login`
- `GET /api/auth/oidc/login` and `/api/v1/auth/oidc/login`
- `POST /api/public/share/:token/session` and `/api/v1/public/share/:token/session`
- `GET /api/public/share/:token/info` and `/api/v1/public/share/:token/info`
- `GET /api/public/share/:token/folder/contents` and `/api/v1/public/share/:token/folder/contents`
- `GET /api/public/share/:token/file` and `/api/v1/public/share/:token/file`
- `GET /api/public/share/:token/folder/files/:file_id` and `/api/v1/public/share/:token/folder/files/:file_id`
- `POST /api/public/share/:token/folder/upload` and `/api/v1/public/share/:token/folder/upload`
- authenticated share-management routes such as creating, updating, and revoking shares

Default quotas:
- `RUSTSHARE_RATE_LIMIT_AUTH_LOGIN_PER_MINUTE=10`
- `RUSTSHARE_RATE_LIMIT_OIDC_LOGIN_PER_MINUTE=30`
- `RUSTSHARE_RATE_LIMIT_SHARE_SESSION_PER_MINUTE=5`
- `RUSTSHARE_RATE_LIMIT_SHARE_INFO_PER_MINUTE=30`
- `RUSTSHARE_RATE_LIMIT_SHARE_DOWNLOAD_PER_MINUTE=30`
- `RUSTSHARE_RATE_LIMIT_SHARE_UPLOAD_PER_MINUTE=20`
- `RUSTSHARE_RATE_LIMIT_AUTHENTICATED_SHARE_ADMIN_PER_MINUTE=120`

Operational notes:
- Limits are enforced per client IP using the existing proxy-aware IP extraction middleware.
- Rejections return `429 Too Many Requests` with `Retry-After: 60`.
- Public share password prompts are intentionally the strictest limit because they are the easiest brute-force target.
- Upload-only public folder links are protected by the upload limiter, not the download limiter.

Recommended production tuning:
- tighten `AUTH_LOGIN` and `SHARE_SESSION` if you require strong password policies
- raise `SHARE_DOWNLOAD` only after validating CDN/proxy behavior and bandwidth headroom
- keep `SHARE_UPLOAD` conservative until you have storage and abuse monitoring in place
