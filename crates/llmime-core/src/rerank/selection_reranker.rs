use std::sync::Arc;

use crate::inference::{CandidateWithScore, FallbackChain};

use super::{SelectionRerankRequest, Token};

/// Executes selection-driven rerank requests with left/right contexts.
pub struct SelectionReranker {
    fallback_chain: Arc<FallbackChain>,
}

impl SelectionReranker {
    pub fn new(fallback_chain: Arc<FallbackChain>) -> Self {
        Self { fallback_chain }
    }

    pub async fn rerank(
        &self,
        request: &SelectionRerankRequest,
        recent_tokens: &[Token],
        candidates: Vec<CandidateWithScore>,
    ) -> Vec<CandidateWithScore> {
        let (left_context, right_context) =
            build_context_from_tokens(recent_tokens, request.start, request.end);
        self.fallback_chain
            .rerank_with_right_context(
                &request.selected_text,
                candidates,
                left_context.as_deref(),
                right_context.as_deref(),
            )
            .await
    }
}

pub fn build_context_from_tokens(
    tokens: &[Token],
    selection_start: usize,
    selection_end: usize,
) -> (Option<String>, Option<String>) {
    let left = tokens
        .iter()
        .filter(|token| token.end <= selection_start)
        .map(|token| token.surface.as_str())
        .collect::<String>();
    let right = tokens
        .iter()
        .filter(|token| token.start >= selection_end)
        .map(|token| token.surface.as_str())
        .collect::<String>();

    (
        (!left.is_empty()).then_some(left),
        (!right.is_empty()).then_some(right),
    )
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use async_trait::async_trait;

    use super::*;
    use crate::inference::capabilities::InferencerCapabilities;
    use crate::inference::inferencer::{CandidateSource, DynInferencer, Inferencer};
    use crate::inference::InferenceError;

    fn make_candidates() -> Vec<CandidateWithScore> {
        vec![CandidateWithScore {
            surface: "候補".to_string(),
            score: 1.0,
            source: CandidateSource::Ngram,
        }]
    }

    fn token(surface: &str, start: usize, end: usize) -> Token {
        Token {
            surface: surface.to_string(),
            reading: surface.to_string(),
            pos: "noun".to_string(),
            pos_detail: "general".to_string(),
            start,
            end,
            confidence: 1.0,
        }
    }

    struct RecordingInferencer {
        response_surface: String,
        fail: bool,
    }

    #[async_trait]
    impl Inferencer for RecordingInferencer {
        fn name(&self) -> &'static str {
            "recording"
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
            if self.fail {
                return Err(InferenceError::Unavailable("fail".into()));
            }
            Ok(vec![CandidateWithScore {
                surface: format!("fallback:{}", self.response_surface),
                score: 1.0,
                source: CandidateSource::Ngram,
            }])
        }

        async fn rerank_with_right_context(
            &self,
            _reading: &str,
            _candidates: Vec<CandidateWithScore>,
            left_context: Option<&str>,
            right_context: Option<&str>,
        ) -> Result<Vec<CandidateWithScore>, InferenceError> {
            if self.fail {
                return Err(InferenceError::Unavailable("fail".into()));
            }
            Ok(vec![CandidateWithScore {
                surface: format!(
                    "{}:{}:{}",
                    self.response_surface,
                    left_context.unwrap_or_default(),
                    right_context.unwrap_or_default()
                ),
                score: 2.0,
                source: CandidateSource::WorkersAI,
            }])
        }
    }

    fn make_chain(primary: DynInferencer, fallbacks: Vec<DynInferencer>) -> Arc<FallbackChain> {
        Arc::new(FallbackChain::new(
            primary,
            fallbacks,
            Duration::from_millis(100),
        ))
    }

    #[test]
    fn builds_left_and_right_context_from_token_boundaries() {
        let tokens = vec![
            token("明日", 0, 2),
            token("の", 2, 3),
            token("天気", 3, 5),
            token("予報", 5, 7),
        ];
        let (left, right) = build_context_from_tokens(&tokens, 3, 5);
        assert_eq!(left.as_deref(), Some("明日の"));
        assert_eq!(right.as_deref(), Some("予報"));
    }

    #[tokio::test]
    async fn selection_rerank_uses_right_context_path() {
        let chain = make_chain(
            Arc::new(RecordingInferencer {
                response_surface: "primary".to_string(),
                fail: false,
            }),
            vec![],
        );
        let reranker = SelectionReranker::new(chain);
        let request = SelectionRerankRequest {
            selected_text: "天気".to_string(),
            start: 3,
            end: 5,
            forced: false,
            timestamp: Instant::now(),
        };
        let tokens = vec![
            token("明日", 0, 2),
            token("の", 2, 3),
            token("天気", 3, 5),
            token("予報", 5, 7),
        ];

        let reranked = reranker.rerank(&request, &tokens, make_candidates()).await;
        assert_eq!(reranked.len(), 1);
        assert_eq!(reranked[0].surface, "primary:明日の:予報");
    }

    #[tokio::test]
    async fn selection_rerank_falls_back_when_primary_fails() {
        let chain = make_chain(
            Arc::new(RecordingInferencer {
                response_surface: "primary".to_string(),
                fail: true,
            }),
            vec![Arc::new(RecordingInferencer {
                response_surface: "fallback".to_string(),
                fail: false,
            })],
        );
        let reranker = SelectionReranker::new(chain);
        let request = SelectionRerankRequest {
            selected_text: "天気".to_string(),
            start: 2,
            end: 4,
            forced: true,
            timestamp: Instant::now(),
        };
        let tokens = vec![
            token("今日", 0, 2),
            token("天気", 2, 4),
            token("です", 4, 6),
        ];

        let reranked = reranker.rerank(&request, &tokens, make_candidates()).await;
        assert_eq!(reranked.len(), 1);
        assert_eq!(reranked[0].surface, "fallback:今日:です");
    }
}
