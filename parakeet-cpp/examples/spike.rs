use parakeet_cpp::{Model, StreamSession, TranscribeOptions};
use std::path::Path;
use std::time::Instant;

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

/// Offline one-shot: `spike offline <model> <wav> [lang]`
fn cmd_offline(a: &[String]) {
    let opts = TranscribeOptions {
        language: a.get(4).cloned(),
        word_timestamps: false,
    };
    let mut model = Model::load(Path::new(&a[2])).expect("load");
    eprintln!("is_streaming = {}", model.is_streaming());
    let pcm = load_wav_16k_mono(&a[3]);
    let t = model.transcribe(&pcm, 16_000, &opts).expect("transcribe");
    println!("{}", t.text);
}

/// Word Error Rate (Levenshtein over whitespace tokens), lowercased, punctuation
/// stripped. Rough IT comparison only; not a substitute for a real WER tool.
fn wer(reference: &str, hypothesis: &str) -> f64 {
    fn toks(s: &str) -> Vec<String> {
        s.to_lowercase()
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c.is_whitespace() {
                    c
                } else {
                    ' '
                }
            })
            .collect::<String>()
            .split_whitespace()
            .map(str::to_owned)
            .collect()
    }
    let r = toks(reference);
    let h = toks(hypothesis);
    let (n, m) = (r.len(), h.len());
    if n == 0 {
        return if m == 0 { 0.0 } else { 1.0 };
    }
    let mut prev: Vec<usize> = (0..=m).collect();
    let mut cur = vec![0usize; m + 1];
    for i in 1..=n {
        cur[0] = i;
        for j in 1..=m {
            let cost = usize::from(r[i - 1] != h[j - 1]);
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[m] as f64 / n as f64
}

/// Criterion 1+3: `spike stream <model> <wav> [lang]`
fn cmd_stream(model_path: &str, wav: &str, lang: Option<String>) {
    let mut model = Model::load(Path::new(model_path)).expect("load");
    assert!(model.is_streaming(), "model is not a streaming model");
    let pcm = load_wav_16k_mono(wav);
    let opts = TranscribeOptions {
        language: lang,
        word_timestamps: false,
    };
    let mut s = Box::new(model.stream_real(&opts).expect("stream_real"));
    let chunk = 16_000 / 2; // 500 ms chunks
    let (mut eou_count, mut text_after_first_eou, mut first_eou_seen) = (0u32, false, false);
    let mut max_feed_ms = 0.0_f64;
    for c in pcm.chunks(chunk) {
        let t0 = Instant::now();
        let p = s.feed(c).expect("feed");
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        max_feed_ms = max_feed_ms.max(ms);
        if first_eou_seen && !p.delta.trim().is_empty() {
            text_after_first_eou = true;
        }
        if p.eou {
            eou_count += 1;
            first_eou_seen = true;
        }
        eprintln!("[feed {:>6.1}ms eou={}] +{:?}", ms, p.eou, p.delta);
    }
    let final_t = s.finish().expect("finish");
    println!("--- CRITERION 1 (continuity past EOU) ---");
    println!("eou_events={eou_count} text_after_first_eou={text_after_first_eou}");
    println!("PASS(1) = {}", eou_count == 0 || text_after_first_eou);
    println!("--- CRITERION 3 (latency) ---");
    println!("max_feed_ms={max_feed_ms:.1} (target: comfortably < 400)");
    println!("final: {}", final_t.text);
}

/// Criterion 2: `spike wer <model> [lang]` over eval/audio/*.wav + eval/refs/<stem>.txt
fn cmd_wer(model_path: &str, lang: Option<String>) {
    let mut model = Model::load(Path::new(model_path)).expect("load");
    let opts = TranscribeOptions {
        language: lang,
        word_timestamps: false,
    };
    let mut total = 0.0_f64;
    let mut count = 0u32;
    for entry in std::fs::read_dir("eval/audio").expect("eval/audio") {
        let wav = entry.unwrap().path();
        if wav.extension().and_then(|e| e.to_str()) != Some("wav") {
            continue;
        }
        let stem = wav.file_stem().unwrap().to_string_lossy().into_owned();
        let reference = std::fs::read_to_string(format!("eval/refs/{stem}.txt")).expect("ref");
        let pcm = load_wav_16k_mono(wav.to_str().unwrap());
        let hyp = model
            .transcribe(&pcm, 16_000, &opts)
            .expect("transcribe")
            .text;
        let e = wer(&reference, &hyp);
        total += e;
        count += 1;
        println!("{stem}: WER={e:.3}");
    }
    println!("--- CRITERION 2 (mean WER over {count} clips) ---");
    println!("mean_WER={:.3}", total / f64::from(count.max(1)));
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    match a.get(1).map(String::as_str) {
        Some("offline") => cmd_offline(&a),
        Some("stream") => cmd_stream(&a[2], &a[3], a.get(4).cloned()),
        Some("wer") => cmd_wer(&a[2], a.get(3).cloned()),
        _ => {
            eprintln!("usage: spike <offline|stream|wer> ...");
            std::process::exit(2);
        }
    }
}
