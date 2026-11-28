# ITG MPEG-1/2 optimization pass 2 — 2026-11-28

Three sequential changes were retained: power-of-two motion wrapping, a division-free hot prediction bounds check, and packed-byte bilinear interpolation. The direct single-bit reader was measured first and rejected after a clip-level pipeline regression reproduced. Each accepted comparison uses the preceding accepted source.

| Stage | Before source | After source | Decision |
| --- | --- | --- | --- |
| Direct flag read | `2caab3421` | Temporary candidate shown below | Rejected and reverted |
| Motion-vector wrapping | `2caab3421` | `74c0335e0` | Retained: substantial isolated gain; overall pipeline effect inconclusive |
| Prediction bounds | `74c0335e0` | `01cb7d017` | Retained: small simplification; CPU 14 confirmation supports a small pipeline gain |
| Bilinear interpolation | `01cb7d017` | `d52f1352e` | Retained: substantial isolated gain; overall pipeline effect inconclusive |

The production changes remove a net six lines, excluding tests and benchmark support. They introduce no new unsafe blocks, memory accesses, heap allocations, dependencies, or production abstraction layers. The SIMD change reuses the existing SSE2 safety boundary and load/store bounds.

## Corpus and measurement conditions

Windows x86-64 on an Intel Xeon E5-2696 v4, 22 physical cores and 44 logical processors, using Rust 1.100.0. Both libraries and the benchmark use identical flags:

```text
--edition 2021 -C opt-level=3 -C codegen-units=1
```

These are baseline x86-64/SSE2 builds; no `target-cpu=native` setting is used. Both revisions run in one executable on one thread pinned to logical CPU 12. Retests and confirmation runs use logical CPU 14. Order alternates by round and clip. `QueryThreadCycleTime` measures thread cycles. The host was not isolated from other activity, and results from different stages must not be compared as a single before/after series.

The repeated corpus contains 18 full-length clips, 105,384,111 input bytes and 11,080 decoded frames. Eight MPEG-1 clips from each Downloads collection were selected at evenly spaced file-size quantiles, including the smallest and largest files; the two established MPEG-2 samples preserve MPEG-2 coverage.

| In The Groove | In The Groove 2 | MPEG-2 |
| --- | --- | --- |
| `jb_072.mpg` | `952_JumpBack.mpg` | `dix.mpg` |
| `vol3-12-ntsc.mpg` | `843_JumpBack.mpg` | `lights.mpg` |
| `vol4-24-ntsc.mpg` | `800_JumpBack.mpg` | |
| `VOL1-29-NTSC.mpg` | `938_JumpBack.mpg` | |
| `VOL1-35-NTSC.mpg` | `906_JumpBack.mpg` | |
| `EV01453N.mpg` | `453_JumpBack.mpg` | |
| `676_JumpBack.mpg` | `929_JumpBack.mpg` | |
| `jb_042.mpg` | `451_JumpBack.mpg` | |

The broad confirmation corpus is all 261 Downloads MPG files plus those two MPEG-2 samples. ITG3 AVI files use MPEG-4 and are outside this decoder. Source media, absolute manifests, captured bytes/pixels, binaries, and temporary source copies are not included in these results.

The timed pipeline includes program-stream extraction where applicable, decoder construction, unit parsing, reconstruction, the output callback, finish/drain, and destruction. It uses MPEG-1 recovery as playback does. File I/O and output comparison are outside timing. Thus this is the full native decoding pipeline, including demux when needed; it does not measure display/rendering or audio.

Allocation passes use a separate instrumented executable and count successful `alloc`, `alloc_zeroed`, and `realloc` requests and their full requested sizes. They do not measure frees, peak live bytes, or resident memory. Throughput is completed frames or calls per billion measured cycles, not wall-clock FPS.

All recorded pairs are retained. Each primary pipeline comparison has seven rounds: 77,560 frames per revision. The A/A run and the bounds CPU 14 full-subset confirmation have five rounds: 55,400 frames per revision. Targeted retests use eleven rounds over the specified clips.

## Isolated measurements

The hooks are appended to temporary source copies only. Production pipeline builds contain no capture or replay hooks. Capture and replay-buffer allocation are outside timing.

| Operation | Captured contexts | Sampling and replay |
| --- | ---: | --- |
| Flag read, rejected | 70,384 | Every 256th actual flag call; next four bytes or the actual shorter tail, available length, and original bit offset; 256 replays per round |
| Motion read | 10,362 | Every 256th actual call; up to eight bytes, tail length, bit offset, both f_codes and incoming predictor; 256 replays per round |
| Full prediction | 617 | Every 16,384th actual call; padded source and destination geometry, full source samples, affected destination pixels, coordinates, motion and average mode; 256 replays per round |
| Bilinear prediction | 63 | Filtered from those 617 records using both odd motion components; 39 width-16, 24 width-8, including 15 average-mode records; 8,192 replays per round |

All isolated cycle runs have fourteen pairs. Motion and flag replay call the real decoder methods and compare exact results, error status and consumed bit position before timing; timed results feed checked checksums. Prediction replay allocates plane buffers once and restores only the affected W×W destination tile for each invocation. It calls the real `Plane::predict` and compares the entire destination buffers before and after replay, including unused destinations and padding.

Prediction measurements therefore include the same tile-reset cost and cache effects on both sides. They are not measurements of the bounds expression or SIMD instructions alone.

| Accepted change | Calls per revision | Cycles before | Cycles after | Cycle change | Million calls / billion cycles before | After | Throughput change |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Motion wrapping | 37,137,408 | 3,090,013,437 | 2,824,961,933 | −8.5777% | 12.0185 | 13.1462 | +9.3825% |
| Prediction bounds | 2,211,328 | 1,336,294,056 | 1,311,302,881 | −1.8702% | 1.6548 | 1.6864 | +1.9058% |
| Bilinear, long replay | 7,225,344 | 2,846,890,619 | 2,345,747,549 | −17.6032% | 2.5380 | 3.0802 | +21.3639% |

Sources: [motion](motion-kernel-cycles.csv), [bounds](bounds-kernel-cycles.csv), [bilinear long replay](bilinear-long-kernel-cycles.csv).

Motion improves in all 14 pairs, with a median paired change of −6.9038%; one −27.38% pair enlarges the aggregate gain. Bounds improves in 13/14 pairs, median −1.5888%, a modest gain appropriate to its small source change. Bilinear improves in 14/14 long-replay pairs, median −17.8222%.

The bounds CPU 14 isolated confirmation records 1,403,072,105 → 1,394,668,763 cycles, −0.5989%, or 1.5761 → 1.5856 million calls/billion cycles (+0.6025%), over the same 2,211,328 calls. Ten of fourteen pairs improve. This run is noisier and reinforces that the isolated bounds gain is modest. [Confirmation data](bounds-confirm-kernel-cpu14.csv)

The short bilinear pilot used only 256 repetitions: 225,792 calls per revision, 86,936,486 → 70,440,186 cycles (−18.9751%), and 2.5972 → 3.2054 million calls/billion cycles (+23.4189%). Its short individual intervals motivated the longer replay above. The pilot is preserved but is not the primary SIMD acceptance measurement. [Pilot](bilinear-kernel-cycles.csv)

## End-to-end measurements and allocation checks

| Accepted change and run | Frames per revision | Cycles before | Cycles after | Cycle change | Frames / billion cycles before | After | Throughput change |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Motion, primary CPU 12 | 77,560 | 73,983,695,010 | 74,209,618,141 | +0.3054% | 1,048.339 | 1,045.148 | −0.3044% |
| Bounds, primary CPU 12 | 77,560 | 58,939,533,056 | 59,342,081,799 | +0.6830% | 1,315.925 | 1,306.998 | −0.6784% |
| Bounds, full-subset CPU 14 confirmation | 55,400 | 42,441,093,048 | 41,920,368,513 | −1.2269% | 1,305.339 | 1,321.553 | +1.2422% |
| Bilinear, primary CPU 12 | 77,560 | 57,762,747,760 | 57,720,316,825 | −0.0735% | 1,342.734 | 1,343.721 | +0.0735% |

Sources: [motion](motion-cycles.csv), [bounds primary](bounds-cycles.csv), [bounds confirmation](bounds-confirm-pipeline-cpu14.csv), [bilinear](bilinear-cycles.csv).

| Scope, each accepted change | Calls before | Calls after | Requested bytes before | Requested bytes after | Change |
| --- | ---: | ---: | ---: | ---: | ---: |
| Complete 18-clip pipeline, one round | 11,268 | 11,268 | 56,540,038 | 56,540,038 | 0% |
| Isolated timed operation | 0 | 0 | 0 | 0 | 0% |

The rejected flag candidate has the same allocation results. [Motion allocations](motion-allocations.csv), [bounds allocations](bounds-allocations.csv), [bilinear allocations](bilinear-allocations.csv), [long bilinear kernel allocations](bilinear-long-kernel-allocations.csv).

### Motion wrapping: isolated improvement, overall effect inconclusive

The validated f_code gives a power-of-two interval. The production change replaces:

```rust
(vector + limit).rem_euclid(2 * limit) - limit
```

with:

```rust
((vector + limit) & (2 * limit - 1)) - limit
```

This removes a remainder operation without changing predictor wrapping. The isolated improvement is clear. Primary aggregate pipeline rounds range from −3.07% to +4.37%, so the +0.3054% total is inconclusive.

An eleven-round CPU 14 retest of the five apparent primary outliers records 33,351,740,939 → 33,297,382,779 cycles (−0.1630%), or 1,142.159 → 1,144.024 frames/billion cycles (+0.1633%), across 38,093 frames per revision. The original large slowdowns do not reproduce.

| Retested clip | Total cycle change | Median paired change |
| --- | ---: | ---: |
| `906_JumpBack` | +0.8700% | +0.4162% |
| `EV01453N` | +0.2024% | −0.0500% |
| `jb_072` | −0.4194% | −0.4255% |
| `lights` | −0.1203% | −0.2007% |
| `vol3-12-ntsc` | −1.8644% | −0.5915% |

The small positive shift on `906` remains visible; this is not a claim that every clip improved. The evidence supports retaining the simple isolated improvement with no reproducible aggregate pipeline regression. [Retest](motion-retest-cpu14.csv)

### Prediction bounds: small simplification with a small confirmed pipeline gain

The old hot path divides padded sample length by stride before checking each predicted block. The new path compares the required row count multiplied by stride against padded sample length, and computes height by division only for edge extension. It retains the padded dimensions used by prediction.

The primary total is +0.6830%, but 100/126 pairs improve and the median pair is −0.9196%; several large transient outliers dominate the aggregate. An eleven-round CPU 14 retest of `453_JumpBack`, `676_JumpBack`, `800_JumpBack` and `lights` records 32,438,799,841 → 32,097,840,145 cycles (−1.0511%) over 33,594 frames, with throughput 1,035.612 → 1,046.612 frames/billion cycles (+1.0623%). All eleven aggregate retest rounds improve; clip totals are −1.1656%, −0.6027%, −1.2563%, and −1.1432%, respectively. [Retest](bounds-retest-cpu14.csv)

The subsequent five-round CPU 14 confirmation uses all 18 clips and improves in all five aggregate rounds, totaling −1.2269% cycles. It supports an approximately 1% gain in these conditions. The noisier original result remains part of the evidence, and the isolated gain is described as modest rather than substantial.

### Bilinear interpolation: substantial isolated improvement, overall effect inconclusive

The previous SSE2 implementation expands bytes into low and high 16-bit lanes, adds four samples and rounding bias, shifts, then packs the results. The replacement uses rounded byte averages and a parity-derived one-byte correction. It preserves `(a + b + c + d + 2) >> 2` exactly, while removing the separate low/high arithmetic and the helper's width parameter. Existing loads and stores are unchanged.

The long isolated replay improves by 17.6032%. The full pipeline's −0.0735% change is too small to resolve reliably. Bilinear accounted for 63 of the 617 sampled predictions (10.2%), and interpolation arithmetic is only part of each prediction and the complete decoder; a proportional pipeline improvement is not expected.

An eleven-round CPU 14 retest of `929_JumpBack` and `VOL1-35-NTSC` records 9,339,632,380 → 9,292,863,491 cycles (−0.5008%) over 14,850 frames, or 1,589.998 → 1,598.000 frames/billion cycles (+0.5033%). The clip totals are −0.9328% and +0.2350%. These do not establish a meaningful regression or a reliable overall gain. [Retest](bilinear-retest-cpu14.csv)

## Rejected candidate and noise calibration

A/A compares two differently named builds of identical source. The representative run records 42,015,679,022 → 41,872,464,241 cycles (−0.3409%) over 55,400 frames. Its aggregate round differences range from −3.09% to +1.88%, and the median individual pair is −0.0144%. A three-clip CPU 14 A/A retest records 19,191,072,878 → 19,414,077,243 cycles (+1.1620%) over 24,761 frames, with a median individual pair of −0.2322%. These show substantial transient variability; they are not a universal fixed noise threshold. [A/A](aa-cycles.csv), [A/A retest](aa-retest-cpu14.csv)

The tested flag candidate replaced `Ok(self.read(1)? != 0)` with this implementation in a temporary baseline copy:

```rust
pub(crate) fn flag(&mut self) -> Result<bool> {
    let byte = self.bytes.get(self.position / 8).ok_or(Error::Truncated)?;
    let value = byte & (0x80 >> (self.position % 8)) != 0;
    self.position = self.position.checked_add(1).ok_or(Error::Truncated)?;
    Ok(value)
}
```

It reduced isolated cycles from 2,631,763,074 to 1,793,236,202 (−31.8618%) over 252,256,256 calls per revision. Throughput rose from 95.8507 to 140.6710 million calls/billion cycles (+46.7605%), with all fourteen pairs improving.

Its primary pipeline total was 66,438,529,007 → 65,749,527,136 cycles (−1.0371%), or 1,167.395 → 1,179.628 frames/billion cycles (+1.0479%). However, only 49/126 individual pairs improved and the median pair was +0.3647%; favorable large transients distorted that aggregate.

More decisively, `929_JumpBack` slowed by +8.041% in the primary run and +4.831% in the CPU 14 retest, with retest median +3.622% and 8/11 pairs slower. The matching A/A retest was −0.484% total, median −0.047%. The whole three-clip flag retest recorded 22,289,762,537 → 22,795,906,276 cycles (+2.2707%) over 24,761 frames, or 1,110.869 → 1,086.204 frames/billion cycles (−2.2203%). The candidate was rejected and reverted before motion wrapping was applied.

Sources: [flag isolated](flag-kernel-cycles.csv), [flag pipeline](flag-cycles.csv), [flag retest](flag-retest-cpu14.csv). Its allocation count and bytes remained unchanged.

Do not subtract or chain absolute cycle totals from different stage conditions. Likewise, targeted retest totals are not combined with the representative corpus: doing so would overweight the selected clips.

## Correctness and final gates

Each candidate's representative-corpus comparison matched all visible pixels, the six recorded frame metadata fields, frame counts, and error outcomes, including output produced before an error. Each comparison covers all 18 clips, 11,080 frames, and 1,827,145,920 directly compared pixel/metadata bytes. [Motion verification](motion-verify.csv), [bounds verification](bounds-verify.csv), [bilinear verification](bilinear-verify.csv), [rejected flag verification](flag-verify.csv).

Ten unit tests passed. The added tests cover motion wrapping for all supported f_codes and predictor boundaries, invalid/truncated motion input, padded prediction dimensions and edge extension, and every one of the 261,121 pairs of byte-pair sums for bilinear rounding at both widths. Existing scalar/SSE2 comparisons retain all fractional and averaging modes.

All six integration tests passed, including MPEG-1 recovery, full-pel motion, program-stream packet splitting, display order, and golden frame hashes. Final formatting checks and Clippy with `-D warnings` passed.

### Full Downloads corpus confirmation

The final comparison of baseline `2caab3421` against `d52f1352e` completed across all 261 Downloads MPG files plus the two MPEG-2 clips: **263 clips, 1,304,001,107 input bytes and 135,185 frames**. Exact comparison matched **16,123,776,000 visible pixel bytes plus 3,244,440 metadata bytes**, totaling **16,127,020,440 directly compared bytes**. [Exact results](full-exact.csv) · [Corpus SHA-256 hashes](corpus-sha256.csv)

| Revision | Pipeline cycles | Frames / billion cycles | Allocation requests | Requested bytes |
| --- | ---: | ---: | ---: | ---: |
| Baseline | 102,674,199,596 | 1,316.640 | 137,578 | 163,054,518 |
| Final | 100,853,631,808 | 1,340.408 | 137,578 | 163,054,518 |
| Change | −1.7732% | +1.8052% | 0% | 0% |

This run uses one pair per clip on logical CPU 14, alternating revision order by clip. It provides supplementary cumulative evidence over the full corpus; the repeated per-change comparisons above remain the acceptance measurements. The separate full-corpus allocation pass has identical request counts and requested bytes for both revisions. [Full cycles](full-cycles.csv) · [Full allocations](full-allocations.csv) · [Machine-readable summary](full-summary.json)

## Reproduction

Use the current [PowerShell runner](../../itg_pass.ps1), [paired executable source](../../itg_pass.rs), and [bilinear trace filter](../../itg_bilinear_trace.py). Export each revision's `crates/deadlib-video-mpeg2/src/*.rs` into a separate temporary source directory outside the repository. The runner makes its own instrumented copies. Use UTF-8 manifests with one complete input path per line, matching the original cycle CSV order. The AVUTIL directory must contain the ITGmania x64 import library and DLL.

In the commands below, `$baseline`, `$motion`, `$bounds`, and `$bilinear` name those source directories; `$work` is a new external work directory, `$selected` is the 18-clip manifest, and `$avutil` is the library/DLL directory.

```powershell
$runner = 'crates/deadlib-video-mpeg2/bench/itg_pass.ps1'
$common = @{ Inputs = $selected; Avutil = $avutil; Cpu = 12; Rounds = 7 }
$motionTrace = Join-Path $work 'motion-capture'
$predictionTrace = Join-Path $work 'prediction-capture'
$bilinearTrace = Join-Path $work 'bilinear-capture'

& $runner @common -Before $baseline -After $baseline -Operation read-motion -Capture -Work $motionTrace
& $runner @common -Before $baseline -After $motion -Operation read-motion -Trace $motionTrace -Work (Join-Path $work 'motion')

& $runner @common -Before $motion -After $motion -Operation prediction -Capture -Work $predictionTrace
& $runner @common -Before $motion -After $bounds -Operation prediction -Trace $predictionTrace -Work (Join-Path $work 'bounds')

python crates/deadlib-video-mpeg2/bench/itg_bilinear_trace.py (Join-Path $predictionTrace 'predictions.bin') (Join-Path $bilinearTrace 'predictions.bin')
& $runner @common -Before $bounds -After $bilinear -Operation prediction -Trace $bilinearTrace -KernelRepeats 8192 -Work (Join-Path $work 'bilinear')
```

To reproduce the flag rejection, make a temporary copy of the baseline with only the function replacement above. Capture from the unchanged baseline with `-Operation flag -Capture`, then compare that baseline with the temporary candidate using `-Operation flag -Trace <capture-directory>`.

The runner's default kernel repeat count remains 256 for these operations; block/inverse defaults remain 64. `-KernelRepeats` requires a positive value and applies equally to cycle and allocation runs. The executable's optional final argument provides the same override:

```powershell
& $kernelCycles kernels 14 12 $bilinearTrace prediction 8192
& $kernelAllocations kernels 1 12 $bilinearTrace prediction 8192
```

For CPU 14 confirmation, use the same source pair and manifest with `-Cpu 14`; bounds full-subset confirmation used five pipeline rounds. Targeted retests use manifests containing only the listed clips and eleven pipeline rounds. `-VerifyOnly` performs direct output comparison without timing. `-FullCorpus` performs direct comparison and one supplementary pipeline pair per clip; it does not replace the repeated per-change acceptance measurements.

The stored CSVs preserve all recorded values in UTF-8. Each filename is prefixed by its stage. Pilot, confirmation, A/A and rejected-candidate results are retained so their scope remains explicit.
