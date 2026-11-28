# ITG3 AVI optimization pass

## Method

Windows x86-64, Intel Xeon E5-2696 v4, Rust 1.100.0. Both revisions use
`rustc --edition 2021 -C opt-level=3 -C codegen-units=1`, baseline SSE2,
and the same original in-memory AVI input. The original source is
`8353114ae`; each subsequent comparison uses the preceding accepted revision.
The benchmark runs both versions in one process on logical CPU 12 and
alternates order by round and clip. Targeted retests use CPUs 14 and 4.
`QueryThreadCycleTime` counts thread cycles; frequency and contention can
still introduce variance. An A/A comparison of two identical builds
returned +0.55% pipeline cycles and -0.50% scanner cycles.

Sixteen complete files were selected from the 356-file ITG3 collection using
evenly spaced file-size quantiles, including the smallest and largest.
The repeated subset contains 40,391,316 input bytes and 8,362 decoded frames.
Sampled files are 320x240 Xvid MPEG-4 Visual. The original AVI container is
passed directly to `decode_avi`, including parsing, demux, decoder setup,
reconstruction, display-order callback, drain and destruction. File I/O
and exact output verification are outside the timed region. The allocation
pass counts successful alloc/alloc_zeroed/realloc requests and requested bytes.
It does not measure frees, peak live allocation, or RSS.

The isolated scanner traverses the actual track-00 AVI video packet payloads
16 times per measurement; demuxing the packets for this isolated test is
outside the measured region. Intra-block inputs were captured from the
original decoder at every 512th invocation: 3,337 blocks, including
282 uncoded blocks without AC prediction. All sampled blocks had separately
coded DC. The isolated intra replay measures 64 repetitions per round; the
direct DC lookup replay uses 512. The captured predictor state, bit position,
and pixel samples are compared directly before each kernel timing batch.
The benchmark hooks are compiled only into temporary source copies.
No AVI files or captured block data are included in the repository.

## Per-change measurements

The pipeline totals include five paired rounds, 41,810 decoded frames
per revision. The isolated scanner traverses 3,209,487,040 payload bytes;
the mixed intra-block kernel traverses 2,135,680 blocks (ten pairs).
The targeted DC-only fill uses 360,960 blocks (20 pairs); the isolated
DC lookup uses 25,628,160 coefficient decodes (15 pairs).

| Change / operation | Cycles before | Cycles after | Change |
| --- | ---: | ---: | ---: |
| Start-code scanner | 5,141,726,048 | 1,515,129,690 | -70.53% |
| Scanner pipeline | 29,286,790,354 | 29,313,330,327 | +0.09% |
| DC-only fill (affected blocks) | 151,221,674 | 52,842,867 | -65.06% |
| DC-only fill (all intra blocks) | 1,852,739,577 | 1,791,469,889 | -3.31% |
| DC-only fill pipeline | 29,213,730,650 | 28,910,340,368 | -1.04% |
| Fused DC lookup | 1,959,019,053 | 1,205,667,409 | -38.46% |
| Fused DC lookup (all intra blocks) | 1,780,508,278 | 1,750,260,744 | -1.70% |
| Fused DC lookup pipeline | 28,345,140,619 | 28,315,287,252 | -0.11% |

| Change / operation | Throughput before | Throughput after | Change |
| --- | ---: | ---: | ---: |
| Scanner (decimal MB/billion cycles) | 624.20 | 2118.29 | +239.36% |
| Scanner pipeline (frames/billion cycles) | 1427.61 | 1426.31 | -0.09% |
| DC-only fill (million blocks/billion cycles) | 2.387 | 6.831 | +186.17% |
| DC-only fill mixed (million blocks/billion cycles) | 1.153 | 1.192 | +3.42% |
| DC-only fill pipeline (frames/billion cycles) | 1431.18 | 1446.20 | +1.05% |
| Fused DC lookup (million coefficients/billion cycles) | 13.082 | 21.256 | +62.48% |
| Fused lookup mixed (million blocks/billion cycles) | 1.199 | 1.220 | +1.73% |
| Fused lookup pipeline (frames/billion cycles) | 1475.03 | 1476.59 | +0.11% |

Every isolated allocation pass reports zero heap calls and zero requested bytes.
Each paired pipeline allocation pass over the 16 complete clips records
exactly 8,678 heap calls and 12,947,904 requested bytes on both revisions.
All 16 files matched visible pixels, frame dimensions, picture type and
presentation time exactly after each change.

The scanner replaces bytewise checks with safe 64-bit candidate detection.
Its pipeline difference is inconclusive; the two apparent per-file outliers
were retested on CPU 14. `058_JumpBack` improved 0.25%; `321_JumpBack`
varied by +1.29% aggregate with opposite transient spikes in individual
pairs, and its median pair improved slightly. The direct DC-only fill
extends the existing MP42 shortcut to Visual, preserving distinct coefficient
and predictor clipping. Its benefit is concentrated on 282 of the 3,337
sampled blocks. Three apparent pipeline outliers were retested: the
434 and 031 clips were effectively unchanged; the 657 clip showed +1.53%
on CPU 14, but an identical-build control varied +0.96% there and the
eleven-pair CPU 4 retest improved 1.77%. The aggregate gain of 1.04% is
small and close to normal variation. The fused lookup shares the existing
MP42 short table generator and adds two 2 KiB static Visual tables. No clip
regressed by more than 0.71% in its repeated pipeline comparison.

All three changes are safe Rust; they add no dependencies or unsafe blocks.
The release tests include a start-code boundary differential test, Visual
DC predictor/clipping checks, exhaustive DC lookup comparisons over all
65,536 two-byte prefixes and their one-byte truncations, and existing
MPEG-4 Visual and MP42 reference-frame tests.

## Full ITG3 collection

The original and final builds decoded all 356 AVI files (796,563,156
input bytes) successfully. A direct comparison covered 222,701 decoded
frames, 25,655,155,200 visible pixel bytes and 10,689,648 frame metadata
bytes, with zero differences.

| Revision | Pipeline cycles | Frames/billion cycles |
| --- | ---: | ---: |
| Original | 135,560,090,085 | 1642.82 |
| Final | 134,183,454,738 | 1659.68 |
| Change | -1.02% | +1.03% |

The separate allocation pass over all 356 files records exactly 229,320
heap calls and 280,161,792 requested bytes in each revision, with identical
per-file allocation counts and byte totals. See `full-allocations.csv`,
`full-cycles.csv` and `full-exact.csv`.

This supplementary single-pair-per-file run confirms no measurable
aggregate regression; the observed cumulative improvement is small and
near the measured benchmark variance. The repeated per-change comparisons
above remain the acceptance measurements.

All ten release tests passed, as did rustfmt and the AArch64 Windows
compile check. Strict Clippy reports the pre-existing
`clippy::large_enum_variant` warning in `avi.rs` on both the original and
final source; the final all-target check passes with that single warning
allowed. The enum is unchanged to avoid adding heap indirection.

## Reproduce

Export the relevant `src/*.rs` files from each revision into separate
temporary directories. Create a UTF-8 manifest with one absolute AVI path
per line; the selected input order is in `scan-cycles.csv`. The output
directory and trace should be outside the repository.

```powershell
$runner = 'crates/deadlib-video-mpeg4/bench/itg3.ps1'
& $runner -Before $original -After $original -Inputs $selected -Work $aa -Operation scan -Rounds 2 -SkipVerify
& $runner -Before $original -After $original -Inputs $selected -Work $capture -Capture
& $runner -Before $original -After $scanner -Inputs $selected -Work $scanRun -Operation scan
& $runner -Before $scanner -After $fill -Inputs $selected -Work $fillRun -Operation intra -Trace "$capture/intra.bin"
& $runner -Before $fill -After $final -Inputs $selected -Work $lookupRun -Operation dc -Trace "$capture/intra.bin"
& $runner -Before $original -After $final -Inputs $allClips -Work $fullRun -Rounds 1 -Full
```

The targeted DC-only sample can be reproduced by filtering the capture
records for the coded and predict_ac flags both zero, then running the
intra kernel binary on the filtered trace. Records consist of five flag
bytes, a little-endian i32 quantizer, three 16-i32 predictors, a little-endian
u16 payload size, then the payload bytes. `-Full` compares all visible output
bytes and metadata directly, including any output before an error, and takes
one pipeline pair per successfully decoded clip.
