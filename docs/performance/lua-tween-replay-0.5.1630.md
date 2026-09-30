# Lua tween replay and score formatting - 0.5.1630

Parent: `dc21ff490` (0.5.1629). Date: 2026-09-30.

This pass follows the local performance guide's M-HOTPATH, M-MEM-REUSE and
M-THROUGHPUT recommendations with three changes:

1. `FormatPercentScore` writes directly into a 64-byte stack buffer before
   creating the Lua string. It preserves the original f32 conversion, rounding,
   non-finite spellings, default values and ignored extra arguments. Formatting
   no longer creates a temporary Rust String. The buffer covers every f32
   magnitude at the original fixed precision, including the percent suffix.
2. Captured final values use a u128 target-membership mask when both the final
   target count and scheduled-write count reach 16. This replaces repeated
   scans of scheduled writes with one scan plus constant-time membership checks.
   Restoration lookup also happens once in that wide path. Smaller batches keep
   their original loop. All 77 target discriminants fit in the mask; a compile
   assertion protects its bound. Scheduled writes retain precedence, and final
   targets retain their original write order.
3. Scheduled replay reuses its last spring/elastic factor for equal timing and
   curve keys. The cache borrows the last immutable sample and stores a curve
   identifier and factor; key comparisons use timing bits and, for elastic,
   period bits. Signed zero and NaN bits remain distinct. Spring does
   not consume the optional period. The cache is local to a single replay call;
   each call's current time still drives the calculation. A one-time check of
   the first two properties enables caching only when they share an expensive
   tween key. Other batches use the original replay loop. A batch beginning
   with unrelated properties can miss a later reuse opportunity; this
   conservative choice limits cache overhead. State updates, Lua writes,
   errors and partial writes retain their original order.

These changes add no dependency, public API change, unsafe code or persistent
heap cache. The score buffer and target mask live on the stack. Warm scalar
replay and target reconciliation have zero allocator churn in both versions;
the grouped/wide paths reduce repeated CPU work. The score callback removes temporary Rust
ownership while preserving allocations required by Lua and its calling API.

## Measurements

The [raw CSV](lua-tween-replay-0.5.1630.csv) contains 496 rows: 62 workloads,
two implementations and four independent runs. The table's percentage ranges
cover all four paired runs. Absolute figures use run 1; allocation, reallocation,
free and byte counts agree across runs for each workload/implementation.

| Workload | Thread cycles/batch, old -> new | CPU reduction | Throughput gain | Allocations/batch, old -> new | Requested bytes/batch, old -> new |
| --- | ---: | ---: | ---: | ---: | ---: |
| 64 normal percentage-score calls | 89,977.9 -> 76,047.5 | 15.5% to 22.9% | 18.2% to 29.6% | 128 -> 64 | 3,072 -> 2,560 |
| 64 very large percentage-score calls | 230,996.2 -> 204,947.6 | 10.6% to 24.1% | 11.8% to 31.7% | 192 -> 128 | 14,400 -> 6,912 |
| 16 final targets / 16 queued writes | 211.9 -> 103.5 | 47.2% to 51.2% | 90.2% to 105.0% | 0 -> 0 | 0 -> 0 |
| 32 final targets / 32 queued writes | 823.1 -> 212.1 | 74.2% to 77.0% | 289.0% to 337.8% | 0 -> 0 | 0 -> 0 |
| 77 final targets / 1,232 queued writes | 69,641.2 -> 5,201.9 | 92.5% to 93.3% | 1239.6% to 1383.4% | 0 -> 0 | 0 -> 0 |
| Complete capture: 4 actors x 32 properties, 16 queued writes/property | 313,061.8 -> 297,467.1 | 2.6% to 12.8% | 2.8% to 14.9% | 143 -> 143 | 518,532 -> 518,532 |
| 256 shared spring properties, states | 13,832.0 -> 6,245.4 | 54.8% to 56.8% | 121.5% to 131.5% | 0 -> 0 | 0 -> 0 |
| 256 shared elastic properties, states | 16,812.0 -> 5,790.0 | 57.5% to 65.7% | 135.1% to 191.4% | 0 -> 0 | 0 -> 0 |
| 256 shared spring properties, Lua getters | 68,451.9 -> 62,190.9 | 9.1% to 12.6% | 10.0% to 14.4% | 0 -> 0 | 0 -> 0 |
| 256 shared elastic properties, Lua getters | 71,658.2 -> 61,554.2 | 11.7% to 15.4% | 13.2% to 18.1% | 0 -> 0 | 0 -> 0 |

Normal public score calls go from 128 allocations/frees to 64 per 64-call
batch: one 8-byte temporary formatting allocation disappears per call. Requested
and freed bytes go from 3,072 to 2,560 per batch. mlua's 40-byte argument-frame
allocation remains. A separate warmed direct-callback test without an argument
frame verifies zero churn in the new formatting body.

Large score outputs go from 192 allocations plus 64 reallocations to 128
allocations and zero reallocations per batch; requested bytes fall from 14,400
to 6,912 and freed bytes from 10,048 to 2,560. Required long Lua-string outputs
remain. All replay and isolated reconciliation workloads have zero allocation,
reallocation, free and byte churn in both implementations.

Complete capture retains exactly the parent's output-storage costs in every
paired run: for example, four actors x 32 properties with 16 queued writes each
use 143 allocations, 18 reallocations, six frees, 518,532 requested bytes and
256,756 freed bytes per batch in both versions. Fixture destruction is outside
the measured window, so retained output is not counted as a free there.

## Controls and limits

Negative percentages mean increased cost or lower throughput. The controls
below disclose the entry-check and call-layout costs as well as variation in
the fresh-fixture complete-capture timings. The 32-property/16-write complete
capture fixture improves in every run; the other complete-capture cases have
mixed timings, so no general complete-capture speedup is claimed.

| Control | Thread cycles/batch, old -> new | CPU reduction | Throughput gain |
| --- | ---: | ---: | ---: |
| Empty replay, states | 18.8 -> 28.5 | -54.8% to -24.9% | -35.7% to -20.2% |
| One elastic property, states | 89.4 -> 95.0 | -69.4% to 0.6% | -40.9% to 0.6% |
| 256 linear properties, states | 6,899.9 -> 6,660.8 | 2.3% to 3.7% | 2.5% to 3.9% |
| 256 linear properties, Lua getters | 62,565.0 -> 60,655.7 | -1.1% to 3.1% | -1.1% to 3.1% |
| 256 distinct elastic timings, states | 16,666.0 -> 16,340.5 | -14.9% to 2.0% | -12.8% to 2.0% |
| 256 distinct elastic timings, Lua getters | 70,413.8 -> 74,280.0 | -5.5% to -0.3% | -5.2% to -0.3% |
| One final target / one queued write | 7.7 -> 8.4 | -95.6% to -9.1% | -50.5% to -8.0% |
| 4 final targets / 4 queued writes | 25.0 -> 38.2 | -52.8% to -22.0% | -34.9% to -18.3% |
| 5 final targets / 5 queued writes | 41.3 -> 50.9 | -71.8% to -23.2% | -38.3% to -18.9% |
| 15 final targets / 15 queued writes | 237.8 -> 227.8 | -6.2% to 5.4% | -5.7% to 6.4% |
| Complete capture: 4 actors x 32 properties, 1 queued write/property | 107,844.8 -> 80,165.5 | -41.2% to 25.7% | -29.8% to 35.7% |
| Complete capture: 4 actors x 32 properties, 16 queued writes/property | 313,061.8 -> 297,467.1 | 2.6% to 12.8% | 2.8% to 14.9% |

Small and empty operations can show large percentage changes for only a few
cycles. The raw CSV includes every empty, small, boundary and distinct-timing
workload. The replay entry guard deliberately misses later reuse opportunities
in batches whose first pair is unrelated. Only batches with an initially shared
expensive key enter the cached loop; mixed batches can still pay cache-miss
costs after that pair.

These are measurements of the named operations, not whole-game frame rates or
loading times. Allocator requested/freed bytes measure churn, not peak RSS or
retained-memory reductions. Required output allocations in complete capture
batches remain. Lua GC is stopped during the benchmark fixtures to separate
collection pauses from the operations; fresh fixture construction and final
destruction are outside the timed complete-capture window.

## Method and behavior checks

The frozen baseline functions come from `dc21ff490`. The percentage callback
body and captured-final-values loop are extracted verbatim, apart from formatting
and function wrappers. The complete capture and replay functions are frozen
copies of that parent. Their unchanged dependencies are shared. The audit checks
the frozen copies against the parent, and Cargo manifest/lock/metadata changes
against the exact patch bump.

The existing allocator harness counts allocation, reallocation, free, requested
bytes and freed bytes separately from timing. Seven samples per workload produce
median wall time and Windows `QueryThreadCycleTime` calling-thread CPU cycles;
throughput is units per second. Runs alternate old-first and new-first. Timing
includes the harness's allocator checks equally for both versions. Complete
capture uses fresh fixtures and per-operation clocks, whose overhead is included
equally. Other measurements reuse warmed fixtures with loop timing. All benchmark
runs are serial, with test/build jobs finished before timing.

Score batches contain 64 public callback calls. Replay batches contain 0, 1, 4,
32 or 256 scheduled properties, over 16 states, with optional scalar Lua getter
writes. Reconciliation tests 1 through 77 final targets and up to 1,232 scheduled
writes. Complete capture uses four actors, up to 32 final properties per actor
and up to 16 queued writes per property. Those fixtures include immediate writes
before and after queued writes and a restored actor. Empty replay throughput
uses one nominal unit and is only a control.

Eight new behavior tests pass in debug and release. They cover:

- Score defaults, coercions, invalid UTF-8, extra arguments, non-finite/extreme
  values, 16,384 random f32 bit patterns and the installed global callback.
- Exact replay results for all supported curves, shared/mixed keys, timing
  boundaries, non-finite values, 1,024 random timing keys, and individual curve
  groups before later writes overwrite them.
- Lua getter order, injected errors, partial state updates and missing actors;
  zero warm churn for scalar replay with and without Lua getters.
- All target discriminants, duplicate scheduled writes, 15/16/17 cutoff cases,
  missing states, restored actors, complete capture output and tween precedence;
  zero churn in target reconciliation.

Both full library suites report **668 passed, 4 failed, 64 ignored**. The four
failures match the parent's assertions and locations:

- `compile_song_lua_extracts_actorproxy_targets`
- `compile_song_lua_resolves_local_and_hidden_screen_proxy_targets`
- `compile_song_lua_runs_cmd_queuecommand_builders`
- `compile_song_lua_supports_notefield_column_api`

Clippy passes with existing repository warnings. Formatting and `git diff --check`
pass. Commands:

```text
cargo test --locked -p deadsync-song-lua --lib
cargo test --locked -p deadsync-song-lua --release --lib
cargo clippy --locked -p deadsync-song-lua --all-targets
cargo metadata --locked --no-deps --format-version 1
cargo test --locked -p deadsync-song-lua --release --lib tween_replay_bench -- --ignored --nocapture --test-threads=1
cargo test --locked -p deadsync-song-lua --release --lib percent_score_bench -- --ignored --nocapture --test-threads=1
```

The last two commands run four times; `DEADSYNC_PERF_REVERSE=1` reverses order
for runs 2 and 4. Preliminary measurements prompted the 16-target cutoff and
compact curve identifier and first-pair entry guard; the CSV contains only
the final implementation's runs.

Workspace version: **0.5.1629 -> 0.5.1630**, exactly one patch increment.
Cargo.toml and Cargo.lock are included in the pass. The four excluded files
`deadsync-song.json.gz`, `rust-performance.md`, `optimize.sh` and `optimize.ps1`
are outside the commit.
