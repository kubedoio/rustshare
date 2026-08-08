# Specification: Elembra Application Manifest v1alpha1

Status: Draft  
Date: 2026-08-07

## Purpose

The Application manifest is the declarative contract between an Elembra Application and the Elembra Shell/Platform registry.

It describes identity, ownership, contracts and Contributions. It does **not** load executable native code and does not imply a stable public third-party ABI.

## API identity

```yaml
apiVersion: elembra.io/v1alpha1
kind: Application
```

`v1alpha1` is intentionally allowed to change incompatibly while the first-party architecture is being proven.

## Example

```yaml
apiVersion: elembra.io/v1alpha1
kind: Application
metadata:
  id: io.elembra.notes
  name: Elembra Notes
  version: 0.1.0
  description: Durable file-backed knowledge and notes.

runtime:
  kind: embedded

contracts:
  provides:
    - id: io.elembra.notes.api
      version: v1alpha1
  requires:
    - id: io.elembra.files.artifacts
      version: v1alpha1
    - id: io.elembra.platform.authorization
      version: v1alpha1
    - id: io.elembra.memory.publish
      version: v1alpha1

resources:
  - type: note
    actions:
      - notes.read
      - notes.write
      - notes.delete
      - notes.share

contributions:
  navigation:
    - id: notes
      label: Notes
      icon: sticky-note
      route: /apps/notes
      order: 30
  routes:
    - id: notes.main
      route: /apps/notes
      renderer: notes-main
  commands:
    - id: notes.create
      label: New note
      action: notes.create
  dashboard:
    - id: notes.recent
      renderer: recent-notes
      order: 20
  settings:
    - id: notes.settings
      route: /settings/apps/notes

integrationEvents:
  publishes:
    - io.elembra.notes.note.created.v1
    - io.elembra.notes.note.updated.v1
    - io.elembra.notes.note.deleted.v1
  subscribes:
    - io.elembra.files.file.deleted.v1

memory:
  sourceTypes:
    - notes.note
  publication: reference-first

configuration:
  schema: contracts/io.elembra.notes/config-v1alpha1.schema.json

data:
  owner: io.elembra.notes
  preserveOnDisable: true
  exportSupported: true

health: null
```

## Required fields

### `metadata.id`

Globally unique stable Application ID. Reverse-DNS style is required for first-party Applications:

```text
io.elembra.files
io.elembra.notes
io.elembra.mail
io.elembra.memory
io.elembra.chat
io.elembra.agents
```

Application ID is part of ResourceRef ownership and must not encode deployment location.

### `metadata.version`

Application implementation/product version. It is not the contract version.

### `runtime.kind`

Allowed initial values:

- `embedded` — compiled into the Elembra process;
- `service` — independently deployed Elembra-owned runtime;
- `bridge` — adapter around an independent Engine/system.

Runtime changes are deployment migrations, not Application identity migrations.

For `service`/`bridge`, runtime deployment configuration may reference an endpoint/service name outside the manifest. Secrets and environment-specific URLs must not be hard-coded into the portable manifest.

## Contracts

`contracts.provides` and `contracts.requires` list stable logical contract IDs and their current schema versions.

A contract definition must live in an owned schema/OpenAPI/shared contract package, not only in prose.

A manifest dependency means the Application needs the contract, not direct access to the provider's implementation or database.

## Resources and actions

Each Application declares owned resource types and action-capability names.

Action names use a stable namespace:

```text
files.read
files.write
files.delete
files.share
notes.read
notes.write
mail.read
mail.send
chat.read
chat.post
memory.query
agents.run
```

Actions should be specific enough for safe delegation. `application.admin` may exist for configuration but must not replace resource-specific actions.

## Contributions

Contributions are declarative shell composition points.

Initial supported families:

- `navigation`;
- `routes`;
- `commands`;
- `dashboard`;
- `settings`;
- `admin`;
- `searchProviders`;
- `renderers` where the renderer key is compiled/registered by first-party code.

Initial manifests must not inject arbitrary JavaScript/HTML into the Shell.

### Route namespace

User-facing Application routes use:

```text
/apps/<application-short-name>/...
```

The public API uses:

```text
/api/v1/applications/...
```

Application-owned APIs may use a domain path such as `/api/v1/files/...` when the domain itself is clearer; registry/configuration APIs use `/api/v1/applications/...`.

There is no requirement to maintain `/modules/...` after the one-time migration.

## Integration events

Published/subscribed event names are contract declarations, not automatic subscriptions.

The authoritative event schema is registered by the owning publisher and follows `integration-event-v1alpha1.md`.

## Memory

The manifest declares whether/how resources participate in Elembra Memory.

Initial publication values:

- `none`;
- `reference-first`;
- `artifact-backed`.

This is a policy/default declaration. It does not grant Memory access beyond Principal/resource authorization.

## Data policy

`data.owner` must match the Application that owns private data/migrations.

`preserveOnDisable` is required for user-data Applications and should normally be `true`.

Disabling an Application removes its active UI/runtime availability but must not silently delete data.

Uninstall semantics are not part of v1alpha1 because there is no public plugin package lifecycle yet.

## Health

Service/bridge Applications declare or expose:

- liveness;
- readiness;
- version/contract status;
- degraded dependency state where relevant.

Embedded Applications may use the main backend health model.

## Validation rules

The registry rejects a manifest if:

- Application ID is invalid/duplicate;
- required fields are missing;
- contract dependencies are unsatisfied;
- contribution IDs collide within an Application;
- contribution renderer/action references are unknown;
- event names are not namespace-valid;
- resource/action namespaces conflict with another owner;
- runtime kind is unknown;
- `data.owner` does not match ownership rules.

## Enablement

Application enable/disable is a tenant/workspace configuration operation, not executable hot unloading.

The registry must distinguish:

```text
installed/available implementation
configured for tenant/workspace
enabled/disabled
healthy/degraded/unavailable
```

These states must not be collapsed into one boolean once service-backed Applications exist.

## Migration from current Module metadata

Existing Module metadata maps approximately as follows:

| Current Module field | Application target |
|---|---|
| `module_key` | manifest/Application short identity + migration mapping |
| `display_name` | `metadata.name` |
| `description` | `metadata.description` |
| `enabled` | tenant/workspace Application enablement record |
| `renderer` | route/render Contribution |
| `default_template` | Notes/content-specific Application configuration |
| `icon`, sidebar/dashboard UI JSON | Contributions |
| `permissions` | migrated to Application grants/domain authorization config, not opaque permanent JSON |
| `ai_indexing` | Memory publication/index policy |
| `audit` | audit policy/config |
| `schema_version` | replaced by explicit manifest/contract/schema versions |

Do not preserve the old `Module` type as a permanent adapter. Data migration is temporary; the target model is Application-native.

## Non-goals

- third-party package installation;
- native dynamic library loading;
- WASM execution;
- dependency solver;
- marketplace;
- public compatibility guarantee across `v1alpha1` changes;
- automatic service process management.

## Acceptance tests

- Parse and validate first-party manifests.
- Reject duplicate IDs/contributions/action namespaces.
- Reject missing required contracts.
- Enable/disable an Application without deleting data.
- Compose navigation/routes/dashboard from Contributions.
- Change an Application runtime fixture from embedded to service without changing ResourceRefs/Application ID.
- Migrate representative current Module records into Application config without preserving legacy aliases in the final API model.
