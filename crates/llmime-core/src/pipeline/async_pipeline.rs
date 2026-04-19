use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, OwnedSemaphorePermit, Semaphore};
use tokio::time::{sleep, Duration};

use crate::inference::{CandidateWithScore, InferenceError, Inferencer};

pub type PipelineResult = Vec<CandidateWithScore>;

pub struct PipelineRequest {
    pub input: String,
    pub response_tx: oneshot::Sender<Result<PipelineResult, InferenceError>>,
}

pub struct PipelineConfig {
    pub debounce_ms: u64,
    pub max_concurrent: usize,
    pub buffer_size: usize,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            debounce_ms: 150,
            max_concurrent: 3,
            buffer_size: 10,
        }
    }
}

pub struct AsyncPipeline {
    sender: mpsc::Sender<PipelineRequest>,
}

impl AsyncPipeline {
    pub fn new(inferencer: Arc<dyn Inferencer>, config: PipelineConfig) -> Self {
        let (tx, rx) = mpsc::channel(config.buffer_size);
        let semaphore = Arc::new(Semaphore::new(config.max_concurrent));
        let debounce = Duration::from_millis(config.debounce_ms);

        tokio::spawn(Self::run_worker(rx, inferencer, semaphore, debounce));

        Self { sender: tx }
    }

    pub async fn submit(&self, input: String) -> Result<PipelineResult, InferenceError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(PipelineRequest {
                input,
                response_tx: tx,
            })
            .await
            .map_err(|_| InferenceError::Unavailable("pipeline closed".to_string()))?;
        rx.await
            .map_err(|_| InferenceError::Unavailable("response channel dropped".to_string()))?
    }

    async fn run_worker(
        mut rx: mpsc::Receiver<PipelineRequest>,
        inferencer: Arc<dyn Inferencer>,
        semaphore: Arc<Semaphore>,
        debounce: Duration,
    ) {
        loop {
            let mut current = match rx.recv().await {
                Some(req) => req,
                None => break,
            };

            // Debounce: coalesce rapid requests, keeping only the latest.
            // Skip when debounce is zero to avoid starving requests via biased select.
            if !debounce.is_zero() {
                loop {
                    tokio::select! {
                        biased;
                        next = rx.recv() => {
                            match next {
                                Some(next_req) => {
                                    let _ = current.response_tx.send(Err(
                                        InferenceError::Unavailable("debounced".to_string()),
                                    ));
                                    current = next_req;
                                }
                                None => {
                                    let _ = current.response_tx.send(Err(
                                        InferenceError::Unavailable("pipeline closed".to_string()),
                                    ));
                                    return;
                                }
                            }
                        }
                        _ = sleep(debounce) => break,
                    }
                }
            }

            let permit: OwnedSemaphorePermit = semaphore.clone().acquire_owned().await.unwrap();
            let inf = inferencer.clone();
            let input = current.input;
            let response_tx = current.response_tx;

            tokio::spawn(async move {
                let _permit = permit;
                let result = inf.rerank(&input, vec![], None).await;
                let _ = response_tx.send(result);
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference::inferencer::{AlwaysSucceedInferencer, AlwaysTimeoutInferencer};
    use tokio::time::timeout;

    fn make_pipeline(inferencer: Arc<dyn Inferencer>, debounce_ms: u64) -> AsyncPipeline {
        AsyncPipeline::new(
            inferencer,
            PipelineConfig {
                debounce_ms,
                max_concurrent: 3,
                buffer_size: 10,
            },
        )
    }

    #[tokio::test]
    async fn test_p4_pipeline_submit() {
        let pipeline = make_pipeline(Arc::new(AlwaysSucceedInferencer), 0);
        let result = pipeline.submit("てすと".to_string()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_p4_pipeline_debounce_cancels_early_requests() {
        let pipeline = make_pipeline(Arc::new(AlwaysSucceedInferencer), 50);

        // Send two requests rapidly; first should be debounced (cancelled)
        let fut1 = pipeline.submit("first".to_string());
        let fut2 = pipeline.submit("second".to_string());

        let (r1, r2) = tokio::join!(fut1, fut2);

        // r1 should be Err(debounced), r2 should succeed
        assert!(
            matches!(r1, Err(InferenceError::Unavailable(ref s)) if s == "debounced"),
            "expected debounced error, got: {:?}",
            r1
        );
        assert!(r2.is_ok(), "second request should succeed, got: {:?}", r2);
    }

    #[tokio::test]
    async fn test_p4_pipeline_max_concurrent() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc as StdArc;

        struct CountingInferencer {
            peak: StdArc<AtomicUsize>,
            active: StdArc<AtomicUsize>,
        }

        #[async_trait::async_trait]
        impl Inferencer for CountingInferencer {
            fn name(&self) -> &'static str {
                "counting"
            }
            fn capabilities(&self) -> crate::inference::InferencerCapabilities {
                crate::inference::InferencerCapabilities {
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
                let prev = self.active.fetch_add(1, Ordering::SeqCst);
                self.peak.fetch_max(prev + 1, Ordering::SeqCst);
                sleep(Duration::from_millis(30)).await;
                self.active.fetch_sub(1, Ordering::SeqCst);
                Ok(candidates)
            }
        }

        let peak = StdArc::new(AtomicUsize::new(0));
        let active = StdArc::new(AtomicUsize::new(0));
        let inf = Arc::new(CountingInferencer {
            peak: peak.clone(),
            active: active.clone(),
        });

        let pipeline = AsyncPipeline::new(
            inf,
            PipelineConfig {
                debounce_ms: 0,
                max_concurrent: 3,
                buffer_size: 20,
            },
        );

        // Submit 6 requests simultaneously; peak concurrent should not exceed 3
        let futs: Vec<_> = (0..6)
            .map(|i| pipeline.submit(format!("input{i}")))
            .collect();
        let results: Vec<_> = futures::future::join_all(futs).await;
        assert!(results.iter().all(|r| r.is_ok()));
        assert!(
            peak.load(Ordering::SeqCst) <= 3,
            "peak concurrency exceeded max_concurrent=3: {}",
            peak.load(Ordering::SeqCst)
        );
    }

    #[tokio::test]
    async fn test_p4_pipeline_cancel_via_debounce() {
        let pipeline = make_pipeline(Arc::new(AlwaysSucceedInferencer), 200);

        let fut1 = pipeline.submit("a".to_string());
        // Give the first request a tiny head start then submit second immediately
        tokio::time::sleep(Duration::from_millis(5)).await;
        let fut2 = pipeline.submit("b".to_string());

        let (r1, r2) = tokio::join!(fut1, fut2);
        assert!(
            matches!(r1, Err(InferenceError::Unavailable(_))),
            "first request should be cancelled"
        );
        assert!(r2.is_ok(), "second request should succeed");
    }

    #[tokio::test]
    async fn test_p4_pipeline_timeout_inferencer() {
        let pipeline = make_pipeline(Arc::new(AlwaysTimeoutInferencer), 0);
        let result = timeout(
            Duration::from_secs(10),
            pipeline.submit("てすと".to_string()),
        )
        .await
        .expect("test timed out");
        assert!(
            matches!(result, Err(InferenceError::Timeout(_))),
            "expected Timeout error, got: {:?}",
            result
        );
    }
}
