use std::future::Future;
use std::time::{Duration, Instant};
use tokio::time::sleep;

use crate::inference::error::InferenceError;

pub struct RetryConfig {
    pub max_retries: u32,
    pub initial_backoff_ms: u64,
    pub backoff_factor: f64,
    pub jitter_pct: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 2,
            initial_backoff_ms: 100,
            backoff_factor: 3.0,
            jitter_pct: 0.2,
        }
    }
}

pub enum RetryDecision<E> {
    Retryable(E),
    Fatal(E),
}

impl<E> RetryDecision<E> {
    pub fn into_inner(self) -> E {
        match self {
            Self::Retryable(e) | Self::Fatal(e) => e,
        }
    }
}

pub async fn with_retry<F, Fut, T>(
    cfg: &RetryConfig,
    deadline: Instant,
    mut f: F,
) -> Result<T, InferenceError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, RetryDecision<InferenceError>>>,
{
    let mut attempt = 0u32;
    loop {
        let remaining = deadline.checked_duration_since(Instant::now());
        if remaining.map_or(true, |d| d.is_zero()) {
            return Err(InferenceError::Timeout(Duration::ZERO));
        }

        match f().await {
            Ok(v) => return Ok(v),
            Err(RetryDecision::Fatal(e)) => return Err(e),
            Err(RetryDecision::Retryable(e)) => {
                if attempt >= cfg.max_retries {
                    return Err(e);
                }
                let backoff = compute_backoff(cfg, attempt);
                let remaining = deadline
                    .checked_duration_since(Instant::now())
                    .unwrap_or(Duration::ZERO);
                if backoff >= remaining {
                    return Err(InferenceError::Timeout(remaining));
                }
                sleep(backoff).await;
                attempt += 1;
            }
        }
    }
}

fn compute_backoff(cfg: &RetryConfig, attempt: u32) -> Duration {
    let base = cfg.initial_backoff_ms as f64 * cfg.backoff_factor.powi(attempt as i32);
    let ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    let jitter_ratio = (ns % 1000) as f64 / 1000.0 * 2.0 - 1.0;
    let jitter = base * cfg.jitter_pct * jitter_ratio;
    Duration::from_millis((base + jitter).max(1.0) as u64)
}

#[cfg(test)]
mod p4_retry {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    fn make_deadline(ms: u64) -> Instant {
        Instant::now() + Duration::from_millis(ms)
    }

    fn cfg_fast() -> RetryConfig {
        RetryConfig {
            max_retries: 2,
            initial_backoff_ms: 1,
            backoff_factor: 1.0,
            jitter_pct: 0.0,
        }
    }

    #[tokio::test]
    async fn test_p4_retry_success_immediate() {
        let cfg = cfg_fast();
        let result: Result<u32, InferenceError> = with_retry(&cfg, make_deadline(500), || async {
            Ok::<u32, RetryDecision<InferenceError>>(42)
        })
        .await;
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_p4_retry_success_after_retry() {
        let cfg = cfg_fast();
        let count = Arc::new(AtomicU32::new(0));
        let c = count.clone();
        let result: Result<u32, InferenceError> = with_retry(&cfg, make_deadline(500), || {
            let c = c.clone();
            async move {
                let n = c.fetch_add(1, Ordering::SeqCst);
                if n == 0 {
                    Err(RetryDecision::Retryable(InferenceError::Upstream(
                        anyhow::anyhow!("HTTP 503: Service Unavailable"),
                    )))
                } else {
                    Ok(99u32)
                }
            }
        })
        .await;
        assert_eq!(result.unwrap(), 99);
        assert_eq!(count.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn test_p4_retry_max_retry_exceeded() {
        let cfg = cfg_fast();
        let count = Arc::new(AtomicU32::new(0));
        let c = count.clone();
        let result: Result<u32, InferenceError> = with_retry(&cfg, make_deadline(500), || {
            let c = c.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Err::<u32, _>(RetryDecision::Retryable(InferenceError::Upstream(
                    anyhow::anyhow!("HTTP 429: Too Many Requests"),
                )))
            }
        })
        .await;
        assert!(result.is_err());
        // initial + 2 retries = 3 attempts
        assert_eq!(count.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_p4_retry_fatal_no_retry() {
        let cfg = cfg_fast();
        let count = Arc::new(AtomicU32::new(0));
        let c = count.clone();
        let result: Result<u32, InferenceError> = with_retry(&cfg, make_deadline(500), || {
            let c = c.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Err::<u32, _>(RetryDecision::Fatal(InferenceError::Upstream(
                    anyhow::anyhow!("HTTP 400: Bad Request"),
                )))
            }
        })
        .await;
        assert!(result.is_err());
        // must not retry — exactly 1 attempt
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_p4_retry_budget_exceeded() {
        // backoff_ms=200 > deadline of 50ms → Timeout on first retry attempt
        let cfg = RetryConfig {
            max_retries: 2,
            initial_backoff_ms: 200,
            backoff_factor: 1.0,
            jitter_pct: 0.0,
        };
        let result: Result<u32, InferenceError> = with_retry(&cfg, make_deadline(50), || async {
            Err::<u32, _>(RetryDecision::Retryable(InferenceError::Upstream(
                anyhow::anyhow!("HTTP 503: Service Unavailable"),
            )))
        })
        .await;
        assert!(matches!(result, Err(InferenceError::Timeout(_))));
    }

    #[tokio::test]
    async fn test_p4_retry_deadline_already_expired() {
        let cfg = cfg_fast();
        let past = Instant::now() - Duration::from_millis(1);
        let result: Result<u32, InferenceError> = with_retry(&cfg, past, || async {
            Ok::<u32, RetryDecision<InferenceError>>(1)
        })
        .await;
        assert!(matches!(result, Err(InferenceError::Timeout(_))));
    }
}
