use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::inference::{
    capabilities::InferencerCapabilities,
    error::InferenceError,
    inferencer::{CandidateSource, CandidateWithScore, Inferencer},
};

pub struct WorkersAIInferencer {
    account_id: String,
    api_token: String,
    model_id: String,
    timeout: Duration,
    client: reqwest::Client,
}

impl WorkersAIInferencer {
    pub fn new(account_id: String, api_token: String, model_id: String) -> Self {
        Self {
            account_id,
            api_token,
            model_id,
            timeout: Duration::from_millis(1500),
            client: reqwest::Client::new(),
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn from_env() -> anyhow::Result<Self> {
        let _ = dotenvy::from_filename(".env.local");
        let account_id = std::env::var("CLOUDFLARE_ACCOUNT_ID")
            .map_err(|_| anyhow::anyhow!("CLOUDFLARE_ACCOUNT_ID not set"))?;
        let api_token = std::env::var("CLOUDFLARE_API_TOKEN")
            .map_err(|_| anyhow::anyhow!("CLOUDFLARE_API_TOKEN not set"))?;
        let model_id = std::env::var("WORKERS_AI_MODEL_ID")
            .unwrap_or_else(|_| "@cf/qwen/qwen3-30b-a3b-fp8".to_string());
        Ok(Self::new(account_id, api_token, model_id))
    }
}

#[derive(Serialize)]
struct WorkersAIRequest {
    messages: Vec<WorkersAIMessage>,
}

#[derive(Serialize)]
struct WorkersAIMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct WorkersAIResponse {
    result: WorkersAIResult,
}

#[derive(Deserialize)]
struct WorkersAIResult {
    response: String,
}

fn build_prompt(reading: &str, candidates: &[CandidateWithScore]) -> String {
    let candidate_list: Vec<String> = candidates
        .iter()
        .enumerate()
        .map(|(i, c)| format!("{}. {}", i + 1, c.surface))
        .collect();
    format!(
        "次の読み仮名に対する変換候補を自然な日本語として正しい順に並べ、1位の番号のみを答えよ（数字のみ）:\n読み: {}\n候補:\n{}\n答え:",
        reading,
        candidate_list.join("\n")
    )
}

fn parse_best_index(response: &str, len: usize) -> Option<usize> {
    let trimmed = response.trim();
    if let Ok(n) = trimmed.parse::<usize>() {
        if n >= 1 && n <= len {
            return Some(n - 1);
        }
    }
    // Try extracting first digit
    for ch in trimmed.chars() {
        if let Some(d) = ch.to_digit(10) {
            let idx = d as usize;
            if idx >= 1 && idx <= len {
                return Some(idx - 1);
            }
        }
    }
    None
}

#[async_trait]
impl Inferencer for WorkersAIInferencer {
    fn name(&self) -> &'static str {
        "workers-ai"
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
        mut candidates: Vec<CandidateWithScore>,
        _left_context: Option<&str>,
    ) -> Result<Vec<CandidateWithScore>, InferenceError> {
        if candidates.is_empty() {
            return Ok(candidates);
        }

        let prompt = build_prompt(reading, &candidates);
        let url = format!(
            "https://api.cloudflare.com/client/v4/accounts/{}/ai/run/{}",
            self.account_id, self.model_id
        );
        let payload = WorkersAIRequest {
            messages: vec![WorkersAIMessage {
                role: "user".to_string(),
                content: prompt,
            }],
        };

        let request = self
            .client
            .post(&url)
            .bearer_auth(&self.api_token)
            .json(&payload);

        let response = tokio::time::timeout(self.timeout, request.send())
            .await
            .map_err(|_| InferenceError::Timeout(self.timeout))?
            .map_err(|e| InferenceError::Upstream(e.into()))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(InferenceError::Upstream(anyhow::anyhow!(
                "HTTP {}: {}",
                status,
                body
            )));
        }

        let api_resp: WorkersAIResponse = response
            .json()
            .await
            .map_err(|e| InferenceError::Upstream(e.into()))?;

        let len = candidates.len();
        if let Some(best_idx) = parse_best_index(&api_resp.result.response, len) {
            // Assign scores: best gets highest, others get lower
            for (i, c) in candidates.iter_mut().enumerate() {
                c.source = CandidateSource::WorkersAI;
                c.score = if i == best_idx { 10.0 } else { 1.0 };
            }
            // Move best to front
            candidates.swap(0, best_idx);
        }

        Ok(candidates)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path_regex};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    struct TestWorkersAIInferencer {
        inner: WorkersAIInferencer,
        base_url: String,
    }

    impl TestWorkersAIInferencer {
        fn new(base_url: String) -> Self {
            Self {
                inner: WorkersAIInferencer {
                    account_id: "test_account".to_string(),
                    api_token: "test_token".to_string(),
                    model_id: "@cf/qwen/qwen3-30b-a3b-fp8".to_string(),
                    timeout: Duration::from_millis(1500),
                    client: reqwest::Client::new(),
                },
                base_url,
            }
        }

        fn with_timeout(mut self, timeout: Duration) -> Self {
            self.inner.timeout = timeout;
            self
        }

        async fn rerank(
            &self,
            reading: &str,
            mut candidates: Vec<CandidateWithScore>,
            _left_context: Option<&str>,
        ) -> Result<Vec<CandidateWithScore>, InferenceError> {
            if candidates.is_empty() {
                return Ok(candidates);
            }

            let prompt = build_prompt(reading, &candidates);
            let url = format!(
                "{}/client/v4/accounts/{}/ai/run/{}",
                self.base_url, self.inner.account_id, self.inner.model_id
            );
            let payload = WorkersAIRequest {
                messages: vec![WorkersAIMessage {
                    role: "user".to_string(),
                    content: prompt,
                }],
            };

            let request = self
                .inner
                .client
                .post(&url)
                .bearer_auth(&self.inner.api_token)
                .json(&payload);

            let response = tokio::time::timeout(self.inner.timeout, request.send())
                .await
                .map_err(|_| InferenceError::Timeout(self.inner.timeout))?
                .map_err(|e| InferenceError::Upstream(e.into()))?;

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                return Err(InferenceError::Upstream(anyhow::anyhow!(
                    "HTTP {}: {}",
                    status,
                    body
                )));
            }

            let api_resp: WorkersAIResponse = response
                .json()
                .await
                .map_err(|e| InferenceError::Upstream(e.into()))?;

            let len = candidates.len();
            if let Some(best_idx) = parse_best_index(&api_resp.result.response, len) {
                for (i, c) in candidates.iter_mut().enumerate() {
                    c.source = CandidateSource::WorkersAI;
                    c.score = if i == best_idx { 10.0 } else { 1.0 };
                }
                candidates.swap(0, best_idx);
            }

            Ok(candidates)
        }
    }

    fn make_candidates() -> Vec<CandidateWithScore> {
        vec![
            CandidateWithScore {
                surface: "天気".to_string(),
                score: 1.0,
                source: CandidateSource::Ngram,
            },
            CandidateWithScore {
                surface: "転機".to_string(),
                score: 0.8,
                source: CandidateSource::Ngram,
            },
            CandidateWithScore {
                surface: "点鬼".to_string(),
                score: 0.3,
                source: CandidateSource::Ngram,
            },
        ]
    }

    #[tokio::test]
    async fn test_rerank_success() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path_regex(".*/ai/run/.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "result": { "response": "1" },
                "success": true
            })))
            .mount(&server)
            .await;

        let inferencer = TestWorkersAIInferencer::new(server.uri());
        let candidates = make_candidates();
        let result = inferencer.rerank("てんき", candidates, None).await.unwrap();

        assert_eq!(result.len(), 3);
        assert_eq!(result[0].surface, "天気");
        assert_eq!(result[0].score, 10.0);
        assert!(matches!(result[0].source, CandidateSource::WorkersAI));
    }

    #[tokio::test]
    async fn test_rerank_second_candidate_best() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path_regex(".*/ai/run/.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "result": { "response": "2" },
                "success": true
            })))
            .mount(&server)
            .await;

        let inferencer = TestWorkersAIInferencer::new(server.uri());
        let candidates = make_candidates();
        let result = inferencer.rerank("てんき", candidates, None).await.unwrap();

        assert_eq!(result[0].surface, "転機");
        assert_eq!(result[0].score, 10.0);
    }

    #[tokio::test]
    async fn test_rerank_timeout() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path_regex(".*/ai/run/.*"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_delay(Duration::from_millis(2000))
                    .set_body_json(serde_json::json!({
                        "result": { "response": "1" },
                        "success": true
                    })),
            )
            .mount(&server)
            .await;

        let inferencer =
            TestWorkersAIInferencer::new(server.uri()).with_timeout(Duration::from_millis(100));
        let candidates = make_candidates();
        let result = inferencer.rerank("てんき", candidates, None).await;

        assert!(matches!(result, Err(InferenceError::Timeout(_))));
    }

    #[tokio::test]
    async fn test_rerank_http_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path_regex(".*/ai/run/.*"))
            .respond_with(ResponseTemplate::new(500).set_body_string("Internal Server Error"))
            .mount(&server)
            .await;

        let inferencer = TestWorkersAIInferencer::new(server.uri());
        let candidates = make_candidates();
        let result = inferencer.rerank("てんき", candidates, None).await;

        assert!(matches!(result, Err(InferenceError::Upstream(_))));
    }

    #[tokio::test]
    async fn test_rerank_empty_candidates() {
        let inferencer = TestWorkersAIInferencer::new("http://localhost:1".to_string());
        let result = inferencer.rerank("てんき", vec![], None).await.unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_best_index() {
        assert_eq!(parse_best_index("1", 3), Some(0));
        assert_eq!(parse_best_index("2", 3), Some(1));
        assert_eq!(parse_best_index("3", 3), Some(2));
        assert_eq!(parse_best_index("4", 3), None);
        assert_eq!(parse_best_index("0", 3), None);
        assert_eq!(parse_best_index("答えは2です", 3), Some(1));
    }

    #[test]
    fn test_capabilities() {
        let inf = WorkersAIInferencer::new(
            "acct".to_string(),
            "tok".to_string(),
            "@cf/qwen/qwen3-30b-a3b-fp8".to_string(),
        );
        let caps = inf.capabilities();
        assert!(caps.supports_rerank);
        assert!(!caps.supports_right_context);
        assert_eq!(inf.name(), "workers-ai");
    }
}
