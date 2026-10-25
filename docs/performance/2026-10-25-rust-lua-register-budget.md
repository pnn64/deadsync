# Lua compiler register-budget measurements

The harness measures 37 execution and eighteen frontend workloads with five
paired samples in alternating engine order, plus an independent confirmation
run. The oracle is vendored official Lua 5.1.5 on Windows x86-64 MSVC, static
CRT and C locale. The machine and shared workspace are not CPU-isolated.

Logical register accounting runs during streaming parsing, retaining flat
16-byte descriptors and per-function constant identities. It adds parsing work
and temporary compiler storage. Physical assignment/return slots grow as
expressions finish; fixed result ranges remain live across collection. The ISA,
VM loop and value/frame layouts are unchanged. These changes improve acceptance
and rejection compatibility; they do not establish a general speedup.

The new `frontend_results_199` case calls a vararg function with integers 1–247,
binds 199 local results, and returns 199. Both engines check that result outside
timing. Thirty complete compile/verify/drop operations run per sample. The
previous Rust backend rejects this source after reserving all destinations
before compiling the arguments. The new entry has 249 register slots; instruction
counts below include all prototypes. Sources and execution checks are outside
timing; compilation, verification and program drop are inside.

| Source | Rust ms | C ms | Paired Rust/C | Rust ms confirmation | C ms confirmation | Paired confirmation |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `frontend_arithmetic_10000` | 9.143 | 1.310 | 7.047 | 9.350 | 1.149 | 7.986 |
| `frontend_call_chain_10000` | 11.500 | 1.895 | 6.105 | 12.505 | 1.999 | 6.162 |
| `frontend_captures_1000` | 10.522 | 3.264 | 3.147 | 9.671 | 3.060 | 3.088 |
| `frontend_captures_10000` | 105.047 | 33.603 | 3.126 | 97.465 | 31.002 | 3.174 |
| `frontend_comment_10000` | 0.009 | 0.029 | 0.298 | 0.008 | 0.025 | 0.312 |
| `frontend_comment_100000` | 0.039 | 0.256 | 0.154 | 0.032 | 0.216 | 0.147 |
| `frontend_comment_1000000` | 0.314 | 2.624 | 0.124 | 0.267 | 2.134 | 0.125 |
| `frontend_constructor_190` | 0.269 | 0.044 | 6.336 | 0.288 | 0.047 | 6.295 |
| `frontend_functions_190` | 1.217 | 0.600 | 1.904 | 1.402 | 0.609 | 2.302 |
| `frontend_index_10000` | 9.893 | 0.861 | 11.500 | 10.117 | 0.884 | 11.050 |
| `frontend_logical_20000` | 30.578 | 575.910 | 0.054 | 29.600 | 564.621 | 0.052 |
| `frontend_method_chain_10000` | 18.725 | 2.907 | 6.547 | 19.513 | 3.063 | 6.420 |
| `frontend_quoted_10000` | 0.037 | 0.064 | 0.567 | 0.029 | 0.050 | 0.576 |
| `frontend_quoted_100000` | 0.255 | 0.572 | 0.456 | 0.222 | 0.457 | 0.488 |
| `frontend_quoted_1000000` | 3.590 | 6.318 | 0.568 | 2.792 | 4.978 | 0.563 |
| `frontend_results_199` | 0.232 | 0.101 | 2.327 | 0.244 | 0.106 | 2.295 |
| `frontend_scopes_1000` | 6.655 | 1.084 | 6.061 | 6.159 | 1.004 | 6.114 |
| `frontend_scopes_10000` | 68.022 | 11.396 | 6.443 | 75.108 | 11.200 | 6.286 |

Most structured-source compilation remains slower than C. Long comments/strings
and this particular long logical chain remain faster. Extra literal-string
hashing for logical constant identity has a measurable cost. Results apply to
these fixtures and include metadata generation and verification absent from the
same form in the reference.

Per-full-compile allocation counters agree across all samples and both new runs.
For unchanged fixtures, the prior milestone supplies deterministic allocation
counts below; its timings are not treated as a paired parent comparison.
Requested bytes count all allocation/reallocation requests, including replaced
capacities. They are not retained or exact peak bytes.

| Source | Parent allocations/reallocations | Current allocations/reallocations | Parent requested bytes | Current requested bytes | Current instructions | Current entry slots |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `frontend_quoted_100000` | 32/22 | 35/22 | 205,946 | 206,170 | 5 | 2 |
| `frontend_scopes_1000` | 13,048/4,146 | 13,052/4,157 | 3,368,274 | 3,630,738 | 9,006 | 7 |
| `frontend_captures_1000` | 25,028/19,114 | 25,032/19,125 | 4,412,551 | 4,675,015 | 9,003 | 3 |
| `frontend_logical_20000` | 40,060/146 | 40,066/160 | 23,820,134 | 25,917,766 | 60,010 | 3 |
| `frontend_results_199` | not accepted | 466/127 | — | 246,106 | 255 | 249 |

Parser heap estimates include accounting storage, which is released before
backend compilation. Exact compiler allocator live/peak bytes and RSS remain
unmeasured. Entry slot counts do not describe the maximum child-function layout.

An auxiliary executable links parent `06abcaa71` and the current engine as two
independent zero-dependency libraries. Before timing, 269 moderate programs
accepted by the parent compare scalar bits, byte strings, nil, booleans and
arity. Newly accepted large-result cases use separate official-oracle tests.

Five warmups precede nine alternating pairs. Compile/drop repeats are 1,000 for
tiny chunks, five for scope/capture cases, 100 for distinct locals and ten for
a 100,000-byte quoted string (`local s='...';return 7`). Execution repeats twenty
times for loops, ten for Fibonacci, fifty for operand reuse and thirty for the
call/table kernel. Expected returns are checked before timing. Direct rustc
optimization level 3, static CRT and final fat LTO differ from Cargo's protocol.

| Auxiliary workload | Paired current/parent |
| --- | ---: |
| `captures_1000` | 0.805 |
| `distinct_190` | 1.044 |
| `execute_call_table` | 1.028 |
| `execute_fib` | 1.015 |
| `execute_for` | 0.965 |
| `execute_operand_reuse` | 1.031 |
| `execute_while` | 1.015 |
| `quoted_100000` | 1.716 |
| `scopes_1000` | 1.176 |
| `tiny` | 1.197 |

This single same-executable comparison measures the accounting cost on several
layouts. Execution changes remain workload-specific; mixed small timing changes
do not establish an overall runtime improvement or regression bound.

| Workload | Parent entry slots | Current entry slots | Parent instructions | Current instructions |
| --- | ---: | ---: | ---: | ---: |
| `tiny` | 2 | 2 | 3 | 3 |
| `scopes_1000` | 7 | 7 | 9,006 | 9,006 |
| `captures_1000` | 3 | 3 | 9,003 | 9,003 |
| `distinct_190` | 191 | 191 | 193 | 193 |
| `quoted_100000` | 2 | 2 | 4 | 4 |
| `execute_for` | 6 | 6 | 11 | 11 |
| `execute_while` | 4 | 4 | 13 | 13 |
| `execute_fib` | 3 | 3 | 25 | 25 |
| `execute_operand_reuse` | 8 | 8 | 18 | 18 |
| `execute_call_table` | 9 | 9 | 21 | 21 |

The current execution baseline remains:

| Workload | Paired Rust/C | Confirmation |
| --- | ---: | ---: |
| `array_table` | 2.252 | 2.165 |
| `byte_concat` | 4.794 | 4.849 |
| `call_table` | 1.630 | 1.603 |
| `capture_limit` | 1.886 | 1.891 |
| `closure_calls` | 3.598 | 3.649 |
| `coroutine_deep_yield` | 2.725 | 2.843 |
| `coroutine_resume` | 2.760 | 2.750 |
| `coroutine_reuse` | 3.401 | 3.193 |
| `coroutine_wrap` | 3.851 | 3.788 |
| `debug_hook` | 2.070 | 2.026 |
| `debug_info` | 1.059 | 1.069 |
| `debug_locals` | 2.646 | 2.496 |
| `debug_traceback` | 1.532 | 1.570 |
| `gc_cycles` | 1.382 | 1.446 |
| `gc_weak_values` | 1.142 | 1.146 |
| `io_tmpfile` | 1.236 | 1.181 |
| `loadfile_text` | 1.123 | 1.119 |
| `native_arg_error` | 0.461 | 0.468 |
| `numeric_for` | 4.155 | 4.369 |
| `numeric_while` | 3.846 | 4.046 |
| `operand_reuse` | 2.094 | 1.884 |
| `pattern_callback` | 2.929 | 2.823 |
| `pattern_find` | 0.830 | 0.812 |
| `pattern_replace` | 2.905 | 3.025 |
| `reader_load_bytes` | 3.063 | 3.097 |
| `reader_load_single` | 2.697 | 3.006 |
| `recursive_fib` | 3.602 | 3.723 |
| `require_cached` | 2.643 | 2.590 |
| `require_preload` | 1.860 | 1.611 |
| `runtime_error` | 0.477 | 0.467 |
| `stack_overflow` | 2.197 | 2.122 |
| `string_format` | 1.151 | 1.100 |
| `syntax_error` | 0.542 | 0.529 |
| `table_sort` | 2.203 | 2.290 |
| `table_sort_callback` | 2.070 | 2.111 |
| `userdata_finalize` | 1.527 | 1.428 |
| `userdata_proxy` | 1.388 | 1.460 |

The call/table workload still allocates during churn. Heap estimates agree
between runs; thirty scripts per sample create 30,000 tables.

| Phase | Live bytes | Reserved bytes | Observed reserved peak | Collections | Reclaimed objects |
| --- | ---: | ---: | ---: | ---: | ---: |
| before collect | 30,707 | 55,099 | 77,659 | 873 | 151,117 |
| after collect | 22,483 | 49,819 | 77,659 | 874 | 151,151 |

Three standard userdata remain live. C reports
30,287 bytes after collection under its
own accounting. Capacity estimates and C accounting are not directly equivalent.

JSON, TSV and build logs retain samples, allocation/heap observations, layouts,
source/reference digests, 66 per-file source hashes and machine/build metadata.
Both main runs have identical source hashes. The auxiliary report retains its
driver, fixture/source hashes and exact commands. Measurements precede the
milestone commit in a dirty shared working tree; hashes identify measured code.

```sh
python scripts/benchmark_rust_lua.py --output docs/performance/rust-lua-register-budget.json
```

See [validation and known remaining limits](../testing/2026-10-25-rust-lua-register-budget.md).
Full resource/debug/scratch parity, remaining libraries, binary chunks, native
modules, incremental GC, exact allocator/RSS peaks, native C allocation counts,
other Rust engines and wider standalone workloads remain open. This is not a
complete-engine performance claim.
