# Shared model and animation data performance 0.5.1662

Parent: `7db83fdcf` (0.5.1661). Date: 2026-10-01.

Three optimizations apply `rust-performance.md`'s M-HOTPATH,
M-INITIAL-CAPACITY, M-MEM-REUSE and M-THROUGHPUT guidance to model loading,
noteskin source preparation and Lua model layer creation:

1. Expand parsed MilkShape triangles directly into their final vertex Arc.
   The parent allocated a Vec sized for all declared faces, filled valid
   faces, then allocated and copied the final Arc during material resolution.
   The new exact-length iterator uses the actual valid face count and skips
   empty meshes before allocating. Parsing still updates normals for all
   triangles before expansion, preserving the last-normal-wins rule. Triangle
   scratch capacity remains reusable across meshes. Bounds are scanned in
   emitted order after construction, outside the collector. The complete
   32-mesh parser fixture records 18.7% to 30.4%
   CPU reduction, including file I/O and material resolution.
2. Collect note-color lane indices directly into their final Arc above 64
   frames. Small lanes retain the parent's bounded stack staging array.
   Large lanes remove the Vec allocation/copy. Sequential sheets keep
   their shared empty index array and sheet-origin semantics. The 1024-frame
   row fixture records 19.9% to 28.9% CPU reduction.
3. Build Lua additive-frame UV and cumulative-delay metadata directly in its
   final Arc. Both animated sources and single-frame atlases remove a Vec
   allocation and copy. Delay accumulation remains sequential with the same
   missing-duration fallback, including nonfinite and negative values. The
   isolated 8-frame array fixture records
   18.3% to 25.7% CPU reduction. Complete Lua layer
   results below retain geometry and draw metadata costs as well.

No dependencies or unsafe production code were added. These paths prepare
noteskin/model resources and Lua model layers; the measurements do not
establish a gameplay frame-rate change. Independently owned immutable outputs
still require their final allocation: this pass removes staging buffers rather
than claiming zero allocations for complete model loads. The isolated mesh
expansion and Lua additive-frame builders each use one final allocation.
Lane sources additionally allocate the source Arc. Rejected model/animated
lane inputs remain allocation-free.

Vertex Arc construction now precedes material resolution. A mesh whose
material cannot resolve may therefore request the 16-byte Arc header before
being rejected, whereas the parent held only its temporary Vec at that point.
The memory savings below apply to resolved fixture meshes. Malformed and
truncated input behavior is covered separately. Final Arc sizes are exact;
no oversized vertex Vec capacity survives. Triangle scratch storage retains
the parent's capacity policy.

## Measurement method

The [raw CSV](shared-model-data-0.5.1662.csv) contains 276 measurements:
23 workloads, two implementations and six serial rounds alternating
old-first/new-first. Each median uses seven timing batches and the CSV keeps
batch minima/maxima. Windows QueryThreadCycleTime records calling-thread CPU
cycles. A System allocator wrapper separately counts allocation, reallocation,
free and requested/freed bytes; instrumentation is disabled during timing.
Requested/freed bytes measure allocator traffic, not process RSS or peak live
memory. GPU upload, other threads and system-wide CPU work were not measured.

Environment: Windows x64, Intel Xeon E5-2696 v4 at 2.20 GHz, 22 cores / 44 logical
processors, Rust 1.98.1, LLVM 22.1.8. Release uses opt-level 3 and full LTO with
test unwinding. Compilation and checks finish before measurement. Each
executable runs on logical processor 6 (affinity mask 64), verified by the
runner, with --test-threads=1. Three untimed operations warm each workload.

The committed manual benchmarks can be replayed after the release build:

```text
cargo test -p deadsync-noteskin --lib --release --locked --offline benchmark_shared_mesh_preparation -- --ignored --nocapture --test-threads=1
cargo test -p deadsync-assets --lib --release --locked --offline benchmark_lane_indices -- --ignored --nocapture --test-threads=1
cargo test -p deadsync-assets --lib --release --locked --offline benchmark_lua_model_frames -- --ignored --nocapture --test-threads=1
```

Set DEADSYNC_PERF_ORDER to old-first on odd rounds and new-first on even
rounds; pin the invoking process and inherited test process to the same
logical processor. The recorded rounds invoke the already built test
executables directly, so Cargo/compiler work is absent during timing.

All throughput values are operations/s. Inputs and filesystem fixture creation
are outside measurement. Isolated mesh operations clone the same input
triangle Vec in both variants, then expand, drain and destroy the complete
output and input clone. Thus they count two allocations in the new version:
one input clone and one final vertex Arc. Complete parser operations include
file reads, parsing, normal assignment, expansion, material resolution and
output destruction; one operation loads one model, not one vertex. Lane
operations include source validation, index/source construction and complete
output destruction. Lua frame operations borrow one texture slot and destroy
the output Arc. Complete Lua layer operations also include geometry and draw
metadata construction, texture key sharing and output destruction.

Every counted operation balances allocation/free calls and requested/freed
bytes. All five churn metrics remain stable across six rounds. Workloads
with reallocations: none. Counts below retain any unchanged
parser/input work; they do not isolate vertex-buffer costs from complete loads.

Three complete function bodies are frozen from the parent: MilkShape model
parsing, note animation source construction and Lua model layer creation.
The isolated mesh and Lua additive-frame baselines preserve the parent's
exact buffer-building stages. An audit compares all five bodies/stages with
the parent and checks every other parent function in the changed modules.
Common unchanged types and helpers are shared. Older frozen loader tests
keep their original Vec intermediate type under cfg(test); production uses
the new Arc intermediate type.

15 of 23 workloads improve CPU cycles and throughput in every round.
Workloads without consistent improvement in both metrics: `shared_expand_mostly_invalid`, `shared_parse_small`, `shared_lane_row8`, `shared_lane_column4`, `shared_lane_sequential`, `shared_additive_empty`, `shared_additive_mixed129`, `shared_additive_frames1024`.
Workloads with higher CPU cost in every round: `shared_lane_row8`.
Negative reductions mean higher measured cost. All rounds are retained;
allocation savings alone do not establish a CPU improvement. Small fixtures
and complete file loads can vary between rounds, so isolated builder gains
are not presented as equivalent full-load gains.

The 8-frame color-lane path keeps the parent's stack algorithm, but the
complete source call uses
0.7% to 8.7%
more CPU in all six rounds with identical allocation counts. This measured
cost is retained, and large-lane gains are not claimed for small sources.
The unchanged sequential control has mixed results. The isolated mixed
129-frame and ordinary 1024-frame additive builders also have mixed CPU
results despite nearly halving their requested bytes. Complete Lua layer
calls improve in all six rounds, including at 1024 frames. The experiment
does not isolate the cause of differences in unchanged branch algorithms.

An initial candidate updated bounds inside the vertex collector and used
125.7% to 151.1% more CPU in the 4096-face expansion fixture. The final version
separates construction from the bounds scan. An initial all-size lane
collector also used 4.8% to 23.6% more CPU at 64 frames, so the final version
retains the parent's small-lane stack path. The tables and CSV below contain
fresh rounds of the final implementation, not those rejected candidates.

An earlier measurement of the final code used short batches for tiny
workloads. Even unchanged controls varied substantially, so the committed
benchmarks increase batch counts before collecting the six rounds below.
Both implementations use identical iteration counts. The shorter-batch
measurements are retained locally and are excluded from the final CSV.

## MilkShape vertex expansion

Fixtures use 17 input vertices and 1, 128 or 4096 valid triangles. The
mostly-invalid fixture has 128 valid triangles but a declared capacity of
4096, exposing the parent's over-reservation. Batch sizes are 16384, 2048,
128 and 2048 operations respectively. Emission order and bounds calculation
are compared bit-for-bit before timing.

| Workload | ns/op, old -> new (round 1) | Thread cycles/op, old -> new (round 1) | CPU reduction, six rounds | Throughput gain, six rounds | Alloc/realloc/free, old -> new | Requested bytes, old -> new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `shared_expand_triangle` | 266.0 -> 216.2 | 583.5 -> 474.2 | 18.6% to 20.7% | 22.6% to 26.4% | 3/0/3 -> 2/0/2 | 280 -> 160 |
| `shared_expand_mesh128` | 5,780.6 -> 5,559.9 | 12,676.9 -> 12,188.1 | 1.5% to 6.2% | 1.3% to 6.6% | 3/0/3 -> 2/0/2 | 33808 -> 18448 |
| `shared_expand_mesh4096` | 204,743.8 -> 196,159.4 | 448,397.4 -> 429,866.7 | 2.6% to 7.9% | 2.7% to 8.7% | 3/0/3 -> 2/0/2 | 1081360 -> 589840 |
| `shared_expand_mostly_invalid` | 5,981.1 -> 5,771.0 | 13,111.2 -> 12,642.6 | -3.4% to 3.6% | -3.3% to 3.6% | 3/0/3 -> 2/0/2 | 509968 -> 18448 |

## Complete MilkShape model loads

The small fixture has one mesh with three faces; many_meshes has 32 meshes
with 64 faces each; dense has one mesh with 4096 faces; invalid has four
meshes with 129 declared faces each and rejects every third face. Fixtures
alternate bone assignments and white/default versus sphere/NoMove material
flags. Batch sizes are 128, 16, 16 and 32 loads respectively. Files and
material references are warmed; disk cold-start latency is not measured.

| Workload | ns/op, old -> new (round 1) | Thread cycles/op, old -> new (round 1) | CPU reduction, six rounds | Throughput gain, six rounds | Alloc/realloc/free, old -> new | Requested bytes, old -> new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `shared_parse_small` | 114,049.2 -> 107,704.7 | 249,622.9 -> 234,077.9 | -0.5% to 6.2% | -0.7% to 5.9% | 12/0/12 -> 11/0/11 | 2930 -> 2562 |
| `shared_parse_many_meshes` | 1,089,675.0 -> 812,387.5 | 2,386,404.0 -> 1,778,430.2 | 18.7% to 30.4% | 23.0% to 43.8% | 136/0/136 -> 104/0/104 | 556171 -> 310155 |
| `shared_parse_dense` | 1,176,918.8 -> 1,082,093.8 | 2,572,114.7 -> 2,370,188.5 | 7.9% to 13.0% | 8.8% to 14.9% | 12/0/12 -> 11/0/11 | 1154028 -> 662500 |
| `shared_parse_invalid` | 321,681.2 -> 245,803.1 | 704,018.8 -> 538,838.2 | 3.0% to 27.5% | 3.0% to 37.9% | 24/0/24 -> 20/0/20 | 119865 -> 57913 |

## Note-color lane sources

Fixtures include rows of 8, 64, 65 or 1024 frames and columns of 4 or 129
frames, crossing the parent's 64-frame staging boundary. Slots have a
nonzero sheet origin and both mirror axes. Sequential is an unchanged
8x4 sheet control. Batches contain 16384 operations.

| Workload | ns/op, old -> new (round 1) | Thread cycles/op, old -> new (round 1) | CPU reduction, six rounds | Throughput gain, six rounds | Alloc/realloc/free, old -> new | Requested bytes, old -> new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `shared_lane_row8` | 261.3 -> 284.3 | 573.1 -> 623.0 | -8.7% to -0.7% | -8.1% to -0.5% | 2/0/2 -> 2/0/2 | 344 -> 344 |
| `shared_lane_column4` | 271.7 -> 269.2 | 595.9 -> 590.1 | -0.6% to 6.6% | -0.8% to 7.0% | 2/0/2 -> 2/0/2 | 312 -> 312 |
| `shared_lane_row64` | 289.3 -> 284.8 | 633.8 -> 623.8 | 0.6% to 10.3% | 0.8% to 11.5% | 2/0/2 -> 2/0/2 | 792 -> 792 |
| `shared_lane_row65` | 424.5 -> 323.1 | 931.0 -> 708.5 | 15.1% to 25.3% | 17.7% to 34.0% | 3/0/3 -> 2/0/2 | 1320 -> 800 |
| `shared_lane_row1024` | 1,759.4 -> 1,280.3 | 3,856.0 -> 2,806.8 | 19.9% to 28.9% | 24.8% to 40.7% | 3/0/3 -> 2/0/2 | 16664 -> 8472 |
| `shared_lane_column129` | 440.2 -> 312.3 | 965.6 -> 684.8 | 19.0% to 29.1% | 23.4% to 41.0% | 3/0/3 -> 2/0/2 | 2344 -> 1312 |
| `shared_lane_sequential` | 194.7 -> 197.1 | 427.0 -> 432.2 | -6.0% to 0.2% | -5.4% to 0.0% | 1/0/1 -> 1/0/1 | 264 -> 264 |

## Lua additive-frame arrays

Fixtures cover an atlas, an empty animated sequence and 8, 129 or 1024
frames. The 129-frame fixture includes zero, negative zero, negative,
fractional, NaN and infinite delays with custom UVs. The duration array
omits the final two entries to exercise fallback. Explicit index arrays
also run out before large frame counts, preserving index fallback. Batch
counts are 32768 for atlas and empty, 16384 for 8 frames, 4096 for 129 frames
and 1024 for 1024 frames. Empty is a control: both variants allocate the
empty Arc header.

| Workload | ns/op, old -> new (round 1) | Thread cycles/op, old -> new (round 1) | CPU reduction, six rounds | Throughput gain, six rounds | Alloc/realloc/free, old -> new | Requested bytes, old -> new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `shared_additive_atlas` | 181.9 -> 107.8 | 398.4 -> 236.3 | 40.7% to 47.7% | 68.8% to 91.4% | 2/0/2 -> 1/0/1 | 60 -> 40 |
| `shared_additive_empty` | 81.7 -> 85.0 | 179.1 -> 185.9 | -3.8% to 7.0% | -3.9% to 7.6% | 1/0/1 -> 1/0/1 | 16 -> 16 |
| `shared_additive_frames8` | 428.7 -> 341.0 | 940.4 -> 747.8 | 18.3% to 25.7% | 22.4% to 34.5% | 2/0/2 -> 1/0/1 | 336 -> 176 |
| `shared_additive_mixed129` | 1,569.8 -> 1,808.3 | 3,434.0 -> 3,956.7 | -15.2% to 3.7% | -13.2% to 3.9% | 2/0/2 -> 1/0/1 | 5180 -> 2600 |
| `shared_additive_frames1024` | 45,178.4 -> 37,532.5 | 98,968.1 -> 82,260.0 | -8.5% to 24.7% | -8.0% to 32.9% | 2/0/2 -> 1/0/1 | 40976 -> 20496 |

## Complete Lua model layers

These retain geometry construction, draw flags and additive texture metadata
with an atlas, 8 frames or 1024 frames. They use the same unchanged model
fixture in both variants. Batch counts are 32768, 16384 and 1024 complete
layer operations respectively.

| Workload | ns/op, old -> new (round 1) | Thread cycles/op, old -> new (round 1) | CPU reduction, six rounds | Throughput gain, six rounds | Alloc/realloc/free, old -> new | Requested bytes, old -> new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `shared_lua_layer_atlas` | 360.5 -> 275.1 | 790.7 -> 603.0 | 12.9% to 23.7% | 14.8% to 31.0% | 3/0/3 -> 2/0/2 | 140 -> 120 |
| `shared_lua_layer_frames8` | 634.0 -> 580.2 | 1,389.9 -> 1,271.4 | 7.9% to 13.8% | 8.7% to 17.7% | 3/0/3 -> 2/0/2 | 416 -> 256 |
| `shared_lua_layer_frames1024` | 38,801.2 -> 38,135.7 | 84,975.2 -> 83,587.5 | 1.6% to 5.5% | 0.4% to 5.1% | 3/0/3 -> 2/0/2 | 41056 -> 20576 |

## Regression validation

Nine new ordinary tests compare frozen parent behavior and allocation budgets:

- Exact vertex fields and bounds bits, emitted order, repeated vertex indices,
  normalized/fallback normals, last-normal-wins, signed zero, subnormal values,
  NaN and infinities; empty/invalid faces and retained triangle scratch.
- Complete layers with same/mixed bone assignments, embedded/separate
  material files, material flags, bad signatures, missing files and every
  line-boundary truncation through a two-mesh model.
- Lane grid sizes/origins, row/column precedence, mirror flags, frame counts,
  beat/time rates, UV bits, finite/nonfinite spacing and animation lengths,
  and allocation-free rejection of model/already animated inputs.
- Lua atlas/empty/animated sources, partial explicit indices and durations,
  custom UVs, rotation/mirroring, sequential cumulative-delay bits, unchanged
  shared source contents, complete model metadata and rejected missing/empty
  model geometry.
- One final mesh/metadata allocation, final-only lane arrays across the
  64-frame boundary, and reduced owning allocation/free/byte churn. Small
  lane stack staging and sequential sheets serve as unchanged controls.

Validation passed:

```text
cargo test -p deadsync-noteskin -p deadsync-assets --lib --locked --offline shared_mesh_preparation
cargo test -p deadsync-assets --lib lane_indices --locked --offline
cargo test -p deadsync-assets --lib lua_model_frames --locked --offline
cargo test -p deadsync-assets -p deadsync-noteskin -p deadsync-notefield --lib --release --locked --offline
cargo check --workspace --locked --offline
cargo clippy -p deadsync-noteskin -p deadsync-assets --lib --locked --offline -- -D clippy::perf -A clippy::large_enum_variant
rustfmt --edition 2024 --check --config skip_children=true <changed Rust files>
git diff --check
```

The complete release library suites pass 961 tests, with 30 manual
benchmarks ignored. The three new manual benchmarks pass in all six rounds.
Performance lint checks pass; existing style warnings remain. Validation uses
library targets; the previously identified unrelated safe_parsing integration
failure involving private script_random is outside this pass's scope. These
results do not imply that every workspace integration target passes.

Cargo.toml and Cargo.lock change only 0.5.1661 -> 0.5.1662, including the three
workspace packages that inherit the version. The commit excludes
deadsync-song.json.gz, rust-performance.md, optimize.sh and optimize.ps1.
