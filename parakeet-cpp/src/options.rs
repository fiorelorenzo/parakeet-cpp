#[derive(Debug, Clone, Default)]
pub struct TranscribeOptions {
    /// Target language code (e.g. "it", "en", "auto"). None = engine default.
    pub language: Option<String>,
    /// Reserved: request per-word timestamps. NOT yet implemented — `transcribe`
    /// currently always returns an empty `words` vec regardless of this flag.
    pub word_timestamps: bool,
}

#[derive(Debug, Clone)]
pub struct Word {
    pub text: String,
    pub start: f64,
    pub end: f64,
    pub conf: f64,
}

#[derive(Debug, Clone, Default)]
pub struct Transcript {
    pub text: String,
    pub words: Vec<Word>,
}
