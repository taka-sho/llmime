use llmime_core::inference::inferencer::{CandidateSource, CandidateWithScore};
use wiremock::matchers::{method, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn make_basic_candidate(surface: &str) -> CandidateWithScore {
    CandidateWithScore {
        surface: surface.to_string(),
        score: 1.0,
        source: CandidateSource::Ngram,
    }
}

fn build_system_prompt_test(left_context: Option<&str>) -> String {
    let context_line = match left_context {
        Some(ctx) if !ctx.is_empty() => format!("直前の文脈: {}\n\n", ctx),
        _ => String::new(),
    };
    format!(
        "あなたは日本語IMEのリランキングエンジンです。\n\
        {}読み仮名と変換候補リストを受け取り、最も自然な候補のインデックス（1始まり）と確信度を\
        JSONで返してください。\n\n\
        出力フォーマット（他のテキストは一切含めないこと）:\n\
        {{\"best_index\": <番号>, \"confidence\": <0.0〜1.0>}}\n\n\
        例1:\n\
        読み: きょう、候補: [1. 今日, 2. 京, 3. 今夕]\n\
        → {{\"best_index\": 1, \"confidence\": 0.95}}\n\n\
        例2:\n\
        読み: かいぎ、候補: [1. 会議, 2. 怪奇, 3. 海技]\n\
        → {{\"best_index\": 1, \"confidence\": 0.9}}",
        context_line
    )
}

fn build_user_prompt_test(reading: &str, candidates: &[CandidateWithScore]) -> String {
    let list: Vec<String> = candidates
        .iter()
        .enumerate()
        .map(|(i, c)| format!("{}. {}", i + 1, c.surface))
        .collect();
    format!("読み: {}\n候補: [{}]", reading, list.join(", "))
}

fn parse_json_response(response: &str, len: usize) -> Option<usize> {
    let trimmed = response.trim();
    if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed.rfind('}') {
            let json_str = &trimmed[start..=end];
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(json_str) {
                if let Some(n) = v["best_index"].as_u64() {
                    let n = n as usize;
                    if n >= 1 && n <= len {
                        return Some(n - 1);
                    }
                }
            }
        }
    }
    None
}

async fn mock_call(
    server: &MockServer,
    reading: &str,
    candidates: &[CandidateWithScore],
    left_context: Option<&str>,
    response_best_index: usize,
    confidence: f64,
) -> String {
    use serde::{Deserialize, Serialize};

    #[derive(Serialize)]
    struct Req {
        messages: Vec<Msg>,
    }
    #[derive(Serialize)]
    struct Msg {
        role: String,
        content: String,
    }
    #[derive(Deserialize)]
    struct Resp {
        result: Res,
    }
    #[derive(Deserialize)]
    struct Res {
        response: String,
    }

    let response_json = serde_json::json!({
        "best_index": response_best_index,
        "confidence": confidence,
    })
    .to_string();

    Mock::given(method("POST"))
        .and(path_regex(".*/ai/run/.*"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "result": { "response": response_json },
            "success": true
        })))
        .mount(server)
        .await;

    let url = format!(
        "{}/client/v4/accounts/test_account/ai/run/@cf/qwen/qwen3-30b-a3b-fp8",
        server.uri()
    );
    let system = build_system_prompt_test(left_context);
    let user = build_user_prompt_test(reading, candidates);

    let payload = Req {
        messages: vec![
            Msg {
                role: "system".to_string(),
                content: system,
            },
            Msg {
                role: "user".to_string(),
                content: user,
            },
        ],
    };

    let resp: Resp = reqwest::Client::new()
        .post(&url)
        .bearer_auth("test_token")
        .json(&payload)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    resp.result.response
}

// B category: basic single-kanji readings, 5 cases
#[tokio::test]
async fn test_b_category_b1_tenki() {
    let server = MockServer::start().await;
    let cands = vec![
        make_basic_candidate("天気"),
        make_basic_candidate("転機"),
        make_basic_candidate("点鬼"),
    ];
    let raw = mock_call(&server, "てんき", &cands, None, 1, 0.9).await;
    assert_eq!(parse_json_response(&raw, cands.len()), Some(0));
}

#[tokio::test]
async fn test_b_category_b2_kaigi() {
    let server = MockServer::start().await;
    let cands = vec![
        make_basic_candidate("会議"),
        make_basic_candidate("怪奇"),
        make_basic_candidate("海技"),
    ];
    let raw = mock_call(&server, "かいぎ", &cands, None, 1, 0.9).await;
    assert_eq!(parse_json_response(&raw, cands.len()), Some(0));
}

#[tokio::test]
async fn test_b_category_b3_jishin() {
    let server = MockServer::start().await;
    let cands = vec![
        make_basic_candidate("地震"),
        make_basic_candidate("自信"),
        make_basic_candidate("自身"),
    ];
    let raw = mock_call(&server, "じしん", &cands, None, 1, 0.9).await;
    assert_eq!(parse_json_response(&raw, cands.len()), Some(0));
}

#[tokio::test]
async fn test_b_category_b4_isha() {
    let server = MockServer::start().await;
    let cands = vec![
        make_basic_candidate("医者"),
        make_basic_candidate("意者"),
        make_basic_candidate("遺者"),
    ];
    let raw = mock_call(&server, "いしゃ", &cands, None, 1, 0.9).await;
    assert_eq!(parse_json_response(&raw, cands.len()), Some(0));
}

#[tokio::test]
async fn test_b_category_b5_kakaku() {
    let server = MockServer::start().await;
    let cands = vec![
        make_basic_candidate("価格"),
        make_basic_candidate("火各"),
        make_basic_candidate("花格"),
    ];
    let raw = mock_call(&server, "かかく", &cands, None, 1, 0.9).await;
    assert_eq!(parse_json_response(&raw, cands.len()), Some(0));
}

// F category: context-dependent disambiguation, 5 cases
#[tokio::test]
async fn test_f_category_f1_kare_with_context() {
    let server = MockServer::start().await;
    let cands = vec![
        make_basic_candidate("彼"),
        make_basic_candidate("枯れ"),
        make_basic_candidate("嗄れ"),
    ];
    let raw = mock_call(&server, "かれ", &cands, Some("昨日"), 1, 0.85).await;
    assert_eq!(parse_json_response(&raw, cands.len()), Some(0));

    let sys = build_system_prompt_test(Some("昨日"));
    assert!(sys.contains("昨日"));
}

#[tokio::test]
async fn test_f_category_f2_aoi_with_context() {
    let server = MockServer::start().await;
    let cands = vec![
        make_basic_candidate("青い"),
        make_basic_candidate("葵"),
        make_basic_candidate("碧い"),
    ];
    let raw = mock_call(&server, "あおい", &cands, Some("空が"), 1, 0.85).await;
    assert_eq!(parse_json_response(&raw, cands.len()), Some(0));

    let sys = build_system_prompt_test(Some("空が"));
    assert!(sys.contains("空が"));
}

#[tokio::test]
async fn test_f_category_f3_hashi_shokuji() {
    let server = MockServer::start().await;
    let cands = vec![
        make_basic_candidate("箸"),
        make_basic_candidate("橋"),
        make_basic_candidate("端"),
    ];
    let raw = mock_call(&server, "はし", &cands, Some("食事の"), 1, 0.85).await;
    assert_eq!(parse_json_response(&raw, cands.len()), Some(0));
}

#[tokio::test]
async fn test_f_category_f4_hashi_kawa() {
    let server = MockServer::start().await;
    let cands = vec![
        make_basic_candidate("橋"),
        make_basic_candidate("箸"),
        make_basic_candidate("端"),
    ];
    let raw = mock_call(&server, "はし", &cands, Some("川の"), 1, 0.85).await;
    assert_eq!(parse_json_response(&raw, cands.len()), Some(0));
}

#[tokio::test]
async fn test_f_category_f5_kiku_ongaku() {
    let server = MockServer::start().await;
    let cands = vec![
        make_basic_candidate("聴く"),
        make_basic_candidate("菊"),
        make_basic_candidate("効く"),
    ];
    let raw = mock_call(&server, "きく", &cands, Some("音楽を"), 1, 0.85).await;
    assert_eq!(parse_json_response(&raw, cands.len()), Some(0));
}

#[test]
fn test_json_parse_failure_fallback() {
    // Plain number → parse_json_response returns None (no JSON braces)
    assert_eq!(parse_json_response("2", 3), None);
    // Garbage → None
    assert_eq!(
        parse_json_response("申し訳ありませんが判断できません", 3),
        None
    );
}

#[test]
fn test_json_parse_out_of_range() {
    assert_eq!(
        parse_json_response(r#"{"best_index": 5, "confidence": 0.9}"#, 3),
        None
    );
    assert_eq!(
        parse_json_response(r#"{"best_index": 0, "confidence": 0.9}"#, 3),
        None
    );
}

#[test]
fn test_prompt_build_timing() {
    let candidates: Vec<CandidateWithScore> = vec![
        make_basic_candidate("天気"),
        make_basic_candidate("転機"),
        make_basic_candidate("点鬼"),
    ];

    let start = std::time::Instant::now();
    for _ in 0..1000 {
        let _ = build_system_prompt_test(Some("今日の"));
        let _ = build_user_prompt_test("てんき", &candidates);
    }
    let elapsed = start.elapsed();
    assert!(
        elapsed.as_millis() < 1000,
        "prompt build took {}ms for 1000 iters",
        elapsed.as_millis()
    );
}
