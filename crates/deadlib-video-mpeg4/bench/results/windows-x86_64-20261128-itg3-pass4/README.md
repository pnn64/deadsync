# ITG3 AVI optimization pass 4

## Method

Baseline `d66a37570` (end of pass 3), with each candidate compared to
the preceding accepted revision. Windows x86-64, Intel Xeon E5-2696 v4,
Rust 1.100.0, `--edition 2021 -C opt-level=3 -C codegen-units=1`.
Both revisions run in the same process, alternating order by round and
clip on logical CPU 12. Targeted retests use CPUs 14 and 4.
The same sixteen original ITG3 AVI files contain 40,391,316 input bytes
and 8,362 decoded frames. Timing covers complete in-memory AVI decode
including demux, setup, reconstruction, frame callback, drain and
destruction; file I/O and exact comparison are outside timing.
Allocation instrumentation runs separately. The identical-build A/A
control over three pairs per clip records 16,503,163,252 versus
16,465,833,992 cycles (-0.23%).

New temporary capture hooks sample every 2,048th flag, peek and skip
operation and every 512th texture block. The traces contain 6,430 flags,
36,734 lookaheads, 33,911 skips, and 12,143 coded texture blocks (2,810
intra and 9,333 inter). Bit traces retain the bit offset, count and
up to 33 source bytes, sufficient for every u8-sized skip. Texture
traces retain the scan order, initial coefficient position, bit offset
and up to 512 source bytes. Motion-grid replay captures every 2,048th
invocation: 544 macroblocks (401 single-vector, 42 skipped, 101
four-vector), including the original grid, packet boundary and input
bits. The replay restores the four updated cells after every invocation;
this identical restoration overhead is included on both revisions.
All isolated replay outputs are compared exactly before timing.
No captured input, grid or video data are committed.

## Measurements

Five paired pipeline rounds per clip, 41,810 decoded frames per revision.
Ten paired isolated rounds; 256 repetitions for flags and vector grids,
128 for lookaheads and skips, and 64 for texture blocks.

| Accepted change / operation | Cycles before | Cycles after | Change |
| --- | ---: | ---: | ---: |
| Direct flag read (16,460,800 reads) | 133,952,951 | 105,172,111 | -21.49% |
| Flag pipeline | 27,638,945,481 | 27,516,571,577 | -0.44% |
| Texture lookup (7,771,520 blocks) | 2,265,253,617 | 2,138,135,456 | -5.61% |
| Texture pipeline | 27,849,370,257 | 27,196,824,165 | -2.34% |
| Shared vector grids (1,392,640 macroblocks) | 414,074,020 | 365,717,505 | -11.68% |
| Vector pipeline | 27,720,336,235 | 27,482,499,705 | -0.86% |

| Operation | Throughput before | Throughput after | Change |
| --- | ---: | ---: | ---: |
| Flag (million reads/billion cycles) | 122.885 | 156.513 | +27.37% |
| Flag pipeline (frames/billion cycles) | 1512.72 | 1519.45 | +0.44% |
| Texture (million blocks/billion cycles) | 3.431 | 3.635 | +5.95% |
| Texture pipeline (frames/billion cycles) | 1501.29 | 1537.31 | +2.40% |
| Grid (million macroblocks/billion cycles) | 3.363 | 3.808 | +13.22% |
| Vector pipeline (frames/billion cycles) | 1508.28 | 1521.33 | +0.87% |

Every isolated allocation run reports zero heap calls and zero requested
bytes. Each pipeline allocation run records exactly 8,678 calls and
12,947,904 requested bytes on both revisions over the sixteen videos.
All sixteen clips match visible pixels and frame metadata exactly after
each change.

The flag reader directly extracts the bit from the source byte, preserving
truncation and the checked position increment. No table or state is added.
The largest per-file slowdown in the repeated pipeline is +0.87%; the
aggregate effect is small and the isolated result is stronger evidence.

The coefficient lookup grows from 9 to 11 bits, adding 12 KiB static
storage across the two tables. More signed run-level codes fit the fast
path. Longer codes and escapes retain the original decoder. All short
entries are tested against the full decoder for both coefficient tables.
The largest per-file slowdown is +0.60%.

The shared motion-vector path updates the two adjacent grid rows directly
when the macroblock is skipped or has a single vector, preserving the
packet-boundary visibility rule. Four-vector blocks retain the sequential
prediction/update order. Two apparent pipeline outliers (+7.33% for 043
and +7.93% for 633) retest at -1.00% and -6.32% on CPU 14. The large
initial improvement for 058 (-18.80%) is also noisy, retesting at -5.09%.
The isolated replay verifies the returned vectors, updated cells and
consumed bit positions. All raw pairs are retained, including spikes.

## Rejected candidates

| Candidate | Isolated cycles before | After | Change | Pipeline change |
| --- | ---: | ---: | ---: | ---: |
| Four-byte lookahead with cold tail helper | 337,916,515 | 317,617,375 | -6.01% | +0.81% |
| Cached bit length | 224,915,063 | 218,936,767 | -2.66% | +0.79% |

The lookahead candidate moves zero padding into a cold helper and uses a
direct range check. The CPU 14 full-subset retest is -0.15% aggregate, but
clip 462 regresses +1.52%, +1.27% and +0.75% across the original and two
retests. The gain does not justify that repeatable slowdown.

The cached bit-length candidate adds a machine word to Bits and replaces
the byte-ceiling check with a direct bound comparison. CPU 14 targeted
retests of 073, 282 and 780 measure +4.10%, +1.20% and +0.32%.
Both candidates were reverted before the next accepted change.

## Full ITG3 collection

Both revisions decoded all 356 original AVI files (796,563,156 input
bytes) successfully, producing 222,701 frames. Direct comparison covered
25,655,155,200 visible pixel bytes and 10,689,648 frame metadata bytes
without differences.

| Revision | Pipeline cycles | Frames/billion cycles |
| --- | ---: | ---: |
| Pass 3 baseline | 128,565,736,823 | 1732.20 |
| Pass 4 final | 124,855,532,940 | 1783.67 |
| Change | -2.89% | +2.97% |

The separate allocation pass over all 356 files records 229,320
heap calls and 280,161,792 requested bytes for each revision. Every
individual file has identical allocation counts and requested bytes.
See `full-allocations.csv`, `full-cycles.csv` and `full-exact.csv`.

This supplementary single-pair-per-file measurement compares the two
revisions directly. Individual cycle measurements contain transient
variance, so the exact size of the cumulative gain remains uncertain.

## Reproduce

Export the source revisions and use the same UTF-8 AVI manifests as
previous passes. Revisions: `d66a37570` baseline, `1d2c99e5d` flag,
`81c24b355` texture, `30af281e9` vectors. Keep trace paths outside the
repository. Capture and replay hooks are compiled only into temporary
copies; all pipeline builds use uninstrumented source.

```powershell
$runner = 'crates/deadlib-video-mpeg4/bench/itg3.ps1'
& $runner -Before $original -After $original -Inputs $selected -Work $aa -Operation pipeline -Rounds 3 -SkipVerify
& $runner -Before $original -After $original -Inputs $selected -Work $capture -Capture -CaptureBits -CaptureTexture
& $runner -Before $original -After $flag -Inputs $selected -Work $flagRun -Operation flag -Trace "$capture/intra.bin.flag"
& $runner -Before $flag -After $texture -Inputs $selected -Work $textureRun -Operation texture -Trace "$capture/intra.bin.texture"
& $runner -Before $texture -After $texture -Inputs $selected -Work $vectorCapture -Capture -CaptureVectors
& $runner -Before $texture -After $vectors -Inputs $selected -Work $vectorRun -Operation vectors -Trace "$vectorCapture/intra.bin.vectors"
& $runner -Before $original -After $vectors -Inputs $allClips -Work $fullRun -Rounds 1 -Full
```

All 19 release tests pass, including flag truncation, short texture
lookup equivalence, and motion grid updates across packet boundaries.
No dependencies or unsafe blocks were added.
Rust formatting, the AArch64 Windows compile check, and strict Clippy
with only the pre-existing `large_enum_variant` warning allowed all pass.
