# Model compilation and sequential sheet loading - 0.5.1653

Parent: `1a2129e19` (0.5.1652). Date: 2026-10-01.

This pass follows `rust-performance.md`'s M-HOTPATH, M-MEM-REUSE,
M-INITIAL-CAPACITY and M-THROUGHPUT guidance in three production noteskin paths:

1. Borrow the initialization and active model scripts directly. The active-slot
   adapter previously built a temporary `HashMap`, allocated replacement keys,
   cloned script strings and looked up those cloned strings again. It now passes
   the two borrowed scripts to a private compiler entry point in the same order.
   The existing owning command-selection helper and public map compiler remain
   available. The short active-slot workload uses 31.8–46.7%
   fewer CPU cycles; its temporary map and script copies disappear.
2. Keep model modifier groups in an eight-element inline `SmallVec`. Modifiers
   still apply as a batch at the original tween, sleep, control and effect
   boundaries, in the same order. Common groups need no modifier allocation;
   larger groups retain the heap fallback and grow fewer times. The one-modifier
   workload uses 10.1–22.9% fewer CPU cycles. Static programs
   still return their owning, 16-byte empty timeline `Arc`.
3. Represent whole-sheet sequential frames through the existing identity
   fallback using an empty explicit index list. One immutable process-wide
   `Arc<[usize]>` supplies that list to each constructed source. This removes
   both the temporary range vector and each source's full owning index array.
   Keeping an explicit list preserves the indexed sheet origin, including
   nonzero slot offsets, later delay rebuilds and pause handling. Color-axis
   subsets keep their actual indices. One-frame animation axes reject before
   allocating their unused index vector. The 32-frame source workload uses
   29.3–31.3% fewer CPU cycles.

The active-slot path is used for hold/roll model visuals, and the model compiler
also serves resolved model declarations and actor layers. Sequential source
preparation is called by the production layer animation adapter. These
measurements cover those loading helpers and UV sampling, rather than complete
load time, texture decoding/upload or overall rendering frame rate.

No dependencies or unsafe production code were added. Zero scratch allocation
applies to inline modifier groups, borrowed command selection and warmed
sequential frame indices. Timelines, texture identifiers, source objects,
oversized modifier groups and required subset indices still allocate.
The shared empty index list initializes once, requesting
16 bytes on this x64 build, remains immutable and retains constant storage for
the process lifetime. It has no texture identity, growth or invalidation policy.

## Measurement method

The [raw CSV](model-and-sheet-loading-0.5.1653.csv) retains 240 measurements:
20 workloads, two implementations and six serial runs, alternating
old-first/new-first. Every median comes from seven timing batches, whose ranges
are also retained. Throughput is operations/s except the UV-sampling workload,
which reports sampled frames/s. CPU cycles use Windows `QueryThreadCycleTime`
on the calling thread. Allocation counting runs separately for one complete
operation, including destruction of its owning result. Counts agree across all
six runs for each workload and implementation.

Environment: Windows x64, Intel Xeon E5-2696 v4 at 2.20 GHz, 22 cores / 44
logical processors, Rust 1.98.1, LLVM 22.1.8. Release builds use opt-level 3 and
full LTO, with test unwinding. Processes run serially on logical processor 6
(affinity mask 64), using `--test-threads=1`, after compiler processes finish.
Both variants use the existing counting allocator wrapper over `System`, with
counting disabled during timing. Input strings, command maps, source fixtures
and sheet metadata are prepared outside the measured operation. Warmup also
initializes the shared empty index list. Requested and freed bytes include
reallocation sizes; they measure allocator churn, not process RSS or peak
resident memory.

The parent compiler and owning command selector are frozen in
`deadsync-noteskin/tests/model_loading/baseline_program.rs`. The parent active
slot adapter is frozen in `baseline_runtime.rs` and shares the new compiler
only for the isolated command-borrowing benchmarks. Its full output is also
checked against the frozen parent compiler. The parent sprite source builder
and source factory are both frozen in
`deadsync-assets/tests/sequential_animation/baseline.rs`, so the new shared-list
factory cannot affect the source-construction baseline. A source audit verifies
all five function bodies and compiler attributes against the parent. Other
unchanged parsers, types and UV arithmetic are shared.

12 of 20 workloads improve both CPU cycles and throughput in
every run. All recorded runs, including timing variations, are
retained. No consistent timing win is claimed for:
`load_model_empty`, `load_model_many`, `load_model_tween`, `load_model_timeline`, `load_model_lua`, `load_sheet_column`, `load_sheet_row`, `load_sheet_sampling`.
Use the per-workload ranges below to distinguish a repeatable gain from timing
variation; negative reduction values mean higher measured CPU cost. These
measurements establish gains for the targeted small groups, active commands
and sequential sheets, but do not establish a CPU improvement for every input.

## Model modifier compilation

Each operation compiles and destroys one complete program, reporting programs/s.
`empty` has no modifiers; `one`, `eight` and `many` have 1, 8 and 256 relative-X
modifiers. `tween` adds four modifiers with tween/sleep boundaries; `timeline`
repeats a tween/sleep group 16 times; `lua` exercises normalization of a Lua
method chain. There are 16,384 operations per batch, except `many` and `timeline`
use 2,048. Required timeline construction and destruction remain inside both
variants. The modifier allocation check additionally covers 1,024 commands
without imposing a new group-size limit.

| Workload | ns/op, old -> new (run 1) | Thread cycles/op, old -> new (run 1) | CPU reduction, six runs | Throughput gain, six runs | Alloc/realloc/free, old -> new | Requested bytes, old -> new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `load_model_empty` | 185.6 -> 189.3 | 407.2 -> 415.3 | -7.5% to 8.8% | -6.8% to 9.7% | 1/0/1 -> 1/0/1 | 16 -> 16 |
| `load_model_one` | 424.3 -> 348.2 | 930.6 -> 763.7 | 10.1% to 22.9% | 11.2% to 29.8% | 2/0/2 -> 1/0/1 | 96 -> 16 |
| `load_model_eight` | 1,593.6 -> 1,519.3 | 3,493.1 -> 3,330.2 | 4.7% to 21.8% | 4.9% to 27.9% | 2/1/2 -> 1/0/1 | 256 -> 16 |
| `load_model_many` | 38,347.1 -> 37,406.5 | 84,057.9 -> 81,987.0 | -16.0% to 6.6% | -13.6% to 7.1% | 2/6/2 -> 2/4/2 | 10176 -> 9936 |
| `load_model_tween` | 1,206.3 -> 1,087.9 | 2,644.3 -> 2,385.5 | -9.4% to 9.8% | -8.6% to 10.9% | 3/0/3 -> 2/0/2 | 1528 -> 1448 |
| `load_model_timeline` | 12,340.6 -> 12,458.8 | 27,031.1 -> 27,305.3 | -1.9% to 20.2% | -1.8% to 25.3% | 3/3/3 -> 2/3/2 | 18864 -> 18784 |
| `load_model_lua` | 1,397.4 -> 1,307.8 | 3,062.9 -> 2,867.6 | -3.7% to 8.8% | -3.5% to 9.6% | 4/0/4 -> 3/0/3 | 1170 -> 1090 |

## Borrowed active model scripts

Each operation clones the same stack-sized slot, compiles the selected scripts,
calls the same consumer once and destroys the returned program, reporting
slots/s. `short` selects initialization and holding scripts; `long` has 64
initialization modifiers and 32 active tweens. `alias` selects `initcommand`
as the active key, preserving the parent's twice-parsed initialization behavior.
`missing` retains initialization without a matching active script. `empty`
contains no commands. There are 16,384 operations per batch, except `long` uses
2,048. This isolates map/string removal by sharing the new model compiler in
both benchmark variants; the modifier table above isolates compiler changes.

| Workload | ns/op, old -> new (run 1) | Thread cycles/op, old -> new (run 1) | CPU reduction, six runs | Throughput gain, six runs | Alloc/realloc/free, old -> new | Requested bytes, old -> new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `load_active_empty` | 129.8 -> 117.7 | 284.7 -> 258.2 | 2.0% to 15.1% | 2.2% to 17.8% | 1/0/1 -> 1/0/1 | 16 -> 16 |
| `load_active_short` | 1,190.7 -> 736.6 | 2,610.5 -> 1,614.9 | 31.8% to 46.7% | 46.7% to 87.9% | 7/0/7 -> 2/0/2 | 1299 -> 1040 |
| `load_active_long` | 19,470.6 -> 18,991.1 | 42,663.4 -> 41,633.6 | 2.4% to 13.1% | 2.4% to 15.1% | 8/5/8 -> 3/5/3 | 22472 -> 21024 |
| `load_active_alias` | 1,215.9 -> 779.6 | 2,665.8 -> 1,708.1 | 30.7% to 42.6% | 44.3% to 74.7% | 6/0/6 -> 1/0/1 | 280 -> 16 |
| `load_active_missing` | 626.5 -> 340.5 | 1,373.6 -> 746.2 | 39.6% to 50.7% | 65.6% to 116.3% | 4/0/4 -> 1/0/1 | 247 -> 16 |

## Sequential sprite sources and sampling

Each source operation prepares and destroys a complete animation source, with
32,768 operations per batch, reporting sources/s. `two`, `sheet`, `large` and
`odd` use 2x1, 8x4, 16x16 and 3x5 sheets. Their slots have a nonzero [64, 128]
source offset to test indexed sheet-origin semantics. `column` and `row`
retain color-axis subsets. `reject` has only one usable animation frame after
color-axis selection and now returns without allocating. Source string and
object ownership remain inside measurement.

`sampling` processes 4,096 UV lookups per operation on already-created 8x4
sources, with 1,024 operations per batch. Divide its timing/cycle totals by
4,096 for per-frame values. Both sampling implementations have zero heap
churn; the new source avoids reading the full frame-index array.

| Workload | ns/op, old -> new (run 1) | Thread cycles/op, old -> new (run 1) | CPU reduction, six runs | Throughput gain, six runs | Alloc/realloc/free, old -> new | Requested bytes, old -> new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `load_sheet_two` | 379.1 -> 272.2 | 830.8 -> 596.4 | 21.1% to 31.1% | 26.5% to 45.4% | 5/0/5 -> 3/0/3 | 375 -> 327 |
| `load_sheet_sheet` | 411.0 -> 286.2 | 901.0 -> 627.5 | 29.3% to 31.3% | 41.5% to 45.7% | 5/0/5 -> 3/0/3 | 855 -> 327 |
| `load_sheet_large` | 498.6 -> 329.3 | 1,092.3 -> 722.0 | 28.9% to 46.4% | 40.8% to 86.8% | 5/0/5 -> 3/0/3 | 4449 -> 337 |
| `load_sheet_odd` | 466.3 -> 279.6 | 1,022.2 -> 612.9 | 28.0% to 40.0% | 39.0% to 66.8% | 5/0/5 -> 3/0/3 | 583 -> 327 |
| `load_sheet_column` | 372.7 -> 381.5 | 817.1 -> 835.4 | -18.7% to 6.1% | -15.8% to 6.5% | 5/0/5 -> 5/0/5 | 407 -> 407 |
| `load_sheet_row` | 383.5 -> 491.7 | 840.5 -> 1,077.7 | -28.2% to 3.7% | -22.0% to 3.8% | 5/0/5 -> 5/0/5 | 471 -> 471 |
| `load_sheet_reject` | 116.2 -> 44.6 | 254.9 -> 97.8 | 51.8% to 66.7% | 107.7% to 200.4% | 1/0/1 -> 0/0/0 | 8 -> 0 |
| `load_sheet_sampling` | 14,665.3 -> 14,942.2 | 32,144.7 -> 32,755.9 | -1.9% to 8.8% | -1.9% to 9.8% | 0/0/0 -> 0/0/0 | 0 -> 0 |

## Behavior and validation

Eight new ordinary tests check behavior and allocations against frozen parent
implementations:

- Model draw states and every tween endpoint match bit for bit, including
  signed zeros and non-finite values. Effect parameters, tween types, times
  and durations match. Coverage includes effects, base zoom, ignored UV/depth
  commands, invalid commands, Lua syntax, every pair of 14 script fragments,
  512 deterministic generated script pairs and groups up to 1,024 modifiers.
- Active model selection covers missing initialization/active entries, empty
  scripts, aliases, unrelated keys, absent keys, exactly one callback, retained
  slot fields and preservation of the original input slot.
- Animation sources match frame counts, effective index sequences, rates,
  dimensions and exact UV bits across sequential/subset sheets, zero/negative/
  nonzero offsets, unusual sizes, color-axis boundaries and non-finite lengths.
  Later delay/property rebuilds, frame overrides, seeking and pausing retain
  equal definitions and output. Model and already-animated fallback paths
  remain allocation-free.
- Scoped allocation assertions require reduced model/map/source churn, only
  the returned empty timeline allocation for static groups up to eight modifiers,
  fewer growth reallocations/bytes for oversized groups, and no unused
  subset-vector allocation on warmed one-frame rejections. Shared sheet metadata
  is warmed before those checks, so its first key registration is excluded.

The existing layer-animation regression snapshot now canonicalizes equivalent
full, empty and implicit sequential index representations. It still compares
all other stored fields, owner preservation, sharing relationships, animation
playback and UV output; the new tests independently exercise the representation
through later animation operations.

Validation completed:

```text
cargo test -p deadsync-noteskin --lib model_loading_perf --offline --locked
  4 passed, 1 manual benchmark ignored
cargo test -p deadsync-assets --lib sequential_animation --offline --locked
  4 passed, 1 manual benchmark ignored
cargo test -p deadsync-noteskin -p deadsync-assets --lib --release --offline --locked
  noteskin: 303 passed, 12 ignored
  assets:   157 passed, 4 ignored
cargo check --workspace --offline --locked
  passed
cargo clippy -p deadsync-noteskin -p deadsync-assets --lib --offline --locked --no-deps -- -D clippy::perf -A clippy::large_enum_variant
  passed; existing general warnings and the existing large-enum exception remain
rustfmt --check --edition 2024 <the ten changed/new Rust files>
git diff --check
  passed
```

Both crates' full release library suites passed, including bundled noteskin
loaders and asset consumers. Both manual benchmarks passed in all six runs.
The workspace was compiled; release behavior tests were scoped to the changed
crates and their asset loading consumers.

## Reproduction

Build the test binaries, then alternate benchmark order on the same processor
after builds finish. Run the two tests serially; `--nocapture` prints batch
medians/ranges, cycles, throughput and allocator counts.

```powershell
cargo test -p deadsync-noteskin -p deadsync-assets --lib --release --offline --locked --no-run
$taskBenchProcess = Get-Process -Id $PID
$taskBenchProcess.ProcessorAffinity = [IntPtr]64
$taskModelBinary = Get-ChildItem target/release/deps -Filter 'deadsync_noteskin-*.exe' |
    Sort-Object LastWriteTime -Descending | Select-Object -First 1
$taskSheetBinary = Get-ChildItem target/release/deps -Filter 'deadsync_assets-*.exe' |
    Sort-Object LastWriteTime -Descending | Select-Object -First 1
foreach ($taskRun in 1..6) {
    $env:DEADSYNC_PERF_ORDER = if ($taskRun % 2 -eq 0) { 'new-first' } else { 'old-first' }
    & $taskModelBinary.FullName benchmark_model_loading --ignored --nocapture --test-threads=1
    if ($LASTEXITCODE -ne 0) { throw "Model benchmark $taskRun failed" }
    & $taskSheetBinary.FullName benchmark_sequential_animation --ignored --nocapture --test-threads=1
    if ($LASTEXITCODE -ne 0) { throw "Sheet benchmark $taskRun failed" }
}
Remove-Item Env:DEADSYNC_PERF_ORDER
```

DeadSync's workspace version changes exactly once from 0.5.1652 to 0.5.1653;
`Cargo.lock` updates the three workspace-versioned packages accordingly.
The fixture archive, performance guideline and optimization scripts are
excluded from the commit.
