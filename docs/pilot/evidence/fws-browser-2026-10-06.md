# FWS authenticated browser evidence — 2026-10-06

Result: **PASS**.

- Application source SHA: `1b4aeb18c9578f225e732ff44225ea6df54a874a`
- Build/version: `pilot-1b4aeb18c957`
- Deployment identity: `fws-app-20261006-rustshare`
- Configuration identity: `fws-compose-backup-20261006T163355Z`
- Target: `https://app.kubedo.io` (public HTTPS through the load balancer)
- Runner repository HEAD: `1ddfa9757215bb8a3e8ccbc42bb7d742a8d716ac`
- Browser test source SHA-256: `3d45b67be38f2879be343cb4b9dc1df4ee36b8bc6b3027fb6fcdfe6050d48ce0`
- Playwright: `1.63.0`; Chromium: `153.0.8010.12`
- Run window: `2026-10-06T17:58:44Z`–`2026-10-06T17:58:53Z`
- Machine report: one expected test, zero skipped, unexpected, or flaky; Playwright
  recorded start `2026-10-06T17:58:46.089Z` and duration `7681 ms`.
- Diagnostic artifact: [`fws-browser-2026-10-06.json`](fws-browser-2026-10-06.json),
  SHA-256 `bd7d575453783e401aa97e02eaf98ec2a18cf68da4d29084be7053207a4c9b3a`.

The authenticated browser journey opened the FWS Files UI and verified the
pilot File, opened the pilot Note, changed its Markdown H1 without renaming the
Note, renamed the Note without changing the H1, and verified both values after
reload. The test's `finally` cleanup restored the original fixture title/H1 and
verified them again after reload. Credentials are not included in this evidence.

The fixture has multiple folders with the same display name after preserved
canonical smoke runs. The browser test selects among those folders by checking
for the unique pilot File, rather than assuming the display name is unique.
