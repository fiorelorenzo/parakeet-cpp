# Contributing to parakeet-cpp

Thank you for your interest in contributing. This document covers how to set up
the development environment, run the tests, navigate the build model, and submit
changes.

---

## Cloning

The repository uses a git submodule for the vendored upstream (`vendor/parakeet.cpp`),
which itself contains a nested submodule for ggml. Clone with `--recursive` to
initialize both:

```sh
git clone --recursive https://github.com/fiorelorenzo/parakeet-cpp.git
cd parakeet-cpp
```

If you already cloned without `--recursive`, initialize the submodules now:

```sh
git submodule update --init --recursive
```

**This step is mandatory.** The build script will fail with a clear error message
if `vendor/parakeet.cpp` is empty.

---

## Building

The build is driven by Cargo. The `build.rs` in `parakeet-cpp-sys` runs CMake
over the vendored upstream, applies any ggml patches, and links the resulting
static libraries. You need:

- Rust 1.88+ (toolchain pinned in `rust-toolchain.toml`; `rustup` installs it
  automatically on first build if a `rust-toolchain.toml` is present)
- CMake 3.15+
- A C/C++ toolchain (`clang`/`clang++` or `gcc`/`g++`)
- **macOS:** Xcode Command Line Tools

```sh
# CPU-only (works everywhere)
cargo build

# macOS Metal (already implicit; no flag needed)
cargo build --release

# Linux / Windows with Vulkan
cargo build --features vulkan

# CUDA
cargo build --features cuda

# ROCm / HIP
cargo build --features hip
```

The first build is slow because it compiles the upstream C++ library and, on
macOS, JIT-compiles Metal shaders. Subsequent incremental builds are fast.

---

## Running tests

Unit tests (no model required) run with a plain `cargo test`:

```sh
cargo test
```

Integration tests that load a real model are gated on environment variables and
skip silently when those variables are absent. To enable them:

```sh
PARAKEET_TEST_MODEL=./models/tdt-0.6b-v3-q8_0.gguf \
PARAKEET_TEST_WAV=/path/to/audio_16k_mono.wav \
PARAKEET_OFFLINE_MODEL=./models/tdt-0.6b-v3-q8_0.gguf \
cargo test
```

| Variable | Purpose |
|----------|---------|
| `PARAKEET_TEST_MODEL` | Path to any `.gguf` model for transcription tests |
| `PARAKEET_TEST_WAV` | Path to a 16 kHz mono WAV file |
| `PARAKEET_OFFLINE_MODEL` | Path to a non-streaming `.gguf`; used to verify `stream_real` rejects it |

Audio must be 16 kHz, mono, f32 or int PCM WAV. See the `load_wav_16k_mono`
helper in `parakeet-cpp/tests/integration.rs` for the expected format.

---

## The `spike` example

The `spike` example exercises offline transcription, real streaming, and WER
evaluation. See the README for full usage. Running it requires a model file:

```sh
cargo run --release --example spike -- offline \
    ./models/tdt-0.6b-v3-q8_0.gguf /path/to/audio.wav
```

Use `--release` — debug builds of the C++ layer are significantly slower and
Metal shader compilation behaves differently.

---

## The bindgen / CMake build model

`parakeet-cpp-sys/build.rs` is the only place that touches the native build:

1. It locates `vendor/parakeet.cpp` relative to `CARGO_MANIFEST_DIR` and aborts
   with a human-readable message if the directory is empty (submodule not
   initialized).
2. It applies ggml patches from `vendor/parakeet.cpp/third_party/ggml-patches/`
   using `git apply` (idempotent: checks `--reverse --check` before applying).
   No shell scripts or bash are used — all patch application goes through the
   `git` binary invoked via `std::process::Command`.
3. It invokes `cmake::Config` to build the upstream with fixed flags:
   `PARAKEET_SHARED=OFF`, `GGML_NATIVE=OFF` (portable artifacts),
   `BUILD_SHARED_LIBS=OFF`, `PARAKEET_BUILD_CLI=OFF`,
   `PARAKEET_BUILD_TESTS=OFF`. GPU backends are added conditionally based on
   the active Cargo features.
4. It links the produced static libraries in dependency order. The linked
   library names are verified against the build output directory — see the
   `candidates` list in `build.rs`. After bumping the submodule, re-verify
   that the library names still match by checking the CMake build output.
5. It generates Rust FFI bindings via `bindgen` over
   `vendor/parakeet.cpp/include/parakeet_capi.h`, allowlisting only
   `parakeet_capi_*` functions and `parakeet_*` types.

If you add or remove a backend feature, update both `parakeet-cpp-sys/Cargo.toml`
(feature definition) and `build.rs` (the corresponding `cfg.define(...)` call).
Mirror the feature in `parakeet-cpp/Cargo.toml` so consumers can enable it
through the safe wrapper.

---

## Cross-platform discipline

The crate is macOS-first today, but must remain portable. Follow these rules:

- **Do not break non-macOS compilation.** Before merging any change that touches
  `build.rs` or adds platform-specific logic, verify with:
  ```sh
  cargo check --target x86_64-unknown-linux-gnu
  ```
  If you cannot run this locally, call it out explicitly in your pull request.
- **macOS-only system calls go in `build.rs` only via feature or target guards.**
  The Rust wrapper code (`parakeet-cpp/src/`) must not contain `#[cfg(target_os = "macos")]`
  blocks unless they correspond to a documented behavioral difference in the
  upstream C ABI.
- **GPU backends are feature flags, not platform guards.** A contributor on
  Linux should be able to pass `--features vulkan` without hitting a compile
  error. If a backend is genuinely macOS-only, document that clearly in the
  feature description.
- **No bash or shell scripts in the build.** `build.rs` must use only Rust code
  and `std::process::Command`. This keeps the build working on Windows where
  bash is not available.

---

## Coding standards

**Formatting:** `cargo fmt` is mandatory. Format before committing:

```sh
cargo fmt --all
```

**Lints:** `cargo clippy` with `-D warnings` is enforced. Fix warnings rather
than suppressing them. The workspace enables `clippy::pedantic` per-crate; new
`#[allow(...)]` attributes require a one-line justification comment.

```sh
cargo clippy --workspace --all-targets -- -D warnings
```

**Unsafe:** All `unsafe` blocks must have a `// SAFETY:` comment explaining why
the invariants hold. Follow the pattern established in `model.rs` and `stream.rs`.

**Comments:** Add a comment only when the *why* is non-obvious. Do not restate
what well-named code already expresses.

**Commits:** Use [Conventional Commits](https://www.conventionalcommits.org/)
prefixes (`feat:`, `fix:`, `docs:`, `chore:`, `test:`, `refactor:`). Keep the
subject line under 72 characters. Put detail in the commit body.

---

## Bumping the vendored upstream

To update the `vendor/parakeet.cpp` submodule pin:

1. Update the submodule:
   ```sh
   cd vendor/parakeet.cpp
   git fetch origin
   git checkout <new-commit-or-tag>
   cd ../..
   git add vendor/parakeet.cpp
   ```
2. Re-initialize the nested ggml submodule at the new pin:
   ```sh
   git submodule update --init --recursive
   ```
3. Run a full build to confirm CMake still configures cleanly:
   ```sh
   cargo clean && cargo build --release
   ```
4. **Re-verify the static library names** against the CMake build output.
   The `candidates` list in `build.rs` reflects the names produced by the
   pinned upstream (`parakeet`, `ggml`, `ggml-base`, `ggml-cpu`, `ggml-metal`,
   etc.). ggml has split and renamed backend libraries across versions; after a
   bump, check `target/.../out/build/` and `target/.../out/lib/` for the actual
   `.a` filenames and update `candidates` accordingly.
5. Update the pinned commit hash in `README.md`.
6. Run the integration tests with a real model to confirm transcription still
   works.
7. Check whether the C ABI version reported by `parakeet_capi_abi_version()`
   changed. If it did, update `SUPPORTED_ABI` in `parakeet-cpp/src/model.rs`
   and review any breaking changes in `parakeet_capi.h`.

---

## Reporting issues

Open an issue on GitHub describing:
- the host OS, CPU, and Rust toolchain version
- the exact `cargo` command and feature flags used
- the full error output (build log or panic message)
- the model file name and its source (Hugging Face repo)

For issues that appear to originate in the upstream C++ library rather than the
Rust wrapper, please also check the
[mudler/parakeet.cpp issue tracker](https://github.com/mudler/parakeet.cpp/issues).
