# Initial hook measurements

The harness measures 26 execution workloads and eight frontend compilation cases,
with five paired samples in alternating engine order. The reference is vendored
official Lua 5.1.5 on Windows x86-64 MSVC, static CRT and C locale. Automatic
collection remains enabled. The machine is not CPU-isolated.

The new `debug_hook` script installs a Lua line hook, runs a loaded function with
a 100-iteration sum loop, then removes the hook. Its checked result is 5,558:
sum 5,050 plus the sum of reported source lines, 508. Ten complete scripts run
per timing sample. Timing includes text loading/compilation of the small function,
hook setup/removal, Lua calls, callback dispatch, the loop and host entry/return.
This is not an isolated hook-dispatch microbenchmark. The reference opens
`ALL_SAFE | DEBUG` for the four `debug_*` workloads only.

| Hook script | Current | Confirmation |
| --- | ---: | ---: |
| Rust median | 0.074 ms | 0.073 ms |
| Reference median | 0.035 ms | 0.032 ms |
| Median paired Rust/reference | 2.129 | 2.121 |

Ten-script Rust samples record allocation-call totals of 440, 440, 440, 446 and
440, and reallocation totals of 314, 306, 300, 323 and 300 in both runs. Requested
bytes range from 77,460 to 94,824 per sample. These include compilation and closure
creation on each invocation; they do not isolate allocations inside hook dispatch.

The hook workload reports the same heap estimates in both runs. Before full
collection: 33,489 estimated live bytes and 44,297 reserved bytes. After collection:
15,949 live and 28,933 reserved bytes, two cumulative collections and 206 reclaimed
objects. The observed reserved peak is 63,093 bytes. The reference reports 32,209
bytes after collection through its own accounting; the methods differ.

Hook controls use 48 bytes per thread. Callback/event phases use the existing
64-byte frame layout, and values remain 16 bytes. A flat per-prototype line map
identifies observable instruction steps; zero entries omit compiler copies and
combined branch work. Its storage is included in heap estimates. Comparison
materialization stays separate when instruction hooks are enabled, preserving
callback mutations; numeric execution fuses the steps when they are disabled.

Single-local returns retain their register, and relocatable single assignments
avoid an extra copy. Disabled-hook checks precede the hook-control paths. The
following ratios compare the previous milestone with the current one. They show
mixed results: loop ratios improve, while recursive/coroutine calls retain a cost
requiring further profiling. These runs do not isolate the cause of each change.

| Workload | Prior | Prior confirmation | Current | Confirmation |
| --- | ---: | ---: | ---: | ---: |
| `numeric_for` | 5.220 | 5.326 | 4.250 | 4.191 |
| `numeric_while` | 4.337 | 4.256 | 3.914 | 3.899 |
| `recursive_fib` | 3.641 | 3.577 | 3.890 | 3.944 |
| `coroutine_resume` | 2.550 | 2.508 | 2.804 | 2.797 |

| Execution workload | Current paired Rust/reference | Confirmation |
| --- | ---: | ---: |
| `array_table` | 2.350 | 2.311 |
| `byte_concat` | 5.411 | 5.210 |
| `closure_calls` | 3.609 | 3.682 |
| `coroutine_deep_yield` | 2.696 | 2.819 |
| `coroutine_resume` | 2.804 | 2.797 |
| `coroutine_reuse` | 3.614 | 3.406 |
| `coroutine_wrap` | 3.647 | 3.715 |
| `debug_hook` | 2.129 | 2.121 |
| `debug_info` | 0.990 | 0.995 |
| `debug_locals` | 2.666 | 2.672 |
| `debug_traceback` | 1.281 | 1.291 |
| `gc_cycles` | 1.201 | 1.206 |
| `gc_weak_values` | 1.032 | 1.054 |
| `numeric_for` | 4.250 | 4.191 |
| `numeric_while` | 3.914 | 3.899 |
| `pattern_callback` | 2.948 | 2.929 |
| `pattern_find` | 0.883 | 0.877 |
| `pattern_replace` | 2.948 | 2.965 |
| `reader_load_bytes` | 2.721 | 2.828 |
| `reader_load_single` | 2.315 | 2.303 |
| `recursive_fib` | 3.890 | 3.944 |
| `string_format` | 1.124 | 1.122 |
| `table_sort` | 2.118 | 2.172 |
| `table_sort_callback` | 2.041 | 2.071 |
| `userdata_finalize` | 1.325 | 1.309 |
| `userdata_proxy` | 1.281 | 1.255 |

Lower ratios are better. Most workloads remain slower than reference Lua, and
the added hook support does not establish overall performance parity.

Compilation includes destruction. Source generation and correctness execution
are outside timing. Comment/quoted-string cases cover 10 KB, 100 KB and 1 MB;
dense-source cases contain 1,000/10,000 short scopes with locals and nested calls.

| Compilation workload | Current paired Rust/reference | Confirmation |
| --- | ---: | ---: |
| `frontend_comment_10000` | 0.394 | 0.477 |
| `frontend_comment_100000` | 0.293 | 0.314 |
| `frontend_comment_1000000` | 0.286 | 0.279 |
| `frontend_quoted_10000` | 0.315 | 0.322 |
| `frontend_quoted_100000` | 0.288 | 0.289 |
| `frontend_quoted_1000000` | 0.313 | 0.319 |
| `frontend_scopes_1000` | 4.629 | 4.590 |
| `frontend_scopes_10000` | 4.881 | 4.806 |

Allocation counters sum Rust `GlobalAlloc` allocation/reallocation requests over
all iterations in a sample. They exclude direct native C malloc and cannot compare
total allocation counts between engines. Heap figures use capacity estimates;
observed peaks are sampled at GC checkpoints and can miss transient peaks. These
are not RSS, exact live allocator bytes or exact peaks. Startup opens different
library sets between engines.

- [Current run](2026-10-25-rust-lua-hooks.json)
- [Confirmation](2026-10-25-rust-lua-hooks-confirm.json)
- [Previous milestone](2026-10-25-rust-lua-traceback.md)

Matching TSV/build logs retain samples, counters and machine/build metadata. Both
runs and the official-suite report match the final source hashes. Git revision
and dirty state describe measurement before the milestone commit. Startup/drop
and small-chunk compilation samples remain in the raw data.

```sh
python scripts/benchmark_rust_lua.py --output docs/performance/rust-lua-current.json
```

The [validation report](../testing/2026-10-25-rust-lua-hooks.md) records 148 passing
tests and eight of fifteen passing unchanged official probes. Full instruction-event
parity, exact PUC scratch-slot footprints, complete diagnostics, remaining libraries,
binary chunks, incremental collection pauses and later dialects remain unfinished.
The engine remains safe Rust with no normal/build dependencies.
