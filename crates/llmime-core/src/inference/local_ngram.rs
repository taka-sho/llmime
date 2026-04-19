use async_trait::async_trait;

use crate::inference::{
    capabilities::InferencerCapabilities,
    error::InferenceError,
    inferencer::{CandidateWithScore, Inferencer},
};

pub struct LocalNgramInferencer {
    model_path: std::path::PathBuf,
}

impl LocalNgramInferencer {
    pub fn new(model_path: impl Into<std::path::PathBuf>) -> Self {
        Self {
            model_path: model_path.into(),
        }
    }

    pub fn new_in_memory() -> Self {
        Self {
            model_path: std::path::PathBuf::new(),
        }
    }

    fn score_surface(surface: &str) -> f32 {
        -(surface.chars().count() as f32) * 0.1
    }
}

#[async_trait]
impl Inferencer for LocalNgramInferencer {
    fn name(&self) -> &'static str {
        "local-ngram"
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
        mut candidates: Vec<CandidateWithScore>,
        _left_context: Option<&str>,
    ) -> Result<Vec<CandidateWithScore>, InferenceError> {
        let model_empty = self.model_path.as_os_str().is_empty();
        let model_exists = !model_empty && self.model_path.exists();

        if model_empty {
            return Ok(candidates);
        }

        if !model_exists {
            return Err(InferenceError::Unavailable(
                self.model_path.display().to_string(),
            ));
        }

        for c in &mut candidates {
            c.score += Self::score_surface(&c.surface);
        }
        candidates.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(candidates)
    }

    async fn warmup(&self) -> Result<(), InferenceError> {
        if self.model_path.as_os_str().is_empty() {
            return Err(InferenceError::Unavailable(
                "model_path not set".to_string(),
            ));
        }
        if !self.model_path.exists() {
            return Err(InferenceError::Unavailable(
                self.model_path.display().to_string(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::inference::inferencer::CandidateSource;

    fn make_candidates(surfaces: &[&str]) -> Vec<CandidateWithScore> {
        surfaces
            .iter()
            .map(|s| CandidateWithScore {
                surface: s.to_string(),
                score: 0.0,
                source: CandidateSource::Ngram,
            })
            .collect()
    }

    #[tokio::test]
    async fn test_new_in_memory_trait_works() {
        let inf = LocalNgramInferencer::new_in_memory();
        let candidates = make_candidates(&["テスト"]);
        let result = inf.rerank("てすと", candidates, None).await.unwrap();
        assert_eq!(result.len(), 1);
    }

    #[tokio::test]
    async fn test_rerank_preserves_length() {
        let inf = LocalNgramInferencer::new_in_memory();
        let candidates = make_candidates(&["東京", "とうきょう", "トウキョウ"]);
        let result = inf.rerank("とうきょう", candidates, None).await.unwrap();
        assert_eq!(result.len(), 3);
    }

    #[tokio::test]
    async fn test_as_arc_dyn_inferencer() {
        let inf: Arc<dyn Inferencer> = Arc::new(LocalNgramInferencer::new_in_memory());
        let candidates = make_candidates(&["変換"]);
        let result = inf.rerank("へんかん", candidates, None).await.unwrap();
        assert_eq!(result.len(), 1);
    }

    #[tokio::test]
    async fn test_empty_candidates_returns_empty() {
        let inf = LocalNgramInferencer::new_in_memory();
        let result = inf.rerank("てすと", vec![], None).await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_nonexistent_model_path_no_panic() {
        let inf = LocalNgramInferencer::new("/nonexistent/path/model.arpa");
        let candidates = make_candidates(&["テスト"]);
        let result = inf.rerank("てすと", candidates, None).await;
        assert!(matches!(result, Err(InferenceError::Unavailable(_))));
    }
}
