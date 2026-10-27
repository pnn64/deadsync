# Floor-zero parameter and reader parity

The target is exact reference behavior, counts, positions and channel order,
with the original justified floating PCM tolerances. This follow-up closes
specific amplitude-reader, Bark-grid, incomplete-LSP and IEEE-class differences.
It does not establish complete Vorbis parity.

## Published fields and explicit reference behavior

`Floor0Behavior::Vorbis` remains the default: the amplitude field is unsigned
and supports every width from zero through 63. `Floor0Behavior::Libvorbis`
selects the pinned libvorbis 1.3.7 reader and arithmetic behavior through
`DecodeOptions`, `OggOptions` and `SeekOptions`, including reset and selected
setup reparsing. Empty-book acceptance and IEEE propagation remain independent
options.

| Operation | Published/default policy | Explicit pinned-reference policy |
| --- | --- | --- |
| Amplitude widths 0..31 | Unsigned field | Same unsigned values |
| Width 32, high bit set | Positive unsigned amplitude | Signed integer is nonpositive; floor absent after 32 consumed bits |
| Width 32, positive signed value | Unsigned normalization denominator | Observed pinned x86 normalization denominator is zero |
| Width 33..63 | Read both field parts | Sticky EOF from unsupported libogg read width; following reads fail |
| Bark grid | f64 equation | f32 products/frequency/scale around double transcendental operations |
| Incomplete LSP codeword | Native sticky-read policy | Consume available Huffman prefix and conceal the floor without sticky EOF |

The C width-32 normalization expression contains an undefined signed shift.
The explicit policy models the observed pinned x86 build deterministically with
wrapping unsigned arithmetic. It does not claim a portable C-language result or
the behavior of every libvorbis build. IEEE propagation must also be enabled to
retain its NaN/infinity results. The finite-output policy still rejects a
nonfinite final curve or uses its existing wider-range finite synthesis.

Before EOF, direct component tests compare exact consumed-bit positions. After
EOF, the public native reader retains its documented last-successful position;
libogg exposes an internal endbyte/endbit sentinel. Logical sticky EOF and all
following channel/packet decisions are checked independently of that API
representation. A one-byte witness (`0d`, three prefix bits, amplitude width
three) preserves the distinct incomplete-Huffman behavior without sticky EOF.

## Numerical and behavioral evidence

The direct helper invokes the unmodified pinned floor-zero backend callbacks;
it does not reproduce their algorithm. It captures presence, reader state,
amplitude, every integer Bark index and each curve value. The component matrix
has 344 setups and 39,946 cases, including all amplitude widths, high orders,
odd/even and partial VQ vectors, sequence books, all relevant selector edges,
empty books and truncated prefixes. It checks 3,091,639 finite curve values,
104,041 nonfinite classes and 12,818 EOF cases.

A retained independent C grid contains 1,088 Bark maps and 1,436,160 integer
indices across 17 rates, 16 grid sizes and all selected short/long block bins.
The explicit reference policy matches every index. The f64 published/default
equation differs at 312 indices, proving that a PCM tolerance cannot substitute
for checking these integer decisions.

The packet matrix covers 1,411 configurations and 31,209 cases (156,045 packet
decisions), complete short/long transitions,
byte-prefix truncation and later recovery, multiple channels/submaps, coupling,
maximum order and signed/unsupported widths. It compares packet decisions,
block sizes, frame counts and cumulative positions exactly. Finite floor-zero
PCM retains peak/RMS bounds `5e-5` / `3e-6` multiplied by
`max(1, reference peak)`. NaN classes and infinity signs match at each position;
NaN payload bits are unspecified. The matrix checks 25,123,840 finite values and
2,274,784 nonfinite classes.

One packet regression contains an isolated infinite spectral value. The
independently derived f64 FFT can retain infinities where the reference real
butterfly transform produces NaNs. Explicit IEEE packet synthesis now emits
an all-NaN block for any nonfinite spectrum before windowing/overlap. Direct
reference tests place positive infinity, negative infinity and a quiet NaN at
every spectral position at every legal block size: 24,480 inputs and
134,215,680 all-NaN output samples. The standalone transform retains its own
arithmetic. Finite-input overflow behavior is not claimed exhaustively by this
nonfinite-input check.

Seven procedural PCM goldens retain the reader, Bark, empty-book, coupling and
isolated-infinity regressions. Raw reset, pull decoding, one-/17-byte/whole-input
push/reset and both index pre-roll strategies compare complete PCM bits after
canonicalizing NaN payloads. Their 196 native seeks include block boundaries,
endpoints and backward seeks. Independently, seven C Ogg decodes and 98 repeated
C seeks match their raw PCM suffixes after NaN payload canonicalization.

## Reproduction

Set `DEADLIB_LIBVORBIS_ORACLE` to the pinned helper, then run:

```powershell
cargo test -p deadlib-audio-decode-vorbis --release --lib floor_zero_ -- --include-ignored --skip regenerate_floor0_reference_goldens --nocapture
cargo test -p deadlib-audio-decode-vorbis --release --test floor_zero_parameters -- --include-ignored --skip regenerate_floor0_pcm_golden --nocapture
```

Normal tests read the three retained fixtures and never regenerate them.
Explicit manual generators are `regenerate_floor0_reference_goldens` and
`regenerate_floor0_pcm_golden`, each requiring `--ignored` and the external
helper. Source builders are independent of the native encoder.

Reference sources remain libogg 1.3.6 at
`be05b13e98b048f0b5a0f5fa8ce514d56db5f822` and libvorbis 1.3.7 at
`0657aee69dec8508a0011f47f3b69d7538e9d262`. Production depends only on the native
Ogg crate. The diagnostic generator now copies the complete codec into an
isolated crate so private bit-reader helpers retain their privacy boundary;
that generated driver is compiled/linted only. No application integration or
performance measurement is part of this milestone. The [parity ledger](PARITY.md) retains the wider unfinished work.

The three 300-second AddressSanitizer campaigns (headers, raw packets and
streaming) complete 265,903 executions with no failure artifacts or slow units.
They inherit the preceding corpus and add 105 procedural seeds covering both
floor-zero policies and their independent IEEE/empty-book flags. Source and
binary hashes are rechecked against the tested inputs.

The [checks and provenance](validation/2026-10-27-floor0-parameters-checks.json),
[validation probe](validation/2026-10-27-floor0-parameters-validation-probe.py),
[independent reference probe](validation/2026-10-27-floor0-parameters-reference-probe.py)
and [reference results](validation/2026-10-27-floor0-parameters-independent-reference.json)
retain commands, original failure logs, small procedural witnesses, all three
golden hashes, corpus reports and sanitizer evidence. The original source
snapshot is verified against the preceding codec milestone `1a91a2ab0`.

Final gates pass: 165 release tests with external reference/corpus checks, 118
normal tests in an independent offline workspace (54 external/manual tests
ignored there), both default/all-feature Clippy runs, and two fuzz-harness unit
tests plus lint. Earlier configuration, mutation, empty-book, extent and residue
matrices retain their exact scope and original tolerances, including
1,836,059,120 residue-geometry PCM comparisons. These gates establish this
milestone's scope; complete parity remains unfinished.
