# eval/

WER evaluation harness for parakeet-cpp models.

## Layout

```
eval/
  audio/   16 kHz mono WAV clips (gitignored — provide your own)
  refs/    Plain-text reference transcripts, one per clip stem (gitignored)
```

Both directories are gitignored; you supply your own clips and references locally.

## Adding clips

1. Place your audio clips as `eval/audio/<name>.wav` (16 kHz, mono, f32 or int16 WAV).
2. Place the matching reference transcript as `eval/refs/<name>.txt` (plain text, one
   utterance per file, lowercased and punctuation-stripped if you want WER to match
   how models emit text).

## Running WER evaluation

From the workspace root:

```bash
# Offline model (e.g. parakeet-tdt-0.6b-v3)
cargo run --release --example spike -- wer ./models/tdt-0.6b-v3-q8_0.gguf it

# Streaming model (e.g. nemotron-3.5-asr-streaming-0.6b)
cargo run --release --example spike -- wer ./models/nemotron-3.5-asr-streaming-0.6b-q5_k.gguf it
```

The second argument is the language code passed to the model (e.g. `it`, `en`).

WER is computed as Levenshtein distance over whitespace-split tokens, after lowercasing
and stripping punctuation. This is a rough metric; adjust the normalization in
`examples/spike.rs` to match your reference format.

## Notes

- Strip `<lang-tag>` language tokens from model output before scoring if using nemotron
  (it emits e.g. `<it-IT>` at sentence boundaries).
- For representative WER numbers, use real human-recorded speech rather than TTS clips.
  TTS audio is atypically clean and will produce optimistic scores.
