# Sprite preparation performance 0.5.1659

Parent: `d8b34ebb9` (0.5.1658). Date: 2026-10-01.

Three changes apply `rust-performance.md`'s M-HOTPATH, M-MEM-REUSE and
M-THROUGHPUT guidance to sprite loading and animation preparation:

1. Static frame selection retains the texture-key Arc and rebuilds the atlas
   source directly. The old path allocated a temporary String, copied it into
   another Arc, and allocated a new source. The new path reuses source storage
   when Arc::get_mut grants exclusive access. Shared or weakly referenced
   sources receive a fresh source without changing other owners. The static
   shared-source fixture uses 34.4% to 37.3% fewer CPU cycles;
   the unique-source fixture makes zero allocations or frees during mutation.
2. Note-part animation preparation retains the texture-key Arc and builds
   its animated source directly. Color lanes up to 64 frames construct their
   final index Arc from 512 bytes of stack scratch, removing the temporary
   index Vec. Larger lanes retain their Vec fallback. Sequential animations
   retain the shared explicit empty index array and its sheet-origin behavior.
   The color-X fixture uses 42.2% to 48.2% fewer CPU cycles.
3. StateProperties consumes its owned delay buffer. A new owned noteskin
   planner normalizes and truncates that buffer in place when capacity fits,
   and pads it without growing the buffer. When capacity is insufficient,
   it keeps the old fresh-buffer normalization path, avoiding new reallocations.
   The existing borrowed planner shares geometry preparation and retains its
   original allocation behavior. The exact 32-delay fixture uses
   12.4% to 23.3% fewer CPU cycles.

All three paths retain frame selection, sheet origin/clamping, rate arithmetic,
duration bits, UV/timing construction and texture-cache reset behavior. Static
selection retains the original source size, mirrors, rotation and animation
offsets. Animated selection still seeks within the existing source. Required
output sources, index arrays and duration arrays still own allocations.

No dependencies or unsafe production code were added. These measurements
cover asset preparation, with warmed metadata. File I/O, GPU upload, gameplay
frame rate, process RSS and peak live memory were not measured. Requested and
freed bytes describe allocation traffic, rather than retained process memory.

## Measurement method

The [raw CSV](sprite-preparation-0.5.1659.csv) retains 192 measurements:
16 workloads, two implementations and six serial rounds alternating
old-first/new-first. Each median uses seven timing batches; batch elapsed-time
minima and maxima are retained in the CSV. Windows QueryThreadCycleTime counts
calling-thread CPU cycles. Allocation counting uses the existing System
allocator wrapper in a separate operation, disabled during timing.

Environment: Windows x64, Intel Xeon E5-2696 v4 at 2.20 GHz, 22 cores / 44 logical
processors, Rust 1.98.1, LLVM 22.1.8. Release uses opt-level 3 and full LTO with
test unwinding. After compilation and checks finish, the test executable runs
on logical processor 6 (affinity mask 64), with --test-threads=1. The runner
verifies each process's affinity. Inputs and sheet/source metadata are warmed
before timing, including the shared empty index array.

Six functions are frozen from the parent in
`crates/deadsync-assets/tests/sprite_preparation/baseline.rs`: the source
constructor, frame override, note animation, command dispatcher, state-command
application and borrowed noteskin state planner. An audit compares their bodies
and attributes with the parent. Ten unchanged source builders, metadata-facing
wrappers and other command/seek/freeze helpers are also checked against it.

12 of 16 workloads improve CPU cycles and throughput in every
round. All results are retained. Workloads without consistent improvement in
both metrics: `prep_frame_animated_seek`, `prep_note_rejected`, `prep_state_padded`, `prep_state_truncated`.
Workloads with higher CPU cost in every round: none.
Negative reductions indicate higher measured cost. Already-bypassed note
commands and animated frame seeks remain zero-churn controls; the padded
state fixture cannot reuse its undersized input and retains its old churn.

## Static frame selection

Shared-source operations clone, prepare and destroy 64 slots from a held
template. Timing includes output destruction, with 256 operations per batch;
throughput is prepared slots/s. The long-key fixture has a 1032-byte key.
Animated-seek is a control that changes the slot's start frame/time while
retaining its source. Unique-source operations use 64 freshly prepared slots
per operation, with setup and final slot destruction outside timing/counting,
and 128 operations per batch. Their mutation still counts old-source releases.

| Workload | ns/op, old -> new (round 1) | Thread cycles/op, old -> new (round 1) | CPU reduction, six rounds | Throughput gain, six rounds | Alloc/realloc/free, old -> new | Requested bytes, old -> new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `prep_frame_static` | 21,442.2 -> 13,641.8 | 46,901.8 -> 29,913.7 | 34.4% to 37.3% | 51.0% to 59.6% | 192/0/192 -> 64/0/64 | 20672 -> 16896 |
| `prep_frame_long_key` | 31,194.9 -> 21,491.0 | 68,412.0 -> 47,090.5 | 31.1% to 38.0% | 45.0% to 60.9% | 192/0/192 -> 64/0/64 | 150016 -> 16896 |
| `prep_frame_animated_seek` | 4,232.8 -> 4,668.8 | 9,259.3 -> 10,247.9 | -10.7% to 0.8% | -9.3% to 2.0% | 0/0/0 -> 0/0/0 | 0 -> 0 |
| `prep_frame_unique` | 16,854.7 -> 4,980.5 | 38,880.6 -> 12,280.0 | 66.9% to 71.1% | 224.7% to 278.5% | 192/0/192 -> 0/0/0 | 20672 -> 0 |

## Note animation preparation

Each operation creates and destroys 64 returned sources from a held static
slot. The input slot is borrowed and unchanged. Batches contain 256 operations;
throughput is prepared sources/s. The 8x4 sheet exercises sequential, color-X
and color-Y paths. Boundary64 and large65 exercise 64/65-index color-Y lanes.
Long-key uses a 1032-byte key and color-X. Rejected uses both color spacings.

| Workload | ns/op, old -> new (round 1) | Thread cycles/op, old -> new (round 1) | CPU reduction, six rounds | Throughput gain, six rounds | Alloc/realloc/free, old -> new | Requested bytes, old -> new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `prep_note_sequential` | 17,684.8 -> 9,891.8 | 38,776.0 -> 21,702.2 | 38.6% to 50.3% | 62.5% to 101.3% | 192/0/192 -> 64/0/64 | 20672 -> 16896 |
| `prep_note_color_x` | 25,055.5 -> 14,134.8 | 54,961.6 -> 30,994.9 | 42.2% to 48.2% | 73.1% to 93.2% | 320/0/320 -> 128/0/128 | 25792 -> 19968 |
| `prep_note_color_y` | 25,478.9 -> 14,162.5 | 55,858.5 -> 30,974.4 | 41.3% to 49.8% | 70.1% to 99.5% | 320/0/320 -> 128/0/128 | 29888 -> 22016 |
| `prep_note_boundary64` | 28,394.9 -> 18,530.5 | 62,201.7 -> 40,637.5 | 34.7% to 48.2% | 53.2% to 93.1% | 320/0/320 -> 128/0/128 | 87296 -> 50688 |
| `prep_note_large65` | 30,533.6 -> 22,105.5 | 66,920.9 -> 48,477.8 | 15.3% to 27.6% | 17.9% to 38.1% | 320/0/320 -> 192/0/192 | 88320 -> 84480 |
| `prep_note_long_key` | 35,793.0 -> 21,528.5 | 78,465.2 -> 47,225.1 | 39.8% to 48.5% | 66.3% to 94.2% | 320/0/320 -> 128/0/128 | 155136 -> 19968 |
| `prep_note_rejected` | 2,043.0 -> 2,034.8 | 4,464.6 -> 4,476.6 | -5.6% to 5.3% | -5.3% to 5.6% | 0/0/0 -> 0/0/0 | 0 -> 0 |

## Owned StateProperties delays

Shared-source operations clone, prepare and destroy 64 indexed slots. Input
command cloning is included equally in both implementations, with 256
operations per batch; throughput is prepared slots/s. Exact uses 32 delays
for 32 frames. Padded has two delays for 32 frames and exercises the unchanged
allocation fallback. Truncated has 64 delays for 32 frames. Large uses 129
delays for 129 frames on a 2x129 sheet. Unique uses an exact 32-delay command
and freshly prepared slots; setup/final destruction are excluded as above.

| Workload | ns/op, old -> new (round 1) | Thread cycles/op, old -> new (round 1) | CPU reduction, six rounds | Throughput gain, six rounds | Alloc/realloc/free, old -> new | Requested bytes, old -> new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `prep_state_exact` | 45,727.0 -> 39,903.5 | 100,271.2 -> 87,446.7 | 12.4% to 23.3% | 14.1% to 30.7% | 256/0/256 -> 192/0/192 | 42496 -> 34304 |
| `prep_state_padded` | 43,916.4 -> 45,572.3 | 95,393.3 -> 99,916.2 | -5.4% to 1.0% | -5.1% to 1.0% | 256/0/256 -> 256/0/256 | 34816 -> 34816 |
| `prep_state_truncated` | 44,410.5 -> 33,927.7 | 97,393.7 -> 74,413.1 | -5.4% to 23.6% | -5.1% to 30.9% | 256/0/256 -> 192/0/192 | 50688 -> 42496 |
| `prep_state_large` | 58,637.9 -> 46,262.1 | 128,539.5 -> 101,475.9 | 9.8% to 21.1% | 10.9% to 26.8% | 256/0/256 -> 192/0/192 | 117248 -> 84224 |
| `prep_state_unique` | 33,345.3 -> 26,635.9 | 74,400.2 -> 60,067.6 | 12.4% to 22.4% | 14.3% to 29.4% | 192/0/256 -> 128/0/192 | 25600 -> 17408 |

Allocation totals in every table cover 64 items. Divide by 64 for per-item
counts. Complete owning workloads balance requested and freed bytes. The two
unique mutation workloads exclude construction and final destruction, so
their requested and freed bytes may differ:

For `prep_frame_unique`, old requested/freed bytes are 20,672/20,672; new values are 0/0. For `prep_state_unique`, old requested/freed bytes are 25,600/21,504; new values are 17,408/13,312.

These scopes are identical for each old/new pair. No allocation-counted
operation performs a reallocation in either implementation. The owned delay
planner also has a zero-churn test that retains its input pointer/capacity
when supplied spare capacity, including padding from an empty input.

## Behavior and validation

Seven new ordinary tests compare complete slot/source state with the frozen
parent. Comparisons cover duration and rate bits, indices, UVs, timed/phase
frame selection, NaN/infinities, negative and subnormal delays, zero/oversized
frame counts, sheet rejection, lane origins, negative/zero sizes, the 64/65
boundary, model/animated bypass, command sequences and seek/pause/freezing.
Ownership tests check unique/shared/weak sources, retained old values and
cache reset. Allocation assertions cover zero-churn mutations and planner
reuse, rejected note animations, and reduced complete owning churn.

Validation commands:

```powershell
cargo test -p deadsync-assets --lib sprite_preparation --offline
cargo test -p deadsync-assets -p deadsync-noteskin --lib --release --locked --offline
cargo check --workspace --locked --offline
cargo clippy -p deadsync-assets -p deadsync-noteskin --lib --locked --offline -- -D clippy::perf -A clippy::large_enum_variant
rustfmt --edition 2024 --check crates/deadsync-assets/src/noteskin/texture.rs crates/deadsync-noteskin/src/sprite.rs crates/deadsync-noteskin/src/lib.rs
git diff --check
```

The release suites pass 180 asset tests and 321 noteskin tests; 24 manual
performance tests remain ignored. Workspace checking, the performance lint
check, formatting and frozen-parent auditing pass. Cargo.toml advances once
from 0.5.1658 to 0.5.1659; Cargo.lock updates the three workspace-version packages.
The excluded files remain outside this commit.

An additional `cargo check --workspace --all-targets --locked --offline` failed
on the existing `deadsync-noteskin/tests/safe_parsing.rs` integration target:
its included `src/explosion.rs` calls the crate-private `script_random` through
a re-exported module (E0603). The test, explosion implementation and script
implementation are unchanged from the parent. The standard workspace check
and both complete library release suites pass; the all-targets integration
check remains limited by that pre-existing error.
