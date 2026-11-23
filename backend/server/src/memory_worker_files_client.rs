//! Workload-authenticated client for the scoped Files snapshot contract.

use rustshare_memory::files_source::{
    FilesProjectionSnapshot, FilesReconcilePage, FilesReconcileTenantPage,
};
use serde::Serialize;
use uuid::Uuid;

use crate::memory_worker_auth::MemoryWorkerRequestAuth;

const ROUTE_PATH: &str = "/internal/v1/memory/files/snapshot";
const TENANT_PAGE_PATH: &str = "/internal/v1/memory/files/reconcile/tenants";
const RECONCILE_PAGE_PATH: &str = "/internal/v1/memory/files/reconcile/page";
const SIGNATURE_HEADER: &str = "X-Elembra-Memory-Signature";
const MAX_RESPONSE_BYTES: usize = 1_000_000;

#[derive(Serialize)]
struct SnapshotRequest<'a> {
    source: &'a str,
    event_id: Uuid,
}

#[derive(Serialize)]
struct TenantPageRequest {
    after: Option<Uuid>,
    limit: u32,
}

#[derive(Serialize)]
struct ReconcilePageRequest {
    tenant_id: Uuid,
    after_file_id: Option<Uuid>,
    limit: u32,
}

#[derive(Clone)]
pub struct MemoryWorkerFilesClient {
    client: reqwest::Client,
    base_url: String,
    auth: MemoryWorkerRequestAuth,
}

impl MemoryWorkerFilesClient {
    pub fn from_env() -> anyhow::Result<Self> {
        let base_url = std::env::var("RUSTSHARE_API_BASE_URL")?;
        let url = reqwest::Url::parse(&base_url)?;
        anyhow::ensure!(
            matches!(url.scheme(), "http" | "https")
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
                && url.path() == "/"
                && url.query().is_none()
                && url.fragment().is_none(),
            "RUSTSHARE_API_BASE_URL must be an HTTP(S) origin without credentials, query, or fragment"
        );
        Ok(Self {
            client: reqwest::Client::builder()
                .connect_timeout(std::time::Duration::from_secs(5))
                .timeout(std::time::Duration::from_secs(60))
                .build()?,
            base_url: base_url.trim_end_matches('/').to_owned(),
            auth: MemoryWorkerRequestAuth::from_env()?,
        })
    }

    pub async fn snapshot(
        &self,
        source: &str,
        event_id: Uuid,
    ) -> anyhow::Result<FilesProjectionSnapshot> {
        let body = serde_json::to_vec(&SnapshotRequest { source, event_id })?;
        let body = self.post(ROUTE_PATH, body).await?;
        Ok(serde_json::from_slice(&body)?)
    }

    pub async fn tenant_page(
        &self,
        after: Option<Uuid>,
        limit: u32,
    ) -> anyhow::Result<FilesReconcileTenantPage> {
        let body = serde_json::to_vec(&TenantPageRequest { after, limit })?;
        let body = self.post(TENANT_PAGE_PATH, body).await?;
        Ok(serde_json::from_slice(&body)?)
    }

    pub async fn reconcile_page(
        &self,
        tenant_id: Uuid,
        after_file_id: Option<Uuid>,
        limit: u32,
    ) -> anyhow::Result<FilesReconcilePage> {
        let body = serde_json::to_vec(&ReconcilePageRequest {
            tenant_id,
            after_file_id,
            limit,
        })?;
        let body = self.post(RECONCILE_PAGE_PATH, body).await?;
        Ok(serde_json::from_slice(&body)?)
    }

    async fn post(&self, path: &str, body: Vec<u8>) -> anyhow::Result<bytes::Bytes> {
        let signature = self.auth.sign_request("POST", path, &body)?;
        let response = self
            .client
            .post(format!("{}{path}", self.base_url))
            .header(SIGNATURE_HEADER, signature)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body)
            .send()
            .await?;
        anyhow::ensure!(
            response.status().is_success(),
            "Files snapshot endpoint unavailable"
        );
        let body = response.bytes().await?;
        anyhow::ensure!(
            body.len() <= MAX_RESPONSE_BYTES,
            "Files snapshot exceeded response limit"
        );
        Ok(body)
    }
}
