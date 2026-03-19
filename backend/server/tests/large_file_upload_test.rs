use bytes::Bytes;
use reqwest::multipart::{Form, Part};
use serde_json::Value;

#[tokio::test]
async fn test_upload_large_file_10mb() {
    // Initialize test environment
    let base_url = std::env::var("TEST_BASE_URL").unwrap_or_else(|_| "http://localhost:8080".to_string());
    let client = reqwest::Client::new();

    // Login to get auth token
    let login_response = client
        .post(format!("{}/api/auth/login", base_url))
        .json(&serde_json::json!({
            "email": "admin@localhost",
            "password": "admin123"
        }))
        .send()
        .await
        .expect("Failed to login");

    assert_eq!(login_response.status(), 200, "Login should succeed");

    let login_data: Value = login_response.json().await.expect("Failed to parse login response");
    let token = login_data["token"].as_str().expect("Token not found in response");

    // Create 10MB file in memory
    let file_size = 10 * 1024 * 1024; // 10MB
    let file_data = vec![0u8; file_size];
    let file_bytes = Bytes::from(file_data);

    // Create multipart form
    let form = Form::new()
        .part("file", Part::bytes(file_bytes.to_vec())
            .file_name("test-10mb.bin")
            .mime_str("application/octet-stream")
            .unwrap())
        .text("name", "test-10mb.bin");

    // Upload file
    let upload_response = client
        .post(format!("{}/api/files/upload", base_url))
        .header("Authorization", format!("Bearer {}", token))
        .multipart(form)
        .send()
        .await
        .expect("Failed to upload file");

    assert_eq!(
        upload_response.status(),
        201,
        "Upload should return 201 Created. Status: {}, Body: {}",
        upload_response.status(),
        upload_response.text().await.unwrap_or_default()
    );

    let upload_data: Value = upload_response.json().await.expect("Failed to parse upload response");

    // Verify response contains expected fields
    assert!(upload_data["id"].is_string(), "Response should contain file ID");
    assert_eq!(upload_data["name"], "test-10mb.bin", "File name should match");
    assert_eq!(upload_data["size"], file_size, "File size should match");
    assert_eq!(upload_data["current_version"], 1, "Version should be 1");

    println!("✓ Successfully uploaded 10MB file with ID: {}", upload_data["id"]);
}

#[tokio::test]
async fn test_upload_file_size_boundary_5mb() {
    // Test at the 5MB boundary that was previously failing
    let base_url = std::env::var("TEST_BASE_URL").unwrap_or_else(|_| "http://localhost:8080".to_string());
    let client = reqwest::Client::new();

    // Login
    let login_response = client
        .post(format!("{}/api/auth/login", base_url))
        .json(&serde_json::json!({
            "email": "admin@localhost",
            "password": "admin123"
        }))
        .send()
        .await
        .expect("Failed to login");

    let login_data: Value = login_response.json().await.expect("Failed to parse login response");
    let token = login_data["token"].as_str().expect("Token not found in response");

    // Create exactly 5MB file
    let file_size = 5 * 1024 * 1024; // 5MB
    let file_data = vec![0xABu8; file_size]; // Use non-zero bytes to differentiate
    let file_bytes = Bytes::from(file_data);

    let form = Form::new()
        .part("file", Part::bytes(file_bytes.to_vec())
            .file_name("test-5mb.bin")
            .mime_str("application/octet-stream")
            .unwrap())
        .text("name", "test-5mb.bin");

    // Upload file
    let upload_response = client
        .post(format!("{}/api/files/upload", base_url))
        .header("Authorization", format!("Bearer {}", token))
        .multipart(form)
        .send()
        .await
        .expect("Failed to upload file");

    assert_eq!(
        upload_response.status(),
        201,
        "5MB upload should succeed. Status: {}, Body: {}",
        upload_response.status(),
        upload_response.text().await.unwrap_or_default()
    );

    let upload_data: Value = upload_response.json().await.expect("Failed to parse upload response");
    assert_eq!(upload_data["size"], file_size, "File size should be exactly 5MB");

    println!("✓ Successfully uploaded 5MB file (previously failing boundary)");
}
