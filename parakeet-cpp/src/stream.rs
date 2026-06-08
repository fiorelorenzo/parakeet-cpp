use std::ffi::CStr;

use parakeet_cpp_sys as sys;

use crate::error::Error;
use crate::model::Model;
use crate::options::{TranscribeOptions, Transcript};

/// One incremental update from a streaming session.
///
/// Note: some multilingual models (e.g. nemotron) embed language tags like
/// `<it-IT>` in the text, and the first delta of an utterance may carry a
/// leading space. Stripping these is the consumer's responsibility.
#[derive(Debug, Clone, Default)]
pub struct Partial {
    /// Full cumulative transcript so far (authoritative).
    pub text: String,
    /// Newly appended tail vs the previous `text` (a hint; may be rewritten).
    pub delta: String,
    /// An end-of-utterance/backchannel event fired during this feed.
    pub eou: bool,
    pub is_final: bool,
}

/// Backend-uniform streaming surface. Real and pseudo sessions both implement it
/// so a consumer treats them identically.
pub trait StreamSession {
    fn feed(&mut self, pcm: &[f32]) -> Result<Partial, Error>;
    fn finish(self: Box<Self>) -> Result<Transcript, Error>;
}

/// Pseudo-streaming: grow a buffer, re-decode the whole buffer with the offline
/// transcribe per feed, diff against the previous text. O(n^2); fine for
/// dictation-length audio. Mirrors today's audiopipe behaviour, on any model.
pub struct PseudoStreamSession<'a> {
    model: &'a mut Model,
    sample_rate: u32,
    opts: TranscribeOptions,
    buffer: Vec<f32>,
    last_text: String,
}

impl<'a> PseudoStreamSession<'a> {
    pub(crate) fn new(model: &'a mut Model, sample_rate: u32, opts: TranscribeOptions) -> Self {
        Self {
            model,
            sample_rate,
            opts,
            buffer: Vec::new(),
            last_text: String::new(),
        }
    }
}

impl StreamSession for PseudoStreamSession<'_> {
    fn feed(&mut self, pcm: &[f32]) -> Result<Partial, Error> {
        self.buffer.extend_from_slice(pcm);
        let t = self
            .model
            .transcribe(&self.buffer, self.sample_rate, &self.opts)?;
        let n = common_prefix_len(&self.last_text, &t.text);
        let delta = t.text[n..].to_string();
        self.last_text = t.text.clone();
        Ok(Partial {
            text: t.text,
            delta,
            eou: false,
            is_final: false,
        })
    }

    fn finish(self: Box<Self>) -> Result<Transcript, Error> {
        // The last feed already produced the full transcript; return it without
        // re-decoding. If no feed happened, decode whatever is buffered.
        if self.last_text.is_empty() && !self.buffer.is_empty() {
            // Split fields to satisfy the borrow checker: model and buffer are
            // disjoint, but Box<Self> deref doesn't know that without help.
            let model = self.model;
            let buffer = &self.buffer;
            let sample_rate = self.sample_rate;
            let opts = &self.opts;
            return model.transcribe(buffer, sample_rate, opts);
        }
        Ok(Transcript {
            text: self.last_text,
            words: Vec::new(),
        })
    }
}

/// Real cache-aware streaming. Borrows the Model so it can read last_error from
/// the same ctx. `feed` pushes only the new tail; returns newly-finalized text.
///
/// Deltas are SentencePiece-detokenized incremental text accumulated verbatim;
/// inter-word spaces are embedded as regular `' '` characters by detokenize().
pub struct RealStreamSession<'a> {
    model: &'a mut Model,
    stream: *mut sys::parakeet_stream,
    cumulative: String,
}

impl<'a> RealStreamSession<'a> {
    pub(crate) fn begin(model: &'a mut Model, lang: Option<&str>) -> Result<Self, Error> {
        let raw = match lang {
            Some(l) => {
                let c =
                    std::ffi::CString::new(l).map_err(|_| Error::Transcribe("lang NUL".into()))?;
                unsafe { sys::parakeet_capi_stream_begin_lang(model.ctx, c.as_ptr()) }
            }
            None => unsafe { sys::parakeet_capi_stream_begin(model.ctx) },
        };
        if raw.is_null() {
            return Err(Error::NotStreaming);
        }
        Ok(Self {
            model,
            stream: raw,
            cumulative: String::new(),
        })
    }
}

impl StreamSession for RealStreamSession<'_> {
    fn feed(&mut self, pcm: &[f32]) -> Result<Partial, Error> {
        let n =
            i32::try_from(pcm.len()).map_err(|_| Error::Transcribe("too many samples".into()))?;
        let mut eou_out: std::os::raw::c_int = 0;
        // Returns "" (empty, non-null) when nothing newly finalized; NULL on error.
        let raw =
            unsafe { sys::parakeet_capi_stream_feed(self.stream, pcm.as_ptr(), n, &mut eou_out) };
        if raw.is_null() {
            return Err(Error::Transcribe(self.model.last_error()));
        }
        // SAFETY: raw is a malloc'd NUL-terminated string. We materialize a Rust
        // String before freeing so no borrow outlives the allocation. free_string
        // runs on every path (success and UTF-8 error) exactly once.
        let delta_result = unsafe { CStr::from_ptr(raw) }.to_str().map(str::to_owned);
        unsafe { sys::parakeet_capi_free_string(raw) };
        let delta = delta_result.map_err(|_| Error::Utf8)?;
        // Accumulate verbatim: the C API returns the exact newly-finalized byte
        // range of the full detokenized text (via take_new_text()). Inter-word
        // spaces are embedded as regular ' ' characters by detokenize(); sub-word
        // continuations have no separator. Trimming or force-inserting spaces
        // here would split words when the model finalizes sub-word fragments.
        self.cumulative.push_str(&delta);
        Ok(Partial {
            text: self.cumulative.clone(),
            delta,
            eou: eou_out != 0,
            is_final: false,
        })
    }

    fn finish(mut self: Box<Self>) -> Result<Transcript, Error> {
        // stream_finalize flushes the tail but does NOT free the stream;
        // Drop (stream_free) owns the free. Do not null self.stream here or
        // Drop would leak it.
        let raw = unsafe { sys::parakeet_capi_stream_finalize(self.stream) };
        if raw.is_null() {
            return Err(Error::Transcribe(self.model.last_error()));
        }
        // SAFETY: same as feed — materialize before free, free exactly once.
        let tail_result = unsafe { CStr::from_ptr(raw) }.to_str().map(str::to_owned);
        unsafe { sys::parakeet_capi_free_string(raw) };
        let tail = tail_result.map_err(|_| Error::Utf8)?;
        // Accumulate verbatim for the same reason as feed: the finalize tail is
        // the remaining substring of the full detokenized text.
        self.cumulative.push_str(&tail);
        Ok(Transcript {
            text: std::mem::take(&mut self.cumulative),
            words: Vec::new(),
        })
    }
}

impl Drop for RealStreamSession<'_> {
    fn drop(&mut self) {
        // Frees the stream exactly once for both the finish() path and the
        // drop-without-finish path.
        if !self.stream.is_null() {
            unsafe { sys::parakeet_capi_stream_free(self.stream) };
            self.stream = std::ptr::null_mut();
        }
    }
}

/// Returns the longest common prefix length in bytes of `a` and `b`, snapped
/// down to a char boundary valid in both strings.
///
/// Used internally to compute streaming delta diffs: after each decode, we
/// advance the consumer's view by this many bytes rather than resetting from
/// zero, so sub-word continuations are not split at multibyte boundaries.
#[must_use]
pub fn common_prefix_len(a: &str, b: &str) -> usize {
    let mut n = a
        .as_bytes()
        .iter()
        .zip(b.as_bytes())
        .take_while(|(x, y)| x == y)
        .count();
    while n > 0 && (!a.is_char_boundary(n) || !b.is_char_boundary(n)) {
        n -= 1;
    }
    n
}
