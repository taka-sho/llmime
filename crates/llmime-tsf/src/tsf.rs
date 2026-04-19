//! ITfTextInputProcessor implementation — lifecycle and sink registration.

use windows::{
    core::{implement, Interface, Result, GUID},
    Win32::UI::TextServices::{
        ITfKeystrokeMgr, ITfTextInputProcessor, ITfTextInputProcessor_Impl, ITfThreadMgr,
    },
};

use crate::key_sink::KeyEventSink;
use crate::state::new_shared_state;

#[implement(ITfTextInputProcessor)]
pub struct TextInputProcessor {
    state: crate::state::SharedState,
    /// Cookie returned by AdviseKeyEventSink; used to unregister in Deactivate.
    keystroke_cookie: std::cell::Cell<u32>,
}

impl Default for TextInputProcessor {
    fn default() -> Self {
        Self {
            state: new_shared_state(),
            keystroke_cookie: std::cell::Cell::new(0),
        }
    }
}

impl ITfTextInputProcessor_Impl for TextInputProcessor_Impl {
    fn Activate(&self, ptim: Option<&ITfThreadMgr>, tid: u32) -> Result<()> {
        log::info!("llmime TSF: Activate tid={tid}");

        if let Some(tim) = ptim {
            let keystroke_mgr: ITfKeystrokeMgr = tim.cast()?;
            let sink: crate::key_sink::ITfKeyEventSink_Impl_Ref =
                KeyEventSink::new(self.state.clone()).into();
            let sink_itf = sink.cast::<windows::Win32::UI::TextServices::ITfKeyEventSink>()?;

            let cookie = unsafe {
                keystroke_mgr.AdviseKeyEventSink(
                    tid,
                    &sink_itf,
                    windows::Win32::Foundation::TRUE,
                )?
            };
            // AdviseKeyEventSink doesn't return a cookie in some windows-rs versions;
            // store tid as a handle for Deactivate.
            let _ = cookie;
            self.keystroke_cookie.set(tid);
        }

        Ok(())
    }

    fn Deactivate(&self) -> Result<()> {
        log::info!("llmime TSF: Deactivate");
        // Unregistration would require storing the ITfKeystrokeMgr reference.
        // For this skeleton, cancelling composition is sufficient.
        self.state.lock().unwrap().cancel();
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
