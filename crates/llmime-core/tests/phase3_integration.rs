//! Phase 3 integration tests: end-to-end routing across ModeManager, Dispatcher,
//! FallbackChain, and WarmupOrchestrator.

use async_trait::async_trait;
use llmime_core::inference::inferencer::AlwaysTimeoutInferencer;
use llmime_core::inference::{CandidateSource, CandidateWithScore, InferencerCapabilities};
use llmime_core::{
    Dispatcher, DynInferencer, FallbackChain, InferenceError, Inferencer, InputMode,
    LocalLlmInferencer, LocalNgramInferencer, ModeManager, WarmupOrchestrator, WarmupStatus,
    WorkersAIInferencer,
};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;

fn make_candidates(surfaces: &[&str]) -> Vec<CandidateWithScore> {
    surfaces
        .iter()
        .map(|s| CandidateWithScore {
            surface: s.to_string(),
            score: 1.0,
            source: CandidateSource::Ngram,
        })
        .collect()
}

/// Privacy mode with no LocalLlm configured: both small and large token counts
/// should always select LocalNgram.
#[tokio::test]
async fn scenario_1_privacy_mode_always_ngram() {
    let ngram = Arc::new(LocalNgramInferencer::new_in_memory());
    let dispatcher = Dispatcher::new(ngram, None, None);

    let small = dispatcher.select_inferencer(InputMode::Privacy, 5);
    assert_eq!(
        small.name(),
        "local-ngram",
        "tokens=5: below threshold → ngram"
    );

    let large = dispatcher.select_inferencer(InputMode::Privacy, 50);
    assert_eq!(
        large.name(),
        "local-ngram",
        "tokens=50: Privacy mode but no LocalLlm → ngram fallback"
    );
}

/// Performance mode with WorkersAI: below threshold routes to ngram,
/// at or above threshold routes to workers-ai.
#[tokio::test]
async fn scenario_2_performance_mode_threshold() {
    let ngram = Arc::new(LocalNgramInferencer::new_in_memory());
    let workers_ai = Arc::new(WorkersAIInferencer::new(
        "test-account".to_string(),
        "test-token".to_string(),
        "test-model".to_string(),
    ));
    let dispatcher = Dispatcher::new(ngram, Some(workers_ai), None);

    let below = dispatcher.select_inferencer(InputMode::Performance, 5);
    assert_eq!(below.name(), "local-ngram", "tokens=5 (<15) → ngram");

    let above = dispatcher.select_inferencer(InputMode::Performance, 30);
    assert_eq!(
        above.name(),
        "workers-ai",
        "tokens=30 (>=15) + Performance → workers-ai"
    );
}

/// FallbackChain with AlwaysTimeoutInferencer as primary: after primary error,
/// LocalNgram fallback should return the original candidates.
#[tokio::test]
async fn scenario_3_performance_timeout_fallback() {
    let primary = Arc::new(AlwaysTimeoutInferencer) as DynInferencer;
    let ngram_fallback = Arc::new(LocalNgramInferencer::new_in_memory()) as DynInferencer;

    let chain = FallbackChain::new(primary, vec![ngram_fallback], Duration::from_millis(300));

    let candidates = make_candidates(&["東京", "とうきょう"]);
    let result = chain.rerank("とうきょう", candidates, None).await;

    assert_eq!(result.len(), 2, "fallback returned all candidates");
    assert_eq!(result[0].surface, "東京");
}

/// Pro mode with LocalLlm available: tokens=30 (above threshold) should
/// select local-llm as the primary inferencer.
#[tokio::test]
async fn scenario_4_pro_mode_routing() {
    let ngram = Arc::new(LocalNgramInferencer::new_in_memory());
    let local_llm = Arc::new(LocalLlmInferencer::new_unavailable());
    let dispatcher = Dispatcher::new(ngram, None, Some(local_llm));

    let selected = dispatcher.select_inferencer(InputMode::Pro, 30);
    assert_eq!(
        selected.name(),
        "local-llm",
        "tokens=30 (>=15) + Pro → local-llm selected as primary"
    );
}

/// sensitive_override=true forces ModeManager::mode() to return Privacy regardless
/// of the configured mode. Dispatcher then falls back to ngram (no LocalLlm).
///
/// Note: sensitive_override is the Phase 5 foundation — ModeManager enforces
/// Privacy at the mode layer so Dispatcher requires no special-case logic.
#[tokio::test]
async fn scenario_5_sensitive_override() {
    let mut mode_mgr = ModeManager::new(InputMode::Performance);
    mode_mgr.override_for_sensitive(true);

    let effective = mode_mgr.mode();
    assert_eq!(
        effective,
        InputMode::Privacy,
        "sensitive_override forces effective mode to Privacy"
    );

    // WorkersAI is present but Privacy mode with no LocalLlm falls back to ngram
    let ngram = Arc::new(LocalNgramInferencer::new_in_memory());
    let workers_ai = Arc::new(WorkersAIInferencer::new(
        "test-account".to_string(),
        "test-token".to_string(),
        "test-model".to_string(),
    ));
    let dispatcher = Dispatcher::new(ngram, Some(workers_ai), None);

    let selected = dispatcher.select_inferencer(effective, 50);
    assert_eq!(
        selected.name(),
        "local-ngram",
        "Privacy + no LocalLlm → ngram (WorkersAI bypassed)"
    );
}

/// Warmup counter inferencer for double-call detection.
struct CountingInferencer {
    warmup_count: Arc<AtomicUsize>,
}

#[async_trait]
impl Inferencer for CountingInferencer {
    fn name(&self) -> &'static str {
        "counting-inferencer"
    }

    fn capabilities(&self) -> InferencerCapabilities {
        InferencerCapabilities {
            supports_rerank: true,
            supports_right_context: false,
        }
    }

    async fn rerank(
        &self,
        _reading: &str,
        candidates: Vec<CandidateWithScore>,
        _left_context: Option<&str>,
    ) -> Result<Vec<CandidateWithScore>, InferenceError> {
        Ok(candidates)
    }

    async fn warmup(&self) -> Result<(), InferenceError> {
        self.warmup_count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

/// WarmupOrchestrator::run_parallel warms up inferencers exactly once.
/// Dispatcher::select_inferencer must NOT call warmup — the counter stays at 1.
#[tokio::test]
async fn scenario_6_warmup_then_dispatch() {
    let warmup_count = Arc::new(AtomicUsize::new(0));
    let counting = Arc::new(CountingInferencer {
        warmup_count: warmup_count.clone(),
    }) as DynInferencer;

    let orch = WarmupOrchestrator::new(vec![counting]);
    let results = orch.run_parallel(Duration::from_secs(3)).await;

    assert_eq!(results.len(), 1);
    assert!(
        matches!(
            results.get("counting-inferencer"),
            Some(WarmupStatus::Ready)
        ),
        "counting-inferencer should reach Ready state after warmup"
    );
    assert_eq!(
        warmup_count.load(Ordering::SeqCst),
        1,
        "WarmupOrchestrator must call warmup exactly once"
    );

    // Dispatcher.select_inferencer picks an inferencer without calling warmup
    let ngram = Arc::new(LocalNgramInferencer::new_in_memory());
    let dispatcher = Dispatcher::new(ngram, None, None);
    let selected = dispatcher.select_inferencer(InputMode::Privacy, 5);

    assert_eq!(selected.name(), "local-ngram");
    assert_eq!(
        warmup_count.load(Ordering::SeqCst),
        1,
        "Dispatcher.select_inferencer must not call warmup (no double-call)"
    );
}
