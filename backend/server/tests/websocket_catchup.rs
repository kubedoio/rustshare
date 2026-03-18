use tokio_tungstenite::tungstenite::Message;
use futures_util::{SinkExt, StreamExt};
use serde_json::json;

mod common;

#[tokio::test]
#[ignore] // TODO: Implement test server setup
async fn test_catchup_after_disconnect() {
    let (state, base_url) = common::setup_test_server().await;
    let token = common::create_test_token(&state);

    // Connect and get initial event
    let (mut ws, _) = common::connect_websocket(&token, &base_url).await.unwrap();

    // Upload file 1
    common::upload_test_file(&base_url, &token, "file1.txt").await;

    let msg = ws.next().await.unwrap().unwrap();
    let notification: serde_json::Value = serde_json::from_str(&msg.to_text().unwrap()).unwrap();
    let last_event_id = notification["event_id"].as_str().unwrap();

    // Disconnect
    ws.close(None).await.unwrap();

    // Perform 5 operations while disconnected
    for i in 2..=6 {
        common::upload_test_file(&base_url, &token, &format!("file{}.txt", i)).await;
    }

    // Reconnect and request catch-up
    let (mut ws, _) = common::connect_websocket(&token, &base_url).await.unwrap();

    let sync_request = json!({
        "type": "sync",
        "last_seen_event_id": last_event_id
    });
    ws.send(Message::Text(sync_request.to_string())).await.unwrap();

    // Should receive 5 catch-up events
    let mut events = Vec::new();
    for _ in 0..5 {
        let msg = tokio::time::timeout(
            tokio::time::Duration::from_secs(2),
            ws.next()
        ).await.unwrap().unwrap().unwrap();

        if let Message::Text(text) = msg {
            events.push(serde_json::from_str::<serde_json::Value>(&text).unwrap());
        }
    }

    assert_eq!(events.len(), 5);
    assert_eq!(events[0]["event_type"], "FileUploaded");
}

#[tokio::test]
#[ignore] // TODO: Implement test server setup
async fn test_catchup_with_invalid_id() {
    let (state, base_url) = common::setup_test_server().await;
    let token = common::create_test_token(&state);

    // Connect
    let (mut ws, _) = common::connect_websocket(&token, &base_url).await.unwrap();

    // Request catch-up with non-existent ID
    let sync_request = json!({
        "type": "sync",
        "last_seen_event_id": "00000000-0000-0000-0000-000000000000"
    });
    ws.send(Message::Text(sync_request.to_string())).await.unwrap();

    // Should not crash, should return empty or start from beginning
    // (Implementation detail - just verify it doesn't fail)
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
}
