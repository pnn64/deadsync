# Lua frame sampling - 0.5.1625

Parent: `2028a9df0f` (0.5.1624). Date: 2026-09-30.

The 60 Hz song compiler previously copied modifier names into separate
`BTreeMap<String, f32>` snapshots on every frame, used the default hasher for
actor-track indices, and grew six frame buffers from a single element.

This pass applies the local performance guide's M-HOTPATH, M-MEM-REUSE,
M-BOX-DST, M-FAST-HASHER, M-INITIAL-CAPACITY, and M-THROUGHPUT guidelines:

1. Modifier and approach-speed snapshots use sorted boxed slices with names
   shared through compilation-local `Rc<str>` storage. Bulk Lua table traversal
   and a cache of validated Lua names avoid recurring string copies and Lua
   reference-counter allocations. The cache retains each Lua string so garbage
   collection cannot reuse its pointer for a different key. Snapshot values
   remain independent; later writes and removals affect subsequent samples.
2. Actor/target track indices use `FxHashMap`. These keys are internal actor
   indices and enum discriminants. The capture and scheduled-tween algorithms
   are unchanged; their helpers accept either hasher for direct comparison.
3. All six sample buffers reserve the exact reference replay length, including
   the initial sample. This removes growth reallocations and copying. The frame
   buffer benchmark isolates these outer containers using the final element
   types; the complete compiler benchmark also includes the snapshot changes.

The public modifier-map APIs retain their original types and behavior. There
are no new dependencies or unsafe code. Snapshot output still requires owned
storage; zero allocation applies to warm track lookup and filling already
reserved frame buffers, rather than to complete chart compilation.

## Measurements

All 88 measurement rows, including timing ranges and both execution orders,
are in [the CSV](lua-frame-sampling-0.5.1625.csv). The following table uses
run 1 for absolute values and all four runs for CPU reductions.

| Workload | CPU cycles/op old -> new | CPU reduction across runs | Allocation/reallocation calls old -> new | Requested bytes old -> new |
| --- | ---: | ---: | ---: | ---: |
| 64 frames, 32 modifiers per player | 8,164,035 -> 2,972,003 | 57.8-64.3% | 17,920/0 -> 256/0 | 614,912 -> 199,680 |
| 128 actors, 512 warm unchanged target writes | 45,894 -> 13,302 | 69.2-71.5% | 0/0 -> 0/0 | 0 -> 0 |
| Six buffers, 4,097 samples | 1,780,073 -> 569,832 | 64.9-73.6% | 6/72 -> 6/0 | 4,455,632 -> 1,114,384 |
| Complete 32-actor compiler | 58,337,201 -> 49,912,708 | 14.4-16.3% | 40,927/250 -> 7,992/222 | 1,997,641 -> 1,327,517 |

The complete compiler takes 26.65 -> 22.80 ms in run 1 and 27.54 ->
23.40 ms with reversed order in run 2. Runs 3 and 4 give 26.27 -> 21.99 ms
and 25.93 -> 22.09 ms. Throughput increases 16.9-19.4% across the four runs.
Allocation calls fall 80.5-80.7%, requested bytes fall 33.5-38.1%, and
reallocations fall 250 -> 222. Lua GC makes the separately counted old
compiler operation vary between runs; every count is retained in the CSV.

Warm dense snapshot sampling reduces allocations and frees from 280 to four
per frame (98.6%), with no reallocations. Those four allocations hold the two
option and two approach-speed snapshots. Track lookup has zero allocator
churn in both variants. At 4,097 samples, exact reservation removes 72 buffer
reallocations and reduces requested bytes by 75.0%.

The 601-sample storage control is order-sensitive: reservation uses 61.9-71.0%
fewer CPU cycles in the old-first runs, but 19.8-45.9% more cycles in the
new-first runs. Requested bytes still fall 70.6%, and reallocations fall
54 -> 0 in every run. This repeatable sensitivity is consistent with allocator
state influencing this isolated allocate/fill/drop workload; it is not a
universal CPU improvement at every size. The 61-, 4,097-, and 8,193-sample
controls, all snapshot and lookup sizes, and the complete compiler improve
in both execution orders. Larger frame buffers retain the capacity benefit
and the complete compiler retains its measured CPU improvement.

## Method and reproduction

Windows x86-64, Intel Xeon E5-2696 v4 @ 2.20 GHz, Rust 1.98.1. Both variants run
inside the same release test executable with the repository's release settings.
The existing scoped allocator records calling-thread allocation, reallocation,
free, and requested/freed byte counts. `QueryThreadCycleTime` records CPU cycles.
Timing and allocation accounting are separate. Counts include destruction of
Rust operation results; full-compiler fixture creation/destruction is outside
the measured operation. They describe allocator churn, not process RSS.

Each row reports the median of seven timing samples, plus a separately counted
operation after warmup. Four runs alternate old-first and new-first order. Modifier rows process
64 frames (two option snapshots and two speed snapshots per frame); track rows
process four targets per actor. Storage rows append every sample to six buffers.
The complete compiler fixture uses 32 actors, two players with 32 modifiers,
changing speed modes, a persistent tween message, BPM changes, a 1.25 music
rate, and a two-second chart. These are measured workloads, not whole-game
frame-rate claims.

```powershell
cargo test -p deadsync-song-lua --lib frame_sampling_perf -- --test-threads=1
cargo test -p deadsync-song-lua --release --lib frame_sampling_perf -- --test-threads=1
cargo test -p deadsync-song-lua --release --lib frame_sampling_hot_path_bench -- --ignored --nocapture --test-threads=1
$env:DEADSYNC_PERF_REVERSE = '1'
cargo test -p deadsync-song-lua --release --lib frame_sampling_hot_path_bench -- --ignored --nocapture --test-threads=1
Remove-Item Env:DEADSYNC_PERF_REVERSE
```

`tests/perf/frame_sampling_baseline.rs` freezes the complete parent
`compile_update_functions` body; only visibility and formatting differ.
The original public option snapshot reader and test-only original speed reader
provide the isolated snapshot baseline. The hasher benchmark calls the same
capture algorithm with `RandomState` and `FxBuildHasher`. Buffer comparisons
use the original grow-from-one strategy and exact reservation.

## Behavior validation

Seven new regression tests cover numeric/string key collisions, Unicode,
booleans and numeric strings, ignored values, NaN and signed zero, exact
conversion errors, raw lookup and metamethod behavior, all speed modes and
clears, reverse overrides, live writes and removals, shared name identity,
independent snapshot values, initial actor writes, changed tracks, complete
compiler output, and sample order at capacity boundaries up to 8,193 samples.
Allocation checks bound warm snapshot work, require zero warm lookup churn,
and rule out frame-buffer growth.

- Final debug library suite: 628 passed, 4 failed, 55 ignored.
- Untouched parent debug library suite in a detached worktree: 621 passed,
  the same 4 failed, 54 ignored.
- Final release library suite: 628 passed, the same 4 failed, 55 ignored.
- All seven new tests pass in debug and release; all four benchmark runs pass.
- Clippy completed with existing warnings; no warnings point at the new
  snapshot code or regression-test file.
- Formatting and `git diff --check` passed.

The four failures reproduce on the parent with identical assertions:
`compile_song_lua_extracts_actorproxy_targets`,
`compile_song_lua_resolves_local_and_hidden_screen_proxy_targets`,
`compile_song_lua_runs_cmd_queuecommand_builders`, and
`compile_song_lua_supports_notefield_column_api`. The first three expect
different initial proxy visibility; the last expects `-96:-125` while both
versions produce `-96:-135`.

The workspace patch version advances exactly once from 0.5.1624 to 0.5.1625;
Cargo.lock updates all three packages inheriting it.
