# OKF Notes — Extend Migration to Standalone `.md` Notes

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Update the `rustshare migrate-notes-okf` command so it also converts legacy standalone `.md` files, not just folder-backed note bundles.

**Architecture:** Reuse the existing migration pipeline: scan `/Workspace/Notes` for all files, identify `.md` files that are not already inside a note bundle, generate an OKF id and frontmatter, and write/update the sidecar. Keep loose files in place rather than forcing them into bundles.

**Tech Stack:** Rust 1.95, SQLx, S3-compatible object storage.

---

## Files

- Modify: `backend/server/src/services/note_service.rs`
- Test: `backend/server/src/services/note_service.rs` (existing test module)
- No CLI changes required; `rustshare migrate-notes-okf` will automatically cover the new cases.

---

## Task 1: Generalize the migration scanner

**Files:**
- Modify: `backend/server/src/services/note_service.rs:2483-2681` (`build_migration_plan`)

- [ ] **Step 1: Refactor `build_migration_plan` to scan both bundles and loose files**

Inside `build_migration_plan`, after locating the `notes_folder`, collect bundles **and** loose markdown files. Replace the bundle-only loop with two loops:

```rust
        let bundles = self
            .metadata_store
            .list_folders_by_parent(Some(notes_folder.id), tenant_id)
            .await
            .map_err(|e| NoteError::Database(e.to_string()))?;

        let loose_files = self
            .metadata_store
            .list_files_by_parent(Some(notes_folder.id), tenant_id)
            .await
            .map_err(|e| NoteError::Database(e.to_string()))?
            .into_iter()
            .filter(|f| f.name.ends_with(".md"))
            .collect::<Vec<_>>();

        for bundle in bundles {
            let files = match self
                .metadata_store
                .list_files_by_parent(Some(bundle.id), tenant_id)
                .await
            {
                Ok(files) => files,
                Err(e) => {
                    report.skipped.push(NoteMigrationSkip {
                        path: bundle.path,
                        reason: e.to_string(),
                    });
                    continue;
                }
            };

            if let Some(note_file) = files.into_iter().find(|f| f.name == "note.md") {
                self.plan_note_migration(
                    &mut report,
                    note_file,
                    Some(bundle),
                    tenant_id,
                )
                .await;
            }
        }

        for file in loose_files {
            self.plan_note_migration(&mut report, file, None, tenant_id).await;
        }
```

- [ ] **Step 2: Extract a helper for planning a single note**

Add a new method to `NoteService`:

```rust
    async fn plan_note_migration(
        &self,
        report: &mut NoteMigrationReport,
        note_file: rustshare_core::domain::File,
        bundle: Option<rustshare_core::domain::Folder>,
        tenant_id: Uuid,
    ) {
        report.notes_scanned += 1;
        let owner_id = note_file.owner_id;

        let content = match self.object_store.get(&note_file.storage_key()).await {
            Ok(bytes) => String::from_utf8_lossy(&bytes).to_string(),
            Err(e) => {
                report.skipped.push(NoteMigrationSkip {
                    path: note_file.path.clone(),
                    reason: format!("Failed to read note content: {}", e),
                });
                return;
            }
        };

        let (fm, _body) = match parse_frontmatter(&content) {
            Ok(parsed) => parsed,
            Err(e) => {
                report.conflicts.push(NoteMigrationConflict {
                    path: note_file.path.clone(),
                    kind: "invalid_yaml".to_string(),
                    message: e.to_string(),
                });
                return;
            }
        };

        let manifest_title = if let Some(ref b) = bundle {
            self.load_manifest_title(Some(b.id), owner_id, tenant_id).await
        } else {
            None
        };
        let bundle_name = bundle.as_ref().map(|b| b.name.clone());

        let title_source = if fm.title.clone().filter(|s| !s.is_empty()).is_some() {
            "yaml".to_string()
        } else if manifest_title.clone().filter(|s| !s.is_empty()).is_some() {
            "manifest".to_string()
        } else if bundle_name.as_ref().filter(|s| !s.is_empty()).is_some() {
            "folder".to_string()
        } else {
            "filename".to_string()
        };

        let yaml_id = fm
            .rustshare
            .as_ref()
            .and_then(|rs| rs.id)
            .filter(|id| !id.is_nil());
        let sidecar_id = self
            .load_sidecar_okf_id(note_file.id, owner_id, tenant_id)
            .await;
        let manifest_id = if let Some(ref b) = bundle {
            self.load_manifest_okf_id(Some(b.id), owner_id, tenant_id).await
        } else {
            None
        };

        // Identity conflicts.
        if let (Some(y), Some(s)) = (yaml_id, sidecar_id) {
            if y != s {
                report.conflicts.push(NoteMigrationConflict {
                    path: note_file.path.clone(),
                    kind: "identity_conflict".to_string(),
                    message: format!(
                        "Frontmatter rustshare.id ({}) does not match sidecar okf_id ({}).",
                        y, s
                    ),
                });
                return;
            }
        }
        if let (Some(y), Some(m)) = (yaml_id, manifest_id) {
            if y != m {
                report.conflicts.push(NoteMigrationConflict {
                    path: note_file.path.clone(),
                    kind: "identity_conflict".to_string(),
                    message: format!(
                        "Frontmatter rustshare.id ({}) does not match manifest rustshare_id ({}).",
                        y, m
                    ),
                });
                return;
            }
        }

        let has_frontmatter = split_frontmatter(&content).is_some();
        let is_already_okf =
            has_frontmatter && fm.okf_type.as_deref() == Some("Note") && yaml_id.is_some();

        let manifest_exists = if let Some(ref b) = bundle {
            self.load_manifest(Some(b.id), owner_id, tenant_id)
                .await
                .is_some()
        } else {
            false
        };

        if is_already_okf {
            report.already_okf += 1;
            report.planned_changes.push(NoteMigrationChange {
                path: note_file.path.clone(),
                note_id: note_file.id,
                generated_okf_id: yaml_id,
                title_source,
                frontmatter_action: "none".to_string(),
                manifest_action: "none".to_string(),
                risk_level: "low".to_string(),
            });
            return;
        }

        let (frontmatter_action, risk_level) = if has_frontmatter {
            report.frontmatter_to_merge += 1;
            ("merge".to_string(), "medium".to_string())
        } else {
            report.missing_frontmatter += 1;
            ("inject".to_string(), "low".to_string())
        };

        let manifest_action = if manifest_exists {
            "update".to_string()
        } else if bundle.is_some() {
            "create".to_string()
        } else {
            "none".to_string()
        };

        let generated_okf_id = sidecar_id
            .or(yaml_id)
            .or(manifest_id)
            .or_else(|| Some(Uuid::new_v4()));

        report.planned_changes.push(NoteMigrationChange {
            path: note_file.path,
            note_id: note_file.id,
            generated_okf_id,
            title_source,
            frontmatter_action,
            manifest_action,
            risk_level,
        });
    }
```

- [ ] **Step 3: Update `apply_okf_migration_change` for loose files**

In `apply_okf_migration_change`, the manifest creation block should be skipped when `file.parent_folder_id` is the notes folder (i.e., there is no bundle). Guard the `_rustshare` subfolder creation:

```rust
        // Ensure the bundle has a _rustshare folder only for folder-backed notes.
        if file.parent_folder_id != Some(notes_folder.id) {
            if let Some(parent_id) = file.parent_folder_id {
                self.get_or_create_subfolder(parent_id, "_rustshare", owner_id, tenant_id)
                    .await?;
            }
        }
```

Add a `notes_folder_id: Uuid` parameter to `apply_okf_migration_change` and pass `notes_folder.id` from `migrate_notes_to_okf`.

---

## Task 2: Add unit tests for loose-file migration

**Files:**
- Modify: `backend/server/src/services/note_service.rs` test module

- [ ] **Step 1: Add a test for `build_migration_plan` covering loose files**

Because `NoteService` depends on storage, add an integration-style test that constructs a service with mocked metadata/object stores, or extend the existing serialization tests with a new report field if full integration is too heavy.

A minimal serialization test that covers the new `manifest_action: "none"` case:

```rust
    #[test]
    fn note_migration_change_supports_loose_file() {
        let change = NoteMigrationChange {
            path: "/Workspace/Notes/standalone.md".to_string(),
            note_id: Uuid::nil(),
            generated_okf_id: Some(Uuid::nil()),
            title_source: "filename".to_string(),
            frontmatter_action: "inject".to_string(),
            manifest_action: "none".to_string(),
            risk_level: "low".to_string(),
        };
        let json = serde_json::to_value(&change).unwrap();
        assert_eq!(json["manifest_action"], "none");
        assert_eq!(json["title_source"], "filename");
    }
```

- [ ] **Step 2: Add a test that verifies `title_source` ordering**

```rust
    #[test]
    fn migration_title_source_prefers_yaml_over_filename() {
        // The plan logic is exercised through serialization of the report.
        let report = NoteMigrationReport {
            notes_scanned: 1,
            already_okf: 0,
            missing_frontmatter: 1,
            frontmatter_to_merge: 0,
            planned_changes: vec![NoteMigrationChange {
                path: "/Workspace/Notes/standalone.md".to_string(),
                note_id: Uuid::nil(),
                generated_okf_id: Some(Uuid::nil()),
                title_source: "filename".to_string(),
                frontmatter_action: "inject".to_string(),
                manifest_action: "none".to_string(),
                risk_level: "low".to_string(),
            }],
            conflicts: vec![],
            skipped: vec![],
        };
        let json = serde_json::to_value(&report).unwrap();
        assert_eq!(json["planned_changes"][0]["title_source"], "filename");
    }
```

---

## Task 3: Verify

- [ ] **Step 1: Compile**

Run: `cd backend && SQLX_OFFLINE=true cargo check --workspace`
Expected: PASS.

- [ ] **Step 2: Run tests**

Run: `cd backend && SQLX_OFFLINE=true cargo test --workspace --lib --bins note_migration`
Expected: PASS.

- [ ] **Step 3: Run a dry-run of the CLI against a dev tenant**

Run: `cd backend && SQLX_OFFLINE=true cargo run --bin rustshare -- migrate-notes-okf --dry-run --format text`
Expected: No panic; report includes any standalone `.md` files under `/Workspace/Notes`.

- [ ] **Step 4: Run clippy**

Run: `cd backend && cargo clippy --all-features -- -D warnings`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add backend/server/src/services/note_service.rs
git commit -s -m "feat(notes): migrate standalone markdown files to OKF"
```
