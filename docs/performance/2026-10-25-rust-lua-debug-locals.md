# Local inspection and operand-reference measurements

The harness now measures 24 execution workloads and eight frontend compilation
cases. Each has five paired wall-time samples in alternating engine order. The
reference is vendored official Lua 5.1.5 on Windows x86-64 MSVC, static CRT and C
locale. Automatic collection stays enabled. The machine is not CPU-isolated.

Local operand references fix reads after right-hand callbacks and remove eager
register copies from arithmetic and indexed operations. Conflicting assignment
addresses are copied where required. The existing loop/array fixtures produce
identical checked results in both engines. Their paired Rust/reference ratios
change as follows; lower is better:

| Workload | Prior | Prior confirmation | Current | Confirmation |
| --- | ---: | ---: | ---: | ---: |
| `numeric_for` | 7.082 | 6.680 | 5.703 | 5.656 |
| `numeric_while` | 6.381 | 6.702 | 4.712 | 4.788 |
| `array_table` | 3.233 | 3.209 | 2.475 | 2.593 |

Both current runs improve these ratios, while remaining slower than the reference.
This is a correctness change with measured benefits on workloads that used those
copies; it does not establish overall engine parity. Raw samples and machine/build
metadata are retained for assessing variation.

The new `debug_locals` script calls a Lua function 1,000 times. Each invocation
reads its first parameter with `debug.getlocal(1, 1)` and returns that value;
the checked sum is 500,500. Ten complete script invocations are timed per sample.
The reference opens `ALL_SAFE | DEBUG` for `debug_info` and `debug_locals` only;
other workloads keep their previous library configuration.

| Local-inspection script | Current | Confirmation |
| --- | ---: | ---: |
| rust | 0.365 ms | 0.335 ms |
| lua51 | 0.136 ms | 0.130 ms |

All Rust inspection samples record zero allocation calls. Reallocation totals
over their ten script invocations are 4, 2, 0, 2 and 0 in both runs. The VM retains
reusable arena storage, and each invocation creates a fresh closure. Names
resolve through prototype indices, and temporary register maps use one flat byte
arena per prototype. These timings include the surrounding Lua calls, loop and
host entry/return, so they are not an isolated native-function microbenchmark.

| All execution workloads | Current paired Rust/reference | Confirmation |
| --- | ---: | ---: |
| `array_table` | 2.475 | 2.593 |
| `byte_concat` | 5.266 | 5.300 |
| `closure_calls` | 3.838 | 3.867 |
| `coroutine_deep_yield` | 2.596 | 2.607 |
| `coroutine_resume` | 2.557 | 2.582 |
| `coroutine_reuse` | 3.349 | 3.350 |
| `coroutine_wrap` | 3.326 | 3.488 |
| `debug_info` | 1.016 | 1.023 |
| `debug_locals` | 2.764 | 2.613 |
| `gc_cycles` | 1.298 | 1.252 |
| `gc_weak_values` | 1.044 | 1.078 |
| `numeric_for` | 5.703 | 5.656 |
| `numeric_while` | 4.712 | 4.788 |
| `pattern_callback` | 3.067 | 3.029 |
| `pattern_find` | 0.874 | 0.899 |
| `pattern_replace` | 2.906 | 2.848 |
| `reader_load_bytes` | 2.747 | 2.743 |
| `reader_load_single` | 2.215 | 2.210 |
| `recursive_fib` | 3.866 | 3.929 |
| `string_format` | 1.177 | 1.170 |
| `table_sort` | 2.080 | 2.091 |
| `table_sort_callback` | 1.953 | 1.992 |
| `userdata_finalize` | 1.318 | 1.358 |
| `userdata_proxy` | 1.262 | 1.284 |

Compilation includes destruction; source generation and correctness execution
are outside timing. The six comment/string cases remain, and two new cases
compile 1,000 and 10,000 short scopes with repeated local declarations and nested
calls. Active-name bitmaps avoid rescanning prior declarations at every call,
and temporary-map verification checks bounded register/GC-map membership.

| Compilation workload | Current paired Rust/reference | Confirmation |
| --- | ---: | ---: |
| `frontend_comment_10000` | 0.417 | 0.419 |
| `frontend_comment_100000` | 0.345 | 0.299 |
| `frontend_comment_1000000` | 0.279 | 0.285 |
| `frontend_quoted_10000` | 0.355 | 0.320 |
| `frontend_quoted_100000` | 0.301 | 0.284 |
| `frontend_quoted_1000000` | 0.311 | 0.316 |
| `frontend_scopes_1000` | 4.712 | 4.689 |
| `frontend_scopes_10000` | 4.653 | 4.559 |

| Dense compilation | Rust current | Rust confirmation | Reference current | Reference confirmation |
| --- | ---: | ---: | ---: | ---: |
| `frontend_scopes_1000` | 4.699 ms | 4.990 ms | 1.021 ms | 1.067 ms |
| `frontend_scopes_10000` | 48.427 ms | 49.970 ms | 10.255 ms | 10.943 ms |

A tenfold scope increase takes approximately tenfold compilation time here. Two
sizes do not prove a general complexity bound. Dense compilation remains about
4.6–4.7 times the reference duration and allocates substantially: 13,042 Rust
allocations and 3,214,282 requested bytes per 1,000-scope compilation; 130,042
allocations and 37,044,282 requested bytes per 10,000 scopes. This is a remaining
frontend performance cost. Counters sum `GlobalAlloc` allocation/reallocation
requests and exclude direct native C malloc; they are not live bytes or RSS.

The local-inspection workload retains 14,895 estimated live bytes and 19,695
reserved bytes after full collection in both runs, with one collection and 52
reclaimed objects. Estimates include metadata, collector and reusable storage;
they can increase when collection grows its work buffers. Observed peaks are
sampled at collector checkpoints and can miss transient peaks. Values remain
16 bytes and frames 64 bytes on this target. Exact allocator/RSS peaks and native
C allocation counts remain unmeasured.

- [Current run](2026-10-25-rust-lua-debug-locals.json)
- [Confirmation](2026-10-25-rust-lua-debug-locals-confirm.json)
- [Prior implementation](2026-10-25-rust-lua-debug-spans.json)
- [Prior confirmation](2026-10-25-rust-lua-debug-spans-confirm.json)

Each JSON has matching TSV/build logs, source hashes and machine/build metadata.
The current reports and official-suite report match the tested source; changes
during measurement invalidate a run. Startup/drop and small-chunk compilation
samples are also retained. Startup opens different library sets between engines.

```sh
python scripts/benchmark_rust_lua.py --output docs/performance/rust-lua-current.json
```

The [validation report](../testing/2026-10-25-rust-lua-debug-locals.md) covers
115 passing tests, callback-order fixes, assignment generation and compatibility
boundaries. Tracebacks, hooks, private native/parser scratch slots, remaining
libraries, binary loading/dumping, broader diagnostics/resources, incremental
collection pauses and later dialects remain unfinished. The crate remains safe
Rust with no normal/build dependencies.
