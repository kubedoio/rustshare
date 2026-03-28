# RustShare Contract Test Matrix

**Version:** 1.0  
**Status:** Draft  

---

## Test Categories

### 1. Bucket Isolation Tests

| ID | Test Name | Description | Expected Result |
|----|-----------|-------------|-----------------|
| BI-01 | `user_a_file_not_in_user_b_bucket` | Upload file as User A, verify not in User B's bucket | File exists only in A's bucket |
| BI-02 | `user_b_cannot_access_user_a_file` | User B attempts to read User A's file directly | Permission denied / Not found |
| BI-03 | `shared_file_references_use_portable_locators` | Share from A to B, verify B has portable locator | Locator contains bucket, key, resource_type |
| BI-04 | `favourites_isolated_per_user` | User A and B star same shared file, verify independent storage | Separate favourite entries per user |
| BI-05 | `received_shares_in_recipient_bucket` | Share from A to B, verify received doc in B's bucket | ReceivedShareDocV2 in B's received/shares/ |

### 2. File Lifecycle Tests

| ID | Test Name | Description | Expected Result |
|----|-----------|-------------|-----------------|
| FL-01 | `upload_creates_file_doc` | Upload file, verify FileDocV2 created | File doc exists with correct fields |
| FL-02 | `upload_creates_version_doc` | Upload file, verify FileVersionDocV2 created | Version doc exists with correct fields |
| FL-03 | `upload_stores_blob_content_addressed` | Upload same content twice, verify single blob | Blob stored once by hash |
| FL-04 | `upload_updates_folder_children_index` | Upload to folder, verify index updated | File appears in folder's children index |
| FL-05 | `upload_updates_user_roots_index` | Upload to root, verify roots index updated | File appears in roots index |
| FL-06 | `get_file_returns_correct_metadata` | Get uploaded file, verify metadata matches | All fields correct |
| FL-07 | `list_files_uses_folder_index` | List folder contents, verify uses index | Returns files from children index |
| FL-08 | `rename_updates_file_doc_and_index` | Rename file, verify doc and index updated | New name in doc and index |
| FL-09 | `move_updates_parent_and_indexes` | Move file between folders, verify updates | Old index updated, new index updated |
| FL-10 | `delete_creates_tombstone` | Delete file, verify tombstone created | Tombstone doc exists |
| FL-11 | `delete_updates_indexes` | Delete file, verify removed from indexes | Not in folder children or roots |
| FL-12 | `restore_removes_tombstone` | Restore file, verify tombstone removed | Tombstone deleted |
| FL-13 | `restore_updates_indexes` | Restore file, verify re-added to indexes | Back in folder children or roots |
| FL-14 | `list_versions_returns_all_versions` | Upload multiple versions, verify list correct | All versions returned |

### 3. Folder Lifecycle Tests

| ID | Test Name | Description | Expected Result |
|----|-----------|-------------|-----------------|
| FO-01 | `create_folder_creates_doc` | Create folder, verify FolderDocV2 created | Folder doc exists |
| FO-02 | `create_folder_updates_parent_index` | Create subfolder, verify parent index updated | Folder in parent's children |
| FO-03 | `create_root_folder_updates_roots_index` | Create root folder, verify roots index | Folder in roots index |
| FO-04 | `list_children_returns_files_and_folders` | Get folder contents, verify both types | Files and folders returned |
| FO-05 | `rename_folder_updates_doc_and_child_paths` | Rename folder, verify doc and child paths | Path updated recursively |
| FO-06 | `move_folder_updates_parent_indexes` | Move folder, verify old and new parent indexes | Removed from old, added to new |
| FO-07 | `delete_folder_creates_tombstone` | Delete folder, verify tombstone created | Tombstone doc exists |
| FO-08 | `delete_folder_cascades_to_children` | Delete folder with contents, verify cascade | Children marked deleted |
| FO-09 | `restore_folder_restores_children` | Restore folder, verify children restored | Children no longer deleted |

### 4. Share Lifecycle Tests

| ID | Test Name | Description | Expected Result |
|----|-----------|-------------|-----------------|
| SL-01 | `create_share_creates_outbound_doc` | Create share, verify OutboundShareDocV2 | Outbound doc in sharer's bucket |
| SL-02 | `create_share_creates_received_doc` | Create share, verify ReceivedShareDocV2 | Received doc in recipient's bucket |
| SL-03 | `create_share_updates_recipient_shared_with_me` | Create share, verify recipient's index | Share in recipient's shared_with_me |
| SL-04 | `outbound_share_contains_portable_locator` | Verify outbound share has locator | Locator with bucket, key, type |
| SL-05 | `received_share_contains_portable_locator` | Verify received share has locator | Locator with bucket, key, type |
| SL-06 | `revoke_share_updates_outbound_doc` | Revoke share, verify outbound updated | Marked revoked or deleted |
| SL-07 | `revoke_share_updates_received_doc` | Revoke share, verify received updated | Marked revoked or deleted |
| SL-08 | `revoke_share_updates_recipient_index` | Revoke share, verify removed from index | Not in shared_with_me |
| SL-09 | `list_outbound_shares_returns_shares` | Create shares, verify list correct | All outbound shares returned |
| SL-10 | `list_received_shares_returns_shares` | Receive shares, verify list correct | All received shares returned |

### 5. Favourites Tests

| ID | Test Name | Description | Expected Result |
|----|-----------|-------------|-----------------|
| FV-01 | `star_owned_file_updates_favourites_index` | Star owned file, verify index | File in favourites index |
| FV-02 | `star_owned_folder_updates_favourites_index` | Star owned folder, verify index | Folder in favourites index |
| FV-03 | `star_received_share_updates_received_favourites` | Star received share, verify index | Share in received favourites |
| FV-04 | `unstar_removes_from_index` | Unstar item, verify removed | Not in favourites index |
| FV-05 | `star_does_not_modify_owner_file_doc` | Star shared file, verify owner doc unchanged | Owner's file doc not modified |
| FV-06 | `favourites_isolated_between_users` | User A stars, User B doesn't see | Only A's favourites index updated |
| FV-07 | `list_favourites_returns_correct_items` | Star multiple items, verify list | All starred items returned |

### 6. Portable Locator Tests

| ID | Test Name | Description | Expected Result |
|----|-----------|-------------|-----------------|
| PL-01 | `locator_serializes_correctly` | Create locator, verify JSON format | Correct JSON structure |
| PL-02 | `locator_deserializes_correctly` | Parse locator JSON, verify fields | All fields parsed correctly |
| PL-03 | `locator_contains_storage_alias` | Verify locator has storage_alias field | Field present and valid |
| PL-04 | `locator_contains_bucket_name` | Verify locator has bucket field | Field present and valid |
| PL-05 | `locator_contains_object_key` | Verify locator has key field | Field present and valid |
| PL-06 | `cross_bucket_read_uses_locator` | Read via locator, verify correct resource | Resource returned from correct bucket |

### 7. Request Scoping Tests

| ID | Test Name | Description | Expected Result |
|----|-----------|-------------|-----------------|
| RS-01 | `authenticated_request_uses_correct_bucket` | Request with valid auth, verify correct bucket | Operations use auth user's bucket |
| RS-02 | `no_system_user_fallback` | Verify no system user constant in request paths | No SYSTEM_USER_ID usage |
| RS-03 | `owned_operations_use_owner_bucket` | User modifies own file, verify owner bucket | Operations on owner's bucket |
| RS-04 | `share_creation_writes_to_both_buckets` | Create share, verify dual-write | Owner and recipient buckets modified |
| RS-05 | `received_operations_use_recipient_bucket` | Recipient lists shares, verify recipient bucket | Operations on recipient's bucket |

### 8. Bucket Provisioning Tests

| ID | Test Name | Description | Expected Result |
|----|-----------|-------------|-----------------|
| BP-01 | `user_creation_provisions_bucket` | Create user, verify bucket exists | Bucket created in RustFS |
| BP-02 | `provisioning_creates_required_indexes` | Provision bucket, verify indexes | All required indexes initialized |
| BP-03 | `provisioning_is_idempotent` | Provision twice, verify success | Second call succeeds, no duplicates |
| BP-04 | `provisioning_failure_fails_user_creation` | Force provision failure, verify user not created | User creation fails with clear error |
| BP-05 | `existing_bucket_not_reprovisioned` | Provision existing bucket, verify no data loss | Existing data preserved |

### 9. Redis Optionality Tests

| ID | Test Name | Description | Expected Result |
|----|-----------|-------------|-----------------|
| RO-01 | `standalone_mode_works_without_redis` | Run without Redis, verify functionality | All features work |
| RO-02 | `distributed_mode_uses_redis_for_coordination` | Run with Redis, verify coordination | Locks, leases use Redis |
| RO-03 | `redis_loss_does_not_destroy_durable_truth` | Stop Redis, verify data intact | All durable data in RustFS |
| RO-04 | `canonical_metadata_not_in_redis` | Verify no metadata stored in Redis | Only ephemeral data in Redis |

### 10. Stub Elimination Tests

| ID | Test Name | Description | Expected Result |
|----|-----------|-------------|-----------------|
| SE-01 | `file_handlers_not_stubbed` | Test file endpoints, verify real behavior | Real storage operations |
| SE-02 | `folder_handlers_not_stubbed` | Test folder endpoints, verify real behavior | Real storage operations |
| SE-03 | `share_handlers_not_stubbed` | Test share endpoints, verify real behavior | Real storage operations |
| SE-04 | `favourite_handlers_not_stubbed` | Test favourite endpoints, verify real behavior | Real storage operations |
| SE-05 | `device_handlers_not_stubbed` | Test device endpoints, verify real behavior | Real storage operations |
| SE-06 | `notification_handlers_not_stubbed` | Test notification endpoints, verify real behavior | Real storage operations |

---

## Test Implementation Priority

### Phase 1 (Critical Path)
- BI-01 through BI-05 (Bucket Isolation)
- FL-01 through FL-05 (File Upload)
- FO-01 through FO-03 (Folder Create)
- BP-01 through BP-03 (Bucket Provisioning)

### Phase 2 (Core Features)
- FL-06 through FL-14 (File Lifecycle)
- FO-04 through FO-09 (Folder Lifecycle)
- SL-01 through SL-05 (Share Creation)

### Phase 3 (Advanced Features)
- SL-06 through SL-10 (Share Revoke/List)
- FV-01 through FV-07 (Favourites)
- PL-01 through PL-06 (Portable Locators)

### Phase 4 (Quality)
- RS-01 through RS-05 (Request Scoping)
- BP-04 through BP-05 (Provisioning Edge Cases)
- RO-01 through RO-04 (Redis Optionality)
- SE-01 through SE-06 (Stub Elimination)

---

END OF CONTRACT TEST MATRIX
