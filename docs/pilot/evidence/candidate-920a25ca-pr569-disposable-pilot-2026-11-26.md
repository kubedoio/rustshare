# PR #569 100-user disposable pilot evidence — d0-based candidate

Date: 2026-11-26

## Candidate identity

- PR head: `920a25ca8f021108a0be0f6cbc72adb2b268549a` (based on protected main `d0fc70c0594f709223e3ff83423cc38340f03dc6`).
- Pilot Release run: [45849369794](https://github.com/kubedoio/rustshare/actions/runs/45849369794), attempt 1 — **passed**; publish job skipped.
- Tested backend image ID: `sha256:0f93f645188120b75240a9dc75dee4d5a14476b200f2a2190a4c14b7ed1ab98b`.
- Compose config ID: `a9228505660c8d8e4123cf7fe0c932fba98ca5857288b97745c6ac8498f6cf64`.
- Evidence artifact [15137723500](https://github.com/kubedoio/rustshare/actions/runs/45849369794/artifacts/15137723500), digest `sha256:9426a5fa8fc3d2b3907484b00334ca4accf8444149fe32d1fcd43e4baf18bf64`.
- Tested-image transfer artifact [15137094740](https://github.com/kubedoio/rustshare/actions/runs/45849369794/artifacts/15137094740), digest `sha256:b4d8996f547dce9d00d19ab3c3f27810d6570430c295090ee6971da0c4889022`; image archive SHA-256 `63c9b642bd8526ba0901406e5e18e1a1857f5654d0c8ecaaa01f269466b1da93`.
- Image publication was skipped; there is no registry digest.

## Qualification results

- Fresh PostgreSQL and RustFS volumes; all 17 migrations through `20261123190000`; clean install, runtime database checks, restart/persistence, dependency failure/recovery, and invalid-configuration checks passed.
- All five Playwright tests passed: Calendar import/export/idempotency; denial for unavailable linked Calendar content; synthetic 100-user organization administration; Files and independent Note title/H1 editing; exact historical Memory citation.
- Backup verification, restore to fresh volumes, installed upgrade from immutable `v0.8.0-alpha.7`, representative migration validation, and fresh-volume rollback passed.

## Qualification boundary

This result qualifies only the exact d0-based PR head above. It does not qualify the later protected-main commit `b9baaaefe59c9b0fd4f42abddd4763e576617066` or the refreshed PR #569 head `d3f6e442b46dc77b3669253b26cd5a765a096635`. The latter has a separate non-publishing Pilot Release run [45862470683](https://github.com/kubedoio/rustshare/actions/runs/45862470683), queued at the time of this record. Main CI and Integration Tests for b9 were also in progress. FWS remains **NO-GO** pending operator-owned DNS, HTTPS, authentication, secret custody, backup destination and restore, monitoring, and authorized-user gates. No institutional data, real users, publication, or deployment was involved.
