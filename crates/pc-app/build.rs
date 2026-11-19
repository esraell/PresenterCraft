//! With NDI switched on, tell the program where the NDI runtime is found when it starts: where
//! the NDI installers put it, and the SDK's own copy.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var_os("CARGO_FEATURE_NDI").is_some() && std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos")
    {
        for folder in ["/usr/local/lib", "/Library/NDI SDK for Apple/lib/macOS"] {
            println!("cargo:rustc-link-arg=-Wl,-rpath,{folder}");
        }
    }
}
