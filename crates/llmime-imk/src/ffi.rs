// C-callable FFI functions invoked from the Objective-C layer (LlmimeIMController.m).
// Safety invariant for all pub unsafe extern "C" fns: callers must pass valid, aligned pointers
// and ensure no aliasing; null checks are performed at the start of each function.
#![allow(clippy::missing_safety_doc)]
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};

use crate::candidates::get_candidates;
use crate::session::{session_begin, session_end, with_session};

#[no_mangle]
pub extern "C" fn llmime_imk_session_begin(session_id: u64) {
    session_begin(session_id);
}

#[no_mangle]
pub extern "C" fn llmime_imk_session_end(session_id: u64) {
    session_end(session_id);
}

/// Returns 1 if the input was consumed, 0 otherwise.
#[no_mangle]
pub unsafe extern "C" fn llmime_imk_input_text(
    session_id: u64,
    utf8: *const c_char,
    _modifiers: u32,
) -> c_int {
    if utf8.is_null() {
        return 0;
    }
    let s = unsafe { CStr::from_ptr(utf8) };
    let Ok(text) = s.to_str() else { return 0 };

    match text {
        "\u{7F}" | "\u{8}" => {
            with_session(session_id, |sess| sess.pop_char());
            1
        }
        "\u{1B}" => {
            with_session(session_id, |sess| {
                sess.preedit.clear();
                sess.candidates.clear();
            });
            1
        }
        _ => {
            for ch in text.chars() {
                if !ch.is_control() {
                    with_session(session_id, |sess| sess.push_char(ch));
                }
            }
            1
        }
    }
}

#[no_mangle]
pub extern "C" fn llmime_imk_get_candidate_count(session_id: u64) -> u32 {
    get_candidates(session_id).len() as u32
}

/// Writes candidate at `index` as a UTF-8 string into `buf` (null-terminated, max `buf_len` bytes).
#[no_mangle]
pub unsafe extern "C" fn llmime_imk_get_candidate(
    session_id: u64,
    index: u32,
    buf: *mut c_char,
    buf_len: u32,
) {
    if buf.is_null() || buf_len == 0 {
        return;
    }
    let cands = get_candidates(session_id);
    let text = cands.get(index as usize).map(String::as_str).unwrap_or("");
    let cs = CString::new(text).unwrap_or_default();
    let bytes = cs.as_bytes_with_nul();
    let copy_len = bytes.len().min(buf_len as usize);
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr() as *const c_char, buf, copy_len);
        // Ensure null-termination if buffer was too small.
        *buf.add(copy_len - 1) = 0;
    }
}

#[no_mangle]
pub unsafe extern "C" fn llmime_imk_candidate_selected(session_id: u64, utf8: *const c_char) {
    if utf8.is_null() {
        return;
    }
    let s = unsafe { CStr::from_ptr(utf8) };
    let Ok(text) = s.to_str() else { return };
    with_session(session_id, |sess| sess.commit(text));
}

#[no_mangle]
pub unsafe extern "C" fn llmime_imk_candidate_selection_changed(
    session_id: u64,
    utf8: *const c_char,
) {
    if utf8.is_null() {
        return;
    }
    let s = unsafe { CStr::from_ptr(utf8) };
    let Ok(text) = s.to_str() else { return };
    log::debug!("session {}: selection changed to {:?}", session_id, text);
}

/// Writes the current preedit string into `buf` (null-terminated, max `buf_len` bytes).
#[no_mangle]
pub unsafe extern "C" fn llmime_imk_get_preedit(
    session_id: u64,
    buf: *mut c_char,
    buf_len: u32,
) {
    if buf.is_null() || buf_len == 0 {
        return;
    }
    let preedit = with_session(session_id, |sess| sess.preedit.clone()).unwrap_or_default();
    let cs = CString::new(preedit).unwrap_or_default();
    let bytes = cs.as_bytes_with_nul();
    let copy_len = bytes.len().min(buf_len as usize);
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr() as *const c_char, buf, copy_len);
        *buf.add(copy_len - 1) = 0;
    }
}

/// Commits the first candidate (or preedit as fallback) and writes committed text into `buf`.
/// Returns 1 if something was committed, 0 if preedit was empty.
#[no_mangle]
pub unsafe extern "C" fn llmime_imk_commit_first(
    session_id: u64,
    buf: *mut c_char,
    buf_len: u32,
) -> c_int {
    if buf.is_null() || buf_len == 0 {
        return 0;
    }
    let result: Option<Option<String>> = with_session(session_id, |sess| {
        if sess.preedit.is_empty() {
            return None;
        }
        let text = sess
            .candidates
            .first()
            .cloned()
            .unwrap_or_else(|| sess.preedit.clone());
        sess.commit(&text);
        Some(text)
    });
    let Some(Some(text)) = result else {
        return 0;
    };
    let cs = CString::new(text).unwrap_or_default();
    let bytes = cs.as_bytes_with_nul();
    let copy_len = bytes.len().min(buf_len as usize);
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr() as *const c_char, buf, copy_len);
        *buf.add(copy_len - 1) = 0;
    }
    1
}
