# Lua deferred-operand measurements

The harness measures 39 execution and twenty-two frontend workloads with five
paired samples in alternating engine order, plus an independent confirmation
run. The oracle is vendored official Lua 5.1.5 on Windows x86-64 MSVC, static
CRT and C locale. The machine and shared workspace are not CPU-isolated.

Numeric arithmetic left operands and reference compact constants for arithmetic
and comparisons materialize after the right result. Constant-pool insertion
retains its backend ordering. The right side reuses the working slot; the left
constant needs at most one additional temporary. Named values and pending roots
survive callbacks. The ISA, VM dispatch and value/frame layouts are unchanged.
Lower slot pressure does not establish an overall speedup.

Small arithmetic/unary trees of numeric literals and variable references keep
grouped loads when a conservative bound fits the remaining physical registers.
Parsing computes a capped 32-node hint once in existing AST span padding on
64-bit targets. Code generation uses one lookup without a walk or allocation.
Explicit calls and other expression forms retain deferral. The constant-left
kernel keeps its parent instruction/register layout.

The new `constant_left` execution kernel computes `1+(2*(3+(4*(5+x))))` for
`x=1.25` inside 10,000 loop iterations and returns 57. Fifty complete scripts run
per sample. Compilation and correctness checks are outside timing; host
entry/return and automatic collection are inside.

New frontend cases declare 100 ordinary locals and `local x=1`, then return
190 right-associated powers with numeric `2` or string `'2'` left operands.
Both engines check positive infinity outside timing. The parent rejects both
sources. Thirty compile/verify/drop operations run per sample. Source generation
and correctness execution are outside timing; compilation, verification and
program drop are inside.

| Source | Rust ms | C ms | Paired Rust/C | Rust ms confirmation | C ms confirmation | Paired confirmation |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `frontend_arithmetic_10000` | 10.072 | 1.285 | 7.648 | 10.342 | 1.394 | 7.534 |
| `frontend_call_chain_10000` | 15.309 | 2.472 | 6.042 | 14.750 | 2.445 | 5.918 |
| `frontend_captures_1000` | 11.827 | 3.829 | 3.160 | 11.752 | 3.745 | 3.132 |
| `frontend_captures_10000` | 113.373 | 38.150 | 3.084 | 121.482 | 41.016 | 3.033 |
| `frontend_comment_10000` | 0.010 | 0.030 | 0.349 | 0.013 | 0.033 | 0.386 |
| `frontend_comment_100000` | 0.047 | 0.278 | 0.166 | 0.042 | 0.262 | 0.153 |
| `frontend_comment_1000000` | 0.335 | 2.587 | 0.129 | 0.370 | 2.521 | 0.148 |
| `frontend_compact_pool_150` | 0.498 | 0.148 | 3.372 | 0.485 | 0.143 | 3.500 |
| `frontend_constructor_190` | 0.372 | 0.056 | 6.697 | 0.363 | 0.053 | 6.717 |
| `frontend_functions_190` | 1.505 | 0.673 | 2.200 | 1.502 | 0.693 | 2.190 |
| `frontend_index_10000` | 12.785 | 1.117 | 11.307 | 12.558 | 1.092 | 11.594 |
| `frontend_indexed_targets_150` | 0.326 | 0.091 | 3.624 | 0.326 | 0.090 | 3.620 |
| `frontend_logical_20000` | 34.279 | 656.853 | 0.052 | 35.723 | 658.532 | 0.054 |
| `frontend_method_chain_10000` | 23.836 | 3.749 | 6.325 | 24.227 | 3.546 | 6.817 |
| `frontend_quoted_10000` | 0.035 | 0.065 | 0.554 | 0.035 | 0.066 | 0.527 |
| `frontend_quoted_100000` | 0.261 | 0.592 | 0.440 | 0.260 | 0.586 | 0.427 |
| `frontend_quoted_1000000` | 3.461 | 6.552 | 0.523 | 3.344 | 6.414 | 0.525 |
| `frontend_results_199` | 0.292 | 0.133 | 2.187 | 0.299 | 0.140 | 2.194 |
| `frontend_right_power_190` | 0.231 | 0.072 | 3.366 | 0.242 | 0.066 | 3.656 |
| `frontend_right_string_power_190` | 0.286 | 0.052 | 5.800 | 0.303 | 0.055 | 5.665 |
| `frontend_scopes_1000` | 7.480 | 1.305 | 5.872 | 7.545 | 1.276 | 5.791 |
| `frontend_scopes_10000` | 81.287 | 12.634 | 6.539 | 80.821 | 12.442 | 6.547 |

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
| `frontend_scopes_1000` | 13,052/4,157 | 13,052/4,157 | 3,630,770 | 3,630,770 | 9,006 | 7 |
| `frontend_captures_1000` | 25,032/19,125 | 25,032/19,125 | 4,675,047 | 4,675,047 | 9,003 | 3 |
| `frontend_logical_20000` | 40,066/160 | 40,066/160 | 25,917,802 | 25,917,802 | 60,010 | 3 |
| `frontend_results_199` | 466/127 | 466/127 | 246,138 | 246,138 | 255 | 249 |
| `frontend_indexed_targets_150` | 199/106 | 199/106 | 309,360 | 309,360 | 455 | 152 |
| `frontend_compact_pool_150` | 211/126 | 211/126 | 361,144 | 361,144 | 722 | 152 |
| `frontend_constructor_190` | 637/300 | 637/300 | 424,473 | 424,473 | 587 | 192 |
| `frontend_index_10000` | 20,044/120 | 20,044/120 | 11,230,855 | 11,230,855 | 20,012 | 3 |
| `frontend_right_power_190` | not accepted | 244/109 | — | 338,346 | 483 | 116 |
| `frontend_right_string_power_190` | not accepted | 624/305 | — | 341,510 | 483 | 103 |

Parser heap estimates include accounting and key metadata. Reference descriptors
and pools are released before backend compilation; sparse key/zero metadata stays
with the AST until compilation finishes. Exact compiler allocator live/peak bytes and RSS remain
unmeasured. Entry slot counts do not describe the maximum child-function layout.

An auxiliary executable links parent `551c81c23` and the current engine as two
independent zero-dependency libraries. Before timing, 276 moderate programs
accepted by the parent compare scalar bits, byte strings, nil, booleans and
arity. Newly accepted deep cases use separate official-oracle tests.

Five warmups precede nine alternating pairs. Compile/drop repeats are 1,000 for
tiny chunks, five for scope/capture cases, 100 for distinct locals and ten for
a 100,000-byte quoted string (`local s='...';return 7`). Execution repeats twenty
times for loops, ten for Fibonacci, fifty for operand reuse and thirty for the
call/table, indexed-assignment and constant-left kernels. Expected returns are checked before timing. Direct rustc
optimization level 3, static CRT and final fat LTO differ from Cargo's protocol.

| Auxiliary workload | Paired current/parent |
| --- | ---: |
| `captures_1000` | 1.014 |
| `distinct_190` | 0.992 |
| `execute_call_table` | 1.008 |
| `execute_constant_left` | 1.028 |
| `execute_fib` | 0.981 |
| `execute_for` | 0.961 |
| `execute_indexed_assignment` | 0.926 |
| `execute_operand_reuse` | 1.019 |
| `execute_while` | 1.009 |
| `quoted_100000` | 1.006 |
| `scopes_1000` | 0.988 |
| `tiny` | 0.973 |

This single same-executable comparison measures deferred operands on several
layouts. Execution changes remain workload-specific; mixed small timing changes
do not establish an overall runtime improvement or regression bound.

| Workload | Parent entry slots | Current entry slots | Parent instructions | Current instructions |
| --- | ---: | ---: | ---: | ---: |
| `tiny` | 2 | 2 | 3 | 3 |
| `scopes_1000` | 7 | 7 | 9,006 | 9,006 |
| `captures_1000` | 3 | 3 | 9,003 | 9,003 |
| `distinct_190` | 191 | 191 | 193 | 193 |
| `quoted_100000` | 2 | 2 | 4 | 4 |
| `execute_constant_left` | 11 | 11 | 21 | 21 |
| `execute_indexed_assignment` | 22 | 22 | 76 | 76 |
| `execute_for` | 6 | 6 | 11 | 11 |
| `execute_while` | 4 | 4 | 13 | 13 |
| `execute_fib` | 3 | 3 | 25 | 25 |
| `execute_operand_reuse` | 8 | 8 | 18 | 18 |
| `execute_call_table` | 9 | 9 | 21 | 21 |

The new constant-left kernel measures:

| Source | Rust ms | C ms | Paired Rust/C | Rust ms confirmation | C ms confirmation | Paired confirmation |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `constant_left` | 1.128 | 0.282 | 3.998 | 1.106 | 0.272 | 4.074 |

Its Rust allocator totals per fifty-script sample agree between both runs:

| Sample | Allocations | Reallocations | Requested bytes |
| --- | ---: | ---: | ---: |
| 0 | 0 | 0 | 0 |
| 1 | 0 | 0 | 0 |
| 2 | 0 | 0 | 0 |
| 3 | 0 | 0 | 0 |
| 4 | 0 | 0 | 0 |

The current execution baseline remains:

| Workload | Paired Rust/C | Confirmation |
| --- | ---: | ---: |
| `array_table` | 2.446 | 2.538 |
| `byte_concat` | 4.505 | 4.621 |
| `call_table` | 1.661 | 1.589 |
| `capture_limit` | 1.845 | 1.932 |
| `closure_calls` | 3.651 | 3.389 |
| `constant_left` | 3.998 | 4.074 |
| `coroutine_deep_yield` | 2.765 | 2.659 |
| `coroutine_resume` | 2.662 | 2.697 |
| `coroutine_reuse` | 3.354 | 3.437 |
| `coroutine_wrap` | 3.386 | 3.495 |
| `debug_hook` | 2.223 | 2.106 |
| `debug_info` | 1.043 | 1.049 |
| `debug_locals` | 2.450 | 2.692 |
| `debug_traceback` | 1.540 | 1.547 |
| `gc_cycles` | 1.376 | 1.352 |
| `gc_weak_values` | 1.159 | 1.164 |
| `indexed_assignment` | 2.673 | 2.492 |
| `io_tmpfile` | 1.183 | 1.180 |
| `loadfile_text` | 1.072 | 1.109 |
| `native_arg_error` | 0.452 | 0.467 |
| `numeric_for` | 4.121 | 4.169 |
| `numeric_while` | 3.761 | 3.762 |
| `operand_reuse` | 1.810 | 1.882 |
| `pattern_callback` | 3.301 | 3.150 |
| `pattern_find` | 0.881 | 0.897 |
| `pattern_replace` | 3.243 | 3.091 |
| `reader_load_bytes` | 3.164 | 3.274 |
| `reader_load_single` | 2.942 | 2.881 |
| `recursive_fib` | 3.653 | 3.668 |
| `require_cached` | 2.719 | 2.494 |
| `require_preload` | 1.853 | 1.736 |
| `runtime_error` | 0.484 | 0.502 |
| `stack_overflow` | 2.130 | 2.121 |
| `string_format` | 1.148 | 1.155 |
| `syntax_error` | 0.529 | 0.531 |
| `table_sort` | 2.191 | 2.123 |
| `table_sort_callback` | 2.123 | 2.135 |
| `userdata_finalize` | 1.460 | 1.477 |
| `userdata_proxy` | 1.387 | 1.379 |

Heap estimates for the constant-left workload agree between runs. This kernel
creates no Lua objects inside its loop. Collection can allocate tracing workspaces;
capacity estimates can rise after a collection even without object reclamation.

| Phase | Live bytes | Reserved bytes | Observed reserved peak | Collections | Reclaimed objects |
| --- | ---: | ---: | ---: | ---: | ---: |
| before collect | 18,309 | 24,013 | 24,013 | 0 | 0 |
| after collect | 22,277 | 27,981 | 27,981 | 1 | 0 |

Three standard userdata remain live. C reports
30,127 bytes after collection under its
own accounting. Capacity estimates and C accounting are not directly equivalent.

JSON, TSV and build logs retain samples, allocation/heap observations, layouts,
source/reference digests, 66 per-file source hashes and machine/build metadata.
Both main runs have identical source hashes. The auxiliary report retains its
driver, fixture/source hashes and exact commands. Measurements precede the
milestone commit in a dirty shared working tree; hashes identify measured code.

```sh
python scripts/benchmark_rust_lua.py --output docs/performance/rust-lua-deferred-operands.json
```

See [validation and known remaining limits](../testing/2026-10-25-rust-lua-deferred-operands.md).
Full resource/debug/scratch parity, remaining libraries, binary chunks, native
modules, incremental GC, exact allocator/RSS peaks, native C allocation counts,
other Rust engines and wider standalone workloads remain open. This is not a
complete-engine performance claim.
