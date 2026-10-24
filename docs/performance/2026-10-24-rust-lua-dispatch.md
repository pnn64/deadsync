# Lua 5.1 compatibility and dispatch measurements

The engine now supports metatable dispatch, protected calls, arbitrary error
objects, iteration, function/thread environments, dynamic compilation and output.
This remains an incomplete Lua implementation: the official suite probes still
stop at missing standard libraries and GC. These measurements cover six existing
microbenchmarks, not complete Lua or gameplay workloads.

The added compatibility paths exposed substantial dispatch overhead. Guarding
ordinary numeric operands and tables without metatables alone did not produce a
consistent numeric-loop improvement. Retaining stable code/frame context outside
the instruction loop produced the larger measured improvement. Calls refresh
context, including environments changed by native callbacks. Immutable code uses
one shared owner per frame entry; value/register operations do not change reference
counts. The implementation remains safe Rust.

Each column below is a median of five paired Rust/reference wall-time ratios;
lower is better. The final confirmation is a separate run on the same core code.

| Workload | Compatibility paths | Guarded paths | Frame-context loop | Confirmation |
| --- | ---: | ---: | ---: | ---: |
| Numeric for | 21.91 | 22.32 | 6.79 | 6.63 |
| Numeric while | 20.31 | 19.99 | 5.89 | 5.86 |
| Array table | 10.05 | 9.85 | 3.40 | 3.88 |
| Recursive Fibonacci | 7.84 | 7.38 | 3.79 | 3.69 |
| Captured-cell calls | 6.62 | 6.58 | 3.53 | 3.56 |
| Byte concatenation | 2.91 | 2.79 | 2.53 | 2.57 |

Raw samples, build metadata and source hashes:

- [Compatibility paths](2026-10-24-rust-lua-metatables.json)
- [Guarded paths](2026-10-24-rust-lua-fast-paths.json)
- [Frame-context loop](2026-10-24-rust-lua-dispatch.json)
- [Confirmation](2026-10-24-rust-lua-dispatch-confirm.json)

Each JSON has a matching TSV and build log. Run the current benchmark with:

```sh
python scripts/benchmark_rust_lua.py --output docs/performance/rust-lua-current.json
```

Warmed numeric loops still record zero allocations/reallocations, and values
remain 16 bytes. The allocation-heavy concatenation path still reports 2,999
allocation/reallocation events per invocation. Other missing metrics and the
instrumentation limitations are recorded in each JSON and the
[initial baseline](2026-10-24-rust-lua-baseline.md). Startup library surfaces still
differ. There are no live/peak Rust memory or process RSS measurements yet.

All 22 differential/adversarial groups, the verifier mutation test and documentation
example pass, including 325 generated metamethod combinations and a native
metamethod that changes the running function's environment. Strict Clippy passes.
The [official suite gap report](../testing/2026-10-24-rust-lua-suite.md) remains
failing and sets the next compatibility priorities.
