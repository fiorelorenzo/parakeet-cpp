#[derive(Debug, Clone, Default)]
pub struct TranscribeOptions {
    /// Target language code (e.g. "it", "en", "auto"). None = engine default.
    pub language: Option<String>,
    /// Request per-word timestamps (uses the JSON C entry points). Off by default.
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
