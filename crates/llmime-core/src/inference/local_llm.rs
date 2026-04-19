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
        if self.model_path.is_none() {
            return Err(InferenceError::Unavailable("no model path".to_string()));
        }
        Ok(candidates)
    }
}
