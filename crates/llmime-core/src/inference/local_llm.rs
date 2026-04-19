use std::path::PathBuf;

use async_trait::async_trait;

use crate::inference::{
    capabilities::InferencerCapabilities,
    error::InferenceError,
    inferencer::{CandidateWithScore, Inferencer},
};

pub struct LocalLlmInferencer {
    model_path: Option<PathBuf>,
}

impl LocalLlmInferencer {
    pub fn new(model_path: Option<PathBuf>) -> Self {
        Self { model_path }
    }
}

#[async_trait]
impl Inferencer for LocalLlmInferencer {
    fn name(&self) -> &'static str {
        "local-llm"
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
        match &self.model_path {
            None => Err(InferenceError::Unavailable(
                "GGUF model not loaded".to_string(),
            )),
            Some(_path) => {
                // TODO: Phase 6 で llama-cpp-rs 本実装
                Ok(candidates)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference::inferencer::CandidateSource;

    #[tokio::test]
    async fn test_no_model_path_returns_unavailable() {
        let inf = LocalLlmInferencer::new(None);
        let result = inf.rerank("てすと", vec![], None).await;
        assert!(matches!(result, Err(InferenceError::Unavailable(_))));
    }

    #[tokio::test]
    async fn test_with_model_path_returns_candidates() {
        let inf = LocalLlmInferencer::new(Some(PathBuf::from("/fake/model.gguf")));
        let candidates = vec![CandidateWithScore {
            surface: "テスト".to_string(),
            score: 1.0,
            source: CandidateSource::LocalLlm,
        }];
        let result = inf.rerank("てすと", candidates, None).await.unwrap();
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_capabilities() {
        let inf = LocalLlmInferencer::new(None);
        let caps = inf.capabilities();
        assert!(caps.supports_rerank);
        assert!(!caps.supports_right_context);
    }
}
