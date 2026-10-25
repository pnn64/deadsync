# Lua indexed-assignment measurements

The harness measures 38 execution and nineteen frontend workloads with five
paired samples in alternating engine order, plus an independent confirmation
run. The oracle is vendored official Lua 5.1.5 on Windows x86-64 MSVC, static
CRT and C locale. The machine and shared workspace are not CPU-isolated.

Local table/key references reserve no unused slots. Compact literal assignment
keys retain their constant-pool insertion order but materialize in one reusable
scratch slot before each reverse-ordered store. Noncompact keys retain ordinary
storage. Logical parser budgeting is unchanged. The ISA, VM loop and value/frame
layouts are unchanged. Fewer slots do not establish a general speedup.

The new execution case assigns sixteen numeric fields in one statement inside
a 1,000-iteration loop and returns 1,015. Thirty complete scripts run per sample.
Source preparation, compilation and expected-result checks are outside timing;
host entry/return, table creation and automatic collection are inside. The new
`frontend_indexed_targets_150` case assigns integers 1–150 to literal numeric
keys and returns 150. Thirty compile/verify/drop operations run per sample.
The parent rejects that large source. All frontend sources and executions are
outside timing; compilation, verification and program drop are inside.

| Source | Rust ms | C ms | Paired Rust/C | Rust ms confirmation | C ms confirmation | Paired confirmation |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `frontend_arithmetic_10000` | 7.924 | 1.133 | 6.995 | 9.895 | 1.387 | 7.177 |
| `frontend_call_chain_10000` | 12.504 | 2.381 | 5.182 | 13.361 | 2.322 | 5.759 |
| `frontend_captures_1000` | 11.310 | 3.736 | 3.003 | 11.425 | 3.790 | 3.089 |
| `frontend_captures_10000` | 110.864 | 36.236 | 3.118 | 106.345 | 30.761 | 3.095 |
| `frontend_comment_10000` | 0.010 | 0.029 | 0.337 | 0.011 | 0.030 | 0.345 |
| `frontend_comment_100000` | 0.037 | 0.260 | 0.144 | 0.037 | 0.260 | 0.144 |
| `frontend_comment_1000000` | 0.312 | 2.701 | 0.115 | 0.339 | 2.579 | 0.131 |
| `frontend_constructor_190` | 0.303 | 0.052 | 6.232 | 0.330 | 0.050 | 6.546 |
| `frontend_functions_190` | 1.349 | 0.624 | 2.163 | 1.438 | 0.710 | 2.082 |
| `frontend_index_10000` | 9.606 | 0.862 | 10.434 | 11.766 | 1.111 | 10.973 |
| `frontend_indexed_targets_150` | 0.266 | 0.073 | 3.725 | 0.318 | 0.081 | 3.604 |
| `frontend_logical_20000` | 33.532 | 562.713 | 0.057 | 27.666 | 536.182 | 0.052 |
| `frontend_method_chain_10000` | 19.279 | 2.851 | 6.652 | 21.477 | 3.560 | 5.828 |
| `frontend_quoted_10000` | 0.035 | 0.063 | 0.548 | 0.037 | 0.064 | 0.556 |
| `frontend_quoted_100000` | 0.276 | 0.609 | 0.450 | 0.271 | 0.618 | 0.438 |
| `frontend_quoted_1000000` | 3.081 | 6.517 | 0.473 | 2.801 | 5.925 | 0.507 |
| `frontend_results_199` | 0.241 | 0.106 | 2.246 | 0.280 | 0.125 | 2.281 |
| `frontend_scopes_1000` | 7.293 | 1.285 | 5.826 | 7.342 | 1.308 | 5.587 |
| `frontend_scopes_10000` | 77.446 | 12.653 | 6.011 | 64.177 | 12.206 | 5.335 |

Most structured-source compilation remains slower than C. Long comments/strings
and this particular long logical chain remain faster. Logical constant identity
accounting retains its previous cost. Results apply to
these fixtures and include metadata generation and verification absent from the
same form in the reference.

Per-full-compile allocation counters agree across all samples and both new runs.
For unchanged fixtures, the prior milestone supplies deterministic allocation
counts below; its timings are not treated as a paired parent comparison.
Requested bytes count all allocation/reallocation requests, including replaced
capacities. They are not retained or exact peak bytes.

| Source | Parent allocations/reallocations | Current allocations/reallocations | Parent requested bytes | Current requested bytes | Current instructions | Current entry slots |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `frontend_quoted_100000` | 35/22 | 35/22 | 206,170 | 206,170 | 5 | 2 |
| `frontend_scopes_1000` | 13,052/4,157 | 13,052/4,157 | 3,630,738 | 3,630,738 | 9,006 | 7 |
| `frontend_captures_1000` | 25,032/19,125 | 25,032/19,125 | 4,675,015 | 4,675,015 | 9,003 | 3 |
| `frontend_logical_20000` | 40,066/160 | 40,066/160 | 25,917,770 | 25,917,770 | 60,010 | 3 |
| `frontend_results_199` | 466/127 | 466/127 | 246,106 | 246,106 | 255 | 249 |
| `frontend_indexed_targets_150` | 198/100 | 198/100 | 297,168 | 297,168 | 455 | 152 |

Parser heap estimates include accounting storage, which is released before
backend compilation. Exact compiler allocator live/peak bytes and RSS remain
unmeasured. Entry slot counts do not describe the maximum child-function layout.

An auxiliary executable links parent `35aa1c18c` and the current engine as two
independent zero-dependency libraries. Before timing, 270 moderate programs
accepted by the parent compare scalar bits, byte strings, nil, booleans and
arity. Newly accepted large-target cases use separate official-oracle tests.

Five warmups precede nine alternating pairs. Compile/drop repeats are 1,000 for
tiny chunks, five for scope/capture cases, 100 for distinct locals and ten for
a 100,000-byte quoted string (`local s='...';return 7`). Execution repeats twenty
times for loops, ten for Fibonacci, fifty for operand reuse and thirty for the
call/table and indexed-assignment kernels. Expected returns are checked before timing. Direct rustc
optimization level 3, static CRT and final fat LTO differ from Cargo's protocol.

| Auxiliary workload | Paired current/parent |
| --- | ---: |
| `captures_1000` | 0.990 |
| `distinct_190` | 1.035 |
| `execute_call_table` | 1.003 |
| `execute_fib` | 1.016 |
| `execute_for` | 1.013 |
| `execute_indexed_assignment` | 1.023 |
| `execute_operand_reuse` | 0.963 |
| `execute_while` | 1.003 |
| `quoted_100000` | 0.998 |
| `scopes_1000` | 1.007 |
| `tiny` | 0.950 |

This single same-executable comparison measures these storage changes on several
layouts. Execution changes remain workload-specific; mixed small timing changes
do not establish an overall runtime improvement or regression bound.

| Workload | Parent entry slots | Current entry slots | Parent instructions | Current instructions |
| --- | ---: | ---: | ---: | ---: |
| `tiny` | 2 | 2 | 3 | 3 |
| `scopes_1000` | 7 | 7 | 9,006 | 9,006 |
| `captures_1000` | 3 | 3 | 9,003 | 9,003 |
| `distinct_190` | 191 | 191 | 193 | 193 |
| `quoted_100000` | 2 | 2 | 4 | 4 |
| `execute_indexed_assignment` | 53 | 22 | 76 | 76 |
| `execute_for` | 6 | 6 | 11 | 11 |
| `execute_while` | 4 | 4 | 13 | 13 |
| `execute_fib` | 3 | 3 | 25 | 25 |
| `execute_operand_reuse` | 8 | 8 | 18 | 18 |
| `execute_call_table` | 9 | 9 | 21 | 21 |

The new execution kernel measures:

| Source | Rust ms | C ms | Paired Rust/C | Rust ms confirmation | C ms confirmation | Paired confirmation |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `indexed_assignment` | 0.798 | 0.311 | 2.639 | 0.715 | 0.292 | 2.409 |

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
| `array_table` | 2.331 | 2.417 |
| `byte_concat` | 4.578 | 4.766 |
| `call_table` | 1.740 | 1.657 |
| `capture_limit` | 1.825 | 1.806 |
| `closure_calls` | 3.598 | 3.563 |
| `coroutine_deep_yield` | 2.682 | 2.652 |
| `coroutine_resume` | 2.767 | 2.678 |
| `coroutine_reuse` | 3.374 | 3.332 |
| `coroutine_wrap` | 3.911 | 3.401 |
| `debug_hook` | 2.171 | 2.370 |
| `debug_info` | 0.981 | 1.086 |
| `debug_locals` | 2.377 | 2.487 |
| `debug_traceback` | 1.472 | 1.478 |
| `gc_cycles` | 1.345 | 1.362 |
| `gc_weak_values` | 1.176 | 1.151 |
| `indexed_assignment` | 2.639 | 2.409 |
| `io_tmpfile` | 1.077 | 1.206 |
| `loadfile_text` | 1.190 | 1.172 |
| `native_arg_error` | 0.455 | 0.454 |
| `numeric_for` | 4.184 | 3.851 |
| `numeric_while` | 4.163 | 3.904 |
| `operand_reuse` | 1.829 | 1.767 |
| `pattern_callback` | 3.542 | 3.034 |
| `pattern_find` | 0.828 | 0.882 |
| `pattern_replace` | 3.300 | 3.199 |
| `reader_load_bytes` | 3.112 | 3.074 |
| `reader_load_single` | 2.788 | 2.869 |
| `recursive_fib` | 3.677 | 3.547 |
| `require_cached` | 2.766 | 2.619 |
| `require_preload` | 1.727 | 1.802 |
| `runtime_error` | 0.438 | 0.495 |
| `stack_overflow` | 2.041 | 1.933 |
| `string_format` | 1.121 | 1.192 |
| `syntax_error` | 0.530 | 0.515 |
| `table_sort` | 2.174 | 2.173 |
| `table_sort_callback` | 2.142 | 2.165 |
| `userdata_finalize` | 1.503 | 1.402 |
| `userdata_proxy` | 1.413 | 1.427 |

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
python scripts/benchmark_rust_lua.py --output docs/performance/rust-lua-index-targets.json
```

See [validation and known remaining limits](../testing/2026-10-25-rust-lua-index-targets.md).
Full resource/debug/scratch parity, remaining libraries, binary chunks, native
modules, incremental GC, exact allocator/RSS peaks, native C allocation counts,
other Rust engines and wider standalone workloads remain open. This is not a
complete-engine performance claim.
