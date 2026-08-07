# Specification: Elembra Connector Contract v1alpha1

Status: Draft  
Date: 2026-08-07

## Purpose

Connectors integrate external systems and local data sources with Elembra without pretending those systems have identical storage, authorization or version semantics.

This contract applies to examples such as:

- Google Drive;
- Microsoft OneDrive;
- Dropbox;
- local text folders used by Sublime Text / Notepad++;
- Obsidian-compatible vault folders;
- Shell Memory collector;
- GitHub;
- future SaaS/business systems.

A Connector is not a server plugin and is not automatically an Elembra Application. It may run locally, as a worker/service, or as part of an Application-specific integration process.

## Connector descriptor

Example:

```yaml
apiVersion: elembra.io/v1alpha1
kind: Connector
metadata:
  id: io.elembra.connector.gdrive
  name: Google Drive Connector
  version: 0.1.0

source:
  system: google-drive
  authority: external

supports:
  operations:
    list: true
    read: true
    watch: true
    write: false
    delete: false
    versions: true
    externalSharingMetadata: true
  modes:
    reference: true
    mirror: true
    import: true
    archive: true
    export: false
    bidirectionalSync: false

publishes:
  resources:
    - drive_file
  memory: true
```

The matrix is explicit. Elembra must not advertise functionality the provider/implementation cannot safely preserve.

## Integration modes

### `reference`

Keep the external object authoritative. Elembra stores a Connector-owned ResourceRef/provenance and fetches through the Connector when needed/authorized.

### `mirror`

Maintain an Elembra-side read/cache copy while the external source remains authoritative. Mirror freshness and deletion/conflict semantics must be explicit.

### `import`

Copy selected external content into an Elembra Application (normally Files) as a new Elembra-owned resource while preserving source provenance. After import, the Elembra copy can evolve independently unless a separate sync relationship is declared.

### `archive`

Create a durable Elembra-owned immutable/retention-governed copy with source provenance. Appropriate for Mail/external records where policy requires retention.

### `export`

Publish an Elembra-owned resource to the external system. The external result becomes a distinct external resource with a recorded relationship; it is not the same database object.

### `bidirectionalSync`

Synchronize mutations in both directions. This mode is difficult and must define conflict, rename, deletion, version and permission semantics before being declared supported.

Do not implement bidirectional sync merely because a provider supports read/write APIs.

## Connector state ownership

A Connector owns its integration-specific state:

- encrypted credential references/tokens;
- provider account identity;
- sync/watch cursors;
- delta tokens;
- local device identity;
- provider object/revision IDs;
- retry/backoff state;
- mapping between external IDs and Connector/Elembra ResourceRefs;
- conflict state;
- last successful synchronization/health.

Credentials are stored through the approved secrets mechanism/keyring for the runtime. They are never placed in manifests, events or normal logs.

## Authentication

### Remote/SaaS connector

Use provider-supported OAuth/OIDC/service credentials as appropriate. The Connector's external authorization is separate from Elembra authorization.

### Local/device connector

Use Elembra device authorization/scoped tokens or OIDC device flow. Never store the user's Elembra password.

Each installation has a stable revocable device/connector instance ID.

## Elembra authority

Connector service identity authenticates the process. An Elembra PrincipalContext identifies the user/workspace whose integration is being exercised.

A Connector may not use service credentials to bypass the Principal's Elembra authority when writing into Files/Memory.

## External authority

The Connector must preserve enough provider identity/provenance to explain:

- external account/source;
- stable external object ID;
- external revision/version where available;
- observed timestamps;
- original path/container/project;
- source URL only when safe;
- provider ownership/sharing metadata where relevant.

External object visibility is not automatically equivalent to Elembra workspace visibility.

## Resource references

Reference-first Connector resources use Connector-owned ResourceRefs, e.g.:

```json
{
  "application": "io.elembra.connector.gdrive",
  "resource_type": "drive_file",
  "resource_id": "1AbC...",
  "version": "drive-revision:123"
}
```

The Connector resolves/fetches these refs and enforces both external-provider access and current Elembra integration policy.

Imported resources receive new Files ResourceRefs while their provenance points to the external Connector ResourceRef.

## Ingestion and Memory

Connectors never write Memory/Search tables directly.

They publish through contracts:

```text
Connector
  -> Files import/artifact API (when importing)
  -> Memory publication API/event (when reference-first)
```

Memory indexing is owned by Elembra Memory, not by each Connector. A Connector may provide source-specific normalized metadata/text extraction hints but does not own the global vector/search index.

## Local Text Connector

Sublime Text and Notepad++ do not justify separate server-side plugins.

Use one local text/folder Connector with source adapters/configuration that can watch selected folders/files and preserve:

- relative path;
- content hash;
- mtime/observed time;
- device ID;
- source editor/profile label;
- configured inclusion/exclusion patterns.

The source files remain ordinary local files. The Connector may import/mirror them into Files and publish Memory provenance.

## Shell Memory Connector

Shell capture is a specialized local Connector/collector.

Required pre-upload behavior:

- capture command plus explicitly allowed context;
- local secret/redaction filter;
- private/ignore convention support;
- pause/disable/forget controls;
- offline SQLite queue/buffer;
- idempotent upload IDs;
- no remote command execution in the first version.

Structured shell events may be stored in a Shell-owned/Memory source model or deliberately materialized into Files views. Generated Markdown is a view/export, not necessarily the authoritative event store.

## Drive/OneDrive/Dropbox rule

These providers must **not** be implementations of one fake fully interchangeable `ArtifactStore` contract.

Reasons include differing:

- IDs/path models;
- version histories;
- ACL/sharing models;
- consistency/delta APIs;
- rename/move semantics;
- deletion/trash behavior;
- quotas/rate limits;
- shortcuts/links/native document types;
- change notification guarantees.

Instead each Connector declares its matrix and normalizes only the operations that are truly semantically safe.

## Change detection

Preferred order by source:

1. provider watch/webhook/delta API when reliable;
2. incremental cursor polling;
3. local filesystem watcher plus periodic reconciliation;
4. bounded full scan as repair/fallback.

Every Connector must have reconciliation; watchers/webhooks are hints for timeliness, not complete truth forever.

## Idempotency

Each observed external version/change has a stable ingestion identity derived from Connector instance + external object + revision/change token.

Retries must not create duplicate imported artifacts/memory records unless the source represents a genuinely new version.

## Deletions

Deletion semantics are mode-specific and must be configured/documented.

Examples:

- reference: mark external ref unavailable/tombstoned;
- mirror: remove/mark mirror according to cache policy;
- import: external deletion does not silently delete independent imported Elembra data;
- archive: external deletion must not delete retained archive;
- bidirectional sync: requires explicit deletion propagation policy and recovery window.

## Failure/health

Connector status exposes at minimum:

```text
healthy | degraded | auth_required | rate_limited | paused | failed
last_success_at
last_cursor/checkpoint (redacted where sensitive)
queued/retry count
last safe error summary
```

Failure of one Connector does not make Files/Memory globally unavailable.

## Security rules

- provider tokens/secrets never in event payloads/logs;
- local redaction occurs before upload for sensitive collectors such as shell history;
- server-side validation/redaction remains defense in depth;
- Connector cannot select a different tenant/workspace than its authorized Principal/session;
- redirects/URLs/webhooks follow existing SSRF defenses where server-side fetches are possible;
- file names/paths are treated as untrusted input;
- imported content follows Files malware/content-policy hooks when introduced.

## Non-goals v1alpha1

- universal two-way sync abstraction;
- public Connector marketplace;
- arbitrary code loading into the Elembra server;
- claiming all providers support identical operations;
- storing external-provider secrets in Memory/Files metadata;
- making the Connector an authorization authority for Elembra-native resources.

## Contract tests

Every Connector implementation needs tests for:

- tenant/principal scoping;
- credential redaction;
- idempotent retry;
- cursor/reconciliation recovery;
- provider deletion behavior per mode;
- duplicate/reordered changes;
- rate-limit/retry handling;
- revoked external auth;
- import provenance;
- no direct write to Memory/Application private tables.
