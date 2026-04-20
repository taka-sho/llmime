//! Windows Text Services Framework (TSF) integration for llmime.
//! All TSF-specific code is gated behind #[cfg(target_os = "windows")].

pub mod tsf_adapter;
pub use tsf_adapter::TsfLiveAdapter;

pub mod field_detector;
pub use field_detector::{FieldClass, FieldDetector};

#[cfg(target_os = "windows")]
mod state;

#[cfg(target_os = "windows")]
mod key_sink;

#[cfg(target_os = "windows")]
pub mod text_store;

#[cfg(target_os = "windows")]
mod tsf;

#[cfg(target_os = "windows")]
mod dll;

#[cfg(target_os = "windows")]
pub use text_store::LlmimeTextStore;

#[cfg(target_os = "windows")]
pub use tsf::{TextInputProcessor, CLSID_LLMIME_TSF};
