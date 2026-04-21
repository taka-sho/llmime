use std::path::{Path, PathBuf};

#[cfg(feature = "local-llm")]
use std::sync::Arc;

use async_trait::async_trait;

#[cfg(feature = "local-llm")]
use llama_cpp_rs::{standard_sampler::StandardSampler, LlamaModel, LlamaParams, SessionParams};

use crate::inference::{
    capabilities::InferencerCapabilities,
    error::InferenceError,
    inferencer::{CandidateWithScore, Inferencer},
    memory_estimator::{check_memory_for_model, show_memory_warning_dialog, MemoryCheckResult},
};

#[cfg(any(feature = "local-llm", test))]
use crate::inference::inferencer::CandidateSource;

pub struct LocalLlmInferencer {
    model_path: Option<PathBuf>,
    #[cfg(feature = "local-llm")]
    model: Option<Arc<LlamaModel>>,
}

#[cfg(feature = "local-llm")]
impl LocalLlmInferencer {
    pub fn new_unavailable() -> Self {
        Self {
            model_path: None,
            model: None,
        }
    }

    pub fn new(path: &Path) -> Result<Self, InferenceError> {
        match check_memory_for_model(path) {
            MemoryCheckResult::Insufficient {
                available_gb,
                required_gb,
            } => {
                let proceed = show_memory_warning_dialog(available_gb, required_gb);
                if !proceed {
                    return Err(InferenceError::Unavailable(format!(
                        "user cancelled: insufficient RAM ({:.1} GB available, {:.1} GB required)",
                        available_gb, required_gb
                    )));
                }
            }
            MemoryCheckResult::Warning {
                available_gb,
                required_gb,
            } => {
                eprintln!(
                    "INFO: RAM margin thin ({:.1} GB available, {:.1} GB required). Proceeding.",
                    available_gb, required_gb
                );
            }
            MemoryCheckResult::Ok => {}
        }
        let model = LlamaModel::load_from_file(path, LlamaParams::default())
            .map_err(|e| InferenceError::Unavailable(format!("model load failed: {e}")))?;
        Ok(Self {
            model_path: Some(path.to_path_buf()),
            model: Some(Arc::new(model)),
        })
    }
}

#[cfg(not(feature = "local-llm"))]
impl LocalLlmInferencer {
    /// Creates a stub inferencer that always returns `Unavailable`.
    pub fn new_unavailable() -> Self {
        Self { model_path: None }
    }

    /// In stub mode (no `local-llm` feature), validates that the path exists.
    /// Returns `Err` for nonexistent paths; `Ok` otherwise (rerank returns `Unavailable`).
    pub fn new(path: &Path) -> Result<Self, InferenceError> {
        if !path.exists() {
            return Err(InferenceError::Unavailable(format!(
                "model not found: {}",
                path.display()
            )));
        }
        Ok(Self {
            model_path: Some(path.to_path_buf()),
        })
    }
}

#[cfg(any(feature = "local-llm", test))]
fn build_prompt(
    reading: &str,
    candidates: &[CandidateWithScore],
    left_context: Option<&str>,
    right_context: Option<&str>,
) -> String {
    let candidate_list = candidates
        .iter()
        .enumerate()
        .map(|(i, c)| format!("{}: {}", i + 1, c.surface))
        .collect::<Vec<_>>()
        .join(", ");

    let context_hint = match (left_context, right_context) {
        (Some(l), Some(r)) => format!("\n文脈: {}[変換]{}", l, r),
        (Some(l), None) => format!("\n文脈: {}[変換]", l),
        (None, Some(r)) => format!("\n文脈: [変換]{}", r),
        (None, None) => String::new(),
    };

    format!(
        "以下の日本語入力読みに対して、最も適切な変換候補を選んでください。{}\n読み: {}\n候補: {}\n最適な候補の番号を答えてください:",
        context_hint, reading, candidate_list
    )
}

#[cfg(any(feature = "local-llm", test))]
fn apply_rerank_response(
    response: &str,
    mut candidates: Vec<CandidateWithScore>,
) -> Vec<CandidateWithScore> {
    if candidates.is_empty() {
        return candidates;
    }
    // Find the first digit in the response and use it as a 1-based index.
    if let Some(idx) = response
        .chars()
        .find(|c| c.is_ascii_digit())
        .and_then(|c| c.to_digit(10))
    {
        let idx = (idx as usize).saturating_sub(1);
        if idx < candidates.len() {
            let selected = candidates.remove(idx);
            candidates.insert(
                0,
                CandidateWithScore {
                    source: CandidateSource::LocalLlm,
                    score: selected.score + 1.0,
                    ..selected
                },
            );
            return candidates;
        }
    }
    candidates
}

#[cfg(feature = "local-llm")]
async fn run_inference(model: Arc<LlamaModel>, prompt: String) -> Result<String, InferenceError> {
    tokio::task::spawn_blocking(move || {
        let mut session = model
            .create_session(SessionParams::default())
            .map_err(|e| InferenceError::Unavailable(e.to_string()))?;
        session
            .advance_context(prompt.as_bytes())
            .map_err(|e| InferenceError::Unavailable(e.to_string()))?;
        let output = session
            .start_completing_with(StandardSampler::default(), 16)
            .map_err(|e| InferenceError::Unavailable(e.to_string()))?
            .into_string();
        Ok(output)
    })
    .await
    .map_err(|e| InferenceError::Unavailable(e.to_string()))?
}

#[async_trait]
impl Inferencer for LocalLlmInferencer {
    fn name(&self) -> &'static str {
        "local-llm"
    }

    fn capabilities(&self) -> InferencerCapabilities {
        InferencerCapabilities {
            supports_rerank: true,
            supports_right_context: cfg!(feature = "local-llm"),
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
        candidates: Vec<CandidateWithScore>,
        left_context: Option<&str>,
        right_context: Option<&str>,
    ) -> Result<Vec<CandidateWithScore>, InferenceError> {
        #[cfg(feature = "local-llm")]
        {
            let model = self
                .model
                .as_ref()
                .ok_or_else(|| InferenceError::Unavailable("no model loaded".to_string()))?;
            let prompt = build_prompt(reading, &candidates, left_context, right_context);
            let response = run_inference(Arc::clone(model), prompt).await?;
            return Ok(apply_rerank_response(&response, candidates));
        }
        #[allow(unused_variables, unreachable_code)]
        {
            let _ = (reading, left_context, right_context);
            if self.model_path.is_none() {
                return Err(InferenceError::Unavailable("no model path".to_string()));
            }
            Ok(candidates)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference::inferencer::CandidateSource;

    fn make_candidates() -> Vec<CandidateWithScore> {
        vec![
            CandidateWithScore {
                surface: "東京".to_string(),
                score: 1.0,
                source: CandidateSource::Ngram,
            },
            CandidateWithScore {
                surface: "東峡".to_string(),
                score: 0.5,
                source: CandidateSource::Ngram,
            },
            CandidateWithScore {
                surface: "投京".to_string(),
                score: 0.3,
                source: CandidateSource::Ngram,
            },
        ]
    }

    #[tokio::test]
    async fn no_model_path_returns_unavailable() {
        let inf = LocalLlmInferencer::new_unavailable();
        let result = inf.rerank("とうきょう", make_candidates(), None).await;
        assert!(matches!(result, Err(InferenceError::Unavailable(_))));
    }

    #[test]
    fn nonexistent_path_returns_err() {
        let result = LocalLlmInferencer::new(Path::new("/nonexistent/model.gguf"));
        assert!(result.is_err());
    }

    #[test]
    fn capabilities_name() {
        let inf = LocalLlmInferencer::new_unavailable();
        assert_eq!(inf.name(), "local-llm");
        assert!(inf.capabilities().supports_rerank);
    }

    #[test]
    fn apply_rerank_response_first_candidate() {
        let candidates = make_candidates();
        let result = apply_rerank_response("1", candidates);
        assert_eq!(result[0].surface, "東京");
        assert!(matches!(result[0].source, CandidateSource::LocalLlm));
    }

    #[test]
    fn apply_rerank_response_second_candidate() {
        let candidates = make_candidates();
        let result = apply_rerank_response("2", candidates);
        assert_eq!(result[0].surface, "東峡");
    }

    #[test]
    fn apply_rerank_response_out_of_range_returns_unchanged() {
        let candidates = make_candidates();
        let result = apply_rerank_response("9", candidates.clone());
        assert_eq!(result[0].surface, "東京");
    }

    #[test]
    fn apply_rerank_response_no_digit_returns_unchanged() {
        let candidates = make_candidates();
        let result = apply_rerank_response("no digit here", candidates);
        assert_eq!(result[0].surface, "東京");
    }

    #[test]
    fn build_prompt_includes_reading_and_candidates() {
        let candidates = make_candidates();
        let prompt = build_prompt("とうきょう", &candidates, None, None);
        assert!(prompt.contains("とうきょう"));
        assert!(prompt.contains("東京"));
        assert!(prompt.contains("1:"));
    }

    #[test]
    fn build_prompt_with_context() {
        let candidates = make_candidates();
        let prompt = build_prompt("とうきょう", &candidates, Some("私は"), Some("に住む"));
        assert!(prompt.contains("私は"));
        assert!(prompt.contains("に住む"));
    }
}
