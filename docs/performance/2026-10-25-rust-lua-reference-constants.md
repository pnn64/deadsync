# Lua reference-constant measurements

The harness measures 38 execution and twenty frontend workloads with five paired
samples in alternating engine order, plus an independent confirmation run. The
oracle is vendored official Lua 5.1.5 on Windows x86-64 MSVC, static CRT and C
locale. The machine and shared workspace are not CPU-isolated.

Sparse AST metadata retains the reference's compact constant operands for
assignment/constructor keys. Strings borrow existing literal indices; read-only
index chains retain no key metadata. Function-local zero signs preserve reference
pooling independently of backend materializations. Store-key scratch reuse,
the ISA, VM dispatch and value/frame layouts are unchanged. This compatibility work
adds compiler metadata and lookup costs without a general speedup claim.

The new `frontend_compact_pool_150` case compiles scoped boolean/nil and 253-number
list prefixes, then assigns integers 1–150 to numeric keys and returns 150. The
parent rejects that source because its backend pool forces all keys into
persistent registers. Both engines check the result outside timing.
Thirty complete compile/verify/drop operations run per sample. All frontend
sources and correctness executions are outside timing; compilation,
verification and program drop are inside.

| Source | Rust ms | C ms | Paired Rust/C | Rust ms confirmation | C ms confirmation | Paired confirmation |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `frontend_arithmetic_10000` | 8.334 | 1.147 | 7.265 | 8.177 | 1.138 | 6.882 |
| `frontend_call_chain_10000` | 11.903 | 2.046 | 5.819 | 11.879 | 1.972 | 6.119 |
| `frontend_captures_1000` | 11.210 | 3.660 | 3.014 | 9.908 | 3.160 | 3.122 |
| `frontend_captures_10000` | 107.470 | 35.969 | 2.988 | 96.848 | 34.239 | 2.884 |
| `frontend_comment_10000` | 0.010 | 0.028 | 0.351 | 0.011 | 0.033 | 0.334 |
| `frontend_comment_100000` | 0.040 | 0.242 | 0.157 | 0.038 | 0.262 | 0.146 |
| `frontend_comment_1000000` | 0.302 | 2.453 | 0.125 | 0.319 | 2.416 | 0.132 |
| `frontend_compact_pool_150` | 0.401 | 0.118 | 3.392 | 0.483 | 0.135 | 3.650 |
| `frontend_constructor_190` | 0.311 | 0.044 | 7.054 | 0.306 | 0.045 | 6.566 |
| `frontend_functions_190` | 1.368 | 0.662 | 2.140 | 1.294 | 0.579 | 2.200 |
| `frontend_index_10000` | 9.649 | 0.934 | 10.163 | 10.317 | 1.020 | 10.173 |
| `frontend_indexed_targets_150` | 0.273 | 0.080 | 3.414 | 0.307 | 0.079 | 3.630 |
| `frontend_logical_20000` | 30.219 | 581.389 | 0.053 | 28.693 | 577.841 | 0.050 |
| `frontend_method_chain_10000` | 18.699 | 2.965 | 6.293 | 19.472 | 3.053 | 6.344 |
| `frontend_quoted_10000` | 0.033 | 0.060 | 0.553 | 0.039 | 0.067 | 0.571 |
| `frontend_quoted_100000` | 0.252 | 0.569 | 0.436 | 0.270 | 0.593 | 0.456 |
| `frontend_quoted_1000000` | 3.156 | 5.851 | 0.539 | 3.349 | 6.350 | 0.535 |
| `frontend_results_199` | 0.250 | 0.111 | 2.272 | 0.281 | 0.125 | 2.262 |
| `frontend_scopes_1000` | 6.688 | 1.134 | 5.810 | 7.241 | 1.243 | 5.925 |
| `frontend_scopes_10000` | 70.740 | 12.152 | 6.149 | 66.899 | 10.758 | 6.147 |

Most structured-source compilation remains slower than C. Long comments/strings
and this particular long logical chain remain faster. Logical constant identity
hashing remains part of parsing. Results apply to these fixtures and include metadata generation and verification absent from the
same form in the reference.

Per-full-compile allocation counters agree across all samples and both new runs.
For unchanged fixtures, the prior milestone supplies deterministic allocation
counts below; its timings are not treated as a paired parent comparison.
Requested bytes count all allocation/reallocation requests, including replaced
capacities. They are not retained or exact peak bytes.

| Source | Parent allocations/reallocations | Current allocations/reallocations | Parent requested bytes | Current requested bytes | Current instructions | Current entry slots |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `frontend_quoted_100000` | 35/22 | 35/22 | 206,170 | 206,170 | 5 | 2 |
| `frontend_scopes_1000` | 13,052/4,157 | 13,052/4,157 | 3,630,738 | 3,630,770 | 9,006 | 7 |
| `frontend_captures_1000` | 25,032/19,125 | 25,032/19,125 | 4,675,015 | 4,675,047 | 9,003 | 3 |
| `frontend_logical_20000` | 40,066/160 | 40,066/160 | 25,917,770 | 25,917,802 | 60,010 | 3 |
| `frontend_results_199` | 466/127 | 466/127 | 246,106 | 246,138 | 255 | 249 |
| `frontend_indexed_targets_150` | 198/100 | 199/106 | 297,168 | 309,360 | 455 | 152 |
| `frontend_compact_pool_150` | not accepted | 211/126 | — | 361,144 | 722 | 152 |
| `frontend_constructor_190` | 636/294 | 637/300 | 412,281 | 424,473 | 587 | 192 |
| `frontend_index_10000` | 20,043/120 | 20,044/120 | 11,230,759 | 11,230,855 | 20,012 | 3 |

Parser heap estimates include accounting and key metadata. Reference descriptors
and pools are released before backend compilation; sparse key/zero metadata stays
with the AST until compilation finishes. Exact compiler allocator live/peak bytes and RSS remain
unmeasured. Entry slot counts do not describe the maximum child-function layout.

An auxiliary executable links parent `f6d3ae65c` and the current engine as two
independent zero-dependency libraries. Before timing, 270 moderate programs
accepted by the parent compare scalar bits, byte strings, nil, booleans and
arity. Newly accepted and sign-corrected cases use separate official-oracle tests.

Five warmups precede nine alternating pairs. Compile/drop repeats are 1,000 for
tiny chunks, five for scope/capture cases, 100 for distinct locals and ten for
a 100,000-byte quoted string (`local s='...';return 7`). Execution repeats twenty
times for loops, ten for Fibonacci, fifty for operand reuse and thirty for the
call/table and indexed-assignment kernels. Expected returns are checked before timing. Direct rustc
optimization level 3, static CRT and final fat LTO differ from Cargo's protocol.

| Auxiliary workload | Paired current/parent |
| --- | ---: |
| `captures_1000` | 1.004 |
| `distinct_190` | 0.987 |
| `execute_call_table` | 0.991 |
| `execute_fib` | 0.997 |
| `execute_for` | 0.991 |
| `execute_indexed_assignment` | 0.977 |
| `execute_operand_reuse` | 1.006 |
| `execute_while` | 0.980 |
| `quoted_100000` | 1.019 |
| `scopes_1000` | 1.009 |
| `tiny` | 1.025 |

This single same-executable comparison measures the metadata changes on several
layouts. Execution changes remain workload-specific; mixed small timing changes
do not establish an overall runtime improvement or regression bound.

| Workload | Parent entry slots | Current entry slots | Parent instructions | Current instructions |
| --- | ---: | ---: | ---: | ---: |
| `tiny` | 2 | 2 | 3 | 3 |
| `scopes_1000` | 7 | 7 | 9,006 | 9,006 |
| `captures_1000` | 3 | 3 | 9,003 | 9,003 |
| `distinct_190` | 191 | 191 | 193 | 193 |
| `quoted_100000` | 2 | 2 | 4 | 4 |
| `execute_indexed_assignment` | 22 | 22 | 76 | 76 |
| `execute_for` | 6 | 6 | 11 | 11 |
| `execute_while` | 4 | 4 | 13 | 13 |
| `execute_fib` | 3 | 3 | 25 | 25 |
| `execute_operand_reuse` | 8 | 8 | 18 | 18 |
| `execute_call_table` | 9 | 9 | 21 | 21 |

The retained indexed-assignment kernel measures:

| Source | Rust ms | C ms | Paired Rust/C | Rust ms confirmation | C ms confirmation | Paired confirmation |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `indexed_assignment` | 0.953 | 0.414 | 2.425 | 0.687 | 0.259 | 2.547 |

Its Rust allocator totals per thirty-script sample agree between both runs:

| Sample | Allocations | Reallocations | Requested bytes |
| --- | ---: | ---: | ---: |
| 0 | 90 | 182 | 52,928 |
| 1 | 92 | 189 | 55,168 |
| 2 | 90 | 180 | 47,040 |
| 3 | 90 | 180 | 47,040 |
| 4 | 90 | 180 | 47,040 |

The current execution baseline remains:

| Workload | Paired Rust/C | Confirmation |
| --- | ---: | ---: |
| `array_table` | 2.421 | 2.749 |
| `byte_concat` | 4.722 | 4.684 |
| `call_table` | 1.593 | 1.649 |
| `capture_limit` | 1.887 | 1.818 |
| `closure_calls` | 3.700 | 3.811 |
| `coroutine_deep_yield` | 2.629 | 2.584 |
| `coroutine_resume` | 2.640 | 2.613 |
| `coroutine_reuse` | 3.327 | 3.324 |
| `coroutine_wrap` | 3.522 | 3.558 |
| `debug_hook` | 2.180 | 2.066 |
| `debug_info` | 1.109 | 1.115 |
| `debug_locals` | 2.489 | 2.521 |
| `debug_traceback` | 1.546 | 1.563 |
| `gc_cycles` | 1.345 | 1.282 |
| `gc_weak_values` | 1.100 | 1.070 |
| `indexed_assignment` | 2.425 | 2.547 |
| `io_tmpfile` | 1.181 | 1.233 |
| `loadfile_text` | 1.101 | 1.179 |
| `native_arg_error` | 0.457 | 0.468 |
| `numeric_for` | 3.254 | 4.006 |
| `numeric_while` | 3.770 | 3.925 |
| `operand_reuse` | 1.853 | 1.886 |
| `pattern_callback` | 3.068 | 3.073 |
| `pattern_find` | 0.863 | 0.882 |
| `pattern_replace` | 3.218 | 3.008 |
| `reader_load_bytes` | 3.170 | 3.265 |
| `reader_load_single` | 2.895 | 2.920 |
| `recursive_fib` | 3.707 | 3.294 |
| `require_cached` | 2.706 | 2.690 |
| `require_preload` | 1.939 | 1.837 |
| `runtime_error` | 0.486 | 0.483 |
| `stack_overflow` | 2.225 | 2.106 |
| `string_format` | 1.145 | 1.151 |
| `syntax_error` | 0.551 | 0.548 |
| `table_sort` | 2.226 | 2.188 |
| `table_sort_callback` | 2.126 | 2.129 |
| `userdata_finalize` | 1.434 | 1.370 |
| `userdata_proxy` | 1.375 | 1.351 |

The indexed-assignment workload allocates its initial table. Heap estimates agree
between runs; thirty scripts per sample create thirty tables.

| Phase | Live bytes | Reserved bytes | Observed reserved peak | Collections | Reclaimed objects |
| --- | ---: | ---: | ---: | ---: | ---: |
| before collect | 50,840 | 57,368 | 70,808 | 3 | 124 |
| after collect | 24,272 | 33,176 | 70,808 | 4 | 151 |

Three standard userdata remain live. C reports
30,461 bytes after collection under its
own accounting. Capacity estimates and C accounting are not directly equivalent.

JSON, TSV and build logs retain samples, allocation/heap observations, layouts,
source/reference digests, 66 per-file source hashes and machine/build metadata.
Both main runs have identical source hashes. The auxiliary report retains its
driver, fixture/source hashes and exact commands. Measurements precede the
milestone commit in a dirty shared working tree; hashes identify measured code.

```sh
python scripts/benchmark_rust_lua.py --output docs/performance/rust-lua-reference-constants.json
```

See [validation and known remaining limits](../testing/2026-10-25-rust-lua-reference-constants.md).
Full resource/debug/scratch parity, remaining libraries, binary chunks, native
modules, incremental GC, exact allocator/RSS peaks, native C allocation counts,
other Rust engines and wider standalone workloads remain open. This is not a
complete-engine performance claim.
