# Sprite initialization performance 0.5.1658

Parent: `3e6ac31e2` (0.5.1657). Date: 2026-10-01.

Three changes follow `rust-performance.md`'s M-HOTPATH, M-MEM-REUSE and
M-THROUGHPUT guidance in sprite loading and animation command preparation:

1. Share the two immutable empty model arrays used by newly loaded sprites.
   Previously every slot allocated separate Arc headers for its empty model
   timeline and rotation-key list. Two lazy static arrays now serve all slots,
   removing two allocations and two frees per slot after their first use.
   The atlas fixture uses 27.0% to 33.5% fewer CPU cycles.
2. Apply StateProperties directly from the existing source and definition.
   The old path copied its key, indices and durations into a temporary slot
   plan, then copied the key again, even when the requested animation could
   not be applied. The new path borrows source metadata, invokes the same
   noteskin animation planner, and retains the existing texture key. Old
   metadata is neither cloned nor converted to temporary vectors. The indexed
   fixture uses 32.3% to 39.9% fewer CPU cycles.
3. Apply AllStateDelays directly, preserving the existing texture key and
   frame-index Arc. Up to 64 uniform delays are built using 256 bytes of stack
   scratch and allocated directly into their output Arc; larger animations
   retain the Vec conversion fallback. The existing source allocation is
   reused when Arc::get_mut grants exclusive access. Shared or weakly
   referenced sources get a fresh source value. The 64-frame indexed fixture
   uses 45.1% to 49.1% fewer CPU cycles.

Both direct commands rebuild the same UV/timing caches and reset the same
texture-cache fields as the old source constructor. Frame-delay normalization,
rate arithmetic, sheet origin/clamping, model bypass and rejected commands
keep their old behavior. The public noteskin planners are unchanged.

No dependencies or unsafe production code were added. These are asset loading
and initialization operations; gameplay frame rate, file I/O, texture upload,
peak live memory and process RSS were not measured. The empty static arrays
retain two Arc headers for process lifetime, about 32 bytes on this target;
their one-time allocation is outside warmed benchmarks. Required source and
duration results still own their output allocations.

## Measurement method

The [raw CSV](sprite-initialization-0.5.1658.csv) retains 180 measurements:
fifteen workloads, two implementations and six serial rounds alternating
old-first/new-first. Each median uses seven timing batches. Every elapsed
batch minimum/maximum remains in the CSV. Windows QueryThreadCycleTime
measures calling-thread CPU cycles. Allocation counting runs separately
through the existing counting System allocator, disabled during timing.

Environment: Windows x64, Intel Xeon E5-2696 v4 at 2.20 GHz, 22 cores / 44 logical
processors, Rust 1.98.1, LLVM 22.1.8. Release uses opt-level 3 and full LTO with
test unwinding. The test executable runs on logical processor 6 (affinity mask
64), with --test-threads=1, after compilation and checks finish. The runner
verifies each process's affinity. Inputs and sheet/source metadata are warmed
before timing; no filesystem lookup is measured.

Five functions are frozen exactly from the parent in
`crates/deadsync-assets/tests/sprite_initialization/baseline.rs`: source and
slot constructors, slot-to-plan conversion, plan application and animation
command application. A source audit compares all bodies and attributes with
the parent. Unchanged noteskin planners and metadata helpers are shared.
The frame-override path, initial state/seek/pause logic, command ordering,
model atlas building and existing freeze implementation are unchanged.

14 of fifteen workloads improve both CPU cycles and throughput
in every round. All measurements are retained. Workloads without consistent
improvement in both metrics: `init_model_noop`.
Workloads with higher CPU cost in every round: none.
Negative reductions indicate higher measured cost, rather than a gain.

The model bypass control has mixed measured CPU changes: 35.3% higher cost
to 9.9% lower cost. Both variants retain zero allocations and frees. The
small dispatcher carries an inline hint, but these measurements establish
no consistent timing improvement for already-bypassed model commands.

## Shared empty defaults

Every operation creates and destroys 64 slots from cloned declarations.
Timing includes declaration cloning, source preparation and output destruction;
batches contain 256 operations. Atlas, animated and explicitly indexed plans
exercise the three declaration shapes. Throughput is created slots/s.
The two empty headers are the only allocation change being claimed here;
declaration cloning and required key/source/index/duration allocations remain.

| Workload | ns/op, old -> new (round 1) | Thread cycles/op, old -> new (round 1) | CPU reduction, six rounds | Throughput gain, six rounds | Alloc/realloc/free, old -> new | Requested bytes, old -> new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `init_defaults_atlas` | 21,730.5 -> 15,111.3 | 47,482.3 -> 33,111.9 | 27.0% to 33.5% | 36.3% to 50.4% | 320/0/320 -> 192/0/192 | 22720 -> 20672 |
| `init_defaults_animated` | 30,185.9 -> 24,749.2 | 66,084.9 -> 53,836.7 | 16.0% to 18.5% | 18.6% to 22.6% | 448/0/448 -> 320/0/320 | 25792 -> 23744 |
| `init_defaults_indexed` | 37,477.3 -> 32,068.8 | 81,671.2 -> 69,470.0 | 13.4% to 24.8% | 15.1% to 33.8% | 576/0/576 -> 448/0/448 | 30912 -> 28864 |

## Direct animation commands

Each operation clones, prepares and destroys 64 slots, with a held template
keeping the original source shared. Command cloning is included equally in
both variants. Batches contain 256 operations; throughput is prepared slots/s.
Uniform-small uses 7 frames; uniform-indexed uses 64; uniform-large uses 129
and exercises the heap fallback. Uniform-static is a rejected atlas command.
State-atlas/state-indexed replace an atlas or indexed source with 32 frames.
State-long-key has a 1032-byte key. State-static and state-one reject an
inapplicable sheet or one-frame request. AllStateDelays retains index storage;
StateProperties still clears explicit indices, as the parent did.

| Workload | ns/op, old -> new (round 1) | Thread cycles/op, old -> new (round 1) | CPU reduction, six rounds | Throughput gain, six rounds | Alloc/realloc/free, old -> new | Requested bytes, old -> new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `init_command_uniform_small` | 30,464.5 -> 17,344.5 | 66,705.7 -> 37,214.7 | 44.2% to 60.0% | 75.6% to 152.2% | 384/0/384 -> 128/0/128 | 26560 -> 19968 |
| `init_command_uniform_indexed` | 47,236.7 -> 25,785.2 | 103,049.2 -> 56,525.5 | 45.1% to 49.1% | 82.6% to 97.8% | 512/0/512 -> 128/0/128 | 60608 -> 34304 |
| `init_command_uniform_large` | 54,781.6 -> 39,055.5 | 119,265.7 -> 85,087.1 | 28.7% to 38.4% | 40.3% to 62.9% | 512/0/512 -> 192/0/192 | 94144 -> 84224 |
| `init_command_uniform_static` | 10,396.9 -> 4,754.7 | 22,677.9 -> 10,355.9 | 50.2% to 54.3% | 100.4% to 120.7% | 64/0/64 -> 0/0/0 | 1216 -> 0 |
| `init_command_state_atlas` | 52,378.5 -> 40,273.0 | 113,725.0 -> 87,224.7 | 23.3% to 25.2% | 30.1% to 35.0% | 448/0/448 -> 256/0/256 | 39808 -> 34816 |
| `init_command_state_indexed` | 55,254.7 -> 36,280.5 | 119,655.8 -> 78,931.7 | 32.3% to 39.9% | 45.9% to 64.9% | 576/0/576 -> 256/0/256 | 42880 -> 34816 |
| `init_command_state_long_key` | 239,982.8 -> 225,480.1 | 523,389.2 -> 490,838.9 | 6.2% to 9.0% | 3.0% to 10.2% | 576/0/576 -> 256/0/256 | 237056 -> 34816 |
| `init_command_state_static` | 17,573.8 -> 9,378.1 | 38,158.7 -> 20,557.6 | 44.8% to 46.4% | 81.8% to 87.8% | 192/0/192 -> 64/0/64 | 2432 -> 512 |
| `init_command_state_one` | 24,800.8 -> 9,525.0 | 53,829.8 -> 20,756.5 | 59.7% to 63.5% | 150.9% to 177.3% | 320/0/320 -> 64/0/64 | 5760 -> 256 |

The model bypass is an unchanged control, with the same complete cloned-slot
lifetime and zero allocations in both variants.

| Workload | ns/op, old -> new (round 1) | Thread cycles/op, old -> new (round 1) | CPU reduction, six rounds | Throughput gain, six rounds | Alloc/realloc/free, old -> new | Requested bytes, old -> new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `init_model_noop` | 5,116.8 -> 4,921.1 | 11,194.5 -> 10,792.4 | -35.3% to 9.9% | -30.4% to 13.1% | 0/0/0 -> 0/0/0 | 0 -> 0 |

## Unique source allocation reuse

Each operation prepares 64 freshly created, uniquely owned slots. Setup and
final slot destruction occur outside timing/counting through
measure_sampled_with_setup, with 128 operations per timing batch. Command
cloning remains inside the operation. The original source/key/index/duration
buffers can be freed during replacement, so requested/freed bytes differ.
They are allocator churn, rather than peak or retained memory.

For `init_unique_uniform`, old requested/freed bytes are 60,608/45,248; new values are 17,408/2,048. For `init_unique_state`, old requested/freed bytes are 42,880/38,784; new values are 17,920/13,824.

| Workload | ns/op, old -> new (round 1) | Thread cycles/op, old -> new (round 1) | CPU reduction, six rounds | Throughput gain, six rounds | Alloc/realloc/free, old -> new | Requested bytes, old -> new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `init_unique_uniform` | 53,829.7 -> 25,629.7 | 118,732.3 -> 57,207.2 | 45.2% to 55.1% | 86.8% to 125.4% | 512/0/512 -> 64/0/64 | 60608 -> 17408 |
| `init_unique_state` | 55,123.4 -> 31,173.4 | 121,640.7 -> 69,608.9 | 42.8% to 45.6% | 76.8% to 86.6% | 576/0/640 -> 192/0/256 | 42880 -> 17920 |

## Behavior and validation

Seven new ordinary tests verify complete slot state against the parent,
separate cache identities, immutable empty-array sharing, and:

- AllStateDelays float bits for eleven delays including signed zero,
  subnormals, negative values, ordinary/boundary values, infinities and NaN.
  Eight requested frame counts cover the inline/fallback boundary. Identity,
  explicit-empty and partial indexed lists preserve timing, frame selection
  and UV bits for both second-based and beat-based clocks.
- StateProperties for four sheet names, atlas/animated sources, four origins,
  seven requested counts and four delay patterns. Rejection, origin clamping,
  doubled-resolution source size, fallback delays and every resulting rate,
  duration and sampled UV bit match the parent.
- Unique sources retain their allocation address; shared owners keep their
  original animation/cache state; weak references expire as before. Commands
  reset populated texture-cache fields exactly like the parent constructor.
- Model/static bypasses retain state and source identity without churn.
  Chained commands, seeks and final freezing preserve the parent results.
- Complete owning construction/command operations reduce allocations, frees
  and requested/freed bytes. Ordinary uniform commands add no heap scratch.

Validation: seven new debug tests; the full release library suites
(173 assets + 321 noteskin = 494 passed, 23 manual benchmarks ignored);
workspace check; performance Clippy; rustfmt on all three changed Rust files;
frozen-source audit; git diff --check. Performance Clippy uses
-D clippy::perf -A clippy::large_enum_variant for the preexisting large enum;
style warnings are retained.

Reproduce checks and paired benchmarks from PowerShell:

```powershell
cargo test -p deadsync-assets -p deadsync-noteskin --lib --release --offline --locked
cargo check --workspace --offline --locked
cargo clippy -p deadsync-assets --lib --offline --locked --no-deps -- -D clippy::perf -A clippy::large_enum_variant
$env:DEADSYNC_PERF_ORDER = 'old-first' # alternate with new-first each round
# After building, run serially on one core for comparable measurements.
cargo test -p deadsync-assets --lib --release --offline --locked benchmark_sprite_initialization -- --ignored --nocapture --test-threads=1
```
