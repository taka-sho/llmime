// C-callable FFI for the macOS settings UI (SwiftUI layer).
// All functions are no_mangle and use C types for Swift interop.
// Safety invariants match those in ffi.rs: callers ensure valid pointers.
#![allow(clippy::missing_safety_doc)]

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_uint};
use std::path::PathBuf;

use llmime_core::config::SettingsSnapshot;
use llmime_core::inference::{scan_local_models, ModelDownloadManager, DefaultModelConfig};

// ─── Settings read/write ───────────────────────────────────────────────────

/// Load current settings as a JSON string into `buf`.
/// Returns 0 on success, -1 on error.
#[no_mangle]
pub unsafe extern "C" fn llmime_settings_load(buf: *mut c_char, buf_len: c_uint) -> c_int {
    if buf.is_null() || buf_len == 0 {
        return -1;
    }
    let snap = match SettingsSnapshot::load() {
        Ok(s) => s,
        Err(_) => return -1,
    };
    let json = match serde_json::to_string(&snap) {
        Ok(j) => j,
        Err(_) => return -1,
    };
    write_str_to_buf(&json, buf, buf_len as usize)
}

/// Save settings from a JSON string. Returns 0 on success, -1 on error.
#[no_mangle]
pub unsafe extern "C" fn llmime_settings_save(json: *const c_char) -> c_int {
    if json.is_null() {
        return -1;
    }
    let s = unsafe { CStr::from_ptr(json) };
    let Ok(text) = s.to_str() else { return -1 };
    let snap: SettingsSnapshot = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(_) => return -1,
    };
    match snap.save() {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

// ─── Model scanning ───────────────────────────────────────────────────────

/// Scan local LM Studio/Jan directories for GGUF models.
/// For each found model, `callback` is invoked with (path_utf8, filename_utf8, size_bytes).
/// Returns number of models found, or -1 on error.
#[no_mangle]
pub unsafe extern "C" fn llmime_scan_local_models(
    callback: Option<unsafe extern "C" fn(*const c_char, *const c_char, u64)>,
) -> c_int {
    let cb = match callback {
        Some(f) => f,
        None => return -1,
    };
    let candidates = scan_local_models(&[]);
    let count = candidates.len() as c_int;
    for c in candidates {
        let path_cs = CString::new(c.path.to_string_lossy().as_ref()).unwrap_or_default();
        let name_cs = CString::new(c.filename.as_str()).unwrap_or_default();
        unsafe { cb(path_cs.as_ptr(), name_cs.as_ptr(), c.size_bytes) };
    }
    count
}

// ─── Model download ───────────────────────────────────────────────────────

/// Start downloading the default Qwen model asynchronously.
/// `progress_cb` is called periodically with (downloaded_bytes, total_bytes_or_0).
/// `done_cb` is called on completion: done_cb(dest_path_utf8, null_on_success_or_error_msg).
/// Returns 0 if download task was spawned, -1 if tokio runtime unavailable.
#[no_mangle]
pub unsafe extern "C" fn llmime_download_default_model(
    progress_cb: Option<unsafe extern "C" fn(u64, u64)>,
    done_cb: Option<unsafe extern "C" fn(*const c_char, *const c_char)>,
) -> c_int {
    let progress_cb = progress_cb;
    let done_cb = done_cb;

    let rt = match tokio::runtime::Runtime::new() {
        Ok(r) => r,
        Err(_) => return -1,
    };

    std::thread::spawn(move || {
        rt.block_on(async move {
            let models_dir = llmime_core::inference::default_models_dir();
            let mgr = ModelDownloadManager::new(models_dir);
            let cfg = DefaultModelConfig::default();

            let tx_opt = if progress_cb.is_some() {
                let (tx, mut rx) = tokio::sync::mpsc::channel::<llmime_core::inference::DownloadProgress>(32);
                let pcb = progress_cb.unwrap();
                tokio::spawn(async move {
                    while let Some(p) = rx.recv().await {
                        let total = p.total_bytes.unwrap_or(0);
                        unsafe { pcb(p.downloaded_bytes, total) };
                    }
                });
                Some(tx)
            } else {
                None
            };

            match mgr.download_default(&cfg, tx_opt).await {
                Ok(path) => {
                    if let Some(dcb) = done_cb {
                        let path_cs = CString::new(path.to_string_lossy().as_ref()).unwrap_or_default();
                        let null_err: *const c_char = std::ptr::null();
                        unsafe { dcb(path_cs.as_ptr(), null_err) };
                    }
                }
                Err(e) => {
                    if let Some(dcb) = done_cb {
                        let empty = CString::new("").unwrap_or_default();
                        let err_cs = CString::new(e.to_string()).unwrap_or_default();
                        unsafe { dcb(empty.as_ptr(), err_cs.as_ptr()) };
                    }
                }
            }
        });
    });

    0
}

// ─── RAM estimation ───────────────────────────────────────────────────────

/// Estimate RAM usage in MiB for a GGUF model at `path`.
/// Uses a heuristic: file_size_bytes * 1.25 / 1024^2.
/// Returns 0 if path is invalid or file not found.
#[no_mangle]
pub unsafe extern "C" fn llmime_estimate_model_ram_mb(path: *const c_char) -> u64 {
    if path.is_null() {
        return 0;
    }
    let s = unsafe { CStr::from_ptr(path) };
    let Ok(p) = s.to_str() else { return 0 };
    let meta = std::fs::metadata(PathBuf::from(p)).ok()?;
    (meta.len() as f64 * 1.25 / (1024.0 * 1024.0)) as u64
}

// ─── Helpers ──────────────────────────────────────────────────────────────

fn write_str_to_buf(s: &str, buf: *mut c_char, buf_len: usize) -> c_int {
    let cs = CString::new(s).unwrap_or_default();
    let bytes = cs.as_bytes_with_nul();
    let copy_len = bytes.len().min(buf_len);
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr() as *const c_char, buf, copy_len);
        *buf.add(copy_len - 1) = 0;
    }
    0
}
