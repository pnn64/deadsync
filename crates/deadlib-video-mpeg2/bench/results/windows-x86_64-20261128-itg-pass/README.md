# Paired ITG video optimization measurements

## Setup

Windows x86-64, Xeon E5-2696 v4, Rust 1.100.0. Both revisions use the same
`rustc --edition 2021 -C opt-level=3 -C codegen-units=1` settings, baseline
x86-64 SSE2, and the same in-memory input. The original source is `e4aba094c`
for the MPEG crate; each subsequent comparison uses the preceding accepted
implementation. The cycle benchmark runs both revisions in one process, on
one thread pinned to logical CPU 12; targeted outlier retests use CPU 14.
Order alternates by round and clip. `QueryThreadCycleTime` counts thread cycles;
frequency variation and other system activity can still affect results.

The repeated corpus contains 18 full-length clips (105,384,111 input bytes,
11,080 frames): eight MPEG-1 clips from each of the two Downloads ITG video
collections, selected at evenly spaced file-size quantiles including the
smallest and largest files, plus two established MPEG-2 ITGmania clips.

| In The Groove | In The Groove 2 | MPEG-2 |
| --- | --- | --- |
| jb_072.mpg | 952_JumpBack.mpg | dix.mpg |
| vol3-12-ntsc.mpg | 843_JumpBack.mpg | lights.mpg |
| vol4-24-ntsc.mpg | 800_JumpBack.mpg | |
| VOL1-29-NTSC.mpg | 938_JumpBack.mpg | |
| VOL1-35-NTSC.mpg | 906_JumpBack.mpg | |
| EV01453N.mpg | 453_JumpBack.mpg | |
| 676_JumpBack.mpg | 929_JumpBack.mpg | |
| jb_042.mpg | 451_JumpBack.mpg | |

The ITG3 AVI collection is outside this MPEG-1/2 pipeline. No source media,
absolute input paths, or captured pixel/coefficient traces are committed.

The timed pipeline includes program-stream demux when applicable, decoder
construction, unit parsing, decoding and reconstruction, frame callback,
finish/drain and destruction. It uses MPEG-1 recovery, as in playback. File
I/O and output verification are outside the measured region. A separate
allocation-instrumented build measures successful alloc, alloc_zeroed and
realloc calls and the full requested bytes. It does not count frees, resident
memory, or peak live memory. Throughput is units per billion measured cycles.

For isolated start-code scanning each clip's elementary stream is traversed
eight times per measurement. For pixel reconstruction and inverse transform,
the benchmark replays 38,949 actual blocks sampled from the original decoding
(every 512th invocation of each kernel), 64 times per round. Reconstruction
restores the sampled prediction plane each repetition; its measured cycles
therefore include this constant replay cost. The hooks exist only in temporary
copies, not the production crate. The timed production decode uses unmodified
source apart from the candidate optimization.

## Results

All cycle totals include every recorded pair, with no samples removed. Pipeline
totals use five paired rounds (55,400 frames per revision). Kernel totals use
ten paired rounds (24,927,360 blocks per revision); the scanner uses five
paired rounds (4,138,845,080 elementary-stream bytes per revision).

| Change | Isolated cycles before | Isolated cycles after | Change | Pipeline cycles before | Pipeline cycles after | Change |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Start-code scanner | 7,049,944,910 | 3,132,484,723 | -55.57% | 44,355,937,266 | 44,160,661,538 | -0.44% |
| Saturating pixel reconstruction | 1,857,798,832 | 1,580,234,503 | -14.94% | 43,725,414,120 | 42,831,135,055 | -2.05% |
| Uniform inverse transform | 2,908,822,112 | 2,745,282,761 | -5.62% | 43,437,578,079 | 42,586,068,407 | -1.96% |

| Change | Isolated units/billion cycles before | After | Change | Pipeline frames/billion cycles before | After | Change |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Scanner (decimal MB) | 587.1 | 1321.3 | +125.1% | 1249.0 | 1254.5 | +0.44% |
| Reconstruction (million blocks) | 13.42 | 15.77 | +17.56% | 1267.0 | 1293.5 | +2.09% |
| Inverse (million blocks) | 8.57 | 9.08 | +5.96% | 1275.4 | 1300.9 | +2.00% |

All three isolated operations request zero heap allocations and zero bytes in
both revisions. Each separate allocation pass over the complete 18-clip subset
clips (11,080 frames) records exactly 11,268 successful allocation/reallocation
calls and 56,540,038 requested bytes before and after each change (0% change).

The A/A run compares two builds of identical source: aggregate pipeline cycles
vary by -1.42% and scanner cycles by +2.40%. Consequently the scanner's
pipeline improvement is inconclusive, but its isolated gain is large and no
pipeline regression is measurable. The scanner uses safe word arithmetic in
place of a byte-at-a-time search. The reconstruction change replaces two
production lines with safe saturating i16 arithmetic; four of five aggregate
pipeline rounds improve, but the magnitude is near the observed noise.

The inverse transform skips both dimensions when all intermediate frequency
rows are zero and the only possible remaining coefficients are DC and a
mismatch bit of one. A conservative bound of 11 * 22725 on the mismatch
contribution proves identical final rounding before filling the block. The
fast path is applicable to 15,393 of the 38,949 captured blocks (39.5%). All ten
isolated pairs and all five aggregate pipeline pairs improve. The two MPEG-2
clips improve by 8.00% (`dix`) and 6.65% (`lights`) in pipeline cycles. The
existing SSE2 load/store bounds and fallback remain intact; no new unsafe
blocks or unchecked memory accesses are introduced.

Targeted seven-pair CPU 14 retests address apparent individual regressions.
Scanner: `843_JumpBack` -4.03%, `VOL1-29-NTSC` +1.27% (within observed noise).
Inverse: `EV01453N` -0.70%; `952_JumpBack` has a first-pair transient cycle
spike, also seen in the A/A run, with six subsequent ordinary pairs near
unchanged. The full retest including the spike is retained in the CSV; the
ordinary pairs do not reproduce the regression.

The scanner's 18 clips matched SHA-256 over visible pixels and metadata;
the reconstruction comparison matched every output byte directly. Release
unit and integration tests passed (12 total), including all 8,192 combinations
of legal DC values and mismatch bits against the scalar inverse transform.

## Full Downloads corpus confirmation

The original and final implementations were compared directly on all 261 MPG
files in the two Downloads ITG collections and the two established MPEG-2
clips. All 263 clips decoded successfully in both revisions, with exactly
135,185 frames, 16,123,776,000 visible pixel bytes and 3,244,440 metadata bytes
compared without any differences. The input total was 1,304,001,107 bytes.

| Revision | Pipeline cycles | Frames/billion cycles |
| --- | ---: | ---: |
| Original | 106,400,318,465 | 1270.53 |
| Final | 102,430,234,217 | 1319.78 |
| Change | -3.73% | +3.88% |

These are paired measurements, one pair per clip, as a supplementary check of
the cumulative changes rather than five additional repeated rounds. Allocation
counts were measured separately on the repeated 18-clip subset above.
See `full-exact.csv` and `full-cycles.csv` for every individual result.

## Reproduce

Use the adjacent `itg_pass.ps1` runner from a Windows x64 Rust environment
with the ITGmania FFmpeg avutil import library and DLL in one directory.
Export the `.rs` files from each revision's `src` directory to separate
temporary directories; the script builds copies of those files. Supply a
UTF-8 manifest with one complete MPEG file path per line (use the same order
as the original cycle CSV). All generated data should stay outside the repo.

```powershell
$runner = 'crates/deadlib-video-mpeg2/bench/itg_pass.ps1'
& $runner -Before $original -After $original -Inputs $selected -Work $capture -Avutil $avutil -Capture
& $runner -Before $original -After $scanner -Inputs $selected -Work $scanRun -Avutil $avutil -Operation scan
& $runner -Before $scanner -After $reconstruction -Inputs $selected -Work $blockRun -Avutil $avutil -Trace $capture -Operation block
& $runner -Before $reconstruction -After $final -Inputs $selected -Work $inverseRun -Avutil $avutil -Trace $capture -Operation inverse
& $runner -Before $original -After $final -Inputs $allClips -Work $fullRun -Avutil $avutil -FullCorpus
```

`-VerifyOnly` runs direct output comparison without cycle measurements.
`-FullCorpus` compares all visible bytes and frame metadata (also on partial
output before an error), checks the error result, and takes one pipeline pair
per successfully decoded nonempty clip. The broad run is supplementary to the
repeated subset; it does not replace the per-change acceptance measurements.
