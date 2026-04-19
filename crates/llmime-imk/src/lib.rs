// llmime-imk: macOS Input Method Kit integration
//
// Architecture:
//   Objective-C (LlmimeIMController.m)
//     └── C FFI (ffi.rs)  ← llmime_imk_* extern "C" functions
//           └── session.rs  ← per-session preedit + candidate state
//                 └── candidates.rs  ← candidate list access

pub mod candidates;
pub mod ffi;
pub mod session;

pub use candidates::get_candidates;
pub use session::{session_begin, session_end, with_session, Session};
