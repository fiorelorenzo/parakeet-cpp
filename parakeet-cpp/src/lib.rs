//! Safe Rust bindings to [mudler/parakeet.cpp](https://github.com/mudler/parakeet.cpp),
//! a ggml-based inference library for NVIDIA Parakeet and Nemotron ASR models in GGUF format.
//!
//! Audio input must be 16 kHz mono f32 PCM. Two transcription modes are available:
//!
//! - **Offline** — [`Model::transcribe`] decodes a complete audio buffer in one call.
//!   Works with any GGUF model; `tdt-0.6b-v3-q8_0` is the recommended starting point.
//! - **Streaming** — feed audio incrementally via the [`StreamSession`] trait.
//!   [`RealStreamSession`] (obtained from [`Model::stream_real`]) uses the model's native
//!   cache-aware streaming path and requires a streaming-capable GGUF (e.g.
//!   `nemotron-3.5-asr-streaming-0.6b-q5_k`).
//!   [`PseudoStreamSession`] (obtained from [`Model::stream_pseudo`]) buffers audio and
//!   re-runs the offline decoder each feed, making any model streamable at O(n^2) cost.
//!
//! # Example
//!
//! ```rust,no_run
//! use parakeet_cpp::{Model, TranscribeOptions};
//! use std::path::Path;
//!
//! let mut model = Model::load(Path::new("tdt-0.6b-v3-q8_0.gguf")).unwrap();
//! let pcm: Vec<f32> = vec![0.0f32; 16_000]; // 1 s of silence at 16 kHz
//! let opts = TranscribeOptions { language: Some("it".into()), ..Default::default() };
//! let transcript = model.transcribe(&pcm, 16_000, &opts).unwrap();
//! println!("{}", transcript.text);
//! ```
//!
//! See the repository README for build prerequisites and model download instructions.

mod error;
mod model;
mod options;
mod stream;

pub use error::Error;
pub use model::{Model, SUPPORTED_ABI};
pub use options::{TranscribeOptions, Transcript, Word};
pub use stream::{
    common_prefix_len, Partial, PseudoStreamSession, RealStreamSession, StreamSession,
};
