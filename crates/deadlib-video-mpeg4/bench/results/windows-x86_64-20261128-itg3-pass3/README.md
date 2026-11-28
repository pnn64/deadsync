# ITG3 AVI optimization pass 3

## Method

The baseline is `31689b487`, the end of the second ITG3 optimization pass.
Each candidate uses the preceding accepted decoder as its baseline. Both
revisions use Rust 1.100.0 on the Intel Xeon E5-2696 v4, with identical
`--edition 2021 -C opt-level=3 -C codegen-units=1` settings. They run in
one process on logical CPU 12 with alternating order by round and clip.
Targeted retests use CPUs 14 and 4. The complete AVI decode pipeline
includes container parsing, decoder setup, reconstruction, frame callback,
drain and destruction. File I/O and exact output verification are outside
timing. Allocation counts and requested bytes are measured separately.

The repeated benchmark uses the same sixteen original ITG3 AVI files as
the preceding two passes (40,391,316 bytes, 8,362 decoded frames). The
identical-build A/A control over three pairs per clip records
17,634,537,544 versus 17,311,522,468 cycles (-1.83%). Concurrent system
activity and CPU frequency variation contribute substantial noise in
individual pairs; all raw measurements, including spikes, are retained.

The motion replay uses 9,884 previously captured actual motion-component
inputs. Edge and transform traces sample every 512th invocation of the
original decoder on the same videos: 898 edge tiles, of which 499 only
cross the vertical boundary, and 12,388 transform blocks, of which 1,434
contain only DC. The edge trace includes the original source plane, origin
and tile dimensions; the replay includes the complete edge-extension
operation and initializes the destination tile. The DC prediction replay
uses the chosen predictor and scale from all 3,337 captured intra records.
The benchmark checks exact output for each replay outside timing.
Trace hooks are compiled only into temporary source copies. No source
videos, captured pixels or coefficient traces are committed.

## Per-change measurements

Each pipeline comparison uses five paired rounds, 41,810 decoded frames
per revision. The isolated motion replay uses 12,651,520 components
(ten pairs, 128 repetitions per sample), edge replay uses 2,298,880 tiles
(ten pairs, 256 repetitions), and DC predictor replay uses 34,170,880
predictions (ten pairs, 1,024 repetitions). Mixed intra replay covers
2,135,680 blocks (ten pairs, 64 repetitions).

| Change / operation | Cycles before | Cycles after | Change |
| --- | ---: | ---: | ---: |
| Fused motion codes | 651,520,789 | 342,184,701 | -47.48% |
| Motion pipeline | 27,944,883,534 | 27,378,648,988 | -2.03% |
| Row edge extension (final version) | 1,206,205,628 | 793,116,488 | -34.25% |
| Edge pipeline (final version) | 28,269,199,129 | 27,809,960,981 | -1.62% |
| Reciprocal DC prediction | 325,553,617 | 115,405,858 | -64.55% |
| DC prediction, mixed intra | 1,645,872,231 | 1,634,594,104 | -0.69% |
| DC prediction pipeline (CPU 12) | 27,184,860,810 | 27,465,557,593 | +1.03% |
| DC prediction pipeline (CPU 14) | 27,756,009,185 | 27,622,501,833 | -0.48% |

| Change / operation | Throughput before | Throughput after | Change |
| --- | ---: | ---: | ---: |
| Motion (million components/billion cycles) | 19.418 | 36.973 | +90.40% |
| Motion pipeline (frames/billion cycles) | 1496.16 | 1527.10 | +2.07% |
| Edge (million tiles/billion cycles) | 1.906 | 2.899 | +52.08% |
| Edge pipeline (frames/billion cycles) | 1478.99 | 1503.42 | +1.65% |
| DC prediction (million predictions/billion cycles) | 104.962 | 296.093 | +182.09% |
| DC mixed (million blocks/billion cycles) | 1.298 | 1.307 | +0.69% |
| DC pipeline CPU 12 (frames/billion cycles) | 1537.99 | 1522.27 | -1.02% |
| DC pipeline CPU 14 (frames/billion cycles) | 1506.34 | 1513.62 | +0.48% |

All isolated operations have zero heap calls and zero requested bytes.
Each candidate has exactly 8,678 heap calls and 12,947,904 requested bytes
before and after decoding the sixteen clips, with exact visible pixels and
frame metadata. These allocation measurements count successful alloc,
alloc_zeroed and realloc requests and requested bytes, not peak memory.

The motion lookup adds seven 2 KiB static tables, one for each legal residual
width. The table combines code length, signed magnitude and residual when
all fit in the ten-bit prefix. The original long path handles everything
else. The apparent outlier 083 measured +15.16% initially, +3.10% on CPU 14,
and -4.67% on CPU 4. An identical-build CPU 4 control on that clip varied
by -8.70%. Other targeted retests measured -2.70% for 043 and +0.73% for
780. These results do not establish a reproducible regression; the isolated
gain is much larger than the observed noise.

The first edge-copy version replaced per-pixel horizontal clamping with
two fills and a copy, reducing isolated cycles from 1,876,688,115 to
1,754,831,375 (-6.49%) and pipeline cycles from 27,651,788,442 to
27,415,122,622 (-0.86%). The dense-clip retest was noisy (+5.07%, including
a +43.06% pair). That candidate was superseded by an explicit full-row
copy when no horizontal extension is required; the final version was
measured independently against the original edge implementation.
Its two apparent outliers retested at -1.68% and -0.69%.

The reciprocal DC calculation uses a 188-byte table of rounded-up
20-bit reciprocals. The Visual predictor is in 0..=2047 and the scale
is in 8..=46. With numerator * scale < 2^20, the reciprocal approximation
cannot cross the next integer quotient. This preserves the original
rounding; MP42 still uses its original signed division. The CPU 12
pipeline run contains a +25.81% aggregate spike on clip 321, which
retested at -0.86% on CPU 14. The complete CPU 14 repeat gives -0.48%,
and the combined pipeline cycle difference across CPUs is +0.27%,
within observed noise. The isolated improvement justifies this small
arithmetic replacement without claiming a pipeline speedup.

## Rejected inverse-transform candidate

The attempted SSE2 outer-row path checked that the final row and first
row AC terms were zero, then returned a constant array using the existing
inverse_dc rounding function. This candidate matched all output but
measured 2,191,531,671 versus 2,181,924,381 cycles (-0.44%) across the
mixed transform trace and 53,547,025 versus 53,465,592 cycles (-0.15%)
across the DC-only subset. Pipeline cycles increased from 27,321,451,004
to 27,709,682,628 (+1.42%). Retests of 282, 633 and 780 on CPU 14
reproduced increases of 3.31%, 3.00% and 5.72%. The change was reverted
before testing reciprocal DC prediction. Its raw measurements are
retained for transparency.

All three accepted changes preserve exact reconstruction and introduce
no new dependencies or unsafe blocks. Sixteen release tests pass,
including exhaustive short-motion-code comparisons, edge clamping
comparisons, and every legal DC predictor/scale combination.

## Full ITG3 collection

Both revisions decoded all 356 original AVI files (796,563,156 input
bytes) successfully, producing 222,701 frames. Direct comparison covered
25,655,155,200 visible pixel bytes and 10,689,648 frame metadata bytes
without differences.

| Revision | Pipeline cycles | Frames/billion cycles |
| --- | ---: | ---: |
| Pass 2 baseline | 132,004,537,631 | 1687.07 |
| Pass 3 final | 127,206,381,341 | 1750.71 |
| Change | -3.63% | +3.77% |

The separate allocation pass over all 356 files records 229,320
heap calls and 280,161,792 requested bytes for each revision. Every
individual file has identical allocation counts and requested bytes.
See `full-allocations.csv`, `full-cycles.csv` and `full-exact.csv`.

This supplementary single-pair-per-file measurement compares the
two revisions directly rather than adding the independent optimization
percentages. Some individual pairs show transient variance, so the exact
size of the cumulative gain remains uncertain.

Rust formatting, the AArch64 Windows compile check, and strict Clippy
with only the previously confirmed `large_enum_variant` warning
allowed all pass. The original AVI decoder enum remains unchanged.

## Reproduce

Use the same UTF-8 AVI manifests and source exports as passes 1 and 2.
Source revisions are `31689b487` (baseline), `ea34042af` (motion),
`ff98a252b` (edge), and `bfa822aa8` (DC prediction). Store temporary
inputs and traces outside the repository.

```powershell
$runner = 'crates/deadlib-video-mpeg4/bench/itg3.ps1'
& $runner -Before $original -After $original -Inputs $selected -Work $aa -Operation pipeline -Rounds 3 -SkipVerify
& $runner -Before $original -After $original -Inputs $selected -Work $capture -Capture -CaptureExtra -CapturePixels
& $runner -Before $original -After $motion -Inputs $selected -Work $motionRun -Operation motion -Trace "$capture/intra.bin.motion"
& $runner -Before $motion -After $edge -Inputs $selected -Work $edgeRun -Operation edge -Trace "$capture/intra.bin.edge"
& $runner -Before $edge -After $final -Inputs $selected -Work $dcRun -Operation intra -Trace "$capture/intra.bin"
& $runner -Before $edge -After $final -Inputs $selected -Work $dcDirect -Operation dcpredict -Trace "$capture/intra.bin" -KernelsOnly
& $runner -Before $original -After $final -Inputs $allClips -Work $fullRun -Rounds 1 -Full
```

The DC predictor replay inserts the original division expression into a
temporary benchmark-only helper when the source revision predates the
reciprocal helper. It uses the same real predictor/scale pairs in both
builds. All end-to-end libraries use the uninstrumented source.
