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

## Models

Download GGUF weights from the [mudler/parakeet-cpp-gguf](https://huggingface.co/mudler/parakeet-cpp-gguf) HuggingFace repo:

```bash
pip install -U "huggingface_hub[cli]"
# On macOS with Homebrew Python, use `hf` instead of `huggingface-cli`:
hf download mudler/parakeet-cpp-gguf tdt-0.6b-v3-q8_0.gguf --local-dir ./models
hf download mudler/parakeet-cpp-gguf nemotron-3.5-asr-streaming-0.6b-q5_k.gguf --local-dir ./models
```

`tdt-0.6b-v3-q8_0.gguf` (~897 MB) is the offline TDT-v3 batch model.
`nemotron-3.5-asr-streaming-0.6b-q5_k.gguf` is the real-time streaming model.

### Quick smoke test

After downloading, generate a test clip and run the bundled `spike` example:

```bash
# Generate a 16 kHz mono WAV (macOS; Italian voice if available)
say -v Alice "Ciao, questo è un test del sistema di riconoscimento vocale Parakeet." -o /tmp/smoke.aiff || \
  say "Hello, this is a test of the Parakeet speech recognition system." -o /tmp/smoke.aiff
afconvert -f WAVE -d LEI16@16000 -c 1 /tmp/smoke.aiff /tmp/smoke.wav

# Run inference (Metal on Apple Silicon; first run JITs shaders — be patient)
cargo run --release --example spike -- ./models/tdt-0.6b-v3-q8_0.gguf /tmp/smoke.wav it
```

Expected output: `is_streaming = false` followed by a transcript of the spoken sentence.

## Pinned upstream

`vendor/parakeet.cpp` is pinned to commit `e270af73b94c9a5c37ec516230219ed4580e1db6` (master, 2026-06-08).
