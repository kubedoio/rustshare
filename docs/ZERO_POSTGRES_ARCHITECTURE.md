# Zero-PostgreSQL Architecture (Experimental Roadmap)

> **Status:** Experimental / migration roadmap only.  
> **Last updated:** 2026-07-20

PostgreSQL 16 is the only supported production metadata backend for RustShare.
The `RUSTSHARE_METADATA_BACKEND` stages described here (`dual_write`,
`rustfs_reads`, `rustfs`) are experiments and migration stepping-stones. They
must not be used for production data.

## Why PostgreSQL remains the supported backend

The product needs a relational metadata store with rich indexing and
JSONB/JSON-style document support. Future work such as graph-structured RAG
context and advanced indexing also depends on a mature relational engine, so
PostgreSQL is the long-term metadata authority.

## Experimental stages

| Stage | Value | Behavior |
|-------|-------|----------|
| 1 | `postgres` | PostgreSQL only (current default and supported mode). |
| 2 | `dual_write` | Writes to both PostgreSQL and RustFS; reads from PostgreSQL. |
| 3 | `rustfs_reads` | Writes to both; reads from RustFS. |
| 4 | `rustfs` | RustFS only; PostgreSQL still required for supported deployments. |

The later stages are preserved as a migration/observability roadmap. Moving to
any stage beyond `postgres` requires explicit operator validation and is not
recommended for production deployments.

## Operational notes

- Do not disable PostgreSQL in production.
- Treat RustFS metadata stages as preview features that may change or be
  removed.
- Backup/restore and migration tooling assumes PostgreSQL is the authoritative
  metadata store.

See [Production Readiness](PRODUCTION_READINESS.md) for the deployment contract
and [Architecture](architecture.md) for the supported system design.
