use std::path::PathBuf;

use serde::Deserialize;
use thiserror::Error;

use crate::inference::{scan_local_models, InputMode};

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("missing required field: CLOUDFLARE_ACCOUNT_ID or workers_ai.account_id")]
    MissingAccountId,
    #[error("missing required field: CLOUDFLARE_API_TOKEN or workers_ai.api_token")]
    MissingApiToken,
    #[error("invalid config file: {0}")]
    ParseError(#[from] toml::de::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid field value: {0}")]
    InvalidValue(String),
}

#[derive(Debug, Clone)]
pub struct WorkersAIConfig {
    pub account_id: String,
    pub api_token: String,
    pub model_id: String,
    pub timeout_ms: u64,
    pub retry_count: u32,
    pub cost_limit_hour: f64,
    pub cost_limit_day: f64,
}

impl WorkersAIConfig {
    const DEFAULT_MODEL_ID: &'static str = "@cf/qwen/qwen3-30b-a3b-fp8";
    const DEFAULT_TIMEOUT_MS: u64 = 1500;
    const DEFAULT_RETRY_COUNT: u32 = 2;
    const DEFAULT_COST_LIMIT_HOUR: f64 = 0.10;
    const DEFAULT_COST_LIMIT_DAY: f64 = 1.00;
}

#[derive(Debug, Clone, Default)]
pub struct LocalLlmConfig {
    pub model_path: Option<PathBuf>,
    pub model_search_paths: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct OllamaConfig {
    pub endpoint: String,
    pub model: String,
}

impl OllamaConfig {
    const DEFAULT_ENDPOINT: &'static str = "http://localhost:11434";
    const DEFAULT_MODEL: &'static str = "qwen2.5:1.5b";
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            endpoint: OllamaConfig::DEFAULT_ENDPOINT.to_owned(),
            model: OllamaConfig::DEFAULT_MODEL.to_owned(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LlmimeConfig {
    pub workers_ai: WorkersAIConfig,
    pub local_llm: LocalLlmConfig,
    pub ollama: OllamaConfig,
    pub mode: InputMode,
}

/// Raw TOML deserialization targets
#[derive(Debug, Deserialize, Default)]
struct RawConfig {
    workers_ai: Option<RawWorkersAI>,
    local_llm: Option<RawLocalLlm>,
    ollama: Option<RawOllama>,
    input_mode: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct RawWorkersAI {
    account_id: Option<String>,
    api_token: Option<String>,
    model_id: Option<String>,
    timeout_ms: Option<u64>,
    retry_count: Option<u32>,
    cost_limit_hour: Option<f64>,
    cost_limit_day: Option<f64>,
}

#[derive(Debug, Deserialize, Default)]
struct RawLocalLlm {
    model_path: Option<PathBuf>,
    model_search_paths: Option<Vec<PathBuf>>,
}

#[derive(Debug, Deserialize, Default)]
struct RawOllama {
    endpoint: Option<String>,
    model: Option<String>,
}

impl LlmimeConfig {
    /// Load config with priority: env vars > config.toml > defaults.
    /// Also loads .env.local via dotenvy before reading env vars.
    pub fn load() -> Result<Self, ConfigError> {
        dotenvy::from_filename(".env.local").ok();
        Self::load_inner()
    }

    fn load_inner() -> Result<Self, ConfigError> {
        let raw = Self::load_toml_file()?;
        let raw_wai = raw.workers_ai.unwrap_or_default();
        let raw_local = raw.local_llm.unwrap_or_default();
        let raw_ollama = raw.ollama.unwrap_or_default();

        let account_id = std::env::var("CLOUDFLARE_ACCOUNT_ID")
            .ok()
            .or(raw_wai.account_id)
            .ok_or(ConfigError::MissingAccountId)?;

        let api_token = std::env::var("CLOUDFLARE_API_TOKEN")
            .ok()
            .or(raw_wai.api_token)
            .ok_or(ConfigError::MissingApiToken)?;

        let model_id = std::env::var("WORKERS_AI_MODEL_ID")
            .ok()
            .or(raw_wai.model_id)
            .unwrap_or_else(|| WorkersAIConfig::DEFAULT_MODEL_ID.to_owned());

        let timeout_ms = std::env::var("WORKERS_AI_TIMEOUT_MS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .or(raw_wai.timeout_ms)
            .unwrap_or(WorkersAIConfig::DEFAULT_TIMEOUT_MS);

        let retry_count = std::env::var("WORKERS_AI_RETRY_COUNT")
            .ok()
            .and_then(|v| v.parse::<u32>().ok())
            .or(raw_wai.retry_count)
            .unwrap_or(WorkersAIConfig::DEFAULT_RETRY_COUNT);

        let cost_limit_hour = std::env::var("WORKERS_AI_COST_LIMIT_HOUR")
            .ok()
            .and_then(|v| v.parse::<f64>().ok())
            .or(raw_wai.cost_limit_hour)
            .unwrap_or(WorkersAIConfig::DEFAULT_COST_LIMIT_HOUR);

        let cost_limit_day = std::env::var("WORKERS_AI_COST_LIMIT_DAY")
            .ok()
            .and_then(|v| v.parse::<f64>().ok())
            .or(raw_wai.cost_limit_day)
            .unwrap_or(WorkersAIConfig::DEFAULT_COST_LIMIT_DAY);

        let mode = std::env::var("LLMIME_INPUT_MODE")
            .ok()
            .or(raw.input_mode)
            .map(|s| s.parse::<InputMode>().map_err(ConfigError::InvalidValue))
            .transpose()?
            .unwrap_or_default();

        Ok(LlmimeConfig {
            workers_ai: WorkersAIConfig {
                account_id,
                api_token,
                model_id,
                timeout_ms,
                retry_count,
                cost_limit_hour,
                cost_limit_day,
            },
            local_llm: LocalLlmConfig {
                model_path: raw_local.model_path,
                model_search_paths: raw_local.model_search_paths.unwrap_or_default(),
            },
            ollama: OllamaConfig {
                endpoint: std::env::var("OLLAMA_ENDPOINT")
                    .ok()
                    .or(raw_ollama.endpoint)
                    .unwrap_or_else(|| OllamaConfig::DEFAULT_ENDPOINT.to_owned()),
                model: std::env::var("OLLAMA_MODEL")
                    .ok()
                    .or(raw_ollama.model)
                    .unwrap_or_else(|| OllamaConfig::DEFAULT_MODEL.to_owned()),
            },
            mode,
        })
    }

    fn load_toml_file() -> Result<RawConfig, ConfigError> {
        let path = Self::config_path();
        if !path.exists() {
            return Ok(RawConfig::default());
        }
        let content = std::fs::read_to_string(&path)?;
        let raw: RawConfig = toml::from_str(&content)?;
        Ok(raw)
    }

    /// Auto-detect a local GGUF model: explicit config path first, then scan well-known dirs.
    pub fn detect_local_model(&self) -> Option<PathBuf> {
        if let Some(ref p) = self.local_llm.model_path {
            return Some(p.clone());
        }
        let candidates = scan_local_models(&self.local_llm.model_search_paths);
        if let Some(first) = candidates.into_iter().next() {
            eprintln!(
                "[llmime] auto-detected local model: {} ({})",
                first.filename,
                first.path.display()
            );
            return Some(first.path);
        }
        None
    }

    fn config_path() -> PathBuf {
        let base = std::env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| dirs::config_dir().unwrap_or_else(|| PathBuf::from("~/.config")));
        base.join("llmime").join("config.toml")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    fn clear_env_vars() {
        for key in &[
            "CLOUDFLARE_ACCOUNT_ID",
            "CLOUDFLARE_API_TOKEN",
            "WORKERS_AI_MODEL_ID",
            "WORKERS_AI_TIMEOUT_MS",
            "WORKERS_AI_RETRY_COUNT",
            "WORKERS_AI_COST_LIMIT_HOUR",
            "WORKERS_AI_COST_LIMIT_DAY",
            "LLMIME_INPUT_MODE",
            "XDG_CONFIG_HOME",
        ] {
            std::env::remove_var(key);
        }
    }

    /// Helper: point XDG_CONFIG_HOME to a temp dir containing llmime/config.toml
    fn use_toml_in_xdg(toml_content: &str) -> tempfile::TempDir {
        let dir = tempfile::TempDir::new().unwrap();
        let llmime_dir = dir.path().join("llmime");
        std::fs::create_dir_all(&llmime_dir).unwrap();
        std::fs::write(llmime_dir.join("config.toml"), toml_content).unwrap();
        std::env::set_var("XDG_CONFIG_HOME", dir.path());
        dir
    }

    #[test]
    #[serial]
    fn test_env_vars_take_priority_over_toml() {
        clear_env_vars();
        let _dir = use_toml_in_xdg(
            "[workers_ai]\naccount_id = \"toml_account\"\napi_token = \"toml_token\"\nmodel_id = \"toml_model\"\n",
        );
        std::env::set_var("CLOUDFLARE_ACCOUNT_ID", "env_account");
        std::env::set_var("CLOUDFLARE_API_TOKEN", "env_token");
        std::env::set_var("WORKERS_AI_MODEL_ID", "env_model");

        let cfg = LlmimeConfig::load_inner().unwrap();
        assert_eq!(cfg.workers_ai.account_id, "env_account");
        assert_eq!(cfg.workers_ai.api_token, "env_token");
        assert_eq!(cfg.workers_ai.model_id, "env_model");

        clear_env_vars();
    }

    #[test]
    #[serial]
    fn test_toml_config_loaded_when_no_env() {
        clear_env_vars();
        let _dir = use_toml_in_xdg(
            "[workers_ai]\naccount_id = \"toml_acct\"\napi_token = \"toml_tok\"\nmodel_id = \"custom_model\"\ntimeout_ms = 3000\n",
        );

        let cfg = LlmimeConfig::load_inner().unwrap();
        assert_eq!(cfg.workers_ai.account_id, "toml_acct");
        assert_eq!(cfg.workers_ai.api_token, "toml_tok");
        assert_eq!(cfg.workers_ai.model_id, "custom_model");
        assert_eq!(cfg.workers_ai.timeout_ms, 3000);

        clear_env_vars();
    }

    #[test]
    #[serial]
    fn test_default_values_applied() {
        clear_env_vars();
        let _dir = use_toml_in_xdg("[workers_ai]\naccount_id = \"acct\"\napi_token = \"tok\"\n");

        let cfg = LlmimeConfig::load_inner().unwrap();
        assert_eq!(cfg.workers_ai.model_id, WorkersAIConfig::DEFAULT_MODEL_ID);
        assert_eq!(
            cfg.workers_ai.timeout_ms,
            WorkersAIConfig::DEFAULT_TIMEOUT_MS
        );
        assert_eq!(
            cfg.workers_ai.retry_count,
            WorkersAIConfig::DEFAULT_RETRY_COUNT
        );
        assert!(
            (cfg.workers_ai.cost_limit_hour - WorkersAIConfig::DEFAULT_COST_LIMIT_HOUR).abs()
                < f64::EPSILON
        );
        assert!(
            (cfg.workers_ai.cost_limit_day - WorkersAIConfig::DEFAULT_COST_LIMIT_DAY).abs()
                < f64::EPSILON
        );
        assert_eq!(cfg.mode, InputMode::Privacy);

        clear_env_vars();
    }

    #[test]
    #[serial]
    fn test_missing_account_id_returns_error() {
        clear_env_vars();
        let dir = tempfile::TempDir::new().unwrap();
        std::env::set_var("XDG_CONFIG_HOME", dir.path());
        std::env::set_var("CLOUDFLARE_API_TOKEN", "some_token");

        let result = LlmimeConfig::load_inner();
        assert!(matches!(result, Err(ConfigError::MissingAccountId)));

        clear_env_vars();
    }

    #[test]
    #[serial]
    fn test_missing_api_token_returns_error() {
        clear_env_vars();
        let dir = tempfile::TempDir::new().unwrap();
        std::env::set_var("XDG_CONFIG_HOME", dir.path());
        std::env::set_var("CLOUDFLARE_ACCOUNT_ID", "some_account");

        let result = LlmimeConfig::load_inner();
        assert!(matches!(result, Err(ConfigError::MissingApiToken)));

        clear_env_vars();
    }

    #[test]
    #[serial]
    fn test_invalid_input_mode_returns_error() {
        clear_env_vars();
        let _dir = use_toml_in_xdg("[workers_ai]\naccount_id = \"acct\"\napi_token = \"tok\"\n");
        std::env::set_var("LLMIME_INPUT_MODE", "invalid_mode");

        let result = LlmimeConfig::load_inner();
        assert!(matches!(result, Err(ConfigError::InvalidValue(_))));

        clear_env_vars();
    }
}
