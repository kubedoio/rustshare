# ADR 004: Local State Store Choice

## Status: Accepted
## Date: 2026-04-05

## Context
The desktop client must persist sync metadata across restarts to avoid re-scanning the entire Workspace Root.

## Decision
Use SQLite as the local state store for persistent metadata.
- Rust crate: `rusqlite` (bundled).
- Migrations: Managed locally within the desktop client.
- Data stored: Sync roots, local file inventory (hashes, mtimes, sizes), remote file mapping (ETags, remote IDs), sync queue, conflict records.

## Alternatives Considered
- **Plain JSON/TOML**: Inefficient for large file counts, lack of transaction safety.
- **Sled (KV Store)**: Fast but lacks complex querying and relational indexing for sync roots/mappings.

## Consequences
- **Pros**: ACID transactions, cross-platform stability, mature tooling, simplifies complex queries for state reconciliation.
- **Cons**: Minor overhead for small file counts; requirement to manage SQL migrations.
    
