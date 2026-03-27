//! Runtime profile configuration and detection

use rustshare_crypto::SecretEncryptionKey;

/// Runtime profile for the application
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeProfile {
    /// Standalone mode - single node, no Redis required
    Standalone,
    /// Distributed mode - multiple nodes, Redis required
    Distributed,
}

impl std::fmt::Display for RuntimeProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuntimeProfile::Standalone => write!(f, "standalone"),
            RuntimeProfile::Distributed => write!(f, "distributed"),
        }
    }
}

impl std::str::FromStr for RuntimeProfile {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "standalone" => Ok(RuntimeProfile::Standalone),
            "distributed" => Ok(RuntimeProfile::Distributed),
            _ => Err(format!("Invalid runtime profile: {}. Must be 'standalone' or 'distributed'", s)),
        }
    }
}

/// Configuration for profile detection and initialization
#[derive(Debug, Clone)]
pub struct ProfileConfig {
    /// Explicitly configured profile (optional)
    pub explicit_profile: Option<RuntimeProfile>,
    /// Redis enabled flag
    pub redis_enabled: bool,
    /// Redis URL
    pub redis_url: Option<String>,
    /// RustFS endpoint
    pub rustfs_endpoint: String,
    /// RustFS region
    pub rustfs_region: String,
    /// RustFS bucket
    pub rustfs_bucket: String,
    /// Metadata prefix
    pub metadata_prefix: String,
    /// Metadata namespace
    pub metadata_namespace: String,
    /// Local storage path (for standalone)
    pub local_storage_path: String,
    /// JWT secret
    pub jwt_secret: String,
    /// Secret encryption key
    pub secret_key: SecretEncryptionKey,
    /// Broadcast capacity
    pub broadcast_capacity: usize,
}

impl ProfileConfig {
    /// Load configuration from environment
    pub fn from_env() -> anyhow::Result<Self> {
        let explicit_profile = std::env::var("RUSTSHARE_RUNTIME_PROFILE")
            .ok()
            .and_then(|s| s.parse().ok());
        
        let redis_enabled = std::env::var("RUSTSHARE_REDIS_ENABLED")
            .ok()
            .map(|s| s == "true" || s == "1")
            .unwrap_or(false);
        
        let redis_url = std::env::var("RUSTSHARE_REDIS_URL").ok();
        
        let jwt_secret = std::env::var("JWT_SECRET")
            .map_err(|_| anyhow::anyhow!("JWT_SECRET environment variable required"))?;
        
        let secret_key = SecretEncryptionKey::from_env()
            .map_err(|e| anyhow::anyhow!("Failed to load secret key: {}", e))?;
        
        Ok(Self {
            explicit_profile,
            redis_enabled,
            redis_url,
            rustfs_endpoint: std::env::var("RUSTFS_ENDPOINT")
                .map_err(|_| anyhow::anyhow!("RUSTFS_ENDPOINT environment variable required"))?,
            rustfs_region: std::env::var("RUSTFS_REGION")
                .map_err(|_| anyhow::anyhow!("RUSTFS_REGION environment variable required"))?,
            rustfs_bucket: std::env::var("RUSTFS_BUCKET")
                .map_err(|_| anyhow::anyhow!("RUSTFS_BUCKET environment variable required"))?,
            metadata_prefix: std::env::var("RUSTSHARE_METADATA_PREFIX")
                .unwrap_or_else(|_| "apps/rustshare".to_string()),
            metadata_namespace: std::env::var("RUSTSHARE_METADATA_NAMESPACE")
                .unwrap_or_else(|_| "default".to_string()),
            local_storage_path: std::env::var("RUSTSHARE_LOCAL_STORAGE_PATH")
                .unwrap_or_else(|_| "./rustshare-data".to_string()),
            jwt_secret,
            secret_key,
            broadcast_capacity: std::env::var("BROADCAST_CAPACITY")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1000),
        })
    }
    
    /// Detect the runtime profile based on configuration
    ///
    /// Priority:
    /// 1. Explicit profile from RUSTSHARE_RUNTIME_PROFILE
    /// 2. If Redis is enabled and URL is provided -> Distributed
    /// 3. Otherwise -> Standalone
    pub fn detect_profile(&self) -> RuntimeProfile {
        // Check explicit profile first
        if let Some(profile) = self.explicit_profile {
            return profile;
        }
        
        // Auto-detect based on Redis configuration
        if self.redis_enabled && self.redis_url.is_some() {
            RuntimeProfile::Distributed
        } else {
            RuntimeProfile::Standalone
        }
    }
    
    /// Validate the configuration for the detected profile
    pub fn validate(&self) -> anyhow::Result<()> {
        let profile = self.detect_profile();
        
        match profile {
            RuntimeProfile::Distributed => {
                if !self.redis_enabled {
                    return Err(anyhow::anyhow!(
                        "Distributed profile requires RUSTSHARE_REDIS_ENABLED=true"
                    ));
                }
                if self.redis_url.is_none() {
                    return Err(anyhow::anyhow!(
                        "Distributed profile requires RUSTSHARE_REDIS_URL to be set"
                    ));
                }
            }
            RuntimeProfile::Standalone => {
                // Standalone mode doesn't require Redis
                tracing::info!("Running in standalone mode - Redis coordination not required");
            }
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_config() -> ProfileConfig {
        ProfileConfig {
            explicit_profile: None,
            redis_enabled: false,
            redis_url: None,
            rustfs_endpoint: "http://localhost:9000".to_string(),
            rustfs_region: "us-east-1".to_string(),
            rustfs_bucket: "rustshare".to_string(),
            metadata_prefix: "apps/rustshare".to_string(),
            metadata_namespace: "default".to_string(),
            local_storage_path: "./test-data".to_string(),
            jwt_secret: "test-secret".to_string(),
            secret_key: SecretEncryptionKey::generate(),
            broadcast_capacity: 100,
        }
    }

    #[test]
    fn test_detect_profile_standalone() {
        let config = create_test_config();
        assert_eq!(config.detect_profile(), RuntimeProfile::Standalone);
    }

    #[test]
    fn test_detect_profile_distributed() {
        let mut config = create_test_config();
        config.redis_enabled = true;
        config.redis_url = Some("redis://localhost:6379".to_string());
        assert_eq!(config.detect_profile(), RuntimeProfile::Distributed);
    }

    #[test]
    fn test_explicit_profile_override() {
        let mut config = create_test_config();
        config.explicit_profile = Some(RuntimeProfile::Standalone);
        config.redis_enabled = true; // Would normally be distributed
        config.redis_url = Some("redis://localhost:6379".to_string());
        
        // Explicit profile takes precedence
        assert_eq!(config.detect_profile(), RuntimeProfile::Standalone);
    }

    #[test]
    fn test_validate_standalone() {
        let config = create_test_config();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_distributed_missing_redis() {
        let mut config = create_test_config();
        config.explicit_profile = Some(RuntimeProfile::Distributed);
        // Redis not configured
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_distributed_ok() {
        let mut config = create_test_config();
        config.redis_enabled = true;
        config.redis_url = Some("redis://localhost:6379".to_string());
        assert!(config.validate().is_ok());
    }
}
