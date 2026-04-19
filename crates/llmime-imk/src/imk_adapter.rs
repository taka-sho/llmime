use std::sync::Arc;

use llmime_core::LiveConversionHandler;

pub struct ImkLiveAdapter {
    handler: Arc<LiveConversionHandler>,
    runtime: tokio::runtime::Handle,
}

impl ImkLiveAdapter {
    pub fn new(handler: Arc<LiveConversionHandler>) -> Self {
        Self {
            handler,
            runtime: tokio::runtime::Handle::current(),
        }
    }

    /// Synchronous wrapper for IMKit callbacks; returns candidate strings or empty on error.
    pub fn handle_input_change(&self, input: &str) -> Vec<String> {
        let fut = self.handler.on_input_change(input);
        match tokio::task::block_in_place(|| self.runtime.block_on(fut)) {
            Ok(candidates) => candidates,
            Err(e) => {
                log::debug!("ImkLiveAdapter: on_input_change error: {e}");
                vec![]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use llmime_core::consent::ConsentManager;
    use llmime_core::inference::capabilities::InferencerCapabilities;
    use llmime_core::inference::inferencer::{CandidateSource, CandidateWithScore, Inferencer};
    use llmime_core::inference::InferenceError;
    use llmime_core::pipeline::{AsyncPipeline, PipelineConfig};
    use llmime_core::LiveConversionHandler;
    use tempfile::tempdir;

    use super::*;

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

    fn make_adapter(consented: bool) -> (ImkLiveAdapter, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let cm = Arc::new(ConsentManager::with_path(dir.path().join("consent.json")));
        if consented {
            cm.record_consent().unwrap();
        }
        let pipeline = Arc::new(AsyncPipeline::new(
            Arc::new(EchoInferencer),
            PipelineConfig {
                debounce_ms: 0,
                max_concurrent: 3,
                buffer_size: 10,
            },
        ));
        let handler = Arc::new(LiveConversionHandler::new(pipeline, cm));
        (ImkLiveAdapter::new(handler), dir)
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn handle_input_change() {
        let (adapter, _dir) = make_adapter(true);
        let result = tokio::task::spawn_blocking(move || adapter.handle_input_change("てんき"))
            .await
            .unwrap();
        assert_eq!(result, vec!["てんき"]);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn consent_propagation() {
        let (adapter, _dir) = make_adapter(false);
        let result = tokio::task::spawn_blocking(move || adapter.handle_input_change("てんき"))
            .await
            .unwrap();
        assert!(
            result.is_empty(),
            "expected empty result when consent not given"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn error_handling() {
        struct FailInferencer;

        #[async_trait]
        impl Inferencer for FailInferencer {
            fn name(&self) -> &'static str {
                "fail"
            }
            fn capabilities(&self) -> InferencerCapabilities {
                InferencerCapabilities {
                    supports_rerank: true,
                    supports_right_context: false,
                }
            }
            async fn rerank(
                &self,
                _: &str,
                _: Vec<CandidateWithScore>,
                _: Option<&str>,
            ) -> Result<Vec<CandidateWithScore>, InferenceError> {
                Err(InferenceError::Unavailable("forced failure".into()))
            }
        }

        let dir = tempdir().unwrap();
        let cm = Arc::new(ConsentManager::with_path(dir.path().join("consent.json")));
        cm.record_consent().unwrap();
        let pipeline = Arc::new(AsyncPipeline::new(
            Arc::new(FailInferencer),
            PipelineConfig {
                debounce_ms: 0,
                max_concurrent: 3,
                buffer_size: 10,
            },
        ));
        let handler = Arc::new(LiveConversionHandler::new(pipeline, cm));
        let adapter = ImkLiveAdapter::new(handler);
        let result = tokio::task::spawn_blocking(move || adapter.handle_input_change("input"))
            .await
            .unwrap();
        assert!(
            result.is_empty(),
            "expected empty result on inferencer error"
        );
    }
}
