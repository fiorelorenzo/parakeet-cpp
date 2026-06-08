use crate::error::Error;
use crate::model::Model;
use crate::options::{TranscribeOptions, Transcript};

/// One incremental update from a streaming session.
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

/// Longest common prefix length in bytes, snapped to a char boundary.
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
