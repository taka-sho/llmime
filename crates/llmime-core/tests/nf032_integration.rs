//! NF-032 integration tests: Unknown/Sensitive fields must NEVER call WorkersAI.
//!
//! NF-032: In Hybrid mode, WorkersAI is called only for NonSensitive fields.
//! Unknown or Sensitive fields must always resolve to Privacy (LocalNgram/LocalLlm).

use llmime_core::{
    field::FieldClass, Dispatcher, InputMode, LocalNgramInferencer, ModeManager,
    WorkersAIInferencer,
};
use std::sync::Arc;

/// NF-032: Unknown field + Hybrid → effective_mode = Privacy → never WorkersAI.
#[tokio::test]
async fn nf032_unknown_field_never_calls_workers_ai() {
    let mode_mgr = ModeManager::new(InputMode::Hybrid);
    let effective = mode_mgr.effective_mode(FieldClass::Unknown, InputMode::Hybrid);
    assert_eq!(
        effective,
        InputMode::Privacy,
        "NF-032: Unknown field in Hybrid mode must resolve to Privacy"
    );

    let ngram = Arc::new(LocalNgramInferencer::new_in_memory());
    let workers_ai = Arc::new(WorkersAIInferencer::new(
        "test-account".to_string(),
        "test-token".to_string(),
        "test-model".to_string(),
    ));
    let dispatcher = Dispatcher::new(ngram, Some(workers_ai), None);

    // token_count=50 is above the routing threshold to exercise mode-based selection
    let selected = dispatcher.select_inferencer(effective, 50);
    assert_ne!(
        selected.name(),
        "workers-ai",
        "NF-032: Unknown field must never route to WorkersAI"
    );
    assert_eq!(
        selected.name(),
        "local-ngram",
        "NF-032: Unknown field falls back to local-ngram (Privacy, no LocalLlm configured)"
    );
}

/// NF-032: Sensitive field + Hybrid → effective_mode = Privacy → never WorkersAI.
#[tokio::test]
async fn nf032_sensitive_field_never_calls_workers_ai() {
    let mode_mgr = ModeManager::new(InputMode::Hybrid);
    let effective = mode_mgr.effective_mode(FieldClass::Sensitive, InputMode::Hybrid);
    assert_eq!(
        effective,
        InputMode::Privacy,
        "NF-032: Sensitive field in Hybrid mode must resolve to Privacy"
    );

    let ngram = Arc::new(LocalNgramInferencer::new_in_memory());
    let workers_ai = Arc::new(WorkersAIInferencer::new(
        "test-account".to_string(),
        "test-token".to_string(),
        "test-model".to_string(),
    ));
    let dispatcher = Dispatcher::new(ngram, Some(workers_ai), None);

    let selected = dispatcher.select_inferencer(effective, 50);
    assert_ne!(
        selected.name(),
        "workers-ai",
        "NF-032: Sensitive field must never route to WorkersAI"
    );
    assert_eq!(
        selected.name(),
        "local-ngram",
        "NF-032: Sensitive field falls back to local-ngram (Privacy, no LocalLlm configured)"
    );
}

/// NF-032 boundary: NonSensitive field + Hybrid → effective_mode = Performance → WorkersAI OK.
#[tokio::test]
async fn nf032_non_sensitive_field_calls_workers_ai() {
    let mode_mgr = ModeManager::new(InputMode::Hybrid);
    let effective = mode_mgr.effective_mode(FieldClass::NonSensitive, InputMode::Hybrid);
    assert_eq!(
        effective,
        InputMode::Performance,
        "NonSensitive field in Hybrid mode must resolve to Performance"
    );

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
        "workers-ai",
        "NF-032: NonSensitive field in Hybrid mode should use WorkersAI"
    );
}
