//! Windows Text Services Framework (TSF) integration for llmime.
//! All TSF-specific code is gated behind #[cfg(target_os = "windows")].

pub mod tsf_adapter;
pub use tsf_adapter::TsfLiveAdapter;
pub mod inline_popup_layer;
pub use inline_popup_layer::InlinePopupLayer;

pub mod field_detector;
pub use field_detector::{FieldClass, FieldDetector};

pub mod selection_sink;
pub use selection_sink::{SelectionEvent, SelectionSink, ShortcutModifiers};
pub mod text_replace;
pub use text_replace::{
    replace_selected_text_via_tsf, ReplaceError, ReplaceOutcome as TsfReplaceOutcome,
    TsfEditSession, TsfRangeWriter,
};

pub mod mode_indicator;
pub use mode_indicator::ModeIndicator;

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
