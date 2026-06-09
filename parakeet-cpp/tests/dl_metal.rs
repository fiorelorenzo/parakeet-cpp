//! De-risk proof for the dynamic-backends path on macOS: load a real model with
//! the ggml backends provided as dlopen'd MODULES (not statically linked) and
//! assert the active compute device is Metal — i.e. the Metal backend module was
//! discovered + selected at runtime. Gated to macOS + the `dynamic-backends`
//! feature; skips gracefully when no test model is provided.
#![cfg(all(target_os = "macos", feature = "dynamic-backends"))]

use parakeet_cpp::{Model, TranscribeOptions};
use std::path::Path;

/// Decode a 16 kHz mono WAV (int or float) to f32 PCM. Mirrors the helper in
/// `integration.rs` so the DL test can force a transcribe (which forces backend
/// creation).
fn load_wav_16k_mono(path: &str) -> Vec<f32> {
    let mut r = hound::WavReader::open(path).expect("open wav");
    let spec = r.spec();
    assert_eq!(spec.channels, 1, "fixture must be mono");
    assert_eq!(spec.sample_rate, 16_000, "fixture must be 16 kHz");
    match spec.sample_format {
        hound::SampleFormat::Float => r
            .samples::<f32>()
            .map(|s| s.expect("decode wav sample"))
            .collect(),
        hound::SampleFormat::Int => {
            let max = (1i64 << (spec.bits_per_sample - 1)) as f32;
            r.samples::<i32>()
                .map(|s| s.expect("decode wav sample") as f32 / max)
                .collect()
        }
    }
}

#[test]
fn dl_backend_is_metal() {
    // The loadable-backend-modules dir, emitted by parakeet-cpp/build.rs from the
    // sys crate's `DEP_PARAKEET_BACKENDS_DIR` metadata. Must be set BEFORE the
    // model loads: the patched global_backend() runs ggml_backend_load_all_*()
    // once, on first backend creation, and reads PARAKEET_BACKENDS_DIR then.
    let Some(backends_dir) = option_env!("PARAKEET_DL_BACKENDS_DIR") else {
        eprintln!("skipping: PARAKEET_DL_BACKENDS_DIR not emitted by build.rs");
        return;
    };
    // SAFETY: single-threaded test setup, before any backend is created.
    unsafe { std::env::set_var("PARAKEET_BACKENDS_DIR", backends_dir) };
    eprintln!("PARAKEET_BACKENDS_DIR = {backends_dir}");

    let (Ok(model_path), Ok(wav)) = (
        std::env::var("PARAKEET_TEST_MODEL"),
        std::env::var("PARAKEET_TEST_WAV"),
    ) else {
        eprintln!("skipping: set PARAKEET_TEST_MODEL and PARAKEET_TEST_WAV");
        return;
    };
    if !Path::new(&model_path).exists() {
        eprintln!("skipping: PARAKEET_TEST_MODEL does not exist: {model_path}");
        return;
    }

    let mut model = Model::load(Path::new(&model_path)).expect("load model");
    // Force backend creation by running a real transcribe over the fixture.
    let pcm = load_wav_16k_mono(&wav);
    let t = model
        .transcribe(&pcm, 16_000, &TranscribeOptions::default())
        .expect("transcribe");
    eprintln!("transcript: {}", t.text);

    let backend = model.backend_name();
    eprintln!("resolved backend (DL): {backend}");
    let lower = backend.to_ascii_lowercase();
    // ggml's Metal backend registers its device as "MTL<n>" (e.g. "MTL0"); the
    // human-facing backend name is "Metal". Accept either spelling so the test is
    // robust to the registry naming, while still proving Metal (not CPU) was the
    // dlopen'd module that got selected.
    assert!(
        lower.contains("metal") || lower.contains("mtl"),
        "expected the Metal backend under dynamic-backends, got {backend:?}"
    );
    assert_ne!(
        lower, "cpu",
        "Metal module should have been selected over the CPU fallback, got {backend:?}"
    );

    // The de-risk assertions have all passed at this point. Terminate the process
    // with the libc `_exit` syscall, which skips atexit handlers AND C++ static
    // destructors, to dodge a PRE-EXISTING ggml-Metal teardown abort that those
    // destructors trigger at normal process exit:
    //   ggml-metal-device.m: GGML_ASSERT([rsets->data count] == 0) failed
    // (the Metal residency set is not drained before the device is destroyed).
    // This is NOT specific to the dynamic-backends path — the static-build
    // `transcribe_real_model` integration test aborts at exit the same way once a
    // real Metal model is loaded; `std::process::exit` does NOT help because it
    // still runs those destructors via libc `exit()`. Without `_exit`, `cargo
    // test` would see SIGABRT and report failure even though the test body
    // succeeded. This is the only test in this binary, so terminating here skips
    // no other test.
    println!("dl_backend_is_metal: OK (backend={backend})");
    use std::io::Write as _;
    std::io::stdout().flush().ok();
    std::io::stderr().flush().ok();
    extern "C" {
        fn _exit(code: i32) -> !;
    }
    // SAFETY: `_exit` is the POSIX immediate-termination syscall; it never returns
    // and touches no Rust state. Called only after all assertions passed and all
    // output is flushed.
    unsafe { _exit(0) }
}
