# Vault Blob Lifecycle Audit (2026)

## Scope and safety conclusion

RustShare uses one global content-addressed namespace, `blobs/<lowercase
sha256>`, for normal files, historical file versions, vault files, imported and
generated mail artifacts, and completed resumable uploads. A safe reference
check therefore cannot be tenant-scoped and must cover every durable reference
below.

Production deletion of that namespace is currently disabled. The existing
`object_gc_queue` and retention worker intentionally exclude content-addressed
keys because a database-only reference check can race a writer between its
object-store put and metadata commit. Enabling deletion requires one
cross-process exclusion mechanism shared by all `blobs/<sha256>` writers and
the collector, in addition to the grace period and immediate reference
re-check.

## Approved and separate namespaces

| Namespace | Purpose | GC disposition |
|---|---|---|
| `blobs/<64 lowercase hex>` | Globally deduplicated durable content | The only namespace this phase may delete |
| `thumbnails/<file UUID>/<sm|md|lg>.webp` | Derived thumbnails | Separate lifecycle; excluded |
| `temp/uploads/<session UUID>/<chunk>` | Resumable upload chunks | Upload-session cleanup only; excluded |
| `shared/blobs/sha256/...` | Metadata document-store internals | Separate backend namespace; excluded |
| Any other key | Legacy, test, or feature-specific object | Invalid for blob GC; never delete |

Uppercase SHA-256 keys are not produced by current writers and are outside the
approved shape.

## Durable reference inventory

| Table/domain object | Column/field | Namespace | Lifecycle | Retention behavior | Version behavior | Tenant scope | Existing cleanup | Transaction behavior | Tests |
|---|---|---|---|---|---|---|---|---|---|
| Normal files and note files (`files`) | `storage_key`; `content_hash` derives the same key | `blobs/<sha256>` | Created/replaced by `FileService`; soft-trash/restore; hard-delete after retention | A soft-deleted row continues to protect content until hard deletion | Current file row remains valid independently of versions | Row is tenant-owned, key is global | DB triggers enqueue removed/replaced keys | Blob put precedes atomic metadata/event update | file service, trash, upload, notes, storage contracts |
| File versions (`file_versions`) | `storage_key`; `content_hash` | `blobs/<sha256>` | Created for each durable version; removed by version or file retention | Every extant version protects the blob, including history of trashed files | Historical versions are first-class references | Row inherits file tenancy, key is global | DB triggers enqueue removed/replaced keys | Version/current-file writes are committed atomically; object put is earlier | file version and retention tests |
| Vault files (`vault_files`) | `sha256`; key is derived as `blobs/<sha256>` | `blobs/<sha256>` | Create/update, tombstone, restore-by-write, delete with vault | Both live and retained tombstone rows with a non-NULL hash protect content | No separate vault version table; the retained row holds the last hash | Row is tenant/vault scoped, key is global | No existing enqueue trigger | Object put intentionally precedes conditional update/insert | vault service and HTTP/contract tests |
| Mail message source (`mail_messages`) | `blob_key` | Usually `blobs/<sha256>` | EML upload, inbound/import, draft save, sent-copy creation, hard deletion | Soft-deleted messages continue to protect their source until hard deletion | Message artifact remains valid while row exists | Row is tenant/owner scoped, key is global | DB trigger enqueues removed/replaced keys | Blob is written before message persistence in relevant flows | mail import/read/send/draft tests |
| Legacy/external mail source alias (`mail_messages`) | `object_key` | Exact stored key; may alias the same blob | Imported object reference | Row protects until hard deletion | No separate versions | Row is tenant/owner scoped; key may be global | DB trigger enqueues removed/replaced keys | Metadata transaction owns alias change | mail import/read tests |
| Parsed mail parts (`mail_message_parts`) | `blob_key` | `blobs/<sha256>` when body stored externally | Created during parse/import; cascades with message | Row protects until cascade/hard deletion | Immutable parsed artifact | Tenant row, global key | DB trigger enqueues removed/replaced keys | Part metadata is committed after blob write | mail read/import tests |
| Mail attachments (`mail_attachments`) | `blob_key` | `blobs/<sha256>` | Parsed/imported/generated attachment; cascades with message | Row protects until cascade/hard deletion | Immutable attachment artifact | Tenant row, global key | DB trigger enqueues removed/replaced keys | Attachment metadata follows blob write | mail attachment/read tests |
| Draft attachments | File IDs embedded in draft metadata; backing content is protected by `files` and `file_versions`; generated draft EML is protected by `mail_messages.blob_key` | `blobs/<sha256>` through those rows | Attach/detach/save/discard/send | Backing file/history and retained draft row govern protection | File-version rules apply | Tenant rows, global key | Existing file/mail triggers | Draft validation precedes artifact persistence | draft attachment and send-idempotency tests |
| Replication jobs (`replication_jobs`) | `storage_key` | Source is commonly `blobs/<sha256>` | Queued/retrying/syncing/completed/failed | Queued, retrying, syncing, and permanently failed work that may still require the source protects it; completed jobs do not add protection beyond file/version rows | Job points to a specific file version where available | No tenant key partition; source key is global | History retention removes old completed jobs | Jobs are durably leased with `SKIP LOCKED` | replication worker/contract tests |

## Features without an additional durable blob reference

| Feature | Finding |
|---|---|
| Notes and note bundles | Note content and bundle members are normal `files`/`file_versions`; sidecar metadata does not store another object key. |
| Note attachments | Attachment metadata points to RustShare file identities; the referenced bytes are protected by file/version rows. |
| Thumbnails | `file_thumbnails.storage_path` references `thumbnails/...`, not `blobs/...`; derived and excluded from this collector. |
| Temporary and resumable uploads | Chunks use `temp/uploads/...`. On successful assembly the durable result uses `blobs/<sha256>` and must be protected by the resulting file/version row. Active upload sessions do not name a final blob key durably; writer/GC exclusion protects the put-to-commit window. |
| Replication targets/attempts | Targets contain destination configuration; attempts reference jobs, not additional source keys. Active source protection comes from `replication_jobs.storage_key`. |
| Trash/tombstones | Normal files retain their row while trashed. Vault tombstones retain `sha256`. Mail soft deletes retain their rows. These existing rows remain references; GC must not filter them out. |
| Public shares and aliases | Shares point to files/folders, not object keys. Their content is protected by the referenced file/version rows. |
| AI/vector indexes | Store text/vector metadata and file/note IDs, not durable source object keys. |
| Event/audit records | May contain hashes or descriptions but are not authoritative restore/read references and must not be counted. |

## Writers and orphan-creation paths

All durable blob writers must participate in the shared writer/collector
exclusion boundary, not only vault sync:

- `FileService` writes `blobs/<hash>` before file/version metadata creation or
  replacement.
- `UploadService` assembles chunks into `blobs/<hash>` before final file
  metadata is established.
- `VaultSyncService` writes before its conditional update or insert.
- `MailService` writes source EML, parsed bodies, and attachments before their
  durable mail rows.

The confirmed vault orphan paths are:

1. Blob put succeeds and `update_file_conditional_atomic` returns `None`
   because the base revision lost a race.
2. Blob put succeeds and `insert_file_atomic` loses the concurrent unique-path
   create race.
3. Blob put succeeds and either metadata operation returns a database error.

Immediate deletion is unsafe because the same hash may already be referenced,
another writer may be between put and metadata commit, and the key is globally
deduplicated across tenants and features.

## Existing cleanup and gaps

Migration `20260717000000_reference_aware_blob_gc.sql` provides a minimal
`object_gc_queue`, 24-hour delay, change triggers for file/version/mail keys,
and reference indexes. The retention worker deletes non-content-addressed
candidates only after a reference check. It deliberately omits `blobs/...`.

Before content-addressed deletion can be enabled, the implementation must:

- add explicit candidate states, reasons, attempts, leases, errors, and audit
  timestamps without destroying the existing queue;
- coalesce re-enqueues without shortening `not_before`;
- add vault and replication references to the global structured query;
- enqueue all vault post-put failures while preserving their API result;
- coordinate all content-addressed writers with GC across processes;
- lease due candidates atomically and reclaim stale leases;
- validate exact lowercase keys, apply a configurable grace period, check
  references twice, and fail closed on every database or object-store error;
- treat missing objects as an idempotent completion and expose backlog/failure
  metrics.

No production deletion behavior was changed while producing this audit.
