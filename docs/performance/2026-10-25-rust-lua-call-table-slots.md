# Lua call/constructor slot measurements

The harness measures 37 execution and seventeen frontend workloads, with five
paired samples in alternating engine order. The oracle is vendored official Lua
5.1.5 on Windows x86-64 MSVC, static CRT and C locale. The machine is not CPU-isolated.

Calls and constructors reuse top anonymous destinations. Calls evaluate their
callee before reserving arguments, one at a time. Methods retain their receiver
before lookup and arguments. Compact literal record keys keep pool insertion
order but materialize after field values; larger keys use the ordinary path.
Aligned fixed/open result copies are omitted. The ISA and VM loop are unchanged.

The new `call_table` script calls an identity function with `{key=i}` inside a
1,000-iteration sum, then returns 500,500. Thirty complete scripts run per sample.
Both engines check the result before timing. Compilation is outside execution
timing; closure/table creation, host entry/return and automatic GC are inside.

| Workload | Rust ms | C ms | Paired Rust/C | Rust ms confirmation | C ms confirmation | Paired confirmation |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `call_table` | 0.433 | 0.275 | 1.605 | 0.417 | 0.262 | 1.656 |

The workload remains slower than C. Its current program has 21 instructions and
nine entry register slots; the parent has 23 instructions and twelve slots.
Rust execution allocation counters agree between runs. Totals cover thirty
scripts, including 30,000 table constructions per sample:

| Sample | Allocation calls | Reallocation calls | Requested bytes |
| --- | ---: | ---: | ---: |
| 0 | 60,001 | 0 | 4,800,016 |
| 1 | 60,000 | 0 | 4,800,000 |
| 2 | 60,000 | 0 | 4,800,000 |
| 3 | 60,000 | 0 | 4,800,000 |
| 4 | 60,000 | 0 | 4,800,000 |

Requested bytes include replacement capacities, rather than retained bytes.
Table storage still allocates during churn; lower call/constructor slot pressure
does not establish lower total allocator traffic than the parent.

Heap estimates agree between runs:

| Phase | Live bytes | Reserved bytes | Observed reserved peak | Collections | Reclaimed objects |
| --- | ---: | ---: | ---: | ---: | ---: |
| before collect | 30,707 | 55,099 | 77,659 | 873 | 151,117 |
| after collect | 22,483 | 49,819 | 77,659 | 874 | 151,151 |

Three standard userdata remain live in both phases. C reports
30,287 bytes after collection through
its own accounting. These are capacity estimates versus C accounting, not exact
allocator live/peak bytes or process RSS.

Three new frontend cases exercise 190 nested `key` constructors, 10,000 chained
fixed-argument calls and 10,000 chained methods. They return 190, 10,000 and
10,000. Sources and correctness executions are outside timing; compilation,
verification and program drop are inside. Constructor samples repeat twenty
times; each chain sample repeats three times. Durations are milliseconds per
full compile/drop:

| Source | Rust ms | C ms | Paired Rust/C | Rust ms confirmation | C ms confirmation | Paired confirmation |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `frontend_arithmetic_10000` | 7.326 | 1.103 | 6.700 | 8.090 | 1.164 | 6.828 |
| `frontend_call_chain_10000` | 10.585 | 2.034 | 5.298 | 11.156 | 1.931 | 5.819 |
| `frontend_captures_1000` | 8.860 | 3.039 | 2.890 | 9.388 | 3.085 | 3.009 |
| `frontend_captures_10000` | 90.141 | 32.405 | 2.740 | 93.524 | 34.133 | 2.843 |
| `frontend_comment_10000` | 0.008 | 0.024 | 0.334 | 0.008 | 0.025 | 0.319 |
| `frontend_comment_100000` | 0.035 | 0.202 | 0.168 | 0.036 | 0.216 | 0.168 |
| `frontend_comment_1000000` | 0.270 | 2.050 | 0.134 | 0.277 | 2.163 | 0.130 |
| `frontend_constructor_190` | 0.273 | 0.049 | 6.375 | 0.319 | 0.050 | 6.425 |
| `frontend_functions_190` | 1.133 | 0.567 | 2.002 | 1.217 | 0.551 | 2.193 |
| `frontend_index_10000` | 10.505 | 1.040 | 10.001 | 10.003 | 1.077 | 9.620 |
| `frontend_logical_20000` | 30.047 | 602.898 | 0.048 | 26.282 | 536.777 | 0.048 |
| `frontend_method_chain_10000` | 18.344 | 3.011 | 6.171 | 18.409 | 2.827 | 6.426 |
| `frontend_quoted_10000` | 0.017 | 0.051 | 0.337 | 0.021 | 0.054 | 0.348 |
| `frontend_quoted_100000` | 0.097 | 0.471 | 0.206 | 0.107 | 0.493 | 0.215 |
| `frontend_quoted_1000000` | 1.570 | 5.290 | 0.296 | 1.636 | 5.240 | 0.309 |
| `frontend_scopes_1000` | 5.600 | 1.004 | 5.570 | 5.827 | 1.031 | 5.630 |
| `frontend_scopes_10000` | 58.162 | 9.753 | 6.017 | 59.865 | 11.216 | 5.560 |

These sources now compile successfully; compilation remains slower than C.
The retained logical-chain fixture remains faster than this reference for its
particular layout. No general frontend or execution-speed claim follows.

Layout records report instructions across all prototypes and entry slots only,
not maximum child-function storage. New frontend allocation counts per full
compile/drop agree across samples and both runs:

| Source | Instructions | Entry slots | Allocation calls | Reallocation calls | Requested bytes |
| --- | ---: | ---: | ---: | ---: | ---: |
| `frontend_constructor_190` | 587 | 192 | 632 | 280 | 383,409 |
| `frontend_call_chain_10000` | 20,014 | 4 | 10,069 | 10,147 | 11,935,750 |
| `frontend_method_chain_10000` | 50,013 | 5 | 40,077 | 10,150 | 14,259,058 |

The two chain cases use bounded entry slots as their length grows. Constructors
retain each unfinished containing table, so their slots grow with actual nesting.
Counters include the AST, name pool, continuations, code, metadata and verification.
Exact compiler allocator live/peak memory remains unmeasured.

An auxiliary executable links parent `241a16adc` and current engines. Before
timing, 269 moderate programs accepted by the parent compare full scalar results,
including bits, byte strings, nil, booleans and arity. Newly accepted deep sources
have independent C-reference tests. Register maps/code are expected to change.

The timing protocol uses five warmups and nine alternating pairs. Compile/drop
repeats are 1,000 for tiny chunks, five for scope cases and 100 for distinct
locals. Compiled execution repeats twenty times for loops, ten for Fibonacci,
fifty for operand reuse and thirty for the call/table kernel. Exact expected
returns are checked before timing. Direct rustc optimization level 3, static CRT
and final fat LTO differ from the Cargo/reference protocol above.

| Auxiliary workload | Paired current/parent |
| --- | ---: |
| `captures_1000` | 0.986 |
| `distinct_190` | 1.022 |
| `execute_call_table` | 0.983 |
| `execute_fib` | 0.935 |
| `execute_for` | 0.989 |
| `execute_operand_reuse` | 1.022 |
| `execute_while` | 0.973 |
| `scopes_1000` | 0.992 |
| `tiny` | 1.015 |

This single comparison shows modest improvements in some kernels and small mixed
changes elsewhere; it does not establish an overall speedup or a bound for other
source layouts. Lower instruction/entry-slot counts are retained independently:

| Workload | Parent entry slots | Current entry slots | Parent instructions | Current instructions |
| --- | ---: | ---: | ---: | ---: |
| `tiny` | 2 | 2 | 3 | 3 |
| `scopes_1000` | 9 | 7 | 10,006 | 9,006 |
| `captures_1000` | 3 | 3 | 9,003 | 9,003 |
| `distinct_190` | 191 | 191 | 193 | 193 |
| `execute_for` | 6 | 6 | 11 | 11 |
| `execute_while` | 4 | 4 | 13 | 13 |
| `execute_fib` | 4 | 3 | 27 | 25 |
| `execute_operand_reuse` | 8 | 8 | 18 | 18 |
| `execute_call_table` | 12 | 9 | 23 | 21 |

The complete current execution baseline remains:

| Workload | Paired Rust/C | Confirmation |
| --- | ---: | ---: |
| `array_table` | 2.297 | 2.227 |
| `byte_concat` | 4.480 | 4.505 |
| `call_table` | 1.605 | 1.656 |
| `capture_limit` | 1.721 | 1.693 |
| `closure_calls` | 3.732 | 3.651 |
| `coroutine_deep_yield` | 2.760 | 2.793 |
| `coroutine_resume` | 2.731 | 2.670 |
| `coroutine_reuse` | 3.371 | 3.501 |
| `coroutine_wrap` | 3.466 | 3.570 |
| `debug_hook` | 1.935 | 2.018 |
| `debug_info` | 1.026 | 1.005 |
| `debug_locals` | 2.766 | 2.565 |
| `debug_traceback` | 1.572 | 1.548 |
| `gc_cycles` | 1.371 | 1.366 |
| `gc_weak_values` | 1.137 | 1.183 |
| `io_tmpfile` | 1.166 | 1.195 |
| `loadfile_text` | 1.051 | 1.091 |
| `native_arg_error` | 0.451 | 0.489 |
| `numeric_for` | 4.192 | 3.972 |
| `numeric_while` | 3.880 | 3.877 |
| `operand_reuse` | 1.859 | 1.862 |
| `pattern_callback` | 2.955 | 3.026 |
| `pattern_find` | 0.844 | 0.902 |
| `pattern_replace` | 2.983 | 3.016 |
| `reader_load_bytes` | 2.932 | 2.779 |
| `reader_load_single` | 2.715 | 2.565 |
| `recursive_fib` | 3.830 | 3.571 |
| `require_cached` | 2.611 | 2.905 |
| `require_preload` | 1.873 | 1.913 |
| `runtime_error` | 0.472 | 0.453 |
| `stack_overflow` | 2.234 | 2.141 |
| `string_format` | 1.216 | 1.264 |
| `syntax_error` | 0.566 | 0.488 |
| `table_sort` | 2.119 | 2.212 |
| `table_sort_callback` | 2.314 | 2.100 |
| `userdata_finalize` | 1.418 | 1.435 |
| `userdata_proxy` | 1.401 | 1.524 |

Most workloads remain slower than C. JSON, TSV and build-log archives preserve
samples, allocation/heap observations, layouts, source/reference digests, per-file
hashes and machine/build metadata. The auxiliary report preserves its driver,
fixture/source hashes and commands. Measurements use a dirty working tree before
the milestone commit; source hashes identify the measured state.

```sh
python scripts/benchmark_rust_lua.py --output docs/performance/rust-lua-call-table-slots.json
```

See [validation and remaining boundaries](../testing/2026-10-25-rust-lua-call-table-slots.md).
Exact reference resource limits, broader debug/scratch parity, remaining libraries,
incremental GC, exact allocator/RSS peaks, native C allocation counts, other Rust
engines and wider standalone workloads remain open. This is not a complete-engine
performance claim.
