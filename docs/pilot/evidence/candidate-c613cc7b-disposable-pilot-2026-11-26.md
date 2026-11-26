# Candidate c613cc7b disposable pilot evidence — 2026-11-26

This record captures a manual Pilot Release run against PR #569 head `c613cc7b368d0c61d9bef99c053ce3e3c7071f2f`, whose parent is protected-main snapshot `a4d45723f6d63b4b478ce08b449e03354cb26e49`. The run used disposable infrastructure and synthetic data. It did not access FWS, import institutional data, invite real users, publish an image, or deploy a candidate.

## Run and immutable artifact identity

- [Pilot Release run 45746539135](https://github.com/kubedoio/rustshare/actions/runs/45746539135), attempt 1, `workflow_dispatch`, completed **success**.
- Candidate build version: `pilot-c613cc7b368d`.
- Backend image ID: `sha256:5dcc8990b5bfd88f8b4e7979d82c960422599bf0a72f740b425ba3926a864774`.
- Tested-image artifact `rustshare-pilot-tested-image-c613cc7b368d0c61d9bef99c053ce3e3c7071f2f`: artifact ID `15086115378`, Actions digest `sha256:e9fb0d4c180cab1ea9aff2c8e5ed595c59c3fcec2abc171718eeb47bf280d99d`.
- Image archive SHA-256: `f380394ffd4b8b7c2d8832c81f1d4f56ab336572ea8c86efbfa871e5e0f8c8ad`.
- Evidence artifact `rustshare-pilot-evidence-c613cc7b368d0c61d9bef99c053ce3e3c7071f2f`: artifact ID `15086200167`, Actions digest `sha256:f6fe5ec951df9e64a6f89c5611246d68e56beff25b83e941eda440f7b3452d0d`.
- `publish-pilot-image` was **skipped**. Both artifacts are private workflow artifacts; this run did not publish or deploy the candidate.

## Executed acceptance

- Fresh PostgreSQL and RustFS volumes installed the candidate; all 17 SQLx migrations passed, the first boot and canonical pilot journey passed, and the injected migration-failure diagnostic check passed.
- Browser acceptance reported 4 expected tests, 0 skipped, 0 unexpected, and 0 flaky. All tests passed without retries:
  - institutional Calendar journey preserves linked context and denies unavailable content;
  - organization admins manage a 100-user synthetic organization safely;
  - pilot administrator uses Files and edits a Note name independently from its H1;
  - Notes opens an exact historical Memory citation without changing Note identity.
- The scale journey created 100 synthetic users, granted organization-admin capability to two users, confirmed an ordinary user received HTTP 403, verified 50+50 pagination, ran search and offboarding preview/apply, checked audit attribution, and exercised cleanup and last-admin safeguards. This does not qualify two-admin recovery or group-membership revocation.
- Runtime database-role validation and negative checks, restart/readiness, persistence, bounded dependency failure/recovery, and invalid-configuration fail-closed checks passed. Passing runtime-role checks do not establish PostgreSQL RLS tenant isolation.
- Backup creation, verification, and secret scan passed. The restore drill used a separate isolated Compose project with fresh volumes and the exact candidate image; restored persistence and journey verification passed.
- Representative migration validation passed. `upgrade.env` records `UPGRADE_STATUS=not_applicable` and `UPGRADE_REASON=first_clean_install_baseline`. Therefore installed old-image upgrade and rollback from a pre-upgrade backup remain **unqualified**.
- The migration tooling step logged a Cargo warning that locked `crypto-bigint v0.7.5` is yanked. The workflow passed; this warning remains a dependency disposition item.

## Readiness limits

The workflow summary records `READINESS_DECISION=NOT_READY_UNTIL_TARGET_ENVIRONMENT_ACCEPTANCE`. This disposable run does not qualify FWS DNS and HTTPS, authentication and group-claim contract, secret custody, backup destination, monitoring, or authorized users. It also does not replace platform-specific Local Text acceptance, cross-tenant acceptance, a full group-revocation-to-Memory search/citation journey, or the installed upgrade/rollback qualification. No institutional data or real users were used.

Keep this candidate-specific evidence distinct from the earlier protected-main candidate records and from PR #567's installed-upgrade/rollback qualification.

## Complementary cross-tenant integration evidence

Separate from the manual Pilot Release run above, PR #569's integration workflow [run 45746509706](https://github.com/kubedoio/rustshare/actions/runs/45746509706), attempt 1, completed **success** on the same candidate head `c613cc7b368d0c61d9bef99c053ce3e3c7071f2f`. Its `integration-tests` job (`139663831840`) passed. The `Memory source audit integration tests (default features)` and repeated `Ask Workspace security matrix` steps also passed `cross_tenant_candidates_never_appear`.

The integration job logs additionally show these named passing contracts:

- Notes and Meetings: `contract_cross_tenant_get_note_denied`, `contract_cross_tenant_list_notes_does_not_leak`, `contract_cross_tenant_save_note_denied`, `contract_cross_tenant_get_meeting_denied`, `contract_cross_tenant_list_meetings_does_not_leak`, and `contract_cross_tenant_update_meeting_denied`.
- Search and share links: `cross_tenant_candidates_never_appear` in the Memory source audit and Ask Workspace matrices, and `tenant_isolation_contract::test_cross_tenant_share_link_is_denied`.
- Calendar and meeting IDs: `cross_tenant_ids_are_not_visible` and `calendar_event_opens_persistent_meeting_and_denies_other_tenant`.
- Groups and group sharing: `test_get_my_group_foreign_tenant_group_not_found`, `test_list_my_groups_excludes_foreign_tenant_groups`, `tenant_isolation_contract::test_group_roster_query_text_is_tenant_scoped`, `tenant_isolation_contract::test_group_share_with_foreign_group_refused_no_share_row`, and `received_user_shares_include_only_tenant_valid_group_grants`.
- Chat access: `cross_tenant_message_access_rejected` and `buzz_mode_channel_list_denies_when_chat_access_is_deactivated`.

These are integration-test results, not browser evidence from the manual Pilot Release. They complement the disposable candidate record but do not establish database RLS isolation, FWS target acceptance, installed upgrade/rollback, or release readiness.
