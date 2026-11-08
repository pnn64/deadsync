# Flat Lua compiler measurements

The harness measures 35 execution and twelve frontend workloads, with five paired
samples in alternating engine order. The reference is vendored official Lua 5.1.5
on Windows x86-64 MSVC, static CRT and C locale. The machine is not CPU-isolated.

Code generation uses a flat task vector for statements, expressions, calls,
tables, scopes and nested functions, plus iterative ancestor capture propagation.
Each task is bounded to 64 bytes on 64-bit targets. The compiler no longer grows
the host call stack with AST depth or rejects expressions at its former private
120-level recursive guard. Parser syntax limits still apply. The VM execution
loop and bytecode format are unchanged.

Two new frontend cases exercise previously failing compiler layouts:

- `frontend_functions_190` has 190 nested local functions, propagates one captured
  ancestor variable through them, calls the nested functions and returns 8.
- `frontend_logical_20000` has 20,000 left-associated `and` terms calling a counting
  closure, and returns 20,000.

Both engines execute each source and check its result before timing. Source
construction and execution are outside frontend timing; full compile, verification
and drop are inside. The nested-function case repeats twenty times per sample;
the logical-chain case repeats three times. Durations are milliseconds per full
compile/drop:

| Source | Rust ms | C ms | Paired Rust/C | Rust ms confirmation | C ms confirmation | Paired confirmation |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `frontend_captures_1000` | 12.549 | 4.061 | 3.100 | 9.751 | 3.055 | 3.170 |
| `frontend_captures_10000` | 129.444 | 42.213 | 3.095 | 101.084 | 32.850 | 3.081 |
| `frontend_comment_10000` | 0.011 | 0.031 | 0.354 | 0.009 | 0.024 | 0.357 |
| `frontend_comment_100000` | 0.043 | 0.277 | 0.160 | 0.030 | 0.210 | 0.140 |
| `frontend_comment_1000000` | 0.327 | 2.634 | 0.124 | 0.265 | 2.260 | 0.117 |
| `frontend_functions_190` | 1.648 | 0.715 | 2.356 | 1.302 | 0.573 | 2.273 |
| `frontend_logical_20000` | 36.450 | 673.832 | 0.054 | 29.886 | 550.383 | 0.054 |
| `frontend_quoted_10000` | 0.022 | 0.063 | 0.351 | 0.016 | 0.048 | 0.337 |
| `frontend_quoted_100000` | 0.128 | 0.597 | 0.216 | 0.098 | 0.453 | 0.218 |
| `frontend_quoted_1000000` | 2.039 | 6.378 | 0.321 | 1.559 | 4.782 | 0.326 |
| `frontend_scopes_1000` | 7.445 | 1.323 | 5.626 | 5.738 | 0.981 | 5.754 |
| `frontend_scopes_10000` | 77.326 | 13.389 | 5.775 | 59.251 | 10.110 | 5.797 |

The long logical chain compiles faster than this C reference in this fixture;
the nested-function and scope/closure-heavy fixtures remain slower. These cases
do not establish performance across other expression shapes. In particular,
wide arithmetic trees still expose excess temporary-register pressure and need
separate allocation work.

The new cases' allocation counts per compile/drop agree across samples and both
runs. Requested bytes include replacement capacities, rather than retained bytes:

| Source | Allocation calls | Reallocation calls | Requested bytes |
| --- | ---: | ---: | ---: |
| `frontend_functions_190` | 3,843 | 3,318 | 1,182,379 |
| `frontend_logical_20000` | 40,061 | 154 | 27,366,974 |

The AST, name pool, continuation vector, function state, generated metadata and
verification all contribute. Standalone compilation has no retained VM heap
phase; exact compiler live/peak allocator memory and process RSS remain unmeasured.
The execution reports retain before/after-collection heap estimates for all 35
runtime cases. Transient compiler task buffers are released before host execution
returns, so those later estimates do not capture their peaks.

An auxiliary executable links the parent (`7c82297c7`) and current compiler. Before
timing, 261 complete `Program` debug representations match: instructions,
constants, register maps/counts, upvalue descriptors, prototype order, source
lines, locals, calls and other debug metadata. These fixtures comprise the 256
generated expression cases plus nested functions, blocks, shadowed captures and
two short-circuit chains within the parent's supported depth.

The timing section checks that each source returns 7, then performs five warmups
and nine alternating compile/drop pairs. Tiny chunks repeat 1,000 times per
sample, each scope case five times and the distinct-local case 100 times. Direct
rustc builds use optimization level 3, static CRT and final fat LTO, differing
from the Cargo/reference protocol above.

| Auxiliary source | Parent median microseconds | Current median microseconds | Paired current/parent |
| --- | ---: | ---: | ---: |
| `captures_1000` | 10,880.680 | 11,437.240 | 1.042 |
| `distinct_190` | 134.037 | 133.347 | 0.992 |
| `scopes_1000` | 6,610.680 | 7,165.180 | 1.084 |
| `tiny` | 5.630 | 6.076 | 1.079 |

Three ordinary-source comparisons show additional compilation work
for flat scheduling; the distinct-name fixture is close to parity. This single
comparison is not an overhead bound for other source layouts. The driver,
samples, commands and both source hash maps are archived in the `-compare.rs`,
`.tsv` and `.json` files. Future compiler optimization can use this baseline while
retaining the new depth behavior and metadata checks.

The complete runtime execution baseline remains:

| Execution workload | Paired Rust/C | Confirmation |
| --- | ---: | ---: |
| `array_table` | 2.129 | 2.102 |
| `byte_concat` | 4.988 | 4.943 |
| `capture_limit` | 1.652 | 1.639 |
| `closure_calls` | 3.597 | 3.701 |
| `coroutine_deep_yield` | 2.730 | 2.684 |
| `coroutine_resume` | 2.747 | 2.729 |
| `coroutine_reuse` | 3.380 | 3.560 |
| `coroutine_wrap` | 3.738 | 3.559 |
| `debug_hook` | 2.295 | 1.972 |
| `debug_info` | 1.086 | 1.082 |
| `debug_locals` | 2.893 | 2.849 |
| `debug_traceback` | 1.674 | 1.663 |
| `gc_cycles` | 1.361 | 1.329 |
| `gc_weak_values` | 1.096 | 1.071 |
| `io_tmpfile` | 1.179 | 1.135 |
| `loadfile_text` | 1.074 | 1.091 |
| `native_arg_error` | 0.429 | 0.446 |
| `numeric_for` | 4.194 | 4.286 |
| `numeric_while` | 3.758 | 3.959 |
| `pattern_callback` | 3.082 | 2.932 |
| `pattern_find` | 0.868 | 0.849 |
| `pattern_replace` | 2.902 | 2.888 |
| `reader_load_bytes` | 3.188 | 3.327 |
| `reader_load_single` | 2.637 | 2.662 |
| `recursive_fib` | 3.796 | 4.029 |
| `require_cached` | 2.781 | 2.778 |
| `require_preload` | 1.772 | 1.738 |
| `runtime_error` | 0.422 | 0.420 |
| `stack_overflow` | 2.119 | 2.074 |
| `string_format` | 1.154 | 1.168 |
| `syntax_error` | 0.506 | 0.477 |
| `table_sort` | 2.111 | 2.169 |
| `table_sort_callback` | 2.080 | 2.106 |
| `userdata_finalize` | 1.469 | 1.472 |
| `userdata_proxy` | 1.431 | 1.391 |

Most execution workloads remain slower than C. Unchanged bytecode for the
compared fixtures and ordinary timing variability do not establish an overall
runtime speedup. Several runtime workloads include dynamic compilation; their
timings and allocation counters include the new scheduling work.

Both primary JSON reports preserve source/reference digests and per-file hashes,
machine/build metadata, all raw samples, heap estimates and allocation counters.
The TSV and build-log companions retain harness/build output. Measurements were
collected from a dirty tree before the milestone commit; source hashes identify
the exact inputs. The auxiliary driver includes the checked-in test generator.

```sh
python scripts/benchmark_rust_lua.py --output docs/performance/rust-lua-flat-codegen.json
```

See [validation and remaining compatibility work](../testing/2026-10-25-rust-lua-flat-codegen.md).
No complete-engine performance claim is made. Exact allocator peaks, process RSS,
native C allocation counts, incremental GC, other Rust engines and broader
standalone workloads remain unmeasured here.
