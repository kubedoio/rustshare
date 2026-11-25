# H3 Memory qualification evidence — 2026-11-25

Candidate: `c2157181cad37ed130c02b90e0fb5f08d0970b84` (protected `main`).

Status: **PARTIAL**. This record combines source/code inspection with the executable checks listed below. It does not claim a complete browser/API acceptance journey or release qualification. Issue [#119](https://github.com/kubedoio/rustshare/issues/119) remains open.

## Source acceptance matrix

| Source | Owner and retrieval | Scopes and identity | Authorization and citations | Deletion, recovery and availability | Audit |
| --- | --- | --- | --- | --- | --- |
| Files | Files owns content and versions. Memory searches an indexed projection, then re-fetches the current source before materializing it. Keyword search works without AI/embeddings. | Workspace and folder scopes. Resource identity is the Files resource; historical citations use the exact content SHA/version. No separate personal selector exists. | Current Files access is rechecked at result use and before Ask/context/citation. Citations identify the exact authorized version. | Tombstones remove indexed chunks; restoration is reindexed. Bounded reconciliation repairs stale projections; hard-delete cleanup removes orphaned chunks. A source that cannot be fetched is omitted. | Source reads record principal, resource/version, outcome and request attribution. Mandatory audit failure fails closed. |
| Notes | Notes are Files-owned resources. Search uses the indexed Files projection and live source fetch. | Workspace, folder and current-note scopes. A note uses its Files identity and exact content version where versioned. No separate personal selector exists. | Current Files/Notes authorization is rechecked before results enter context or citations. Citations identify the exact file version. | Note deletion follows Files tombstone/index cleanup; restoration is reindexed. Reconciliation repairs stale projections. Unavailable notes are omitted. | Files source-access auditing applies, including resource/version attribution; audit failure fails closed. |
| Chat | Buzz owns signed messages and channel membership. Memory catalogs references first; optional message content indexing is policy gated and is not a second event store. | Workspace and ChatChannel scopes. Identity is the Buzz event/message ID and signed provenance; no artificial versions are created. | Current channel membership is checked for search, Ask and citation. Citations point to the exact message/event. | Edits, tombstones and superseding events follow Buzz semantics. Recovery replays observed events and can page the relay. Unavailable or unauthorized messages are omitted. | Source access is attributed to the requesting principal and message/channel; mandatory audit errors fail closed where the source-read path requires audit. |
| Mail | The Mail account/message is authoritative. Unified Mail search is live authorized retrieval; Mail is not durably indexed into Memory. | Workspace and selected source only; no Mail-specific folder/current-message Memory scope. Identity is the message ResourceRef; no artificial version is created. | Mail authorization is checked against the current source before fetch/citation. | Deletion, restoration and revocation follow current Mail authority. There is no Memory Mail index to reconcile. An unavailable account yields no result. | Mail source-read audit applies where routed through the source-access path; this is not a separately qualified durable Memory audit/replay path. |
| Local Text | The explicitly configured local roots and Connector instance own observations and the local queue. Ingestion goes through the Connector Files API; Memory indexes the resulting Files source. | After import, Files workspace/folder/current-note scopes apply. Provenance retains Connector source identity and content revision; Files citations use the exact content SHA/version. | Current Files authorization controls retrieval and context materialization. Credential rotation, pause or disconnect blocks future ingestion. | Local deletion does not delete the independently imported Files copy. Files trash/restore controls its tombstone and reindexing. SQLite queue survives restart and deduplicates; Connector mappings and Files reconciliation support recovery. Offline sources remain queued. | Reads use Files source-access attribution. Connector credential-safe logging and connected platform behavior are not fully qualified by these unit tests. |

### Scope limitation

The API exposes Workspace, Folder, Note and ChatChannel scopes. Personal scope is enforced through the caller's authorization but is not a separate `personal` scope selector. Mail has only workspace/source selection. Files/Notes and Chat support indexed keyword search without AI; Mail is live search. No result should be inferred for a source/scope combination absent from these semantics.

## Executable evidence

All commands ran from the candidate source tree with `SQLX_OFFLINE=true`:

| Command | Result | What it establishes |
| --- | --- | --- |
| `cargo test -p rustshare-memory --lib` | 52 passed | Memory core unit behavior. |
| `cargo test -p rustshare-server --lib memory` | 23 passed, 647 filtered | Server Memory handler/service unit coverage. |
| `cargo test -p local-text-worker` | 9 passed | Local Text scanner/queue and credential unit coverage. |
| `npm run test -- --run src/lib/components/ask/AskExperience.test.ts` | 11 passed | Memory UI keyword-search-without-AI and scope-change behavior. |

The connected Local Text E2E is available but was not run. It is ignored by default and requires a disposable backend, PostgreSQL, object store, and synthetic owner token. Its documented command is:

```sh
RUSTSHARE_TEST_DISPOSABLE_DB=1 \
RUSTSHARE_TEST_DISPOSABLE_OBJECT_STORE=1 \
RUSTSHARE_CONNECTOR_TEST_SERVER=http://127.0.0.1:18333 \
RUSTSHARE_CONNECTOR_TEST_OWNER_TOKEN=<disposable-owner-token> \
DATABASE_URL=postgres://USER:PASSWORD@127.0.0.1:15432/rustshare_test_local_text \
SQLX_OFFLINE=true cargo test -p local-text-worker --test connector_worker_e2e -- --ignored --test-threads=1
```

No complete browser/API journey was executed for this record. Worker termination/restart/replay/reconciliation, cross-tenant denial across every source, post-index revocation across every source, audit outage handling, and proving that no unauthorized source reaches LLM context still require candidate-specific connected acceptance. The current matrix therefore does not satisfy all of #119's H3 acceptance criteria.
