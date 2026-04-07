//! Device authentication for RustShare
//!
//! Implements the device pairing flow:
//! 1. Client requests a device code
//! 2. User enters the code on the server's device pairing page
//! 3. Client polls for approval
//! 4. On approval, client receives an access token

use anyhow::Result;
use platform::get_device_id;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, error, info};

/// Device authentication state
#[derive(Debug, Clone)]
pub struct DeviceAuth {
    server_url: String,
}

/// Device token stored securely
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceToken {
    pub token: String,
    pub device_id: uuid::Uuid,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Response from device code request
#[derive(Debug, Deserialize)]
struct DeviceCodeResponse {
    user_code: String,
    device_code: String,
    expires_in: i64,
    verification_uri: String,
    verification_uri_complete: String,
}

/// Poll response from server
#[derive(Debug, Deserialize)]
#[serde(tag = "status")]
enum PollResponse {
    #[serde(rename = "pending")]
    Pending,
    #[serde(rename = "approved")]
    Approved { token: String },
    #[serde(rename = "expired")]
    Expired,
}

impl DeviceAuth {
    /// Create a new device auth instance
    pub fn new(server_url: impl Into<String>) -> Self {
        Self {
            server_url: server_url.into(),
        }
    }

    /// Start device pairing flow
    /// 
    /// Returns the user code that should be displayed to the user
    pub async fn start_pairing(&self) -> Result<DevicePairingFlow> {
        // Request device code from server
        let response = pairing_http_client()?
            .post(format!("{}/api/v1/auth/device/request", self.server_url))
            .send()
            .await?;

        if !response.status().is_success() {
            anyhow::bail!("Failed to request device code: {}", response.status());
        }

        let device_code_response: DeviceCodeResponse = response.json().await?;
        let approval_url = build_approval_url(
            &device_code_response.verification_uri,
            &device_code_response.verification_uri_complete,
            &device_code_response.device_code,
        );
        
        info!(
            "Device pairing started. User code: {} (expires in {}s)",
            device_code_response.user_code,
            device_code_response.expires_in
        );

        Ok(DevicePairingFlow {
            user_code: device_code_response.user_code,
            device_code: device_code_response.device_code,
            approval_url,
            expires_at: chrono::Utc::now() + chrono::Duration::seconds(device_code_response.expires_in),
            server_url: self.server_url.clone(),
        })
    }
}

/// Active device pairing flow
#[derive(Debug, Clone)]
pub struct DevicePairingFlow {
    pub user_code: String,
    device_code: String,
    approval_url: String,
    expires_at: chrono::DateTime<chrono::Utc>,
    server_url: String,
}

impl DevicePairingFlow {
    /// Get the user code to display
    pub fn user_code(&self) -> &str {
        &self.user_code
    }

    /// Get the full approval URL to present to the user.
    pub fn approval_url(&self) -> &str {
        &self.approval_url
    }

    /// Check if the pairing code has expired
    pub fn is_expired(&self) -> bool {
        chrono::Utc::now() > self.expires_at
    }

    /// Get remaining time until expiration
    pub fn time_remaining(&self) -> Duration {
        let remaining = self.expires_at - chrono::Utc::now();
        if remaining.num_seconds() > 0 {
            Duration::from_secs(remaining.num_seconds() as u64)
        } else {
            Duration::from_secs(0)
        }
    }

    /// Poll for approval
    /// 
    /// Returns Ok(Some(token)) when approved, Ok(None) if still pending,
    /// and Err if expired or error occurred.
    pub async fn poll(&self) -> Result<Option<String>> {
        if self.is_expired() {
            anyhow::bail!("Device code has expired");
        }

        let response = pairing_http_client()?
            .post(format!("{}/api/v1/auth/device/poll", self.server_url))
            .json(&serde_json::json!({
                "device_code": self.device_code,
            }))
            .send()
            .await?;

        match response.status() {
            reqwest::StatusCode::OK => {
                let poll_response: PollResponse = response.json().await?;
                
                match poll_response {
                    PollResponse::Pending => Ok(None),
                    PollResponse::Approved { token } => {
                        info!("Device pairing approved");
                        Ok(Some(token))
                    }
                    PollResponse::Expired => {
                        anyhow::bail!("Device code has expired on server");
                    }
                }
            }
            reqwest::StatusCode::TOO_MANY_REQUESTS => {
                // Rate limited, backoff
                let retry_after = response
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(5);
                
                debug!("Rate limited, waiting {}s", retry_after);
                sleep(Duration::from_secs(retry_after)).await;
                Ok(None)
            }
            _ => {
                let status = response.status();
                let text = response.text().await.unwrap_or_default();
                anyhow::bail!("Poll failed: {} - {}", status, text);
            }
        }
    }

    /// Poll until approval or expiration
    /// 
    /// This blocks until the user approves the device or the code expires.
    /// It respects rate limits and polls at appropriate intervals.
    pub async fn poll_until_complete(&self) -> Result<String> {
        let poll_interval = Duration::from_secs(5);
        
        loop {
            if self.is_expired() {
                anyhow::bail!("Device code expired before approval");
            }

            match self.poll().await {
                Ok(Some(token)) => return Ok(token),
                Ok(None) => {
                    // Still pending, wait and retry
                    sleep(poll_interval).await;
                }
                Err(e) => {
                    error!("Poll error: {}", e);
                    return Err(e);
                }
            }
        }
    }
}

/// Interactive device pairing
///
/// This function handles the entire pairing flow interactively,
/// displaying the user code and polling until approval.
pub async fn interactive_pairing(server_url: &str) -> Result<DeviceToken> {
    let auth = DeviceAuth::new(server_url);
    
    // Start pairing flow
    let flow = auth.start_pairing().await?;
    
    println!("\n╔══════════════════════════════════════╗");
    println!("║        Device Pairing Code           ║");
    println!("╠══════════════════════════════════════╣");
    println!("║                                      ║");
    println!("║         {}                  ║", flow.user_code());
    println!("║                                      ║");
    println!("╚══════════════════════════════════════╝");
    println!();
    println!("{}", pairing_instructions(flow.approval_url()));
    println!("Waiting for approval...");
    println!();

    // Poll until approved
    let token = flow.poll_until_complete().await?;
    
    let device_token = DeviceToken {
        token,
        device_id: get_device_id()?,
        created_at: chrono::Utc::now(),
    };

    println!("✓ Device paired successfully!");
    
    Ok(device_token)
}

/// Format the user-facing pairing instructions for a device approval link.
pub fn pairing_instructions(approval_url: &str) -> String {
    format!(
        "Approve this device in RustShare:\n{}\n\nThis approval link is valid for 5 minutes.\nOpen it from an authenticated RustShare web UI session to approve this device.",
        approval_url
    )
}

fn build_approval_url(
    verification_uri: &str,
    verification_uri_complete: &str,
    device_code: &str,
) -> String {
    if !verification_uri_complete.trim().is_empty() {
        verification_uri_complete.to_string()
    } else {
        format!(
            "{}?device_code={}",
            verification_uri.trim_end_matches('/'),
            device_code
        )
    }
}

fn desktop_user_agent() -> String {
    format!("rustshare-desktop/{}", crate::VERSION)
}

fn pairing_http_client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .user_agent(desktop_user_agent())
        .build()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_token_serialization() {
        let token = DeviceToken {
            token: "test_token_123".to_string(),
            device_id: uuid::Uuid::new_v4(),
            created_at: chrono::Utc::now(),
        };

        let json = serde_json::to_string(&token).unwrap();
        let deserialized: DeviceToken = serde_json::from_str(&json).unwrap();
        
        assert_eq!(token.token, deserialized.token);
        assert_eq!(token.device_id, deserialized.device_id);
    }

    #[test]
    fn build_approval_url_prefers_complete_url() {
        let approval_url = build_approval_url(
            "https://example.com/device/approve",
            "https://example.com/device/approve?device_code=device-code-123",
            "device-code-123",
        );

        assert_eq!(
            approval_url,
            "https://example.com/device/approve?device_code=device-code-123"
        );
    }

    #[test]
    fn build_approval_url_falls_back_to_device_code_query() {
        let approval_url = build_approval_url(
            "https://example.com/device/approve",
            "",
            "device-code-123",
        );

        assert_eq!(
            approval_url,
            "https://example.com/device/approve?device_code=device-code-123"
        );
    }

    #[test]
    fn pairing_instructions_include_required_guidance() {
        let approval_url = "https://example.com/device/approve?device_code=device-code-123";
        let instructions = pairing_instructions(approval_url);

        assert!(instructions.contains(approval_url));
        assert!(instructions.contains("valid for 5 minutes"));
        assert!(instructions.contains("authenticated RustShare web UI session"));
    }

    #[test]
    fn desktop_user_agent_uses_app_identity() {
        let user_agent = desktop_user_agent();

        assert!(user_agent.starts_with("rustshare-desktop/"));
        assert!(user_agent.ends_with(crate::VERSION));
    }
}
