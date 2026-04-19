//! Windows Text Services Framework (TSF) integration for llmime.
//! All TSF-specific code is gated behind #[cfg(target_os = "windows")].

#[cfg(target_os = "windows")]
mod tsf;

#[cfg(target_os = "windows")]
pub use tsf::TextInputProcessor;

#[cfg(target_os = "windows")]
mod dll;
