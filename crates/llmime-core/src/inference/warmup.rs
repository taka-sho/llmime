use std::collections::HashMap;
use std::time::Duration;

use crate::inference::{error::InferenceError, inferencer::DynInferencer};

pub enum WarmupStatus {
    Ready,
    Failed(InferenceError),
    TimedOut,
}

pub struct WarmupOrchestrator {
    inferencers: Vec<DynInferencer>,
}

impl WarmupOrchestrator {
    pub fn new(inferencers: Vec<DynInferencer>) -> Self {
        Self { inferencers }
    }

    pub async fn run_parallel(&self, timeout: Duration) -> HashMap<String, WarmupStatus> {
        let futures: Vec<_> = self
            .inferencers
            .iter()
            .map(|inf| {
                let name = inf.name().to_string();
                let inf = inf.clone();
                async move {
                    let result = tokio::time::timeout(timeout, inf.warmup()).await;
                    let status = match result {
                        Ok(Ok(())) => WarmupStatus::Ready,
                        Ok(Err(e)) => WarmupStatus::Failed(e),
                        Err(_) => WarmupStatus::TimedOut,
                    };
                    (name, status)
                }
            })
            .collect();

        let results = futures::future::join_all(futures).await;
        results.into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference::inferencer::{AlwaysSucceedInferencer, CandidateWithScore, Inferencer};
    use async_trait::async_trait;
    use std::sync::Arc;

    macro_rules! ready_inferencer {
        ($name:literal) => {{
            struct R;
            #[async_trait]
            impl Inferencer for R {
                fn name(&self) -> &'static str {
                    $name
                }
                fn capabilities(&self) -> crate::inference::capabilities::InferencerCapabilities {
                    crate::inference::capabilities::InferencerCapabilities {
                        supports_rerank: false,
                        supports_right_context: false,
                    }
                }
                async fn rerank(
                    &self,
                    _: &str,
                    c: Vec<CandidateWithScore>,
                    _: Option<&str>,
                ) -> Result<Vec<CandidateWithScore>, InferenceError> {
                    Ok(c)
                }
            }
            Arc::new(R) as DynInferencer
        }};
    }

    struct AlwaysFailWarmup;

    #[async_trait]
    impl Inferencer for AlwaysFailWarmup {
        fn name(&self) -> &'static str {
            "always-fail-warmup"
        }
        fn capabilities(&self) -> crate::inference::capabilities::InferencerCapabilities {
            crate::inference::capabilities::InferencerCapabilities {
                supports_rerank: false,
                supports_right_context: false,
            }
        }
        async fn rerank(
            &self,
            _: &str,
            c: Vec<CandidateWithScore>,
            _: Option<&str>,
        ) -> Result<Vec<CandidateWithScore>, InferenceError> {
            Ok(c)
        }
        async fn warmup(&self) -> Result<(), InferenceError> {
            Err(InferenceError::Unavailable("test failure".to_string()))
        }
    }

    struct SlowWarmup;

    #[async_trait]
    impl Inferencer for SlowWarmup {
        fn name(&self) -> &'static str {
            "slow-warmup"
        }
        fn capabilities(&self) -> crate::inference::capabilities::InferencerCapabilities {
            crate::inference::capabilities::InferencerCapabilities {
                supports_rerank: false,
                supports_right_context: false,
            }
        }
        async fn rerank(
            &self,
            _: &str,
            c: Vec<CandidateWithScore>,
            _: Option<&str>,
        ) -> Result<Vec<CandidateWithScore>, InferenceError> {
            Ok(c)
        }
        async fn warmup(&self) -> Result<(), InferenceError> {
            tokio::time::sleep(Duration::from_secs(10)).await;
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_warmup_all_ready() {
        let inferencers: Vec<DynInferencer> =
            vec![ready_inferencer!("ready-a"), ready_inferencer!("ready-b")];
        let orch = WarmupOrchestrator::new(inferencers);
        let results = orch.run_parallel(Duration::from_secs(3)).await;
        assert_eq!(results.len(), 2);
        for status in results.values() {
            assert!(matches!(status, WarmupStatus::Ready));
        }
    }

    #[tokio::test]
    async fn test_warmup_one_fails_others_continue() {
        let inferencers: Vec<DynInferencer> = vec![
            ready_inferencer!("ready-a"),
            Arc::new(AlwaysFailWarmup),
            ready_inferencer!("ready-b"),
        ];
        let orch = WarmupOrchestrator::new(inferencers);
        let results = orch.run_parallel(Duration::from_secs(3)).await;
        assert_eq!(results.len(), 3);
        let ready_count = results
            .values()
            .filter(|s| matches!(s, WarmupStatus::Ready))
            .count();
        let failed_count = results
            .values()
            .filter(|s| matches!(s, WarmupStatus::Failed(_)))
            .count();
        assert_eq!(ready_count, 2);
        assert_eq!(failed_count, 1);
    }

    #[tokio::test]
    async fn test_warmup_timeout() {
        let inferencers: Vec<DynInferencer> = vec![
            ready_inferencer!("ready-a"),
            Arc::new(SlowWarmup),
            ready_inferencer!("ready-b"),
        ];
        let orch = WarmupOrchestrator::new(inferencers);
        let results = orch.run_parallel(Duration::from_millis(100)).await;
        assert_eq!(results.len(), 3);
        assert!(matches!(
            results.get("slow-warmup").unwrap(),
            WarmupStatus::TimedOut
        ));
    }

    #[tokio::test]
    async fn test_warmup_three_parallel_with_timeout() {
        let inferencers: Vec<DynInferencer> = vec![
            ready_inferencer!("ready-a"),
            Arc::new(AlwaysFailWarmup),
            Arc::new(SlowWarmup),
        ];
        let orch = WarmupOrchestrator::new(inferencers);
        let results = orch.run_parallel(Duration::from_secs(3)).await;
        assert_eq!(results.len(), 3);
        let ready = results
            .values()
            .filter(|s| matches!(s, WarmupStatus::Ready))
            .count();
        let failed = results
            .values()
            .filter(|s| matches!(s, WarmupStatus::Failed(_)))
            .count();
        let timed_out = results
            .values()
            .filter(|s| matches!(s, WarmupStatus::TimedOut))
            .count();
        assert_eq!(ready, 1);
        assert_eq!(failed, 1);
        assert_eq!(timed_out, 1);
    }

    #[tokio::test]
    async fn test_warmup_empty() {
        let orch = WarmupOrchestrator::new(vec![]);
        let results = orch.run_parallel(Duration::from_secs(3)).await;
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_default_warmup_succeeds() {
        let inf = AlwaysSucceedInferencer;
        assert!(inf.warmup().await.is_ok());
    }
}
