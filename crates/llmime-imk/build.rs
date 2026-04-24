fn main() {
    #[cfg(target_os = "macos")]
    {
        cc::Build::new()
            .file("objc/LlmimeIMController.m")
            .file("objc/imk_main.m")
            .flag("-fobjc-arc")
            .compile("llmime_imk_objc");

        println!("cargo:rustc-link-lib=framework=InputMethodKit");
        println!("cargo:rustc-link-lib=framework=AppKit");
        println!("cargo:rustc-link-lib=framework=Foundation");
        println!("cargo:rustc-link-arg=-ObjC");
        println!("cargo:rerun-if-changed=objc/LlmimeIMController.m");
        println!("cargo:rerun-if-changed=objc/imk_main.m");
        println!("cargo:rerun-if-changed=build.rs");
        println!("cargo:rerun-if-changed=Cargo.toml");
    }
}
