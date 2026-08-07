use anyhow::{anyhow, Result};
use keyring_core::Entry;
use std::path::PathBuf;
use uuid::Uuid;

pub const RUSTSHARE_TOKEN_SERVICE: &str = "rustshare";

pub struct TokenStore {
    service: String,
}

impl TokenStore {
    pub fn new(service: &str) -> Self {
        Self {
            service: service.to_string(),
        }
    }

    pub fn save_token(&self, user_id: &str, token: &str) -> Result<()> {
        let entry =
            Entry::new(&self.service, user_id).map_err(|e| anyhow!("Keyring error: {}", e))?;
        entry
            .set_password(token)
            .map_err(|e| anyhow!("Keyring error: {}", e))?;
        Ok(())
    }

    pub fn get_token(&self, user_id: &str) -> Result<Option<String>> {
        let entry =
            Entry::new(&self.service, user_id).map_err(|e| anyhow!("Keyring error: {}", e))?;
        match entry.get_password() {
            Ok(token) => Ok(Some(token)),
            Err(keyring_core::Error::NoEntry) => Ok(None),
            Err(e) => Err(anyhow!("Keyring error: {}", e)),
        }
    }

    pub fn delete_token(&self, user_id: &str) -> Result<()> {
        let entry =
            Entry::new(&self.service, user_id).map_err(|e| anyhow!("Keyring error: {}", e))?;
        entry
            .delete_credential()
            .map_err(|e| anyhow!("Keyring error: {}", e))?;
        Ok(())
    }
}

pub struct PathManager;

impl PathManager {
    pub fn get_app_data_dir() -> Result<PathBuf> {
        let dirs = directories::ProjectDirs::from("io", "rustshare", "RustShare")
            .ok_or_else(|| anyhow!("Could not determine app data directory"))?;
        let path = dirs.data_dir().to_path_buf();
        std::fs::create_dir_all(&path)?;
        Ok(path)
    }
}

pub fn get_device_id() -> Result<Uuid> {
    // In a real app, this would be a stable hardware ID.
    // For Phase 1, we generate and persist it in the app data dir.
    let data_dir = PathManager::get_app_data_dir()?;
    let id_file = data_dir.join("device_id");

    if id_file.exists() {
        let content = std::fs::read_to_string(&id_file)?;
        Ok(Uuid::parse_str(content.trim())?)
    } else {
        let id = Uuid::new_v4();
        std::fs::write(&id_file, id.to_string())?;
        Ok(id)
    }
}

pub fn desktop_token_store() -> TokenStore {
    TokenStore::new(RUSTSHARE_TOKEN_SERVICE)
}
