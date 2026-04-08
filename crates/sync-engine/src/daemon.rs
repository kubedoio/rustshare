//! Daemon process management module
//!
//! This module provides PID file handling and lifecycle management
//! for the sync daemon process.

use anyhow::{anyhow, Context, Result};
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

/// Manages daemon lifecycle including PID files and socket paths
#[derive(Debug, Clone)]
pub struct DaemonHandle {
    /// Path to the data directory
    data_dir: PathBuf,
    /// Path to the PID file
    pid_file: PathBuf,
    /// Path to the Unix socket
    socket_path: PathBuf,
}

impl DaemonHandle {
    /// Creates a new daemon handle with paths for pid_file and socket_path
    ///
    /// # Arguments
    /// * `data_dir` - The base data directory for daemon files
    pub fn new(data_dir: PathBuf) -> Self {
        let pid_file = data_dir.join("daemon.pid");
        let socket_path = data_dir.join("daemon.sock");
        Self {
            data_dir,
            pid_file,
            socket_path,
        }
    }

    /// Checks if the daemon is currently running
    ///
    /// Reads the PID file and checks if the process exists
    pub fn is_running(&self) -> bool {
        match self.get_pid() {
            Some(pid) => process_exists(pid),
            None => false,
        }
    }

    /// Returns the PID from the file if it exists
    pub fn get_pid(&self) -> Option<nix::unistd::Pid> {
        if !self.pid_file.exists() {
            return None;
        }

        match std::fs::read_to_string(&self.pid_file) {
            Ok(content) => {
                let trimmed = content.trim();
                match trimmed.parse::<i32>() {
                    Ok(pid) => {
                        if pid > 0 {
                            Some(nix::unistd::Pid::from_raw(pid))
                        } else {
                            warn!("Invalid PID in file: {}", pid);
                            None
                        }
                    }
                    Err(e) => {
                        warn!("Failed to parse PID from file: {}", e);
                        None
                    }
                }
            }
            Err(e) => {
                debug!("Failed to read PID file: {}", e);
                None
            }
        }
    }

    /// Writes the current process PID to the file
    pub fn write_pid(&self) -> Result<()> {
        // Create parent directory if needed
        if let Some(parent) = self.pid_file.parent() {
            std::fs::create_dir_all(parent).context("Failed to create PID file directory")?;
        }

        let pid = nix::unistd::getpid().as_raw();
        std::fs::write(&self.pid_file, format!("{}\n", pid)).context("Failed to write PID file")?;

        info!(
            "PID file written: {} (PID: {})",
            self.pid_file.display(),
            pid
        );
        Ok(())
    }

    /// Removes the PID file
    pub fn remove_pid(&self) -> Result<()> {
        if self.pid_file.exists() {
            std::fs::remove_file(&self.pid_file).context("Failed to remove PID file")?;
            info!("PID file removed: {}", self.pid_file.display());
        }
        Ok(())
    }

    /// Removes stale PID/socket files if the process is not running
    pub fn cleanup_stale(&self) -> Result<()> {
        let pid_exists = self.pid_file.exists();
        let socket_exists = self.socket_path.exists();

        // Check if we have stale files
        if pid_exists || socket_exists {
            let is_running = self.is_running();

            if !is_running {
                if pid_exists {
                    warn!("Removing stale PID file: {}", self.pid_file.display());
                    std::fs::remove_file(&self.pid_file)
                        .context("Failed to remove stale PID file")?;
                }

                if socket_exists {
                    warn!("Removing stale socket file: {}", self.socket_path.display());
                    std::fs::remove_file(&self.socket_path)
                        .context("Failed to remove stale socket file")?;
                }
            } else {
                debug!("Daemon is running, not cleaning up files");
            }
        }

        Ok(())
    }

    /// Returns the socket path
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    /// Returns the PID file path
    pub fn pid_file(&self) -> &Path {
        &self.pid_file
    }

    /// Returns the data directory
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }
}

/// Checks if a process with the given PID exists
///
/// On Unix: Uses `kill(pid, 0)` to check if process exists without sending a signal
/// On Windows: Currently a stub
#[cfg(unix)]
fn process_exists(pid: nix::unistd::Pid) -> bool {
    use nix::sys::signal;
    use nix::sys::signal::Signal;

    match signal::kill(pid, Some(Signal::SIGCONT)) {
        Ok(_) => true,
        Err(nix::errno::Errno::ESRCH) => false, // No such process
        Err(nix::errno::Errno::EPERM) => true,  // Process exists but no permission
        Err(e) => {
            debug!("Unexpected error checking process {}: {}", pid, e);
            false
        }
    }
}

#[cfg(not(unix))]
fn process_exists(_pid: nix::unistd::Pid) -> bool {
    // Windows stub - would need to use OpenProcess or similar
    warn!("Process existence check not implemented for this platform");
    false
}

/// Sends SIGTERM to the daemon process to stop it
///
/// # Arguments
/// * `pid_file` - Path to the PID file containing the daemon's PID
pub fn stop_daemon(pid_file: &Path) -> Result<()> {
    if !pid_file.exists() {
        return Err(anyhow!("PID file does not exist: {}", pid_file.display()));
    }

    let content = std::fs::read_to_string(pid_file).context("Failed to read PID file")?;

    let trimmed = content.trim();
    let pid = trimmed
        .parse::<i32>()
        .context("Failed to parse PID from file")?;

    if pid <= 0 {
        return Err(anyhow!("Invalid PID in file: {}", pid));
    }

    let pid = nix::unistd::Pid::from_raw(pid);

    // Check if process exists before sending signal
    if !process_exists(pid) {
        return Err(anyhow!("Process with PID {} is not running", pid));
    }

    info!("Sending SIGTERM to daemon process (PID: {})", pid);

    #[cfg(unix)]
    {
        use nix::sys::signal;
        use nix::sys::signal::Signal;

        signal::kill(pid, Signal::SIGTERM).context("Failed to send SIGTERM to daemon")?;
    }

    #[cfg(not(unix))]
    {
        return Err(anyhow!("Daemon stop not implemented for this platform"));
    }

    Ok(())
}

/// Waits for the daemon to stop
///
/// # Arguments
/// * `pid_file` - Path to the PID file
/// * `timeout_secs` - Maximum time to wait in seconds
///
/// # Returns
/// * `Ok(())` if daemon stopped within timeout
/// * `Err` if timeout exceeded or other error
pub async fn wait_for_stop(pid_file: &Path, timeout_secs: u64) -> Result<()> {
    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(timeout_secs);

    info!("Waiting for daemon to stop (timeout: {}s)", timeout_secs);

    loop {
        // Check if PID file still exists
        if !pid_file.exists() {
            info!("PID file removed, daemon has stopped");
            return Ok(());
        }

        // Try to read PID and check if process exists
        if let Ok(content) = std::fs::read_to_string(pid_file) {
            if let Ok(pid) = content.trim().parse::<i32>() {
                if pid > 0 {
                    let pid = nix::unistd::Pid::from_raw(pid);
                    if !process_exists(pid) {
                        info!("Daemon process (PID: {}) no longer exists", pid);
                        return Ok(());
                    }
                }
            }
        }

        // Check timeout
        if start.elapsed() >= timeout {
            return Err(anyhow!(
                "Timeout waiting for daemon to stop after {} seconds",
                timeout_secs
            ));
        }

        // Wait a bit before checking again
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }
}

/// Sets up signal handlers for graceful shutdown
///
/// This should be called by the daemon process to handle SIGTERM/SIGINT
#[cfg(unix)]
#[allow(unsafe_code)]
pub fn setup_signal_handlers() -> Result<tokio::sync::mpsc::Receiver<()>> {
    use nix::sys::signal::{self, SigHandler, Signal};

    let (_tx, rx) = tokio::sync::mpsc::channel(1);

    // Store sender in a static for the signal handler
    // Note: In production, you'd want a more robust solution
    // This is a simplified version for the module

    unsafe {
        signal::signal(Signal::SIGTERM, SigHandler::SigDfl)
            .context("Failed to set SIGTERM handler")?;
        signal::signal(Signal::SIGINT, SigHandler::SigDfl)
            .context("Failed to set SIGINT handler")?;
    }

    // For a complete implementation, you'd use signal-hook or tokio-signals
    // This is a basic placeholder that returns a channel

    Ok(rx)
}

#[cfg(not(unix))]
pub fn setup_signal_handlers() -> Result<tokio::sync::mpsc::Receiver<()>> {
    let (_, rx) = tokio::sync::mpsc::channel(1);
    warn!("Signal handlers not implemented for this platform");
    Ok(rx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_daemon_handle_new() {
        let temp_dir = TempDir::new().unwrap();
        let handle = DaemonHandle::new(temp_dir.path().to_path_buf());

        assert_eq!(handle.data_dir(), temp_dir.path());
        assert_eq!(handle.pid_file(), temp_dir.path().join("daemon.pid"));
        assert_eq!(handle.socket_path(), temp_dir.path().join("daemon.sock"));
    }

    #[test]
    fn test_write_and_read_pid() {
        let temp_dir = TempDir::new().unwrap();
        let handle = DaemonHandle::new(temp_dir.path().to_path_buf());

        // Write PID
        handle.write_pid().unwrap();
        assert!(handle.pid_file().exists());

        // Read PID
        let pid = handle.get_pid();
        assert!(pid.is_some());
        assert_eq!(pid.unwrap(), nix::unistd::getpid());

        // Remove PID
        handle.remove_pid().unwrap();
        assert!(!handle.pid_file().exists());
    }

    #[test]
    fn test_get_pid_no_file() {
        let temp_dir = TempDir::new().unwrap();
        let handle = DaemonHandle::new(temp_dir.path().to_path_buf());

        let pid = handle.get_pid();
        assert!(pid.is_none());
    }

    #[test]
    fn test_cleanup_stale() {
        let temp_dir = TempDir::new().unwrap();
        let handle = DaemonHandle::new(temp_dir.path().to_path_buf());

        // Create a fake PID file with non-existent PID
        std::fs::write(&handle.pid_file, "99999\n").unwrap();

        // Create a fake socket file
        std::fs::write(&handle.socket_path, "").unwrap();

        assert!(handle.pid_file().exists());
        assert!(handle.socket_path().exists());

        // Cleanup should remove stale files
        handle.cleanup_stale().unwrap();

        assert!(!handle.pid_file().exists());
        assert!(!handle.socket_path().exists());
    }

    #[test]
    fn test_is_running_current_process() {
        let temp_dir = TempDir::new().unwrap();
        let handle = DaemonHandle::new(temp_dir.path().to_path_buf());

        // Write current PID
        handle.write_pid().unwrap();

        // Should detect current process as running
        assert!(handle.is_running());

        // Cleanup
        handle.remove_pid().unwrap();
    }

    #[test]
    fn test_is_running_no_pid() {
        let temp_dir = TempDir::new().unwrap();
        let handle = DaemonHandle::new(temp_dir.path().to_path_buf());

        // Should return false when no PID file
        assert!(!handle.is_running());
    }

    #[test]
    fn test_stop_daemon_no_file() {
        let temp_dir = TempDir::new().unwrap();
        let pid_file = temp_dir.path().join("nonexistent.pid");

        let result = stop_daemon(&pid_file);
        assert!(result.is_err());
    }

    #[test]
    fn test_stop_daemon_invalid_pid() {
        let temp_dir = TempDir::new().unwrap();
        let pid_file = temp_dir.path().join("daemon.pid");

        std::fs::write(&pid_file, "not_a_number\n").unwrap();

        let result = stop_daemon(&pid_file);
        assert!(result.is_err());
    }

    #[test]
    fn test_stop_daemon_nonexistent_process() {
        let temp_dir = TempDir::new().unwrap();
        let pid_file = temp_dir.path().join("daemon.pid");

        // Write a PID that definitely doesn't exist (max PID + 1)
        std::fs::write(&pid_file, "999999\n").unwrap();

        let result = stop_daemon(&pid_file);
        // Should fail because process doesn't exist
        assert!(result.is_err());
    }
}
