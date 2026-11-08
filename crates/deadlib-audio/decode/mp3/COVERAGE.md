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

## Scalefactor milestone

The next milestone adds MPEG-1 long/short/mixed scalefactors and all sixteen
SCFSI masks, plus every MPEG-2 LSF compression code (0..511), ordinary and
intensity parameter sets, and long/short/mixed partitions. The numeric LSF
partition counts and formulas were checked directly against ISO/IEC 13818-3
section 2.4.3.2, printed pages 23-24, including visual inspection of the tables.
Transmitted scalefactor widths remain available for later intensity processing;
the final long/short bands remain their implied zero values.

Three additional tests exercise 6,144 LSF version/compression/intensity/shape
combinations, all MPEG-1 compression/shape combinations, all SCFSI masks and
truncated/invalid part2 bounds. A failed read leaves the input cursor unchanged.
Part2 scalefactors also parse within their declared part2_3 fields for all 84
LAME files (1,422 granule/channel partitions). The framing/libmad comparisons
and delay/padding accounting continue to pass. Run the structural gate above
with `--scalefactors` to reproduce this additional check.

This is syntax and bit-consumption evidence, not an independent comparison of
every decoded scalefactor value or synthesized PCM. Huffman values, intensity
stereo reconstruction and complete numerical parity remain subsequent gates.
See [the scalefactor validation record](tests/validation/2026-10-27-scalefactors/validation.json)
for all eleven tests, Clippy, source hashes and raw generated-file rows.
