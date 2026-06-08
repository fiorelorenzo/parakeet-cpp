use parakeet_cpp::{Model, TranscribeOptions};
use std::path::Path;

fn load_wav_16k_mono(path: &str) -> Vec<f32> {
    let mut r = hound::WavReader::open(path).expect("open wav");
    let spec = r.spec();
    assert_eq!(spec.channels, 1);
    assert_eq!(spec.sample_rate, 16_000);
    match spec.sample_format {
        hound::SampleFormat::Float => r
            .samples::<f32>()
            .map(|s| s.expect("decode sample"))
            .collect(),
        hound::SampleFormat::Int => {
            let max = (1i64 << (spec.bits_per_sample - 1)) as f32;
            r.samples::<i32>()
                .map(|s| s.expect("decode sample") as f32 / max)
                .collect()
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: spike <model.gguf> <audio.wav> [lang]");
        std::process::exit(2);
    }
    let opts = TranscribeOptions {
        language: args.get(3).cloned(),
        word_timestamps: false,
    };
    let mut model = Model::load(Path::new(&args[1])).expect("load");
    eprintln!("is_streaming = {}", model.is_streaming());
    let pcm = load_wav_16k_mono(&args[2]);
    let t = model.transcribe(&pcm, 16_000, &opts).expect("transcribe");
    println!("{}", t.text);
}
