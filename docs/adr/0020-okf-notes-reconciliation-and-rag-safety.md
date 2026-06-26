# ADR-0020: OKF Notes Reconciliation and RAG Safety Rules

Status: Accepted  
Date: 2026-06-26  
Owner: RustShare Core Team  
Related: ADR-0019, Issue #118

## Context

ADR-0019 defines Notes as OKF documents. Because RustShare is file-backed, users may also edit `note.md`, YAML frontmatter, or note folder names with external IDEs, sync tools, or Obsidian-like workflows.

This requires stricter rules for identity, reconciliation, migration safety, and permission-aware RAG.

## Decision

RustShare OKF Notes must support external file manipulation safely.

The stable identity of a note is not the folder name, manifest title, YAML title, or Markdown H1. The stable identity is:

```yaml
rustshare:
  id: <stable-note-uuid>
```

`rustshare.id` must be the absolute source of truth for note identity and graph links.

## Source-of-truth hierarchy

Use this hierarchy:

```text
1. rustshare.id in OKF YAML frontmatter = identity source of truth
2. RustShare permission model = access-control source of truth
3. note.md OKF frontmatter = knowledge metadata source when it is the newest edited component
4. _rustshare/manifest.json = RustShare operational cache / compatibility metadata
5. folder name = path/display name, externally renameable
6. Markdown H1 = body content only
```

Changing the first H1 must never rename the note.

## Reconciliation rules

On scan, sync, or open, RustShare must reconcile folder name, manifest, and YAML frontmatter.

### YAML changed externally

If YAML frontmatter was updated more recently than manifest/folder metadata:

1. keep `rustshare.id` unchanged;
2. parse and validate YAML;
3. update `_rustshare/manifest.json` from YAML metadata;
4. update note display name/folder name from YAML `title` when safe;
5. preserve unknown frontmatter fields;
6. enqueue RAG reindexing for the note.

### Folder renamed externally

If the note folder was renamed externally and the folder rename is newer than YAML/manifest title:

1. keep `rustshare.id` unchanged;
2. update YAML `title` from the folder/display name;
3. update `rustshare.bundle_name`;
4. update `_rustshare/manifest.json`;
5. enqueue a RAG metadata update.

### Conflict handling

If YAML title and folder name were both changed since the last known sync, RustShare must not silently overwrite either side.

Required behavior:

- create a sync conflict record;
- show a visible conflict state in the UI;
- keep the note readable where possible;
- do not rename folders or rewrite YAML until resolved;
- provide manual resolution: prefer YAML, prefer folder name, or custom title.

Duplicate or changed `rustshare.id` values must be rejected or placed into a conflict state unless the operation is a controlled import.

## Permission-aware RAG rules

Permission-aware RAG must use pre-filtering at the vector/full-text database level.

Post-retrieval filtering is not sufficient because it can produce empty or truncated context windows and can bias retrieval toward inaccessible chunks before filtering.

Every indexed chunk must include filterable ACL metadata.

Required chunk payload shape:

```json
{
  "tenant_id": "<tenant-id>",
  "workspace_id": "<workspace-id>",
  "note_id": "<rustshare.id>",
  "source_file_id": "<file-id>",
  "source_folder_id": "<folder-id>",
  "read_acl": ["group_engineering", "user_123"],
  "visibility": "private",
  "acl_hash": "<acl-hash>",
  "acl_version": 7,
  "embedding_policy": "allowed"
}
```

Retrieval must filter by ACL metadata before chunks are returned to the answer builder.

When note permissions change:

1. persist the permission change in RustShare;
2. enqueue an async ACL projection update job;
3. update `read_acl`, `acl_hash`, and `acl_version` for all chunks belonging to the note;
4. exclude stale chunks whose ACL version is older than the current RustShare ACL version.

RustShare permissions remain the source of truth. OKF metadata may describe visibility, but it must never grant access by itself.

## Migration safety rules

Migration must be dry-run capable:

```text
rustshare migrate-notes-okf --dry-run --format json
```

Dry-run must output a JSON report of intended changes without modifying files.

The report must include:

- notes scanned;
- notes already OKF-compatible;
- notes missing frontmatter;
- notes with existing frontmatter to merge;
- proposed generated `rustshare.id` values;
- proposed title changes;
- proposed manifest updates;
- detected conflicts;
- skipped files and skip reasons.

Migration must be idempotent:

- parse existing YAML frontmatter before injecting anything;
- never blindly prepend a second `---` block;
- merge manually-added frontmatter with RustShare-required keys;
- preserve unknown frontmatter fields;
- never overwrite a valid user-provided `rustshare.id`;
- re-running migration after a clean migration must produce no new changes.

## Acceptance criteria

- [ ] `rustshare.id` exists in OKF note frontmatter and is stable.
- [ ] Graph links and RAG provenance use `rustshare.id`, not path.
- [ ] External YAML edits reconcile manifest and folder name when YAML is newer.
- [ ] External folder renames reconcile YAML title and manifest when folder rename is newer.
- [ ] Conflicting external edits create a visible conflict state.
- [ ] Vector/full-text retrieval pre-filters by ACL metadata.
- [ ] Permission changes enqueue chunk ACL metadata updates.
- [ ] Stale ACL chunks are excluded from retrieval.
- [ ] Migration supports `--dry-run --format json`.
- [ ] Migration merges existing frontmatter instead of double-wrapping.
- [ ] Migration is idempotent.
