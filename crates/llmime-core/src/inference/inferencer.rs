use async_trait::async_trait;
use std::sync::Arc;

use crate::inference::{capabilities::InferencerCapabilities, error::InferenceError};

#[derive(Debug, Clone)]
pub struct CandidateWithScore {
    pub surface: String,
    pub score: f32,
    pub source: CandidateSource,
}

#[derive(Debug, Clone)]
pub enum CandidateSource {
    Ngram,
    WorkersAI,
    LocalLlm,
    UserDict,
}

#[async_trait]
pub trait Inferencer: Send + Sync + 'static {
    fn name(&self) -> &'static str;
    fn capabilities(&self) -> InferencerCapabilities;
    async fn rerank(
        &self,
        reading: &str,
        candidates: Vec<CandidateWithScore>,
        left_context: Option<&str>,
    ) -> Result<Vec<CandidateWithScore>, InferenceError>;
}

pub type DynInferencer = Arc<dyn Inferencer>;

pub struct AlwaysSucceedInferencer;

#[async_trait]
impl Inferencer for AlwaysSucceedInferencer {
    fn name(&self) -> &'static str {
        "always_succeed"
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
}

pub struct AlwaysTimeoutInferencer;

#[async_trait]
impl Inferencer for AlwaysTimeoutInferencer {
    fn name(&self) -> &'static str {
        "always_timeout"
    }

    fn capabilities(&self) -> InferencerCapabilities {
        InferencerCapabilities {
            supports_rerank: false,
            supports_right_context: false,
        }
    }

    async fn rerank(
        &self,
        _reading: &str,
        _candidates: Vec<CandidateWithScore>,
        _left_context: Option<&str>,
    ) -> Result<Vec<CandidateWithScore>, InferenceError> {
        Err(InferenceError::Timeout(std::time::Duration::from_secs(5)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_always_succeed_as_dyn() {
        let inf: DynInferencer = Arc::new(AlwaysSucceedInferencer);
        let candidates = vec![CandidateWithScore {
            surface: "テスト".to_string(),
            score: 1.0,
            source: CandidateSource::Ngram,
        }];
        let result = inf.rerank("てすと", candidates, None).await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].surface, "テスト");
    }

    #[tokio::test]
    async fn test_always_timeout_returns_error() {
        let inf: DynInferencer = Arc::new(AlwaysTimeoutInferencer);
        let result = inf.rerank("てすと", vec![], None).await;
        assert!(matches!(result, Err(InferenceError::Timeout(_))));
    }
}
