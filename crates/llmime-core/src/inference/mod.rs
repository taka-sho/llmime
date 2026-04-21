pub mod builder;
pub mod capabilities;
pub mod cost_guard;
pub mod cost_monitor;
pub mod dispatcher;
pub mod error;
pub mod fallback_chain;
pub mod inferencer;
pub mod latency_tracker;
pub mod local_llm;
pub mod local_ngram;
pub mod mode;
pub mod model_downloader;
pub mod model_scanner;
pub mod override_manager;
pub mod retry;
pub mod token_counter;
pub mod warmup;
pub mod workers_ai;

pub use builder::default_fallback_chain;
pub use capabilities::InferencerCapabilities;
pub use cost_guard::CostGuard;
pub use cost_monitor::CostMonitor;
pub use dispatcher::Dispatcher;
pub use error::{CostCapKind, InferenceError};
pub use fallback_chain::FallbackChain;
pub use inferencer::{CandidateSource, CandidateWithScore, DynInferencer, Inferencer};
pub use latency_tracker::LatencyTracker;
pub use local_llm::LocalLlmInferencer;
pub use local_ngram::LocalNgramInferencer;
pub use mode::{InputMode, ModeManager};
pub use model_downloader::{
    default_models_dir, resolve_model_path, DefaultModelConfig, DownloadError, DownloadProgress,
    ModelDownloadManager, DEFAULT_MODEL_NAME, DEFAULT_MODEL_SHA256, DEFAULT_MODEL_SIZE_BYTES,
    DEFAULT_MODEL_URL,
};
pub use model_scanner::{scan_local_models, ModelCandidate, ModelSource};
pub use override_manager::OverrideManager;
pub use token_counter::TokenCounter;
pub use warmup::{WarmupOrchestrator, WarmupStatus};
pub use workers_ai::WorkersAIInferencer;
