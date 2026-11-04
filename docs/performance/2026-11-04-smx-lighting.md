# SMX column mapping and brightness, 2026-11-04

Three retained optimizations in `deadsync-smx`, starting at `e9546f28d` on
isolated branch `perf/non-lua-kernels-20261104`. Lua and the main worktree's
existing changes are outside this batch.

## Changes and exactness

1. Native column mapping uses one quotient/remainder by four or five. A double
   field repeats its physical pad every two native lane groups, so its pad is
   `(column / lanes) & 1`, and its local column is `column % lanes`. This is
   exactly the old `column % (2 * lanes)` split. Other widths retain the old
   general dance mapping, including its unusual pad results for large widths.
   Zero widths, saturating field bounds and singles' two-pad rejection remain.
2. After the existing cache update, 0% brightness fills that slot's span with
   zero and 100% leaves it unchanged. Other percentages retain the original
   lookup loop. Cache percentage keys and all 512 table bytes remain identical;
   cold table generation is unchanged. Odd slice lengths still leave the final
   byte untouched, matching the existing cache behavior.
3. Solid indicator/preview frames scale each present RGB colour before copying
   it to the LEDs. The same factor, FMA and byte conversion execute once per
   channel, reducing up to 1,350 conversions to six. Missing pads remain zero;
   the full-brightness guard and percentage clamping retain their behavior.
   The now-unused general frame scaler remains only as a test reference.

All production changes use safe Rust. No production dependency, state field,
cache capacity or public interface is added. Tests reuse the existing GIF
allocation counter through test-only visibility changes rather than adding
another allocator. Cache state is compared as well as output bytes.

## Final paired measurements

| Workload | Before cycles/fixture | After cycles/fixture | CPU throughput |
| --- | ---: | ---: | ---: |
| Dance mapping, 256 calls | 6,776.40 | 5,447.66 | 1.244x |
| Pump mapping, 256 calls | 6,790.12 | 5,971.61 | 1.128x |
| Other-width mapping, 256 calls | 7,673.17 | 6,552.95 | 1.180x |
| Warm cache, both dark (1,350 bytes) | 1,836.89 | 132.36 | 13.825x |
| Warm cache, half dark | 1,836.56 | 941.54 | 1.899x |
| Warm cache, half full | 1,828.11 | 961.85 | 1.917x |
| Warm cache, both dim | 1,847.64 | 1,845.01 | 1.004x |
| Cold cache, dark/dim | 4,379.02 | 3,522.24 | 1.223x |
| Solid frame, both dim | 11,030.75 | 1,076.43 | 10.228x |
| Solid frame, full/dim | 11,033.83 | 1,083.01 | 10.188x |
| Solid frame, both dark | 10,933.29 | 930.68 | 11.737x |
| Solid frame, both full | 894.24 | 891.61 | 1.003x |

Cycle columns are independent five-process medians; throughput is the median
of five paired ratios, so it need not equal the ratio of those columns.
Both-dim cached frames and full-brightness solid frames are effectively unchanged.
All twelve workloads assert zero allocations, reallocations, frees and byte
churn for both variants in every measurement process.

Paired ranges: dance/pump/other mappings 1.232–1.262x, 1.104–1.138x and
1.162–1.200x; dark cache 12.278–15.140x; half-dark/full cache
1.862–1.951x and 1.800–1.924x; cold cache 1.172–1.243x. Dim/full-dim/dark
solid frames range 8.790–10.303x, 9.664–10.471x and 11.545–12.002x.
The cache fixtures include the same seed-frame copy for both variants.

## Behavior verification

The actual SMX crate passes all 108 release unit tests, with three benchmark/
trace tests left ignored. A normal release library check also passes, confirming
the general scaler's test-only gating. Focused Clippy passes with warnings denied
for the extracted production definitions and their tests.

The first full-suite run found a missing-fixture failure in the cached workspace:
its source snapshot omitted the shipped lighting assets. The runner now extracts
both lighting asset directories from Git and verifies all 34 GIF headers. The
unchanged Rust sources then pass the complete suite. This fixture preparation is
the only runner change after measurement; the timing functions are verified
identical to the measured driver. Both the initial failure and successful run
remain in the local archive.

Four focused tests pass in debug and release. They check:

- 67,034 mapping inputs: small field/player/column combinations, saturation
  boundaries and 50,000 generated machine-word triples, against frozen code.
- Every raw percentage byte, thirteen lengths including empty and odd slices,
  and three repeated applications per setting: 10,218 cache transitions per
  implementation. Frames, both keys and both full lookup tables match exactly.
- 1,310 solid-frame cases covering absent pads, black/white/edge RGB channels,
  full brightness and clamped percentages, against the old complete builder.
- The fixture's 675 bytes per pad and 1,350 total bytes against the real SDK
  constants when compiled in the actual crate.

Every measured kernel stage, including the rejected mapping and debug stages,
emits the same deterministic mapping/wire/cache-state trace: **2,838,746 bytes**,
SHA-256 `5537a73438b76a0218f9a605d3fbe3c9592d2358ecc5014abc731e4010c3d379`.
The four focused tests perform direct original/current comparisons; the trace
additionally preserves output across separate builds and optimization levels.

## Evidence and rejected candidates

Adjacent TSVs/manifests retain these cumulative stages:

- `baseline`: unchanged production with calibrated mapping dispatch.
- `native-mapping`: rejected first constant-divisor arrangement. Dance and
  generic mapping improved, but pump mapping regressed to 0.943x.
- `native-quotient`: retained lane-group quotient, improving dance/pump/other
  mapping 1.224x/1.153x/1.162x in this probe.
- `constant-spans`: retained zero/identity span processing; dark cache 13.983x
  and half-dark/full cache 1.889x/1.883x, with exact cache state retained.
- `solid-colors`: retained quantization before LED replication; dim/half-full/
  dark solid frames 10.264x/9.868x/11.434x.
- `debug`: unoptimized focused tests and complete deterministic trace.
- `final` / `control`: five fresh processes each, all retained changes versus
  exact pre-batch production respectively.
- `full-release`: actual crate tests using the cached dependency workspace.

The initial branch-per-input mapping harness showed 10–13% bias even for
identical implementations. It was replaced before measuring production changes
with the same black-boxed function-pointer dispatch for both variants, selected
once per 256-call fixture. That original probe remains in the local archive as
`initial-branch-dispatch`; the published baseline/control use uniform dispatch.

Final unchanged-source control medians span 0.992–1.037x; mapping controls are
0.999–1.000x and the largest median bias is cold-cache construction (3.7%).
Individual pairs span 0.950–1.077x. Small changes are inconclusive. Absolute
cycles vary across stages; paired ratios are the primary comparison. These are
CPU component measurements, not USB throughput, LED refresh rate or whole-game
frame-rate improvements. The worker's existing send scheduling is unchanged.

## Method and reproduction

Windows x86-64, Rust 1.99.1 / LLVM 23.1.1, CPU identifier
`Intel64 Family 6 Model 79 Stepping 1, GenuineIntel`. The focused runner extracts
complete production definitions for mapping, brightness conversion, cache and
solid-frame construction, with their actual constants. The standalone SDK
shape constant is checked against the real dependency by the full-crate test.
The frozen baseline differs only in visibility needed by tests.

Release uses optimization level 3, fat LTO, static MSVC runtime and no debug
information. Debug uses optimization level zero. Benchmark threads are pinned
to CPU 2. Twelve rounds alternate variants; the first is warmup and the median
of eleven remaining rounds is reported. Final/control use five fresh processes.
Allocation accounting wraps `System` and runs separately from timing.

Snapshots retain exact source and extracted-definition hashes, generated code,
compiler commands, binaries, correctness logs and traces. Raw TSVs include
thread cycles, elapsed nanoseconds and allocation counts. Exact artifacts are
at `D:\deadsync-smx-lighting-20261104`. Full validation overlays current source
deltas onto the earlier verified dependency snapshot, records all overlay
hashes and keeps the normal host temporary directory. No connected pad is
needed for the focused tests or benchmarks. The full-suite manifest also records
the fixture archive hash. The adjacent checks JSON records artifact/source
verification, unchanged production surroundings, normal library validation and
focused Clippy results.

```powershell
$env:DEADSYNC_SMX_LIGHTING_ARCHIVE = 'D:\deadsync-smx-lighting-20261104'
python scripts/benchmark_smx_lighting.py baseline --baseline e9546f28d
python scripts/benchmark_smx_lighting.py control --baseline e9546f28d --runs 5
python scripts/benchmark_smx_lighting.py final --runs 5
python scripts/benchmark_smx_lighting.py debug --debug
python scripts/benchmark_smx_lighting.py full-release --full
```
