# Elembra Chat Phase F qualification — 2026-11-11

## Scope and source

- RustShare UI/backend source: `2078abae9f96dcfc4f62e74c72baba99a63c2c5e` (verified `main`).
- Buzz `main`: `fe513d015e9885f53118a48519ef1928fb63d5f5`.
- Candidate Buzz source: `b3aaabf635d992b08b57773071686a92196884e1`, based on supported v1alpha1 commit `c9a373beca1d602d02308c687db5535c63f3921f`, including the guest-write-denial backport through `8ce0b7bd8da474f6b858810b349549377714c33a` and the reviewed runtime-base refresh from Buzz PR #9.
- Candidate image: `ghcr.io/kubedoio/buzz-elembra@sha256:38c362bb37d15ea879634b0f56a867401df4e4e0074a84382dff6b07bb578b4e`, multi-arch OCI index. Linux/amd64 child manifest: `sha256:4bbe7e5a1594f02681607575a71b7c31aa12248c8991d71fd328741a47d0284c`; Linux/arm64 child manifest: `sha256:dedd0ae2a18f64aba43cd9fd4be71d5ee72efc2a006906a24f7f72777a59c7f6`.
- Distroless Elembra base digest: `sha256:cc66d222f0a7ec996b78a99fe76a969bc80b539a5016ad866c14d1949bc6d0ab` (PR #9 refreshed `54df941e…`). The candidate keeps v1alpha1; v1alpha2 and server-managed signing remain disabled.

## Build, scan, and provenance

- Multi-arch build: [Buzz Docker workflow run 43104676847](https://github.com/kubedoio/buzz/actions/runs/43104676847), both linux/amd64 and linux/arm64 build and scan jobs passed.
- The workflow-pinned Trivy image (`aquasec/trivy@sha256:e2b22eac59c02003d8749f5b8d9bd073b62e30fefaef5b7c8371204e0a4b0c08`) reported zero Critical/High findings for both platforms. Independent scans of the exact multi-arch digest with the same scanner/policy also reported zero: `trivy-amd64.log`, `trivy-arm64.log` in the execution evidence directory.
- Sigstore build provenance verified with `gh attestation verify` (GitHub CLI 2.103.3). Verified identity is `kubedoio/buzz/.github/workflows/docker.yml@refs/tags/relay-v0.2.1-elembra.5-rc.1`, source and workflow SHA `b3aaabf635d992b08b57773071686a92196884e1`, subject digest as above. Verification result is retained as `provenance.json` in the execution evidence directory.
- SPDX SBOM generation and attestation: [workflow run 43110050027](https://github.com/kubedoio/buzz/actions/runs/43110050027). Its SBOM attestation verified against the exact candidate digest and the Buzz main SBOM workflow at `fe513d015e9885f53118a48519ef1928fb63d5f5`. The downloadable artifact is `sbom-43110050027`.

## Integrated acceptance

All suites used the RustShare browser UI and a disposable, uniquely named Compose project with fresh databases, object stores, identities, and secrets. That project was removed after testing. The existing development deployment was not reset; `http://127.0.0.1:18080/` still returned 200 afterwards.

- [Channel browser acceptance](chat-channel-acceptance-20261111-phase-f.json): 13/13 PASS. Includes private membership/history/Files authorization and revocation, exact-event retry with no duplicate, guest downgrade denial, service restart and history recovery, account deactivation, then observer restart/replay with the deactivated account still denied (401).
- [Conversation browser acceptance](chat-conversation-acceptance-20261111-phase-f.json): 9/9 PASS. Includes #486 reaction fix, exact replies/nested threads, drafts, author/admin deletion, and private-thread revocation.
- [Direct relay security acceptance](chat-direct-negative-acceptance-20261111-phase-f.json): 17/17 PASS, including guest write denial, removed/non-member writes, cross-community and cross-Host isolation, and NIP-42 authentication.

The browser harness was adapted only in the disposable worktree to use the isolated relay port/Host and to combine account deactivation with observer restart/replay. Those temporary test-driver edits were restored and are retained as `acceptance-driver-port-and-replay-adaptation.patch`; RustShare product code remained at the SHA above.

## CI and review

- RustShare main checks at `2078…`: 26 check runs succeeded or were skipped.
- Buzz main still has unrelated failures: Tauri desktop formatting, musl cross-compilation, and Windows workspace Clippy. The musl/Windows failures cite unresolved `rmcp::model::Content` in `buzz-dev-mcp`; the target GNU Linux relay image builds and scans passed on amd64 and arm64.
- The v1alpha1 backport received an independent code review during qualification, but still needs required human Buzz PR review. The RustShare pin transition is draft PR #488; no production pin has changed on `main`.
- GitHub reports 60 open RustShare Dependabot alerts (2 Critical, 18 High). The lockfile resolves several cited packages to patched versions (including Vite 8.3.5, Vitest 5.0.5, devalue 5.9.4, and happy-dom 21.0.4); the alerts remain open and need maintainer triage before pilot approval.

## Operations and release decision

**Development qualification: PASS for the v1alpha1 relay candidate and the three acceptance matrices. Production/pilot release: NOT READY.** No pilot host/operator access was available. An off-host backup destination, encryption-key custody, isolated full restore, and rollback rehearsal preserving Buzz history and identities remain unverified. The draft pin PR must not be merged or deployed until those gates, independent review, and explicit operator approval are complete.

The repository-level Pilot Release workflow (#335) is not evidence of real pilot-host acceptance. Phase G should follow #333 with a 10–15 user cohort, named institutional operators, verified backup/restore and monitoring, then a controlled host rehearsal before any user enrollment; do not expand directly to 100 users.
