# Statement predicate measurements

Statement conditions now retain boolean edges until their branch destination is
known. Direct local tests and comparison branches remove operand copies and
boolean materializers. Uncaptured scope exits omit Close. Shared jumps resolve
once with cycle detection. Capture flags, logical-test indices and graph scratch
storage exist only while compiling; Vm stays 4,440 B, Value 16 B and Frame 64 B.
No runtime opcode or dependency is added. [Correctness evidence](../testing/2026-10-26-rust-lua-predicates.md)
records the changed hook boundaries and remaining failures.

The [input manifest](2026-10-26-rust-lua-predicates-workspace-inputs/manifest.json) pins an isolated
checkout with exact Lua/legacy/C source bytes and fixed workspace build inputs.
Cargo manifest/lock snapshots are retained and referenced by source_hash_overrides.
Every retained Cargo profile builds in a new, initially absent target directory,
so copied source timestamps cannot admit older profile artifacts.
Both complete runs use identical sources and current layouts. The ordinary CLI
build follows all other builders; focused timing makes no changes to binaries.

Each focused run compares the current CLI, the byte-identical e4b9b2ee5 parent
CLI pinned by its committed audit, and official C Lua 5.1.5. Five fixed scripts
perform 5,000,000 loop iterations per fresh process. Nine rotated/reversed
triples yield 135 measured children per run; each engine occupies every position
three times per fixture. Untimed warmups precede each engine/fixture. Every child
checks exact stdout, empty stderr and exit zero. Wall includes process launch;
CPU/cycles include child compilation, startup, execution and cleanup.

Median process wall milliseconds, first / confirmation. Paired ratios are
medians of nine triples, so separate medians need not divide to the ratio.
Smaller is faster:

| Fixture | Parent ms | Current ms | C ms | Current/parent | Current/C |
| --- | ---: | ---: | ---: | ---: | ---: |
| if_local | 362.47 / 401.17 | 230.96 / 233.27 | 99.14 / 99.66 | 0.615 / 0.581 | 2.328 / 2.383 |
| if_comparison | 373.23 / 380.41 | 253.50 / 269.05 | 97.35 / 98.57 | 0.683 / 0.736 | 2.604 / 2.727 |
| if_logical | 423.66 / 415.34 | 271.06 / 250.10 | 149.85 / 125.73 | 0.621 / 0.605 | 1.735 / 2.008 |
| while_comparison | 295.05 / 293.39 | 219.29 / 226.28 | 97.47 / 97.85 | 0.751 / 0.768 | 2.250 / 2.313 |
| repeat_logical | 341.71 / 360.00 | 276.81 / 277.81 | 125.99 / 124.78 | 0.822 / 0.750 | 2.281 / 2.240 |

All five scripts reduce wall time in both runs, by 18–42% against the parent.
Rust still takes about 1.7–2.7 times C's wall time on these fixed scripts.

These selected whole-script results include arithmetic and loop control and
do not establish isolated opcode latency or a universal speedup. C remains a
separate performance reference. The shared host is not CPU-isolated; warm
filesystem caches, scheduling, frequency changes and timer quantization limit
small differences. No allocation instrumentation runs in these CLI processes.

GetProcessTimes user/kernel CPU and QueryProcessCycleTime are collected after
validated child exit, excluding the Python launcher's CPU. First / confirmation:

| Fixture / counter | Current/parent | Current/C |
| --- | ---: | ---: |
| if_local: CPU | 0.571 / 0.591 | 2.600 / 2.667 |
| if_local: Cycles | 0.588 / 0.611 | 2.781 / 2.866 |
| if_comparison: CPU | 0.700 / 0.714 | 3.000 / 3.750 |
| if_comparison: Cycles | 0.658 / 0.678 | 3.175 / 3.314 |
| if_logical: CPU | 0.625 / 0.560 | 2.143 / 2.333 |
| if_logical: Cycles | 0.597 / 0.580 | 2.084 / 2.281 |
| while_comparison: CPU | 0.688 / 0.737 | 2.600 / 2.750 |
| while_comparison: Cycles | 0.730 / 0.744 | 2.727 / 2.790 |
| repeat_logical: CPU | 0.750 / 0.739 | 2.500 / 2.667 |
| repeat_logical: Cycles | 0.778 / 0.731 | 2.551 / 2.572 |

These are process counters, not sampled stacks. The earlier WPR policy denial
was not retried. Physical allocator live/peak heap, peak RSS and sampled CPU
stacks remain unmeasured. Raw data records absolute counters, process order,
source/output checksums, binary pins and both audit hashes.

Two broad runs each retain 81 runtime and 22 frontend fixtures with five paired
samples, throughput, startup/drop, compilation, Rust allocation requests and
before/after collection heap estimates. 74 of 103 preceding layouts
change; the two current layout sets agree. All changed layouts are listed:

| Fixture | Instructions parent → current | Entry registers parent → current |
| --- | ---: | ---: |
| debug_gsub_args | 38 → 37 | 11 → 11 |
| gsub_callback_wide | 28 → 27 | 10 → 10 |
| string_numeric_args | 24 → 23 | 9 → 9 |
| debug_sort_args | 46 → 45 | 13 → 13 |
| debug_foreach_args | 43 → 42 | 12 → 12 |
| concat_chain_16 | 30 → 29 | 22 → 22 |
| concat_chain_128 | 142 → 141 | 134 → 134 |
| pattern_scan_long | 36 → 32 | 4 → 4 |
| pattern_backtrack_long | 28 → 24 | 3 → 3 |
| table_insert_gap | 34 → 29 | 9 → 9 |
| table_concat_wide_error | 34 → 28 | 12 → 12 |
| unpack_4 | 41 → 35 | 9 → 9 |
| unpack_512 | 41 → 35 | 9 → 9 |
| unpack_7999 | 41 → 35 | 9 → 9 |
| byte_7997 | 40 → 35 | 11 → 11 |
| native_int_narrow | 30 → 29 | 9 → 9 |
| native_result_error | 28 → 22 | 11 → 11 |
| numeric_for | 11 → 10 | 6 → 6 |
| numeric_while | 13 → 11 | 4 → 4 |
| operand_reuse | 18 → 17 | 8 → 8 |
| constant_left | 21 → 20 | 11 → 11 |
| os_difftime | 20 → 19 | 9 → 9 |
| os_getenv | 25 → 20 | 9 → 9 |
| os_shell_query | 25 → 20 | 9 → 9 |
| os_time | 48 → 39 | 10 → 10 |
| os_date_format | 20 → 19 | 9 → 9 |
| os_date_table | 27 → 26 | 9 → 9 |
| os_clock | 29 → 24 | 11 → 11 |
| os_rename | 25 → 24 | 9 → 9 |
| call_table | 21 → 20 | 9 → 9 |
| indexed_assignment | 76 → 75 | 22 → 22 |
| array_table | 22 → 20 | 7 → 7 |
| recursive_fib | 25 → 21 | 3 → 3 |
| closure_calls | 18 → 17 | 8 → 8 |
| byte_concat | 14 → 13 | 7 → 7 |
| pattern_find | 18 → 17 | 8 → 8 |
| pattern_replace | 20 → 19 | 9 → 9 |
| pattern_callback | 22 → 21 | 10 → 10 |
| string_format | 20 → 19 | 9 → 9 |
| table_sort | 32 → 30 | 11 → 11 |
| table_sort_callback | 38 → 36 | 12 → 12 |
| coroutine_resume | 39 → 37 | 10 → 10 |
| coroutine_wrap | 33 → 31 | 7 → 7 |
| coroutine_deep_yield | 59 → 53 | 11 → 11 |
| coroutine_reuse | 41 → 33 | 9 → 9 |
| gc_cycles | 21 → 20 | 8 → 8 |
| gc_weak_values | 32 → 31 | 7 → 7 |
| userdata_proxy | 32 → 27 | 10 → 10 |
| userdata_finalize | 36 → 35 | 8 → 8 |
| gc_restart | 46 → 44 | 8 → 8 |
| gc_steps | 51 → 47 | 9 → 9 |
| gc_mark_mutation | 77 → 70 | 10 → 10 |
| locale_numeric | 50 → 45 | 10 → 10 |
| locale_ctype | 58 → 57 | 13 → 13 |
| binary_header_reject | 41 → 35 | 10 → 10 |
| reader_load_single | 29 → 26 | 8 → 8 |
| reader_load_bytes | 42 → 38 | 9 → 9 |
| debug_info | 29 → 28 | 10 → 10 |
| debug_locals | 23 → 22 | 8 → 8 |
| debug_traceback | 34 → 29 | 10 → 10 |
| io_tmpfile | 64 → 62 | 11 → 11 |
| require_cached | 24 → 19 | 8 → 8 |
| require_preload | 41 → 36 | 9 → 9 |
| syntax_error | 27 → 21 | 9 → 9 |
| runtime_error | 30 → 24 | 10 → 10 |
| native_arg_error | 32 → 26 | 10 → 10 |
| capture_limit | 60 → 52 | 11 → 11 |
| function_dump | 50 → 45 | 10 → 10 |
| dump_load | 54 → 53 | 10 → 10 |
| native_redump | 39 → 34 | 11 → 11 |
| frontend_scopes_1000 | 9006 → 8006 | 7 → 7 |
| frontend_scopes_10000 | 90006 → 80006 | 7 → 7 |
| frontend_constructor_190 | 587 → 585 | 192 → 192 |
| frontend_compact_pool_150 | 722 → 720 | 152 → 152 |

Two callback runs each retain 108 samples and twenty fresh-process CPU children.
All 54 warmed Rust samples per run make zero allocation/reallocation requests
and request zero bytes. Five Lua-loop layouts and heap estimates agree across
the current runs. Rust/C paired wall ratios, first / confirmation:

| Fixture | Rust/C |
| --- | ---: |
| add_direct | 0.686 / 0.658 |
| add_lua_loop | 1.917 / 1.925 |
| capture_lua_loop | 2.137 / 2.061 |
| echo_lua_loop | 2.006 / 2.026 |
| reenter_add_lua_loop | 3.161 / 3.334 |
| reenter_echo_lua_loop | 3.123 / 3.177 |

Leaf C comparisons use typed mlua Rust bridges; nested comparisons use a raw C
callback preserving original arguments and lua_pcall(LUA_MULTRET). Direct Rust
entry returns borrowed Values; its C bridge decodes f64. These measurements
include interpreter and public host-entry/return costs. Rust allocator counters
exclude native C malloc. Capacity-based heaps exclude allocator headers,
native bridge storage and physical peaks.

Two cache runs each retain 135 samples and ten CPU children across five fixtures.
Compile, hit and store include source/name/build/locale/integrity checks, bounded
decoding and verification. Stores include compilation/encoding; forced-miss
deletion occurs outside timing. Current runs agree on layouts and compiled-
versus-cached VM heaps. Tiny sources can still compile faster than reload;
no automatic profitability policy is added. Compiler graph scratch and logical
test storage are included in the measured compilation allocation costs.

All fourteen source maps and pinned binaries/audits/build inputs verify,
including the retained initial process timeout and its unchanged successful repeat.
Focused [first](2026-10-26-rust-lua-predicates-predicates.json) and [confirmation](2026-10-26-rust-lua-predicates-predicates-confirm.json),
full [first](2026-10-26-rust-lua-predicates.json) and [confirmation](2026-10-26-rust-lua-predicates-confirm.json), cache
[first](2026-10-26-rust-lua-predicates-cache.json) and [confirmation](2026-10-26-rust-lua-predicates-cache-confirm.json), callback
[first](2026-10-26-rust-lua-predicates-callbacks.json) and [confirmation](2026-10-26-rust-lua-predicates-callbacks-confirm.json)
retain the observations. Complete standalone compatibility, debug/hook parity,
later dialects, native compilation and gameplay integration remain open.

~~~sh
python scripts/benchmark_rust_lua_unary.py --family predicate --parent-binary target/lua-predicate-parent-e4b9b2ee5.exe --parent-audit docs/testing/2026-10-26-rust-lua-boolean-hooks-audit.json --parent-revision e4b9b2ee5af05f6feb03998e9e9d710ebdddc205 --current-audit docs/testing/2026-10-26-rust-lua-predicates-audit.json --output docs/performance/2026-10-26-rust-lua-predicates-predicates.json
python scripts/benchmark_rust_lua.py --output docs/performance/2026-10-26-rust-lua-predicates.json
python scripts/benchmark_rust_lua_cache.py --output docs/performance/2026-10-26-rust-lua-predicates-cache.json --cpu-time
python scripts/benchmark_rust_lua_host_functions.py --output docs/performance/2026-10-26-rust-lua-predicates-callbacks.json --cpu-time
~~~
