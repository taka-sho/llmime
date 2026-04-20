use std::path::Path;
use std::str::FromStr;

use crate::field::FieldClass;

use super::override_manager::OverrideManager;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InputMode {
    #[default]
    Privacy,
    Performance,
    Pro,
    Hybrid,
}

impl FromStr for InputMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "privacy" => Ok(InputMode::Privacy),
            "performance" => Ok(InputMode::Performance),
            "pro" => Ok(InputMode::Pro),
            "hybrid" => Ok(InputMode::Hybrid),
            other => Err(format!("unknown input_mode: {}", other)),
        }
    }
}

pub struct ModeManager {
    current: InputMode,
    sensitive_override: bool,
}

impl ModeManager {
    pub fn new(initial: InputMode) -> Self {
        Self {
            current: initial,
            sensitive_override: false,
        }
    }

    pub fn mode(&self) -> InputMode {
        if self.sensitive_override {
            InputMode::Privacy
        } else {
            self.current
        }
    }

    pub fn set_mode(&mut self, mode: InputMode) {
        self.current = mode;
    }

    pub fn override_for_sensitive(&mut self, sensitive: bool) {
        self.sensitive_override = sensitive;
    }

    /// Hybrid mode resolution: maps (base=Hybrid, focus) → concrete InputMode.
    /// NF-032: Unknown → Privacy (WorkersAI never called).
    pub fn effective_mode(&self, focus: FieldClass, base: InputMode) -> InputMode {
        match base {
            InputMode::Hybrid => match focus {
                FieldClass::Sensitive => InputMode::Privacy,
                FieldClass::NonSensitive => InputMode::Performance,
                FieldClass::Unknown => InputMode::Privacy,
            },
            other => other,
        }
    }

    /// Hybrid mode resolution with user override support.
    /// If `override_mgr` has an active override, it takes priority over the Hybrid resolution.
    pub fn effective_mode_with_override(
        &self,
        focus: FieldClass,
        base: InputMode,
        override_mgr: &OverrideManager,
    ) -> InputMode {
        override_mgr
            .effective_override()
            .unwrap_or_else(|| self.effective_mode(focus, base))
    }

    /// Hybrid mode resolution with override support AND unconditional Sensitive/Unknown protection.
    /// NF-032: Sensitive and Unknown fields always resolve to Privacy — no override can change this.
    pub fn effective_mode_with_override_secure(
        &self,
        focus: FieldClass,
        base: InputMode,
        override_mgr: &OverrideManager,
    ) -> InputMode {
        if matches!(focus, FieldClass::Sensitive | FieldClass::Unknown) {
            return InputMode::Privacy;
        }
        override_mgr
            .effective_override()
            .unwrap_or_else(|| self.effective_mode(focus, base))
    }

    /// `input_mode = "privacy"` 形式の設定ファイルから初期モードを読み込む。
    /// キーが見つからない場合はデフォルト (Privacy) を使用。
    pub fn from_config_file(path: &Path) -> Self {
        let mode = std::fs::read_to_string(path)
            .ok()
            .and_then(|content| parse_input_mode_from_config(&content))
            .unwrap_or_default();
        Self::new(mode)
    }
}

fn parse_input_mode_from_config(content: &str) -> Option<InputMode> {
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix("input_mode") {
            let rest = rest.trim();
            if let Some(rest) = rest.strip_prefix('=') {
                let value = rest.trim().trim_matches('"').trim_matches('\'');
                return value.parse::<InputMode>().ok();
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_mode_is_privacy() {
        let mgr = ModeManager::new(InputMode::default());
        assert_eq!(mgr.mode(), InputMode::Privacy);
    }

    #[test]
    fn test_mode_transition() {
        let mut mgr = ModeManager::new(InputMode::Privacy);
        mgr.set_mode(InputMode::Performance);
        assert_eq!(mgr.mode(), InputMode::Performance);
        mgr.set_mode(InputMode::Pro);
        assert_eq!(mgr.mode(), InputMode::Pro);
    }

    #[test]
    fn test_sensitive_override_forces_privacy() {
        let mut mgr = ModeManager::new(InputMode::Performance);
        mgr.override_for_sensitive(true);
        assert_eq!(mgr.mode(), InputMode::Privacy);
    }

    #[test]
    fn test_override_release_restores_original_mode() {
        let mut mgr = ModeManager::new(InputMode::Pro);
        mgr.override_for_sensitive(true);
        assert_eq!(mgr.mode(), InputMode::Privacy);
        mgr.override_for_sensitive(false);
        assert_eq!(mgr.mode(), InputMode::Pro);
    }

    #[test]
    fn test_from_str() {
        assert_eq!("privacy".parse::<InputMode>().unwrap(), InputMode::Privacy);
        assert_eq!(
            "performance".parse::<InputMode>().unwrap(),
            InputMode::Performance
        );
        assert_eq!("pro".parse::<InputMode>().unwrap(), InputMode::Pro);
        assert!("unknown".parse::<InputMode>().is_err());
    }

    #[test]
    fn test_from_config_file_performance() {
        let dir = std::env::temp_dir();
        let path = dir.join("llmime_test_config.toml");
        std::fs::write(&path, "# llmime config\ninput_mode = \"performance\"\n").unwrap();
        let mgr = ModeManager::from_config_file(&path);
        assert_eq!(mgr.mode(), InputMode::Performance);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_from_config_file_missing_uses_default() {
        let path = std::path::Path::new("/tmp/llmime_nonexistent_config.toml");
        let mgr = ModeManager::from_config_file(path);
        assert_eq!(mgr.mode(), InputMode::Privacy);
    }

    #[test]
    fn test_parse_config_ignores_comments() {
        let content = "# input_mode = \"pro\"\ninput_mode = \"performance\"\n";
        assert_eq!(
            parse_input_mode_from_config(content),
            Some(InputMode::Performance)
        );
    }

    #[test]
    fn hybrid_mode_from_str() {
        assert_eq!("hybrid".parse::<InputMode>().unwrap(), InputMode::Hybrid);
    }

    // NF-032 / effective_mode tests (5 required cases)

    #[test]
    fn privacy_mode_always_local() {
        let mgr = ModeManager::new(InputMode::Privacy);
        assert_eq!(
            mgr.effective_mode(FieldClass::Sensitive, InputMode::Privacy),
            InputMode::Privacy
        );
        assert_eq!(
            mgr.effective_mode(FieldClass::NonSensitive, InputMode::Privacy),
            InputMode::Privacy
        );
    }

    #[test]
    fn performance_mode_workers() {
        let mgr = ModeManager::new(InputMode::Performance);
        assert_eq!(
            mgr.effective_mode(FieldClass::NonSensitive, InputMode::Performance),
            InputMode::Performance
        );
    }

    #[test]
    fn hybrid_sensitive_to_privacy() {
        let mgr = ModeManager::new(InputMode::Hybrid);
        assert_eq!(
            mgr.effective_mode(FieldClass::Sensitive, InputMode::Hybrid),
            InputMode::Privacy
        );
    }

    #[test]
    fn hybrid_non_sensitive_to_performance() {
        let mgr = ModeManager::new(InputMode::Hybrid);
        assert_eq!(
            mgr.effective_mode(FieldClass::NonSensitive, InputMode::Hybrid),
            InputMode::Performance
        );
    }

    #[test]
    fn hybrid_unknown_to_privacy() {
        let mgr = ModeManager::new(InputMode::Hybrid);
        assert_eq!(
            mgr.effective_mode(FieldClass::Unknown, InputMode::Hybrid),
            InputMode::Privacy
        );
    }

    // effective_mode_with_override tests

    #[test]
    fn override_takes_priority_over_hybrid_resolution() {
        use super::super::override_manager::OverrideManager;
        use std::time::Duration;

        let mgr = ModeManager::new(InputMode::Hybrid);
        let mut override_mgr = OverrideManager::new();
        override_mgr.set_override(InputMode::Performance, Duration::from_secs(60));

        // Sensitive field would normally → Privacy, but override → Performance
        let result = mgr.effective_mode_with_override(
            FieldClass::Sensitive,
            InputMode::Hybrid,
            &override_mgr,
        );
        assert_eq!(result, InputMode::Performance);
    }

    #[test]
    fn no_override_falls_back_to_effective_mode() {
        use super::super::override_manager::OverrideManager;

        let mgr = ModeManager::new(InputMode::Hybrid);
        let override_mgr = OverrideManager::new();

        let result = mgr.effective_mode_with_override(
            FieldClass::Sensitive,
            InputMode::Hybrid,
            &override_mgr,
        );
        assert_eq!(result, InputMode::Privacy);
    }

    #[test]
    fn expired_override_falls_back_to_effective_mode() {
        use super::super::override_manager::OverrideManager;
        use std::{thread::sleep, time::Duration};

        let mgr = ModeManager::new(InputMode::Hybrid);
        let mut override_mgr = OverrideManager::new();
        override_mgr.set_override(InputMode::Performance, Duration::from_millis(10));
        sleep(Duration::from_millis(20));

        // Expired override: NonSensitive → Performance (from Hybrid resolution)
        let result = mgr.effective_mode_with_override(
            FieldClass::NonSensitive,
            InputMode::Hybrid,
            &override_mgr,
        );
        assert_eq!(result, InputMode::Performance);
    }
}
