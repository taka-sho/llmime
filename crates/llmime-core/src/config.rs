use std::path::PathBuf;

#[derive(Default)]
pub struct LlmimeConfig {
    pub workers_ai_api_key: Option<String>,
    pub workers_ai_account_id: Option<String>,
    pub llm_model_path: Option<PathBuf>,
}
