#[cfg(windows)]
use windows::{
    core::*,
    Win32::Storage::CloudFilters::*,
    Win32::Foundation::*,
};
use std::path::Path;
use anyhow::{Result, anyhow};
use tracing::{info, error};

pub struct VfsManagerWin {
    sync_root_id: String,
}

impl VfsManagerWin {
    pub fn new(sync_root_id: &str) -> Self {
        Self {
            sync_root_id: sync_root_id.to_string(),
        }
    }

    #[cfg(windows)]
    pub fn register_root(&self, local_path: &Path) -> Result<()> {
        info!("Registering Windows Cloud Filter root at {:?}", local_path);
        
        let path_u16: Vec<u16> = local_path.to_str().unwrap().encode_utf16().chain(std::iter::once(0)).collect();
        let pc_u16: Vec<u16> = self.sync_root_id.encode_utf16().chain(std::iter::once(0)).collect();

        let info = CF_SYNC_REGISTRATION {
            StructSize: std::mem::size_of::<CF_SYNC_REGISTRATION>() as u32,
            ProviderName: PCWSTR(pc_u16.as_ptr()),
            ProviderVersion: PCWSTR("1.0.0\0".encode_utf16().collect::<Vec<u16>>().as_ptr()),
            SyncRootIdentity: ptr::null(),
            SyncRootIdentityLength: 0,
            FileIdentity: ptr::null(),
            FileIdentityLength: 0,
            ProviderId: GUID::zeroed(),
        };

        let policies = CF_SYNC_POLICIES {
            StructSize: std::mem::size_of::<CF_SYNC_POLICIES>() as u32,
            Hydration: CF_HYDRATION_POLICY_PARTIAL,
            Population: CF_POPULATION_POLICY_PARTIAL,
            InSync: CF_IN_SYNC_POLICY_NONE,
            HardLink: CF_HARDLINK_POLICY_NONE,
            PlaceholderManagement: CF_PLACEHOLDER_MANAGEMENT_POLICY_DEFAULT,
        };

        unsafe {
            CfRegisterSyncRoot(
                PCWSTR(path_u16.as_ptr()),
                &info,
                &policies,
                CF_REGISTER_FLAG_NONE,
            ).map_err(|e| anyhow!("Failed to register sync root: {}", e))?;
        }

        Ok(())
    }

    #[cfg(not(windows))]
    pub fn register_root(&self, _local_path: &Path) -> Result<()> {
        Err(anyhow!("Windows Cloud Filter is only supported on Windows"))
    }
}

#[cfg(windows)]
mod ptr {
    pub use std::ptr::null;
}
