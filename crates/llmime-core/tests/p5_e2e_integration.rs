//! P5 E2E integration tests: full scenario coverage for Hybrid mode + Sensitive protection.
//!
//! Verifies end-to-end behavior of ModeManager + OverrideManager + Dispatcher across 6 scenarios.

use llmime_core::{
    field::FieldClass, Dispatcher, InputMode, LocalNgramInferencer, ModeManager, OverrideManager,
    WorkersAIInferencer,
};
use std::sync::Arc;
use std::time::Duration;

fn make_dispatcher() -> Dispatcher {
    let ngram = Arc::new(LocalNgramInferencer::new_in_memory());
    let workers_ai = Arc::new(WorkersAIInferencer::new(
        "test-account".to_string(),
        "test-token".to_string(),
        "test-model".to_string(),
    ));
    Dispatcher::new(ngram, Some(workers_ai), None)
}

/// Scenario 1: Hybrid + Sensitive → Privacy強制 (WorkersAI呼出しなし, LocalNgramのみ)
#[test]
fn p5_scenario1_hybrid_sensitive_forces_privacy() {
    let mgr = ModeManager::new(InputMode::Hybrid);
    let effective = mgr.effective_mode(FieldClass::Sensitive, InputMode::Hybrid);
    assert_eq!(
        effective,
        InputMode::Privacy,
        "Scenario 1: Sensitive must resolve to Privacy"
    );

    let dispatcher = make_dispatcher();
    let selected = dispatcher.select_inferencer(effective, 50);
    assert_ne!(
        selected.name(),
        "workers-ai",
        "Scenario 1: WorkersAI must NOT be called"
    );
    assert_eq!(
        selected.name(),
        "local-ngram",
        "Scenario 1: LocalNgram must be used"
    );
}

/// Scenario 2: Hybrid + NonSensitive → Performance (WorkersAI呼出しあり)
#[test]
fn p5_scenario2_hybrid_non_sensitive_uses_workers_ai() {
    let mgr = ModeManager::new(InputMode::Hybrid);
    let effective = mgr.effective_mode(FieldClass::NonSensitive, InputMode::Hybrid);
    assert_eq!(
        effective,
        InputMode::Performance,
        "Scenario 2: NonSensitive must resolve to Performance"
    );

    let dispatcher = make_dispatcher();
    let selected = dispatcher.select_inferencer(effective, 50);
    assert_eq!(
        selected.name(),
        "workers-ai",
        "Scenario 2: WorkersAI must be called for NonSensitive"
    );
}

/// Scenario 3: Unknown field → Privacy フェイルセーフ (NF-032, WorkersAI呼出しなし)
#[test]
fn p5_scenario3_hybrid_unknown_privacy_failsafe() {
    let mgr = ModeManager::new(InputMode::Hybrid);
    let effective = mgr.effective_mode(FieldClass::Unknown, InputMode::Hybrid);
    assert_eq!(
        effective,
        InputMode::Privacy,
        "Scenario 3 (NF-032): Unknown must resolve to Privacy"
    );

    let dispatcher = make_dispatcher();
    let selected = dispatcher.select_inferencer(effective, 50);
    assert_ne!(
        selected.name(),
        "workers-ai",
        "Scenario 3 (NF-032): WorkersAI must NOT be called for Unknown"
    );
    assert_eq!(
        selected.name(),
        "local-ngram",
        "Scenario 3 (NF-032): LocalNgram must be used"
    );
}

/// Scenario 4: OverrideManager.set_override(Privacy) + NonSensitive → Privacy強制 (Hybrid無視)
#[test]
fn p5_scenario4_override_privacy_forces_privacy_on_non_sensitive() {
    let mgr = ModeManager::new(InputMode::Hybrid);
    let mut override_mgr = OverrideManager::new();
    override_mgr.set_override(InputMode::Privacy, Duration::from_secs(60));

    let effective = mgr.effective_mode_with_override_secure(
        FieldClass::NonSensitive,
        InputMode::Hybrid,
        &override_mgr,
    );
    assert_eq!(
        effective,
        InputMode::Privacy,
        "Scenario 4: Override(Privacy) on NonSensitive must yield Privacy"
    );

    let dispatcher = make_dispatcher();
    let selected = dispatcher.select_inferencer(effective, 50);
    assert_ne!(
        selected.name(),
        "workers-ai",
        "Scenario 4: WorkersAI must NOT be called when overridden to Privacy"
    );
}

/// Scenario 5: OverrideManager.clear_override() 後 NonSensitive → Performance (Hybrid通常動作復帰)
#[test]
fn p5_scenario5_clear_override_restores_hybrid_performance() {
    let mgr = ModeManager::new(InputMode::Hybrid);
    let mut override_mgr = OverrideManager::new();
    override_mgr.set_override(InputMode::Privacy, Duration::from_secs(60));
    override_mgr.clear_override();

    let effective = mgr.effective_mode_with_override_secure(
        FieldClass::NonSensitive,
        InputMode::Hybrid,
        &override_mgr,
    );
    assert_eq!(
        effective,
        InputMode::Performance,
        "Scenario 5: After clear_override, NonSensitive must return to Performance"
    );

    let dispatcher = make_dispatcher();
    let selected = dispatcher.select_inferencer(effective, 50);
    assert_eq!(
        selected.name(),
        "workers-ai",
        "Scenario 5: WorkersAI must be used after override cleared"
    );
}

/// Scenario 6: override(Performance) + Sensitive → Privacy優先 (override無視 — セキュリティ優先)
/// NF-032 + Sensitive保護はoverride不可。
#[test]
fn p5_scenario6_sensitive_wins_over_performance_override() {
    let mgr = ModeManager::new(InputMode::Hybrid);
    let mut override_mgr = OverrideManager::new();
    override_mgr.set_override(InputMode::Performance, Duration::from_secs(60));

    let effective = mgr.effective_mode_with_override_secure(
        FieldClass::Sensitive,
        InputMode::Hybrid,
        &override_mgr,
    );
    assert_eq!(
        effective,
        InputMode::Privacy,
        "Scenario 6: Sensitive field must ALWAYS resolve to Privacy even with Performance override"
    );

    let dispatcher = make_dispatcher();
    let selected = dispatcher.select_inferencer(effective, 50);
    assert_ne!(
        selected.name(),
        "workers-ai",
        "Scenario 6: WorkersAI must NOT be called for Sensitive field regardless of override"
    );
    assert_eq!(
        selected.name(),
        "local-ngram",
        "Scenario 6: LocalNgram must be used for Sensitive"
    );
}
