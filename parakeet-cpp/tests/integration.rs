use parakeet_cpp::{Model, StreamSession, TranscribeOptions};
use std::path::Path;

#[test]
fn load_bad_path_errors() {
    let err = Model::load(Path::new("/no/such/model.gguf"));
    assert!(err.is_err(), "loading a nonexistent model must fail");
}

fn load_wav_16k_mono(path: &str) -> Vec<f32> {
    let mut r = hound::WavReader::open(path).expect("open wav");
    let spec = r.spec();
    assert_eq!(spec.channels, 1, "fixture must be mono");
    assert_eq!(spec.sample_rate, 16_000, "fixture must be 16 kHz");
    match spec.sample_format {
        hound::SampleFormat::Float => r
            .samples::<f32>()
            .map(|s| s.expect("decode wav sample"))
            .collect(),
        hound::SampleFormat::Int => {
            let max = (1i64 << (spec.bits_per_sample - 1)) as f32;
            r.samples::<i32>()
                .map(|s| s.expect("decode wav sample") as f32 / max)
                .collect()
        }
    }
}

#[test]
fn transcribe_real_model() {
    let (Ok(model_path), Ok(wav)) = (
        std::env::var("PARAKEET_TEST_MODEL"),
        std::env::var("PARAKEET_TEST_WAV"),
    ) else {
        eprintln!("skipping: set PARAKEET_TEST_MODEL and PARAKEET_TEST_WAV");
        return;
    };
    let mut model = Model::load(Path::new(&model_path)).expect("load model");
    let pcm = load_wav_16k_mono(&wav);
    let t = model
        .transcribe(&pcm, 16_000, &TranscribeOptions::default())
        .expect("transcribe");
    eprintln!("transcript: {}", t.text);
    assert!(!t.text.trim().is_empty(), "expected non-empty transcript");
}

#[test]
fn pseudo_stream_accumulates() {
    let (Ok(model_path), Ok(wav)) = (
        std::env::var("PARAKEET_TEST_MODEL"),
        std::env::var("PARAKEET_TEST_WAV"),
    ) else {
        eprintln!("skipping");
        return;
    };
    let mut model = Model::load(Path::new(&model_path)).expect("load");
    let pcm = load_wav_16k_mono(&wav);
    let half = pcm.len() / 2;
    let mut s = Box::new(model.stream_pseudo(16_000, TranscribeOptions::default()));
    let p1 = s.feed(&pcm[..half]).expect("feed1");
    let p2 = s.feed(&pcm[half..]).expect("feed2");
    assert!(
        p2.text.len() >= p1.text.len(),
        "cumulative text should not shrink"
    );
    let final_t = s.finish().expect("finish");
    assert!(!final_t.text.trim().is_empty());
}

#[test]
fn stream_real_rejects_offline_model() {
    let Ok(model_path) = std::env::var("PARAKEET_OFFLINE_MODEL") else {
        eprintln!("skipping: set PARAKEET_OFFLINE_MODEL to a non-streaming gguf");
        return;
    };
    let mut model = Model::load(Path::new(&model_path)).expect("load");
    if !model.is_streaming() {
        assert!(matches!(
            model.stream_real(&TranscribeOptions::default()),
            Err(parakeet_cpp::Error::NotStreaming)
        ));
    }
}
