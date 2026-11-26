# Candidate 592c9188: disposable pilot evidence

Date: 2026-11-26. This qualification used disposable GitHub Actions Compose projects, synthetic data, and the exact protected-main source SHA. It did not access an FWS host, institutional data, real users, or production identity.

## Candidate and immutable artifact identity

- Protected-main source SHA: `592c918880d230367c7fa924d2d19218c84ada28`.
- [Pilot Release run 45717748617](https://github.com/kubedoio/rustshare/actions/runs/45717748617): completed successfully; `publish-pilot-image` was skipped.
- Tested backend image ID: `sha256:4447389b0934b3aaa7b2f848eefe344693b4fbaf1c076ead741bf90540b5ced0`.
- Image revision/version: `592c918880d230367c7fa924d2d19218c84ada28` / `pilot-592c918880d2`.
- Image archive SHA-256: `376c48779a0e1a5be18ba065a09cddbc63ba02752520d0bac8641d436f67c06e`.
- Tested-image artifact `rustshare-pilot-tested-image-592c918880d230367c7fa924d2d19218c84ada28`, Actions artifact digest `sha256:a84f7a4a9a94a5a793f067719e9ee5536ef3541709e288c8fa2f687a059f7db9`; transfer artifact expires 2026-11-28.
- Evidence artifact `rustshare-pilot-evidence-592c918880d230367c7fa924d2d19218c84ada28`, Actions artifact digest `sha256:9dbd12292a9dc7dd2ac76444df933df5fa09b25960ac8c1d78699fe17c9648d2`; expires 2027-02-23.

## Results

- Fresh Compose installation and PostgreSQL/RustFS volumes: **PASS**. All 17 migrations applied; first boot and canonical journey passed.
- Browser acceptance: **3 passed**, 0 skipped, 0 unexpected, 0 flaky. The institutional test created a synthetic Calendar event, opened/reused its Meeting Note, linked existing Files/Notes resources, created a Decision and Action, reconstructed the meeting through search, and verified deleted-link sanitization plus unauthenticated denial. The other journeys covered Files/Notes identity and persistence and an exact historical Note citation.
- Institutional journey gap: this run did not provision multiple administrators or least-privilege users, mutate/revoke group membership, or assert the resulting audit record. The current journey begins with the seeded administrator; those organization-boundary and audit gates remain open.
- Restart and persistence verification: **PASS**.
- Database, storage, dependency, invalid-configuration, and authentication failure/recovery checks: **PASS**.
- Backup bundle verification and restoration to a fresh isolated Compose project, followed by persistence verification: **PASS**.
- Representative migration check and migration-failure diagnostic: **PASS**.
- Runtime database privilege posture: **PASS for the separately provisioned `pilot_runtime` role**. Negative checks confirm the role cannot inherit privileged attributes, own application routines/types, or create schema objects. This is not a claim of global RLS or database-wide tenant isolation.
- Final workflow result: `WORKFLOW_RESULT=passed`; target readiness remains `NOT_READY_UNTIL_TARGET_ENVIRONMENT_ACCEPTANCE`.

## Gates not qualified by this candidate run

`upgrade.env` records `UPGRADE_STATUS=not_applicable` and `UPGRADE_REASON=first_clean_install_baseline`. The successful representative migration check is not an installed-version upgrade or rollback rehearsal. This run also has no 100-user administrative journey, native Windows/macOS connected ingestion, signed production installer, FWS DNS/HTTPS/authentication/secrets/backup-destination/monitoring/authorized-user acceptance, external recovery-domain test, container release scan, SBOM, or provenance qualification.

Therefore this is a successful **disposable clean-install and recovery baseline**, not a deployable pilot or release candidate. Do not publish or deploy it. The image transfer artifact is temporary; retain its exact hashes and the longer-lived evidence artifact with the candidate record.
