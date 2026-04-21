use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::inference::{
    capabilities::InferencerCapabilities,
    error::InferenceError,
    inferencer::{CandidateSource, CandidateWithScore, Inferencer},
};

pub struct OllamaInferencer {
    endpoint: String,
    model: String,
    timeout: Duration,
    client: reqwest::Client,
}

impl OllamaInferencer {
    pub fn new(endpoint: String, model: String) -> Self {
        let client = reqwest::Client::builder()
            .pool_max_idle_per_host(5)
            .build()
            .expect("failed to build reqwest Client");
        Self {
            endpoint,
            model,
            timeout: Duration::from_millis(500),
            client,
        }
    }

    #[cfg(test)]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

#[derive(Serialize)]
struct OllamaRequest {
    model: String,
    prompt: String,
    stream: bool,
}

#[derive(Deserialize)]
struct OllamaResponse {
    response: String,
}

fn build_prompt(
    reading: &str,
    candidates: &[CandidateWithScore],
    left_context: Option<&str>,
    right_context: Option<&str>,
) -> String {
    let context_line = match right_context {
        Some(ctx) if !ctx.is_empty() => {
            let left = left_context.unwrap_or_default();
            format!("文脈: {} [ここ] {}\n", left, ctx)
        }
        _ => match left_context {
            Some(ctx) if !ctx.is_empty() => format!("直前の文脈: {}\n", ctx),
            _ => String::new(),
        },
    };
    let candidate_list: Vec<String> = candidates
        .iter()
        .enumerate()
        .map(|(i, c)| format!("{}. {}", i + 1, c.surface))
        .collect();
    format!(
        "{}以下の日本語入力読みに対して、最も適切な変換候補を選んでください。\n読み: {}\n候補: {}\n最適な候補の番号を答えてください:",
        context_line,
        reading,
        candidate_list.join(", ")
    )
}

fn parse_best_index(response: &str, len: usize) -> Option<usize> {
    let trimmed = response.trim();
    if let Ok(n) = trimmed.parse::<usize>() {
        if n >= 1 && n <= len {
            return Some(n - 1);
        }
    }
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

async fn call_ollama(
    client: &reqwest::Client,
    url: &str,
    payload: &OllamaRequest,
    timeout: Duration,
) -> Result<OllamaResponse, InferenceError> {
    let deadline = Instant::now() + timeout;
    let remaining = deadline
        .checked_duration_since(Instant::now())
        .unwrap_or(Duration::ZERO);

    let response = tokio::time::timeout(remaining, client.post(url).json(payload).send())
        .await
        .map_err(|_| InferenceError::Timeout(timeout))?
        .map_err(|e| {
            if e.is_connect() {
                InferenceError::Unavailable(format!("Ollama not running: {}", e))
            } else {
                InferenceError::Upstream(e.into())
            }
        })?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(InferenceError::Upstream(anyhow::anyhow!(
            "HTTP {}: {}",
            status,
            body
        )));
    }

    response
        .json()
        .await
        .map_err(|e| InferenceError::Upstream(e.into()))
}

#[async_trait]
impl Inferencer for OllamaInferencer {
    fn name(&self) -> &'static str {
        "ollama"
    }

    fn capabilities(&self) -> InferencerCapabilities {
        InferencerCapabilities {
            supports_rerank: true,
            supports_right_context: true,
        }
    }

    async fn rerank(
        &self,
        reading: &str,
        candidates: Vec<CandidateWithScore>,
        left_context: Option<&str>,
    ) -> Result<Vec<CandidateWithScore>, InferenceError> {
        self.rerank_with_right_context(reading, candidates, left_context, None)
            .await
    }

    async fn rerank_with_right_context(
        &self,
        reading: &str,
        mut candidates: Vec<CandidateWithScore>,
        left_context: Option<&str>,
        right_context: Option<&str>,
    ) -> Result<Vec<CandidateWithScore>, InferenceError> {
        if candidates.is_empty() {
            return Ok(candidates);
        }

        let prompt = build_prompt(reading, &candidates, left_context, right_context);
        let url = format!("{}/api/generate", self.endpoint);
        let payload = OllamaRequest {
            model: self.model.clone(),
            prompt,
            stream: false,
        };

        let api_resp = call_ollama(&self.client, &url, &payload, self.timeout).await?;

        let len = candidates.len();
        if let Some(best_idx) = parse_best_index(&api_resp.response, len) {
            for (i, c) in candidates.iter_mut().enumerate() {
                c.source = CandidateSource::Ollama;
                c.score = if i == best_idx { 10.0 } else { 1.0 };
            }
            candidates.swap(0, best_idx);
        }

        Ok(candidates)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

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

    fn make_inferencer(base_url: &str) -> OllamaInferencer {
        OllamaInferencer::new(base_url.to_string(), "qwen2.5:1.5b".to_string())
    }

    #[tokio::test]
    async fn test_rerank_success_first_candidate() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/generate"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"response": "1", "done": true})),
            )
            .mount(&server)
            .await;

        let inf = make_inferencer(&server.uri());
        let result = inf.rerank("てんき", make_candidates(), None).await.unwrap();
        assert_eq!(result[0].surface, "天気");
        assert_eq!(result[0].score, 10.0);
        assert!(matches!(result[0].source, CandidateSource::Ollama));
    }

    #[tokio::test]
    async fn test_rerank_second_candidate_best() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/generate"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"response": "2", "done": true})),
            )
            .mount(&server)
            .await;

        let inf = make_inferencer(&server.uri());
        let result = inf.rerank("てんき", make_candidates(), None).await.unwrap();
        assert_eq!(result[0].surface, "転機");
        assert_eq!(result[0].score, 10.0);
    }

    #[tokio::test]
    async fn test_rerank_empty_candidates() {
        let inf = make_inferencer("http://localhost:11434");
        let result = inf.rerank("てんき", vec![], None).await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_rerank_unavailable_when_not_running() {
        let inf = OllamaInferencer::new(
            "http://127.0.0.1:19999".to_string(),
            "qwen2.5:1.5b".to_string(),
        );
        let result = inf.rerank("てんき", make_candidates(), None).await;
        assert!(matches!(result, Err(InferenceError::Unavailable(_))));
    }

    #[tokio::test]
    async fn test_rerank_timeout() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/generate"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_delay(Duration::from_millis(2000))
                    .set_body_json(serde_json::json!({"response": "1", "done": true})),
            )
            .mount(&server)
            .await;

        let inf = make_inferencer(&server.uri()).with_timeout(Duration::from_millis(100));
        let result = inf.rerank("てんき", make_candidates(), None).await;
        assert!(matches!(result, Err(InferenceError::Timeout(_))));
    }

    #[tokio::test]
    async fn test_rerank_with_right_context() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/generate"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"response": "1", "done": true})),
            )
            .mount(&server)
            .await;

        let inf = make_inferencer(&server.uri());
        let result = inf
            .rerank_with_right_context("てんき", make_candidates(), Some("明日の"), Some("予報"))
            .await
            .unwrap();

        let requests = server.received_requests().await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
        let prompt = body["prompt"].as_str().unwrap();
        assert!(prompt.contains("明日の"));
        assert!(prompt.contains("予報"));
        assert_eq!(result[0].surface, "天気");
    }

    #[test]
    fn test_parse_best_index_digit() {
        assert_eq!(parse_best_index("1", 3), Some(0));
        assert_eq!(parse_best_index("2", 3), Some(1));
        assert_eq!(parse_best_index("3", 3), Some(2));
        assert_eq!(parse_best_index("4", 3), None);
        assert_eq!(parse_best_index("0", 3), None);
    }

    #[test]
    fn test_parse_best_index_in_text() {
        assert_eq!(parse_best_index("答えは2です", 3), Some(1));
    }

    #[test]
    fn test_capabilities() {
        let inf = OllamaInferencer::new(
            "http://localhost:11434".to_string(),
            "qwen2.5:1.5b".to_string(),
        );
        let caps = inf.capabilities();
        assert!(caps.supports_rerank);
        assert!(caps.supports_right_context);
        assert_eq!(inf.name(), "ollama");
    }

    #[test]
    fn test_build_prompt_no_context() {
        let candidates = make_candidates();
        let prompt = build_prompt("てんき", &candidates, None, None);
        assert!(prompt.contains("てんき"));
        assert!(prompt.contains("1. 天気"));
        assert!(prompt.contains("2. 転機"));
        assert!(!prompt.contains("文脈"));
    }

    #[test]
    fn test_build_prompt_with_left_context() {
        let candidates = make_candidates();
        let prompt = build_prompt("てんき", &candidates, Some("明日の"), None);
        assert!(prompt.contains("直前の文脈: 明日の"));
    }

    #[test]
    fn test_build_prompt_with_right_context() {
        let candidates = make_candidates();
        let prompt = build_prompt("てんき", &candidates, Some("明日の"), Some("予報"));
        assert!(prompt.contains("文脈: 明日の [ここ] 予報"));
    }
}
