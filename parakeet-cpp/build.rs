// Under the `dynamic-backends` feature, parakeet-cpp-sys builds the ggml core +
// libparakeet as shared libraries and the backends as runtime-loaded modules.
// Executables that link this crate (tests, examples, downstream bins) therefore
// reference `@rpath/libparakeet.dylib` etc. at runtime. cargo does NOT propagate
// the sys crate's `rustc-link-arg` rpaths to those downstream link units, so we
// re-emit them here from the `DEP_PARAKEET_RPATH` metadata the sys crate exports.
// No-op for the default (fully static) build, where the key is absent.
fn main() {
    if let Ok(rpath) = std::env::var("DEP_PARAKEET_RPATH") {
        let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
        if target_os == "macos" || target_os == "linux" {
            for dir in rpath.split(';').filter(|s| !s.is_empty()) {
                println!("cargo:rustc-link-arg=-Wl,-rpath,{dir}");
            }
        }
    }
    println!("cargo:rerun-if-changed=build.rs");
}
