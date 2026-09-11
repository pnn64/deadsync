# Timing preparation performance - 0.5.1136

This pass follows `M-HOTPATH`, `M-MEM-REUSE`, `M-INITIAL-CAPACITY`, and
`M-THROUGHPUT` from the supplied `rust-performance.md`. It targets chart-load
preparation: constructing timing tables and building Pump hold events. These
benchmarks do not measure gameplay FPS or disk I/O.

## Three changes

1. **Keep timing construction's owned buffers.** Already ordered BPM input is
   borrowed; only disordered input needs a sorted copy. Other ordered timing
   tables skip sorting. Beat-to-time timestamps are recomputed into the original
   uniquely owned vector, and the row table is installed once. The first point's
   offset is deferred until all conversions finish because subsequent queries
   still depend on that original offset. Ordering, floating-point operations,
   timestamps, and clone/offset-mutation behavior remain unchanged.
2. **Resume beat-time conversion between Pump checkpoints.** Each hold uses the
   existing `BeatTimeCache` instead of walking all earlier BPM/stop/delay/warp
   events again for every checkpoint. Regression tests exposed differences in
   that existing cache for some fractional/duplicate BPM positions. The new
   `supports_row_time_cache` predicate therefore restricts this optimization to
   unique, row-aligned BPM positions with finite positive BPMs; other tables keep
   independent conversion. Tickcount order and rewinds remain supported. The
   existing cached API's behavior is unchanged for its other callers.
3. **Size the Pump event buffer before emission.** Shared checkpoint-range logic
   counts the exact head/checkpoint/tail output before allocating, including both
   players and cache/range truncation. One event-buffer allocation replaces the
   old initial reservation for only heads/tails and subsequent growth. Tap-row
   scratch remains one allocation per player. This adds a cheap count pass but
   removes growth copies and excess retained vector capacity. Checkpoint ranges
   use arithmetic counting, without allocating or converting beats to times.

No dependencies, global caches, or unsafe code were added. Returned timing/event
objects still need storage; this pass removes temporary allocations and growth,
while checkpoint emission into a prepared output vector remains allocation-free.

## Method

Windows x86-64, Intel Xeon E5-2696 v4, rustc 1.98.0, repository release profile
(opt-level 3, full LTO). The frozen timing constructor, sorted-table helper,
Pump tap-row collector, checkpoint emitter, and builder are from `c4aeed4fe` /
0.5.1135. Their bodies were checked against that commit, ignoring formatting,
reference names, and the documented checkpoint-writer substitution in the old
builder. Old and new run in the same executable with the same fixtures.

The Pump benchmark has three modes: **old**, **cached** (the old builder/reservation
with the new checkpoint emitter), and **new** (cached emitter plus exact capacity).
This isolates the checkpoint change from buffer sizing. Timing fixtures are
prepared before Pump timing; both versions use identical timing tables. Separate
constructor tests verify those tables against the old constructor.

Each workload has three warmups and seven timed batches. Three invocations run
old/cached/new, new/cached/old, then old/cached/new; construction uses old/new,
new/old, old/new. Results are medians of the three invocation medians. Builds and
other tests finish before benchmarking. Counters are off during timing and on for
one additional operation, including result destruction. Windows calling-thread
cycles use `QueryThreadCycleTime`. All measured work stays on that thread.

Allocation/free call counts and requested/freed byte counts match in every
fixture. Reallocations are counted separately; byte traffic includes each full
reallocation request. These are allocator traffic measurements, not peak RSS or
allocator metadata. Fixture creation is excluded; output allocation, filling,
sorting, score-row counting, and destruction are included in Pump measurements.

Constructor fixtures all copy 8,192 input row beats. They cover default BPM,
one BPM, 1,024 BPM changes, and 256 BPM changes accompanied by stop, delay, warp,
speed, scroll, and fake tables, either ordered or reversed. Batches contain
1,000 default/single operations or 100 larger operations. Throughput counts
complete timing objects.

Pump fixtures contain alternating holds/rolls with a tap halfway into each head
beat. The small case has four one-beat holds (24 output events). Larger cases
have 128 sixteen-beat holds per player (8,448 events/player). The plain case has
one BPM; the dense and two-player cases have 128 BPM entries and 32 entries each
of stops, delays, and warps. Tickcount is four per beat. Batches contain
1,000 / 64 / 16 / 8 operations for small / plain / dense / two-player respectively.
Pump throughput counts output events.

## Results

### Timing construction

| Workload | Old us/op | New us/op | Old cycles/op | New cycles/op | Old -> new objects/s |
|---|---:|---:|---:|---:|---:|
| Default BPM | 2.751 | 2.458 | 6,030.1 | 5,392.0 | 363,491.0 -> 406,917.6 |
| One BPM | 2.584 | 2.301 | 5,654.1 | 5,048.5 | 386,922.0 -> 434,631.4 |
| 1,024 BPMs | 158.500 | 126.577 | 346,625.6 | 277,443.6 | 6,309.1 -> 7,900.3 |
| 256 BPMs plus modifiers | 169.020 | 165.755 | 370,428.2 | 363,388.8 | 5,916.5 -> 6,033.0 |
| Reversed BPM/modifier tables | 172.639 | 169.818 | 378,492.7 | 371,470.8 | 5,792.4 -> 5,888.7 |

### Complete Pump event construction

| Workload | Mode | us/op | Cycles/op | Million events/s |
|---|---|---:|---:|---:|
| 4 short holds | old | 1.671 | 3,669.4 | 14.363 |
| 4 short holds | cached | 1.534 | 3,364.5 | 15.649 |
| 4 short holds | new | 1.534 | 3,365.4 | 15.645 |
| 128 holds, one BPM | old | 986.148 | 2,156,141.6 | 8.567 |
| 128 holds, one BPM | cached | 882.095 | 1,928,053.7 | 9.577 |
| 128 holds, one BPM | new | 842.952 | 1,847,315.4 | 10.022 |
| 128 holds, dense timing | old | 28,941.050 | 63,389,555.4 | 0.292 |
| 128 holds, dense timing | cached | 1,763.294 | 3,851,936.8 | 4.791 |
| 128 holds, dense timing | new | 1,688.000 | 3,690,069.2 | 5.005 |
| 2P, 128 holds each, dense timing | old | 58,010.213 | 127,033,842.6 | 0.291 |
| 2P, 128 holds each, dense timing | cached | 3,684.950 | 8,059,299.0 | 4.585 |
| 2P, 128 holds each, dense timing | new | 3,627.213 | 7,939,233.0 | 4.658 |

### Heap churn per operation

The cached-only Pump variant has the same allocation traffic as old; the change
in this table comes from exact buffer sizing. Calls/bytes include output drops.

| Workload | Old -> new allocation/free calls | Old -> new reallocations | Old -> new requested/freed bytes |
|---|---:|---:|---:|
| Default BPM | 14 -> 10 | 0 -> 0 | 33,088 -> 32,960 |
| One BPM | 14 -> 10 | 0 -> 0 | 33,064 -> 32,960 |
| 1,024 BPMs | 15 -> 10 | 0 -> 0 | 82,176 -> 49,328 |
| 256 BPMs plus modifiers | 16 -> 12 | 0 -> 0 | 66,848 -> 60,624 |
| Reversed BPM/modifier tables | 16 -> 13 | 0 -> 0 | 66,848 -> 62,672 |
| 4 short holds | 2 -> 2 | 2 -> 0 | 1,408 -> 640 |
| 128 holds, one BPM | 2 -> 2 | 6 -> 0 | 782,336 -> 204,800 |
| 128 holds, dense timing | 2 -> 2 | 6 -> 0 | 782,336 -> 204,800 |
| 2P, 128 holds each, dense timing | 3 -> 3 | 7 -> 0 | 1,570,816 -> 409,600 |

Construction saves about 11% of cycles for default/single BPMs and 20% for
1,024 BPMs; the latter reduces allocation traffic by 40%. Modifier-heavy cases
show only about 2% lower median cycles, within timing variation. They still remove
three or four allocations. The allocation test allows 13 calls in debug; release
optimizations reduce the simple measured constructor to 10 calls versus 14 old.

Checkpoint caching is the dominant Pump improvement on timing-heavy charts:
about 94% fewer cycles. Plain one-BPM charts improve more modestly. Comparing
cached-only with exact reservation, median cycles are effectively flat for four
short holds and 1.5-4.2% lower for the larger cases; some individual invocations
are flat or slightly slower. The strongest isolated buffer-sizing result is the
removal of every growth reallocation and roughly 74% less allocation traffic on
large fixtures. End-to-end median cycles improve by 8-94% across these fixtures.
Small-operation timing varies substantially, so the 8% result is not a strong
standalone claim. Fractional/duplicate/invalid BPM tables retain the original
converter; the cache speedups do not apply to that compatibility path.

## Validation and reproduction

- Baseline: 755 gameplay tests and 105 rules tests passed.
- Final debug and release: 761 gameplay tests and 109 rules tests passed, zero
  failures (870 total in each profile); four manual benchmarks are ignored.
- Ten new behavior/allocation tests cover exact timing tables and timestamps,
  input ordering and duplicates, clone offset detachment, cache eligibility,
  fractional BPM/event rows, holds/rolls, two players, taps at checkpoints,
  zero/dense/reversed tickcounts, malformed notes, truncated ranges/caches,
  exact event counts, and allocation budgets.
- Downstream simfile tests: 185 passed, zero failures, one benchmark ignored.
- Workspace binary compilation and Clippy's performance lint group passed.
  Existing warnings outside that group remain.
- New benchmark modules and changed production functions pass Rustfmt checks;
  unrelated existing formatting was preserved.
- Cargo.toml and Cargo.lock bump the workspace patch exactly once,
  0.5.1135 -> 0.5.1136.

```powershell
cargo test -p deadsync-gameplay -p deadsync-rules --lib -- --test-threads=1
cargo test --release -p deadsync-gameplay -p deadsync-rules --lib -- --test-threads=1
cargo test -p deadsync-simfile --lib -- --test-threads=1
cargo check --workspace --bins
cargo clippy -p deadsync-gameplay -p deadsync-rules --all-targets -- -D clippy::perf
```

After builds/tests finish, run both manual benchmarks three times, setting
`DEADSYNC_PERF_REVERSE=1` only for the middle invocation:

```powershell
cargo test --release -p deadsync-gameplay --lib pump_hold_bench -- --ignored --nocapture --test-threads=1
cargo test --release -p deadsync-rules --lib timing_construction_bench -- --ignored --nocapture --test-threads=1
```

Each benchmark prints median/range elapsed time, calling-thread cycles,
throughput, and allocation/reallocation/free traffic. Local raw runs, summaries,
and validation logs are in the ignored `target/timing-perf/` directory.
