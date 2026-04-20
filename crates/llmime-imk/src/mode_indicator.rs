//! macOS mode indicator: displays current InputMode in the menu bar or candidate window.
//!
//! Privacy mode:     🔒 (local processing)
//! Performance mode: ⚡ (cloud LLM)
//! Pro mode:         ⚡ (cloud LLM, same visual as Performance)
//! Hybrid mode:      resolved to Privacy or Performance by effective_mode()

use llmime_core::InputMode;

pub struct ModeIndicator {
    current_mode: InputMode,
}

impl ModeIndicator {
    pub fn new() -> Self {
        Self {
            current_mode: InputMode::Privacy,
        }
    }

    /// Update the displayed mode. Must complete within 100ms (F-064).
    pub fn update(&mut self, effective_mode: InputMode) {
        self.current_mode = effective_mode;
        self.render();
    }

    pub fn current_mode(&self) -> InputMode {
        self.current_mode
    }

    /// Returns the icon string for the current mode.
    pub fn icon(&self) -> &'static str {
        mode_icon(self.current_mode)
    }

    fn render(&self) {
        #[cfg(target_os = "macos")]
        macos::update_status_item(self.icon());
        // Non-macOS: no-op
    }
}

impl Default for ModeIndicator {
    fn default() -> Self {
        Self::new()
    }
}

fn mode_icon(mode: InputMode) -> &'static str {
    match mode {
        InputMode::Privacy => "🔒",
        InputMode::Performance | InputMode::Pro => "⚡",
        // Hybrid is always resolved to Privacy or Performance before reaching here,
        // but fall back to Privacy (safe side) if it somehow arrives.
        InputMode::Hybrid => "🔒",
    }
}

#[cfg(target_os = "macos")]
mod macos {
    /// Updates the NSStatusItem title with the given icon string.
    /// Actual NSStatusItem integration requires ObjC bridge; this stub is ready for wiring.
    pub fn update_status_item(_icon: &str) {
        // TODO: call objc2 NSStatusItem setTitle here when ObjC bridge is wired.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use llmime_core::{
        field::{FieldClass, FieldClassifier, FocusWatcher},
        ModeManager,
    };
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    use std::time::Instant;

    // ── ModeIndicator unit tests ──────────────────────────────────────────

    #[test]
    fn privacy_mode_shows_lock_icon() {
        let mut indicator = ModeIndicator::new();
        indicator.update(InputMode::Privacy);
        assert_eq!(indicator.icon(), "🔒");
        assert_eq!(indicator.current_mode(), InputMode::Privacy);
    }

    #[test]
    fn performance_mode_shows_lightning_icon() {
        let mut indicator = ModeIndicator::new();
        indicator.update(InputMode::Performance);
        assert_eq!(indicator.icon(), "⚡");
        assert_eq!(indicator.current_mode(), InputMode::Performance);
    }

    #[test]
    fn pro_mode_shows_lightning_icon() {
        let mut indicator = ModeIndicator::new();
        indicator.update(InputMode::Pro);
        assert_eq!(indicator.icon(), "⚡");
    }

    #[test]
    fn hybrid_fallback_shows_lock_icon() {
        let mut indicator = ModeIndicator::new();
        indicator.update(InputMode::Hybrid);
        assert_eq!(indicator.icon(), "🔒");
    }

    // ── FocusWatcher → ModeIndicator update path ─────────────────────────

    struct StubClassifier {
        sensitive: AtomicBool,
    }

    impl StubClassifier {
        fn new(sensitive: bool) -> Arc<Self> {
            Arc::new(Self {
                sensitive: AtomicBool::new(sensitive),
            })
        }

        fn set_sensitive(&self, val: bool) {
            self.sensitive.store(val, Ordering::SeqCst);
        }
    }

    impl FieldClassifier for StubClassifier {
        fn classify(&self) -> FieldClass {
            if self.sensitive.load(Ordering::SeqCst) {
                FieldClass::Sensitive
            } else {
                FieldClass::NonSensitive
            }
        }
    }

    #[test]
    fn focus_watcher_updates_indicator_privacy_on_sensitive() {
        let stub = StubClassifier::new(true);
        let watcher = FocusWatcher::new(stub.clone());
        let mgr = ModeManager::new(InputMode::Hybrid);
        let mut indicator = ModeIndicator::new();

        let field = watcher.on_focus_change();
        let effective = mgr.effective_mode(field, InputMode::Hybrid);
        indicator.update(effective);

        assert_eq!(indicator.current_mode(), InputMode::Privacy);
        assert_eq!(indicator.icon(), "🔒");
    }

    #[test]
    fn focus_watcher_updates_indicator_performance_on_non_sensitive() {
        let stub = StubClassifier::new(false);
        let watcher = FocusWatcher::new(stub.clone());
        let mgr = ModeManager::new(InputMode::Hybrid);
        let mut indicator = ModeIndicator::new();

        let field = watcher.on_focus_change();
        let effective = mgr.effective_mode(field, InputMode::Hybrid);
        indicator.update(effective);

        assert_eq!(indicator.current_mode(), InputMode::Performance);
        assert_eq!(indicator.icon(), "⚡");
    }

    #[test]
    fn focus_change_to_sensitive_switches_indicator_within_100ms() {
        let stub = StubClassifier::new(false);
        let watcher = FocusWatcher::new(stub.clone());
        let mgr = ModeManager::new(InputMode::Hybrid);
        let mut indicator = ModeIndicator::new();

        stub.set_sensitive(true);

        let start = Instant::now();
        let field = watcher.on_focus_change();
        let effective = mgr.effective_mode(field, InputMode::Hybrid);
        indicator.update(effective);
        let elapsed = start.elapsed();

        assert_eq!(indicator.current_mode(), InputMode::Privacy);
        assert!(
            elapsed.as_millis() < 100,
            "update path took {}ms, expected <100ms (F-064)",
            elapsed.as_millis()
        );
    }
}
