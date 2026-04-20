use std::sync::Arc;
use std::time::Duration;

use crate::config::LlmimeConfig;
use crate::inference::{
    fallback_chain::FallbackChain, inferencer::DynInferencer, local_llm::LocalLlmInferencer,
    local_ngram::LocalNgramInferencer, mode::InputMode, workers_ai::WorkersAIInferencer,
};

pub fn default_fallback_chain(mode: InputMode, cfg: &LlmimeConfig) -> FallbackChain {
    let ngram: DynInferencer = Arc::new(LocalNgramInferencer::new_in_memory());

    match mode {
        InputMode::Privacy => FallbackChain::new(ngram, vec![], Duration::MAX),
        InputMode::Performance => {
            let workers: DynInferencer = Arc::new(WorkersAIInferencer::new(
                cfg.workers_ai.account_id.clone(),
                cfg.workers_ai.api_token.clone(),
                cfg.workers_ai.model_id.clone(),
            ));
            FallbackChain::new(workers, vec![ngram], Duration::from_millis(300))
        }
        InputMode::Pro => {
            let local_llm: DynInferencer =
                Arc::new(LocalLlmInferencer::new(cfg.local_llm.model_path.clone()));
            FallbackChain::new(local_llm, vec![ngram], Duration::from_millis(800))
        }
        // Hybrid: callers must resolve via ModeManager::effective_mode() first.
        // Unresolved Hybrid uses Privacy-safe ngram-only chain (NF-032).
        InputMode::Hybrid => FallbackChain::new(ngram, vec![], Duration::MAX),
    }
}

#[cfg(test)]
fn make_test_config(account_id: &str, api_token: &str) -> LlmimeConfig {
    use crate::config::{LocalLlmConfig, WorkersAIConfig};
    LlmimeConfig {
        workers_ai: WorkersAIConfig {
            account_id: account_id.to_string(),
            api_token: api_token.to_string(),
            model_id: "@cf/qwen/qwen3-30b-a3b-fp8".to_string(),
            timeout_ms: 1500,
            retry_count: 2,
            cost_limit_hour: 0.10,
            cost_limit_day: 1.00,
        },
        local_llm: LocalLlmConfig::default(),
        mode: InputMode::Privacy,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_privacy_mode_uses_ngram_primary_no_fallbacks() {
        let cfg = make_test_config("", "");
        let chain = default_fallback_chain(InputMode::Privacy, &cfg);
        assert_eq!(chain.primary_name(), "local-ngram");
        assert_eq!(chain.fallback_count(), 0);
    }

    #[test]
    fn test_performance_mode_uses_workers_ai_with_ngram_fallback() {
        let cfg = make_test_config("acct", "key");
        let chain = default_fallback_chain(InputMode::Performance, &cfg);
        assert_eq!(chain.primary_name(), "workers-ai");
        assert_eq!(chain.fallback_count(), 1);
    }

    #[test]
    fn test_pro_mode_uses_local_llm_with_ngram_fallback() {
        let cfg = make_test_config("", "");
        let chain = default_fallback_chain(InputMode::Pro, &cfg);
        assert_eq!(chain.primary_name(), "local-llm");
        assert_eq!(chain.fallback_count(), 1);
    }
}
