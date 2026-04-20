use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::consent::ConsentManager;
use crate::inference::{
    capabilities::InferencerCapabilities,
    cost_guard::CostGuard,
    cost_monitor::CostMonitor,
    error::InferenceError,
    inferencer::{CandidateSource, CandidateWithScore, Inferencer},
    latency_tracker::LatencyTracker,
    retry::{with_retry, RetryConfig, RetryDecision},
};

pub struct WorkersAIInferencer {
    account_id: String,
    api_token: String,
    model_id: String,
    timeout: Duration,
    client: reqwest::Client,
    retry_config: RetryConfig,
    consent_manager: Option<ConsentManager>,
    latency_tracker: Arc<LatencyTracker>,
    cost_monitor: Option<Arc<CostMonitor>>,
    cost_guard: Option<Arc<CostGuard>>,
}

impl WorkersAIInferencer {
    pub fn new(account_id: String, api_token: String, model_id: String) -> Self {
        let client = reqwest::Client::builder()
            .pool_max_idle_per_host(5)
            .tcp_keepalive(Duration::from_secs(30))
            .build()
            .expect("failed to build reqwest Client");
        Self {
            account_id,
            api_token,
            model_id,
            timeout: Duration::from_millis(1500),
            client,
            retry_config: RetryConfig::default(),
            consent_manager: None,
            latency_tracker: LatencyTracker::new(100),
            cost_monitor: None,
            cost_guard: None,
        }
    }

    pub fn new_with_monitor(
        account_id: String,
        api_token: String,
        model_id: String,
        monitor: Arc<CostMonitor>,
    ) -> Self {
        let mut s = Self::new(account_id, api_token, model_id);
        s.cost_monitor = Some(monitor);
        s
    }

    pub fn with_cost_guard(mut self, guard: Arc<CostGuard>) -> Self {
        self.cost_guard = Some(guard);
        self
    }

    pub fn latency_tracker(&self) -> Arc<LatencyTracker> {
        Arc::clone(&self.latency_tracker)
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn with_consent_manager(mut self, cm: ConsentManager) -> Self {
        self.consent_manager = Some(cm);
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

#[derive(Deserialize)]
struct RerankJsonOutput {
    best_index: usize,
    #[allow(dead_code)]
    confidence: Option<f64>,
}

fn build_system_prompt(left_context: Option<&str>, right_context: Option<&str>) -> String {
    let context_line = match right_context {
        Some(ctx) if !ctx.is_empty() => {
            let left = left_context.unwrap_or_default();
            format!("入力中の文字列: {} [ここまで入力] {}\n\n", left, ctx)
        }
        _ => match left_context {
            Some(ctx) if !ctx.is_empty() => {
                format!("直前の文脈: {}\n\n", ctx)
            }
            _ => String::new(),
        },
    };
    format!(
        "あなたは日本語IMEのリランキングエンジンです。\n\
        {}読み仮名と変換候補リストを受け取り、最も自然な候補のインデックス（1始まり）と確信度を\
        JSONで返してください。\n\n\
        出力フォーマット（他のテキストは一切含めないこと）:\n\
        {{\"best_index\": <番号>, \"confidence\": <0.0〜1.0>}}\n\n\
        例:\n\
        読み: きょう、候補: [1. 今日, 2. 京, 3. 今夕]\n\
        → {{\"best_index\": 1, \"confidence\": 0.95}}",
        context_line
    )
}

fn build_user_prompt(reading: &str, candidates: &[CandidateWithScore]) -> String {
    let candidate_list: Vec<String> = candidates
        .iter()
        .enumerate()
        .map(|(i, c)| format!("{}. {}", i + 1, c.surface))
        .collect();
    format!("読み: {}\n候補: [{}]", reading, candidate_list.join(", "))
}

fn parse_best_index(response: &str, len: usize) -> Option<usize> {
    let trimmed = response.trim();
    if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed.rfind('}') {
            let json_str = &trimmed[start..=end];
            if let Ok(parsed) = serde_json::from_str::<RerankJsonOutput>(json_str) {
                let n = parsed.best_index;
                if n >= 1 && n <= len {
                    return Some(n - 1);
                }
            }
        }
    }

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

fn classify_reqwest_error(e: reqwest::Error) -> RetryDecision<InferenceError> {
    if e.is_connect() || e.is_timeout() {
        RetryDecision::Retryable(InferenceError::Upstream(e.into()))
    } else {
        RetryDecision::Fatal(InferenceError::Upstream(e.into()))
    }
}

async fn call_workers_ai_once(
    client: &reqwest::Client,
    url: &str,
    api_token: &str,
    payload: &WorkersAIRequest,
    remaining: Duration,
) -> Result<WorkersAIResponse, RetryDecision<InferenceError>> {
    let request = client.post(url).bearer_auth(api_token).json(payload);

    let response = tokio::time::timeout(remaining, request.send())
        .await
        .map_err(|_| RetryDecision::Fatal(InferenceError::Timeout(remaining)))?
        .map_err(classify_reqwest_error)?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        let err = InferenceError::Upstream(anyhow::anyhow!("HTTP {}: {}", status, body));
        return Err(match status.as_u16() {
            429 | 502 | 503 | 504 => RetryDecision::Retryable(err),
            _ => RetryDecision::Fatal(err),
        });
    }

    response
        .json()
        .await
        .map_err(|e| RetryDecision::Fatal(InferenceError::Upstream(e.into())))
}

#[async_trait]
impl Inferencer for WorkersAIInferencer {
    fn name(&self) -> &'static str {
        "workers-ai"
    }

    fn capabilities(&self) -> InferencerCapabilities {
        InferencerCapabilities {
            supports_rerank: true,
            supports_right_context: true,
        }
    }

    async fn warmup(&self) -> Result<(), InferenceError> {
        let warmup_timeout = Duration::from_secs(2);
        let url = format!(
            "https://api.cloudflare.com/client/v4/accounts/{}/ai/run/{}",
            self.account_id, self.model_id
        );
        let payload = WorkersAIRequest {
            messages: vec![WorkersAIMessage {
                role: "user".to_string(),
                content: "1".to_string(),
            }],
        };
        let request = self
            .client
            .post(&url)
            .bearer_auth(&self.api_token)
            .json(&payload);
        tokio::time::timeout(warmup_timeout, request.send())
            .await
            .map_err(|_| InferenceError::Unavailable("warmup timeout".to_string()))?
            .map_err(|e| InferenceError::Unavailable(e.to_string()))?;
        Ok(())
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
        if let Some(cm) = &self.consent_manager {
            if !cm.is_consented() {
                return Err(InferenceError::ConsentRequired);
            }
        }

        if let Some(cg) = &self.cost_guard {
            cg.check()?;
        }

        if candidates.is_empty() {
            return Ok(candidates);
        }

        let system_prompt = build_system_prompt(left_context, right_context);
        let user_prompt = build_user_prompt(reading, &candidates);
        let url = format!(
            "https://api.cloudflare.com/client/v4/accounts/{}/ai/run/{}",
            self.account_id, self.model_id
        );
        let payload = WorkersAIRequest {
            messages: vec![
                WorkersAIMessage {
                    role: "system".to_string(),
                    content: system_prompt,
                },
                WorkersAIMessage {
                    role: "user".to_string(),
                    content: user_prompt,
                },
            ],
        };

        let deadline = Instant::now() + self.timeout;
        let t0 = Instant::now();
        let client = &self.client;
        let api_resp = with_retry(&self.retry_config, deadline, || {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .unwrap_or(Duration::ZERO);
            call_workers_ai_once(client, &url, &self.api_token, &payload, remaining)
        })
        .await?;
        self.latency_tracker.record(t0.elapsed().as_millis() as u64);
        if let Some(m) = &self.cost_monitor {
            m.record(&self.model_id);
        }

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

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path_regex};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    struct TestWorkersAIInferencer {
        account_id: String,
        api_token: String,
        model_id: String,
        timeout: Duration,
        client: reqwest::Client,
        base_url: String,
        retry_config: RetryConfig,
    }

    impl TestWorkersAIInferencer {
        fn new(base_url: String) -> Self {
            Self {
                account_id: "test_account".to_string(),
                api_token: "test_token".to_string(),
                model_id: "@cf/qwen/qwen3-30b-a3b-fp8".to_string(),
                timeout: Duration::from_millis(1500),
                client: reqwest::Client::new(),
                base_url,
                retry_config: RetryConfig {
                    max_retries: 2,
                    initial_backoff_ms: 1,
                    backoff_factor: 1.0,
                    jitter_pct: 0.0,
                },
            }
        }

        fn with_timeout(mut self, timeout: Duration) -> Self {
            self.timeout = timeout;
            self
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

            let system_prompt = build_system_prompt(left_context, right_context);
            let user_prompt = build_user_prompt(reading, &candidates);
            let url = format!(
                "{}/client/v4/accounts/{}/ai/run/{}",
                self.base_url, self.account_id, self.model_id
            );
            let payload = WorkersAIRequest {
                messages: vec![
                    WorkersAIMessage {
                        role: "system".to_string(),
                        content: system_prompt,
                    },
                    WorkersAIMessage {
                        role: "user".to_string(),
                        content: user_prompt,
                    },
                ],
            };

            let deadline = Instant::now() + self.timeout;
            let client = &self.client;
            let api_resp = with_retry(&self.retry_config, deadline, || {
                let remaining = deadline
                    .checked_duration_since(Instant::now())
                    .unwrap_or(Duration::ZERO);
                call_workers_ai_once(client, &url, &self.api_token, &payload, remaining)
            })
            .await?;

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
    async fn test_rerank_success_json() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path_regex(".*/ai/run/.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "result": { "response": "{\"best_index\": 1, \"confidence\": 0.95}" },
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
    async fn test_rerank_json_with_left_context() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path_regex(".*/ai/run/.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "result": { "response": "{\"best_index\": 1, \"confidence\": 0.9}" },
                "success": true
            })))
            .mount(&server)
            .await;

        let inferencer = TestWorkersAIInferencer::new(server.uri());
        let candidates = make_candidates();
        let result = inferencer
            .rerank("てんき", candidates, Some("明日の"))
            .await
            .unwrap();

        assert_eq!(result[0].surface, "天気");
    }

    #[tokio::test]
    async fn test_rerank_with_right_context_embeds_prompt() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path_regex(".*/ai/run/.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "result": { "response": "{\"best_index\": 1, \"confidence\": 0.95}" },
                "success": true
            })))
            .mount(&server)
            .await;

        let inferencer = TestWorkersAIInferencer::new(server.uri());
        let candidates = make_candidates();
        let _ = inferencer
            .rerank_with_right_context("てんき", candidates, Some("明日の"), Some("天気"))
            .await
            .unwrap();

        let requests = server.received_requests().await.unwrap();
        let body = String::from_utf8_lossy(&requests[0].body);
        assert!(body.contains("入力中の文字列: 明日の [ここまで入力] 天気"));
    }

    #[tokio::test]
    async fn test_rerank_with_empty_right_context_falls_back_to_rerank() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path_regex(".*/ai/run/.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "result": { "response": "{\"best_index\": 1, \"confidence\": 0.95}" },
                "success": true
            })))
            .mount(&server)
            .await;

        let inferencer = TestWorkersAIInferencer::new(server.uri());
        let base = inferencer
            .rerank("てんき", make_candidates(), Some("明日の"))
            .await
            .unwrap();
        let with_empty_right = inferencer
            .rerank_with_right_context("てんき", make_candidates(), Some("明日の"), Some(""))
            .await
            .unwrap();

        assert_eq!(with_empty_right[0].surface, base[0].surface);
        assert_eq!(with_empty_right[0].score, base[0].score);
        assert!(matches!(
            with_empty_right[0].source,
            CandidateSource::WorkersAI
        ));
    }

    #[tokio::test]
    async fn test_rerank_second_candidate_best() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path_regex(".*/ai/run/.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "result": { "response": "{\"best_index\": 2, \"confidence\": 0.8}" },
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
    async fn test_rerank_fallback_plain_number() {
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

        assert_eq!(result[0].surface, "天気");
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
    fn test_parse_best_index_json() {
        assert_eq!(
            parse_best_index(r#"{"best_index": 1, "confidence": 0.9}"#, 3),
            Some(0)
        );
        assert_eq!(
            parse_best_index(r#"{"best_index": 2, "confidence": 0.8}"#, 3),
            Some(1)
        );
        assert_eq!(
            parse_best_index(r#"{"best_index": 4, "confidence": 0.5}"#, 3),
            None
        );
    }

    #[test]
    fn test_parse_best_index_fallback() {
        assert_eq!(parse_best_index("1", 3), Some(0));
        assert_eq!(parse_best_index("2", 3), Some(1));
        assert_eq!(parse_best_index("3", 3), Some(2));
        assert_eq!(parse_best_index("4", 3), None);
        assert_eq!(parse_best_index("0", 3), None);
        assert_eq!(parse_best_index("答えは2です", 3), Some(1));
    }

    #[test]
    fn test_parse_best_index_json_embedded() {
        assert_eq!(
            parse_best_index(r#"回答: {"best_index": 3, "confidence": 0.7}"#, 3),
            Some(2)
        );
    }

    #[test]
    fn test_build_system_prompt_no_context() {
        let prompt = build_system_prompt(None, None);
        assert!(prompt.contains("IMEのリランキング"));
        assert!(prompt.contains("best_index"));
        assert!(!prompt.contains("直前の文脈"));
    }

    #[test]
    fn test_build_system_prompt_with_context() {
        let prompt = build_system_prompt(Some("明日の"), None);
        assert!(prompt.contains("直前の文脈: 明日の"));
    }

    #[test]
    fn test_build_system_prompt_with_right_context() {
        let prompt = build_system_prompt(Some("明日の"), Some("天気"));
        assert!(prompt.contains("入力中の文字列: 明日の [ここまで入力] 天気"));
    }

    #[test]
    fn test_build_user_prompt() {
        let candidates = make_candidates();
        let prompt = build_user_prompt("てんき", &candidates);
        assert!(prompt.contains("てんき"));
        assert!(prompt.contains("1. 天気"));
        assert!(prompt.contains("2. 転機"));
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
        assert!(caps.supports_right_context);
        assert_eq!(inf.name(), "workers-ai");
    }

    mod p4_retry {
        use super::*;

        fn make_inferencer(server_uri: String) -> TestWorkersAIInferencer {
            TestWorkersAIInferencer::new(server_uri)
        }

        #[tokio::test]
        async fn test_p4_retry_429_retries_and_succeeds() {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path_regex(".*/ai/run/.*"))
                .respond_with(ResponseTemplate::new(429).set_body_string("Too Many Requests"))
                .up_to_n_times(1)
                .mount(&server)
                .await;
            Mock::given(method("POST"))
                .and(path_regex(".*/ai/run/.*"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "result": { "response": "{\"best_index\": 1, \"confidence\": 0.9}" },
                    "success": true
                })))
                .mount(&server)
                .await;

            let inferencer = make_inferencer(server.uri());
            let result = inferencer.rerank("てんき", make_candidates(), None).await;
            assert!(result.is_ok());
            assert_eq!(result.unwrap()[0].surface, "天気");
        }

        #[tokio::test]
        async fn test_p4_retry_503_retries_and_succeeds() {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path_regex(".*/ai/run/.*"))
                .respond_with(ResponseTemplate::new(503).set_body_string("Service Unavailable"))
                .up_to_n_times(1)
                .mount(&server)
                .await;
            Mock::given(method("POST"))
                .and(path_regex(".*/ai/run/.*"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "result": { "response": "{\"best_index\": 1, \"confidence\": 0.9}" },
                    "success": true
                })))
                .mount(&server)
                .await;

            let inferencer = make_inferencer(server.uri());
            let result = inferencer.rerank("てんき", make_candidates(), None).await;
            assert!(result.is_ok());
        }

        #[tokio::test]
        async fn test_p4_retry_max_retry_exceeded() {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path_regex(".*/ai/run/.*"))
                .respond_with(ResponseTemplate::new(503).set_body_string("Service Unavailable"))
                .mount(&server)
                .await;

            let inferencer = make_inferencer(server.uri());
            let result = inferencer.rerank("てんき", make_candidates(), None).await;
            assert!(matches!(result, Err(InferenceError::Upstream(_))));
        }

        #[tokio::test]
        async fn test_p4_retry_budget_exceeded() {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path_regex(".*/ai/run/.*"))
                .respond_with(
                    ResponseTemplate::new(429)
                        .set_delay(Duration::from_millis(60))
                        .set_body_string("Too Many Requests"),
                )
                .mount(&server)
                .await;

            let inferencer =
                TestWorkersAIInferencer::new(server.uri()).with_timeout(Duration::from_millis(50));
            let result = inferencer.rerank("てんき", make_candidates(), None).await;
            assert!(matches!(result, Err(InferenceError::Timeout(_))));
        }

        #[tokio::test]
        async fn test_p4_retry_400_no_retry() {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path_regex(".*/ai/run/.*"))
                .respond_with(ResponseTemplate::new(400).set_body_string("Bad Request"))
                .mount(&server)
                .await;

            let inferencer = make_inferencer(server.uri());
            let result = inferencer.rerank("てんき", make_candidates(), None).await;
            assert!(matches!(result, Err(InferenceError::Upstream(_))));
        }

        #[tokio::test]
        async fn test_p4_retry_502_retries() {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path_regex(".*/ai/run/.*"))
                .respond_with(ResponseTemplate::new(502).set_body_string("Bad Gateway"))
                .up_to_n_times(1)
                .mount(&server)
                .await;
            Mock::given(method("POST"))
                .and(path_regex(".*/ai/run/.*"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "result": { "response": "2" },
                    "success": true
                })))
                .mount(&server)
                .await;

            let inferencer = make_inferencer(server.uri());
            let result = inferencer.rerank("てんき", make_candidates(), None).await;
            assert!(result.is_ok());
        }
    }
}
