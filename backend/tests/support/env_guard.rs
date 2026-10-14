//! Process-environment save/restore guard for DB-backed suites.
//!
//! Configuration that handlers read per request from the environment (SCIM
//! scope, feature flags) is installed for the duration of a test and restored
//! on drop. Tests using this remain serialized with their suite's `SERIAL`
//! guard.

/// Saves the previous values of a set of environment variables and restores
/// them when dropped.
pub struct EnvGuard {
    saved: Vec<(&'static str, Option<String>)>,
}

impl EnvGuard {
    /// Set the given variables (`None` removes the variable) and return the
    /// guard that restores the previous values on drop.
    pub fn set(vars: Vec<(&'static str, Option<String>)>) -> Self {
        let saved = vars
            .iter()
            .map(|(name, _)| (*name, std::env::var(name).ok()))
            .collect();
        for (name, value) in &vars {
            match value {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
        }
        EnvGuard { saved }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (name, value) in &self.saved {
            match value {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
        }
    }
}
