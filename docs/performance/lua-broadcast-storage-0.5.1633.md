# Lua broadcast storage and immutable getters - 0.5.1633

Parent: `d95a183b6` (0.5.1632). Date: 2026-09-30.

This pass follows the local performance guide's M-HOTPATH, M-MEM-REUSE,
M-INITIAL-CAPACITY and M-THROUGHPUT recommendations in three areas:

1. Actor message dispatch snapshots registries of up to 32 entries inline.
   Larger registries reserve a heap buffer using their existing raw length.
   All actors are still collected before parameter normalization or callbacks;
   mutation and nested broadcasts therefore keep the same dispatch order.
   The snapshot drains in place to avoid moving its inline buffer while keeping
   element release order. The guarded command creates its long scope key once
   and reuses that Lua string for reading, setting and restoring the scope.
   The parent created a fresh 41-byte Lua string for each of those accesses.
2. Judgment normalization collects key/value pairs directly into inline
   storage for typical compact Notes tables through 16 entries. It reserves
   from the table's raw length, avoiding growth for larger dense tables;
   hash-only or mixed tables can grow as needed. The snapshot drains in place.
   Snapshotting still finishes before normalization, so
   callbacks and metamethods can replace or insert notes without changing
   which original values are normalized. Notes and result-table aliases,
   defaults, eager fallback reads and partial writes keep their behavior.
3. Immutable string getters retain one Lua string when constructed and return
   that same value. Their callbacks ignore arguments without materializing a
   MultiValue buffer. This removes argument-carrier allocations and repeated
   construction of long Lua strings. Empty strings, UTF-8, embedded NULs,
   arbitrary extra arguments and construction-time capture remain supported.
   This helper serves host metadata tables as well as Judgment notes.

There is no new dependency, public API change, unsafe code or global
cache. Inline storage increases bounded stack use. Listener dispatch still
creates one long Lua scope key per callback; actor or note callbacks can
allocate for their own work. Required note tables, functions and captured
values still need storage. Zero churn applies to the measured warm paths,
not to an entire game frame.

## Measurements

The [raw CSV](lua-broadcast-storage-0.5.1633.csv) contains 264 rows: 33 workloads,
two implementations and four independent runs. Percentage ranges cover all
four paired runs. Absolute values use run 1. A warm operation contains 64
getter calls, 16 broadcasts or 32 normalizations, as labeled. Cold operations
contain one construction, normalization or complete Judgment broadcast.

| Workload | Thread cycles/op, old -> new | CPU reduction | Throughput gain | Allocations/reallocations, old -> new | Requested bytes, old -> new |
| --- | ---: | ---: | ---: | ---: | ---: |
| 64 getter calls / short | 41,258.5 -> 17,387.9 | 53.7% to 58.5% | 116.1% to 140.6% | 64/0 -> 0/0 | 2,560 -> 0 |
| 64 getter calls / 4,096 bytes | 405,742.3 -> 17,107.3 | 95.1% to 95.8% | 1952.6% to 2276.8% | 128/0 -> 0/0 | 266,304 -> 0 |
| 16 broadcasts / 8 actors, no listeners | 71,686.5 -> 74,630.9 | -4.1% to 7.7% | -3.9% to 8.4% | 16/0 -> 0/0 | 3,072 -> 0 |
| 16 broadcasts / 8 actors, listeners | 573,526.9 -> 541,448.2 | 5.6% to 8.9% | 5.9% to 9.9% | 400/0 -> 128/0 | 28,416 -> 8,448 |
| 16 broadcasts / 128 actors, listeners | 8,937,147.4 -> 8,047,551.6 | 9.9% to 11.4% | 10.9% to 12.9% | 6160/0 -> 2064/0 | 454,656 -> 184,320 |
| 32 prepared normalizations / 4 notes | 124,895.1 -> 121,439.2 | 1.7% to 3.6% | 1.8% to 3.8% | 32/0 -> 0/0 | 10,240 -> 0 |
| 32 prepared normalizations / 16 notes | 359,484.4 -> 335,545.2 | 2.3% to 8.0% | 2.4% to 8.6% | 32/64 -> 0/0 | 71,680 -> 0 |
| Fresh Judgment normalization / 4 notes | 108,390.1 -> 105,063.4 | 3.1% to 5.4% | 3.2% to 7.1% | 299/0 -> 281/0 | 14,823 -> 14,275 |
| Fresh Judgment normalization / 16 notes | 383,809.4 -> 373,385.0 | 2.3% to 3.3% | 2.3% to 3.4% | 1067/2 -> 989/2 | 54,927 -> 55,427 |
| Complete Judgment broadcast / 8 actors, 4 notes | 251,667.0 -> 197,870.7 | 17.0% to 21.4% | 20.9% to 27.5% | 442/3 -> 322/4 | 26,712 -> 22,603 |

All new warm getter batches and prepared Judgment snapshots up to 16 notes have zero allocation, reallocation, free and byte churn in all four runs. Warm broadcasts without listeners have zero churn through 32 actors; listeners still require one long Lua scope key per callback.

64 getter calls / short: allocations/reallocations/frees change from 64/0/64 to 0/0/0; requested/freed bytes change from 2,560/2,560 to 0/0.

64 getter calls / 4,096 bytes: allocations/reallocations/frees change from 128/0/64 to 0/0/0; requested/freed bytes change from 266,304/2,560 to 0/0.

16 broadcasts / 8 actors, listeners: allocations/reallocations/frees change from 400/0/16 to 128/0/0; requested/freed bytes change from 28,416/3,072 to 8,448/0.

16 broadcasts / 8 actors, no listeners: allocations/reallocations/frees change from 16/0/16 to 0/0/0; requested/freed bytes change from 3,072/3,072 to 0/0.

32 prepared normalizations / 4 notes: allocations/reallocations/frees change from 32/0/32 to 0/0/0; requested/freed bytes change from 10,240/10,240 to 0/0.

32 prepared normalizations / 64 notes: allocations/reallocations/frees change from 32/128/32 to 32/0/32; requested/freed bytes change from 317,440/317,440 to 163,840/163,840.

Fresh Judgment normalization / 4 notes: allocations/reallocations/frees change from 299/0/60 to 281/0/59; requested/freed bytes change from 14,823/2,752 to 14,275/2,432.

Complete Judgment broadcast / 8 actors, 4 notes: allocations/reallocations/frees change from 442/3/165 to 322/4/67; requested/freed bytes change from 26,712/9,648 to 22,603/6,016.

Allocator counts agree across all four runs for every workload and implementation.

## Controls and limits

Negative percentages indicate increased cost or decreased throughput. Empty,
no-argument, cold-construction and heap-spill controls remain in the results.

| Workload | Thread cycles/op, old -> new | CPU reduction | Throughput gain | Allocations/reallocations, old -> new | Requested bytes, old -> new |
| --- | ---: | ---: | ---: | ---: | ---: |
| 64 getter calls / empty | 37,827.5 -> 18,545.0 | 51.0% to 53.4% | 104.2% to 114.6% | 64/0 -> 0/0 | 2,560 -> 0 |
| 64 getter calls / Unicode/NUL, three arguments | 46,245.5 -> 19,010.1 | 55.8% to 58.9% | 126.6% to 143.2% | 64/0 -> 0/0 | 7,680 -> 0 |
| 64 getter calls / 128 bytes | 97,172.5 -> 17,592.8 | 66.7% to 82.4% | 200.3% to 467.9% | 128/0 -> 0/0 | 12,352 -> 0 |
| 64 getter calls / short, no arguments | 27,091.7 -> 16,733.0 | 35.9% to 38.2% | 56.2% to 61.8% | 0/0 -> 0/0 | 0 -> 0 |
| Cold getter construction / short | 3,513.7 -> 4,979.9 | -41.7% to 5.2% | -39.5% to 0.1% | 6/0 -> 6/0 | 223 -> 248 |
| Cold construction plus first call / short | 7,109.7 -> 6,044.9 | 5.8% to 15.0% | 9.4% to 26.6% | 9/0 -> 8/0 | 431 -> 392 |
| Cold getter construction / 4,096 bytes | 4,414.1 -> 4,345.4 | -0.8% to 1.6% | -1.0% to 2.2% | 6/0 -> 6/0 | 4,304 -> 4,329 |
| Cold construction plus first call / 4,096 bytes | 8,872.6 -> 5,674.4 | 7.2% to 36.0% | 18.9% to 75.4% | 9/0 -> 8/0 | 8,593 -> 4,473 |
| 16 broadcasts / 0 actors, no listeners | 29,394.1 -> 34,920.2 | -18.8% to -9.8% | -15.8% to -8.9% | 0/0 -> 0/0 | 0 -> 0 |
| 16 broadcasts / 1 actor, no listeners | 37,864.6 -> 37,622.8 | -4.1% to 4.4% | -4.0% to 4.6% | 16/0 -> 0/0 | 384 -> 0 |
| 16 broadcasts / 32 actors, no listeners | 187,419.6 -> 194,973.5 | -7.2% to -2.2% | -6.8% to -2.2% | 16/0 -> 0/0 | 12,288 -> 0 |
| 16 broadcasts / 33 actors, no listeners | 194,061.2 -> 204,893.0 | -5.6% to -3.3% | -5.3% to -3.1% | 16/0 -> 16/0 | 12,672 -> 12,672 |
| 16 broadcasts / 128 actors, no listeners | 657,014.9 -> 694,847.0 | -5.8% to -1.3% | -5.4% to -1.2% | 16/0 -> 16/0 | 49,152 -> 49,152 |
| 16 broadcasts / 0 actors, listeners | 30,708.6 -> 32,308.5 | -12.8% to -5.2% | -11.3% to -5.0% | 0/0 -> 0/0 | 0 -> 0 |
| 16 broadcasts / 1 actor, listeners | 100,350.1 -> 97,554.9 | 2.8% to 8.8% | 2.9% to 9.6% | 64/0 -> 16/0 | 3,552 -> 1,056 |
| 16 broadcasts / 32 actors, listeners | 2,220,175.6 -> 2,010,270.2 | 9.5% to 13.0% | 10.5% to 14.9% | 1552/0 -> 512/0 | 113,664 -> 33,792 |
| 16 broadcasts / 33 actors, listeners | 2,280,294.6 -> 2,111,098.7 | 7.4% to 9.9% | 8.0% to 11.0% | 1600/0 -> 544/0 | 117,216 -> 47,520 |
| 32 prepared normalizations / 1 note | 69,377.9 -> 66,722.9 | -2.6% to 8.5% | -2.4% to 9.3% | 32/0 -> 0/0 | 10,240 -> 0 |
| 32 prepared normalizations / 17 notes | 381,747.4 -> 363,499.7 | 4.3% to 7.8% | 4.6% to 8.5% | 32/96 -> 32/0 | 153,600 -> 43,520 |
| 32 prepared normalizations / 64 notes | 1,264,966.1 -> 1,167,652.5 | 2.6% to 12.7% | 2.7% to 14.6% | 32/128 -> 32/0 | 317,440 -> 163,840 |
| Fresh Judgment normalization / 1 note | 41,248.8 -> 39,811.9 | -8.1% to 3.5% | -7.7% to 3.7% | 107/0 -> 104/0 | 5,277 -> 4,987 |
| Fresh Judgment normalization / 17 notes | 414,990.5 -> 465,883.7 | -12.3% to 8.1% | -10.9% to 8.9% | 1131/3 -> 1049/2 | 60,669 -> 59,883 |
| Fresh Judgment normalization / 64 notes | 1,549,419.7 -> 1,462,300.4 | -2.7% to 5.6% | -2.6% to 5.9% | 4139/5 -> 3822/4 | 216,703 -> 224,675 |

Workloads with higher CPU cost in every run: 16 broadcasts / 0 actors, no listeners, 16 broadcasts / 32 actors, no listeners, 16 broadcasts / 33 actors, no listeners, 16 broadcasts / 128 actors, no listeners, 16 broadcasts / 0 actors, listeners.

Workloads with mixed CPU changes: Cold getter construction / short, Cold getter construction / 4,096 bytes, 16 broadcasts / 1 actor, no listeners, 16 broadcasts / 8 actors, no listeners, 32 prepared normalizations / 1 note, Fresh Judgment normalization / 1 note, Fresh Judgment normalization / 17 notes, Fresh Judgment normalization / 64 notes.

Heap-spill and cold cases include their full carrier and construction work. No uniform CPU improvement is claimed for mixed or slower cases; all their samples remain in the CSV.

Cases with increased requested bytes: Cold getter construction / short (223 -> 248 bytes); Cold getter construction / 4,096 bytes (4,304 -> 4,329 bytes); Fresh Judgment normalization / 16 notes (54,927 -> 55,427 bytes); Fresh Judgment normalization / 64 notes (216,703 -> 224,675 bytes).

Cases with increased reallocation counts: Complete Judgment broadcast / 8 actors, 4 notes (3 -> 4).

Getter construction now retains Lua strings and owning Lua references. Some cold note fixtures require more byte churn despite fewer allocation calls. These results qualify the memory benefit; no improvement in every metric for every input is claimed.

Creating a getter now creates its Lua string before the first call. An unused
getter can therefore retain Lua string storage earlier. Long strings are
shared by subsequent calls; the parent created separate long Lua strings on
calls. Cold construction and construction plus first call are both measured
so this shift in cost is visible. The parent captured a Rust String; the new
closure captures an owning Lua string handle. Neither requested byte churn nor
inline storage implies lower retained memory for every fixture.

Lua GC is stopped during measurements to exclude collection pauses. Long
4,096-byte getter batches use fewer timing iterations to bound uncollected
output, equally for both implementations. Old long-string outputs remain
uncollected until fixture destruction outside the measured window. Reported
freed bytes therefore do not include that deferred collection. Cold fixtures
are freshly constructed and destroyed outside timing and allocator counts.
Their required output tables/functions also remain alive outside that window.
The same initialized runtime clock is supplied to both variants. Prepared
normalization fixtures use non-nil getter markers to isolate snapshot storage;
fresh-note and complete-broadcast fixtures construct and call actual getters.

No peak RSS, instruction-count, whole-song loading or game frame-rate claim
is made. Requested/freed bytes record allocator work; Windows thread cycle
counts measure this executing thread rather than wall-clock TSC ticks.

## Method and behavior checks

Twelve baseline functions are frozen from `d95a183b6`: broadcast dispatch and
parameter normalization, Judgment and tap-note construction/normalization,
getter installation, and guarded actor dispatch. The old broadcast calls its
frozen normalization and dispatch helpers, which call the frozen getter.
Unchanged queue and callback helpers are shared; benchmark queues are empty.
Complete Judgment benchmarks supply explicit parameter tables. A source audit
checks all twelve functions against the parent, modulo formatting.

The existing scoped System allocator harness records allocation, reallocation,
free, requested bytes and freed bytes on this thread. Timing reports medians
and min/max from seven samples, separately from allocator accounting. Four
serial release runs alternate old-first and new-first order. Warm benchmarks
use batched operations; cold benchmarks use fresh Lua fixtures and include the
same per-operation clock overhead. Fixture creation is outside measurement.
CPU and throughput percentages compare each run's paired medians.

The release profile uses optimization level 3 and fat LTO. Environment:
Windows x86_64; Rust 1.98.1 / LLVM 22.1.8; Intel Xeon E5-2696 v4 at 2.20 GHz,
22 physical cores / 44 logical processors.

Ten new behavior/allocation tests compare the frozen parent with the new code:

- Getter bytes, empty/long/Unicode/NUL values, source mutation, retained results,
  replacement methods, and 0 through 257 arbitrary arguments.
- Getter installation metamethods, callbacks and errors after partial writes.
- Warm short/long getters with zero allocation, reallocation, free or byte churn.
- Actor registry mutation, nested messages, shared parameters, callback errors,
  command-state cleanup and restoration of the previous broadcast scope.
- Actor ordering, non-table entries, sequence gaps, empty messages and the
  32/33-actor boundary through 257 actors.
- Reduced warm dispatch churn with and without listeners; inline snapshot
  carriers avoid heap storage while listener execution retains a scope key.
- Judgment defaults, scalar notes, missing/empty/mixed-key Notes tables,
  field aliases, result identity, repeated normalization, and 16/17 boundaries.
- Note insertion/replacement during normalization, original-value snapshots,
  metamethod access order, partial writes and errors before later notes.
- Invalid UTF-8 field fallback and eager parameter lookup errors.
- Prepared Judgment snapshots up to 16 notes with zero heap churn.

Full debug and release library suites: **694 passed, 4 failed, 67 ignored** in
each. All ten new tests pass. The four failures match the parent's names,
assertions and source locations:

- `compile_song_lua_extracts_actorproxy_targets`
- `compile_song_lua_resolves_local_and_hidden_screen_proxy_targets`
- `compile_song_lua_runs_cmd_queuecommand_builders`
- `compile_song_lua_supports_notefield_column_api`

Clippy completes with inherited warnings and no warnings in the new fixtures.
Scoped rustfmt checks and `git diff --check` pass. Locked Cargo metadata and
manifest/lock audits verify exactly `0.5.1632 -> 0.5.1633`, including all three
packages inheriting the workspace version. Cargo.toml and Cargo.lock are part
of this pass. The four user-excluded local files are absent from the commit.

Reproduce:

```powershell
cargo test --locked -p deadsync-song-lua --lib broadcast_storage -- --test-threads=1
cargo test --locked -p deadsync-song-lua --lib -- --test-threads=1
cargo test --locked -p deadsync-song-lua --release --lib -- --test-threads=1
cargo clippy --locked -p deadsync-song-lua --lib --tests
cargo test --locked -p deadsync-song-lua --release --lib broadcast_storage_bench -- --ignored --nocapture --test-threads=1
$env:DEADSYNC_PERF_REVERSE = '1'
cargo test --locked -p deadsync-song-lua --release --lib broadcast_storage_bench -- --ignored --nocapture --test-threads=1
Remove-Item Env:DEADSYNC_PERF_REVERSE
```
