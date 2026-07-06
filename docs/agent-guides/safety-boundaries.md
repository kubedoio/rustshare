# Safety Boundaries

These RustShare areas are security or data-safety sensitive. Changes here need extra care.

## Areas requiring tests + a security note

When your PR touches any of these, add or update tests and include a short **Risk / Safety Notes** paragraph in the PR description.

- **Permissions and access control** — grants, roles, shares, public links, effective access calculations.
- **Workspace / tenant visibility** — cross-tenant data exposure, workspace membership checks.
- **File and object ownership** — create, rename, move, delete, restore, version lifecycle.
- **Vault sync behavior** — client/server sync, conflict resolution, quarantine, cleanup.
- **Note identity and metadata** — note IDs, public sharing, metadata visibility.
- **Indexing visibility** — what content is indexed and who can find it in search.
- **Future RAG context boundaries** — context windows, permission-aware retrieval, embedding visibility.
- **Connectors and external imports** — importing data from external systems, connector credentials.
- **Migrations and data compatibility** — schema changes, data loss risk, rollback.
- **Secret handling** — tokens, passwords, API keys, credential storage.

## General rule

If a change could let one user see another user's data, bypass a permission check, expose secrets, or corrupt shared state, treat it as a safety-boundary change.

## What to include in the PR

1. A clear description of the permission, visibility, or data-risk.
2. Tests that verify the boundary is enforced.
3. A note under **Risk / Safety Notes** in the PR template.

When in doubt, request human review before merging.
