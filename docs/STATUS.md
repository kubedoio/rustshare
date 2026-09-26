# Elembra Project Status

> **Last updated:** 2026-09-26  
> **Maturity:** controlled Customer Alpha qualification; **not production-ready**

Elembra has moved beyond the original RustShare MVP architecture. The current
product is an Application-based sovereign business-memory workspace with Files,
Notes, Buzz-backed Chat, Memory/Search and permission-aware cited Ask.

The repository-level architecture and release controls are substantially in
place. The remaining launch work is mostly **target-environment evidence**:
install the exact released artifacts on a clean host, prove recovery, TLS/WSS,
real OIDC, monitoring/alerting and adversarial isolation, then record an
evidence-backed GO/NO-GO decision.

## Supported Alpha deployment contract

The controlled Alpha currently supports:

- one Linux host using Docker Engine + Docker Compose;
- PostgreSQL as the supported application metadata database;
- RustFS-compatible S3 object storage for Elembra content;
- web UI/API as the supported client surface;
- bundled Buzz with its own PostgreSQL, Redis and dedicated RustFS lifecycle;
- the hardened Git-disabled `buzz-elembra` runtime for bundled Chat;
- externally terminated HTTPS/WSS;
- OIDC or password login only after the chosen deployment mode is validated.

Not claimed by the Alpha: Kubernetes, HA, multi-region, zero-downtime upgrades,
production mobile/desktop clients or enterprise security certification.

## Implemented foundations

### Platform / Applications

- Application registry/manifests and the one-time Module → Application cutover.
- `PrincipalContext`, `ResourceRef` and source-authorization contracts.
- Transactional outbox + durable Integration Events.
- Tenant-scoped authorization boundaries and protected administrative APIs.

### Files / Notes

- file/folder CRUD, upload/download, move/rename, trash/restore and version history;
- internal/group sharing and public link modes;
- Notes/Markdown workflows;
- RustFS-backed object storage;
- audit/recovery tooling.

### Memory / Search / Ask

- Memory catalog/provenance separated from source ownership;
- PostgreSQL FTS/vector indexing paths;
- permission-aware retrieval with source reauthorization before materialization;
- Files/Notes and Chat projection/search paths;
- cited Ask flows when an LLM provider is configured;
- clean degraded behavior when Ask is not configured.

Advanced retrieval quality, additional Connector sources and some rebuild/audit
coverage remain roadmap work; issue #119 tracks those remaining gaps.

### Chat / Buzz

- Buzz remains the authoritative Chat engine and signing authority;
- explicit Workspace ↔ community mapping;
- Principal ↔ Buzz-key binding and sovereign browser-held key custody;
- admission/revocation through durable 9030/9031 delivery;
- admin Chat revocation endpoint/UX;
- Files attachments via ResourceRefs with read-time reauthorization;
- Chat → Memory projection with Buzz provenance;
- managed observer with reconnect/replay/deduplication;
- automatic community and dynamic channel discovery;
- 13 live Buzz conformance proofs plus a separate no-ACL/direct-Buzz-DB guard;
- hardened dedicated `buzz-elembra` runtime with Git disabled.

Identity/device UX still has limited remaining scope under #215.

## Release and supply-chain state

The Customer Alpha process intentionally rejected several immutable candidates
when exact published images failed the runtime vulnerability policy. Those
rejections are preserved in `docs/releases/customer-alpha-gate.yaml`.

Current controls include:

- protected `main` with required reviews/checks;
- immutable OCI digests;
- SBOM/provenance publication;
- minimal backend and observer runtime images;
- pre-promotion Critical/High vulnerability gating for release images;
- separate hardened Buzz Elembra image with per-architecture vulnerability scans.

The next release candidate must use the hardened Buzz compatibility baseline and
pass the same exact-artifact security/product gates before external qualification.

## What still blocks Customer Alpha GO

The authoritative state is `docs/releases/customer-alpha-gate.yaml`. Mandatory
target-environment evidence still includes:

1. clean install from exact published artifacts;
2. destructive backup + restore rehearsal;
3. real HTTPS/WSS;
4. real target OIDC validation;
5. customer product/admin smoke including offboarding;
6. upgrade + failed-upgrade recovery;
7. monitoring and alert delivery;
8. support-bundle secret review;
9. cross-tenant adversarial campaign;
10. bounded release security review with zero BLOCKER/HIGH findings.

Until those gates pass, the correct decision remains **NO-GO**.

## Known exclusions / deferred work

- Obsidian/vault sync is excluded from the first Customer Alpha while #236 is open.
- Full account-managed Chat device administration remains deferred under #215.
- Reply/thread composer UI remains deferred under #243.
- Connector/Agent expansion, Object Spaces, HA/Kubernetes and broad architecture
  refactors (#286/#287) do not block the controlled Alpha.
- Mobile/desktop are not part of the production claim.

## Canonical references

- [README](../README.md)
- [Elembra platform architecture](architecture/elembra-platform.md)
- [Customer Alpha runbook](runbooks/customer-alpha.md)
- [Customer Alpha evidence](releases/customer-alpha-gate.yaml)
- [Production readiness](PRODUCTION_READINESS.md)
