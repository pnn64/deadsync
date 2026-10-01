# Model preparation - 0.5.1654

Parent: `1be9bf0d7` (0.5.1653). Date: 2026-10-01.

Three changes follow `rust-performance.md`'s M-HOTPATH, M-MEM-REUSE,
M-INITIAL-CAPACITY and M-THROUGHPUT guidance in MilkShape model loading:

1. Reuse the vertex, normal and triangle parsing buffers across mesh sections.
   They retain their capacity, clear their contents and reserve exactly when
   a larger section needs more storage. Triangle expansion still happens after
   every indexed normal assignment, retaining the last assignment's value.
   Expanded output vertices remain independently owned, and scratch storage
   is dropped before material loading. The eight-mesh fixture reduces combined
   allocation/reallocation calls from 60 to 41 and total requested
   bytes by about 14%; its CPU timing results are reported without claiming
   a consistent improvement.
2. Resolve diffuse and additive material textures once per material per load.
   The diffuse resolution used to determine the global animation length is
   reused for the corresponding meshes; additive resolution is lazy. Misses
   are also retained for the rest of that load. Raw material declarations
   borrow their source lines instead of allocating string copies. Returned
   layers still own their paths and animation metadata. Sixteen meshes sharing
   animated diffuse/additive materials use 91.6% to 92.1%
   fewer measured CPU cycles in the isolated material comparison.
3. Reuse the already resolved first image when scanning animated-texture frames.
   The old resolver looked up the first path before the delay loop, then
   immediately resolved it again inside the loop. The first frame now owns a
   clone of that path, removing duplicate filesystem/path-normalization work.
   Later frame paths are resolved as before. Single-frame animated textures
   use 17.1% to 21.3% fewer measured CPU cycles.

These are production model-loading helpers, used by model declarations,
noteskin layer construction and the asset bridge. The changes target loading
CPU work, allocator churn and redundant filesystem/INI work. They do not
measure rendering frame rate or texture decoding/upload.

No dependencies or unsafe production code were added. Entire model loads still
allocate file contents, expanded meshes, returned layers, owned texture paths
and animation frames. Scratch buffers retain the maximum capacity required by
each table during that load; differently sized sections can cause a few growth
reallocations. Material results live only until that load finishes and do not
create a process-wide cache. File changes are picked up on later loads.

## Measurement method

The [raw CSV](model-preparation-0.5.1654.csv) retains 168 measurements:
14 workloads, two implementations and six serial runs, alternating
old-first/new-first. Each median uses seven timing batches; their minimum and
maximum times are retained. Windows `QueryThreadCycleTime` measures the calling
thread's CPU cycles. Allocation counts run separately for one complete operation,
including destruction of its result. Requested/freed bytes include reallocation
sizes and describe allocator churn, rather than peak memory or process RSS.

Environment: Windows x64, Intel Xeon E5-2696 v4 at 2.20 GHz, 22 cores / 44 logical
processors, Rust 1.98.1, LLVM 22.1.8. Release builds use opt-level 3 and full LTO,
with test unwinding. Processes run serially on logical processor 6 (affinity
mask 64), with `--test-threads=1`, after compiler processes finish. The existing
counting allocator delegates to `System`; counting is disabled during timing.

Fixtures and files are created before timing. Every parser measurement includes
file reads, parsing, path resolution, construction of all owning outputs and
their destruction. Behavior checks and warmups populate unchanged noteskin
lookup caches before counting. Files and directory metadata are warm in the OS
cache; these figures do not establish cold-disk loading gains. CPU-cycle
reductions distinguish saved processing from filesystem wait time. Temporary
fixture paths include the process identity, so byte counts describe each
recorded input path.

Six parent functions are frozen in
`crates/deadsync-noteskin/tests/model_preparation/baseline.rs`: model-layer
parsing, asset-reference normalization, relative/noteskin resolution, animated
texture INI resolution, material texture resolution and model texture fallback
resolution. This prevents first-frame reuse from affecting the full parent
baseline. A source audit checks function bodies and attributes against the
parent. Unchanged types, parsers and case-insensitive filesystem helpers are
shared.

`before_material_cache.rs` contains the parent model parser with only scratch
reuse applied. It shares the current animated-texture resolver with production,
isolating the material-resolution and declaration-copy changes. A source audit
checks exactly those scratch-reuse substitutions.

10 of 14 workloads improve both CPU cycles and throughput in every run.
All runs are retained. No consistent timing gain is claimed for:
`prep_ini_invalid`, `prep_scratch_1`, `prep_scratch_8`, `prep_scratch_64`.
Negative CPU reduction values mean higher measured CPU cost; allocation savings
do not by themselves establish a timing improvement for every input.

## First animated-frame resolution

Each operation reads an animated-texture INI file, resolves its frames and
destroys the owning result; throughput is textures/s. There are 128 operations
per batch. `single` has one zero-based frame with a Windows-separator path;
`repeated` has eight references to one image; `mixed` has two different images;
`one_based` starts at Frame0001; `invalid` rejects a negative first delay.
The comparison uses the exact parent animated-texture resolver and its path
helpers. The change removes only the duplicate initial image resolution;
subsequent occurrences of the same image are still resolved independently.

| Workload | ns/op, old -> new (run 1) | Thread cycles/op, old -> new (run 1) | CPU reduction, six runs | Throughput gain, six runs | Alloc/realloc/free, old -> new | Requested bytes, old -> new (run 1) |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `prep_ini_single` | 242,354.7 -> 201,996.9 | 530,128.5 -> 438,703.3 | 17.1% to 21.3% | 20.0% to 26.9% | 18/2/18 -> 16/1/16 | 2264 -> 1845 |
| `prep_ini_repeated` | 642,468.8 -> 602,363.3 | 1,391,223.6 -> 1,305,369.9 | 6.2% to 8.8% | 6.7% to 9.7% | 70/10/70 -> 68/9/68 | 9590 -> 9171 |
| `prep_ini_mixed` | 329,714.8 -> 244,532.8 | 716,372.5 -> 531,953.1 | 16.9% to 26.2% | 20.2% to 35.2% | 27/3/27 -> 25/2/25 | 3380 -> 2961 |
| `prep_ini_one_based` | 236,304.7 -> 202,985.2 | 514,581.8 -> 441,798.6 | 13.9% to 29.7% | 16.4% to 42.3% | 18/2/18 -> 16/1/16 | 2262 -> 1843 |
| `prep_ini_invalid` | 188,153.1 -> 189,548.4 | 408,155.1 -> 410,106.6 | -8.1% to 3.7% | -6.9% to 3.9% | 14/1/14 -> 14/1/14 | 1570 -> 1570 |

## Mesh scratch reuse

Each operation parses a complete model file and destroys all returned layers.
Throughput is meshes/s; divide operation timings by the mesh count for per-mesh
values. There are 256 operations per batch. Fixtures have 1, 8 or 64 meshes,
with repeating vertex-table sizes 3/6/9/3, four normals, two valid triangles
sharing vertices and one invalid triangle. Material index -1 selects the white
texture and there are no material declarations, so this comparison isolates
scratch reuse from material caching and first-frame resolution.

| Workload | ns/op, old -> new (run 1) | Thread cycles/op, old -> new (run 1) | CPU reduction, six runs | Throughput gain, six runs | Alloc/realloc/free, old -> new | Requested bytes, old -> new (run 1) |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `prep_scratch_1` | 96,916.4 -> 94,072.3 | 209,974.0 -> 204,700.9 | -3.1% to 3.3% | -2.9% to 3.7% | 11/0/11 -> 11/0/11 | 1651 -> 1651 |
| `prep_scratch_8` | 120,695.7 -> 115,246.5 | 261,982.7 -> 250,363.8 | -1.3% to 9.2% | -1.2% to 10.0% | 60/0/60 -> 39/2/39 | 12474 -> 10674 |
| `prep_scratch_64` | 377,363.7 -> 321,428.5 | 816,760.4 -> 697,790.5 | -12.1% to 14.6% | -10.4% to 17.4% | 452/0/452 -> 263/2/263 | 98300 -> 78020 |

Reusable scratch replaces repeated fresh allocations, while its capacity can
grow for larger meshes. Allocation regression checks require fewer combined
allocation/reallocation calls, no extra frees and fewer requested/freed bytes;
they deliberately retain the growth reallocations in the reported counters.

## Material resolution

Each operation reads separate mesh/material files and destroys all returned
layers. There are 128 operations per batch; throughput is meshes/s. `image`
shares two image paths across 16 meshes. `ini` shares two two-frame animated
textures across 16 meshes. `single` exercises one mesh with an animated diffuse
texture and no additive texture. `missing` shares unresolved diffuse/additive
paths and exercises the existing model texture fallback. Both implementations
use the same reusable scratch and current animated-texture resolver.

| Workload | ns/op, old -> new (run 1) | Thread cycles/op, old -> new (run 1) | CPU reduction, six runs | Throughput gain, six runs | Alloc/realloc/free, old -> new | Requested bytes, old -> new (run 1) |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `prep_material_image` | 1,843,315.6 -> 339,660.9 | 4,006,413.5 -> 737,672.6 | 80.1% to 83.4% | 399.3% to 505.2% | 159/35/159 -> 96/4/96 | 38145 -> 25463 |
| `prep_material_ini` | 11,534,822.7 -> 972,209.4 | 25,013,590.6 -> 2,109,796.3 | 91.6% to 92.1% | 1086.5% to 1169.4% | 1052/101/1052 -> 278/8/278 | 137865 -> 44793 |
| `prep_material_single` | 912,105.5 -> 569,460.2 | 1,977,501.2 -> 1,233,939.8 | 37.6% to 44.3% | 60.2% to 79.8% | 79/6/79 -> 50/3/50 | 9234 -> 6400 |
| `prep_material_missing` | 8,987,110.9 -> 1,687,227.3 | 19,516,599.0 -> 3,665,296.8 | 79.4% to 81.9% | 382.2% to 453.0% | 1125/146/1125 -> 224/22/224 | 119801 -> 36826 |

## Complete parent comparison

Each operation reads a combined model/material file and destroys all returned
layers. There are 128 operations per batch; throughput is meshes/s. One or
16 meshes share two animated material textures. These rows compare all three
production changes with the fully frozen parent implementations.

| Workload | ns/op, old -> new (run 1) | Thread cycles/op, old -> new (run 1) | CPU reduction, six runs | Throughput gain, six runs | Alloc/realloc/free, old -> new | Requested bytes, old -> new (run 1) |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `prep_full_single` | 1,133,036.7 -> 747,613.3 | 2,460,024.0 -> 1,624,847.1 | 33.9% to 45.3% | 51.6% to 83.0% | 111/12/111 -> 81/6/81 | 13746 -> 10167 |
| `prep_full_shared` | 12,317,307.0 -> 774,008.6 | 26,754,822.6 -> 1,680,058.2 | 93.2% to 93.7% | 1368.9% to 1500.8% | 1161/132/1161 -> 276/8/276 | 155803 -> 44464 |

## Behavior and validation

Five new ordinary tests check:

- Bit-exact positions, normals, UVs, texture-matrix scales, bounds, animation
  lengths and material animation parameters; equal layer ordering, texture
  paths, material flags, rigid bone bindings and additive frames.
- 175 fixture configurations: 0/1/2/8/32 meshes, seven material sets and five
  material indices, each with combined/separate material files and overrides.
  Cases include white textures, missing/out-of-range/negative material indices,
  unused diffuse materials that affect animation length, animated and additive
  materials, quotes and Windows separators.
- Last-normal-wins assignments, invalid triangle indices, missing normal tables,
  invalid normal indices, repeated vertex indices, non-finite values, malformed
  counts and truncated model files. Reused buffers cannot leak an earlier
  mesh's contents into the next mesh.
- Later loads observing edited and invalidated INI files, with earlier returned
  layers retaining their original owning animation metadata.
- First-frame resolution matching zero-/one-based keys, repeated/mixed images,
  Windows separators, overrides, zero/negative/NaN delays, empty/missing paths,
  absent delays and gaps in frame numbering. Allocation checks require reduced
  first-frame resolution churn whenever the accepted delay enters the frame loop.
  Separate checks require fewer aggregate scratch allocation calls and reduced
  material-resolution churn.

Validation completed:

```text
cargo test -p deadsync-noteskin --lib model_preparation --offline --locked
  5 passed, 1 manual benchmark ignored
cargo test -p deadsync-noteskin -p deadsync-assets --lib --release --offline --locked
  noteskin: 308 passed, 13 ignored
  assets:   157 passed, 4 ignored
cargo check --workspace --offline --locked
  passed
cargo clippy -p deadsync-noteskin -p deadsync-assets --lib --offline --locked --no-deps -- -D clippy::perf -A clippy::large_enum_variant
  passed; existing general warnings and the existing large-enum exception remain
rustfmt --check --edition 2024 <the five changed/new Rust files>
git diff --check
  passed
```

Both changed-library/asset-consumer release suites passed: 465 tests, zero
failures. The manual benchmark passed in all six runs. The workspace was
compiled; release behavior tests were scoped to these two library suites.

## Reproduction

Build the test binary, then run it serially on the same logical processor
after compilation finishes, alternating benchmark order:

```powershell
cargo test -p deadsync-noteskin --lib --release --offline --locked --no-run
$taskBenchProcess = Get-Process -Id $PID
$taskBenchProcess.ProcessorAffinity = [IntPtr]64
$taskBinary = Get-ChildItem target/release/deps -Filter 'deadsync_noteskin-*.exe' |
    Sort-Object LastWriteTime -Descending | Select-Object -First 1
foreach ($taskRun in 1..6) {
    $env:DEADSYNC_PERF_ORDER = if ($taskRun % 2 -eq 0) { 'new-first' } else { 'old-first' }
    & $taskBinary.FullName benchmark_model_preparation --ignored --nocapture --test-threads=1
    if ($LASTEXITCODE -ne 0) { throw "Benchmark $taskRun failed" }
}
Remove-Item Env:DEADSYNC_PERF_ORDER
```

The workspace version changes exactly once from 0.5.1653 to 0.5.1654, with the
three workspace-versioned packages updated in `Cargo.lock`. The fixture archive,
performance guideline and both optimization scripts are excluded from the commit.
