# H3 Memory qualification evidence — 2026-11-25

Candidate: protected `main` `267b9f14dc180af79b1cf0522949f1159258ffc9`.

Status: **PARTIAL**. This record combines source/code inspection with exact-main integration and disposable pilot evidence. It does not claim a complete all-source browser/API acceptance journey or release qualification. Issue [#119](https://github.com/kubedoio/rustshare/issues/119) remains open.

## Source acceptance matrix

| Source | Owner and retrieval | Scopes and identity | Authorization and citations | Deletion, recovery and availability | Audit |
| --- | --- | --- | --- | --- | --- |
| Files | Files owns content and versions. Memory searches an indexed projection, then re-fetches the current source before materializing it. Keyword search works without AI/embeddings. | Workspace and folder scopes. Resource identity is the Files resource; historical citations use the exact content SHA/version. No separate personal selector exists. | Current Files access is rechecked at result use and before Ask/context/citation. Citations identify the exact authorized version. | Tombstones remove indexed chunks; restoration is reindexed. Bounded reconciliation repairs stale projections; hard-delete cleanup removes orphaned chunks. A source that cannot be fetched is omitted. | Source reads record principal, resource/version, outcome and request attribution. Mandatory audit failure fails closed. |
| Notes | Notes are Files-owned resources. Search uses the indexed Files projection and live source fetch. | Workspace, folder and current-note scopes. A note uses its Files identity and exact content version where versioned. No separate personal selector exists. | Current Files/Notes authorization is rechecked before results enter context or citations. Citations identify the exact file version. | Note deletion follows Files tombstone/index cleanup; restoration is reindexed. Reconciliation repairs stale projections. Unavailable notes are omitted. | Files source-access auditing applies, including resource/version attribution; audit failure fails closed. |
| Chat | Buzz owns signed messages and channel membership. Memory catalogs references first; optional message content indexing is policy gated and is not a second event store. | Workspace and ChatChannel scopes. Identity is the Buzz event/message ID and signed provenance; no artificial versions are created. | Current channel membership is checked for search, Ask and citation. Citations point to the exact message/event. | Edits, tombstones and superseding events follow Buzz semantics. Recovery replays observed events and can page the relay. Unavailable or unauthorized messages are omitted. | Source access is attributed to the requesting principal and message/channel; mandatory audit errors fail closed where the source-read path requires audit. |
| Mail | The Mail account/message is authoritative. Unified Mail search is live authorized retrieval; Mail is not durably indexed into Memory. | Workspace and selected source only; no Mail-specific folder/current-message Memory scope. Identity is the message ResourceRef; no artificial version is created. | Mail authorization is checked against the current source before fetch/citation. | Deletion, restoration and revocation follow current Mail authority. There is no Memory Mail index to reconcile. An unavailable account yields no result. | Mail source-read audit applies where routed through the source-access path; this is not a separately qualified durable Memory audit/replay path. |
| Local Text | The explicitly configured local roots and Connector instance own observations and the local queue. Ingestion goes through the Connector Files API; Memory indexes the resulting Files source. | After import, Files workspace/folder/current-note scopes apply. Provenance retains Connector source identity and content revision; Files citations use the exact content SHA/version. | Current Files authorization controls retrieval and context materialization. Credential rotation, pause or disconnect blocks future ingestion. | Local deletion does not delete the independently imported Files copy. Files trash/restore controls its tombstone and reindexing. SQLite queue survives restart and deduplicates; Connector mappings and Files reconciliation support recovery. Offline sources remain queued. | Reads use Files source-access attribution. Native credential operations passed isolated CI on Windows, macOS and Linux PR heads; interactive provisioning, credential-safe logging and signed installer acceptance remain open. |

### Scope limitation

The API exposes Workspace, Folder, Note and ChatChannel scopes. Personal scope is enforced through the caller's authorization but is not a separate `personal` scope selector. Mail has only workspace/source selection. Files/Notes and Chat support indexed keyword search without AI; Mail is live search. No result should be inferred for a source/scope combination absent from these semantics.

## Executable evidence

The following evidence is tied to the stated commits and runs:

| Evidence | Result | What it establishes |
| --- | --- | --- |
| [Integration Tests run 45697982362](https://github.com/kubedoio/rustshare/actions/runs/45697982362), commit `267b9f14` | Passed | PostgreSQL/RustFS integration suite, Memory reconciliation lease/restart recovery, source-access audit tests (16 passed), and Ask Workspace authorization matrix (20 passed on each of two runs). Tests include keyword search without embeddings, cross-tenant exclusion, revoked Chat membership and Files shares, stale ACL denial, unauthorized snippet exclusion, and audit failure blocking LLM context submission. |
| Local Text worker E2E in run 45697982362 | Passed | Synthetic disposable database/object-store path exercises connector import/mapping, Files-to-Memory projection/search, offline/outbox recovery, worker restart, credential rotation and old-token denial, deletion/restoration and reconciliation. This is one source-specific E2E, not all-source qualification. |
| [Pilot Release run 45697982396](https://github.com/kubedoio/rustshare/actions/runs/45697982396), commit `267b9f14` | Passed | Clean installation and 17 migrations, canonical Files/Notes/search/user lifecycle, restart/persistence, failure recovery, backup and restore to fresh volumes, and three browser journeys. The browser checks cover Files/Notes identity and persistence, exact historical Note citation, and denied access to unavailable linked content. Image publication was skipped. |
| Earlier focused unit checks on `c2157181` | Passed | Memory core (52), Memory server unit cases (23), Local Text worker (9), and Ask UI tests (11). These remain historical baseline evidence, not exact-main qualification. |
| PR #564 focused Mail tests on its exact head | Passed | Live Mail source owner, cross-tenant empty result, and disabled-app revocation without Ask provider invocation. This is separate PR-head evidence, not a Mail test in the exact-main 267b integration run. |
| [Linux Secret Service job](https://github.com/kubedoio/rustshare/actions/runs/45665747903) on PR #565 head `313f60ae` | Passed | Native secret-service credential provision, lookup, rotation and deletion on a Linux CI runner. |
| [macOS Keychain](https://github.com/kubedoio/rustshare/actions/runs/45703480561) and [Windows Credential Manager](https://github.com/kubedoio/rustshare/actions/runs/45703480503) on PR #566 head `aa060749` | Passed | Provision, lookup, rotation and deletion on isolated platform runners. The emitted Windows ZIP is unsigned and is not a production installer. These are PR-head tests, not exact-main or FWS-host acceptance. |

The Local Text E2E is ignored by default and requires a disposable backend, PostgreSQL, object store, and synthetic owner token. It ran in the exact-main Integration Tests workflow above. Its reproducible command is:

```sh
RUSTSHARE_TEST_DISPOSABLE_DB=1 \
RUSTSHARE_TEST_DISPOSABLE_OBJECT_STORE=1 \
RUSTSHARE_CONNECTOR_TEST_SERVER=http://127.0.0.1:18333 \
RUSTSHARE_CONNECTOR_TEST_OWNER_TOKEN=<disposable-owner-token> \
DATABASE_URL=postgres://USER:PASSWORD@127.0.0.1:15432/rustshare_test_local_text \
SQLX_OFFLINE=true cargo test -p local-text-worker --test connector_worker_e2e -- --ignored --test-threads=1
```

The exact-main browser journey did not cover every Memory source. The Local Text E2E covers its own worker termination/restart/replay/reconciliation and credential revocation path; it does not prove those properties for every source. Cross-tenant denial and post-index revocation have focused Files/Chat search coverage, but not a complete matrix for every source. Audit failure is proven to block LLM context in the Ask Workspace test; a full audit-service outage journey across all sources remains untested. Therefore this matrix does not satisfy all of #119's H3 acceptance criteria.
