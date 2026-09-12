# Pump checkpoint preparation performance - 0.5.1151

This pass applies `M-HOTPATH`, `M-THROUGHPUT`, and the memory-footprint guidance
in the supplied `rust-performance.md` to Pump chart preparation. Measurements
cover event construction and its component routines, not gameplay FPS or audio I/O.

## Three changes

1. **Validate timing-cache eligibility once per player.** The old emitter scanned
   the timing table for every hold. The builder now lazily evaluates that immutable
   predicate for the first usable hold and reuses the result for that player.
   Players without usable holds do not run the scan. Each hold still has its own
   beat-time cursor, and fractional/duplicate BPM tables retain independent time
   conversion when the existing eligibility predicate requires it.
2. **Resume tap-row searches between checkpoints.** Checkpoints use the existing
   hinted partition search to locate their tap row near the previous result.
   Nearby queries avoid repeatedly searching the entire chart. Exponential
   bracketing and binary search retain logarithmic behavior for large gaps and
   rewinds, including reversed tickcount segments. No lookup cache is allocated.
3. **Store tap-row scratch as `u32`.** Rows come from a nonnegative `i32` conversion,
   so their complete domain fits without truncation. On this 64-bit target the
   scratch buffer requests half as many bytes as the old `usize` buffer. Row
   sorting, deduplication, note filtering, and range clipping remain unchanged.

The event buffer and per-player scratch still allocate when needed. Emission into
an already reserved buffer remains allocation-free. No dependencies, global
caches, unsafe code, or public API changes were introduced.

## Method

Measured on Windows x86-64, Intel Xeon E5-2696 v4, rustc 1.98.0, with the repository
release profile (optimization level 3, full LTO). Baseline functions are frozen
from `f1cb2d207` / 0.5.1150; their bodies were checked against that commit ignoring
whitespace and test visibility. Both versions use the same event/note types and
run in the same executable with opaque function dispatch and inputs. The older
0.5.1136 benchmark now explicitly imports the frozen cached emitter so its
historical intermediate comparison remains fixed.

Each workload has three warmups and seven timed batches. Three separate
invocations use old/new, new/old, old/new order. Tables report medians of those
three invocation medians. Builds and other tests were stopped during timing.
Windows `QueryThreadCycleTime` measures calling-thread CPU cycles. Allocation
counters are disabled for timing and enabled for a separate complete operation.
All measured routines run on the calling thread.

Full construction includes allocation, filling, sorting, score-row counting, and
result destruction. Fixture creation is excluded. Component emission tests reuse
a reserved event vector, including its clear/fill in timing. The eligibility
control differs only by accepting eligibility calculated before the measured
batch. The lookup control uses the same `u32` rows and supplied eligibility as the
new emitter, but retains binary search. Full construction includes the actual
lazy eligibility calculation. Scratch tests include allocation and destruction.

Requested/freed bytes measure allocator traffic, not peak RSS, allocator metadata,
or live process memory. Allocation and free counts match in every fixture;
requested and freed bytes match too. Every measured reallocation count is zero.

## Fixtures

The ordinary fixtures contain one tap and one alternating hold/roll per four
beats per player, distributed over five lanes. Timing-change fixtures alternate
120/175 BPM every four beats and add stops, delays, and warps every sixteen beats.
All fixtures use the production default tickcounts.

| Fixture | Holds/player | Hold length (beats) | BPM points | Players | Operations/batch |
| --- | ---: | ---: | ---: | ---: | ---: |
| empty | 0 | 1 | 1 | 1 | 10,000 |
| small | 4 | 1 | 1 | 1 | 1,000 |
| plain | 128 | 16 | 1 | 1 | 64 |
| dense | 128 | 16 | 128 | 1 | 32 |
| versus | 128 | 16 | 128 | 2 | 16 |
| many_changes | 512 | 1 | 1,024 | 1 | 16 |
| long | 32 | 256 | 1 | 1 | 16 |
| tap_heavy | 128 | 1 | 1 | 1 | 128 |

`long` is an overlapping-hold stress fixture. `tap_heavy` instead contains 8,192
notes spaced a quarter beat apart, with one short hold every 64 notes. Component
emission batches contain 32 operations; scratch batches contain 1,024 operations.

## Full construction

Throughput is output events/second; the empty fixture counts complete calls.
Negative cycle changes indicate an improvement.

| Fixture | Old us/op | New us/op | Old cycles/op | New cycles/op | Cycle change | Old throughput/s | New throughput/s |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| empty | 0.033 | 0.039 | 73.1 | 84.2 | +15.18% | 30,084,236 | 25,859,840 |
| small | 1.459 | 1.367 | 3,199.4 | 2,992.9 | -6.45% | 16,447,368 | 17,556,694 |
| plain | 872.058 | 778.847 | 1,902,046.3 | 1,696,443.5 | -10.81% | 9,687,431 | 10,846,805 |
| dense | 1,682.338 | 1,508.203 | 3,669,148.3 | 3,285,983.6 | -10.44% | 5,021,585 | 5,601,368 |
| versus | 3,711.844 | 3,339.988 | 8,127,659.6 | 7,264,105.1 | -10.62% | 4,551,916 | 5,058,702 |
| many_changes | 10,795.906 | 7,357.419 | 23,506,200.6 | 16,019,919.6 | -31.85% | 284,552 | 417,538 |
| long | 3,782.044 | 3,706.631 | 8,282,805.1 | 8,106,368.6 | -2.13% | 8,681,021 | 8,857,639 |
| tap_heavy | 181.970 | 152.277 | 396,844.0 | 333,501.1 | -15.96% | 4,220,469 | 5,043,455 |

| Fixture | Old alloc/free calls | New alloc/free calls | Old bytes allocated/freed | New bytes allocated/freed | Byte reduction |
| --- | ---: | ---: | ---: | ---: | ---: |
| empty | 0/0 | 0/0 | 0/0 | 0/0 | 0.00% |
| small | 2/2 | 2/2 | 640/640 | 608/608 | 5.00% |
| plain | 2/2 | 2/2 | 204,800/204,800 | 203,776/203,776 | 0.50% |
| dense | 2/2 | 2/2 | 204,800/204,800 | 203,776/203,776 | 0.50% |
| versus | 3/3 | 3/3 | 409,600/409,600 | 407,552/407,552 | 0.50% |
| many_changes | 2/2 | 2/2 | 81,920/81,920 | 77,824/77,824 | 5.00% |
| long | 2/2 | 2/2 | 788,480/788,480 | 788,224/788,224 | 0.03% |
| tap_heavy | 2/2 | 2/2 | 83,968/83,968 | 51,200/51,200 | 39.02% |

## Isolated emission comparisons

Every row below has zero allocations, reallocations, frees, and byte churn in both
versions. Throughput counts checkpoints/second.

| Change / fixture | Old us/op | New us/op | Old cycles/op | New cycles/op | Cycle change | Old throughput/s | New throughput/s |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| eligibility / plain | 407.462 | 402.425 | 887,047.5 | 873,678.6 | -1.51% | 20,104,918 | 20,356,588 |
| eligibility / many_changes | 10,403.744 | 7,071.047 | 22,676,839.7 | 15,403,165.8 | -32.08% | 196,852 | 289,632 |
| eligibility / long | 1,406.434 | 1,408.409 | 3,061,592.8 | 3,063,870.2 | +0.07% | 23,298,634 | 23,265,963 |
| eligibility / tap_heavy | 62.356 | 67.769 | 136,727.9 | 146,701.4 | +7.29% | 8,210,885 | 7,555,105 |
| lookup / plain | 398.041 | 304.337 | 869,521.8 | 662,615.6 | -23.80% | 20,580,814 | 26,917,485 |
| lookup / many_changes | 7,134.153 | 7,158.872 | 15,527,086.9 | 15,592,806.5 | +0.42% | 287,070 | 286,079 |
| lookup / long | 1,423.353 | 1,196.178 | 3,095,491.9 | 2,619,636.5 | -15.37% | 23,021,694 | 27,393,913 |
| lookup / tap_heavy | 62.391 | 53.184 | 136,872.0 | 116,307.6 | -15.02% | 8,206,361 | 9,626,888 |

## Scratch collection

Both versions use one allocation and one free, with zero reallocations. Freed
bytes equal requested bytes. Throughput counts input notes/second.

| Fixture | Old ns/op | New ns/op | Cycle change | Old bytes | New bytes | Old throughput/s | New throughput/s |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| plain | 1,569.4 | 1,469.3 | -4.70% | 2,048 | 1,024 | 163,116,172 | 174,228,366 |
| many_changes | 6,720.0 | 6,768.9 | -0.23% | 8,192 | 4,096 | 152,380,510 | 151,279,107 |
| long | 451.2 | 551.0 | +21.81% | 512 | 256 | 141,852,814 | 116,157,391 |
| tap_heavy | 58,767.6 | 60,700.4 | +2.65% | 65,536 | 32,768 | 139,396,590 | 134,957,945 |

## Interpretation and limits

All seven nonempty full-builder fixtures use fewer CPU cycles (2.1-31.9%) and
less byte traffic. The tap-heavy fixture reduces full-builder byte traffic by
39.0% and cycles by 16.0%; its tap-row scratch alone drops from 64 KiB to 32 KiB.
The timing-heavy fixture demonstrates the main eligibility gain, while ordinary
checkpoint emission demonstrates the hinted-search gain (23.8% fewer cycles).

These are not universal speedups for every component. Empty construction increases
from 33.2 to 38.7 ns, with zero churn in both versions. In isolated tests, short
scratch collection uses 21.8% more cycles (about 100 ns), tap-heavy scratch uses
2.7% more, eligibility-only tap-heavy emission uses 7.3% more, and timing-heavy
lookup uses 0.4% more. These costs and code-layout/timing noise are included in
the full-builder comparison, which improves for those nonempty workloads. The
memory reduction is deterministic; timing percentages remain workload- and
machine-dependent. No claim about gameplay FPS or peak process memory is made.

## Validation

- `cargo test -p deadsync-gameplay --locked`: 772 passed, 4 manual benchmarks ignored.
- `cargo test -p deadsync-gameplay --release --locked`: 772 passed, 4 ignored.
- `cargo test -p deadsync-theme --lib --locked -- --test-threads=1`: 22 passed.
- `cargo test -p deadsync-theme-simply-love --lib --locked -- --test-threads=1`:
  1,263 passed, 4 ignored.
- `cargo check -p deadsync --all-targets --locked`: passed.
- `cargo clippy -p deadsync-gameplay --all-targets --locked -- -D clippy::perf`:
  passed; non-performance style warnings remain.
- All 20 benchmark pairs completed three invocations. Source hashes remained
  unchanged through benchmarking and subsequent validation.

The eight added regression tests compare every event field and score-row total
against the frozen baseline. They cover empty and two-player charts, 120 timing/
hold combinations, fractional and duplicate BPMs, reversed/duplicate tickcounts,
invalid or filtered notes, clipped/reversed ranges, truncated timestamp arrays,
64 deterministic perturbed charts, and tap-row conversion at representative
finite/nonfinite beats, including the i32 upper limit. Allocation tests
assert zero churn during prepared emission and exact compact-scratch budgets.

The workspace version changes exactly once from 0.5.1150 to 0.5.1151, with
`Cargo.toml` and all three matching lockfile package entries updated.

Reproduce the benchmark with:

```powershell
cargo test -p deadsync-gameplay --release --locked pump_checkpoint_bench -- --ignored --nocapture --test-threads=1
```

Run three times, setting `DEADSYNC_PERF_REVERSE=1` only for the middle invocation
and removing it for the others. The shared harness reports all seven timing
samples' range alongside each median, cycles, throughput, and allocator counts.
