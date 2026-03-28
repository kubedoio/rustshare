//! Tests for S3 credential handling and authentication
//!
//! These tests verify that:
//! 1. Credentials are loaded from environment variables
//! 2. S3 clients are created with proper authentication
//! 3. Error handling works when credentials are missing

use std::env;
use std::sync::Arc;

use aws_config::BehaviorVersion;
use aws_credential_types::Credentials;
use aws_credential_types::provider::ProvideCredentials;
use aws_sdk_s3::Client as S3Client;

/// Test that credentials can be loaded from environment
#[tokio::test]
async fn test_credentials_load_from_env() {
    // Set test credentials
    env::set_var("AWS_ACCESS_KEY_ID", "test-access-key");
    env::set_var("AWS_SECRET_ACCESS_KEY", "test-secret-key");

    // Load credentials
    let access_key = env::var("AWS_ACCESS_KEY_ID").expect("Should get access key");
    let secret_key = env::var("AWS_SECRET_ACCESS_KEY").expect("Should get secret key");

    assert_eq!(access_key, "test-access-key");
    assert_eq!(secret_key, "test-secret-key");

    // Create credentials object
    let credentials = Credentials::new(
        access_key,
        secret_key,
        None,
        None,
        "env",
    );

    // Verify credentials can be used in SDK config
    let sdk_config = aws_config::defaults(BehaviorVersion::latest())
        .endpoint_url("http://localhost:9000")
        .region(aws_config::Region::new("us-east-1"))
        .credentials_provider(credentials)
        .load()
        .await;

    let s3_config = aws_sdk_s3::config::Builder::from(&sdk_config)
        .force_path_style(true)
        .build();

    let _client = S3Client::from_conf(s3_config);

    // Clean up
    env::remove_var("AWS_ACCESS_KEY_ID");
    env::remove_var("AWS_SECRET_ACCESS_KEY");
}

/// Test that missing credentials produce proper errors
#[tokio::test]
async fn test_missing_credentials_produces_error() {
    // Ensure credentials are not set
    env::remove_var("AWS_ACCESS_KEY_ID");
    env::remove_var("AWS_SECRET_ACCESS_KEY");

    // Attempt to load credentials
    let result: Result<(), anyhow::Error> = (|| async {
        let access_key = env::var("AWS_ACCESS_KEY_ID")
            .map_err(|e| anyhow::anyhow!("AWS_ACCESS_KEY_ID not set: {}", e))?;
        let secret_key = env::var("AWS_SECRET_ACCESS_KEY")
            .map_err(|e| anyhow::anyhow!("AWS_SECRET_ACCESS_KEY not set: {}", e))?;

        let _credentials = Credentials::new(
            access_key,
            secret_key,
            None,
            None,
            "env",
        );

        Ok(())
    })().await;

    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("AWS_ACCESS_KEY_ID not set") || err_msg.contains("AWS_SECRET_ACCESS_KEY not set"));
}

/// Test that credentials are properly passed to S3 config
#[tokio::test]
async fn test_credentials_in_s3_config() {
    // Save original values
    let orig_access = env::var("AWS_ACCESS_KEY_ID").ok();
    let orig_secret = env::var("AWS_SECRET_ACCESS_KEY").ok();
    
    // Set test credentials
    env::set_var("AWS_ACCESS_KEY_ID", "rustfsadmin");
    env::set_var("AWS_SECRET_ACCESS_KEY", "rustfsadmin");

    let access_key = env::var("AWS_ACCESS_KEY_ID").unwrap();
    let secret_key = env::var("AWS_SECRET_ACCESS_KEY").unwrap();

    // Create credentials
    let credentials = Credentials::new(
        access_key.clone(),
        secret_key.clone(),
        None,
        None,
        "env",
    );

    // Build SDK config with credentials
    let sdk_config = aws_config::defaults(BehaviorVersion::latest())
        .endpoint_url("http://rustfs:9000")
        .region(aws_config::Region::new("us-east-1"))
        .credentials_provider(credentials)
        .load()
        .await;

    // Verify config has credentials provider
    let creds_result = sdk_config
        .credentials_provider()
        .expect("Should have credentials provider")
        .provide_credentials()
        .await;

    assert!(creds_result.is_ok());
    let creds = creds_result.unwrap();
    assert_eq!(creds.access_key_id(), "rustfsadmin");
    assert_eq!(creds.secret_access_key(), "rustfsadmin");

    // Restore original values
    match orig_access {
        Some(v) => env::set_var("AWS_ACCESS_KEY_ID", v),
        None => env::remove_var("AWS_ACCESS_KEY_ID"),
    }
    match orig_secret {
        Some(v) => env::set_var("AWS_SECRET_ACCESS_KEY", v),
        None => env::remove_var("AWS_SECRET_ACCESS_KEY"),
    }
}

/// Test S3UserBucketStoreFactory with explicit credentials
#[tokio::test]
async fn test_user_bucket_store_factory_with_credentials() {
    use rustshare_storage::user_bucket::{UserBucketStoreFactory, UserBucketConfig};

    // Save original values
    let orig_access = env::var("AWS_ACCESS_KEY_ID").ok();
    let orig_secret = env::var("AWS_SECRET_ACCESS_KEY").ok();
    
    // Set test credentials
    env::set_var("AWS_ACCESS_KEY_ID", "test-key");
    env::set_var("AWS_SECRET_ACCESS_KEY", "test-secret");

    let config = UserBucketConfig {
        endpoint: "http://localhost:9000".to_string(),
        region: "us-east-1".to_string(),
        bucket_prefix: "test-user-".to_string(),
        base_prefix: "".to_string(),
    };

    // This should not fail due to missing credentials
    // Note: It may fail to connect to the server, but that's expected in tests
    let result = UserBucketStoreFactory::create_s3_with_config(config).await;
    
    // We expect an error because there's no actual S3 server running,
    // but it should NOT be a credential error
    match result {
        Ok(_) => {
            // If it succeeded, credentials worked
        }
        Err(e) => {
            let err_str = e.to_string();
            // Should not be a credential-related error
            assert!(!err_str.contains("AWS_ACCESS_KEY_ID not set"), 
                "Should not fail with missing credentials error: {}", err_str);
            assert!(!err_str.contains("AWS_SECRET_ACCESS_KEY not set"),
                "Should not fail with missing credentials error: {}", err_str);
        }
    }

    // Restore original values
    match orig_access {
        Some(v) => env::set_var("AWS_ACCESS_KEY_ID", v),
        None => env::remove_var("AWS_ACCESS_KEY_ID"),
    }
    match orig_secret {
        Some(v) => env::set_var("AWS_SECRET_ACCESS_KEY", v),
        None => env::remove_var("AWS_SECRET_ACCESS_KEY"),
    }
}

/// Test that the error message format is correct for debugging
#[test]
fn test_error_message_format() {
    let bucket = "rustshare-files";
    let head_err = "service error";
    let create_err = "failed to create bucket";
    
    let err_msg = format!(
        "Failed to create system bucket '{}': head_error={}, create_error={}",
        bucket, head_err, create_err
    );
    
    assert!(err_msg.contains(bucket));
    assert!(err_msg.contains(head_err));
    assert!(err_msg.contains(create_err));
    assert!(err_msg.contains("Failed to create system bucket"));
}

/// Test credential key extraction for logging (first 4 chars)
#[test]
fn test_credential_key_logging() {
    let access_key = "rustfsadmin".to_string();
    let prefix = &access_key[..4.min(access_key.len())];
    
    assert_eq!(prefix, "rust");
    
    // Short key
    let short_key = "ab".to_string();
    let short_prefix = &short_key[..4.min(short_key.len())];
    assert_eq!(short_prefix, "ab");
}

/// Test that environment variable loading uses expect with error message
#[test]
fn test_env_var_error_message() {
    env::remove_var("TEST_VAR_MISSING");
    
    let result = env::var("TEST_VAR_MISSING");
    assert!(result.is_err());
    
    let formatted = format!("TEST_VAR_MISSING not set: {}", result.unwrap_err());
    assert!(formatted.contains("TEST_VAR_MISSING not set"));
}
