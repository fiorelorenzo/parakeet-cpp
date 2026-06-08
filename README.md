# parakeet-cpp

Rust bindings for mudler/parakeet.cpp (ggml-based Parakeet/Nemotron ASR).

## Building

Before building, initialise the vendored submodule:

```sh
git submodule update --init --recursive
```

On macOS the Metal backend is enabled automatically — no feature flag needed.
Non-macOS builds default to CPU-only; pass an explicit feature to enable a GPU
backend:

```sh
cargo build --features vulkan   # Vulkan (Linux / Windows)
cargo build --features cuda     # CUDA
cargo build --features hip      # ROCm / HIP
cargo build --features metal    # Metal (macOS, already implicit)
```

## Pinned upstream

`vendor/parakeet.cpp` is pinned to commit `e270af73b94c9a5c37ec516230219ed4580e1db6` (master, 2026-06-08).
