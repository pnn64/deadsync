# Lua lexical capture-limit measurements

The harness measures 35 execution workloads and ten frontend cases, with five
paired samples in alternating engine order. The oracle is vendored official Lua
5.1.5 on Windows x86-64 MSVC, static CRT and C locale. The machine is not
CPU-isolated. These are compatibility measurements, not a performance-parity claim.

The new `capture_limit` script constructs unfinished source with 61 outer locals
and a nested function assigning each one. It calls `loadstring` 100 times,
checks the complete line-63 error naming the function at line 2, and returns 100.
Ten complete scripts run per sample. Timing includes source construction,
host entry/return, parsing, lexical capture propagation, diagnostic construction
and protected loader recovery. It excludes initial compilation of the enclosing
script. Automatic GC is enabled. The reported standalone `compile_drop` rows
for this workload compile the enclosing script, rather than the failing source.

| Workload | Rust ms | C ms | Paired Rust/C | Rust ms confirmation | C ms confirmation | Paired confirmation |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `capture_limit` | 9.116 | 4.492 | 1.753 | 9.037 | 5.364 | 1.651 |

The workload remains slower than C. Allocation counters agree between runs;
totals cover ten complete scripts, including 1,000 failed loads per sample:

| Sample | Allocation calls | Reallocation calls | Requested bytes |
| --- | ---: | ---: | ---: |
| 0 | 270,836 | 105,171 | 42,671,722 |
| 1 | 270,866 | 105,160 | 42,664,148 |
| 2 | 270,866 | 105,160 | 42,664,148 |
| 3 | 270,866 | 105,160 | 42,664,148 |
| 4 | 270,866 | 105,160 | 42,664,148 |

Requested bytes include replacement capacities, not retained or peak memory.
The current AST still owns identifier bytes; the lexical checker adds an interned
source-span pool and flat scope/local/capture buffers. Reusing capture indices
avoids a separate allocation for each capture. Consolidating name resolution
with code generation and reducing existing AST allocations remain performance work.

Heap estimates agree between runs:

| Phase | Live bytes | Reserved bytes | Observed reserved peak | Collections | Reclaimed objects |
| --- | ---: | ---: | ---: | ---: | ---: |
| before collect | 61,087 | 68,847 | 72,943 | 10 | 2487 |
| after collect | 27,755 | 39,555 | 72,943 | 11 | 2735 |

Three standard userdata remain live in both phases. The reference reports 33,138
bytes after collection through its own accounting. Rust's capacity estimates
exclude transient parser buffers once failed loads have returned; this is not an
exact allocator-byte, transient-peak or process-RSS comparison. Suspended parser
accounting includes the new buffers while reader jobs remain live.

Two new compilation workloads contain 1,000 and 10,000 sequential scopes, each
declaring two locals and a local function capturing them. They return 7. Sources
are constructed and both engines' returned values checked before timing. Timed
compilation includes verification and dropping each program. The other eight
cases remain unchanged. Frontend durations are milliseconds per full compile/drop:

| Source | Rust ms | C ms | Paired Rust/C | Rust ms confirmation | C ms confirmation | Paired confirmation |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `frontend_captures_1000` | 11.529 | 3.784 | 3.056 | 11.747 | 3.885 | 3.041 |
| `frontend_captures_10000` | 102.447 | 34.254 | 2.864 | 106.412 | 36.868 | 2.919 |
| `frontend_comment_10000` | 0.008 | 0.027 | 0.299 | 0.009 | 0.030 | 0.294 |
| `frontend_comment_100000` | 0.038 | 0.246 | 0.161 | 0.043 | 0.273 | 0.152 |
| `frontend_comment_1000000` | 0.297 | 2.209 | 0.129 | 0.319 | 2.659 | 0.121 |
| `frontend_quoted_10000` | 0.018 | 0.062 | 0.333 | 0.021 | 0.065 | 0.318 |
| `frontend_quoted_100000` | 0.155 | 0.501 | 0.311 | 0.175 | 0.623 | 0.278 |
| `frontend_quoted_1000000` | 1.727 | 5.497 | 0.313 | 1.984 | 6.378 | 0.305 |
| `frontend_scopes_1000` | 5.939 | 1.187 | 4.961 | 6.672 | 1.269 | 5.240 |
| `frontend_scopes_10000` | 68.206 | 12.530 | 5.473 | 62.934 | 11.064 | 5.511 |

Closure-heavy compilation takes about 2.9–3.1 times C's duration here. Increasing
the source tenfold produces roughly ninefold Rust compilation time in these two
sizes; this does not establish asymptotic scaling. Allocation counts for one
compile/drop are stable across samples and both runs:

| Source | Allocation calls | Reallocation calls | Requested bytes |
| --- | ---: | ---: | ---: |
| `frontend_captures_1000` | 26,027 | 20,111 | 5,074,927 |
| `frontend_captures_10000` | 260,027 | 200,149 | 55,859,007 |

These totals include the full compiler, metadata, program verification and drop;
they do not isolate the lexical checker's allocations.

An auxiliary comparison links the parent (`3c8e164b6`) and current engine into
one executable. It checks that every source returns 7, performs five warmups,
then times compile/drop in nine alternating pairs. The tiny case repeats 1,000
times per sample, each 1,000-scope case five times and the 190-distinct-local case
100 times. Direct rustc builds use optimization level 3, static CRT and final fat
LTO; this protocol differs from the Cargo/reference reports above.

| Auxiliary source | Parent median microseconds | Current median microseconds | Paired current/parent |
| --- | ---: | ---: | ---: |
| Tiny chunk | 4.237 | 4.670 | 1.111 |
| 1,000 call-heavy scopes | 4,738.440 | 5,238.960 | 1.092 |
| 1,000 capturing scopes | 8,362.320 | 8,771.680 | 1.038 |
| 190 distinct local names | 96.428 | 112.761 | 1.200 |

The added lexical work has a measured compilation cost in these fixtures.
This single comparison does not isolate each component or establish a bound
for other source layouts. Its driver, raw samples, exact commands and both
source hash maps are archived in the `-compare.rs`, `.tsv` and `.json` files.

The remaining execution ratios retain the complete expanded workload baseline:

| Execution workload | Paired Rust/C | Confirmation |
| --- | ---: | ---: |
| `array_table` | 2.309 | 2.449 |
| `byte_concat` | 4.889 | 4.842 |
| `capture_limit` | 1.753 | 1.651 |
| `closure_calls` | 3.595 | 3.665 |
| `coroutine_deep_yield` | 2.949 | 2.786 |
| `coroutine_resume` | 2.653 | 2.760 |
| `coroutine_reuse` | 3.362 | 3.533 |
| `coroutine_wrap` | 3.842 | 3.604 |
| `debug_hook` | 2.103 | 1.827 |
| `debug_info` | 1.028 | 1.024 |
| `debug_locals` | 2.955 | 2.809 |
| `debug_traceback` | 1.545 | 1.521 |
| `gc_cycles` | 1.297 | 1.345 |
| `gc_weak_values` | 1.142 | 1.132 |
| `io_tmpfile` | 1.161 | 1.192 |
| `loadfile_text` | 1.053 | 1.050 |
| `native_arg_error` | 0.561 | 0.481 |
| `numeric_for` | 4.156 | 4.015 |
| `numeric_while` | 3.991 | 3.697 |
| `pattern_callback` | 3.112 | 3.000 |
| `pattern_find` | 0.902 | 0.957 |
| `pattern_replace` | 3.080 | 3.015 |
| `reader_load_bytes` | 3.102 | 3.181 |
| `reader_load_single` | 2.487 | 2.495 |
| `recursive_fib` | 3.642 | 3.897 |
| `require_cached` | 2.799 | 2.718 |
| `require_preload` | 1.889 | 1.726 |
| `runtime_error` | 0.544 | 0.491 |
| `stack_overflow` | 2.225 | 2.161 |
| `string_format` | 1.149 | 1.048 |
| `syntax_error` | 0.505 | 0.498 |
| `table_sort` | 2.171 | 2.033 |
| `table_sort_callback` | 2.032 | 2.016 |
| `userdata_finalize` | 1.440 | 1.471 |
| `userdata_proxy` | 1.442 | 1.471 |

Most execution cases remain slower than the reference. Differences between
these runs show ordinary measurement variability; they do not establish an
overall speedup over the previous engine milestone.

The JSON and TSV archives retain samples, allocation counters, heap estimates,
source/reference digests, source hashes and machine/build metadata. The matching
`.build.log` files retain build output. Reports are collected from a dirty working
tree before committing the milestone; hashes identify measured sources.

```sh
python scripts/benchmark_rust_lua.py --output docs/performance/rust-lua-captures.json
```

See [correctness and remaining limitations](../testing/2026-10-25-rust-lua-captures.md).
Code generation still uses host recursion; the new deep unfinished/error cases
do not establish successful compilation at arbitrary nesting depths. Incremental
GC, exact allocator peaks, process RSS, native C allocation counts, broader
workloads and comparisons with other Rust engines remain unmeasured here.
