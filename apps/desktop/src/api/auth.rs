//! Device authentication for RustShare
//!
//! Implements the device pairing flow:
//! 1. Client requests a device code
//! 2. User enters the code on the server's device pairing page
//! 3. Client polls for approval
//! 4. On approval, client receives an access token

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

use crate::api::client::ApiClient;
use crate::config::Config;

/// Secure storage key for device token
const TOKEN_KEYRING_SERVICE: &str = "rustshare-desktop";
const TOKEN_KEYRING_USERNAME: &str = "device_token";

/// Device authentication state
#[derive(Debug, Clone)]
pub struct DeviceAuth {
    config: Config,
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
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    /// Start device pairing flow
    /// 
    /// Returns the user code that should be displayed to the user
    pub async fn start_pairing(&self) -> Result<DevicePairingFlow> {
        let _client = ApiClient::new(&self.config)?;
        
        // Request device code from server
        let response = reqwest::Client::new()
            .post(format!("{}/api/v1/auth/device/request", self.config.server_url))
            .send()
            .await?;

        if !response.status().is_success() {
            anyhow::bail!("Failed to request device code: {}", response.status());
        }

        let device_code_response: DeviceCodeResponse = response.json().await?;
        
        info!(
            "Device pairing started. User code: {} (expires in {}s)",
            device_code_response.user_code,
            device_code_response.expires_in
        );

        Ok(DevicePairingFlow {
            user_code: device_code_response.user_code,
            device_code: device_code_response.device_code,
            expires_at: chrono::Utc::now() + chrono::Duration::seconds(device_code_response.expires_in),
            server_url: self.config.server_url.clone(),
        })
    }

    /// Load stored token from secure storage
    pub fn load_token(&self) -> Result<Option<DeviceToken>> {
        match keyring::Entry::new(TOKEN_KEYRING_SERVICE, TOKEN_KEYRING_USERNAME) {
            Ok(entry) => match entry.get_password() {
                Ok(token_json) => {
                    match serde_json::from_str::<DeviceToken>(&token_json) {
                        Ok(token) => {
                            debug!("Loaded device token from keyring");
                            Ok(Some(token))
                        }
                        Err(e) => {
                            warn!("Failed to parse stored token: {}", e);
                            Ok(None)
                        }
                    }
                }
                Err(keyring::Error::NoEntry) => Ok(None),
                Err(e) => {
                    warn!("Failed to read from keyring: {}", e);
                    Ok(None)
                }
            }
            Err(e) => {
                warn!("Failed to create keyring entry: {}", e);
                Ok(None)
            }
        }
    }

    /// Save token to secure storage
    pub fn save_token(&self, token: &DeviceToken) -> Result<()> {
        let token_json = serde_json::to_string(token)?;
        
        match keyring::Entry::new(TOKEN_KEYRING_SERVICE, TOKEN_KEYRING_USERNAME) {
            Ok(entry) => {
                entry.set_password(&token_json)?;
                info!("Device token saved to keyring");
                Ok(())
            }
            Err(e) => {
                anyhow::bail!("Failed to create keyring entry: {}", e);
            }
        }
    }

    /// Clear stored token (logout)
    pub fn clear_token(&self) -> Result<()> {
        match keyring::Entry::new(TOKEN_KEYRING_SERVICE, TOKEN_KEYRING_USERNAME) {
            Ok(entry) => {
                // Delete password by setting it to empty
                match entry.set_password("") {
                    Ok(_) => info!("Device token cleared from keyring"),
                    Err(e) => warn!("Failed to delete token: {}", e),
                }
            }
            Err(e) => warn!("Failed to access keyring: {}", e),
        }
        Ok(())
    }

    /// Check if user is logged in
    pub fn is_logged_in(&self) -> bool {
        self.load_token().map(|t| t.is_some()).unwrap_or(false)
    }
}

/// Active device pairing flow
#[derive(Debug, Clone)]
pub struct DevicePairingFlow {
    pub user_code: String,
    device_code: String,
    expires_at: chrono::DateTime<chrono::Utc>,
    server_url: String,
}

impl DevicePairingFlow {
    /// Get the user code to display
    pub fn user_code(&self) -> &str {
        &self.user_code
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

        let response = reqwest::Client::new()
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
pub async fn interactive_pairing(config: &Config) -> Result<DeviceToken> {
    let auth = DeviceAuth::new(config.clone());
    
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
    println!("Go to: {}/device", config.server_url);
    println!("Enter the code above to authorize this device.");
    println!("Waiting for approval...");
    println!();

    // Poll until approved
    let token = flow.poll_until_complete().await?;
    
    let device_token = DeviceToken {
        token,
        device_id: crate::get_or_create_device_id()?,
        created_at: chrono::Utc::now(),
    };
    
    // Save token securely
    auth.save_token(&device_token)?;
    
    println!("✓ Device paired successfully!");
    
    Ok(device_token)
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
}
