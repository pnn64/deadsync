# Unary compiler and interpreter measurements

The compiler removes local-operand copies for unary minus/not/length, folds
constant logical negation and folds comparison negation into its later boolean
materialization. Hook boundaries, NaN comparisons and metamethod operation/order
are checked against official Lua 5.1.5. The existing Bool instruction carries a
verified zero/one inversion bit; its no-hook fused path also honors the bit.
Vm remains 4,440 B, Value 16 B and Frame 64 B. No dependency or runtime field is
added. [Correctness evidence](../testing/2026-10-26-rust-lua-unary-hooks.md) retains the parent regressions.

Two focused runs compare the ordinary current CLI with the byte-identical
preserved parent c2a443bb2 CLI and the current checksum-pinned C reference CLI.
Each has five fixed sources, 5,000,000 loop iterations per fresh process and
nine triples (135 measured children). Engine orders rotate and reverse in
groups, giving each engine three observations at each position per fixture.
One untimed checksum-validated warmup per engine/fixture precedes measurement.
Every child checks exact stdout, empty stderr and exit zero. The parent binary
is SHA-pinned to its committed ordinary-build audit. Reusing this comparison
requires that retained binary; a different parent build needs fresh provenance.

Median whole-process wall milliseconds, first / confirmation. Ratios are
medians of nine paired durations; separate engine medians need not divide to
the paired ratio. Smaller is faster:

| Fixture | Parent ms | Current ms | C ms | Current/parent | Current/C |
| --- | ---: | ---: | ---: | ---: | ---: |
| negate_local | 346.10 / 339.81 | 272.63 / 274.03 | 151.10 / 127.73 | 0.760 / 0.797 | 1.988 / 2.121 |
| not_local | 414.58 / 420.59 | 372.12 / 380.49 | 189.48 / 140.15 | 0.898 / 0.912 | 1.938 / 2.753 |
| length_local | 327.25 / 306.19 | 264.91 / 272.82 | 158.69 / 110.72 | 0.807 / 0.898 | 1.863 / 2.551 |
| not_comparison | 438.15 / 444.37 | 413.75 / 403.00 | 142.91 / 134.27 | 0.950 / 0.923 | 3.003 / 3.018 |
| not_constant | 444.36 / 437.90 | 432.31 / 404.22 | 131.11 / 121.35 | 0.914 / 0.931 | 3.316 / 3.333 |

The negation/local-not/length fixtures repeatedly read locals. Comparison
negation tests one inversion of a numeric comparison; constant-not repeatedly
loads not false. Each loop includes its surrounding arithmetic, branches and
loop control. These are selected whole-script measurements, not isolated
opcode latency or a universal interpreter speedup. Rust still trails C on
these workloads. Wall timing includes process launch. CPU/cycle counters cover the child
lifetime, including compilation, startup, execution and cleanup; they exclude
Python launcher CPU. Filesystem caches are warm.

Process CPU uses GetProcessTimes user/kernel values. Cycles use
QueryProcessCycleTime after validated child exit. Paired ratios, first / confirm:

| Fixture / counter | Current/parent | Current/C |
| --- | ---: | ---: |
| negate_local: Process CPU | 0.789 / 0.750 | 2.667 / 2.429 |
| negate_local: Process cycles | 0.784 / 0.787 | 2.700 / 2.412 |
| not_local: Process CPU | 0.875 / 0.889 | 3.000 / 3.143 |
| not_local: Process cycles | 0.893 / 0.892 | 3.239 / 3.166 |
| length_local: Process CPU | 0.824 / 0.889 | 2.600 / 3.200 |
| length_local: Process cycles | 0.865 / 0.887 | 2.879 / 3.036 |
| not_comparison: Process CPU | 0.929 / 0.923 | 3.429 / 3.571 |
| not_comparison: Process cycles | 0.946 / 0.915 | 3.493 / 3.491 |
| not_constant: Process CPU | 0.931 / 0.926 | 3.714 / 3.833 |
| not_constant: Process cycles | 0.913 / 0.910 | 3.958 / 4.022 |

These are process counters, not sampled call stacks. The earlier WPR policy
denial was not retried. The shared host is not CPU-isolated; scheduling,
frequency and timer quantization limit small differences. Raw data retains
absolute wall/CPU/cycle values, fixture source hashes, outputs, positions,
binary identities and parent/current audit hashes. No allocation instrumentation
runs in the focused CLI processes. Exact physical heap, RSS peaks and sampled
CPU stacks remain unmeasured.

Two full reference runs retain 81 runtime and 22 frontend workloads, five paired
samples each. All current/confirmation layouts agree. Compared with the
preceding compiler, 14 of 103 instruction/register layouts change; every
change reduces or preserves both counts:

| Fixture | Instructions parent → current | Entry registers parent → current |
| --- | ---: | ---: |
| gsub_callback_wide | 29 → 28 | 10 → 10 |
| concat_chain_16 | 31 → 30 | 22 → 22 |
| concat_chain_128 | 143 → 142 | 134 → 134 |
| table_concat_wide_error | 35 → 34 | 12 → 12 |
| native_result_error | 29 → 28 | 11 → 11 |
| byte_concat | 15 → 14 | 7 → 7 |
| pattern_replace | 21 → 20 | 9 → 9 |
| pattern_callback | 23 → 22 | 10 → 10 |
| runtime_error | 31 → 30 | 10 → 10 |
| native_arg_error | 33 → 32 | 10 → 10 |
| stack_overflow | 31 → 30 | 6 → 6 |
| frontend_quoted_10000 | 5 → 4 | 2 → 2 |
| frontend_quoted_100000 | 5 → 4 | 2 → 2 |
| frontend_quoted_1000000 | 5 → 4 | 2 → 2 |

Full raw results retain throughput, startup/drop, compile cost, Rust allocation
counts and before/after collection heap estimates. These instrumented harness
timings complement the ordinary CLI counter measurements; their allocator
models and public entry boundaries differ from C's.

Two callback runs retain 108 samples and twenty fresh-process CPU children each.
All 54 warmed Rust samples per run assert zero System allocations, reallocations
and requested bytes. The five Lua-loop layouts and heap estimates agree
between current runs. Rust/C paired wall ratios, first / confirmation:

| Fixture | Rust/C |
| --- | ---: |
| add_direct | 0.665 / 0.647 |
| add_lua_loop | 1.907 / 1.962 |
| capture_lua_loop | 2.446 / 2.296 |
| echo_lua_loop | 2.196 / 2.145 |
| reenter_add_lua_loop | 3.187 / 3.236 |
| reenter_echo_lua_loop | 3.419 / 3.348 |

Leaf callbacks use typed mlua bridges for the C comparison; nested callbacks
use a raw C function preserving the same original argument slots and
lua_pcall(LUA_MULTRET). Direct Rust entry returns borrowed Values while its
C bridge decodes f64. Results cover those fixed interfaces and interpreters.
Reference allocation counters exclude native C allocation calls; heap estimates
exclude allocator headers, native bridge storage and physical RSS/peaks.

Two Cargo cache runs retain 135 samples and ten CPU children each. Compile,
hit and store paths retain full source/name/build/locale/integrity checks,
bounded decoding and verification. Stores include compilation and encoding;
forced-miss deletion is outside timing. Both current runs agree on the five
program layouts/file sizes and compiled-versus-cached VM heap estimates. Tiny
inputs can still compile faster than reload. No profitability policy is added.

All thirteen current-source maps match the tested tree. Focused
[first](2026-10-26-rust-lua-unary-hooks-unary.json) and [confirmation](2026-10-26-rust-lua-unary-hooks-unary-confirm.json), full
[first](2026-10-26-rust-lua-unary-hooks.json) and [confirmation](2026-10-26-rust-lua-unary-hooks-confirm.json), cache
[first](2026-10-26-rust-lua-unary-hooks-cache.json) and [confirmation](2026-10-26-rust-lua-unary-hooks-cache-confirm.json), and callback
[first](2026-10-26-rust-lua-unary-hooks-callbacks.json) and [confirmation](2026-10-26-rust-lua-unary-hooks-callbacks-confirm.json)
retain their raw observations. Full standalone compatibility, complete hook
parity, later dialects, native compilation and gameplay integration remain open.

~~~sh
python scripts/benchmark_rust_lua_unary.py --parent-binary target/lua-unary-parent-c2a443bb2.exe --parent-audit docs/testing/2026-10-26-rust-lua-host-reentry-audit.json --parent-revision c2a443bb23937e6d161c6dda9b3a67148ed6fba9 --current-audit docs/testing/2026-10-26-rust-lua-unary-hooks-audit.json --output docs/performance/2026-10-26-rust-lua-unary-hooks-unary.json
python scripts/benchmark_rust_lua.py --output docs/performance/2026-10-26-rust-lua-unary-hooks.json
python scripts/benchmark_rust_lua_cache.py --output docs/performance/2026-10-26-rust-lua-unary-hooks-cache.json --cpu-time
python scripts/benchmark_rust_lua_host_functions.py --output docs/performance/2026-10-26-rust-lua-unary-hooks-callbacks.json --cpu-time
~~~
