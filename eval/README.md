# eval/

Offline WER evaluation harness for the parakeet-cpp spike gate (Task 10).

## Layout

```
eval/
  audio/   16 kHz mono WAV clips (gitignored — regenerate with say + afconvert)
  refs/    Plain-text reference transcripts, one per stem (committed)
```

## Running

From the workspace root:

```bash
# Streaming model (nemotron-3.5-asr-streaming-0.6b)
cargo run --release --example spike -- wer ./models/nemotron-3.5-asr-streaming-0.6b-q5_k.gguf it

# Offline model (parakeet-tdt-0.6b-v3)
cargo run --release --example spike -- wer ./models/tdt-0.6b-v3-q8_0.gguf it
```

## Regenerating the audio

The clips are macOS `say -v Alice` (it_IT) output converted to 16 kHz mono WAV.
Use the matching ref text for each stem:

```bash
stem=sentence_01
say -v Alice "$(cat eval/refs/${stem}.txt)" -o /tmp/${stem}.aiff
afconvert -f WAVE -d LEI16@16000 -c 1 /tmp/${stem}.aiff eval/audio/${stem}.wav
```

## Caveat

These clips are clean TTS audio synthesised from the exact reference text.
Both models score near-perfectly on clean TTS and the numbers are not
representative of real-speech WER. The comparison is useful only as a
sanity-check (correct Italian vocabulary) and to surface systematic issues
(tag leakage, number normalisation). The gate decision for criterion 2 needs
human-recorded Italian speech.
