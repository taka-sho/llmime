use std::sync::Arc;

use crate::consent::ConsentManager;
use crate::inference::InferenceError;
use crate::pipeline::AsyncPipeline;

pub struct LiveConversionHandler {
    pipeline: Arc<AsyncPipeline>,
    consent_manager: Arc<ConsentManager>,
}

impl LiveConversionHandler {
    pub fn new(pipeline: Arc<AsyncPipeline>, consent: Arc<ConsentManager>) -> Self {
        Self {
            pipeline,
            consent_manager: consent,
        }
    }

    pub async fn on_input_change(&self, input: &str) -> Result<Vec<String>, InferenceError> {
        if !self.consent_manager.is_consented() {
            return Err(InferenceError::ConsentRequired);
        }
        let candidates = self.pipeline.submit(input.to_string()).await?;
        Ok(candidates.into_iter().map(|c| c.surface).collect())
    }

    pub fn cancel_pending(&self) {
        self.pipeline.cancel();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use async_trait::async_trait;
    use tempfile::tempdir;

    use super::*;
    use crate::consent::ConsentManager;
    use crate::inference::capabilities::InferencerCapabilities;
    use crate::inference::inferencer::{CandidateSource, CandidateWithScore, Inferencer};
    use crate::inference::InferenceError;
    use crate::pipeline::{AsyncPipeline, PipelineConfig};

    struct EchoInferencer;

    #[async_trait]
    impl Inferencer for EchoInferencer {
        fn name(&self) -> &'static str {
            "echo"
        }

        fn capabilities(&self) -> InferencerCapabilities {
            InferencerCapabilities {
                supports_rerank: true,
                supports_right_context: false,
            }
        }

        async fn rerank(
            &self,
            reading: &str,
            _candidates: Vec<CandidateWithScore>,
            _left_context: Option<&str>,
        ) -> Result<Vec<CandidateWithScore>, InferenceError> {
            Ok(vec![CandidateWithScore {
                surface: reading.to_string(),
                score: 1.0,
                source: CandidateSource::Ngram,
            }])
        }
    }

    fn make_handler(
        debounce_ms: u64,
        consented: bool,
    ) -> (LiveConversionHandler, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let cm = Arc::new(ConsentManager::with_path(dir.path().join("consent.json")));
        if consented {
            cm.record_consent().unwrap();
        }
        let pipeline = Arc::new(AsyncPipeline::new(
            Arc::new(EchoInferencer),
            PipelineConfig {
                debounce_ms,
                max_concurrent: 3,
                buffer_size: 10,
            },
        ));
        (LiveConversionHandler::new(pipeline, cm), dir)
    }

    #[tokio::test]
    async fn on_input_change() {
        let (handler, _dir) = make_handler(5, true);
        let result = handler.on_input_change("てんき").await.unwrap();
        assert_eq!(result, vec!["てんき"]);
    }

    #[tokio::test]
    async fn consent_required() {
        let (handler, _dir) = make_handler(5, false);
        let err = handler.on_input_change("てんき").await.unwrap_err();
        assert!(matches!(err, InferenceError::ConsentRequired));
    }

    #[tokio::test]
    async fn cancel_pending() {
        let dir = tempdir().unwrap();
        let cm = Arc::new(ConsentManager::with_path(dir.path().join("consent.json")));
        cm.record_consent().unwrap();
        let pipeline = Arc::new(AsyncPipeline::new(
            Arc::new(EchoInferencer),
            PipelineConfig {
                debounce_ms: 80,
                max_concurrent: 3,
                buffer_size: 10,
            },
        ));
        let handler = Arc::new(LiveConversionHandler::new(pipeline, cm));

        let h2 = handler.clone();
        let task = tokio::spawn(async move { h2.on_input_change("input").await });

        tokio::time::sleep(Duration::from_millis(20)).await;
        handler.cancel_pending();

        let result = task.await.unwrap();
        assert!(
            matches!(result, Err(InferenceError::Cancelled)),
            "expected Cancelled, got: {:?}",
            result
        );
    }

    #[tokio::test]
    async fn debounce_integration() {
        let dir = tempdir().unwrap();
        let cm = Arc::new(ConsentManager::with_path(dir.path().join("consent.json")));
        cm.record_consent().unwrap();
        let pipeline = Arc::new(AsyncPipeline::new(
            Arc::new(EchoInferencer),
            PipelineConfig {
                debounce_ms: 50,
                max_concurrent: 3,
                buffer_size: 10,
            },
        ));
        let handler = Arc::new(LiveConversionHandler::new(pipeline, cm));

        let h2 = handler.clone();
        let first = tokio::spawn(async move { h2.on_input_change("first").await });

        tokio::time::sleep(Duration::from_millis(10)).await;

        let h3 = handler.clone();
        let second = tokio::spawn(async move { h3.on_input_change("second").await });

        let first_result = first.await.unwrap();
        let second_result = second.await.unwrap();

        assert!(
            matches!(first_result, Err(InferenceError::Unavailable(_))),
            "first call should be debounced/cancelled, got: {:?}",
            first_result
        );
        assert_eq!(
            second_result.unwrap(),
            vec!["second"],
            "second call should complete successfully"
        );
    }
}
