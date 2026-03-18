use tokio_tungstenite::tungstenite::Message;
use futures_util::StreamExt;

mod common;

#[tokio::test]
#[ignore] // TODO: Implement test server setup
async fn test_broadcast_to_all_sessions() {
    let (state, base_url) = common::setup_test_server().await;
    let token = common::create_test_token(&state);

    // Connect 3 devices for same user
    let (mut ws1, _) = common::connect_websocket(&token, &base_url).await.unwrap();
    let (mut ws2, _) = common::connect_websocket(&token, &base_url).await.unwrap();
    let (mut ws3, _) = common::connect_websocket(&token, &base_url).await.unwrap();

    // Perform action from device 1 (via HTTP, not WebSocket)
    common::upload_test_file(&base_url, &token, "test.txt").await;

    // All 3 devices should receive notification
    let timeout = tokio::time::Duration::from_secs(2);

    let recv1 = tokio::time::timeout(timeout, ws1.next()).await;
    let recv2 = tokio::time::timeout(timeout, ws2.next()).await;
    let recv3 = tokio::time::timeout(timeout, ws3.next()).await;

    assert!(recv1.is_ok());
    assert!(recv2.is_ok());
    assert!(recv3.is_ok());

    // Verify all received same event
    let parse_event_id = |msg: Option<Result<Message, _>>| {
        let text = msg.unwrap().unwrap().into_text().unwrap();
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        json["event_id"].as_str().unwrap().to_string()
    };

    let id1 = parse_event_id(recv1.unwrap());
    let id2 = parse_event_id(recv2.unwrap());
    let id3 = parse_event_id(recv3.unwrap());

    assert_eq!(id1, id2);
    assert_eq!(id2, id3);
}
