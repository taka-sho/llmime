use llmime_core::inference::InputMode;

pub struct ModeIndicator {
    current_mode: InputMode,
    override_active: bool,
}

impl ModeIndicator {
    pub fn new() -> Self {
        Self {
            current_mode: InputMode::Privacy,
            override_active: false,
        }
    }

    /// Update the displayed mode. Called by FocusWatcher after effective_mode() resolves.
    /// Target: ≤100ms latency from focus change to indicator update (F-064).
    pub fn update(&mut self, effective_mode: InputMode) {
        self.update_with_override(effective_mode, false);
    }

    /// Update with override state. When `is_override` is true, "(上書き中)" suffix is shown.
    pub fn update_with_override(&mut self, effective_mode: InputMode, is_override: bool) {
        self.current_mode = effective_mode;
        self.override_active = is_override;
        #[cfg(target_os = "windows")]
        self.render_windows();
    }

    pub fn current_mode(&self) -> InputMode {
        self.current_mode
    }

    pub fn is_override_active(&self) -> bool {
        self.override_active
    }

    /// Short label used for LanguageBar icon text or system tray tooltip.
    /// Privacy → "P" (local processing), Performance/Pro → "C" (cloud LLM).
    pub fn label(&self) -> &'static str {
        match self.current_mode {
            InputMode::Privacy => "P",
            InputMode::Performance | InputMode::Pro => "C",
            InputMode::Hybrid => "H",
        }
    }

    /// Returns the display label including override suffix if active.
    pub fn display_label(&self) -> String {
        if self.override_active {
            format!("{} (上書き中)", self.label())
        } else {
            self.label().to_string()
        }
    }

    #[cfg(target_os = "windows")]
    fn render_windows(&self) {
        // Windows: update LanguageBar (ITfLangBarItem) or system tray (NotifyIcon).
        log::debug!(
            "ModeIndicator: mode={:?} label={}",
            self.current_mode,
            self.display_label()
        );
    }
}

impl Default for ModeIndicator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use llmime_core::field::FieldClass;
    use llmime_core::inference::{InputMode, ModeManager};

    #[test]
    fn update_privacy_mode() {
        let mut indicator = ModeIndicator::new();
        indicator.update(InputMode::Privacy);
        assert_eq!(indicator.current_mode(), InputMode::Privacy);
        assert_eq!(indicator.label(), "P");
    }

    #[test]
    fn update_performance_mode() {
        let mut indicator = ModeIndicator::new();
        indicator.update(InputMode::Performance);
        assert_eq!(indicator.current_mode(), InputMode::Performance);
        assert_eq!(indicator.label(), "C");
    }

    #[test]
    fn update_pro_mode() {
        let mut indicator = ModeIndicator::new();
        indicator.update(InputMode::Pro);
        assert_eq!(indicator.current_mode(), InputMode::Pro);
        assert_eq!(indicator.label(), "C");
    }

    #[test]
    fn update_hybrid_mode() {
        let mut indicator = ModeIndicator::new();
        indicator.update(InputMode::Hybrid);
        assert_eq!(indicator.current_mode(), InputMode::Hybrid);
        assert_eq!(indicator.label(), "H");
    }

    /// Simulates FocusWatcher → effective_mode() → ModeIndicator::update() path.
    /// Hybrid base mode: sensitive field → Privacy indicator.
    #[test]
    fn focus_watcher_path_hybrid_sensitive() {
        let manager = ModeManager::new(InputMode::Hybrid);
        let mut indicator = ModeIndicator::new();

        let effective = manager.effective_mode(FieldClass::Sensitive, InputMode::Hybrid);
        indicator.update(effective);

        assert_eq!(indicator.current_mode(), InputMode::Privacy);
        assert_eq!(indicator.label(), "P");
    }

    /// FocusWatcher path: hybrid base mode, non-sensitive field → Performance indicator.
    #[test]
    fn focus_watcher_path_hybrid_non_sensitive() {
        let manager = ModeManager::new(InputMode::Hybrid);
        let mut indicator = ModeIndicator::new();

        let effective = manager.effective_mode(FieldClass::NonSensitive, InputMode::Hybrid);
        indicator.update(effective);

        assert_eq!(indicator.current_mode(), InputMode::Performance);
        assert_eq!(indicator.label(), "C");
    }

    /// FocusWatcher path: unknown field → Privacy (NF-032 safety default).
    #[test]
    fn focus_watcher_path_hybrid_unknown_field_defaults_privacy() {
        let manager = ModeManager::new(InputMode::Hybrid);
        let mut indicator = ModeIndicator::new();

        let effective = manager.effective_mode(FieldClass::Unknown, InputMode::Hybrid);
        indicator.update(effective);

        assert_eq!(indicator.current_mode(), InputMode::Privacy);
    }

    #[test]
    fn default_mode_is_privacy() {
        let indicator = ModeIndicator::default();
        assert_eq!(indicator.current_mode(), InputMode::Privacy);
    }

    // ── Override display tests ────────────────────────────────────────────

    #[test]
    fn override_active_shows_override_suffix() {
        let mut indicator = ModeIndicator::new();
        indicator.update_with_override(InputMode::Performance, true);
        assert!(indicator.is_override_active());
        assert!(indicator.display_label().contains("上書き中"));
        assert_eq!(indicator.current_mode(), InputMode::Performance);
    }

    #[test]
    fn no_override_shows_plain_label() {
        let mut indicator = ModeIndicator::new();
        indicator.update_with_override(InputMode::Privacy, false);
        assert!(!indicator.is_override_active());
        assert_eq!(indicator.display_label(), "P");
    }

    #[test]
    fn update_without_override_clears_override_flag() {
        let mut indicator = ModeIndicator::new();
        indicator.update_with_override(InputMode::Performance, true);
        assert!(indicator.is_override_active());
        indicator.update(InputMode::Privacy);
        assert!(!indicator.is_override_active());
    }
}
