use std::time::Duration;

use crate::inference::inferencer::{CandidateWithScore, DynInferencer};

pub struct FallbackChain {
    primary: DynInferencer,
    fallbacks: Vec<DynInferencer>,
    timeout: Duration,
}

impl FallbackChain {
    pub fn new(primary: DynInferencer, fallbacks: Vec<DynInferencer>, timeout: Duration) -> Self {
        Self {
            primary,
            fallbacks,
            timeout,
        }
    }

    pub fn primary_name(&self) -> &'static str {
        self.primary.name()
    }

    pub fn fallback_count(&self) -> usize {
        self.fallbacks.len()
    }

    pub async fn rerank(
        &self,
        reading: &str,
        candidates: Vec<CandidateWithScore>,
        left_context: Option<&str>,
    ) -> Vec<CandidateWithScore> {
        if let Ok(Ok(v)) = tokio::time::timeout(
            self.timeout,
            self.primary
                .rerank(reading, candidates.clone(), left_context),
        )
        .await
        {
            return v;
        }

        for fb in &self.fallbacks {
            if let Ok(Ok(v)) = tokio::time::timeout(
                self.timeout,
                fb.rerank(reading, candidates.clone(), left_context),
            )
            .await
            {
                return v;
            }
        }

        candidates
    }

    pub async fn rerank_with_right_context(
        &self,
        reading: &str,
        candidates: Vec<CandidateWithScore>,
        left_context: Option<&str>,
        right_context: Option<&str>,
    ) -> Vec<CandidateWithScore> {
        if let Ok(Ok(v)) = tokio::time::timeout(
            self.timeout,
            self.primary.rerank_with_right_context(
                reading,
                candidates.clone(),
                left_context,
                right_context,
            ),
        )
        .await
        {
            return v;
        }

        for fb in &self.fallbacks {
            if let Ok(Ok(v)) = tokio::time::timeout(
                self.timeout,
                fb.rerank_with_right_context(
                    reading,
                    candidates.clone(),
                    left_context,
                    right_context,
                ),
            )
            .await
            {
                return v;
            }
        }

        candidates
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;

    use super::*;
    use crate::inference::capabilities::InferencerCapabilities;
    use crate::inference::inferencer::{
        AlwaysSucceedInferencer, AlwaysTimeoutInferencer, Inferencer,
    };
    use crate::inference::InferenceError;

    fn make_candidates() -> Vec<CandidateWithScore> {
        vec![CandidateWithScore {
            surface: "東京".to_string(),
            score: 1.0,
            source: crate::inference::inferencer::CandidateSource::Ngram,
        }]
    }

    #[tokio::test]
    async fn test_primary_success() {
        let chain = FallbackChain::new(
            Arc::new(AlwaysSucceedInferencer),
            vec![],
            Duration::from_millis(1500),
        );
        let result = chain.rerank("とうきょう", make_candidates(), None).await;
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].surface, "東京");
    }

    #[tokio::test]
    async fn test_primary_timeout_fallback_succeeds() {
        let chain = FallbackChain::new(
            Arc::new(AlwaysTimeoutInferencer),
            vec![Arc::new(AlwaysSucceedInferencer)],
            Duration::from_millis(1500),
        );
        let result = chain.rerank("とうきょう", make_candidates(), None).await;
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].surface, "東京");
    }

    #[tokio::test]
    async fn test_all_fallbacks_fail_returns_input() {
        let chain = FallbackChain::new(
            Arc::new(AlwaysTimeoutInferencer),
            vec![Arc::new(AlwaysTimeoutInferencer)],
            Duration::from_millis(1500),
        );
        let candidates = make_candidates();
        let result = chain.rerank("とうきょう", candidates.clone(), None).await;
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].surface, candidates[0].surface);
    }

    #[tokio::test]
    async fn test_empty_fallbacks_primary_timeout_returns_input() {
        let chain = FallbackChain::new(
            Arc::new(AlwaysTimeoutInferencer),
            vec![],
            Duration::from_millis(1500),
        );
        let candidates = make_candidates();
        let result = chain.rerank("とうきょう", candidates.clone(), None).await;
        assert_eq!(result.len(), 1);
    }

    struct RightContextInferencer;

    #[async_trait]
    impl Inferencer for RightContextInferencer {
        fn name(&self) -> &'static str {
            "right-context"
        }

        fn capabilities(&self) -> InferencerCapabilities {
            InferencerCapabilities {
                supports_rerank: true,
                supports_right_context: true,
            }
        }

        async fn rerank(
            &self,
            _reading: &str,
            _candidates: Vec<CandidateWithScore>,
            _left_context: Option<&str>,
        ) -> Result<Vec<CandidateWithScore>, InferenceError> {
            Err(InferenceError::Unavailable(
                "rerank path should not be used".into(),
            ))
        }

        async fn rerank_with_right_context(
            &self,
            _reading: &str,
            _candidates: Vec<CandidateWithScore>,
            left_context: Option<&str>,
            right_context: Option<&str>,
        ) -> Result<Vec<CandidateWithScore>, InferenceError> {
            Ok(vec![CandidateWithScore {
                surface: format!(
                    "{}|{}",
                    left_context.unwrap_or_default(),
                    right_context.unwrap_or_default()
                ),
                score: 9.9,
                source: crate::inference::inferencer::CandidateSource::WorkersAI,
            }])
        }
    }

    #[tokio::test]
    async fn test_rerank_with_right_context_primary_success() {
        let chain = FallbackChain::new(
            Arc::new(RightContextInferencer),
            vec![],
            Duration::from_millis(1500),
        );
        let result = chain
            .rerank_with_right_context("てんき", make_candidates(), Some("明日の"), Some("予報"))
            .await;
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].surface, "明日の|予報");
    }

    #[tokio::test]
    async fn test_rerank_with_right_context_falls_back() {
        let chain = FallbackChain::new(
            Arc::new(AlwaysTimeoutInferencer),
            vec![Arc::new(RightContextInferencer)],
            Duration::from_millis(1500),
        );
        let result = chain
            .rerank_with_right_context("てんき", make_candidates(), Some("明日の"), Some("予報"))
            .await;
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].surface, "明日の|予報");
    }
}
