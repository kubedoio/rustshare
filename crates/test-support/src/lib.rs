use anyhow::Result;
use axum::{
    routing::{get, post},
    Json, Router,
};
use std::net::SocketAddr;
use sync_protocol::{DeltaResponse, DeviceRegistrationResponse};
use tokio::net::TcpListener;
use uuid::Uuid;

pub struct MockBackend {
    pub addr: SocketAddr,
}

impl MockBackend {
    pub async fn start() -> Result<Self> {
        let app = Router::new()
            .route(
                "/api/v1/devices/register",
                post(|| async {
                    Json(DeviceRegistrationResponse {
                        device_id: Uuid::new_v4(),
                    })
                }),
            )
            .route(
                "/api/v1/sync/deltas",
                get(|| async {
                    Json(DeltaResponse {
                        cursor: "123".to_string(),
                        changes: vec![],
                    })
                }),
            );

        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;

        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        Ok(Self { addr })
    }

    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }
}
