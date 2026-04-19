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
            let api_key = cfg.workers_ai_api_key.clone().unwrap_or_default();
            let account_id = cfg.workers_ai_account_id.clone().unwrap_or_default();
            let workers: DynInferencer = Arc::new(WorkersAIInferencer::new(
                account_id,
                api_key,
                "@cf/qwen/qwen3-30b-a3b-fp8".to_string(),
            ));
            FallbackChain::new(workers, vec![ngram], Duration::from_millis(300))
        }
        InputMode::Pro => {
            let local_llm: DynInferencer =
                Arc::new(LocalLlmInferencer::new(cfg.llm_model_path.clone()));
            FallbackChain::new(local_llm, vec![ngram], Duration::from_millis(800))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_privacy_mode_uses_ngram_primary_no_fallbacks() {
        let cfg = LlmimeConfig::default();
        let chain = default_fallback_chain(InputMode::Privacy, &cfg);
        assert_eq!(chain.primary_name(), "local-ngram");
        assert_eq!(chain.fallback_count(), 0);
    }

    #[test]
    fn test_performance_mode_uses_workers_ai_with_ngram_fallback() {
        let cfg = LlmimeConfig {
            workers_ai_api_key: Some("key".to_string()),
            workers_ai_account_id: Some("acct".to_string()),
            llm_model_path: None,
        };
        let chain = default_fallback_chain(InputMode::Performance, &cfg);
        assert_eq!(chain.primary_name(), "workers-ai");
        assert_eq!(chain.fallback_count(), 1);
    }

    #[test]
    fn test_pro_mode_uses_local_llm_with_ngram_fallback() {
        let cfg = LlmimeConfig::default();
        let chain = default_fallback_chain(InputMode::Pro, &cfg);
        assert_eq!(chain.primary_name(), "local-llm");
        assert_eq!(chain.fallback_count(), 1);
    }
}
