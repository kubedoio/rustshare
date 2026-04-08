use serde::{Deserialize, Serialize};
use sync_domain::{RemoteEntry, SyncRoot};

#[derive(Debug, Serialize, Deserialize)]
pub struct DeviceRegistrationRequest {
    pub name: String,
    pub os: String,
    pub device_type: String,
    pub version: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DeviceRegistrationResponse {
    pub device_id: uuid::Uuid,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DeltaRequest {
    pub cursor: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DeltaResponse {
    pub cursor: String,
    pub changes: Vec<DeltaChange>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum DeltaChange {
    Upsert(RemoteEntry),
    Delete(uuid::Uuid),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SyncRootsResponse {
    pub roots: Vec<SyncRoot>,
}
