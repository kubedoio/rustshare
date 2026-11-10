# Elembra migration policy

The first supported Elembra release establishes the database baseline in
`backend/migrations/`. It is for clean installations only. Pre-release POC
databases have no supported upgrade path and must not be attached to this
release.

## Baseline audit

The historical reference was commit `39166bf8f9bf9a2a6b872de03171991585165208`.
All 130 SQLx migrations were applied to a disposable PostgreSQL 16.15 database
with pgvector 0.8.7. A separate empty database received only the three
canonical migrations. A schema-only dump comparison found matching counts:

| Object | Count |
| --- | ---: |
| Tables | 89 |
| Constraints | 1,104 |
| Indexes | 341 |
| Triggers | 61 |
| Functions | 141 |
| RLS policies | 12 |
| Extensions (`plpgsql`, `vector`) | 2 |

The only dump text differences were 16 varchar `IN` check expressions and two
partial-index predicates. PostgreSQL rendered equivalent `ANY(text[])`
expressions differently after dump and restore; the compared values, indexed
columns, predicates, constraint names and enforcement are unchanged. The
remaining tables, constraints, indexes, triggers, functions, policies,
extensions, owners and grants matched. All 12 row-security-enabled tables and
their policies remain present; the baseline retains the existing `FORCE ROW
LEVEL SECURITY` flags.

The baseline preserves the initial OIDC, SMTP, login-protection, invitation
workflow and Memory worker discovery records. The invitation workflow gets a
fixed UUID so clean installations are deterministic. Administrator, tenant,
workspace, application and Chat community records remain application
bootstrap responsibilities. Buzz bridge consumer registration remains
application-managed, as it was for an empty historical install.

## Operating rules

- Never edit a migration after it has reached a release.
- Make every later schema change in a new, forward-only SQLx migration.
- Startup rejects unknown migration versions and checksum drift. Do not delete
  migration records or automatically upgrade pre-baseline databases.
- CI applies the baseline to an empty PostgreSQL database, reruns it to check
  idempotence, verifies checksum drift is rejected, and applies candidate
  migrations after the previous supported migration set when one exists.
- The first baseline has no supported predecessor, so no alpha.5 or alpha.6
  upgrade is claimed or tested.

Before upgrades, back up both Elembra and bundled Buzz data and configuration.
Rollback means restoring that complete pre-upgrade backup and running the
matching previous release.
