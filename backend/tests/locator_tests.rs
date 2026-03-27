//! Portable Storage Locator Contract Tests
//!
//! Tests that verify:
//! - Locator serializes correctly
//! - Locator deserializes correctly
//! - Locator endpoint can be remapped
//! - Cross-bucket read via locator works

use crate::*;

/// Test PL-01: Locator serialization
///
/// Verify locator serializes to correct JSON structure
#[test]
fn test_pl_01_locator_serialization() {
    let locator = PortableStorageLocator {
        locator_version: 1,
        storage_provider_kind: "s3".to_string(),
        endpoint_ref: "primary".to_string(),
        bucket: "rustshare-user-123".to_string(),
        key: "owned/files/abc.json".to_string(),
        resource_type: "file".to_string(),
        resource_id: Uuid::new_v4(),
        version_id: None,
        content_hash: Some("sha256:abc123".to_string()),
    };

    let json = serde_json::to_string(&locator).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed["locator_version"], 1);
    assert_eq!(parsed["storage_provider_kind"], "s3");
    assert_eq!(parsed["endpoint_ref"], "primary");
    assert!(parsed["bucket"].as_str().unwrap().contains("rustshare-user"));
    assert_eq!(parsed["resource_type"], "file");
    assert!(parsed["content_hash"].is_string());
}

/// Test PL-02: Locator deserialization
///
/// Verify locator deserializes correctly from JSON
#[test]
fn test_pl_02_locator_deserialization() {
    let resource_id = Uuid::new_v4();
    let json = format!(
        r#"{{
            "locator_version": 1,
            "storage_provider_kind": "s3",
            "endpoint_ref": "primary",
            "bucket": "rustshare-user-123",
            "key": "owned/files/abc.json",
            "resource_type": "file",
            "resource_id": "{}",
            "version_id": null,
            "content_hash": "sha256:abc123"
        }}"#,
        resource_id
    );

    let locator: PortableStorageLocator = serde_json::from_str(&json).unwrap();

    assert_eq!(locator.locator_version, 1);
    assert_eq!(locator.storage_provider_kind, "s3");
    assert_eq!(locator.resource_type, "file");
    assert_eq!(locator.resource_id, resource_id);
    assert!(locator.content_hash.is_some());
    assert_eq!(locator.content_hash.unwrap(), "sha256:abc123");
}

/// Test PL-03: Locator endpoint remap
///
/// Verify locator endpoint can be remapped for relocation
#[test]
fn test_pl_03_locator_endpoint_remap() {
    let locator = PortableStorageLocator {
        locator_version: 1,
        storage_provider_kind: "s3".to_string(),
        endpoint_ref: "primary".to_string(),
        bucket: "rustshare-user-123".to_string(),
        key: "owned/files/abc.json".to_string(),
        resource_type: "file".to_string(),
        resource_id: Uuid::new_v4(),
        version_id: None,
        content_hash: None,
    };

    // Simulate relocation - remap endpoint
    let mut relocated = locator.clone();
    relocated.endpoint_ref = "eu-west".to_string();
    relocated.bucket = "rustshare-user-123-eu".to_string();

    assert_eq!(relocated.endpoint_ref, "eu-west");
    assert_eq!(relocated.bucket, "rustshare-user-123-eu");
    assert_eq!(relocated.resource_id, locator.resource_id); // Same resource
    assert_eq!(relocated.key, locator.key); // Same key
}

/// Test PL-04: Cross-bucket read via locator
///
/// Verify cross-bucket read using locator works
#[tokio::test]
async fn test_pl_04_cross_bucket_read_via_locator() {
    let ctx = TestContext::new().await;
    let owner_id = Uuid::new_v4();
    let reader_id = Uuid::new_v4();
    ctx.create_user(owner_id).await.unwrap();
    ctx.create_user(reader_id).await.unwrap();

    let file = ctx
        .file_service()
        .upload_file(
            owner_id,
            "cross.txt".to_string(),
            None,
            Bytes::from("content"),
            "text/plain".to_string(),
        )
        .await
        .unwrap();

    // Create locator pointing to owner's file
    let locator = PortableStorageLocator {
        locator_version: 1,
        storage_provider_kind: "s3".to_string(),
        endpoint_ref: "primary".to_string(),
        bucket: format!("rustshare-user-{}", owner_id),
        key: format!("owned/files/{}.json", file.id),
        resource_type: "file".to_string(),
        resource_id: file.id,
        version_id: None,
        content_hash: Some(format!("sha256:{}", file.content_hash)),
    };

    // Read via locator
    let data = ctx.cross_bucket.read_with_locator(&locator).await.unwrap();
    assert!(data.is_some(), "Should be able to read via locator");

    let doc: FileDocument = serde_json::from_slice(&data.unwrap()).unwrap();
    assert_eq!(doc.id, file.id);
    assert_eq!(doc.name, "cross.txt");
}

/// Test PL-05: Locator check returns correct status
///
/// Verify check_locator returns correct availability status
#[tokio::test]
async fn test_pl_05_locator_check_returns_correct_status() {
    let ctx = TestContext::new().await;
    let user_id = Uuid::new_v4();
    ctx.create_user(user_id).await.unwrap();

    let file = ctx
        .file_service()
        .upload_file(
            user_id,
            "exists.txt".to_string(),
            None,
            Bytes::from("content"),
            "text/plain".to_string(),
        )
        .await
        .unwrap();

    // Existing resource locator
    let existing_locator = PortableStorageLocator {
        locator_version: 1,
        storage_provider_kind: "s3".to_string(),
        endpoint_ref: "primary".to_string(),
        bucket: format!("rustshare-user-{}", user_id),
        key: format!("owned/files/{}.json", file.id),
        resource_type: "file".to_string(),
        resource_id: file.id,
        version_id: None,
        content_hash: None,
    };

    // Non-existing resource locator
    let non_existing_locator = PortableStorageLocator {
        locator_version: 1,
        storage_provider_kind: "s3".to_string(),
        endpoint_ref: "primary".to_string(),
        bucket: format!("rustshare-user-{}", user_id),
        key: "owned/files/non-existing.json".to_string(),
        resource_type: "file".to_string(),
        resource_id: Uuid::new_v4(),
        version_id: None,
        content_hash: None,
    };

    assert!(
        ctx.cross_bucket.check_locator(&existing_locator).await.unwrap(),
        "Existing locator should return true"
    );

    assert!(
        !ctx.cross_bucket.check_locator(&non_existing_locator).await.unwrap(),
        "Non-existing locator should return false"
    );
}

/// Test PL-06: Locator with content hash verification
///
/// Verify locator can be used with content hash for integrity
#[tokio::test]
async fn test_pl_06_locator_content_hash_verification() {
    let ctx = TestContext::new().await;
    let user_id = Uuid::new_v4();
    ctx.create_user(user_id).await.unwrap();

    let content = Bytes::from("content for hash verification");
    let file = ctx
        .file_service()
        .upload_file(
            user_id,
            "hashed.txt".to_string(),
            None,
            content.clone(),
            "text/plain".to_string(),
        )
        .await
        .unwrap();

    let locator = PortableStorageLocator {
        locator_version: 1,
        storage_provider_kind: "s3".to_string(),
        endpoint_ref: "primary".to_string(),
        bucket: format!("rustshare-user-{}", user_id),
        key: format!("owned/files/{}.json", file.id),
        resource_type: "file".to_string(),
        resource_id: file.id,
        version_id: None,
        content_hash: Some(format!("sha256:{}", file.content_hash)),
    };

    // Read and verify
    let data = ctx.cross_bucket.read_with_locator(&locator).await.unwrap();
    assert!(data.is_some());

    let doc: FileDocument = serde_json::from_slice(&data.unwrap()).unwrap();

    // Verify content hash matches
    if let Some(expected_hash) = &locator.content_hash {
        let actual_hash = format!("sha256:{}", doc.content_hash);
        assert_eq!(&actual_hash, expected_hash, "Content hash should match");
    }
}
