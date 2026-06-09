use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    // dunce::canonicalize avoids the Windows `\\?\` verbatim-path prefix that
    // breaks `git apply` (and some cmake paths); on Unix it matches canonicalize.
    let upstream = dunce::canonicalize(manifest.join("../vendor/parakeet.cpp"))
        .expect("vendor/parakeet.cpp not found — run: git submodule update --init --recursive");
    let ggml = upstream.join("third_party/ggml");

    // --- Cross-platform, no-bash patch application (spec §5.2 / §13.6 option a) ---
    let patches_dir = upstream.join("third_party/ggml-patches");
    if patches_dir.is_dir() {
        let mut patches: Vec<PathBuf> = std::fs::read_dir(&patches_dir)
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "patch"))
            .collect();
        patches.sort();
        for p in patches {
            let already = Command::new("git")
                .args([
                    "-C",
                    ggml.to_str().unwrap(),
                    "apply",
                    "--reverse",
                    "--check",
                ])
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

    if cfg!(target_os = "macos") || cfg!(feature = "metal") {
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

    // Static libs to link, in dependency order (parakeet depends on ggml).
    // Verified against vendor pin e270af7 (ggml e705c5fe). Revisit on submodule bump:
    // ggml has split/renamed backend libs across versions.
    // Search the install dir (`lib/`) + the build tree (`build/`), plus the
    // `Release/` subdirs that Windows multi-config (MSVC/VS) generators produce.
    let lib_dirs = [
        dst.join("lib"),
        dst.join("build"),
        dst.join("lib").join("Release"),
        dst.join("build").join("Release"),
    ];
    for dir in &lib_dirs {
        println!("cargo:rustc-link-search=native={}", dir.display());
    }
    let candidates = [
        "parakeet",
        "ggml",
        "ggml-base",
        "ggml-cpu",
        "ggml-metal",
        "ggml-blas",
        "ggml-vulkan",
        "ggml-cuda",
        "ggml-hip",
    ];
    for lib in candidates {
        // Unix: `lib<name>.a`; Windows MSVC: `<name>.lib`.
        let found = lib_dirs.iter().any(|d| {
            d.join(format!("lib{lib}.a")).exists() || d.join(format!("{lib}.lib")).exists()
        });
        if found {
            println!("cargo:rustc-link-lib=static={lib}");
        }
    }

    // C++ standard library: libc++ on macOS (clang), libstdc++ on Linux (gcc).
    // MSVC links its C++ runtime automatically, so emit nothing on Windows.
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    match target_os.as_str() {
        "macos" => println!("cargo:rustc-link-lib=c++"),
        "linux" => println!("cargo:rustc-link-lib=stdc++"),
        _ => {}
    }

    // Vulkan loader, required when the ggml-vulkan backend is statically linked.
    if cfg!(feature = "vulkan") {
        match target_os.as_str() {
            "linux" => println!("cargo:rustc-link-lib=vulkan"),
            "windows" => {
                if let Ok(sdk) = env::var("VULKAN_SDK") {
                    println!("cargo:rustc-link-search=native={sdk}/Lib");
                }
                println!("cargo:rustc-link-lib=vulkan-1");
            }
            _ => {}
        }
    }

    if cfg!(target_os = "macos") {
        for fw in [
            "Metal",
            "MetalKit",
            "Foundation",
            "Accelerate",
            "CoreFoundation",
        ] {
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
    println!("cargo:rerun-if-changed=../vendor/parakeet.cpp/include/parakeet_capi.h");
    println!("cargo:rerun-if-changed=../vendor/parakeet.cpp/CMakeLists.txt");
}
