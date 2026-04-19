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

    pub async fn rerank(
        &self,
        reading: &str,
        candidates: Vec<CandidateWithScore>,
        left_context: Option<&str>,
    ) -> Vec<CandidateWithScore> {
        match tokio::time::timeout(
            self.timeout,
            self.primary
                .rerank(reading, candidates.clone(), left_context),
        )
        .await
        {
            Ok(Ok(v)) => return v,
            _ => {}
        }

        for fb in &self.fallbacks {
            match tokio::time::timeout(
                self.timeout,
                fb.rerank(reading, candidates.clone(), left_context),
            )
            .await
            {
                Ok(Ok(v)) => return v,
                _ => {}
            }
        }

        candidates
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::inference::inferencer::{AlwaysSucceedInferencer, AlwaysTimeoutInferencer};

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
}
