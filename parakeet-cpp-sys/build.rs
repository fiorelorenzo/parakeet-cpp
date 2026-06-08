use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let upstream = manifest.join("../vendor/parakeet.cpp").canonicalize().unwrap();
    let ggml = upstream.join("third_party/ggml");

    // --- Cross-platform, no-bash patch application (spec §5.2 / §13.6 option a) ---
    let patches_dir = upstream.join("third_party/ggml-patches");
    if patches_dir.is_dir() {
        let mut patches: Vec<PathBuf> = std::fs::read_dir(&patches_dir)
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().map_or(false, |x| x == "patch"))
            .collect();
        patches.sort();
        for p in patches {
            let already = Command::new("git")
                .args(["-C", ggml.to_str().unwrap(), "apply", "--reverse", "--check"])
                .arg(&p)
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if already {
                continue;
            }
            let status = Command::new("git")
                .args(["-C", ggml.to_str().unwrap(), "apply"])
                .arg(&p)
                .status()
                .expect("failed to spawn git apply");
            assert!(status.success(), "git apply failed for {}", p.display());
        }
    }

    let mut cfg = cmake::Config::new(&upstream);
    cfg.define("PARAKEET_SHARED", "OFF")
        .define("GGML_NATIVE", "OFF")
        .define("BUILD_SHARED_LIBS", "OFF")
        .define("PARAKEET_BUILD_CLI", "OFF")
        .define("PARAKEET_BUILD_TESTS", "OFF");

    if cfg!(feature = "metal") {
        cfg.define("PARAKEET_GGML_METAL", "ON");
    }
    if cfg!(feature = "vulkan") {
        cfg.define("PARAKEET_GGML_VULKAN", "ON");
    }
    if cfg!(feature = "cuda") {
        cfg.define("PARAKEET_GGML_CUDA", "ON");
    }
    if cfg!(feature = "hip") {
        cfg.define("PARAKEET_GGML_HIP", "ON");
    }

    let dst = cfg.build();

    println!("cargo:rustc-link-search=native={}/lib", dst.display());
    println!("cargo:rustc-link-search=native={}/build", dst.display());
    println!("cargo:rustc-link-lib=static=parakeet");
    for lib in ["ggml", "ggml-base", "ggml-cpu", "ggml-metal", "ggml-blas"] {
        println!("cargo:rustc-link-lib=static={lib}");
    }
    println!("cargo:rustc-link-lib=c++");
    if cfg!(target_os = "macos") {
        for fw in ["Metal", "MetalKit", "Foundation", "Accelerate", "CoreFoundation"] {
            println!("cargo:rustc-link-lib=framework={fw}");
        }
    }

    let include = upstream.join("include");
    let bindings = bindgen::Builder::default()
        .header("wrapper.h")
        .clang_arg(format!("-I{}", include.display()))
        .allowlist_function("parakeet_capi_.*")
        .allowlist_type("parakeet_.*")
        .generate()
        .expect("bindgen failed");
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings.write_to_file(out.join("bindings.rs")).unwrap();

    println!("cargo:rerun-if-changed=wrapper.h");
    println!("cargo:rerun-if-changed=build.rs");
}
