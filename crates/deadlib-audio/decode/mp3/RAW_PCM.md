# Raw frame PCM milestone

`FrameDecoder` composes reservoir assembly, scalefactors, Huffman decoding,
requantization, short reordering, intensity/MS stereo, hybrid reconstruction and
polyphase synthesis. Output is planar f32, normalized and deliberately unclipped.
It includes metadata-frame samples and algorithmic delay. Calling `reset()`
clears reservoir and DSP history. Incomplete physical input is retryable; a
malformed complete frame is terminal until reset. Missing reservoir payload is
retained, but DSP history resets and subsequent output needs warmup.

The crate has zero production dependencies. Symphonia is an exact-version dev
dependency; libmad, mpg123 and LAME are external validation tools. No native
codec implementation was translated into production code.

## Gates and numerical criteria

The 84 deterministic LAME files from the foundation gate exercise all nine
sample rates, mono/stereo, CBR/VBR, protection on/off, and twelve MPEG-1
400-kbit/s free-format files. Compare every raw frame's rate, channels, sample
count and every unclipped f32 sample. Fixed limits, selected before comparison:
RMS `2e-6`, maximum absolute error `2e-5`, relative to full scale. These allow
fixed-point libmad rounding and floating-point DSP differences without masking
gain, phase or sample-position errors. All files must pass libmad and mpg123;
the 72 non-free-format files must also pass Symphonia 0.6.1. That Symphonia
version explicitly rejects free bitrate, which the harness records separately.

Eighteen legacy Layer III vector/PCM pairs are pinned to minimp3 mirror revision
`ea99364f61c14656440e8d77e9c233ccf3124633`. The mirror contains MPEG-1 header,
side-information, Huffman, free-format, mode-switch and compliance cases, five
`M2L3_*` cases, and the `l3-test45/46` intensity cases. Original ISO distribution
identity has not been independently certified. The manifest pins each mirror
URL, byte length and SHA-256; no external media is committed.

The supplied PCM is signed 16-bit little-endian. Compare normalized native PCM
after saturation to that representation. The functional gate requires RMS at
most `2^-15/sqrt(3)` and maximum at most `2^-14`: a conservative one-LSB
quantization allowance and the two-LSB peak criterion. Also report, without
conflating it with this gate, the stricter full-accuracy sine-sweep RMS threshold
`2^-15/sqrt(12)`. These fixtures and 16-bit references do not establish complete
ISO conformance. No fitted gain or arbitrary alignment search is used.

Several MPEG-1 reference files stop exactly one physical frame early. The
harness requires that exact length difference and scores every supplied sample
of their prefix. `l3-sin1k0db` has two known lead-in frames omitted by its
reference. The validation CLI reports and zero-fills those two missing-reservoir
frames, then the harness removes exactly those lead-in samples. This concealment
is validation CLI behavior; `FrameDecoder` itself reports the reservoir error.

The DSP gate now covers all 576 isolated frequencies in seven shapes: normal,
start, short, stop, mixed short, mixed start and mixed stop. Its 4,032 frames,
8,064 granules and 4,644,864 samples per hybrid/PCM stage retain the original
fixed limits from SYNTHESIS.md. Mixed start/stop flags select normal windows for
the lower long subbands and start/stop windows above them. Requantization and
scalefactor partitioning remain long-shaped in those cases.

Twenty-five integration tests additionally cover retry/reset/error behavior,
half-step scalefactor gain, preemphasis, subblock gain, stereo pan equations,
zero-width LSF partitions, independent short-window intensity bounds and the
uncoded top-band rule. The ISO header fixture exercises the reserved emphasis
value; header parsing now preserves that raw field. No de-emphasis filter is
currently applied for any emphasis value.

## Stereo and compatibility evidence

The bit-written stereo survey contains 90 streams and 29,310 frames. It varies
all eight three-bit pan positions, rate, long/short/mixed shape, MS enablement,
LSF intensity scale, right-channel bounds and uncoded top bands. It deliberately
reports reference disagreement as a failing universal-parity survey, not as a
passed all-decoder gate. These constructed sequences test reconstruction rules;
they are not a complete conforming-encoder corpus.
The subsequent independent equation audit in [STEREO.md](STEREO.md) passes all
90 streams and 40.6 million sample values, while retaining those reference
disagreements explicitly.

The standard's tangent and LSF power equations drive the pan calculation.
Zero-width LSF factors represent position zero. The uncoded top band inherits
the preceding intensity pan only when that preceding band is intensity coded;
otherwise it uses a balanced default. The independent `l3-test45/46` PCM pairs
exercise this distinction. References disagree on these and other intensity
edge cases, so matching one decoder alone is insufficient evidence.

For MPEG-2.5, 11.025/12 kHz use the documented LSF-style band tables. The 8 kHz
mixed profile has six long bands spanning 72 lines/four subbands, followed by
short band three. The current implementation follows that geometry, including
alias reduction across its long subbands. Synthetic 8 kHz mixed-block PCM
disagrees substantially with libmad, mpg123 and Symphonia. The subsequent
independent all-frequency equation audit in [MPEG25.md](MPEG25.md) confirms the
native 1997 profile and preserves the reference disagreements explicitly.
Ordinary long/short LAME files at 8 kHz pass the generated-file gate.

Band boundaries are factual numeric data from ISO simulation tables and the
pinned LAME numeric table, with provenance in tests/data/bands.json. The 24 kHz
long boundary is 332 per MPEG-2 Table B.2, correcting 330 in the old simulation
decoder table. The 1997 [Layer3 syntax and decoding document](https://mp3guessenc.sourceforge.io/MPEG%20Layer3%20Bitstream%20Syntax%20and%20Decoding.pdf)
describes mixed start/stop windows, zero-width intensity and MPEG-2.5 geometry.
The later [ISO Layer3 intensity clarification](https://cdn.standards.iteh.ai/samples/39584/696c43b397e24f5ea9b6f80f8b193464/ISO-IEC-14496-3-2001-Amd-3-2005.pdf)
also describes the top-band distinction. Published-edition/corrigendum
reconciliation remains part of the full conformance audit.

## Reproduction and remaining work

Build `examples/decode.rs` and `examples/dsp.rs` in release mode. Compile the
public-API wrappers in tests/oracle with libmad/libmpg123 development packages.
Run tests/reference_pcm.py on the foundation corpus; run tests/fetch_iso.py to
fetch the pinned fixture manifest into a new directory, then tests/reference_iso.py.
Run tests/reference_dsp.py for seven-shape DSP parity. tests/reference_stereo.py
generates the stereo survey from tests/data/bands.json; reference_pcm.py with
`--survey` records its disagreements without a universal-parity assertion.

Evidence is retained in tests/validation/2026-10-27-raw-pcm. The goal remains
active: production file/streaming decoding, gapless trimming, exact seeking,
malformed-stream/fuzz campaigns, the 8 kHz mixed case, native encoding and
measured performance optimization remain unfinished. These gates concern raw
PCM correctness within the stated corpus, not completion of the codec.
