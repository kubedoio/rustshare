# Candidate 91ac914a: disposable pilot evidence

Date: 2026-11-25. This qualification used disposable GitHub Actions Compose projects, synthetic data, and the exact protected-main source SHA. It did not access an FWS host, institutional data, real users, or production identity.

## Candidate and immutable artifact identity

- Protected-main source SHA: `91ac914a8bc011fb59a723f7f253adbfdc213494`.
- [Pilot Release run 45711231246](https://github.com/kubedoio/rustshare/actions/runs/45711231246): completed successfully; `publish-pilot-image` was skipped.
- Tested backend image ID: `sha256:a535cff9e35c7ec25ff38a414c12e3009d97825a6b7b43f8a1c09f95bef8b357`.
- Image revision/version: `91ac914a8bc011fb59a723f7f253adbfdc213494` / `pilot-91ac914a8bc0`.
- Image archive SHA-256: `1d21a981a4a5bfa0536fcb809d95b86f8c674706c9712e1ff7b2c80ce9b6817c`.
- Tested-image artifact `rustshare-pilot-tested-image-91ac914a8bc011fb59a723f7f253adbfdc213494`, Actions digest `sha256:a10a48a285a6b4087d34cc15dc253f2fb22da5aae15faf6c18629e48e3d3b30a`; transfer artifact expires 2026-11-28.
- Evidence artifact `rustshare-pilot-evidence-91ac914a8bc011fb59a723f7f253adbfdc213494`, Actions digest `sha256:c33f6b8dc8366f4d8c4fc18b0d6ff0c638c4c6a058163c5fe3b7eb6d45f24aa1`; expires 2027-02-23.

## Results

- Fresh Compose installation and PostgreSQL/RustFS volumes: **PASS**. All 17 migrations applied; first boot and canonical journey passed.
- Browser acceptance: **3 passed**, 0 skipped, 0 unexpected, 0 flaky. Coverage included Calendar linked context/unavailable-content denial, Files/Notes identity and persistence, and opening an exact historical Note citation.
- Restart and persistence verification: **PASS**.
- Database, storage, dependency, invalid-configuration, and authentication failure/recovery checks: **PASS**.
- Backup bundle verification and restoration to a fresh isolated Compose project, followed by persistence verification: **PASS**.
- Representative migration check and migration-failure diagnostic: **PASS**.
- Runtime database privilege posture: **PASS for the separately provisioned `pilot_runtime` role**. Negative checks confirm the role cannot inherit privileged attributes, own application routines/types, or create schema objects. This is not a claim of global RLS or database-wide tenant isolation.
- Final workflow result: `WORKFLOW_RESULT=passed`; target readiness remains `NOT_READY_UNTIL_TARGET_ENVIRONMENT_ACCEPTANCE`.

## Gates not qualified by this candidate run

`upgrade.env` records `UPGRADE_STATUS=not_applicable` and `UPGRADE_REASON=first_clean_install_baseline`. The successful representative migration check is not an installed-version upgrade or rollback rehearsal. This run also has no 100-user administrative journey, native Windows/macOS connected ingestion, signed production installer, FWS DNS/HTTPS/authentication/secrets/backup-destination/monitoring/authorized-user acceptance, external recovery-domain test, container release scan, SBOM, or provenance qualification.

Therefore this is a successful **disposable clean-install and recovery baseline**, not a deployable pilot or release candidate. Do not publish or deploy it. The image transfer artifact is temporary; retain its exact hashes and the longer-lived evidence artifact with the candidate record.
