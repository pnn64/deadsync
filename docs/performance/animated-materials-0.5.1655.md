# Animated material loading - 0.5.1655

Parent: `46a2c2e5c` (0.5.1654). Date: 2026-10-01.

Three changes follow `rust-performance.md`'s M-HOTPATH, M-MEM-REUSE,
M-INITIAL-CAPACITY and M-THROUGHPUT guidance:

1. Reuse the first successfully resolved image for every exact repeat of its
   raw INI reference during the same load. Different references still follow
   normal resolution and override rules. The isolated 32-frame repetition
   fixture uses 85.6% to 86.5% fewer measured CPU cycles.
2. Defer owning frame storage while every resolved path has the first path's
   exact spelling. Single-image scrolling materials avoid the frame vector,
   its growth reallocations and path copies that would all be discarded.
   When another spelling occurs, reserve the known prefix size, reconstruct
   its already validated delays and preserve every owning path. The first
   32 delays stay in a 128-byte stack buffer; longer prefixes revisit the INI
   without allocating scratch storage.
   Semantic path equality separately determines whether an atlas is needed,
   retaining alias behavior and exact spellings. The isolated 32-frame
   single-image fixture removes 33 allocations and three reallocations;
   its measured CPU reduction ranges from -3.6% to 9.0%.
3. Skip the already probed model/INI parent and earlier equal search directories
   during relative-file fallback. The first occurrence keeps its priority,
   and the final noteskin prefix/redirect fallback is unchanged. Deduplication
   scans the existing slice without allocating a set or a persistent cache.
   The missing-reference fixture with duplicate directories uses
   56.1% to 57.5% fewer CPU cycles.

These helpers serve noteskin animated textures, MilkShape material loading
and the asset bridge. Gains concern loading work; rendering frame rate,
texture decoding/upload and cold-disk performance were not measured.
The full 32-frame single-image texture comparison uses
85.8% to 86.6% fewer CPU cycles than the parent. The one-mesh
model using that material uses 78.3% to 80.2% fewer CPU cycles.

No new dependencies or unsafe production code were added. Complete loads
still allocate INI contents/values, the returned texture path, geometry,
layers and genuine animation frames. First-reference reuse lasts only for
one INI load. Directory deduplication lasts only for one path lookup.
Later loads continue to read changed files and directories.

## Measurement method

The [raw CSV](animated-materials-0.5.1655.csv) retains 336 measurements:
28 workloads, two implementations and six serial runs, alternating
old-first/new-first. Each median uses seven batches of 128 operations;
batch minimum/maximum elapsed times are retained. Windows
`QueryThreadCycleTime` measures the calling thread's CPU cycles. The existing
counting `System` allocator runs separately for one complete operation,
including output destruction; counting is disabled during timing.

Environment: Windows x64, Intel Xeon E5-2696 v4 at 2.20 GHz, 22 cores / 44 logical
processors, Rust 1.98.1, LLVM 22.1.8. Release builds use opt-level 3 and full LTO,
with test unwinding. Processes run serially on logical processor 6 (affinity
mask 64), with `--test-threads=1`, after all compiler/check processes finish.

Fixtures, file creation and behavior comparisons happen before timing.
Each measured operation includes file reads, parsing/resolution, owning
output construction and destruction. Warmups populate unchanged noteskin
lookup caches and OS file/metadata caches. Fixed-width temporary path IDs
keep all allocation counters and byte totals stable across these six runs.
Requested/freed bytes include reallocation sizes and describe allocator
churn, rather than peak live memory or process RSS. Per-operation timings
are shown for run 1; all six run medians and ranges remain in the CSV.

Six parent functions are frozen in
`crates/deadsync-noteskin/tests/animated_materials/baseline.rs`: asset-reference
normalization, relative/noteskin path resolution, animated-texture resolution,
material texture resolution, model texture fallback and model-layer parsing.
A source audit checks their bodies/attributes against the parent. Unchanged
types, INI parsing and case-insensitive filesystem helpers are shared.

`after_frame_reuse.rs` is the parent animated resolver with only repeated
first-reference reuse applied. `after_lazy_frames.rs` adds deferred storage
and matches the production animated resolver. Both explicitly use the frozen
parent path resolver, isolating these changes from directory deduplication.
Source audits verify the first stage's two substitutions, the second stage's
match to production and the frozen path imports. Full comparisons use all
parent helpers versus all production helpers.

16 of 28 workloads improve CPU cycles and throughput in every run.
Every measurement is retained. Timing gains are not consistent across all
runs for: `anim_reuse_single`, `anim_lazy_single`, `anim_full_single`,
`anim_lazy_repeat32`, `anim_lazy_late32`, `anim_lazy_late129`,
`anim_reuse_mixed`, `anim_lazy_mixed`, `anim_full_mixed`,
`anim_lazy_alias_late`, `anim_search_local`, `anim_search_unique_missing`.
Negative CPU reduction values mean higher measured CPU cost. Identical
allocation counters on controls do not establish a timing improvement.

Two isolated comparisons are slower in all six runs:
`anim_lazy_late129` uses 2.3% to 5.3% more CPU cycles while removing six
reallocations and reducing requested bytes from 97,830 to 82,670. Its long
prefix rereads validated delays beyond the 32 inline entries. The complete
production-vs-parent comparison for that same input still reduces CPU cycles
by 92.0% to 93.3% through first-reference reuse.
`anim_search_unique_missing` uses 0.2% to 8.3% more CPU cycles and has unchanged
allocation counts; unique folders offer no duplicate work to remove.
These are observed costs, not claimed gains. This pass prioritizes repeated
frame references, owning frame churn and duplicate directory probes.

## Repeated first-reference resolution

Each operation loads one INI and destroys its owning result; throughput is
textures/s. The comparison is exact parent versus first-reference reuse only.
`single` has one frame; `repeat32` has 32 identical references; `late32` changes
only its last image; `repeat129` and `late129` exercise longer sequences and
the bounded delay buffer's fallback; `mixed` has two different images;
`alias_late` uses four frames including `./frames/a.png`, followed by a
different image. Delays vary
between zero and several positive fractions. No probe cache is shared across
loads and no normalized-reference hash table is added.

| Workload | ns/op, old -> new (run 1) | Thread cycles/op, old -> new (run 1) | CPU reduction, six runs | Throughput gain, six runs | Alloc/realloc/free, old -> new | Requested bytes, old -> new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `anim_reuse_single` | 193,516.4 -> 193,579.7 | 423,923.1 -> 424,032.9 | -1.5% to 1.8% | -1.4% to 1.7% | 25/1/25 -> 25/1/25 | 2507 -> 2507 |
| `anim_reuse_repeat32` | 1,713,106.2 -> 245,590.6 | 3,751,774.5 -> 537,862.4 | 85.6% to 86.5% | 596.3% to 640.7% | 246/35/246 -> 184/4/184 | 37246 -> 22645 |
| `anim_reuse_late32` | 1,709,367.2 -> 329,207.0 | 3,740,561.2 -> 720,712.8 | 80.7% to 83.9% | 419.2% to 521.9% | 247/35/247 -> 187/5/187 | 37368 -> 23238 |
| `anim_reuse_repeat129` | 6,341,973.4 -> 454,670.3 | 13,882,922.3 -> 994,468.7 | 92.4% to 94.0% | 1218.3% to 1567.5% | 927/135/927 -> 671/7/671 | 157525 -> 97237 |
| `anim_reuse_late129` | 6,254,495.3 -> 482,835.9 | 13,695,116.0 -> 1,056,986.8 | 92.2% to 93.1% | 1184.9% to 1347.2% | 928/135/928 -> 674/8/674 | 157647 -> 97830 |
| `anim_reuse_mixed` | 261,232.8 -> 249,032.0 | 571,507.7 -> 545,495.2 | -4.0% to 8.1% | -4.0% to 8.9% | 34/2/34 -> 34/2/34 | 4094 -> 4094 |
| `anim_reuse_alias_late` | 386,136.7 -> 335,292.2 | 844,903.5 -> 734,206.9 | 12.6% to 13.9% | 14.5% to 16.1% | 48/4/48 -> 46/3/46 | 5438 -> 4967 |

## Deferred owning frame storage

The same fixtures compare first-reference reuse alone with that reuse plus
deferred frame storage. Both use frozen parent path resolution. A single-image
texture with identical path spellings creates no frame vector or per-frame
owning paths. Actual animations still own their complete ordered frames.
An alternate spelling triggers storage even when it denotes the same image;
this preserves spelling if a later distinct image makes the frames observable.
The first 32 validated delays stay on the stack and are copied directly when
storage becomes necessary. Larger prefixes reread remaining validated delays
from the INI. Timing/allocation tradeoffs on real animations remain visible.

| Workload | ns/op, old -> new (run 1) | Thread cycles/op, old -> new (run 1) | CPU reduction, six runs | Throughput gain, six runs | Alloc/realloc/free, old -> new | Requested bytes, old -> new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `anim_lazy_single` | 194,771.1 -> 201,028.9 | 426,567.4 -> 439,770.0 | -3.1% to 2.6% | -3.1% to 2.4% | 25/1/25 -> 23/1/23 | 2507 -> 2223 |
| `anim_lazy_repeat32` | 239,178.9 -> 238,409.4 | 523,596.7 -> 522,084.2 | -3.6% to 9.0% | -3.5% to 9.9% | 184/4/184 -> 151/1/151 | 22645 -> 16277 |
| `anim_lazy_late32` | 320,175.8 -> 326,374.2 | 700,572.0 -> 714,208.4 | -1.9% to 5.8% | -1.9% to 6.2% | 187/5/187 -> 187/2/187 | 23238 -> 22118 |
| `anim_lazy_repeat129` | 425,129.7 -> 411,895.3 | 930,697.1 -> 899,932.9 | 3.3% to 7.5% | 3.2% to 8.1% | 671/7/671 -> 541/1/541 | 97237 -> 60921 |
| `anim_lazy_late129` | 479,292.2 -> 508,499.2 | 1,049,338.6 -> 1,100,991.4 | -5.3% to -2.3% | -5.7% to -2.2% | 674/8/674 -> 674/2/674 | 97830 -> 82670 |
| `anim_lazy_mixed` | 246,441.4 -> 249,043.8 | 539,045.7 -> 545,503.8 | -5.4% to 1.5% | -5.2% to 1.9% | 34/2/34 -> 34/2/34 | 4094 -> 4094 |
| `anim_lazy_alias_late` | 339,095.3 -> 331,138.3 | 742,441.6 -> 723,765.2 | -2.1% to 2.5% | -1.9% to 2.4% | 46/3/46 -> 46/3/46 | 4967 -> 4967 |

## Duplicate directory probes

Each operation resolves and destroys one optional path; throughput is
lookups/s. The duplicate search list is local/first/local/first/last/last,
and the INI parent is local. Each directory contains 16 unrelated image files
plus fixture images/subdirectories. `local` succeeds before fallback;
`fallback` finds a file only in last; `missing` exhausts duplicate locations.
`unique_missing` uses only first/last search directories and controls the
slice comparison overhead when nothing can be skipped. Final noteskin
fallback and its existing directory cache are shared, warmed and unchanged.

| Workload | ns/op, old -> new (run 1) | Thread cycles/op, old -> new (run 1) | CPU reduction, six runs | Throughput gain, six runs | Alloc/realloc/free, old -> new | Requested bytes, old -> new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `anim_search_local` | 47,162.5 -> 47,740.6 | 103,273.0 -> 104,528.3 | -1.2% to 6.8% | -1.2% to 8.3% | 3/1/3 -> 3/1/3 | 595 -> 595 |
| `anim_search_fallback` | 636,475.8 -> 292,289.8 | 1,393,584.9 -> 640,175.2 | 49.4% to 57.4% | 97.4% to 134.7% | 130/11/130 -> 54/5/54 | 8741 -> 3855 |
| `anim_search_missing` | 813,546.9 -> 352,344.5 | 1,781,563.2 -> 771,852.9 | 56.1% to 57.5% | 127.8% to 135.4% | 180/14/180 -> 78/6/78 | 11381 -> 4886 |
| `anim_search_unique_missing` | 350,664.1 -> 356,018.0 | 768,042.5 -> 779,362.2 | -8.3% to -0.2% | -7.7% to -0.3% | 78/6/78 -> 78/6/78 | 4886 -> 4886 |

## Complete animated-texture loading

Exact parent versus production resolution for the seven INI fixtures,
including all three optimizations; throughput is textures/s.

| Workload | ns/op, old -> new (run 1) | Thread cycles/op, old -> new (run 1) | CPU reduction, six runs | Throughput gain, six runs | Alloc/realloc/free, old -> new | Requested bytes, old -> new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `anim_full_single` | 193,102.3 -> 190,402.3 | 422,909.6 -> 416,921.4 | -2.1% to 1.7% | -2.0% to 1.7% | 25/1/25 -> 23/1/23 | 2507 -> 2223 |
| `anim_full_repeat32` | 1,712,792.2 -> 232,135.9 | 3,751,081.8 -> 508,487.2 | 85.8% to 86.6% | 603.0% to 644.2% | 246/35/246 -> 151/1/151 | 37246 -> 16277 |
| `anim_full_late32` | 1,731,389.1 -> 320,491.4 | 3,790,626.1 -> 701,458.5 | 81.1% to 83.6% | 427.6% to 511.1% | 247/35/247 -> 187/2/187 | 37368 -> 22118 |
| `anim_full_repeat129` | 6,270,383.6 -> 419,427.3 | 13,716,999.2 -> 917,894.1 | 93.3% to 94.4% | 1394.8% to 1679.7% | 927/135/927 -> 541/1/541 | 157525 -> 60921 |
| `anim_full_late129` | 6,278,641.4 -> 502,603.9 | 13,746,388.1 -> 1,098,532.3 | 92.0% to 93.3% | 1149.0% to 1390.1% | 928/135/928 -> 674/2/674 | 157647 -> 82670 |
| `anim_full_mixed` | 260,764.8 -> 245,762.5 | 570,857.8 -> 538,274.0 | -3.5% to 5.7% | -2.8% to 6.1% | 34/2/34 -> 34/2/34 | 4094 -> 4094 |
| `anim_full_alias_late` | 390,391.4 -> 341,146.9 | 854,453.5 -> 746,529.8 | 12.6% to 16.0% | 14.4% to 19.0% | 48/4/48 -> 46/3/46 | 5438 -> 4967 |

## Complete model loading

Each operation reads one combined MilkShape mesh/material file, resolves
the shared 32-frame INI and destroys all returned layers. Throughput is
meshes/s; operation timings cover the entire model. `single` has one mesh,
`shared` has sixteen meshes and one-image scrolling, and `late` has sixteen
meshes with a last-frame image change. The prior pass's mesh scratch and
per-material resolution cache are present in both implementations.

| Workload | ns/op, old -> new (run 1) | Thread cycles/op, old -> new (run 1) | CPU reduction, six runs | Throughput gain, six runs | Alloc/realloc/free, old -> new | Requested bytes, old -> new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `anim_model_single` | 1,890,882.8 -> 380,360.9 | 4,140,656.6 -> 832,855.0 | 78.3% to 80.2% | 360.9% to 405.6% | 261/36/261 -> 166/2/166 | 39458 -> 18489 |
| `anim_model_shared` | 1,891,695.3 -> 422,562.5 | 4,142,803.6 -> 925,473.7 | 77.5% to 79.6% | 345.3% to 390.6% | 321/36/321 -> 226/2/226 | 51915 -> 30946 |
| `anim_model_late` | 1,981,256.2 -> 594,658.6 | 4,338,222.0 -> 1,300,326.6 | 69.1% to 72.9% | 223.8% to 269.4% | 866/36/866 -> 806/3/806 | 137957 -> 122707 |

## Behavior and validation

Five new ordinary tests compare each intermediate implementation and production
with the frozen parent. Generated fixtures cover zero/one-based indexing,
1/2/4/8/31/32/33/129/999 frames, late distinct frames, repeated paths and overrides.
A 1001-frame fixture verifies Frame1000 remains outside the historical limit.
Tests preserve exact path spellings, frame order, sphere/UV metadata and float
bits, including fractional sums, signed zero, EPSILON, overflow, NaN/negative
delay rejection, missing delays, gaps and zero-duration atlas suppression.
Additional cases cover Windows separators, quotes, Unicode/case aliases,
search priority, prefix fallback, duplicate/empty/reversed directories,
absolute paths and filesystem changes across loads. One/sixteen-mesh models
compare bounds, vertex float bits, material flags, bone bindings, animation
lengths and owning texture results. Allocation tests require less owning
churn for each of the three isolated changes.

Final validation:

- New debug behavior/allocation tests: 5 passed, one manual benchmark ignored.
- Release noteskin library: 313 passed, 14 manual benchmarks ignored.
- Release assets library: 157 passed, four manual benchmarks ignored.
- All six paired release benchmark runs passed (336 measurement rows).
- Workspace check passed, offline and locked.
- Noteskin/assets Clippy passed with `clippy::perf` denied and the existing
  `large_enum_variant` exception; unrelated existing warnings remain.
- Changed-file rustfmt, frozen-source audit and `git diff --check` passed.

These checks cover the affected libraries and workspace compilation; they
do not constitute a full workspace test run.

Reproduce correctness and benchmark preparation:

```powershell
cargo test -p deadsync-noteskin --lib animated_materials --offline --locked
cargo test -p deadsync-noteskin -p deadsync-assets --lib --release --offline --locked
cargo check --workspace --offline --locked
cargo clippy -p deadsync-noteskin -p deadsync-assets --lib --offline --locked --no-deps -- -D clippy::perf -A clippy::large_enum_variant
```

After compiler/check processes finish, pin PowerShell to logical processor 6
and run the ignored benchmark serially six times, alternating order:

```powershell
$taskProcess = Get-Process -Id $PID
$taskProcess.ProcessorAffinity = [IntPtr]64
foreach ($taskRun in 1..6) {
    $env:DEADSYNC_PERF_ORDER = if ($taskRun % 2 -eq 0) { 'new-first' } else { 'old-first' }
    cargo test -p deadsync-noteskin --lib --release --offline --locked benchmark_animated_materials -- --ignored --nocapture --test-threads=1
}
Remove-Item Env:DEADSYNC_PERF_ORDER
```

The recorded runs launch the built test executable directly and verify
child affinity mask 64; Cargo is omitted from the measurement processes.
The patch advances exactly 0.5.1654 -> 0.5.1655 in Cargo.toml and Cargo.lock.
The user-excluded reference/input/scripts are absent from this commit.
