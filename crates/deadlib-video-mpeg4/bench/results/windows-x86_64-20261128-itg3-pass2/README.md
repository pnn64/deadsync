# ITG3 AVI optimization pass 2

## Method

The MPEG-4 Visual baseline is the implementation at `cc61d93d8` (unchanged
from the first ITG3 pass, `2dff56533`). Each candidate is compared with the
preceding accepted revision, using Rust 1.100.0 on the Intel Xeon E5-2696 v4
and identical `--edition 2021 -C opt-level=3 -C codegen-units=1` settings.
Both builds run in the same process, alternating order by round and clip
on logical CPU 12. Targeted pipeline retests use logical CPU 14.

The repeated benchmark uses exactly the same 16 complete ITG3 AVI files
as the first pass: 40,391,316 input bytes and 8,362 decoded frames.
The timed operation is `decode_avi`, including RIFF parsing and demux,
decoder setup, reconstruction, frame callback, drain and destruction.
File I/O, output verification and trace generation are outside timing.
CPU cycles use Windows QueryThreadCycleTime; the independent allocator
meter counts successful allocation, zeroed-allocation and reallocation
requests and full requested bytes. It does not measure peak live memory.

The A/A control uses two builds of identical source on this subset,
three paired rounds: 17,182,264,203 versus 17,102,172,804 cycles (-0.47%).
Other system activity and frequency variation can still affect cycle
measurements. All recorded pairs are retained; isolated measurements
are the strongest evidence for changes whose whole-pipeline effect
is close to normal variance.

The temporary capture build samples every 512th invocation of the relevant
operation: 3,337 intra blocks, 3,046 complete coefficient arrays before
inverse quantization, and 9,884 motion components (f_code 1: 7,847;
2: 1,024; 3: 995; 4: 18). The AC-prediction trace contains 1,244 predicted
blocks, of which 1,083 have equal quantizers (87.1%). The targeted AC trace
contains only these 1,083 records. Quantization replay maps all 64
coefficients; motion replay includes parsing the component from the
captured bytes and wrapping its predictor; intra replay includes decoding,
prediction and reconstruction. Each replay checks exact results against
the preceding revision outside timing. Only temporary source copies
include capture and replay hooks; no media or captured inputs are committed.

## Per-change measurements

Pipeline totals cover five pairs per clip, 41,810 frames per revision.
Isolated quantization totals cover 7,797,760 blocks (10 pairs, 256
repetitions per sample); the mixed intra replay covers 2,135,680 blocks
(10 pairs, 64 repetitions). Targeted AC replay covers 1,039,680 affected
blocks (15 pairs, 64 repetitions), and motion replay covers 12,651,520
components (10 pairs, 128 repetitions).

| Change / operation | Cycles before | Cycles after | Change |
| --- | ---: | ---: | ---: |
| Signed inverse quantization | 1,622,290,246 | 1,277,338,320 | -21.26% |
| Quantization, mixed intra | 1,741,223,701 | 1,654,128,954 | -5.00% |
| Quantization, pipeline | 28,518,402,091 | 27,981,158,916 | -1.88% |
| Equal-quantizer AC, affected intra | 854,242,640 | 778,215,074 | -8.90% |
| Equal-quantizer AC, mixed intra | 1,716,812,895 | 1,689,530,789 | -1.59% |
| Equal-quantizer AC, pipeline | 28,252,965,073 | 27,786,307,667 | -1.65% |
| Bitmask motion wrapping | 728,204,207 | 660,653,074 | -9.28% |
| Motion wrapping, pipeline | 28,594,780,665 | 28,136,577,194 | -1.60% |

| Change / operation | Throughput before | Throughput after | Change |
| --- | ---: | ---: | ---: |
| Quantization (million blocks/billion cycles) | 4.807 | 6.105 | +27.01% |
| Quantization mixed (million blocks/billion cycles) | 1.227 | 1.291 | +5.27% |
| Quantization pipeline (frames/billion cycles) | 1466.07 | 1494.22 | +1.92% |
| AC affected (million blocks/billion cycles) | 1.217 | 1.336 | +9.77% |
| AC mixed (million blocks/billion cycles) | 1.244 | 1.264 | +1.61% |
| AC pipeline (frames/billion cycles) | 1479.84 | 1504.70 | +1.68% |
| Motion (million components/billion cycles) | 17.374 | 19.150 | +10.22% |
| Motion pipeline (frames/billion cycles) | 1462.15 | 1485.97 | +1.63% |

Every isolated allocation measurement reports zero heap calls and zero
requested bytes. Each of the three paired pipeline allocation comparisons
over the 16 selected videos records exactly 8,678 heap calls and
12,947,904 requested bytes on both revisions. Each candidate also matches
every visible output byte and frame metadata field on all 16 clips.

The signed quantization identity avoids computing an unsigned magnitude
and then applying the sign to the entire result. It replaces two production
lines with one. Both clipping modes are checked over all signed 16-bit
levels and legal quantizers. All five aggregate pipeline rounds improve
(-1.55% to -2.33%); the two apparent per-file outliers retest at -0.17%
and -0.86% on CPU 14.

The AC shortcut reuses the already quantized predicted coefficient when
the neighbor and current quantizer match, skipping seven integer divisions
and associated arithmetic on affected blocks. Different quantizers still
use the original rounding and clipping. All five aggregate pipeline rounds
improve (-0.56% to -3.65%), with one per-file result of +0.23% within noise.

Motion wrap uses a mask instead of Euclidean remainder since every legal
wrap interval is a power of two. Zero motion and all other decoding
behavior remain unchanged. Four of five aggregate pipeline rounds improve;
one varies +0.32%. Large individual cycle spikes affect some samples,
so the exact 1.60% aggregate reduction is uncertain. CPU 14 retests of
the two small per-file outliers give -0.69% and +0.76%, the latter within
normal paired variation. A release test checks both wrap boundaries and
zero delta for every legal f_code from 1 through 7.

All three changes use safe Rust and add no dependencies or unsafe blocks.
All 13 release tests pass. The Windows shell transiently failed to
initialize with error 800705af during retest setup; the command was
retried successfully, without losing benchmark data.

## Full ITG3 collection

Both revisions decoded all 356 original AVI files (796,563,156 input
bytes) successfully, producing 222,701 frames. Direct comparison covered
25,655,155,200 visible pixel bytes and 10,689,648 frame metadata bytes
without differences.

| Revision | Pipeline cycles | Frames/billion cycles |
| --- | ---: | ---: |
| Pass 1 baseline | 135,064,790,119 | 1648.85 |
| Pass 2 final | 131,519,160,534 | 1693.30 |
| Change | -2.63% | +2.70% |

The separate allocation pass over all 356 files records 229,320
heap calls and 280,161,792 requested bytes for each revision. Every
individual file has identical allocation counts and requested bytes.
See `full-allocations.csv`, `full-cycles.csv` and `full-exact.csv`.

This supplementary single-pair-per-file measurement compares the
two revisions directly rather than adding the independent optimization
percentages. Some individual pairs show transient variance, so the exact
size of the cumulative gain is uncertain.

Rust formatting, the AArch64 Windows compile check, and strict Clippy
with only the previously confirmed `large_enum_variant` warning
allowed all pass. The original AVI decoder enum remains unchanged.

## Reproduce

Use the same UTF-8 AVI manifests and source exports as the first pass.
Use temporary directories outside the repository. The baseline, quantization,
AC and motion source revisions are `cc61d93d8`, `b411b7ce3`,
`c4a4cfbab`, and `84a337fdf`, respectively.

```powershell
$runner = 'crates/deadlib-video-mpeg4/bench/itg3.ps1'
& $runner -Before $original -After $original -Inputs $selected -Work $aa -Operation pipeline -Rounds 3 -SkipVerify
& $runner -Before $original -After $original -Inputs $selected -Work $capture -Capture -CaptureExtra
& $runner -Before $original -After $quant -Inputs $selected -Work $quantMixed -Operation intra -Trace "$capture/intra.bin"
& $runner -Before $original -After $quant -Inputs $selected -Work $quantDirect -Operation quant -Trace "$capture/intra.bin.quant" -KernelsOnly
& $runner -Before $quant -After $ac -Inputs $selected -Work $acRun -Operation intra -Trace "$capture/intra.bin"
& $runner -Before $ac -After $motion -Inputs $selected -Work $motionRun -Operation motion -Trace "$capture/intra.bin.motion"
& $runner -Before $original -After $motion -Inputs $allClips -Work $fullRun -Rounds 1 -Full
```

For the targeted AC replay, filter the intra records to those with the
predict_ac flag set and the selected neighbor's quantizer equal to the
current quantizer, using the original MPEG-4 DC gradient comparison.
The trace record format is described in the first pass report and the
capture hooks. Run the intra kernel binary with `intra 15 12` and the
filtered trace path; its allocation counterpart uses one round.
