# Benchmarks & Findings — parakeet-cpp Rust Bindings

**Machine:** Apple M4, macOS 15, Metal backend
**Date:** 2026-06-08
**Models tested:**
- `tdt-0.6b-v3-q8_0.gguf` (offline, parakeet-tdt-v3)
- `nemotron-3.5-asr-streaming-0.6b-q5_k.gguf` (streaming, nemotron)

**Eval set:** Google FLEURS `it_it` test subset — 8 real-human Italian clips, 16 kHz mono.
**WER metric:** Levenshtein distance over whitespace-split tokens, lowercased, punctuation stripped (rough approximation).

---

## Italian WER (offline `transcribe`, FLEURS `it_it`)

| Model | Mean WER |
|---|---|
| tdt-0.6b-v3-q8\_0 (offline) | 0.005 |
| nemotron-3.5-asr-streaming-0.6b-q5\_k, raw output | 0.165 |
| nemotron, `<it-IT>` tags stripped | 0.068 |
| nemotron, tags stripped + number-words normalized to digits | 0.044 |

**Notes:**

- `tdt-v3` emits clean text with native digit formatting and no language tags. It is the more accurate choice for Italian offline transcription on this hardware.
- Nemotron emits `<it-IT>` language tags at sentence boundaries. These account for roughly 59% of its raw WER gap. The consumer layer must strip all `<...>` tags before displaying or injecting text.
- Even after full normalization (tag strip + number-word-to-digit conversion), nemotron has approximately 9x more genuine ASR errors than tdt-v3 on this set. Residual errors include substitutions such as `prefigge` for `preflige` and `agosta` for `agosto`.

---

## Quantization trade-off (tdt-0.6b-v3, FLEURS `it_it`)

All quantizations of `tdt-0.6b-v3` measured on the same 8-clip FLEURS Italian set. Peak resident set size (RSS) measured with `/usr/bin/time -l` on the release binary running a single offline transcribe.

| Quant | File (MB) | Peak RSS (MB) | Mean WER | vs q8_0 |
|---|---|---|---|---|
| q8_0 | 897 | 1880 | 0.005 | baseline |
| q6_k | 775 | 1636 | 0.010 | 2x (worse) |
| q5_k | 707 | 1501 | 0.005 | identical |
| q4_k | 644 | 1374 | 0.005 | identical |

**Notes:**

- `q5_k` matches `q8_0` accuracy exactly (same transcripts) while cutting peak RSS by roughly 20%, and is slightly faster — k-quants are memory-bandwidth bound on Apple Silicon, so smaller weights decode faster.
- `q6_k` is strictly dominated by `q5_k`: it is larger and shows a 2x WER regression from a single spurious word insertion. More bits do not guarantee better quality with k-quants.
- `q4_k` showed no degradation on this set (−27% RSS vs `q8_0`), but 8 clean read-speech clips is a narrow sample; validate on noisier, conversational speech before relying on it.
- Recommended default: **`q5_k`**. `q4_k` is a reasonable low-RAM option.

---

## Streaming and end-of-utterance (EOU) behaviour

Tested on a 45.5-second concatenation of the 8 real FLEURS Italian clips run through `RealStreamSession` in 500 ms chunks.

- **Continuous decoding worked correctly.** All sentences were present in the final transcript; no freeze or stall occurred.
- **`eou_events = 0`** — the model's end-of-utterance event never fired across the entire recording, including on natural speech pauses between sentences. EOU-based utterance segmentation is effectively non-functional in this build.
- The upstream concern (issue #13: continuous streaming stops after the first EOU) could not be reproduced — but neither could functional EOU segmentation be demonstrated. Callers must not rely on EOU to detect utterance boundaries.

---

## Latency

Feed size: 500 ms chunks (8 000 samples at 16 kHz).

| Metric | Value |
|---|---|
| Max per-feed latency (M4 Metal) | 91.7 ms |
| Real-time headroom | ~5.5x |

Latency is well within a comfortable interactive margin for 500 ms chunk sizes.

---

## Known teardown abort (SIGABRT)

The process exits with SIGABRT after printing all output. The abort is triggered by an upstream ggml-metal residency-set cleanup assertion:

```
GGML_ASSERT([rsets->data count] == 0)
```

All transcription output is fully flushed before the abort occurs, so correctness is not affected. Long-lived host applications that manage model lifetime explicitly (load once, transcribe repeatedly, unload on shutdown) will also encounter this on program exit.

This is an upstream ggml-metal teardown issue, not a binding bug.

---

## Summary

For offline Italian transcription on Apple Silicon, `tdt-0.6b-v3` is markedly more accurate than the streaming nemotron model and requires no post-processing. The streaming nemotron model in this build does not deliver functional EOU segmentation and is less accurate even after normalization. Choose the offline model unless or until upstream improves the streaming model's EOU logic and Italian accuracy.
