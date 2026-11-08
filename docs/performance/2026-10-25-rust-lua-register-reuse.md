# Lua register-reuse measurements

The harness measures 36 execution and fourteen frontend workloads, with five
paired samples in alternating engine order. The oracle is vendored official Lua
5.1.5 on Windows x86-64 MSVC, static CRT and C locale. The machine is not CPU-isolated.

Arithmetic/index generation reuses anonymous result slots and allocates right
operand storage after the left finishes. Named destinations retain their prior
values through callbacks and metamethods. The ISA and VM execution loop are
unchanged. This milestone establishes broader source acceptance and reduced
register pressure, rather than an overall execution-speed improvement.

The new `operand_reuse` script runs 10,000 iterations of
`y=(x+x)+(x+x)+(x+x)+(x+x)` with `x=1`, then returns 8. Fifty complete scripts run
per sample. Compilation is outside execution timing; host entry/return and
automatic GC checks are inside. Both engines check the result before timing.

| Workload | Rust ms | C ms | Paired Rust/C | Rust ms confirmation | C ms confirmation | Paired confirmation |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `operand_reuse` | 0.801 | 0.431 | 1.868 | 0.903 | 0.486 | 1.863 |

The workload remains slower than C. Rust reports zero allocation calls,
reallocation calls and requested bytes during every warmed execution sample in
both runs. Its compiled program has eighteen instructions and eight entry
register slots. The parent uses the same instruction count and fourteen slots.

Heap estimates agree between runs:

| Phase | Live bytes | Reserved bytes | Observed reserved peak | Collections | Reclaimed objects |
| --- | ---: | ---: | ---: | ---: | ---: |
| before collect | 17,905 | 23,609 | 23,609 | 0 | 0 |
| after collect | 21,873 | 27,577 | 27,577 | 1 | 0 |

Three standard userdata remain live in both phases. The first explicit full
collection grows reusable collector storage even though no objects are reclaimed.
C reports 30,063 bytes after collection
through its own accounting. These figures do not compare exact allocator live
bytes, transient peaks or process RSS.

Two new frontend workloads compile and drop 10,000-term sources: a sum of a local
`x=1.25` returning 12,500, and a self-indexing table chain returning 1. Sources are
constructed and executed for correctness outside timing. Each new case compiles
three times per sample. Timed work includes verification and dropping the program.
Durations are milliseconds per full compile/drop:

| Source | Rust ms | C ms | Paired Rust/C | Rust ms confirmation | C ms confirmation | Paired confirmation |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `frontend_arithmetic_10000` | 7.436 | 1.114 | 6.677 | 8.786 | 1.359 | 6.445 |
| `frontend_captures_1000` | 9.244 | 3.264 | 2.789 | 10.888 | 3.589 | 2.995 |
| `frontend_captures_10000` | 86.478 | 30.495 | 2.858 | 101.943 | 34.111 | 2.966 |
| `frontend_comment_10000` | 0.008 | 0.024 | 0.353 | 0.009 | 0.027 | 0.331 |
| `frontend_comment_100000` | 0.035 | 0.201 | 0.173 | 0.039 | 0.237 | 0.163 |
| `frontend_comment_1000000` | 0.269 | 2.091 | 0.130 | 0.339 | 2.418 | 0.141 |
| `frontend_functions_190` | 1.350 | 0.534 | 2.530 | 1.449 | 0.640 | 2.262 |
| `frontend_index_10000` | 8.796 | 0.945 | 9.914 | 10.887 | 1.098 | 9.892 |
| `frontend_logical_20000` | 28.182 | 518.123 | 0.054 | 36.140 | 660.341 | 0.055 |
| `frontend_quoted_10000` | 0.017 | 0.051 | 0.330 | 0.020 | 0.057 | 0.357 |
| `frontend_quoted_100000` | 0.097 | 0.463 | 0.209 | 0.117 | 0.538 | 0.218 |
| `frontend_quoted_1000000` | 1.518 | 4.949 | 0.308 | 1.777 | 5.808 | 0.303 |
| `frontend_scopes_1000` | 5.572 | 1.025 | 5.406 | 6.578 | 1.165 | 5.484 |
| `frontend_scopes_10000` | 57.154 | 9.588 | 5.998 | 68.998 | 11.894 | 5.894 |

The new long arithmetic/index cases now compile successfully, with two and four
entry slots respectively, but compilation remains substantially slower than C.
The retained logical-chain case remains faster than this reference in its
particular layout. These results do not establish a general frontend speedup.

Program layout records include total instructions across all prototypes and
register slots for the entry prototype only. Entry slots do not report maximum
child-function storage. The new frontend allocation counts per compile/drop agree
across samples and both runs:

| Source | Instructions | Entry slots | Allocation calls | Reallocation calls | Requested bytes |
| --- | ---: | ---: | ---: | ---: | ---: |
| `frontend_arithmetic_10000` | 10,002 | 2 | 10,029 | 102 | 9,184,797 |
| `frontend_index_10000` | 20,013 | 4 | 20,039 | 109 | 10,182,487 |

Requested bytes include replacement capacities. AST names, literals, frontend
state, code, GC/debug metadata and verification all contribute. Exact compiler
live/peak allocator memory is not measured. Reducing existing allocations and
metadata work remains necessary.

An auxiliary executable links parent `66db3bca7` and current engines. Before
timing, 263 programs compare complete scalar results, including number bits,
booleans, nil, byte strings and arity. These use the checked-in generator and
moderate-depth sources accepted by the parent. New acceptance and corrected
metamethod behavior have separate C-reference tests. Bytecode/register maps are
expected to change and are not asserted identical.

The auxiliary timing protocol performs five warmups and nine alternating pairs.
Compile/drop cases repeat 1,000 times for tiny chunks, five for scope cases and
100 for distinct locals. Execution cases repeat twenty times for loops, ten for
Fibonacci and fifty for operand reuse. Direct rustc builds use optimization level
3, static CRT and final fat LTO, differing from the Cargo/reference protocol.
A confirmation reruns the identical executable without rebuilding; its binary
hash matches the first run.

| Auxiliary workload | Paired current/parent | Confirmation |
| --- | ---: | ---: |
| `captures_1000` | 0.930 | 0.927 |
| `distinct_190` | 1.014 | 1.000 |
| `execute_fib` | 1.025 | 0.992 |
| `execute_for` | 1.004 | 0.982 |
| `execute_operand_reuse` | 0.990 | 1.011 |
| `execute_while` | 1.119 | 0.998 |
| `scopes_1000` | 1.006 | 0.996 |
| `tiny` | 1.004 | 1.022 |

The first auxiliary `while` comparison was about 12% slower; this did not repeat
in the same-binary confirmation, which was close to parity. Both observations
are retained. Capturing-scope compilation is faster in both runs; ordinary
execution ratios are mixed. No overall runtime speedup or stable execution
regression is established by these samples.

The auxiliary layouts show lower entry counts without changed instruction
counts in these execution kernels:

| Kernel | Parent entry slots | Current entry slots | Instructions in each |
| --- | ---: | ---: | ---: |
| `execute_for` | 7 | 6 | 11 |
| `execute_while` | 6 | 4 | 13 |
| `execute_fib` | 4 | 4 | 27 |
| `execute_operand_reuse` | 14 | 8 | 18 |

The full current execution baseline remains:

| Workload | Paired Rust/C | Confirmation |
| --- | ---: | ---: |
| `array_table` | 2.245 | 2.300 |
| `byte_concat` | 4.672 | 4.646 |
| `capture_limit` | 1.693 | 1.757 |
| `closure_calls` | 3.630 | 3.629 |
| `coroutine_deep_yield` | 2.785 | 2.857 |
| `coroutine_resume` | 2.912 | 2.846 |
| `coroutine_reuse` | 3.389 | 3.346 |
| `coroutine_wrap` | 3.592 | 3.480 |
| `debug_hook` | 2.144 | 2.142 |
| `debug_info` | 1.069 | 1.059 |
| `debug_locals` | 2.762 | 2.640 |
| `debug_traceback` | 1.537 | 1.560 |
| `gc_cycles` | 1.443 | 1.359 |
| `gc_weak_values` | 1.184 | 1.159 |
| `io_tmpfile` | 1.205 | 1.216 |
| `loadfile_text` | 1.129 | 1.114 |
| `native_arg_error` | 0.466 | 0.441 |
| `numeric_for` | 3.937 | 3.960 |
| `numeric_while` | 3.768 | 3.747 |
| `operand_reuse` | 1.868 | 1.863 |
| `pattern_callback` | 3.117 | 3.160 |
| `pattern_find` | 0.894 | 1.088 |
| `pattern_replace` | 3.104 | 3.042 |
| `reader_load_bytes` | 3.029 | 3.125 |
| `reader_load_single` | 2.638 | 2.736 |
| `recursive_fib` | 3.727 | 3.950 |
| `require_cached` | 2.465 | 2.506 |
| `require_preload` | 1.911 | 1.867 |
| `runtime_error` | 0.479 | 0.479 |
| `stack_overflow` | 1.941 | 2.206 |
| `string_format` | 1.176 | 1.143 |
| `syntax_error` | 0.546 | 0.523 |
| `table_sort` | 2.197 | 2.106 |
| `table_sort_callback` | 2.119 | 2.150 |
| `userdata_finalize` | 1.463 | 1.467 |
| `userdata_proxy` | 1.439 | 1.485 |

Most workloads remain slower than C. Primary JSON/TSV/build-log archives retain
samples, allocations, before/after collection estimates, layouts, source and
reference digests, file hashes and machine/build metadata. Auxiliary archives
retain the driver, fixture/source/binary hashes, exact commands and both sets of
samples. Measurements were collected from a dirty tree before the milestone
commit; source hashes identify the measured state.

```sh
python scripts/benchmark_rust_lua.py --output docs/performance/rust-lua-register-reuse.json
```

See [correctness and remaining limits](../testing/2026-10-25-rust-lua-register-reuse.md).
Exact resource-limit parity, remaining expression layouts, incremental GC, exact
allocator peaks/RSS, native C allocation counts, other Rust engines and broader
standalone workloads remain open. This is not a complete-engine performance claim.
