# Native hybrid and polyphase synthesis

`Hybrid` implements alias reduction, normal/start/stop/three-short inverse MDCT,
window placement, retained overlap and frequency inversion. Input is reordered
frequency data; output is 18 time-major sets of 32 subband values. Mixed blocks
use normal windows below the caller's long-subband boundary. `Synthesis` maps
each set into 32 PCM samples using the MPEG cosine matrix, U-vector/window
mapping and a sixteen-block ring history. Both states reset without allocation.

The initial kernels use f64 arithmetic and flat arrays. They are correctness
baselines; optimized transforms, precision/storage tradeoffs and full decode
performance remain later measured work. Immutable cosine/window tables are
generated from the MPEG mathematical equations, rather than translated from a
reference implementation. No runtime trigonometric initialization is required.

`scripts/generate_dsp.py` reads only the numeric `dewindow` table from the ISO
simulation distribution described in HUFFMAN.md. That member is
`dist10/lsf/decoder/tables/dewindow`, SHA-256
`0998ea42d245e615c4bac39afe80f5877bd28db084b3992cb0c95d8c66bf5819`.
All 512 indices are checked. Coefficients retain the table's published decimal
precision; generated cosine coefficients retain their exact f64 values with
shortest roundtripping literals. The generated-table-only Clippy exception
preserves evaluated cosine values near familiar constants. Generation is
byte-reproducible, with input and result hashes recorded.

Sixteen integration tests pass, including silence, finite synthesis history,
overlap decay, reset repeatability and invalid shapes. The independent DSP gate
then encodes 2,880 MPEG-1 mono frames with isolated spectral coefficients at
every frequency line, for each long/start/stop/short/mixed shape. Each signal
granule is followed by a zero granule; the sequence retains actual hybrid and
polyphase history across frames. Some constructed window transitions are
unusual; this is equation/state coverage, not the ISO conformance corpus.

The native validation CLI receives the explicitly reordered spectra and runs
the production kernels. Unmodified libmad public frame/synthesis APIs decode
the corresponding MPEG frames. Both return exactly 5,760 granules and
3,317,760 samples at each stage. Maximum absolute hybrid error is `4.019e-8`;
maximum PCM error is `1.907e-7`. Limits fixed before execution are hybrid RMS
`2e-7` / maximum `1e-6`, and PCM RMS `4e-7` / maximum `2e-6`, in normalized
units. These limits allow fixed-point reference rounding while remaining far
below a 16-bit sample quantum. All cases pass. This compares complete DSP-stage
outputs, without claiming a complete native MP3 file decoder yet.

```sh
python3 crates/deadlib-audio/decode/mp3/scripts/generate_dsp.py --dewindow /path/to/original/dewindow
cargo build --locked --release -p deadlib-audio-decode-mp3 --example dsp --target-dir /path/to/mp3-build
cc -O2 crates/deadlib-audio/decode/mp3/tests/oracle/dsp.c -lmad -o /path/to/dsp-reference
python3 crates/deadlib-audio/decode/mp3/tests/reference_dsp.py --native /path/to/mp3-build/release/examples/dsp --oracle /path/to/dsp-reference --output /path/to/new-dsp-report
```

The [validation record](tests/validation/2026-10-27-synthesis/validation.json)
retains tests, Clippy, generator reproducibility, source hashes and per-stage
reference errors. The deterministic input and full native/reference traces
remain in ignored scratch storage, identified by SHA-256 and replay commands.
Requantization, stereo and full framed PCM composition, all reference-decoder
comparisons, streaming/gapless seeking, encoding and performance remain required.
