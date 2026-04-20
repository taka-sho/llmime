pub mod animation_scheduler;
pub mod rerank_queue;
pub mod selection_reranker;
pub mod selection_trigger;
pub mod streaming_boundary;
pub mod update_gate;

pub use animation_scheduler::{
    AnimationCommand, AnimationPlan, AnimationScheduler, UpdateUxFeedback,
    DEFAULT_HIGHLIGHT_CONFIDENCE_DELTA,
};
pub use rerank_queue::{RerankConfig, RerankQueue, RerankRequest};
pub use selection_reranker::{build_context_from_tokens, SelectionReranker};
pub use selection_trigger::{ModifierState, SelectionRerankRequest, SelectionRerankTrigger};
pub use streaming_boundary::{BoundaryEvent, StreamingBoundaryDetector, Token};
pub use update_gate::{should_apply_update, DEFAULT_MIN_CONFIDENCE_DELTA};
