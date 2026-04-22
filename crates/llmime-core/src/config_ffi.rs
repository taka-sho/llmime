#![allow(clippy::missing_safety_doc)]
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use crate::config::LlmimeConfig;
use crate::inference::{
    default_models_dir, DefaultModelConfig, ModelDownloadManager, ModelScanner,
};

#[derive(Debug, Clone, Copy, Default)]
struct DownloadProgressState {
    downloaded_bytes: u64,
    total_bytes: u64,
    status: c_int, // 0: idle, 1: downloading, 2: complete, -1: failed
}

fn download_state() -> &'static Mutex<DownloadProgressState> {
    static STATE: OnceLock<Mutex<DownloadProgressState>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(DownloadProgressState::default()))
}

fn write_c_string(buf: *mut c_char, buf_len: u32, value: &str) -> c_int {
    if buf.is_null() || buf_len == 0 {
        return 0;
    }
    let Ok(cs) = CString::new(value) else {
        return 0;
    };
    let bytes = cs.as_bytes_with_nul();
    let copy_len = bytes.len().min(buf_len as usize);
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr() as *const c_char, buf, copy_len);
        *buf.add(copy_len - 1) = 0;
    }
    1
}

unsafe fn ptr_to_string(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    let raw = unsafe { CStr::from_ptr(ptr) };
    raw.to_str().ok().map(ToOwned::to_owned)
}

#[no_mangle]
pub unsafe extern "C" fn llmime_config_load_json(buf: *mut c_char, buf_len: u32) -> c_int {
    let Ok(cfg) = LlmimeConfig::load_for_settings() else {
        return 0;
    };
    write_c_string(buf, buf_len, &cfg.to_settings_json())
}

#[no_mangle]
pub unsafe extern "C" fn llmime_config_save_settings(
    mode: *const c_char,
    model_path: *const c_char,
    ollama_endpoint: *const c_char,
) -> c_int {
    let Some(mode) = (unsafe { ptr_to_string(mode) }) else {
        return 0;
    };
    let model_path = unsafe { ptr_to_string(model_path) }.map(PathBuf::from);
    let endpoint = unsafe { ptr_to_string(ollama_endpoint) };

    let Ok(mut cfg) = LlmimeConfig::load_for_settings() else {
        return 0;
    };
    if cfg.apply_settings(&mode, model_path, endpoint).is_err() {
        return 0;
    }
    if cfg.save().is_err() {
        return 0;
    }
    1
}

#[no_mangle]
pub unsafe extern "C" fn llmime_config_scan_models_json(buf: *mut c_char, buf_len: u32) -> c_int {
    let scanner = ModelScanner;
    let models = scanner.scan();
    let json = serde_json::json!(models
        .iter()
        .map(|m| serde_json::json!({
            "path": m.path.display().to_string(),
            "source": m.source,
            "estimated_ram_gb": m.estimated_ram_gb
        }))
        .collect::<Vec<_>>())
    .to_string();
    write_c_string(buf, buf_len, &json)
}

#[no_mangle]
pub extern "C" fn llmime_config_download_default_model() -> c_int {
    {
        let mut state = download_state().lock().unwrap();
        if state.status == 1 {
            return 0;
        }
        *state = DownloadProgressState {
            status: 1,
            ..DownloadProgressState::default()
        };
    }

    std::thread::spawn(|| {
        let rt = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(_) => {
                let mut state = download_state().lock().unwrap();
                state.status = -1;
                return;
            }
        };

        rt.block_on(async {
            let models_dir = default_models_dir();
            let mgr = ModelDownloadManager::new(models_dir);
            let cfg = DefaultModelConfig::default();
            let (tx, mut rx) = tokio::sync::mpsc::channel(32);
            let download_task =
                tokio::spawn(async move { mgr.download_default(&cfg, Some(tx)).await });
            tokio::pin!(download_task);

            loop {
                tokio::select! {
                    maybe = rx.recv() => {
                        if let Some(p) = maybe {
                            let mut state = download_state().lock().unwrap();
                            state.downloaded_bytes = p.downloaded_bytes;
                            state.total_bytes = p.total_bytes.unwrap_or(0);
                        }
                    }
                    result = &mut download_task => {
                        let mut state = download_state().lock().unwrap();
                        state.status = if matches!(result, Ok(Ok(_))) { 2 } else { -1 };
                        break;
                    }
                }
            }
        });
    });

    1
}

#[no_mangle]
pub unsafe extern "C" fn llmime_config_poll_download_progress(
    downloaded_bytes: *mut u64,
    total_bytes: *mut u64,
    status: *mut c_int,
) -> c_int {
    if downloaded_bytes.is_null() || total_bytes.is_null() || status.is_null() {
        return 0;
    }

    let state = download_state().lock().unwrap();
    unsafe {
        *downloaded_bytes = state.downloaded_bytes;
        *total_bytes = state.total_bytes;
        *status = state.status;
    }
    1
}
