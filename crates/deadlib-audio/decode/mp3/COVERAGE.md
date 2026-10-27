# Foundation coverage, 2026-10-27

This milestone provides framing and state primitives. Native PCM synthesis and
encoding remain unimplemented; these results establish neither full MPEG
conformance nor PCM parity/performance. The complete requirements remain in
PLAN.md and the goal remains active.

Eight integration tests cover 2,160 header combinations, checked bit fields at
every position/width, every prefix of representative frames/side information,
CRC and its deliberate main-data exclusion, free-format padding/inference,
reservoir retention/recovery/reset, ID3 versions/unsynchronization/footer/bounds,
and Xing/Info/LAME/VBRI optional fields. The independent CRC check value is
`0xaee7` for `123456789` with MPEG's initial `0xffff` state.

The external structural gate generates 84 deterministic 4,096-sample WAV inputs
through LAME 3.100, then compares the native `inspect` example against libmad
0.15.1b's complete frame-decoding API. All nine MPEG-1/2/2.5 sample rates and
mono/stereo are covered, with CBR/VBR, protection on/off and twelve MPEG-1
400-kbit/s free-format outputs. All 732 frame offsets, counts, sizes, sample
rates, channels and nominal sample counts agree. Fixed-rate header bitrates
agree exactly; free-format native headers expose no invented bitrate. Native
side-info and reservoir acceptance succeeds for every frame. Xing frame counts
and LAME delay/padding account for exactly the 4,096 source samples.

Protected LAME Info frames can overlap the marker with the last side-information
bits. Unused Huffman selectors therefore remain raw values even when they name
a reserved codebook. Entropy decoding must reject a reserved book only when it
is used. MPEG header/side CRC remains checked, including these frames. This
compatibility behavior was required by an independently decoded generated case.

Commands, raw native/reference rows, fixture hashes, binary hashes, versions and
source hashes are retained in [the validation record](tests/validation/2026-10-27-foundation/validation.json)
and [structural comparisons](tests/validation/2026-10-27-foundation/reference-framing.json).
Source media and reference binaries remain in ignored scratch storage.

Reproduce on Linux/WSL with LAME and libmad development packages installed:

```sh
cargo build --locked -p deadlib-audio-decode-mp3 --example inspect --target-dir /path/to/mp3-build
cc -O2 crates/deadlib-audio/decode/mp3/tests/oracle/inspect.c -lmad -o /path/to/inspect-reference
python3 crates/deadlib-audio/decode/mp3/tests/reference_framing.py --native /path/to/mp3-build/debug/examples/inspect --oracle /path/to/inspect-reference --output /path/to/new-report-directory
```

Windows tests and all-target Clippy pass. The eight tests do not include
independent ISO conformance fixtures, mpg123/Symphonia PCM comparisons,
stateful synthesis, arbitrary streaming, seeking, fuzzing, encoding or
performance. Those are still mandatory later milestones. ID3 compressed or
encrypted payloads are retained, without claiming semantic decompression.
