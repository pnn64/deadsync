# Tracing collection measurements

The reference harness runs eighteen correctness-checked workloads against the
vendored official Lua 5.1.5 interpreter on Windows x86-64 MSVC, static CRT, in
the C locale. Automatic collection is enabled. Each run takes five paired wall
time samples, alternating engine order. Ratios below are medians of paired
Rust/reference durations; lower is better.

| Workload | Final implementation | Confirmation |
| --- | ---: | ---: |
| Numeric `for` | 7.24 | 6.90 |
| Numeric `while` | 6.36 | 6.26 |
| Array table | 3.94 | 3.13 |
| Recursive Fibonacci | 4.26 | 4.13 |
| Captured-cell calls | 4.05 | 3.84 |
| Byte concatenation | 4.99 | 4.96 |
| Pattern search | 0.93 | 0.87 |
| Pattern replacement | 2.80 | 2.77 |
| Replacement callback | 2.90 | 2.86 |
| String formatting | 1.15 | 1.11 |
| Numeric sorting | 2.14 | 2.13 |
| Lua comparator sorting | 2.26 | 2.19 |
| Coroutine resume | 2.73 | 2.79 |
| Coroutine wrapper | 3.39 | 3.16 |
| Deep coroutine yield | 2.74 | 2.99 |
| Existing coroutine reuse | 3.35 | 3.37 |
| Cyclic table churn | 1.29 | 1.28 |
| Weak-value table churn | 1.16 | 1.17 |

The cyclic workload creates 5,000 self-referencing tables per invocation. The
weak workload creates 3,000 short-lived values and verifies that full collection
clears the weak table. Both complete an explicit collection before returning.
The machine is not CPU-isolated; array timing in particular varies substantially.
Most workloads remain slower than the reference. This milestone establishes
reclamation behavior and costs, not an overall speed improvement.

Numeric loops, pattern search/replacement/callback workloads and persistent
coroutine reuse record zero warmed Rust allocation/reallocation events in all
five samples of both final runs. Values remain 16 bytes; a compile-time assertion
preserves 64-byte frames on 64-bit targets despite generation-bearing handles.
The final run measures numeric-loop compilation/drop at 11.30 microseconds for
Rust and 6.84 for the reference. Startup/drop measures 27.81 versus 145.73
microseconds, but the engines open different library surfaces.

The harness also records VM-owned storage estimates before and after a final
host collection. Live estimates include retained working-buffer capacities;
reserved estimates additionally include vacant arena slots. Peaks are observed
at statistics/collection points, not exact allocator high-water marks. Shared
compiled code is counted once per loaded program. Allocator headers and exact
hash bucket layouts are excluded; these numbers are not process RSS.

| Workload | Estimated live after collection, bytes | Estimated reserved after collection, bytes | Observed peak reserved, bytes | Objects reclaimed across run |
| --- | ---: | ---: | ---: | ---: |
| Array table | 12,918 | 15,614 | 556,286 | 251 |
| Byte concatenation | 13,552 | 25,224 | 77,309 | 150,849 |
| Cyclic table churn | 13,052 | 38,820 | 76,452 | 505,000 |
| Weak-value table churn | 38,188 | 137,780 | 256,820 | 303,202 |
| Persistent coroutine reuse | 15,127 | 16,823 | 16,823 | 0 |

These estimates and object totals repeat in the confirmation run. The harness
also records the reference's reported heap bytes after collection. Its library
surface, allocator accounting and retained capacities differ, so those totals
do not establish equivalent-application memory ratios.

An initial collection run queued scalar table entries as trace work. Filtering
nil/boolean/number edges reduced the array workload's retained reserved estimate
from 340,734 to 15,614 bytes and its observed peak from 1,258,222 to 556,286.
Smaller retained buffers also change automatic collection thresholds and cycle
counts. Timing does not establish a throughput improvement from this change.

Raw samples, allocation totals and source/build/machine metadata:

- [Initial tracing implementation](2026-10-25-rust-lua-gc.json)
- [Scalar-edge filtering](2026-10-25-rust-lua-gc-final.json)
- [Confirmation](2026-10-25-rust-lua-gc-confirm.json)

Each JSON has a matching TSV and build log. Source hashes are checked before
and after measurement. Reproduce with:

```sh
python scripts/benchmark_rust_lua.py --output docs/performance/rust-lua-current.json
```

Allocation instrumentation covers Rust `GlobalAlloc` calls, excluding allocations
that use native C malloc directly. Execution includes host entry/return; compilation
includes drop. The collector completes cycles atomically; incremental pauses and
userdata/finalization remain unfinished. Exact allocator peaks, RSS, CPU cycles,
other Rust engines and full-library/gameplay workloads remain unmeasured. See
the [validation report](../testing/2026-10-25-rust-lua-gc.md) for the remaining
compatibility gates.
