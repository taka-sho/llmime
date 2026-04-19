//! Minimum stub implementation of ITfTextInputProcessor.

use windows::{
    core::{implement, Result, GUID},
    Win32::UI::TextServices::{ITfTextInputProcessor, ITfTextInputProcessor_Impl, ITfThreadMgr},
};

#[implement(ITfTextInputProcessor)]
pub struct TextInputProcessor;

impl ITfTextInputProcessor_Impl for TextInputProcessor_Impl {
    fn Activate(&self, ptim: Option<&ITfThreadMgr>, tid: u32) -> Result<()> {
        log::info!("llmime TSF: Activate tid={tid}");
        Ok(())
    }

    fn Deactivate(&self) -> Result<()> {
        log::info!("llmime TSF: Deactivate");
        Ok(())
    }
}

/// CLSID for the llmime TSF text service (placeholder — replace before shipping).
pub const CLSID_LLMIME_TSF: GUID = GUID::from_values(
    0xDEAD_BEEF,
    0x0000,
    0x0000,
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
);
