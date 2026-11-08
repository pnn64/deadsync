# Userdata and finalizer measurements

The harness now runs twenty correctness-checked workloads against official
vendored Lua 5.1.5 on Windows x86-64 MSVC, static CRT, in the C locale. Each run
takes five paired wall time samples with alternating engine order. Automatic
collection is enabled except where a script explicitly stops it. Ratios below
are medians of paired Rust/reference durations; lower is better.

| Workload | Implementation | Confirmation |
| --- | ---: | ---: |
| Numeric `for` | 6.80 | 7.03 |
| Numeric `while` | 6.13 | 6.46 |
| Array table | 3.10 | 3.20 |
| Recursive Fibonacci | 4.28 | 4.40 |
| Captured-cell calls | 3.98 | 3.92 |
| Byte concatenation | 5.03 | 5.24 |
| Pattern search | 0.93 | 0.92 |
| Pattern replacement | 2.72 | 2.71 |
| Replacement callback | 2.89 | 2.86 |
| String formatting | 1.13 | 1.16 |
| Numeric sorting | 2.24 | 2.12 |
| Lua comparator sorting | 2.21 | 2.22 |
| Coroutine resume | 2.63 | 2.57 |
| Coroutine wrapper | 3.13 | 3.26 |
| Deep coroutine yield | 2.79 | 2.74 |
| Existing coroutine reuse | 3.08 | 3.32 |
| Cyclic table churn | 1.31 | 1.33 |
| Weak-value table churn | 1.15 | 1.16 |
| Proxy construction and lookup | 1.33 | 1.31 |
| Userdata finalization | 1.29 | 1.23 |

The proxy workload creates 5,000 userdata values sharing one validated metatable,
checks the metatable for each value and completes collection. The finalizer
workload stops automatic collection, creates 3,001 userdata values, drops their
references, collects and checks that all callbacks ran. Callbacks increment a
shared Lua cell. Collection is restarted before returning.

After the first paired sample, proxy construction records zero Rust allocation/
reallocation events in each of the remaining four samples. The arenas and weak
metatable set reuse storage. The finalizer workload records three allocation
events per invocation and no reallocations in those four samples, independent
of its 3,001 callbacks. Allocations cover the new closure/upvalue payload and
metatable hash storage; callbacks use reusable registers and explicit frames.
The first sample includes additional pool growth and is retained in the data.

Numeric loops, warmed pattern search/replacement/callback workloads and persistent
coroutine reuse retain zero allocation/reallocation events across all five samples.
Values remain 16 bytes; frames remain 64 bytes on 64-bit targets. Most execution
workloads are still slower than the reference. These measurements do not establish
an overall speed improvement, full library performance or gameplay performance.
The machine is not CPU-isolated, so run-to-run timing variation remains visible.

The following capacity-based estimates and object counts repeat in both runs.
They include retained working buffers and reusable arena capacities, exclude
allocator headers/exact hash layouts and are not RSS or exact allocator peaks.

| Workload | Userdata before final host collection | Userdata after | Estimated live bytes after | Estimated reserved bytes after | Observed peak reserved bytes | Objects reclaimed across run |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Proxy construction | 1 | 0 | 13,919 | 82,247 | 82,247 | 505,202 |
| Finalizer dispatch | 3,001 | 0 | 197,381 | 725,501 | 725,501 | 303,404 |

Finalized userdata remains allocated until a subsequent sweep, matching the
separation between callback execution and reclamation. The last host collection
demonstrates that no userdata from either workload remains live. Reserved pool
storage is retained for reuse. Reference heap counts are also recorded, but the
library surfaces and accounting differ and do not establish equivalent-application
memory ratios.

Raw samples and source/build/machine metadata:

- [Implementation](2026-10-25-rust-lua-userdata.json)
- [Confirmation](2026-10-25-rust-lua-userdata-confirm.json)

Each JSON has a matching TSV and build log. Source hashes match the final code
and are checked before/after each measurement. Reproduce with:

```sh
python scripts/benchmark_rust_lua.py --output docs/performance/rust-lua-current.json
```

Instrumentation counts Rust `GlobalAlloc` calls; direct native C malloc is excluded.
Execution includes host entry/return and compilation includes drop. Collection
cycles remain atomic. Exact allocator peaks, process RSS, CPU cycles, other Rust
engines and complete-library workloads remain unmeasured. See the
[validation report](../testing/2026-10-25-rust-lua-userdata.md) for compatibility
coverage and remaining work.
