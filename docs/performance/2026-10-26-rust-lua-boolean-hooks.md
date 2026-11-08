# Short-circuit boolean value measurements

Negated and/or values now use flat compiler jobs and a compact descriptor with
pending true/false edges. Edges link through instruction target slots and are
patched after the terminal operand/result is known. No per-expression patch
vector, interpreter field or opcode is added. Local tests read registers
directly. A verified comparison/conditional branch fuses when instruction
hooks are disabled; callbacks retain the separate observable boundary.
Vm stays 4,440 B, Value 16 B and Frame 64 B. Native export keeps its comparison
and boolean skip pairs intact through long-branch relaxation.
[Correctness evidence](../testing/2026-10-26-rust-lua-boolean-hooks.md) records parent failures and limits.

Concurrent edits to the shared workspace manifest/lockfile invalidated initial
timing guards. The retained performance runs use an isolated checkout with exact
Lua, legacy bridge and vendored C source bytes copied from the main worktree.
The workspace manifest/lockfile were frozen after the dependency build settled.
Their snapshots and hashes are retained in the workspace-inputs directory;
source_hash_overrides in each performance JSON identifies those inputs.
The [input manifest](2026-10-26-rust-lua-boolean-hooks-workspace-inputs/manifest.json) records the isolated
revision, target directory and frozen input hashes. Core
source hashes still match the current worktree. The CLI audit now honors
CARGO_TARGET_DIR, and metadata records its actual reference executable path.

Two focused runs compare the ordinary current CLI, the byte-identical preserved
56bc0c2a7 parent CLI and the current pinned official C Lua 5.1.5 CLI. Each has
five sources, 5,000,000 loop iterations per fresh process and nine rotated/
reversed triples (135 measured children). Every engine occupies each position
three times per fixture. One untimed checksum-validated warmup precedes each
engine/fixture. All children check exact stdout, empty stderr and exit zero.
The parent binary is pinned by its committed ordinary-build audit; reuse needs
that retained binary, while a different parent build needs fresh provenance.

Median whole-process wall milliseconds, first / confirmation. Paired ratios
are medians of nine triples; separate medians need not divide to the ratio.
Smaller is faster:

| Fixture | Parent ms | Current ms | C ms | Current/parent | Current/C |
| --- | ---: | ---: | ---: | ---: | ---: |
| or_shortcut_true | 528.74 / 508.28 | 479.02 / 446.41 | 152.56 / 144.01 | 0.873 / 0.865 | 2.962 / 3.100 |
| or_rhs_false | 430.02 / 431.76 | 389.04 / 439.47 | 173.54 / 159.61 | 0.936 / 1.037 | 2.476 / 2.553 |
| and_shortcut_false | 392.92 / 407.69 | 405.61 / 361.24 | 128.06 / 119.37 | 1.063 / 0.935 | 3.066 / 3.105 |
| nested_mixed | 447.92 / 425.41 | 389.11 / 406.91 | 171.97 / 175.26 | 0.850 / 0.945 | 2.501 / 2.349 |
| comparison_rhs | 428.14 / 419.65 | 473.07 / 422.80 | 161.84 / 149.39 | 1.083 / 1.007 | 2.780 / 2.887 |

The true OR shortcut takes about 13% less wall time in both runs, while the
nested fixture improves by about 5-15%. The RHS-false and AND-shortcut results
change direction between runs. The comparison RHS ranges from roughly parity
to 8% slower. These results do not support an across-the-board speedup.

The OR shortcut and RHS fixtures take opposite paths; the AND fixture takes
the false shortcut. The mixed fixture nests AND inside OR. The comparison RHS
checks a deferred numeric predicate and inversion. Timings include surrounding
arithmetic, branches and loop control. They are selected whole-script results,
not isolated opcode latency or a universal interpreter speedup. C remains a
separate performance reference; both gains and remaining costs are retained.
Wall includes process launch. CPU/cycle counters cover the child lifetime,
including compilation, startup, execution and cleanup, excluding Python launcher
CPU. Filesystem caches are warm.

GetProcessTimes supplies user/kernel CPU and QueryProcessCycleTime supplies
cycles after validated child exit. Paired ratios, first / confirmation:

| Fixture / counter | Current/parent | Current/C |
| --- | ---: | ---: |
| or_shortcut_true: Process CPU | 0.886 / 0.867 | 3.571 / 3.571 |
| or_shortcut_true: Process cycles | 0.867 / 0.864 | 3.314 / 3.585 |
| or_rhs_false: Process CPU | 0.920 / 1.040 | 2.750 / 3.250 |
| or_rhs_false: Process cycles | 0.908 / 1.053 | 2.635 / 2.876 |
| and_shortcut_false: Process CPU | 1.043 / 0.885 | 3.714 / 3.500 |
| and_shortcut_false: Process cycles | 1.068 / 0.936 | 3.719 / 3.735 |
| nested_mixed: Process CPU | 0.857 / 0.920 | 2.556 / 2.556 |
| nested_mixed: Process cycles | 0.847 / 0.942 | 2.798 / 2.725 |
| comparison_rhs: Process CPU | 1.120 / 1.000 | 3.375 / 3.429 |
| comparison_rhs: Process cycles | 1.098 / 0.999 | 3.222 / 3.244 |

These are process counters, not sampled stacks. The earlier WPR policy denial
was not retried. The shared host is not CPU-isolated; frequency, scheduling and
timer quantization limit small differences. Raw data includes absolute counters,
source/output hashes, positions, binary identities, family selection and both
audit hashes. No allocation instrumentation runs in focused CLI processes.
Physical allocator live/peak heap, RSS peaks and CPU stacks remain unmeasured.

Two broad reference runs retain 81 runtime and 22 frontend fixtures, five paired
samples each. Current/confirmation layouts agree. Compared with the preceding
compiler, 0 of 103 layouts change:

| Fixture | Instructions parent -> current | Entry registers parent -> current |
| --- | ---: | ---: |
| All 103 prior fixtures | Unchanged | Unchanged |

These raw runs retain throughput, startup/drop, compilation, Rust allocation
counts and before/after collection heap estimates. Existing broad fixtures and
their instrumented public entry boundaries complement the focused logical
paths; their allocator models differ from C's.

Two callback runs retain 108 samples and twenty fresh-process CPU children each.
All 54 warmed Rust samples per run assert zero System allocation/reallocation
requests and requested bytes. Five Lua-loop layouts and heap estimates agree
between current runs. Rust/C paired wall ratios, first / confirmation:

| Fixture | Rust/C |
| --- | ---: |
| add_direct | 0.640 / 0.652 |
| add_lua_loop | 1.867 / 2.037 |
| capture_lua_loop | 2.228 / 2.282 |
| echo_lua_loop | 2.305 / 2.270 |
| reenter_add_lua_loop | 3.207 / 3.046 |
| reenter_echo_lua_loop | 3.486 / 3.448 |

Leaf C comparisons use typed mlua Rust bridges. Nested comparisons use a raw
C callback preserving the same original arguments and lua_pcall(LUA_MULTRET).
Direct Rust entry returns borrowed Values; its C bridge decodes f64. These
measurements include the interpreters and public callback boundaries. Reference
allocation counters exclude native C allocation calls; capacity-based heap
estimates exclude allocator headers, native bridge storage and physical peaks.

Two cache runs retain 135 samples and ten CPU children each. They measure
compile/hit/store with source/name/build/locale/integrity checks, bounded
decoding and verification. Stores include compile/encoding; forced-miss deletion
is outside timing. Current runs agree on five layouts/file sizes and compiled-
versus-cached VM heaps. Tiny inputs can still compile faster than reload; no
automatic profitability policy is added.

All thirteen source maps match the Lua sources and recorded build inputs. Focused
[first](2026-10-26-rust-lua-boolean-hooks-logical.json) and [confirmation](2026-10-26-rust-lua-boolean-hooks-logical-confirm.json), full
[first](2026-10-26-rust-lua-boolean-hooks.json) and [confirmation](2026-10-26-rust-lua-boolean-hooks-confirm.json), cache
[first](2026-10-26-rust-lua-boolean-hooks-cache.json) and [confirmation](2026-10-26-rust-lua-boolean-hooks-cache-confirm.json), and callback
[first](2026-10-26-rust-lua-boolean-hooks-callbacks.json) and [confirmation](2026-10-26-rust-lua-boolean-hooks-callbacks-confirm.json)
retain their observations. Full standalone Lua 5.1, complete hook/predicate
parity, later dialects, native compilation and gameplay integration remain open.

~~~sh
python scripts/benchmark_rust_lua_unary.py --family logical --parent-binary target/lua-logical-parent-56bc0c2a7.exe --parent-audit docs/testing/2026-10-26-rust-lua-unary-hooks-audit.json --parent-revision 56bc0c2a73ce75b7d30734234d0c7ce71cf28cf9 --current-audit docs/testing/2026-10-26-rust-lua-boolean-hooks-audit.json --output docs/performance/2026-10-26-rust-lua-boolean-hooks-logical.json
python scripts/benchmark_rust_lua.py --output docs/performance/2026-10-26-rust-lua-boolean-hooks.json
python scripts/benchmark_rust_lua_cache.py --output docs/performance/2026-10-26-rust-lua-boolean-hooks-cache.json --cpu-time
python scripts/benchmark_rust_lua_host_functions.py --output docs/performance/2026-10-26-rust-lua-boolean-hooks-callbacks.json --cpu-time
~~~
