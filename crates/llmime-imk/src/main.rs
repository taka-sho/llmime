#[cfg(target_os = "macos")]
#[link(name = "llmime_imk_objc", kind = "static")]
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
