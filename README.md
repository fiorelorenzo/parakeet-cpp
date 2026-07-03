# parakeet-cpp

[![ci](https://github.com/fiorelorenzo/parakeet-cpp/actions/workflows/ci.yml/badge.svg)](https://github.com/fiorelorenzo/parakeet-cpp/actions/workflows/ci.yml)

Safe Rust bindings for [mudler/parakeet.cpp](https://github.com/mudler/parakeet.cpp) — a
ggml-based C++ port of NVIDIA's Parakeet and Nemotron ASR models. Provides offline
one-shot transcription and two streaming modes (real cache-aware streaming and
pseudo-streaming), all backed by a stable C ABI. Designed for embedding in
Rust applications that need on-device speech recognition without a Python runtime
or cloud dependency.

---

## Features

- **Offline transcription** — one-shot decode of a complete audio buffer via
  `Model::transcribe`, backed by NVIDIA's Parakeet-TDT-v3.
- **Real streaming** — true cache-aware streaming via `Model::stream_real` +
  `RealStreamSession`; only streaming-capable models (e.g. Nemotron-3.5-ASR)
  support this path; end-of-utterance events are surfaced on each `feed` call.
- **Pseudo-streaming** — compatible with any model; `Model::stream_pseudo` +
  `PseudoStreamSession` grows an internal audio buffer and re-decodes on each
  `feed`, diffing the output via longest-common-prefix to produce incremental
  deltas. O(n^2) in audio length but sufficient for dictation-length audio.
- **Unified streaming surface** — both session types implement the `StreamSession`
  trait (`feed` / `finish`), so a consumer can hold a `Box<dyn StreamSession>`
  and switch backends without changing call sites.
- **Metal acceleration** on macOS (Apple Silicon and Intel) — enabled automatically;
  no feature flag needed.
- **Vulkan / CUDA / HIP** backends wired as opt-in Cargo features for Linux and
  Windows; CPU fallback always available.
- **Safe wrapper over a documented C ABI** — ABI version is checked at `Model::load`
  time; mismatches surface as `Error::AbiMismatch` before any unsafe code runs.
- **No C++ exceptions or global state cross the boundary** — the C ABI contract
  is enforced by the upstream header.

---

## Supported platforms and backends

| Platform          | Backend        | Status                                          |
|-------------------|----------------|-------------------------------------------------|
| macOS (Apple Silicon / Intel) | Metal | Working; CI-tested on M4 macOS 15 |
| macOS             | CPU fallback   | Working                                         |
| Linux             | Vulkan (`--features vulkan`) | Wired; not yet in CI          |
| Linux             | CUDA (`--features cuda`)   | Wired; not yet in CI            |
| Linux             | CPU fallback   | Expected to work; not yet in CI                 |
| Windows           | Vulkan (`--features vulkan`) | Wired; not yet in CI          |
| Windows           | HIP (`--features hip`)     | Wired; not yet in CI            |
| Windows           | CPU fallback   | Expected to work; not yet in CI                 |

---

## Requirements

- **Rust 1.88** or later (toolchain pinned in `rust-toolchain.toml`).
- A **C/C++ toolchain** — `clang` or `gcc` + `g++`/`clang++`, whichever CMake
  finds first.
- **CMake 3.15+** — used by `build.rs` to configure and build the vendored upstream.
- **git** — `build.rs` applies ggml patches with `git apply` (no shell script
  dependency).
- **macOS:** Xcode Command Line Tools (provides Metal, Accelerate, MetalKit, and
  CoreFoundation frameworks linked automatically).
- **Linux/Windows (Vulkan):** Vulkan SDK headers and `libvulkan` available at
  link time.

---

## Installation

`parakeet-cpp` is not yet published to crates.io. Add it as a git dependency:

```toml
[dependencies]
parakeet-cpp = { git = "https://github.com/fiorelorenzo/parakeet-cpp", branch = "main" }
```

After adding the dependency, initialize the vendored submodule — **this step
is mandatory**; the build will fail with an explicit error message if it is
skipped:

```sh
git submodule update --init --recursive
```

The build script (`build.rs`) runs CMake over the vendored upstream, applies any
ggml patches automatically via `git apply`, and links the resulting static
libraries. On macOS the Metal backend is enabled without any feature flag. To
enable a GPU backend on Linux or Windows:

```sh
cargo build --features vulkan   # Vulkan (Linux / Windows)
cargo build --features cuda     # CUDA
cargo build --features hip      # ROCm / HIP
```

CPU-only builds require no flags.

---

## Quickstart

### Offline one-shot transcription

```rust
use parakeet_cpp::{Model, TranscribeOptions};
use std::path::Path;

fn main() -> Result<(), parakeet_cpp::Error> {
    // Load a GGUF model. ABI version is checked here; mismatches return an error.
    let mut model = Model::load(Path::new("./models/tdt-0.6b-v3-q8_0.gguf"))?;

    // Audio must be 16 kHz, mono, f32 PCM — see "Audio format" below.
    let pcm: Vec<f32> = load_audio_16k_mono("./audio.wav");

    let opts = TranscribeOptions {
        language: Some("en".to_string()), // or None for model default
        word_timestamps: false,           // reserved; not yet implemented
    };

    let transcript = model.transcribe(&pcm, 16_000, &opts)?;
    println!("{}", transcript.text);

    Ok(())
}
```

### Real streaming (streaming model required)

```rust
use parakeet_cpp::{Model, StreamSession, TranscribeOptions};
use std::path::Path;

fn main() -> Result<(), parakeet_cpp::Error> {
    let mut model = Model::load(
        Path::new("./models/nemotron-3.5-asr-streaming-0.6b-q5_k.gguf")
    )?;

    // Reject at runtime if the model does not support real streaming.
    assert!(model.is_streaming(), "not a streaming model");

    let opts = TranscribeOptions {
        language: Some("en".to_string()),
        word_timestamps: false,
    };

    let mut session = Box::new(model.stream_real(&opts)?);

    // Feed 500 ms chunks of 16 kHz mono f32 PCM.
    let chunk_size = 16_000 / 2;
    let pcm: Vec<f32> = load_audio_16k_mono("./audio.wav");

    for chunk in pcm.chunks(chunk_size) {
        let partial = session.feed(chunk)?;
        // `partial.delta` is the newly finalized text since the last feed.
        // `partial.text` is the full cumulative transcript so far.
        // `partial.eou` fires on end-of-utterance events (model-dependent).
        print!("{}", partial.delta);
    }

    let final_transcript = session.finish()?;
    println!("\nFinal: {}", final_transcript.text);

    Ok(())
}
```

To use pseudo-streaming instead (works with any model, including offline ones),
replace `model.stream_real(&opts)?` with
`model.stream_pseudo(16_000, opts)` — the call signature and `StreamSession`
usage are otherwise identical.

Note: some multilingual models (for example nemotron) embed language tags such
as `<it-IT>` in the output text, and the first streaming delta of an utterance
may carry a leading space. Stripping these is the consumer's responsibility.

---

## Models

Download GGUF weights from the
[mudler/parakeet-cpp-gguf](https://huggingface.co/mudler/parakeet-cpp-gguf) repository
on Hugging Face:

```sh
pip install -U "huggingface_hub[cli]"
# On macOS with Homebrew Python, `hf` may be the CLI name instead:
hf download mudler/parakeet-cpp-gguf tdt-0.6b-v3-q8_0.gguf --local-dir ./models
hf download mudler/parakeet-cpp-gguf nemotron-3.5-asr-streaming-0.6b-q5_k.gguf --local-dir ./models
```

| File | Size | Mode | Notes |
|------|------|------|-------|
| `tdt-0.6b-v3-q8_0.gguf` | ~897 MB | Offline (one-shot) | Parakeet-TDT-v3, multilingual EU, q8_0 quantization |
| `nemotron-3.5-asr-streaming-0.6b-q5_k.gguf` | ~400 MB | Real streaming | Nemotron-3.5-ASR-Streaming, multilingual, q5_k quantization |

**License note:** model weights carry their own NVIDIA license, which is separate
from and independent of this crate's MIT license. Read the model card on Hugging
Face before redistribution.

---

## The `spike` example

The workspace includes a `spike` example that exercises all three transcription
modes and computes a rough Word Error Rate (WER):

```sh
# Offline one-shot
cargo run --release --example spike -- offline \
    ./models/tdt-0.6b-v3-q8_0.gguf /path/to/audio.wav [lang]

# Real streaming (prints per-chunk latency + EOU events on stderr)
cargo run --release --example spike -- stream \
    ./models/nemotron-3.5-asr-streaming-0.6b-q5_k.gguf /path/to/audio.wav [lang]

# Batch WER over eval/audio/*.wav against eval/refs/<stem>.txt references
cargo run --release --example spike -- wer \
    ./models/tdt-0.6b-v3-q8_0.gguf [lang]
```

The `offline` subcommand prints `is_streaming = false|true` and then the
transcript. The `stream` subcommand prints per-feed latency and EOU status on
stderr, followed by CRITERION 1 (streaming continuity) and CRITERION 3 (latency)
summaries on stdout. The `wer` subcommand prints per-clip WER and a mean WER
over the eval corpus.

Measured latency on Apple M4 macOS 15, Metal backend, 500 ms chunks:
max feed latency ~93 ms (well under the 400 ms target).

---

## Audio format

All transcription entry points expect **16 kHz, mono, f32 PCM**. The C layer
accepts an arbitrary `sample_rate` argument and linearly resamples if
`sample_rate != 16000`, but passing pre-resampled 16 kHz audio is recommended
for best results. There is no built-in channel downmix; stereo input must be
downmixed to mono before calling the Rust API.

---

## Status and known limitations

This crate tracks a young, fast-moving upstream. Pin a specific submodule
revision (as this workspace does) and re-verify after any bump.

- **Upstream maturity.** `mudler/parakeet.cpp` was approximately ten days old
  at the time the bindings were written. The bus factor is low and the API may
  change. The current submodule pin is commit
  `e270af73b94c9a5c37ec516230219ed4580e1db6` (2026-06-08).

- **SIGABRT at process exit on macOS.** The process aborts (exit code 134) after
  all transcription output is flushed, triggered by a
  `GGML_ASSERT([rsets->data count] == 0)` assertion in the ggml Metal residency-set
  cleanup path. This does not affect correctness — the full transcript is produced
  and returned before the abort. Long-lived host applications that hold the model
  for the full process lifetime will not observe this; it only manifests when the
  process exits after having run Metal inference. This is a known upstream ggml
  issue, not a binding bug.

- **Language tag leakage in streaming output.** Some multilingual models (for
  example nemotron) embed language tags such as `<it-IT>` in the text stream at
  sentence boundaries, and may prefix the first delta of an utterance with a
  leading space. The binding does not strip these; stripping is the consumer's
  responsibility.

- **`word_timestamps` is not yet implemented.** The `TranscribeOptions::word_timestamps`
  field is reserved for future use. `Model::transcribe` always returns an empty
  `words` vec regardless of this flag. The underlying C ABI does expose JSON
  endpoints with per-word timestamps; surfacing them in the safe wrapper is a
  planned future addition.

- **macOS-first today.** The Metal backend is CI-tested. Linux and Windows
  backends (Vulkan, CUDA, HIP) are wired via feature flags and should build, but
  have not been exercised in CI.

- **EOU events depend on the model.** `RealStreamSession` surfaces `Partial::eou`
  when the model fires an end-of-utterance event. In testing with the nemotron
  model on synthetic TTS audio, EOU events did not fire even with multi-second
  silences. Real continuous speech may behave differently; the upstream bug
  ([mudler/parakeet.cpp#13](https://github.com/mudler/parakeet.cpp/issues/13))
  regarding continuous streaming stopping after an EOU event has not been
  reproduced on Metal but has not been definitively ruled out.

---

## Testing

Most unit tests (error display, `common_prefix_len` correctness) run without
model weights. Integration tests that require a real model are gated on
environment variables and skip automatically when those variables are unset:

| Test | Required environment variables |
|------|-------------------------------|
| `transcribe_real_model` | `PARAKEET_TEST_MODEL` (path to a `.gguf`), `PARAKEET_TEST_WAV` (path to a 16 kHz mono WAV) |
| `pseudo_stream_accumulates` | `PARAKEET_TEST_MODEL`, `PARAKEET_TEST_WAV` |
| `stream_real_rejects_offline_model` | `PARAKEET_OFFLINE_MODEL` (path to a non-streaming `.gguf`) |

To run the full test suite including model tests:

```sh
PARAKEET_TEST_MODEL=./models/tdt-0.6b-v3-q8_0.gguf \
PARAKEET_TEST_WAV=/path/to/audio_16k_mono.wav \
PARAKEET_OFFLINE_MODEL=./models/tdt-0.6b-v3-q8_0.gguf \
cargo test
```

---

## License

This crate (`parakeet-cpp` and `parakeet-cpp-sys`) is released under the
[MIT license](LICENSE).

The vendored `vendor/parakeet.cpp` upstream is also MIT. The vendored
`vendor/parakeet.cpp/third_party/ggml` (ggml) is also MIT.

Model weights downloaded from Hugging Face are separately licensed under NVIDIA's
model license. Refer to each model card for the exact terms before use or
redistribution.

---

## Acknowledgements

- [mudler/parakeet.cpp](https://github.com/mudler/parakeet.cpp) — the ggml-based
  C++ ASR engine and C ABI that these bindings wrap.
- [ggml-org/ggml](https://github.com/ggml-org/ggml) — the tensor computation
  backend powering the inference.
- NVIDIA for the original Parakeet and Nemotron ASR model architectures and weights.

---

## Pinned upstream

`vendor/parakeet.cpp` is pinned to commit
`e270af73b94c9a5c37ec516230219ed4580e1db6` (master, 2026-06-08).
