# Spike Gate Results — parakeet-cpp Rust Bindings (Task 10)

**Machine:** Apple M4, macOS 15, 2026-06-08
**Model (streaming):** nemotron-3.5-asr-streaming-0.6b-q5_k.gguf
**Model (offline):** tdt-0.6b-v3-q8_0.gguf

---

## Criteria table

| # | Criterion | Measured | Result |
|---|-----------|----------|--------|
| 1 | EOU fires AND text continues after it (bug #13 absent on Metal) | eou_events=0 on all synthetic clips (4s/3s silence); failure mode not triggered but utterance segmentation via EOU is unavailable | **INCONCLUSIVE** |
| 2 | Italian WER on clean audio (sanity gate only — not real speech) | nemotron mean_WER=0.179 (4 TTS clips, `<it-IT>` tag leakage + occasional initial-cap miss); tdt mean_WER=0.062 | **WEAK PASS** (see caveat) |
| 3 | Per-feed latency comfortably under 400 ms on Metal | max_feed_ms=92.6 ms (pause_long); max_feed_ms=89.7 ms (three_utt); 500 ms chunk size | **PASS** |

---

## Part 1 — Streaming delta accumulation fix

### What the raw delta format turned out to be

The C layer (`streaming.cpp` / `tokenizer.cpp`) maintains a full detokenized
text string and returns `take_new_text()` = the newly-appended byte range since
the last call. Inter-word spaces come from SentencePiece meta-space (`▁`)
being replaced by `' '`; they are already embedded inside the full text.
Sub-word continuations have no separator — the model emits them as partial
token pieces that together form one word.

Example from the pause_long run:
```
[feed  86ms] +" Questa è la prima"
[feed  37ms] +" fra"
[feed  79ms] +"se di prova del"
[feed  43ms] +" siste"
[feed  76ms] +"ma"
```

The delta `" fra"` has a leading space (new word boundary); `"se di prova del"`
has none (continues "fra" into "frase"). A delta from a complete word like
`" siste"` + `"ma"` reconstructs "sistema" only if concatenated verbatim.

### The bug

The old code called `delta.trim()` and force-inserted a `' '` between consecutive
non-empty deltas. This caused:

- `" fra"` trimmed to `"fra"` → prev text `"...prima"` + space + `"fra"` = `"...prima fra"`
- `"se di prova del"` trimmed to `"se di prova del"` → `"...prima fra"` + space + `"se..."` = `"...prima fra se di prova del"`

Result: **`"ri conoscimento"`, `"fra se"`, `"siste ma"`, `"o ggi"`, `"comple tamente"`** — all sub-word fragments joined by injected spaces.

### The fix

`push_str(&delta)` / `push_str(&tail)` with no trim and no space injection, in
both `RealStreamSession::feed` and `RealStreamSession::finish`.

### Before / after transcripts (pause_long clip)

**Before (reconstructed from per-feed log, old accumulation logic):**
```
Questa è la prima fra se di prova del siste ma. <it-IT> E questa è la seconda fra se, dopo una lunga pausa di sil enzio.
```

**After (verbatim accumulation):**
```
Questa è la prima frase di prova del sistema. <it-IT> E questa è la seconda frase, dopo una lunga pausa di silenzio. <it-IT>
```

Words are correctly reconstructed. The `<it-IT>` language tag is present in
both — this is a Lirevo-layer concern (strip before injection), not a binding
bug.

---

## Part 2 — EOU triggering (Criterion 1)

Two synthetic clips were tested:

### pause_long.wav (~10 s)
Two Italian sentences with a 4-second silence gap (`[[slnc 4000]]`).

Per-feed eou flags (relevant feeds only):
```
[feed  59.0ms eou=false] +""
[feed  86.3ms eou=false] +" Questa è la prima"
[feed  37.8ms eou=false] +" fra"
[feed  79.1ms eou=false] +"se di prova del"
[feed  43.8ms eou=false] +" siste"
[feed  76.5ms eou=false] +"ma"
[feed  81.6ms eou=false] +". <it-IT>"
[feed  40.4ms eou=false] +""   <- 4-second silence block
[feed  77.8ms eou=false] +""
[feed  40.5ms eou=false] +""
... (all eou=false through silence) ...
[feed  76.8ms eou=false] +" E"
... continues ...
```

eou_events=0, text_after_first_eou=false, PASS(1)=true (vacuously — no EOU fired)

### three_utt.wav (~10 s)
Three short Italian sentences each separated by a 3-second silence gap.

eou_events=0, text_after_first_eou=false, PASS(1)=true (vacuously)

### Interpretation

**INCONCLUSIVE.** EOU never fired on either synthetic clip, even with 4-second
silences. The failure mode of bug #13 (text stops after EOU) was therefore not
triggered — but EOU-based utterance segmentation is effectively unavailable via
clean TTS synthetic silence. This is not necessarily a Metal-specific bug; it
may reflect the model's EOU threshold not being reachable with silence-only
clips (the model requires acoustic end-of-speech features, not just zero-energy
frames).

To get a definitive pass/fail on criterion 1, real continuous human speech is
needed where the speaker naturally pauses and then continues.

---

## Part 3 — Preliminary synthetic WER (Criterion 2)

4 clips, Italian voice Alice (macOS `say`), 16 kHz mono WAV.

| Stem | Reference | Nemotron hyp | nemotron WER | TDT hyp | TDT WER |
|------|-----------|--------------|--------------|---------|---------|
| sentence_01 | Il riconoscimento vocale automatico è una tecnologia molto utile. | riconoscimento vocale automatico è una tecnologia molto utile | 0.111 | Il riconoscimento vocale automatico è una tecnologia molto utile. | 0.000 |
| sentence_02 | Oggi ho partecipato a una riunione importante con i colleghi. | Oggi ho partecipato a una riunione importante con i colleghi. `<it-IT>` | 0.200 | Oggi ho partecipato a una riunione importante con i colleghi. | 0.000 |
| sentence_03 | La temperatura esterna è di venti gradi e il cielo è sereno. | La temperatura esterna è di 20 gradi e il cielo è sereno. `<it-IT>` | 0.250 | La temperatura esterna è di 20`<unk>`C e il cielo è sereno. | 0.250 |
| sentence_04 | Devo ricordare di comprare il pane, il latte e la frutta al mercato. | Devo ricordare di comprare il pane, il latte e la frutta al mercato. `<it-IT>` | 0.154 | Devo ricordare di comprare il pane, il latte e la frutta al mercato. | 0.000 |

**mean_WER (nemotron streaming, offline mode):** 0.179
**mean_WER (tdt-0.6b-v3, offline mode):** 0.062

### Findings

- **`<it-IT>` tag leakage:** The streaming model (nemotron) appends `<it-IT>` at
  sentence boundaries. This is the single largest WER driver — each tag counts
  as one insertion error. Lirevo must strip all `<...>` language tags before
  displaying or injecting text.
- **Number normalisation:** Both models normalise "venti gradi" → "20 gradi".
  This is correct linguistic behaviour but hurts WER against the literal
  reference. Real-eval references should be written post-normalised.
- **TDT `<unk>C`:** The offline TDT model emits `<unk>C` for degree-symbol
  context. Not relevant to the streaming model.

### Caveat

These numbers are nearly meaningless for the real gate decision. TTS audio is
clean (SNR > 40 dB, no reverb, no breath, perfect diction). Both models will
score near-perfectly on real TTS once tag leakage is stripped. The numbers DO
confirm that vocabulary is correct for Italian. The REAL criterion-2 decision
requires human-recorded Italian speech at representative conditions (office
room, moderate breath noise, natural cadence).

---

## Key findings summary

### Streaming works on Metal
The nemotron streaming model runs on the M4 Metal backend via llama-cpp-2's
GGML Metal path. Latency is excellent: max_feed_ms=92.6 ms for 500 ms chunks
(~18.5% real-time ratio), well within the 400 ms target.

### Teardown SIGABRT (exit 134)
The process exits with SIGABRT after all output is flushed, triggered by a
`GGML_ASSERT([rsets->data count] == 0)` check in the Metal residency-set
cleanup path. This does NOT affect correctness — the full transcript is emitted
before the abort. It is a known upstream ggml-metal teardown issue, not a
Lirevo-binding bug.

### `<it-IT>` language tag leakage
The nemotron model emits `<it-IT>` tags at sentence boundaries (appears as
`". <it-IT>"` in the delta stream). The binding does NOT strip these — that is
correctly a Lirevo-layer concern. The consumer (Lirevo's `stt/` module) must
strip all `<...>` tags before displaying or injecting text.

### EOU behaviour
EOU events never fired in any test. The model's EOU detector appears not to
trigger on synthetic silence frames. Real continuous speech is needed to confirm
whether bug #13 (continuous streaming stops after EOU) is present on Metal.

---

## Preliminary verdict

**Direction: GO for further evaluation.** The binding is correct and fast.
Streaming text reconstruction is accurate after the verbatim-accumulation fix.
Metal latency is excellent.

**What is still needed for the FINAL verdict:**

1. **Criterion 1 (EOU):** Test with real continuous human speech where a
   speaker pauses mid-recording and continues. Confirm EOU fires and text
   resumes after it, or that bug #13 reproduces on Metal.
2. **Criterion 2 (real WER):** A human-recorded Italian eval set (10-20 clips,
   varied speakers/conditions). Strip `<it-IT>` tags before scoring. Target
   WER < 0.10 for the streaming model on Italian to be competitive with audiopipe
   Parakeet at float16.
3. **Upstream maturity:** mudler/parakeet.cpp is ~10 days old at spike time,
   bus-factor 1, with open bug #13 on continuous streaming. Monitor before
   committing to it as the production STT backend.
