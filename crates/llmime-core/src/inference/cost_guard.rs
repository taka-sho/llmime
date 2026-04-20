use std::sync::Arc;

use crate::config::WorkersAIConfig;
use crate::inference::cost_monitor::CostMonitor;
use crate::inference::error::{CostCapKind, InferenceError};

pub struct CostGuard {
    monitor: Arc<CostMonitor>,
    config: WorkersAIConfig,
}

impl CostGuard {
    pub fn new(monitor: Arc<CostMonitor>, config: WorkersAIConfig) -> Arc<Self> {
        Arc::new(Self { monitor, config })
    }

    pub fn check(&self) -> Result<(), InferenceError> {
        if self.monitor.cost_today("") > self.config.cost_limit_day {
            return Err(InferenceError::CostCapExceeded(CostCapKind::Daily));
        }
        if self.monitor.cost_hour("") > self.config.cost_limit_hour {
            return Err(InferenceError::CostCapExceeded(CostCapKind::Hourly));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config(cost_limit_hour: f64, cost_limit_day: f64) -> WorkersAIConfig {
        WorkersAIConfig {
            account_id: "acct".to_string(),
            api_token: "tok".to_string(),
            model_id: "@cf/qwen/qwen3-30b-a3b-fp8".to_string(),
            timeout_ms: 1500,
            retry_count: 2,
            cost_limit_hour,
            cost_limit_day,
        }
    }

    #[test]
    fn under_limit() {
        let monitor = CostMonitor::new();
        let guard = CostGuard::new(Arc::clone(&monitor), make_config(0.10, 1.00));
        assert!(guard.check().is_ok());
    }

    #[test]
    fn daily_exceeded() {
        let monitor = CostMonitor::new();
        // Record enough requests to exceed daily limit ($1.00)
        // Each qwen3 request costs ~$0.0004202, so ~2381 requests exceed $1.00
        // Use a tiny limit instead
        let guard = CostGuard::new(Arc::clone(&monitor), make_config(0.10, 0.0));
        monitor.record("@cf/qwen/qwen3-30b-a3b-fp8");
        let result = guard.check();
        assert!(matches!(
            result,
            Err(InferenceError::CostCapExceeded(CostCapKind::Daily))
        ));
    }

    #[test]
    fn hourly_exceeded() {
        let monitor = CostMonitor::new();
        let guard = CostGuard::new(Arc::clone(&monitor), make_config(0.0, 1.00));
        monitor.record("@cf/qwen/qwen3-30b-a3b-fp8");
        let result = guard.check();
        assert!(matches!(
            result,
            Err(InferenceError::CostCapExceeded(CostCapKind::Hourly))
        ));
    }

    #[test]
    fn no_retry_on_cap() {
        // CostCapExceeded must be Fatal (not retryable) — verified structurally:
        // workers_ai.rs returns Err early before retry_wrapper on cost_guard failure.
        let monitor = CostMonitor::new();
        let guard = CostGuard::new(Arc::clone(&monitor), make_config(0.0, 1.00));
        monitor.record("@cf/qwen/qwen3-30b-a3b-fp8");
        // check() returns Err immediately — not wrapped in RetryDecision::Retryable
        let err = guard.check().unwrap_err();
        assert!(matches!(
            err,
            InferenceError::CostCapExceeded(CostCapKind::Hourly)
        ));
    }

    #[test]
    fn fallback_on_cap() {
        // When CostGuard blocks, FallbackChain falls back to local backend.
        // This test verifies CostCapExceeded is an Err variant that callers can match on.
        let monitor = CostMonitor::new();
        let guard = CostGuard::new(Arc::clone(&monitor), make_config(0.0, 0.0));
        monitor.record("@cf/qwen/qwen3-30b-a3b-fp8");
        let result = guard.check();
        assert!(result.is_err());
        // FallbackChain sees Err(_) from primary.rerank() and falls back to local N-gram.
    }
}
