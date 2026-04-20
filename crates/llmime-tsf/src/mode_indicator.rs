use llmime_core::inference::InputMode;

pub struct ModeIndicator {
    current_mode: InputMode,
}

impl ModeIndicator {
    pub fn new() -> Self {
        Self {
            current_mode: InputMode::Privacy,
        }
    }

    /// Update the displayed mode. Called by FocusWatcher after effective_mode() resolves.
    /// Target: ≤100ms latency from focus change to indicator update (F-064).
    pub fn update(&mut self, effective_mode: InputMode) {
        self.current_mode = effective_mode;
        #[cfg(target_os = "windows")]
        self.render_windows();
    }

    pub fn current_mode(&self) -> InputMode {
        self.current_mode
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

    #[cfg(target_os = "windows")]
    fn render_windows(&self) {
        // Windows: update LanguageBar (ITfLangBarItem) or system tray (NotifyIcon).
        // Full COM/Win32 integration deferred to P5-T8 (LanguageBar wiring).
        log::debug!(
            "ModeIndicator: mode={:?} label={}",
            self.current_mode,
            self.label()
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
}
