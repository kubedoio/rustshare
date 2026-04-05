//! RustShare Desktop Sync Client
//!
//! A selective sync client for RustShare that provides:
//! - Device pairing and authentication
//! - Local file synchronization with conflict resolution
//! - Real-time and polling-based sync
//! - Journal-based crash recovery

// pub mod api;
// pub mod config;
// pub mod db;
// pub mod fs;
// pub mod sync;

use anyhow::Result;
use std::path::PathBuf;
use tracing::{info, warn};
use uuid::Uuid;

/// Desktop client version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Application name for directories
pub const APP_NAME: &str = "rustshare";

/// Get the user's config directory for RustShare
pub fn config_dir() -> Result<PathBuf> {
    let dirs = directories::ProjectDirs::from("com", "rustshare", APP_NAME)
        .ok_or_else(|| anyhow::anyhow!("Could not determine config directory"))?;
    Ok(dirs.config_dir().to_path_buf())
}

/// Get the user's data directory for RustShare
pub fn data_dir() -> Result<PathBuf> {
    let dirs = directories::ProjectDirs::from("com", "rustshare", APP_NAME)
        .ok_or_else(|| anyhow::anyhow!("Could not determine data directory"))?;
    Ok(dirs.data_dir().to_path_buf())
}

/// Get the cache directory for RustShare
pub fn cache_dir() -> Result<PathBuf> {
    let dirs = directories::ProjectDirs::from("com", "rustshare", APP_NAME)
        .ok_or_else(|| anyhow::anyhow!("Could not determine cache directory"))?;
    Ok(dirs.cache_dir().to_path_buf())
}

/// Ensure all required directories exist
pub fn ensure_directories() -> Result<()> {
    let config = config_dir()?;
    let data = data_dir()?;
    let cache = cache_dir()?;

    std::fs::create_dir_all(&config)?;
    std::fs::create_dir_all(&data)?;
    std::fs::create_dir_all(&cache)?;

    info!("Directories ensured: config={}, data={}, cache={}",
        config.display(), data.display(), cache.display());

    Ok(())
}

/// Generate a unique device identifier
/// 
/// This ID is persisted and used to identify this device to the server.
pub fn get_or_create_device_id() -> Result<Uuid> {
    let device_id_path = data_dir()?.join("device_id");

    if device_id_path.exists() {
        let content = std::fs::read_to_string(&device_id_path)?;
        let id = Uuid::parse_str(content.trim())
            .map_err(|e| anyhow::anyhow!("Invalid device ID file: {}", e))?;
        return Ok(id);
    }

    // Generate new device ID
    let id = Uuid::new_v4();
    std::fs::write(&device_id_path, id.to_string())?;
    
    info!("Generated new device ID: {}", id);
    Ok(id)
}

/// Clear all local data (for logout/reset)
pub fn clear_local_data() -> Result<()> {
    let data = data_dir()?;
    let cache = cache_dir()?;

    if data.exists() {
        warn!("Removing data directory: {}", data.display());
        std::fs::remove_dir_all(&data)?;
    }

    if cache.exists() {
        warn!("Removing cache directory: {}", cache.display());
        std::fs::remove_dir_all(&cache)?;
    }

    info!("Local data cleared");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert!(!VERSION.is_empty());
    }
}
