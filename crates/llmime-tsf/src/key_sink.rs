//! ITfKeyEventSink — handles keyboard input for TSF composition.

use windows::{
    core::{implement, Result, GUID},
    Win32::{
        Foundation::{BOOL, FALSE, TRUE},
        UI::{
            Input::KeyboardAndMouse::{
                GetKeyState, VIRTUAL_KEY, VK_BACK, VK_CONTROL, VK_ESCAPE, VK_LWIN, VK_MENU,
                VK_RETURN, VK_RWIN, VK_SHIFT, VK_SPACE,
            },
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

    fn current_modifiers() -> ModifierState {
        ModifierState {
            command_or_ctrl: is_pressed(VK_CONTROL) || is_pressed(VK_LWIN) || is_pressed(VK_RWIN),
            shift: is_pressed(VK_SHIFT),
            alt: is_pressed(VK_MENU),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct ModifierState {
    command_or_ctrl: bool,
    shift: bool,
    alt: bool,
}

fn is_pressed(vk: VIRTUAL_KEY) -> bool {
    // High-order bit is set when key is currently pressed.
    (unsafe { GetKeyState(i32::from(vk.0)) }) < 0
}

fn is_force_rerank_shortcut(vk: VIRTUAL_KEY, modifiers: ModifierState) -> bool {
    vk.0 == 0x52 && modifiers.command_or_ctrl && modifiers.shift && !modifiers.alt
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
        let modifiers = Self::current_modifiers();
        if is_force_rerank_shortcut(vk, modifiers) {
            return Ok(TRUE);
        }
        if modifiers.command_or_ctrl || modifiers.alt {
            // Let host apps process copy/cut/etc. without IME interference.
            return Ok(FALSE);
        }
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
        let modifiers = Self::current_modifiers();
        let mut state = self.state.lock().unwrap();

        if is_force_rerank_shortcut(vk, modifiers) {
            state.request_force_rerank();
            log::debug!("llmime TSF: forced rerank shortcut captured");
            return Ok(TRUE);
        }
        if modifiers.command_or_ctrl || modifiers.alt {
            return Ok(FALSE);
        }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcut_requires_ctrl_or_cmd_shift_and_r() {
        let mods = ModifierState {
            command_or_ctrl: true,
            shift: true,
            alt: false,
        };
        assert!(is_force_rerank_shortcut(VIRTUAL_KEY(0x52), mods));
        assert!(!is_force_rerank_shortcut(VIRTUAL_KEY(0x51), mods));
    }

    #[test]
    fn shortcut_does_not_fire_without_shift() {
        let mods = ModifierState {
            command_or_ctrl: true,
            shift: false,
            alt: false,
        };
        assert!(!is_force_rerank_shortcut(VIRTUAL_KEY(0x52), mods));
    }

    #[test]
    fn shortcut_conflict_is_suppressed_when_alt_is_pressed() {
        let mods = ModifierState {
            command_or_ctrl: true,
            shift: true,
            alt: true,
        };
        assert!(!is_force_rerank_shortcut(VIRTUAL_KEY(0x52), mods));
    }

    #[test]
    fn shortcut_does_not_fire_without_command_or_ctrl() {
        let mods = ModifierState {
            command_or_ctrl: false,
            shift: true,
            alt: false,
        };
        assert!(!is_force_rerank_shortcut(VIRTUAL_KEY(0x52), mods));
    }
}
