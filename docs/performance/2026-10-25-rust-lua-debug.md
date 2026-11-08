# Lua 5.1 debug metadata measurements

The harness now measures 23 execution workloads and six frontend scaling cases.
`debug_info` performs 1,000 `debug.getinfo(f, "SufL")` queries and calls each
returned function per script invocation. Both engines check the expected sum
of 3,000 before timing. There are five paired samples in alternating engine order,
with ten script invocations per sample for inspection and reader workloads.
Automatic collection remains enabled.

The reference is vendored official Lua 5.1.5 on Windows x86-64 MSVC, static CRT
and C locale. Its inspection workload opens `ALL_SAFE | DEBUG`; other workloads
keep their previous library configuration. The initial attempt used the default
state, which excludes debug, and failed before producing a completed JSON report.
The retained initial/final reports all use the corrected setup.

Source positions were initially held in two additional growing AST vectors.
The final implementation stores start, end and materialization positions beside
each expression in its existing flat arena entry. This removes two separately
allocated buffers without changing the debug results. All 106 tests and
the fourteen official probes were rerun on that implementation.

| Workload, per script invocation | Initial allocations | Final allocations | Initial reallocations | Final reallocations | Initial requested bytes | Final requested bytes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `reader_load_single` | 4,600.0 | 4,200.0 | 2,200.0 | 2,200.0 | 645,200.0 | 645,200.0 |
| `reader_load_bytes` | 3,997.2 | 3,797.2 | 2,700.0 | 2,500.0 | 726,997.2 | 726,997.2 |
| `debug_info` | 5,000.0 | 5,000.0 | 2,000.0 | 2,000.0 | 561,000.0 | 561,000.0 |

Requested bytes remain unchanged on these scripts; fewer allocation calls and
separate buffers are the measured benefit. Timing variation does not establish
an isolated throughput gain. These counters sum Rust `GlobalAlloc` requests across
the complete script, including execution and collection. Direct native C malloc
is excluded. The single-chunk script loads 200 chunks; the one-byte script loads
100 chunks. Compilation metadata still adds work compared with the preceding
reader milestone; this change does not remove that compatibility cost.

| Execution workload | Final paired Rust/reference | Confirmation |
| --- | ---: | ---: |
| `array_table` | 3.233 | 3.209 |
| `byte_concat` | 5.192 | 5.191 |
| `closure_calls` | 3.852 | 3.924 |
| `coroutine_deep_yield` | 2.444 | 2.724 |
| `coroutine_resume` | 2.826 | 2.868 |
| `coroutine_reuse` | 3.221 | 3.244 |
| `coroutine_wrap` | 3.548 | 3.352 |
| `debug_info` | 1.047 | 1.065 |
| `gc_cycles` | 1.278 | 1.274 |
| `gc_weak_values` | 1.177 | 1.213 |
| `numeric_for` | 7.082 | 6.680 |
| `numeric_while` | 6.381 | 6.702 |
| `pattern_callback` | 3.238 | 3.089 |
| `pattern_find` | 0.940 | 0.889 |
| `pattern_replace` | 2.839 | 2.932 |
| `reader_load_bytes` | 2.818 | 2.812 |
| `reader_load_single` | 2.170 | 2.074 |
| `recursive_fib` | 4.130 | 4.034 |
| `string_format` | 1.238 | 1.156 |
| `table_sort` | 2.189 | 2.122 |
| `table_sort_callback` | 2.211 | 2.270 |
| `userdata_finalize` | 1.332 | 1.308 |
| `userdata_proxy` | 1.373 | 1.357 |

Ratios are medians of paired wall times; lower is better. Inspection is close to
the reference on this limited workload. Most execution workloads remain slower,
and these results do not establish an overall engine speed improvement. The
machine is not CPU-isolated; use raw samples and paired ratios when assessing
variation. Execution includes host entry/return and automatic collection.

| Inspection script duration | Final | Confirmation |
| --- | ---: | ---: |
| rust | 1.618 ms | 2.039 ms |
| lua51 | 1.547 ms | 1.875 ms |

The six compilation cases use complete fixed source, with source construction
outside timing and destruction included. Payload sizes are decimal bytes. They
retain the scanner bulk paths and use the same parser as reader loading.

| Compilation payload | Final paired Rust/reference | Confirmation |
| --- | ---: | ---: |
| `frontend_comment_10000` | 0.448 | 0.431 |
| `frontend_comment_100000` | 0.315 | 0.307 |
| `frontend_comment_1000000` | 0.285 | 0.278 |
| `frontend_quoted_10000` | 0.301 | 0.286 |
| `frontend_quoted_100000` | 0.273 | 0.261 |
| `frontend_quoted_1000000` | 0.312 | 0.287 |

| After final collection | Live bytes | Reserved bytes | Observed peak reserved | Collections | Reclaimed objects |
| --- | ---: | ---: | ---: | ---: | ---: |
| `debug_info` | 15,261 | 41,053 | 75,628 | 574 | 102,060 |
| `reader_load_single` | 16,313 | 29,161 | 44,953 | 238 | 40,800 |
| `reader_load_bytes` | 17,399 | 29,639 | 51,370 | 276 | 30,520 |

Confirmation reports identical estimates for these workloads.
Counts cover correctness warmup and timed invocations in persistent VMs.
Estimates include cold prototype names/ranges, active-line sets and reusable
vectors. They are capacity-based, not allocator live/peak measurements or RSS;
the observed peak is sampled at collector checkpoints and can miss transient
peaks. Values remain 16 bytes and frames 64 bytes on this target.

- [Initial metadata implementation](2026-10-25-rust-lua-debug.json)
- [Initial confirmation](2026-10-25-rust-lua-debug-confirm.json)
- [Final expression spans](2026-10-25-rust-lua-debug-spans.json)
- [Final confirmation](2026-10-25-rust-lua-debug-spans-confirm.json)

Each JSON has matching TSV/build logs and source/build/machine metadata. Initial
reports record the earlier separate span vectors; final reports and the official
suite report match the final source hashes. The runner rejects source changes
during measurement. Startup/drop and small-script compilation samples are also
retained; startup opens different library sets between engines.

```sh
python scripts/benchmark_rust_lua.py --output docs/performance/rust-lua-current.json
```

The [validation report](../testing/2026-10-25-rust-lua-debug.md) records new
constructs/closure passes and remaining library/debug stops. Local inspection,
tracebacks, hooks, broader diagnostics/resource compatibility, binary loading,
incremental GC pauses, other Rust engine comparisons and exact allocator/RSS
measurements remain unfinished. The engine remains safe Rust with no normal/build
dependencies.
