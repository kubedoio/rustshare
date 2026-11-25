# H3 dependency and release status — 2026-11-26

Snapshot: protected `main` `592c918880d230367c7fa924d2d19218c84ada28`. This is a status record, not release authorization.

## Dependency pull requests

| PR | Disposition | Evidence / next action |
| --- | --- | --- |
| [#543](https://github.com/kubedoio/rustshare/pull/543) | Merged | Docker dependency patch group; merge `267b9f14dc180af79b1cf0522949f1159258ffc9`. |
| [#547](https://github.com/kubedoio/rustshare/pull/547) | Merged | `rand` patch update; merge `91ac914a8bc011fb59a723f7f253adbfdc213494`. |
| [#544](https://github.com/kubedoio/rustshare/pull/544) | Do not merge as submitted | Frontend typecheck passes, but `frontend` fails Prettier across 69 files after the proposed Prettier/Svelte-plugin change. Rebase and make the formatter/version choice explicit, or split the dependency group before reconsidering. |
| [#545](https://github.com/kubedoio/rustshare/pull/545) | Defer / revise | TypeScript 7 is outside the current SvelteKit peer range (`^6`) and frontend plus Buzz/Chat checks fail. Keep the supported TypeScript 6 line until the framework compatibility plan is coordinated. |
| [#552](https://github.com/kubedoio/rustshare/pull/552) | Await required CI | Approved patch/minor Rust group, exact head `cfb1084f7b6e57f4de369b2e0cb4ef1fd6b58b48`. At this snapshot, required integration, compose, Chat, format, Clippy and Cargo Deny checks are pending or running. Merge only after all required checks pass on the current base under branch protection. |
| [#553](https://github.com/kubedoio/rustshare/pull/553) | Do not merge as submitted | The major Rust group fails build, integration, Clippy, OpenAPI and Chat compatibility checks, including the removed `nostr::Keys` API. Split the upgrades and resolve each compatibility break before resubmission. |

## Release and security gates

- The `v0.8.0-alpha.7` Release run [42423992645](https://github.com/kubedoio/rustshare/actions/runs/42423992645) failed in `Build and push Docker image` while resolving the pre-qualified multi-platform image digest. The image scan, attestations, SBOM and release publication jobs were skipped. This is **not evidence of a runtime-image vulnerability scan failure**; the tagged run's qualification chain did not complete. The separate earlier [42421199320](https://github.com/kubedoio/rustshare/actions/runs/42421199320) workflow succeeded on the same source SHA `fd6cfba71f1cd3c07ec4729b6a9521bd821a4c67`, including its own image, attestation and SBOM jobs. Its artifacts are not evidence from the failed tagged run, and neither run qualifies current `main` `592c9188`.
- Current-main CodeQL run [45717748683](https://github.com/kubedoio/rustshare/actions/runs/45717748683) passed on `592c9188`. Open repository alerts remain: Dependabot **62** (2 critical, 19 high, 33 medium, 8 low); CodeQL **73** (45 critical, 4 high, 14 medium, 8 low, 2 unranked). A green CodeQL run does not close or dismiss those alerts.
- The disposable Pilot Release [45717748617](https://github.com/kubedoio/rustshare/actions/runs/45717748617) passed on `592c9188`; publication was skipped. It produced no release scan, SBOM or provenance and did not run the installed-upgrade/rollback or 100-user journeys. See [candidate evidence](candidate-592c9188-disposable-pilot-2026-11-26.md).

Therefore no release candidate is qualified at this snapshot. Do not infer qualification from a passing disposable Pilot Release, a successful current-main CodeQL workflow, or a dependency PR that has not passed its exact-head required checks.
