use axum::http::StatusCode;
use rustshare_server::AppState;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use futures_util::{SinkExt, StreamExt};

mod common;

/// Helper to create authenticated WebSocket connection
async fn connect_websocket(token: &str, base_url: &str) -> Result<tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>, String> {
    let url = format!("{}/api/sync", base_url.replace("http://", "ws://"));
    let (ws_stream, _) = connect_async(
        tokio_tungstenite::tungstenite::http::Request::builder()
            .uri(&url)
            .header("Authorization", format!("Bearer {}", token))
            .body(())
            .unwrap()
    )
    .await
    .map_err(|e| e.to_string())?;

    Ok(ws_stream)
}

#[tokio::test]
#[ignore] // TODO: Implement test server setup
async fn test_connect_with_valid_jwt() {
    let (state, base_url) = common::setup_test_server().await;
    let token = common::create_test_token(&state);

    let result = connect_websocket(&token, &base_url).await;
    assert!(result.is_ok(), "Should connect with valid JWT");
}

#[tokio::test]
#[ignore] // TODO: Implement test server setup
async fn test_connect_without_jwt() {
    let (_state, base_url) = common::setup_test_server().await;

    let url = format!("{}/api/sync", base_url.replace("http://", "ws://"));
    let result = connect_async(url).await;

    assert!(result.is_err(), "Should fail without JWT");
}

#[tokio::test]
#[ignore] // TODO: Implement test server setup
async fn test_receive_notification_on_upload() {
    let (state, base_url) = common::setup_test_server().await;
    let token = common::create_test_token(&state);
    let user_id = common::get_user_id_from_token(&token, &state);

    // Connect WebSocket
    let (mut ws_stream, _) = connect_websocket(&token, &base_url).await.unwrap();

    // Upload a file via HTTP
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/api/files/upload", base_url))
        .header("Authorization", format!("Bearer {}", token))
        .multipart(common::create_test_file_upload("test.txt", b"test content"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // Wait for WebSocket notification
    let timeout = tokio::time::sleep(tokio::time::Duration::from_secs(2));
    tokio::pin!(timeout);

    let notification = tokio::select! {
        msg = ws_stream.next() => {
            match msg {
                Some(Ok(Message::Text(text))) => text,
                _ => panic!("Expected text message"),
            }
        }
        _ = &mut timeout => panic!("Timeout waiting for notification"),
    };

    // Verify notification format
    let json: serde_json::Value = serde_json::from_str(&notification).unwrap();
    assert_eq!(json["event_type"], "FileUploaded");
    assert!(json["event_id"].is_string());
    assert!(json["aggregate_id"].is_string());
}

#[tokio::test]
#[ignore] // TODO: Implement test server setup
async fn test_multiple_devices_receive_notification() {
    let (state, base_url) = common::setup_test_server().await;
    let token = common::create_test_token(&state);

    // Connect 3 WebSocket clients
    let (mut ws1, _) = connect_websocket(&token, &base_url).await.unwrap();
    let (mut ws2, _) = connect_websocket(&token, &base_url).await.unwrap();
    let (mut ws3, _) = connect_websocket(&token, &base_url).await.unwrap();

    // Upload a file
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/api/files/upload", base_url))
        .header("Authorization", format!("Bearer {}", token))
        .multipart(common::create_test_file_upload("test.txt", b"test content"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // All 3 clients should receive notification
    let timeout = tokio::time::Duration::from_secs(2);

    let msg1 = tokio::time::timeout(timeout, ws1.next()).await.unwrap().unwrap().unwrap();
    let msg2 = tokio::time::timeout(timeout, ws2.next()).await.unwrap().unwrap().unwrap();
    let msg3 = tokio::time::timeout(timeout, ws3.next()).await.unwrap().unwrap().unwrap();

    assert!(matches!(msg1, Message::Text(_)));
    assert!(matches!(msg2, Message::Text(_)));
    assert!(matches!(msg3, Message::Text(_)));
}
