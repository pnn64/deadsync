# Avoid copying unchanged noteskin animation layers

Baseline: `faef9065f` (0.5.1602). Review range: the 19 commits in
`eb4c21471..faef9065f`, following the previous performance pass.

## Finding and change

The color-material change in `4614fca6b` makes animation preparation visit
separate layer sets for each note color. `animate_layer_groups` cloned every
set into a temporary `Vec<SpriteSlot>`, then copied that into a new `Arc` slice.
It also did this for models, static images, and sprites that already had an
animation, even though their animation setup immediately returned without
changing anything. Each slot clone also updates its atomic identity and clones
its shared resource handles.

Preparation now computes a replacement texture source before touching layer
ownership. Unchanged groups stay intact. Changed groups use `Arc::make_mut`:
uniquely owned color variants are updated in place, and shared groups copy once
directly into their final allocation. The temporary layer `Vec` is removed.
Color grouping and animation calculations are unchanged. Other owners retain
their original data, including tap/lift fallback sharing.

This follows `rust-performance.md`'s M-HOTPATH, M-MEM-REUSE and M-THROUGHPUT
guidance. It adds no production unsafe code, dependencies, or persistent cache.
The cost occurs while preparing selected noteskins for previews or gameplay;
these measurements do not establish a frame-rate or whole-song loading gain.

The review also covered model UVs/bounds, ordered mine and receptor layers,
depth masks, effect/command parsing, fallback resolution, Lua actor loading,
startup texture discovery, preview texture selection, and upload error handling.
Their recent compatibility behavior remains intact. No measured change to
those paths is claimed by this report.

## Behavior validation

The test-only baseline freezes both the original group loop and its animation
helper. The comparison covers 160 combinations of five layer kinds (model,
static, atlas, already animated, mixed), shared/unique color groups, retained
external owners, four color-spacing modes, and beat/second clocks. It includes
an empty layer set and an incomplete trailing column.

Every stored slot field is compared, excluding the cache identity assigned by
cloning. Frame selection, UVs and model draw states are also compared at five
times. Tests check sharing within the result, distinct identities for separate
nonempty groups, and unchanged data in other owners. A separate allocation
test checks zero churn and retained ownership for unchanged groups after
warming the texture-metadata registry.

## Measurement method

Windows x86-64, Intel Xeon E5-2696 v4, Rust 1.98.1 / LLVM 22.1.8. Repository
release profile (`opt-level=3`, full LTO). The existing counted System allocator
records allocations, reallocations, frees, and requested bytes; this is not a
peak-resident-memory measurement. Windows `QueryThreadCycleTime` supplies
calling-thread cycles, rather than elapsed TSC ticks.

Both variants run through black-boxed function pointers in the same executable.
Each operation prepares four columns, nine quantizations, and three layers per
quantization (108 layer references). Shared cases have four distinct layer
sets; color cases have 36 uniquely owned sets. Fixture construction and final
destruction are excluded. Allocations/frees performed by the preparation
itself are included. Texture metadata is warm, with no file or GPU I/O.

Three paired runs are pinned to logical CPU 6, reversing variant order in run
2. Each variant has three warmups and seven batches of 1,024 fresh fixtures;
allocation counting is a separate operation. Per-operation clock overhead is
included equally, and is significant for the shortest unchanged-group cases.
Throughput counts prepared layer references, not rendered notes or frames.

## Results

Values are medians of the three run medians. Cycle reduction ranges show
the three paired runs; raw timings and all allocation counters are in the
[CSV](noteskin-layer-animation-0.5.1602.csv). All eight cases improved in every run.

| Layers / ownership | Cycles/op before -> after | Cycle reduction (range) | Allocations/op before -> after | Million layer refs/s before -> after |
| --- | ---: | ---: | ---: | ---: |
| model / shared | 7,182 -> 1,610 | 77.8% (75.6-78.1%) | 8 -> 0 | 42.26 -> 997.22 |
| model / colors | 43,091 -> 2,214 | 94.9% (94.9-94.9%) | 72 -> 0 | 5.72 -> 356.86 |
| static / shared | 7,481 -> 2,425 | 67.0% (66.4-67.6%) | 8 -> 0 | 39.79 -> 221.63 |
| static / colors | 45,292 -> 10,000 | 77.8% (77.0-77.9%) | 72 -> 0 | 5.43 -> 28.09 |
| atlas / shared | 18,271 -> 17,154 | 6.1% (1.5-12.3%) | 68 -> 64 | 13.98 -> 15.04 |
| atlas / colors | 149,802 -> 111,527 | 25.6% (24.9-28.9%) | 612 -> 540 | 1.59 -> 2.15 |
| animated / shared | 6,704 -> 1,746 | 74.6% (74.0-75.4%) | 8 -> 0 | 44.00 -> 983.91 |
| animated / colors | 39,314 -> 3,182 | 91.9% (91.7-91.9%) | 72 -> 0 | 6.28 -> 150.90 |

Unchanged shared groups avoid 8,128 requested allocation bytes per operation;
unchanged color groups avoid 73,152 bytes. Atlas preparation drops from
18,328 to 14,296 bytes for shared groups, and from 164,952 to 91,800 bytes for
unique color groups. Every case has zero reallocations. Counts are identical
across the three runs.

Validation: all 150 active asset-library tests passed in both release and debug
mode (three manual benchmarks ignored). Performance Clippy and formatting checks passed;
Clippy reported existing style warnings in unchanged code.


## Reproduce

```powershell
cargo test -p deadsync-assets --lib layer_animation --release
cargo test -p deadsync-assets --lib
cargo clippy -p deadsync-assets --lib --tests -- -D clippy::perf
cargo fmt -p deadsync-assets -- --check

$exe = Get-ChildItem target/release/deps/deadsync_assets-*.exe |
    Sort-Object LastWriteTime -Descending | Select-Object -First 1
$process = [System.Diagnostics.Process]::GetCurrentProcess()
$affinity = $process.ProcessorAffinity
try {
    $process.ProcessorAffinity = [IntPtr]64
    foreach ($run in 1..3) {
        if ($run -eq 2) { $env:PERF_REVERSE = '1' }
        else { Remove-Item Env:PERF_REVERSE -ErrorAction SilentlyContinue }
        & $exe.FullName benchmark_layer_animation --ignored --nocapture --test-threads=1
    }
} finally {
    $process.ProcessorAffinity = $affinity
    Remove-Item Env:PERF_REVERSE -ErrorAction SilentlyContinue
}
```
