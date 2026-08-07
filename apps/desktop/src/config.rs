//! Configuration management for the desktop client
//!
//! Configuration is stored in `~/.config/rustshare/config.toml`

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::config_dir;

/// Default server URL
pub const DEFAULT_SERVER_URL: &str = "https://app.rustshare.io";

/// Client configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Server URL
    pub server_url: String,

    /// Sync folders (folder_id -> local path)
    pub sync_folders: Vec<SyncFolderConfig>,
}

/// Sync folder configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncFolderConfig {
    /// Folder ID on the server
    pub folder_id: uuid::Uuid,

    /// Local path where the folder is synced
    pub local_path: PathBuf,

    /// Whether this folder is currently enabled for sync
    pub enabled: bool,

    /// Sync direction
    #[serde(default)]
    pub direction: SyncDirection,

    /// File patterns to ignore (gitignore-style)
    #[serde(default)]
    pub ignore_patterns: Vec<String>,
}

/// Sync direction
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SyncDirection {
    /// Bidirectional sync (default)
    #[default]
    Bidirectional,
    /// Only upload local changes
    UploadOnly,
    /// Only download remote changes
    DownloadOnly,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server_url: DEFAULT_SERVER_URL.to_string(),
            sync_folders: Vec::new(),
        }
    }
}

impl Config {
    /// Load configuration from the default location
    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;

        if !path.exists() {
            let config = Config::default();
            config.save()?;
            return Ok(config);
        }

        let content = std::fs::read_to_string(&path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    /// Load configuration from a specific path
    pub fn load_from(path: &PathBuf) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    /// Save configuration to the default location
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        self.save_to(&path)
    }

    /// Save configuration to a specific path
    pub fn save_to(&self, path: &PathBuf) -> Result<()> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;

        tracing::debug!("Configuration saved to {}", path.display());
        Ok(())
    }

    /// Get the configuration file path
    pub fn config_path() -> Result<PathBuf> {
        Ok(config_dir()?.join("config.toml"))
    }

    /// Add a sync folder in memory.
    ///
    /// The caller is responsible for persisting the configuration with
    /// [`Config::save`] or [`Config::save_to`].
    pub fn add_sync_folder(&mut self, folder_id: uuid::Uuid, local_path: PathBuf) {
        // Check if already exists
        if let Some(existing) = self
            .sync_folders
            .iter_mut()
            .find(|f| f.folder_id == folder_id)
        {
            existing.local_path = local_path;
            existing.enabled = true;
        } else {
            self.sync_folders.push(SyncFolderConfig {
                folder_id,
                local_path,
                enabled: true,
                direction: SyncDirection::default(),
                ignore_patterns: default_ignore_patterns(),
            });
        }
    }

    /// Remove a sync folder in memory.
    ///
    /// The caller is responsible for persisting the configuration with
    /// [`Config::save`] or [`Config::save_to`].
    pub fn remove_sync_folder(&mut self, folder_id: uuid::Uuid) -> bool {
        let initial_len = self.sync_folders.len();
        self.sync_folders.retain(|f| f.folder_id != folder_id);

        self.sync_folders.len() < initial_len
    }

    /// Get the local path for a synced folder
    pub fn get_folder_local_path(&self, folder_id: uuid::Uuid) -> Option<&PathBuf> {
        self.sync_folders
            .iter()
            .find(|f| f.folder_id == folder_id && f.enabled)
            .map(|f| &f.local_path)
    }
}

/// Update parameters for a sync folder
#[derive(Debug, Default)]
pub struct FolderUpdate {
    pub local_path: Option<PathBuf>,
    pub enabled: Option<bool>,
    pub direction: Option<SyncDirection>,
    pub add_ignore_patterns: Vec<String>,
    pub remove_ignore_patterns: Vec<String>,
    pub clear_ignores: bool,
}

impl Config {
    /// Update a sync folder in memory.
    ///
    /// Returns true if the folder was found and updated, false otherwise.
    /// The caller is responsible for persisting the configuration with
    /// [`Config::save`] or [`Config::save_to`].
    pub fn update_sync_folder(&mut self, folder_id: uuid::Uuid, updates: FolderUpdate) -> bool {
        let Some(folder) = self
            .sync_folders
            .iter_mut()
            .find(|f| f.folder_id == folder_id)
        else {
            return false;
        };

        // Apply basic field updates
        if let Some(local_path) = updates.local_path {
            folder.local_path = local_path;
        }
        if let Some(enabled) = updates.enabled {
            folder.enabled = enabled;
        }
        if let Some(direction) = updates.direction {
            folder.direction = direction;
        }

        // Handle ignore patterns
        if updates.clear_ignores {
            folder.ignore_patterns = default_ignore_patterns();
        } else {
            // Remove patterns first
            if !updates.remove_ignore_patterns.is_empty() {
                folder
                    .ignore_patterns
                    .retain(|p| !updates.remove_ignore_patterns.contains(p));
            }
            // Then add new patterns (avoiding duplicates)
            for pattern in updates.add_ignore_patterns {
                if !folder.ignore_patterns.contains(&pattern) {
                    folder.ignore_patterns.push(pattern);
                }
            }
        }

        true
    }

    /// Convenience method to enable or disable a sync folder in memory.
    ///
    /// Returns true if the folder was found and updated, false otherwise.
    /// The caller is responsible for persisting the configuration with
    /// [`Config::save`] or [`Config::save_to`].
    pub fn set_folder_enabled(&mut self, folder_id: uuid::Uuid, enabled: bool) -> bool {
        self.update_sync_folder(
            folder_id,
            FolderUpdate {
                enabled: Some(enabled),
                ..FolderUpdate::default()
            },
        )
    }

    /// Get a reference to a sync folder configuration by ID
    pub fn get_sync_folder(&self, folder_id: uuid::Uuid) -> Option<&SyncFolderConfig> {
        self.sync_folders.iter().find(|f| f.folder_id == folder_id)
    }
}

/// Default ignore patterns (similar to .gitignore)
fn default_ignore_patterns() -> Vec<String> {
    vec![
        ".*".to_string(),    // Hidden files
        "*.tmp".to_string(), // Temp files
        "*.temp".to_string(),
        "*.swp".to_string(),       // Swap files
        "*.lock".to_string(),      // Lock files
        "~$*".to_string(),         // Office temp files
        "Thumbs.db".to_string(),   // Windows thumbnails
        ".DS_Store".to_string(),   // macOS metadata
        "desktop.ini".to_string(), // Windows desktop config
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.server_url, "https://app.rustshare.io");
        assert!(config.sync_folders.is_empty());
    }

    #[test]
    fn test_save_and_load() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");

        let config = Config {
            server_url: "https://example.com".to_string(),
            sync_folders: vec![SyncFolderConfig {
                folder_id: uuid::Uuid::new_v4(),
                local_path: "/test/path".into(),
                enabled: true,
                direction: SyncDirection::Bidirectional,
                ignore_patterns: vec!["*.tmp".to_string()],
            }],
        };

        // Save to temp location
        let content = toml::to_string_pretty(&config).unwrap();
        std::fs::write(&config_path, content).unwrap();

        // Load and verify
        let loaded = Config::load_from(&config_path).unwrap();
        assert_eq!(loaded.server_url, "https://example.com");
        assert_eq!(loaded.sync_folders.len(), 1);
    }

    #[test]
    fn test_update_sync_folder() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");
        let mut config = Config::default();

        // Add a sync folder
        let folder_id = uuid::Uuid::new_v4();
        config.add_sync_folder(folder_id, "/test/path".into());
        assert_eq!(config.sync_folders.len(), 1);
        assert_eq!(
            config.sync_folders[0].local_path,
            PathBuf::from("/test/path")
        );
        assert!(config.sync_folders[0].enabled);
        assert_eq!(
            config.sync_folders[0].direction,
            SyncDirection::Bidirectional
        );

        // Update with FolderUpdate - change path, disable, change direction, add ignore pattern
        let updates = FolderUpdate {
            local_path: Some("/new/path".into()),
            enabled: Some(false),
            direction: Some(SyncDirection::UploadOnly),
            add_ignore_patterns: vec!["*.log".to_string()],
            remove_ignore_patterns: vec![],
            clear_ignores: false,
        };
        assert!(config.update_sync_folder(folder_id, updates));

        // Save and reload to verify persistence
        config.save_to(&config_path).unwrap();
        let loaded = Config::load_from(&config_path).unwrap();

        // Verify all changes persisted
        let folder = loaded.get_sync_folder(folder_id).unwrap();
        assert_eq!(folder.local_path, PathBuf::from("/new/path"));
        assert!(!folder.enabled);
        assert_eq!(folder.direction, SyncDirection::UploadOnly);
        assert!(folder.ignore_patterns.contains(&"*.log".to_string()));
    }

    #[test]
    fn test_set_folder_enabled() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");
        let mut config = Config::default();

        // Add a sync folder
        let folder_id = uuid::Uuid::new_v4();
        config.add_sync_folder(folder_id, "/test/path".into());
        assert!(config.sync_folders[0].enabled);

        // Disable it
        assert!(config.set_folder_enabled(folder_id, false));
        assert!(!config.sync_folders[0].enabled);

        // Save and reload
        config.save_to(&config_path).unwrap();
        let loaded = Config::load_from(&config_path).unwrap();
        assert!(!loaded.sync_folders[0].enabled);

        // Enable it again
        let mut config = loaded;
        assert!(config.set_folder_enabled(folder_id, true));
        assert!(config.sync_folders[0].enabled);

        // Verify enabled=true persisted
        config.save_to(&config_path).unwrap();
        let loaded = Config::load_from(&config_path).unwrap();
        assert!(loaded.sync_folders[0].enabled);
    }

    #[test]
    fn test_remove_sync_folder() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");
        let mut config = Config::default();

        // Add a sync folder
        let folder_id = uuid::Uuid::new_v4();
        config.add_sync_folder(folder_id, "/test/path".into());
        assert_eq!(config.sync_folders.len(), 1);

        // Remove it
        assert!(config.remove_sync_folder(folder_id));
        assert!(config.sync_folders.is_empty());

        // Save and reload to verify persistence
        config.save_to(&config_path).unwrap();
        let mut loaded = Config::load_from(&config_path).unwrap();
        assert!(loaded.sync_folders.is_empty());

        // Try removing again - should return false
        assert!(!loaded.remove_sync_folder(folder_id));
    }
}
