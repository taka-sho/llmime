use std::time::{Duration, Instant};

use super::InputMode;

/// Manages user-initiated temporary mode overrides during Hybrid mode.
/// Overrides are session-scoped: they reset on application restart.
/// An expired override returns `None` from `effective_override()`.
pub struct OverrideManager {
    override_mode: Option<InputMode>,
    expires_at: Option<Instant>,
}

impl OverrideManager {
    pub fn new() -> Self {
        Self {
            override_mode: None,
            expires_at: None,
        }
    }

    /// Set a temporary override for `duration`. Replaces any existing override.
    pub fn set_override(&mut self, mode: InputMode, duration: Duration) {
        self.override_mode = Some(mode);
        self.expires_at = Some(Instant::now() + duration);
    }

    /// Set a permanent (session-scoped) override with no expiry.
    pub fn set_permanent_override(&mut self, mode: InputMode) {
        self.override_mode = Some(mode);
        self.expires_at = None;
    }

    /// Clear any active override immediately.
    pub fn clear_override(&mut self) {
        self.override_mode = None;
        self.expires_at = None;
    }

    /// Returns the active override mode, or `None` if expired or not set.
    pub fn effective_override(&self) -> Option<InputMode> {
        match (self.override_mode, self.expires_at) {
            (Some(mode), Some(exp)) if Instant::now() < exp => Some(mode),
            (Some(mode), None) => Some(mode),
            _ => None,
        }
    }

    /// Returns true if an override is currently active (not expired).
    pub fn is_active(&self) -> bool {
        self.effective_override().is_some()
    }
}

impl Default for OverrideManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn set_override_returns_mode() {
        let mut mgr = OverrideManager::new();
        mgr.set_override(InputMode::Privacy, Duration::from_secs(60));
        assert_eq!(mgr.effective_override(), Some(InputMode::Privacy));
    }

    #[test]
    fn clear_override_returns_none() {
        let mut mgr = OverrideManager::new();
        mgr.set_override(InputMode::Performance, Duration::from_secs(60));
        mgr.clear_override();
        assert_eq!(mgr.effective_override(), None);
    }

    #[test]
    fn expired_override_returns_none() {
        let mut mgr = OverrideManager::new();
        mgr.set_override(InputMode::Performance, Duration::from_millis(10));
        sleep(Duration::from_millis(20));
        assert_eq!(mgr.effective_override(), None);
    }

    #[test]
    fn permanent_override_not_expired() {
        let mut mgr = OverrideManager::new();
        mgr.set_permanent_override(InputMode::Privacy);
        assert_eq!(mgr.effective_override(), Some(InputMode::Privacy));
        assert!(mgr.is_active());
    }

    #[test]
    fn no_override_by_default() {
        let mgr = OverrideManager::new();
        assert_eq!(mgr.effective_override(), None);
        assert!(!mgr.is_active());
    }

    #[test]
    fn override_replaces_previous() {
        let mut mgr = OverrideManager::new();
        mgr.set_override(InputMode::Privacy, Duration::from_secs(60));
        mgr.set_override(InputMode::Performance, Duration::from_secs(60));
        assert_eq!(mgr.effective_override(), Some(InputMode::Performance));
    }
}
