//! ITfKeyEventSink — handles keyboard input for TSF composition.

use windows::{
    core::{implement, Result, GUID},
    Win32::{
        Foundation::{BOOL, FALSE, TRUE},
        UI::{
            Input::KeyboardAndMouse::{VIRTUAL_KEY, VK_BACK, VK_ESCAPE, VK_RETURN, VK_SPACE},
            TextServices::{ITfContext, ITfKeyEventSink, ITfKeyEventSink_Impl},
        },
    },
};

use crate::state::SharedState;

#[implement(ITfKeyEventSink)]
pub struct KeyEventSink {
    state: SharedState,
}

impl KeyEventSink {
    pub fn new(state: SharedState) -> Self {
        Self { state }
    }

    fn is_handled_key(vk: VIRTUAL_KEY) -> bool {
        // Handle printable ASCII, backspace, enter, escape, space
        let c = vk.0;
        (0x41..=0x5A).contains(&c)  // A–Z
            || (0x30..=0x39).contains(&c)  // 0–9
            || c == VK_BACK.0
            || c == VK_RETURN.0
            || c == VK_ESCAPE.0
            || c == VK_SPACE.0
    }
}

impl ITfKeyEventSink_Impl for KeyEventSink_Impl {
    fn OnSetFocus(&self, _fforeground: BOOL) -> Result<()> {
        Ok(())
    }

    fn OnTestKeyDown(
        &self,
        _pic: Option<&ITfContext>,
        wparam: windows::Win32::Foundation::WPARAM,
        _lparam: windows::Win32::Foundation::LPARAM,
    ) -> Result<BOOL> {
        let vk = VIRTUAL_KEY(wparam.0 as u16);
        if Self::is_handled_key(vk) {
            let state = self.state.lock().unwrap();
            // Consume key if composing, or if it would start composition (a-z)
            if state.is_composing() || (0x41..=0x5A).contains(&vk.0) {
                return Ok(TRUE);
            }
        }
        Ok(FALSE)
    }

    fn OnTestKeyUp(
        &self,
        _pic: Option<&ITfContext>,
        _wparam: windows::Win32::Foundation::WPARAM,
        _lparam: windows::Win32::Foundation::LPARAM,
    ) -> Result<BOOL> {
        Ok(FALSE)
    }

    fn OnKeyDown(
        &self,
        _pic: Option<&ITfContext>,
        wparam: windows::Win32::Foundation::WPARAM,
        _lparam: windows::Win32::Foundation::LPARAM,
    ) -> Result<BOOL> {
        let vk = VIRTUAL_KEY(wparam.0 as u16);
        let mut state = self.state.lock().unwrap();

        match vk {
            VK_BACK => {
                if state.is_composing() {
                    state.backspace();
                    log::debug!("llmime TSF: backspace preedit={:?}", state.preedit);
                    return Ok(TRUE);
                }
            }
            VK_ESCAPE => {
                if state.is_composing() {
                    state.cancel();
                    log::debug!("llmime TSF: composition cancelled");
                    return Ok(TRUE);
                }
            }
            VK_RETURN | VK_SPACE => {
                if state.is_composing() {
                    let committed = state.commit();
                    log::debug!("llmime TSF: committed {} chars", committed.len());
                    return Ok(TRUE);
                }
            }
            _ => {
                // A–Z: append lowercase char to preedit
                let c = vk.0;
                if (0x41..=0x5A).contains(&c) {
                    // VK_A=0x41 → 'a'=0x61
                    let ch = (c + 0x20) as u16;
                    state.append_char(ch);
                    log::debug!("llmime TSF: preedit len={}", state.preedit.len());
                    return Ok(TRUE);
                }
            }
        }

        Ok(FALSE)
    }

    fn OnKeyUp(
        &self,
        _pic: Option<&ITfContext>,
        _wparam: windows::Win32::Foundation::WPARAM,
        _lparam: windows::Win32::Foundation::LPARAM,
    ) -> Result<BOOL> {
        Ok(FALSE)
    }

    fn OnPreservedKey(&self, _pic: Option<&ITfContext>, _rguid: *const GUID) -> Result<BOOL> {
        Ok(FALSE)
    }
}
