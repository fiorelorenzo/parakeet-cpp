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
    // Two independent patch sets, applied to two different roots:
    //   * `patches/parakeet/*`        → the parakeet.cpp submodule root (CMakeLists/src)
    //   * `<upstream>/third_party/ggml-patches/*` → the vendored ggml submodule
    // Each apply is idempotent: a `--reverse --check` succeeds only when the patch
    // is already in the tree, in which case we skip the forward apply.
    let parakeet_patches_dir = manifest.join("patches/parakeet");
    apply_patches(&parakeet_patches_dir, &upstream);
    let ggml_patches_dir = upstream.join("third_party/ggml-patches");
    apply_patches(&ggml_patches_dir, &ggml);
    // Our own (repo-tracked) ggml patches, applied to the ggml submodule root.
    // The upstream `ggml-patches` above live INSIDE the parakeet.cpp submodule
    // (not tracked by this repo); patches that must survive a clean checkout /
    // CI / `git submodule update` belong here instead.
    let our_ggml_patches_dir = manifest.join("patches/ggml");
    apply_patches(&our_ggml_patches_dir, &ggml);

    // Dynamic-backends mode (the `dynamic-backends` feature): ggml builds each
    // backend (CPU variants + GPU) as a loadable MODULE that is dlopen'd at
    // runtime, and the core (ggml/ggml-base) + parakeet build SHARED. This lets
    // the app ship a portable CPU-only core and pick up a GPU backend module
    // when present. The opposite (default) is a fully static link.
    let dl = cfg!(feature = "dynamic-backends");

    let mut cfg = cmake::Config::new(&upstream);
    cfg.define("PARAKEET_BUILD_CLI", "OFF")
        .define("PARAKEET_BUILD_TESTS", "OFF")
        // Use ggml's built-in threadpool for the CPU backend instead of OpenMP,
        // so we don't have to link libgomp (Linux) / vcomp (Windows) into the
        // Rust binary. CPU is a fallback here (GPU backends are primary).
        .define("GGML_OPENMP", "OFF");

    if dl {
        // The patch's PARAKEET_GGML_BACKEND_DL flips GGML_BACKEND_DL +
        // BUILD_SHARED_LIBS + GGML_CPU_ALL_VARIANTS on and skips forcing
        // GGML_NATIVE (a DL build must stay portable). Build parakeet shared so
        // it links against the shared core rather than absorbing it.
        cfg.define("PARAKEET_GGML_BACKEND_DL", "ON")
            .define("PARAKEET_SHARED", "ON");
        // parakeet's backend.cpp / model_loader.cpp call CPU-backend symbols
        // (ggml_backend_cpu_init / _is_cpu / _set_n_threads) directly. Under DL
        // those live in the dlopen'd CPU module, not the link-time core, so the
        // shared parakeet/final binary link must defer them to runtime resolution
        // (the loaded CPU module exports them into the global table).
        // macOS (Apple ld) syntax only. On Linux, GNU ld leaves undefined symbols
        // in a shared object to be resolved at load time by default, so no flag is
        // needed — the RTLD_GLOBAL ggml patch exposes the dlopen'd CPU module's
        // symbols at runtime. (`-undefined dynamic_lookup` is not valid GNU ld.)
        if cfg!(target_os = "macos") {
            cfg.define("CMAKE_SHARED_LINKER_FLAGS", "-Wl,-undefined,dynamic_lookup");
        }
    } else {
        cfg.define("PARAKEET_SHARED", "OFF")
            .define("GGML_NATIVE", "OFF")
            .define("BUILD_SHARED_LIBS", "OFF");
    }

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

    // Windows: the Visual Studio (multi-config) generator fails ggml-vulkan's
    // `vulkan-shaders-gen` ExternalProject sub-configure. Ninja (single-config)
    // is the generator llama.cpp uses there. Requires `ninja` + an MSVC dev
    // environment on PATH (the CI sets both up).
    if cfg!(target_os = "windows") {
        cfg.generator("Ninja");
    }

    let dst = cfg.build();

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    // Link-search dirs. cmake-rs installs to `dst/`; under DL the SHARED core
    // (libggml*.dylib / .so / .dll + import lib) lands in `lib/`, the loadable
    // backend MODULES land in `bin/`. The static build keeps everything as
    // `lib*.a` / `*.lib` across `lib/` + the build tree (+ `Release/` on the
    // Windows multi-config generators).
    let lib_dirs = [
        dst.join("lib"),
        dst.join("bin"),
        dst.join("build"),
        dst.join("lib").join("Release"),
        dst.join("bin").join("Release"),
        dst.join("build").join("Release"),
    ];
    for dir in &lib_dirs {
        println!("cargo:rustc-link-search=native={}", dir.display());
    }

    if dl {
        // Dynamic-backends: link only the SHARED core (parakeet + the ggml
        // dispatcher + ggml-base). The backends (ggml-cpu*/ggml-metal/...) are
        // loadable MODULES, dlopen'd at runtime by ggml_backend_load_all — they
        // must NOT be linked.
        for lib in ["parakeet", "ggml", "ggml-base"] {
            println!("cargo:rustc-link-lib=dylib={lib}");
        }

        // parakeet + ggml are shared, so the final binary needs them resolvable
        // at RUNTIME via an rpath to each dir that holds a produced dynamic lib
        // (libparakeet.dylib lives in `build/`, the ggml core in `lib/`, the
        // backend modules in `bin/`).
        let dylib_dirs: Vec<&PathBuf> = lib_dirs
            .iter()
            .filter(|d| d.is_dir() && dir_has_dynamic_lib(d))
            .collect();
        // Emit the rpaths for this crate's own link units. NOTE: cargo does NOT
        // propagate `rustc-link-arg` to downstream bins/tests, so dependent
        // crates that build an executable (e.g. the DL test) must re-emit these
        // from their own build script — read them from the `DEP_PARAKEET_RPATH`
        // metadata key below. (Windows has no rpath concept; the loader finds the
        // DLLs via the link-search dirs / PATH instead.)
        if target_os == "macos" || target_os == "linux" {
            for dir in &dylib_dirs {
                println!("cargo:rustc-link-arg=-Wl,-rpath,{}", dir.display());
            }
        }
        // Linux: the consumer binary links libparakeet.so, which leaves the
        // ggml-cpu symbols undefined (resolved at runtime from the dlopen'd CPU
        // module via the RTLD_GLOBAL patch). GNU ld rejects undefined shared-lib
        // symbols at exe-link time unless told to allow them. (macOS uses the
        // cmake `-undefined dynamic_lookup` flag instead.)
        if target_os == "linux" {
            println!("cargo:rustc-link-arg=-Wl,--allow-shlib-undefined");
        }
        // Export the dylib dirs as `links` metadata so dependents can re-emit the
        // rpath: `links = "parakeet"` maps `cargo:rpath=…` → `DEP_PARAKEET_RPATH`.
        let rpath = dylib_dirs
            .iter()
            .map(|d| d.display().to_string())
            .collect::<Vec<_>>()
            .join(";");
        println!("cargo:rpath={rpath}");

        // Backend MODULES live in `dst/bin`. Surface that dir so a test can point
        // PARAKEET_BACKENDS_DIR at it (the patched global_backend() honors it).
        // Exposed as a `links` metadata key (`DEP_PARAKEET_BACKENDS_DIR` for
        // dependents) and printed for build-log discoverability.
        let backends_dir = dst.join("bin");
        println!("cargo:backends_dir={}", backends_dir.display());
    } else {
        // Static link, in dependency order (parakeet depends on ggml). Verified
        // against vendor pin e270af7 (ggml e705c5fe). Revisit on submodule bump:
        // ggml has split/renamed backend libs across versions.
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
    }

    // C++ standard library: libc++ on macOS (clang), libstdc++ on Linux (gcc).
    // MSVC links its C++ runtime automatically, so emit nothing on Windows.
    match target_os.as_str() {
        "macos" => println!("cargo:rustc-link-lib=c++"),
        "linux" => println!("cargo:rustc-link-lib=stdc++"),
        _ => {}
    }

    // Vulkan loader, required when the ggml-vulkan backend is statically linked.
    // Under DL the backend is a runtime module that links its own loader, so the
    // Rust binary needs nothing.
    if cfg!(feature = "vulkan") && !dl {
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

    // Frameworks are only needed at link time for the STATIC build (the Metal
    // backend is compiled into the binary). Under DL the Metal module is a
    // separate dlopen'd MODULE that links its own frameworks, so the Rust binary
    // links none of them.
    if cfg!(target_os = "macos") && !dl {
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
    if cfg!(target_os = "windows") && !dl {
        // ggml-cpu reads the Windows registry (CPU feature detection) →
        // RegOpenKeyExA / RegQueryValueExA / RegCloseKey live in advapi32.
        println!("cargo:rustc-link-lib=advapi32");
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
    println!("cargo:rerun-if-changed=patches/parakeet");
    println!("cargo:rerun-if-changed=patches/ggml");
    println!("cargo:rerun-if-changed=../vendor/parakeet.cpp/include/parakeet_capi.h");
    println!("cargo:rerun-if-changed=../vendor/parakeet.cpp/CMakeLists.txt");
}

/// Apply every `*.patch` in `dir` (sorted) to the git tree rooted at `root`,
/// idempotently: a patch that already applies cleanly in reverse is assumed
/// present and skipped. No-op when `dir` does not exist. Uses `git apply`
/// (no bash) so it works the same on every platform.
fn apply_patches(dir: &std::path::Path, root: &std::path::Path) {
    if !dir.is_dir() {
        return;
    }
    let mut patches: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "patch"))
        .collect();
    patches.sort();
    let root = root.to_str().unwrap();
    for p in patches {
        // `--ignore-whitespace` makes apply EOL-robust: on Windows the source is
        // often checked out CRLF while the patch context is LF, which otherwise
        // fails with "patch does not apply".
        let already = Command::new("git")
            .args([
                "-C",
                root,
                "apply",
                "--reverse",
                "--check",
                "--ignore-whitespace",
            ])
            .arg(&p)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if already {
            continue;
        }
        let status = Command::new("git")
            .args(["-C", root, "apply", "--ignore-whitespace"])
            .arg(&p)
            .status()
            .expect("failed to spawn git apply");
        assert!(status.success(), "git apply failed for {}", p.display());
    }
}

/// True if `dir` holds at least one dynamic library (`.dylib` / `.so` / `.dll`).
/// Used under DL to decide which install dirs deserve a runtime rpath.
fn dir_has_dynamic_lib(dir: &std::path::Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries.filter_map(Result::ok).any(|e| {
        e.path()
            .extension()
            .and_then(|x| x.to_str())
            .is_some_and(|x| matches!(x, "dylib" | "so" | "dll"))
    })
}
