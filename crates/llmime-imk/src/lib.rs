// llmime-imk: macOS Input Method Kit integration
//
// Architecture:
//   Objective-C (LlmimeIMController.m)
//     └── C FFI (ffi.rs)  ← llmime_imk_* extern "C" functions
//           └── session.rs  ← per-session preedit + candidate state
//                 └── candidates.rs  ← candidate list access

pub mod candidates;
pub mod ffi;
pub mod field_detector;
pub mod imk_adapter;
pub mod mode_indicator;
pub mod selection_watcher;
pub mod session;

pub use candidates::get_candidates;
pub use field_detector::{FieldClass, FieldDetector};
pub use imk_adapter::ImkLiveAdapter;
pub use mode_indicator::ModeIndicator;
pub use selection_watcher::{NSRange, SelectionEvent, SelectionWatcher};
pub use session::{session_begin, session_end, with_session, Session};
