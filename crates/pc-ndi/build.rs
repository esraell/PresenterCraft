//! The NDI runtime is found at run time through these folders: where the NDI installers put
//! it, and the SDK's own copy. This only matters for this crate's own example and tests; an
//! app that uses the crate sets the same folders for itself.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var_os("CARGO_FEATURE_SDK").is_some() && std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos")
    {
        for folder in ["/usr/local/lib", "/Library/NDI SDK for Apple/lib/macOS"] {
            println!("cargo:rustc-link-arg=-Wl,-rpath,{folder}");
        }
    }
}
