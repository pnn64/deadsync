# Register/constant operand measurements

The eight-byte ISA now reads constants directly on arithmetic, comparison and
table operations. Hidden operand loads disappear from native imports and
eligible source instructions. Nested left constants remain in instruction
fields; single nonlocal stores read a local RHS directly. Logical debug bounds
and RK eligibility are computed by the parser. Their compile/allocation cost
is included below. The [correctness report](../testing/2026-10-26-rust-lua-rk-windows.md) records the
hook-window changes, schema-2 cache format and remaining stack-view failures.
Vm remains 4,440 B, Value 16 B and Frame 64 B, with zero normal/build dependencies.

The [snapshot manifest](2026-10-26-rust-lua-rk-windows-workspace-inputs/manifest.json) pins a checkout
based on b56425ea7 with exact Lua/legacy/C bytes and frozen workspace inputs.
Every retained Cargo profile starts in a new target directory. Cargo manifest
and lock snapshots are referenced through source_hash_overrides. The final
ordinary CLI build runs after all other builders; focused timing does not build.

Each focused run compares current, preserved b56425ea7 and official C Lua 5.1.5
CLIs. The parent is byte-identical to its committed ordinary-build audit; reuse
requires the retained binary or fresh provenance for a different build. Five
fixed scripts perform 5,000,000 iterations per fresh process. Nine rotated/
reversed triples yield 135 measured children, with each engine in every position
three times per fixture. Untimed warmups precede each engine/fixture. Every
child checks exact stdout, empty stderr and exit zero.

Median process wall milliseconds, first / confirmation. Paired ratios are
medians of triples; separate medians need not divide to the ratio. Smaller is faster:

| Fixture | Parent ms | Current ms | C ms | Current/parent | Current/C |
| --- | ---: | ---: | ---: | ---: | ---: |
| right_constant | 227.40 / 239.38 | 213.25 / 228.08 | 93.86 / 103.94 | 0.946 / 0.959 | 2.248 / 2.198 |
| left_constant | 214.05 / 243.35 | 216.23 / 224.95 | 98.94 / 103.65 | 0.955 / 0.917 | 2.208 / 2.177 |
| field_get | 289.12 / 293.49 | 290.14 / 286.30 | 134.98 / 133.15 | 0.962 / 0.956 | 2.108 / 2.113 |
| field_set | 302.07 / 322.96 | 252.62 / 263.23 | 119.33 / 118.23 | 0.845 / 0.814 | 2.147 / 2.241 |
| register_add | 203.60 / 189.75 | 218.97 / 198.60 | 103.90 / 96.74 | 1.075 / 1.019 | 2.119 / 2.006 |

Paired medians show 15–19% less wall time for field stores, about 4–8% less
for constant arithmetic, and about 4% less for field reads. The register-only
control takes about 2–8% more wall time and 9–10% more process CPU time; cycle
ratios also show a cost. These results support the constant paths and expose
remaining register-dispatch overhead. Rust takes about 2.0–2.3 times C's wall
time on these scripts.

Right/left constants exercise arithmetic operand modes. Field get/set use a
constant string key with a register value. register_add is a register-only
control for dispatch overhead. These whole-script results include surrounding
arithmetic, branches, loops, process launch, parsing and cleanup; they do not
establish isolated opcode latency or a universal speedup. Both wins and costs
must be read from the table. The host is shared and filesystem caches are warm.
Scheduling, frequency changes and timer quantization limit small differences.

GetProcessTimes user/kernel CPU and QueryProcessCycleTime are collected after
validated child exit, excluding Python launcher CPU. First / confirmation:

| Fixture / counter | Current/parent | Current/C |
| --- | ---: | ---: |
| right_constant: CPU | 0.846 / 0.929 | 2.500 / 2.167 |
| right_constant: Cycles | 0.911 / 0.956 | 2.642 / 2.596 |
| left_constant: CPU | 0.929 / 0.929 | 3.000 / 2.600 |
| left_constant: Cycles | 0.954 / 0.945 | 2.642 / 2.553 |
| field_get: CPU | 1.000 / 0.944 | 2.286 / 2.429 |
| field_get: Cycles | 0.955 / 0.960 | 2.356 / 2.403 |
| field_set: CPU | 0.800 / 0.789 | 2.333 / 2.500 |
| field_set: Cycles | 0.825 / 0.824 | 2.435 / 2.588 |
| register_add: CPU | 1.091 / 1.100 | 2.600 / 2.750 |
| register_add: Cycles | 1.100 / 1.040 | 2.492 / 2.404 |

These are process counters, not sampled stacks. The earlier WPR policy denial
was not retried. Focused CLI processes have no allocator instrumentation.
Physical live/peak allocator heap, peak RSS and sampled CPU stacks remain
unmeasured. Raw data retains absolute counters, process order, checksums,
binary pins and the parent/current audit hashes.

Two broad runs retain 81 runtime and 22 frontend fixtures with five paired
samples each. They cover throughput, startup/drop, compilation, Rust allocation
requests and before/after collection heap estimates. 77 of 103
preceding layouts change; both current sets agree. All changed layouts:

| Fixture | Instructions parent -> current | Entry registers parent -> current |
| --- | ---: | ---: |
| debug_gsub_args | 37 -> 33 | 11 -> 11 |
| gsub_callback_wide | 27 -> 25 | 10 -> 10 |
| string_numeric_args | 23 -> 22 | 9 -> 9 |
| debug_sort_args | 45 -> 41 | 13 -> 13 |
| debug_foreach_args | 42 -> 39 | 12 -> 12 |
| pattern_scan_long | 32 -> 28 | 4 -> 4 |
| pattern_backtrack_long | 24 -> 21 | 3 -> 3 |
| table_insert_gap | 29 -> 26 | 9 -> 9 |
| table_concat_wide_error | 28 -> 25 | 12 -> 12 |
| unpack_4 | 35 -> 33 | 9 -> 9 |
| unpack_512 | 35 -> 33 | 9 -> 9 |
| unpack_7999 | 35 -> 33 | 9 -> 9 |
| byte_7997 | 35 -> 33 | 11 -> 11 |
| native_int_narrow | 29 -> 27 | 9 -> 9 |
| native_result_error | 22 -> 20 | 11 -> 11 |
| numeric_while | 11 -> 9 | 4 -> 4 |
| constant_left | 20 -> 15 | 11 -> 7 |
| os_difftime | 19 -> 16 | 9 -> 9 |
| os_getenv | 20 -> 17 | 9 -> 9 |
| os_shell_query | 20 -> 18 | 9 -> 9 |
| os_execute | 7 -> 6 | 2 -> 2 |
| os_time | 39 -> 29 | 10 -> 10 |
| os_date_format | 19 -> 17 | 9 -> 9 |
| os_date_table | 26 -> 21 | 9 -> 9 |
| os_clock | 24 -> 22 | 11 -> 11 |
| os_rename | 24 -> 23 | 9 -> 9 |
| call_table | 20 -> 18 | 9 -> 9 |
| indexed_assignment | 75 -> 42 | 22 -> 22 |
| array_table | 20 -> 19 | 7 -> 7 |
| recursive_fib | 21 -> 18 | 3 -> 3 |
| pattern_find | 17 -> 16 | 8 -> 8 |
| pattern_replace | 19 -> 18 | 9 -> 9 |
| pattern_callback | 21 -> 19 | 10 -> 10 |
| string_format | 19 -> 17 | 9 -> 9 |
| table_sort | 30 -> 26 | 11 -> 11 |
| table_sort_callback | 36 -> 32 | 12 -> 12 |
| coroutine_resume | 37 -> 34 | 10 -> 10 |
| coroutine_wrap | 31 -> 29 | 7 -> 7 |
| coroutine_deep_yield | 53 -> 47 | 11 -> 11 |
| coroutine_reuse | 33 -> 30 | 9 -> 9 |
| gc_cycles | 20 -> 17 | 8 -> 7 |
| gc_weak_values | 31 -> 28 | 7 -> 7 |
| userdata_proxy | 27 -> 26 | 10 -> 10 |
| userdata_finalize | 35 -> 33 | 8 -> 8 |
| gc_restart | 44 -> 39 | 8 -> 7 |
| gc_steps | 47 -> 46 | 9 -> 9 |
| gc_mark_mutation | 70 -> 62 | 10 -> 10 |
| locale_numeric | 45 -> 39 | 10 -> 10 |
| locale_ctype | 57 -> 51 | 13 -> 13 |
| binary_header_reject | 35 -> 31 | 10 -> 10 |
| reader_load_bytes | 38 -> 36 | 9 -> 9 |
| debug_info | 28 -> 24 | 10 -> 10 |
| debug_locals | 22 -> 21 | 8 -> 8 |
| debug_traceback | 29 -> 27 | 10 -> 10 |
| debug_hook | 25 -> 23 | 5 -> 5 |
| io_tmpfile | 62 -> 60 | 11 -> 11 |
| require_cached | 19 -> 18 | 8 -> 8 |
| require_preload | 36 -> 28 | 9 -> 9 |
| syntax_error | 21 -> 18 | 9 -> 9 |
| runtime_error | 24 -> 21 | 10 -> 10 |
| native_arg_error | 26 -> 23 | 10 -> 10 |
| stack_overflow | 30 -> 27 | 6 -> 6 |
| capture_limit | 52 -> 48 | 11 -> 11 |
| function_dump | 45 -> 38 | 10 -> 10 |
| dump_load | 53 -> 52 | 10 -> 10 |
| native_redump | 34 -> 30 | 11 -> 11 |
| dump_long_skip | 12 -> 11 | 4 -> 4 |
| dump_long_loop | 12 -> 11 | 4 -> 4 |
| frontend_functions_190 | 958 -> 957 | 3 -> 3 |
| frontend_logical_20000 | 60010 -> 60009 | 3 -> 3 |
| frontend_index_10000 | 20012 -> 10010 | 3 -> 3 |
| frontend_constructor_190 | 585 -> 391 | 192 -> 192 |
| frontend_method_chain_10000 | 50013 -> 50012 | 5 -> 5 |
| frontend_indexed_targets_150 | 455 -> 304 | 152 -> 152 |
| frontend_compact_pool_150 | 720 -> 569 | 152 -> 152 |
| frontend_right_power_190 | 483 -> 293 | 116 -> 102 |
| frontend_right_string_power_190 | 483 -> 293 | 103 -> 102 |

Two callback runs each retain 108 samples and twenty fresh-process CPU children.
All 54 warmed Rust samples per run make zero allocation/reallocation requests
and request zero bytes. Five Lua-loop layouts and heap estimates agree across
current runs. Rust/C paired wall ratios, first / confirmation:

| Fixture | Rust/C |
| --- | ---: |
| add_direct | 0.636 / 0.701 |
| add_lua_loop | 1.859 / 1.957 |
| capture_lua_loop | 2.176 / 2.148 |
| echo_lua_loop | 1.839 / 1.895 |
| reenter_add_lua_loop | 3.070 / 3.055 |
| reenter_echo_lua_loop | 3.027 / 2.972 |

Leaf C comparisons use typed mlua Rust bridges; nested comparisons use a raw C
callback preserving original arguments and lua_pcall(LUA_MULTRET). Direct Rust
entry returns borrowed Values; its C bridge decodes f64. These comparisons
include interpreter and public callback boundaries. Rust allocator counters
exclude native C malloc. Capacity-based heaps exclude allocator headers,
native bridge storage and physical peaks.

Two cache runs each retain 135 samples and ten CPU children across five fixtures.
Compile, hit and store include source/name/build/locale/integrity checks, bounded
decoding and verification. Stores include compilation and encoding; forced-miss
deletion occurs outside timing. Current runs agree on layouts and compiled-
versus-cached VM heaps. Schema 2 retains logical debug bounds and RK fields.
Tiny sources can still compile faster than reload; no automatic profitability
policy is added. Logical-stack and operand metadata costs are included in
compilation allocation measurements.

All 13 source maps and pinned binaries/audits/build inputs verify.
Focused [first](2026-10-26-rust-lua-rk-windows-rk.json) and [confirmation](2026-10-26-rust-lua-rk-windows-rk-confirm.json), full
[first](2026-10-26-rust-lua-rk-windows.json) and [confirmation](2026-10-26-rust-lua-rk-windows-confirm.json), cache
[first](2026-10-26-rust-lua-rk-windows-cache.json) and [confirmation](2026-10-26-rust-lua-rk-windows-cache-confirm.json), callback
[first](2026-10-26-rust-lua-rk-windows-callbacks.json) and [confirmation](2026-10-26-rust-lua-rk-windows-callbacks-confirm.json)
retain the observations. Complete standalone compatibility, debug/stack parity,
later dialects, machine-code compilation and gameplay integration remain open.

~~~sh
python scripts/benchmark_rust_lua_unary.py --family rk --parent-binary target/lua-rk-parent-b56425ea7.exe --parent-audit docs/testing/2026-10-26-rust-lua-predicates-audit.json --parent-revision b56425ea7e986c69df3c4e882fc7d424bf37eef2 --current-audit docs/testing/2026-10-26-rust-lua-rk-windows-audit.json --output docs/performance/2026-10-26-rust-lua-rk-windows-rk.json
python scripts/benchmark_rust_lua.py --output docs/performance/2026-10-26-rust-lua-rk-windows.json
python scripts/benchmark_rust_lua_cache.py --output docs/performance/2026-10-26-rust-lua-rk-windows-cache.json --cpu-time
python scripts/benchmark_rust_lua_host_functions.py --output docs/performance/2026-10-26-rust-lua-rk-windows-callbacks.json --cpu-time
~~~
