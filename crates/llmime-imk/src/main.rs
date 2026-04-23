// Force the Rust FFI symbols (called by LlmimeIMController via -ObjC) into the binary.
// Without this, the lib's rlib is not linked and the symbols are absent at link time.
#[cfg(target_os = "macos")]
#[used]
static _LINK_FFI: unsafe extern "C" fn(u64) = llmime_imk::ffi::llmime_imk_session_begin;

#[cfg(target_os = "macos")]
extern "C" {
    fn llmime_imk_run_main();
}

#[cfg(target_os = "macos")]
#[link(name = "objc")]
extern "C" {}

#[cfg(target_os = "macos")]
#[link(name = "InputMethodKit", kind = "framework")]
extern "C" {}

#[cfg(target_os = "macos")]
#[link(name = "AppKit", kind = "framework")]
extern "C" {}

#[cfg(target_os = "macos")]
#[link(name = "Foundation", kind = "framework")]
extern "C" {}

fn main() {
    #[cfg(target_os = "macos")]
    unsafe {
        llmime_imk_run_main();
    }
    #[cfg(not(target_os = "macos"))]
    {
        eprintln!("llmime: macOS only");
        std::process::exit(1);
    }
}
