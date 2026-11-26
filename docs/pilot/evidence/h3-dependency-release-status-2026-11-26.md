# H3 dependency and release status — 2026-11-26

Snapshot: protected `main` `a4d45723f6d63b4b478ce08b449e03354cb26e49`. Disposable-pilot and release evidence remains pinned to its tested SHA; `a4` is not itself a release candidate. This is a status record, not release authorization.

## Dependency pull requests

| PR | Disposition | Evidence / next action |
| --- | --- | --- |
| [#543](https://github.com/kubedoio/rustshare/pull/543) | Merged | Docker dependency patch group; merge `267b9f14dc180af79b1cf0522949f1159258ffc9`. |
| [#547](https://github.com/kubedoio/rustshare/pull/547) | Merged | `rand` patch update; merge `91ac914a8bc011fb59a723f7f253adbfdc213494`. |
| [#544](https://github.com/kubedoio/rustshare/pull/544) | Do not merge as submitted | Frontend typecheck passes, but `frontend` fails Prettier across 69 files after the proposed Prettier/Svelte-plugin change. Rebase and make the formatter/version choice explicit, or split the dependency group before reconsidering. |
| [#545](https://github.com/kubedoio/rustshare/pull/545) | Defer / revise | TypeScript 7 is outside the current SvelteKit peer range (`^6`) and frontend plus Buzz/Chat checks fail. Keep the supported TypeScript 6 line until the framework compatibility plan is coordinated. |
| [#552](https://github.com/kubedoio/rustshare/pull/552) | Await required CI | Approved patch/minor Rust group, refreshed exact head `79bcec74f12ded5a33b14ec0c315f7d0f908ae87` on main `a4d45723`. Exact-head review found no actionable findings. At 2026-11-26 02:53 UTC, 4 checks had passed, 2 were in progress and 16 were queued; none had failed. Merge only after every required check passes on this head and branch protection reports ready. |
| [#564](https://github.com/kubedoio/rustshare/pull/564) | Await refreshed CI and required approval | Focused synthetic Mail acceptance tests passed on previous head `87b770a1`; refreshed head `a275f18a` has 4 checks passed, 2 in progress and 13 queued as of 02:53 UTC, with none failed; review remains required. This does not yet qualify Mail on protected main. |
| [#566](https://github.com/kubedoio/rustshare/pull/566) | Await refreshed platform checks and review | Native-store synthetic tests passed on pre-rebase heads; refreshed head `83ab5894` has 3 checks in progress and 19 queued as of 02:53 UTC, with none failed. CI ZIP is unsigned and does not establish connected platform or installer acceptance. |
| [#553](https://github.com/kubedoio/rustshare/pull/553) | Do not merge as submitted | The major Rust group fails build, integration, Clippy, OpenAPI and Chat compatibility checks, including the removed `nostr::Keys` API. Split the upgrades and resolve each compatibility break before resubmission. |

## Release and security gates

- The `v0.8.0-alpha.7` Release run [42423992645](https://github.com/kubedoio/rustshare/actions/runs/42423992645) failed in `Build and push Docker image` while resolving the pre-qualified multi-platform image digest. The image scan, attestations, SBOM and release publication jobs were skipped. This is **not evidence of a runtime-image vulnerability scan failure**; the tagged run's qualification chain did not complete. The separate earlier [42421199320](https://github.com/kubedoio/rustshare/actions/runs/42421199320) workflow succeeded on the same source SHA `fd6cfba71f1cd3c07ec4729b6a9521bd821a4c67`, including its own image, attestation and SBOM jobs. Its artifacts are not evidence from the failed tagged run, and neither run qualifies current `main` `a4d45723`.
- Latest completed protected-main CodeQL run [45717748683](https://github.com/kubedoio/rustshare/actions/runs/45717748683) passed on `592c9188`; PR #573's CodeQL check also passed on its exact head before merge to `a4d45723`. Open repository alerts remain: Dependabot **62** (2 critical, 19 high, 33 medium, 8 low); CodeQL **73** (45 critical, 4 high, 14 medium, 8 low, 2 unranked). A green CodeQL run does not close or dismiss those alerts.
- The disposable Pilot Release [45729254164](https://github.com/kubedoio/rustshare/actions/runs/45729254164) passed on `b3f64684`; publication was skipped. It produced no release scan, SBOM or provenance and did not run the installed-upgrade/rollback or 100-user journeys. See [candidate evidence](candidate-b3f64684-disposable-pilot-2026-11-26.md).

Therefore no release candidate is qualified at this snapshot. Do not infer qualification from a passing disposable Pilot Release, a successful current-main CodeQL workflow, or a dependency PR that has not passed its exact-head required checks.
