# Noteskin visual assembly - 0.5.1206

Baseline: `66b76b17c7f0d6b5d2fefed0d562f2d62f9d2169` / 0.5.1205.
This pass follows `rust-performance.md` M-HOTPATH, M-MEM-REUSE and
M-THROUGHPUT: remove disposable copies from noteskin construction and retain
only the allocations needed by the final owned visuals. These are loading and
preview preparation costs, not per-frame rendering or whole-game load times.

## Three changes

1. **Move hold/roll parts into the result.** The consuming assembly functions
   no longer clone inactive slots that already have a separate active slot.
   Head layer Arcs also move when already owned. Cloning remains where one
   inactive slot must supply two outputs, or where rolls borrow hold fallbacks.
   These functions run for each column in `itg_runtime_columns_selected`.
   SpriteSlot creates a fresh atomic identity when cloned; moved slots now
   retain their original identities. Fallback copies remain distinct and shared
   head layers retain the same Arc ownership relationships.
2. **Borrow tap explosion source metadata.** The resolved-layer adapter keeps
   element strings, command maps and payloads borrowed while matching judgment
   windows. Only final animation layers clone their slots. Up to four sources
   per mode use inline scratch; larger skins still spill safely. Direct fallback
   Vecs stay alive through construction. The public owned-source API uses the
   same selection algorithm through a private, statically dispatched adapter,
   without an intermediate conversion Vec. Explicit source mode overrides,
   layer ordering, preferred/fallback selection, command order and callback
   order are preserved.
3. **Reuse Down-column explosion slices.** Tap, hold and roll column assembly
   now read the already resolved Down explosion list rather than cloning its
   sprites, command maps and strings before reading them. Other buttons still
   resolve in the same order. Pump uses its existing button behavior; this pass
   does not reinterpret its Center/Down rules or change callback counts.

## Method

Windows x86_64 MSVC, Intel Xeon E5-2696 v4 @ 2.20 GHz; Rust 1.98.0
(`88d9e12ae`). Release opt-level 3 and full LTO. Five complete runs alternate
old/new and new/old order. Each row is the median of five run medians, each
itself measured over seven batches after three warmups. Allocation counting
runs separately from timing, using the existing thread-local counted allocator.
Outputs are black-boxed and destroyed inside each measured operation.

Frozen baseline function bodies are committed next to the tests and audited
against the parent ignoring whitespace and visibility/import changes. The
animation parser is shared: its algorithm is unchanged; only its input changes
from the source object to its borrowed command map. The column-only baseline
shares the new tap-map builder to isolate the third change; the combined runtime
baseline includes the old tap-map builder and old column assembly.

Inputs, loader metadata and column templates are prepared before timing.
Hold/roll benchmarks include identical input clones to provide owned parts and
use a SpriteSlot surrogate with a fresh atomic identity, four Arc resources and
inline state. These slot clones do not allocate; their cost includes atomic
identity and Arc updates. The Vec payload regression test separately proves the generic API
moves heap-owning slots without allocation. Tap and runtime measurements use
u32 payloads plus real owned sprite metadata and actual production animation
parsing. Direct tap cases include identical resolver Vec/metadata clones. Both
missing metric commands (no final animations) and present metric commands
(renderable animations) are measured. Owned source cases include identical
source clones and partitioning on both sides.
Runtime cases include column input clones and resolver output construction, but
exclude filesystem access, Lua compilation, image decoding and graphics upload.

One throughput unit is one hold/roll assembly, tap-map construction or post-column
runtime assembly. Iterations per timing batch: 4,096 hold/roll; 128 tap maps;
32 runtimes. Cycles are calling-thread `QueryThreadCycleTime`, not retired
instructions or whole-process CPU. Requested/freed bytes are allocator traffic,
not peak live memory, RSS, cache misses or GPU memory. Full rendered skins and
frame rate were not benchmarked. All DeadSync builds finished before timing.
Unrelated project builds were observed on the shared host during preparation
and could still contribute
noise; those processes were left alone. Small differences in unchanged controls
may reflect scheduling or code layout on this host.

## Results

- Complete hold/roll assembly uses **25.1-25.5% fewer thread cycles**.
  The SpriteSlot surrogate has zero allocations before and after; savings come
  from avoiding discarded clones and atomic resource-reference churn. Necessary
  inactive-to-active fallbacks retain their clones.
- Nonempty actor tap maps use **27.7-33.8% fewer cycles**.
  At eight layers, allocation/free calls fall **126 -> 14**, reallocations
  **1 -> 0**, and requested/freed bytes **11,935 -> 6,128** per map. Final
  animation storage remains owned. Rejected actor commands fit inline and can
  build an empty result with zero allocator churn.
- Down-column slice reuse, isolated from the tap-source change, uses
  **10.7-12.0% fewer cycles** for four/eight dance columns,
  saving **339/678 allocations and frees** per runtime assembly.
  Both explosion changes together use **25.3-26.5% fewer
  cycles** for these dance fixtures. The combined runtime fixture starts after
  column preparation, so it does not include the first optimization's savings.

All measured cases are shown below. `tap_direct` has no metric commands and
produces no animation; `tap_metric_direct` supplies metric commands and constructs
animations. `tap_owned` retains the public owned-source API as a compatibility
control. `runtime_columns` isolates slice reuse; `runtime_combined` includes both
explosion changes. Pump column-only cases have identical allocation counts and
unchanged work, so their timing differences are not attributed to an optimization.
The owned-source controls likewise have unchanged allocation counts and the same
selection/parsing algorithm. The one/eight-source controls measured 2.6%/2.0%
more cycles, with unchanged allocation budgets; the 32-source control was flat.
These small measured slowdowns are retained in the results. A universal speedup
is not claimed. Direct fallbacks with metric animations use 8.2-13.1% fewer
cycles, with fewer allocations at every tested size.

Cycle change is `(new / old - 1) * 100`; negative is better. Times are microseconds
per operation, and throughput is complete operations per second.

| Workload | Old us | New us | Old thread cycles | New thread cycles | Cycle change | Old -> new ops/s |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `hold_complete_slots` | 0.957 | 0.717 | 2,093.1 | 1,559.1 | -25.5% | 1,045,485 -> 1,393,908 |
| `roll_complete_slots` | 1.055 | 0.784 | 2,297.2 | 1,719.9 | -25.1% | 948,104 -> 1,274,861 |
| `hold_fallback_slots` | 0.708 | 0.684 | 1,529.4 | 1,501.2 | -1.8% | 1,412,609 -> 1,461,761 |
| `roll_fallback_slots` | 0.774 | 0.755 | 1,697.3 | 1,657.3 | -2.4% | 1,292,154 -> 1,323,938 |
| `tap_actor_0` | 0.056 | 0.046 | 132.0 | 111.5 | -15.5% | 18,028,169 -> 21,694,915 |
| `tap_direct_0` | 0.057 | 0.048 | 133.8 | 113.2 | -15.4% | 17,534,247 -> 20,983,607 |
| `tap_actor_1` | 4.314 | 2.856 | 9,479.7 | 6,278.0 | -33.8% | 231,800 -> 350,205 |
| `tap_direct_1` | 4.149 | 2.295 | 8,802.3 | 5,045.1 | -42.7% | 241,009 -> 435,819 |
| `tap_actor_4` | 13.163 | 9.553 | 28,907.1 | 20,905.7 | -27.7% | 75,969 -> 104,678 |
| `tap_direct_4` | 17.709 | 11.273 | 38,810.3 | 24,729.8 | -36.3% | 56,467 -> 88,710 |
| `tap_actor_8` | 26.702 | 18.862 | 58,570.5 | 41,372.3 | -29.4% | 37,450 -> 53,017 |
| `tap_direct_8` | 35.280 | 22.680 | 76,500.9 | 49,723.6 | -35.0% | 28,345 -> 44,092 |
| `tap_actor_32` | 111.423 | 79.184 | 243,663.9 | 173,091.2 | -29.0% | 8,975 -> 12,629 |
| `tap_direct_32` | 159.104 | 102.025 | 348,020.7 | 222,950.3 | -35.9% | 6,285 -> 9,802 |
| `tap_metric_direct_1` | 30.341 | 28.119 | 66,551.4 | 61,089.6 | -8.2% | 32,959 -> 35,564 |
| `tap_metric_direct_4` | 43.487 | 39.048 | 94,877.2 | 85,659.9 | -9.7% | 22,996 -> 25,610 |
| `tap_metric_direct_8` | 116.864 | 101.817 | 254,892.7 | 223,205.8 | -12.4% | 8,557 -> 9,822 |
| `tap_metric_direct_32` | 463.893 | 403.387 | 1,014,623.3 | 882,045.3 | -13.1% | 2,156 -> 2,479 |
| `tap_owned_1` | 3.373 | 3.480 | 7,413.3 | 7,603.6 | +2.6% | 296,502 -> 287,382 |
| `tap_owned_8` | 23.551 | 24.298 | 51,661.4 | 52,673.1 | +2.0% | 42,461 -> 41,155 |
| `tap_owned_32` | 98.094 | 98.030 | 214,976.2 | 214,842.5 | -0.1% | 10,194 -> 10,201 |
| `runtime_columns_4` | 178.975 | 159.812 | 391,183.3 | 349,450.9 | -10.7% | 5,587 -> 6,257 |
| `runtime_combined_4` | 210.781 | 154.922 | 462,171.0 | 339,648.8 | -26.5% | 4,744 -> 6,455 |
| `runtime_columns_5` | 222.741 | 218.297 | 487,749.6 | 478,750.1 | -1.8% | 4,490 -> 4,581 |
| `runtime_combined_5` | 267.059 | 221.562 | 583,424.1 | 485,815.2 | -16.7% | 3,744 -> 4,513 |
| `runtime_columns_8` | 348.697 | 308.006 | 763,290.7 | 672,054.2 | -12.0% | 2,868 -> 3,247 |
| `runtime_combined_8` | 411.300 | 307.791 | 899,229.7 | 671,292.7 | -25.3% | 2,431 -> 3,249 |
| `runtime_columns_10` | 436.528 | 432.256 | 955,085.7 | 945,256.2 | -1.0% | 2,291 -> 2,313 |
| `runtime_combined_10` | 515.109 | 429.447 | 1,125,987.0 | 939,528.5 | -16.6% | 1,941 -> 2,329 |

All measured outputs are dropped, so allocation and free call counts match and
requested/freed bytes match. Reallocations are reported separately; their old
and new buffer sizes are included in freed/requested byte traffic. The CSV also
contains each run's median and minimum/maximum timing batches.

| Workload | Allocations/frees old -> new | Reallocations old -> new | Requested/freed bytes old -> new |
| --- | ---: | ---: | ---: |
| `hold_complete_slots` | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| `roll_complete_slots` | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| `hold_fallback_slots` | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| `roll_fallback_slots` | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| `tap_actor_0` | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| `tap_direct_0` | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| `tap_actor_1` | 19 -> 4 | 0 -> 0 | 3,003 -> 2,040 |
| `tap_direct_1` | 51 -> 26 | 0 -> 0 | 2,880 -> 1,360 |
| `tap_actor_4` | 64 -> 7 | 0 -> 0 | 6,111 -> 3,216 |
| `tap_direct_4` | 195 -> 99 | 1 -> 0 | 11,212 -> 5,766 |
| `tap_actor_8` | 126 -> 14 | 1 -> 0 | 11,935 -> 6,128 |
| `tap_direct_8` | 394 -> 202 | 2 -> 1 | 24,156 -> 13,230 |
| `tap_actor_32` | 492 -> 44 | 13 -> 12 | 50,283 -> 27,056 |
| `tap_direct_32` | 1,546 -> 778 | 18 -> 17 | 101,612 -> 57,910 |
| `tap_metric_direct_1` | 94 -> 69 | 0 -> 0 | 11,020 -> 9,500 |
| `tap_metric_direct_4` | 238 -> 142 | 1 -> 0 | 19,352 -> 13,906 |
| `tap_metric_direct_8` | 500 -> 308 | 2 -> 1 | 50,048 -> 39,122 |
| `tap_metric_direct_32` | 1,876 -> 1,108 | 32 -> 31 | 210,160 -> 166,458 |
| `tap_owned_1` | 20 -> 20 | 0 -> 0 | 3,083 -> 3,083 |
| `tap_owned_8` | 127 -> 127 | 1 -> 1 | 12,575 -> 12,575 |
| `tap_owned_32` | 493 -> 493 | 13 -> 13 | 52,843 -> 52,843 |
| `runtime_columns_4` | 1,533 -> 1,194 | 0 -> 0 | 101,411 -> 83,990 |
| `runtime_combined_4` | 1,981 -> 1,194 | 4 -> 0 | 124,639 -> 83,990 |
| `runtime_columns_5` | 1,886 -> 1,886 | 0 -> 0 | 125,260 -> 125,260 |
| `runtime_combined_5` | 2,446 -> 1,886 | 5 -> 0 | 154,295 -> 125,260 |
| `runtime_columns_8` | 2,945 -> 2,267 | 0 -> 0 | 196,807 -> 161,965 |
| `runtime_combined_8` | 3,841 -> 2,267 | 8 -> 0 | 243,263 -> 161,965 |
| `runtime_columns_10` | 3,651 -> 3,651 | 0 -> 0 | 244,505 -> 244,505 |
| `runtime_combined_10` | 4,771 -> 3,651 | 10 -> 0 | 302,575 -> 244,505 |

## Validation

- 248 noteskin unit tests passed (7 manual benchmarks ignored), including all
  1,024 hold part combinations and 9,216 roll/fallback combinations; exact Arc
  sharing; moved heap pointers and slot identities; ordered resolver/metric
  callbacks; blank commands; animation state samples; owned mode overrides;
  dance/Pump layouts; blank loader requests; and partial preview selection.
- 36 noteskin integration tests passed (2 manual benchmarks ignored).
- 127 asset and 370 note-field unit tests passed.
- The eight new behavior/allocation tests passed in release mode.
- `cargo check -p deadsync --locked`, scoped rustfmt and `git diff --check` passed.
- Performance Clippy passed for noteskin library/tests with
  `-A clippy::all -D clippy::perf -A clippy::cloned_ref_to_slice_refs`.
  The last allowance covers two existing `parent.0.clone()` slice constructions
  in `tests/packs.rs` (lines 242/245); that file is unchanged. The first full
  Clippy invocation identified those existing failures.

No final animation storage or necessary fallback clone was removed. The pass
adds no dependency and bumps the workspace patch version exactly once, from
0.5.1205 to 0.5.1206, including the three matching Cargo.lock package entries.

## Reproduction

```powershell
cargo test -p deadsync-noteskin --tests --locked -- --test-threads=1
cargo test -p deadsync-noteskin --release --lib --locked visual_assembly_perf -- --test-threads=1
cargo check -p deadsync --locked
cargo test -p deadsync-assets -p deadsync-notefield --lib --locked -- --test-threads=1
cargo clippy -p deadsync-noteskin --lib --tests --locked --no-deps -- -A clippy::all -D clippy::perf -A clippy::cloned_ref_to_slice_refs
# Finish all builds before timing. Run five times, alternating the environment variable.
cargo test -p deadsync-noteskin --release --lib --locked benchmark_visual_assembly -- --ignored --test-threads=1 --nocapture
$env:DEADSYNC_PERF_REVERSE = '1'
cargo test -p deadsync-noteskin --release --lib --locked benchmark_visual_assembly -- --ignored --test-threads=1 --nocapture
Remove-Item Env:DEADSYNC_PERF_REVERSE
```

The recorded measurements use the already built test executable directly to
avoid Cargo work between paired runs. Raw samples are in
[visual-assembly-0.5.1206.csv](visual-assembly-0.5.1206.csv).
