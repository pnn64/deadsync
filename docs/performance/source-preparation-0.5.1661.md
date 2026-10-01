# Model geometry and animation source performance 0.5.1661

Parent: `000a47b8c` (0.5.1660). Date: 2026-10-01.

Three optimizations apply `rust-performance.md`'s M-HOTPATH, M-INITIAL-CAPACITY,
M-MEM-REUSE and M-THROUGHPUT guidance to model preparation and runtime animation
updates:

1. Build transformed model vertices directly in their final Arc in both
   `deadsync-assets` and `deadsync-notefield`. The parent first filled a Vec,
   then allocated and copied the output Arc. The exact-length slice map
   allocates one final buffer. Texture mode is computed once per mesh; empty
   geometry shares immutable storage after first use. The note-field builder
   serves model geometry cache misses and uncached model actors. The asset
   builder serves song Lua and themed model backgrounds.
2. Build model animation delays directly in their final Arc. Preparing the
   source no longer stages the delays in a Vec or routes them through an
   otherwise unused sprite slot plan. Image header validation, atlas layout,
   decoding, registry lookup and generated-texture registration are unchanged.
   Metadata preparation removes one allocation and its matching free at all
   tested sizes. The 16-frame uniform fixture uses
   8.0% to 25.1% fewer CPU cycles.
3. Reuse the delay array for uniform runtime overrides when the source and
   array are uniquely owned and the array length matches the normalized frame
   count. Shared or weak owners and missing/mismatched arrays get a replacement.
   Large replacement arrays use an exact-length `repeat_n` iterator, avoiding
   a staging Vec above 64 frames. Small replacements retain the parent's
   bounded stack copy. A strong-count check skips the mutation probe when the
   source is already shared. Timing and UV caches are rebuilt
   with the same definition and rate rules. Unique updates retain zero
   allocator churn, including at 1024 frames. The 16-frame unique fixture uses
   27.1% to 51.0% fewer CPU cycles.

No dependencies or unsafe production code were added. Regression comparisons
retain every emitted vertex field, source metadata, delay bits, animation
timing, UVs, initial render cache state, validation errors and ownership rules.
The in-memory geometry and metadata benchmarks measure preparation cost;
they do not establish a gameplay frame-rate change. Geometry cache hits are
unchanged. Runtime overrides are used during noteskin command application.

The two shared empty geometry arrays each retain a 16-byte Arc header on this
target. First use is outside warmed measurements. Reused delay buffers retain
the same exact-length Arc allocation; no oversized Vec capacity is retained.
Requested/freed bytes below measure allocator traffic. Process RSS, GPU upload,
peak live memory and system-wide CPU work were not measured.

## Measurement method

The [raw CSV](source-preparation-0.5.1661.csv) retains 264 measurements:
22 workloads, two implementations and six serial rounds alternating
old-first/new-first. Each median uses seven timing batches; the CSV also keeps
batch minima/maxima. Windows QueryThreadCycleTime counts calling-thread CPU
cycles. The existing System allocator wrapper counts calls and requested/freed
bytes in a separate operation, disabled during timing.

Environment: Windows x64, Intel Xeon E5-2696 v4 at 2.20 GHz, 22 cores / 44 logical
processors, Rust 1.98.1, LLVM 22.1.8. Release uses opt-level 3 and full LTO with
test unwinding. Compilation and checks finish before measurements. Each
executable runs on logical processor 6 (affinity mask 64), verified by the
runner, with --test-threads=1. Three untimed operations warm each workload.

All CSV throughput values are operations/s. One geometry operation builds and
destroys one output vertex Arc; input meshes are borrowed. Metadata operations
include an identical input key allocation, output construction and destruction.
Registered-source operations execute the complete source function against an
already registered atlas, including image header I/O and key construction.
They exclude atlas decoding/registration. Unique runtime operations repeatedly
update one initialized slot; its initial construction and final destruction
are outside the measurement. Shared runtime operations clone the same retained
template and drop the complete result inside measurement. Owning runtime
operations construct and initialize a fresh slot using the parent initializer,
perform one override and drop it, equally in both variants. Every counted
operation balances requested/freed bytes, and all five churn metrics remain
stable across six rounds. Registered-source calls include one unchanged
reallocation in both implementations; the in-memory workloads do not reallocate.

Four complete function bodies are frozen from the parent: both geometry
builders, model animation source preparation and uniform delay application.
The model metadata baseline isolates the exact unchanged planning tail.
An audit compares those bodies and the extracted tail with the parent, and
verifies that source constructors, state-property application, image copying,
decoding, model application, source replacement and pre-metadata error/registry
logic are unchanged. Common unchanged types/helpers are shared.

13 of 22 workloads improve CPU cycles and throughput in every round.
Workloads without consistent improvement in both metrics: `source_metadata_one`, `source_metadata_mixed129`, `source_metadata_uniform1024`, `source_registered_frames16`, `source_registered_frames1024`, `source_runtime_shared16`, `source_runtime_shared1024`, `source_runtime_owning1024`, `source_notefield_triangle`.
Workloads with higher CPU cost in every round: `source_registered_frames16`.
Negative reductions mean higher measured cost. All results are retained;
allocation savings alone do not establish a CPU improvement.

Several small and I/O-heavy fixtures vary between rounds; this experiment does
not isolate the cause. The complete registered 16-frame source call uses
0.1% to 3.3%
more CPU in these rounds, while removing one allocation. That is an explicit
memory/CPU tradeoff, and the isolated metadata gain is not claimed for that
complete call. Large model metadata also retains its allocation savings without
a consistent measured CPU improvement. Runtime shared-small replacement keeps
the same allocation count as the parent and has mixed CPU results; zero-churn
claims apply to uniquely owned, matching-length storage.

## Asset model geometry

Meshes contain 0, 3, 128, 4096 or 32768 vertices. The nonempty fixtures mirror
the horizontal axis and exercise ordinary or additive/sphere-mapped texture
modes. Batch sizes are respectively 1024, 1024, 512, 64 and 16 operations.

| Workload | ns/op, old -> new (round 1) | Thread cycles/op, old -> new (round 1) | CPU reduction, six rounds | Throughput gain, six rounds | Alloc/realloc/free, old -> new | Requested bytes, old -> new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `source_geometry_empty` | 78.6 -> 21.3 | 174.1 -> 48.2 | 67.6% to 73.2% | 215.0% to 282.1% | 1/0/1 -> 0/0/0 | 16 -> 0 |
| `source_geometry_triangle` | 187.6 -> 89.9 | 413.5 -> 200.2 | 35.6% to 51.6% | 55.6% to 108.6% | 2/0/2 -> 1/0/1 | 380 -> 200 |
| `source_geometry_mesh128` | 1,180.3 -> 727.5 | 2,594.1 -> 1,600.4 | 38.3% to 56.1% | 62.2% to 131.6% | 2/0/2 -> 1/0/1 | 15376 -> 7696 |
| `source_geometry_mesh4096` | 56,842.2 -> 19,489.1 | 124,730.9 -> 42,733.9 | 63.4% to 65.8% | 173.6% to 192.8% | 2/0/2 -> 1/0/1 | 491536 -> 245776 |
| `source_geometry_mesh32768` | 1,637,225.0 -> 810,493.8 | 3,582,198.9 -> 1,775,329.7 | 49.0% to 50.8% | 95.9% to 103.0% | 2/0/2 -> 1/0/1 | 3932176 -> 1966096 |

## Note-field model geometry

These independently benchmark the generic note-field builder through a
NoteskinSlot implementation, with texture mode 7 and both mirror axes. Mesh
sizes and batch counts match the asset geometry group. Cache behavior is also
covered by regression tests.

| Workload | ns/op, old -> new (round 1) | Thread cycles/op, old -> new (round 1) | CPU reduction, six rounds | Throughput gain, six rounds | Alloc/realloc/free, old -> new | Requested bytes, old -> new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `source_notefield_empty` | 78.4 -> 25.5 | 173.6 -> 57.4 | 65.0% to 78.4% | 190.2% to 372.4% | 1/0/1 -> 0/0/0 | 16 -> 0 |
| `source_notefield_triangle` | 177.1 -> 85.0 | 389.9 -> 188.0 | -52.9% to 57.7% | -34.6% to 141.9% | 2/0/2 -> 1/0/1 | 380 -> 200 |
| `source_notefield_mesh128` | 1,234.0 -> 562.9 | 2,711.6 -> 1,238.5 | 44.6% to 58.2% | 80.1% to 139.6% | 2/0/2 -> 1/0/1 | 15376 -> 7696 |
| `source_notefield_mesh4096` | 60,310.9 -> 19,546.9 | 132,265.9 -> 42,936.3 | 66.3% to 68.0% | 197.9% to 212.7% | 2/0/2 -> 1/0/1 | 491536 -> 245776 |
| `source_notefield_mesh32768` | 1,656,675.0 -> 805,112.5 | 3,626,908.3 -> 1,762,941.8 | 49.7% to 51.7% | 98.4% to 106.9% | 2/0/2 -> 1/0/1 | 3932176 -> 1966096 |

## Model animation metadata

Fixtures contain 1, 16, 129 or 1024 delays. Uniform values are 0.125; mixed values
include zero, negative zero, negative values, fractional values, NaN and both
infinities. Delay bits are preserved without clamping. The grid matches the
atlas's square-root column rule. Batches contain 512 operations.

| Workload | ns/op, old -> new (round 1) | Thread cycles/op, old -> new (round 1) | CPU reduction, six rounds | Throughput gain, six rounds | Alloc/realloc/free, old -> new | Requested bytes, old -> new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `source_metadata_one` | 381.6 -> 425.0 | 840.3 -> 925.6 | -10.2% to 32.9% | -10.2% to 51.4% | 5/0/5 -> 4/0/4 | 333 -> 329 |
| `source_metadata_uniform16` | 402.9 -> 335.7 | 886.6 -> 739.5 | 8.0% to 25.1% | 8.0% to 34.4% | 5/0/5 -> 4/0/4 | 449 -> 385 |
| `source_metadata_mixed129` | 1,153.9 -> 952.1 | 2,535.8 -> 2,084.4 | -8.1% to 27.1% | -8.3% to 35.8% | 5/0/5 -> 4/0/4 | 1357 -> 841 |
| `source_metadata_uniform1024` | 3,683.6 -> 3,427.5 | 8,079.1 -> 7,526.4 | -10.1% to 9.3% | -9.8% to 10.3% | 5/0/5 -> 4/0/4 | 8513 -> 4417 |

## Registered model animation sources

Both complete calls use a 2x3 PNG and an already registered atlas, with 16 or
1024 frames. Batches contain 128 operations. Identical image header I/O and
registry/key work can dominate the small metadata savings; this group retains
that cost so isolated metadata results are not presented as full-call gains.

| Workload | ns/op, old -> new (round 1) | Thread cycles/op, old -> new (round 1) | CPU reduction, six rounds | Throughput gain, six rounds | Alloc/realloc/free, old -> new | Requested bytes, old -> new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `source_registered_frames16` | 90,591.4 -> 91,585.2 | 198,311.4 -> 200,364.1 | -3.3% to -0.1% | -3.2% to 0.0% | 18/1/18 -> 17/1/17 | 29924 -> 29860 |
| `source_registered_frames1024` | 94,038.3 -> 93,828.1 | 205,856.7 -> 205,609.8 | -0.9% to 2.1% | -0.9% to 2.2% | 18/1/18 -> 17/1/17 | 37988 -> 33892 |

## Runtime uniform delay updates

Unique updates reuse one initialized slot. Shared updates clone a retained
source and must preserve its old delays. Owning updates include a fresh slot's
construction and parent initialization. Each applies a beat-based 0.125 delay;
all batches contain 512 operations. These groups include rebuilding timing and
UV caches, not just filling the array.

| Workload | ns/op, old -> new (round 1) | Thread cycles/op, old -> new (round 1) | CPU reduction, six rounds | Throughput gain, six rounds | Alloc/realloc/free, old -> new | Requested bytes, old -> new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `source_runtime_unique16` | 198.6 -> 92.0 | 417.6 -> 204.5 | 27.1% to 51.0% | 37.7% to 115.9% | 1/0/1 -> 0/0/0 | 80 -> 0 |
| `source_runtime_shared16` | 348.4 -> 303.5 | 758.8 -> 668.8 | -16.0% to 24.6% | -13.8% to 32.8% | 2/0/2 -> 2/0/2 | 344 -> 344 |
| `source_runtime_owning16` | 792.4 -> 600.6 | 1,738.0 -> 1,321.3 | 2.1% to 25.5% | 1.5% to 34.0% | 5/0/5 -> 4/0/4 | 481 -> 401 |
| `source_runtime_unique1024` | 2,808.6 -> 2,473.6 | 6,134.4 -> 5,419.8 | 5.1% to 14.2% | 6.2% to 16.4% | 2/0/2 -> 0/0/0 | 8208 -> 0 |
| `source_runtime_shared1024` | 2,794.7 -> 2,621.1 | 6,130.6 -> 5,756.7 | -3.3% to 22.1% | -3.1% to 28.5% | 3/0/3 -> 2/0/2 | 8472 -> 4376 |
| `source_runtime_owning1024` | 5,727.1 -> 5,393.4 | 12,559.1 -> 11,829.4 | -0.6% to 7.7% | -0.6% to 8.4% | 7/0/7 -> 5/0/5 | 16737 -> 8529 |

## Regression validation

Nine new ordinary tests compare against frozen parent behavior and assert
allocation budgets. They cover:

- Exact vertex bits for model texture modes, both mirror axes, rotations,
  empty and large meshes, signed zero, subnormal values, NaN and infinities.
- One final nonempty geometry allocation, shared empty storage, retained
  note-field geometry cache identity and the non-model panic.
- Model delay bits, rate, frame timing, UVs, cache initialization, negative
  clocks, frame boundaries, large clocks and nonfinite values.
- Both cold-first registration orders, warm atlas reuse, empty sequences,
  missing and corrupt files, oversized atlases and later-frame decode errors.
- Uniform override behavior for authored counts 0, 1, 2, 16, 64, 65, 129 and 1024
  (initialization normalizes zero to one before the override);
  missing/mismatched arrays, source and delay strong/weak owners, beat/time
  rates, zero/negative/NaN/infinite delays, and atlas rejection.
- Zero churn and stable array pointers for unique overrides, preserved shared
  contents, and reduced complete owning churn for large shared replacements.

Validation passed:

```text
cargo test -p deadsync-assets -p deadsync-notefield --lib preparation --locked --offline
cargo test -p deadsync-assets --lib model_source_preparation --locked --offline
cargo test -p deadsync-assets -p deadsync-noteskin -p deadsync-notefield --lib --release --locked --offline
cargo check --workspace --locked --offline
cargo clippy -p deadsync-assets -p deadsync-notefield --lib --locked --offline -- -D clippy::perf -A clippy::large_enum_variant
rustfmt --edition 2024 --check --config skip_children=true <changed Rust files>
git diff --check
```

The complete release library suites pass 952 tests, with 27 manual
benchmarks ignored. The two new manual benchmarks pass in all six rounds.
Performance lint checks pass; existing style warnings in the affected crates
and their dependencies remain. Validation uses library targets; the previously
identified unrelated all-targets safe_parsing integration failure involving
private script_random is outside this pass's test scope. It does not imply
that every workspace integration target passes.

Cargo.toml and Cargo.lock change only 0.5.1660 -> 0.5.1661 (including the three
workspace packages that inherit the version). The commit excludes
deadsync-song.json.gz, rust-performance.md, optimize.sh and optimize.ps1.
