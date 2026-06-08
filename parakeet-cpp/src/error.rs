#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to load model: {0}")]
    Load(String),
    #[error("transcription failed: {0}")]
    Transcribe(String),
    #[error("model does not support real streaming")]
    NotStreaming,
    #[error("ABI mismatch: binding built for {expected}, library is {found}")]
    AbiMismatch { expected: i32, found: i32 },
    #[error("invalid UTF-8 from native layer")]
    Utf8,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn display_is_human_readable() {
        assert_eq!(
            Error::AbiMismatch {
                expected: 4,
                found: 3
            }
            .to_string(),
            "ABI mismatch: binding built for 4, library is 3"
        );
        assert!(Error::NotStreaming.to_string().contains("real streaming"));
    }
}
