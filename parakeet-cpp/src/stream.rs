use crate::error::Error;
use crate::options::Transcript;

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
