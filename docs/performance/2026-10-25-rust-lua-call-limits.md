# Lua call-limit and protected-overflow measurements

The harness measures 34 execution workloads and eight frontend compilation cases,
with five paired samples in alternating engine order. The reference is vendored
official Lua 5.1.5 on Windows x86-64 MSVC, static CRT and C locale. The machine
is not CPU-isolated.

The new `stack_overflow` script stops automatic collection, recursively calls a
Lua function through `pcall`, and checks the complete overflow message and a
recursion count above 10,000. It returns 1. Three complete scripts run per sample.
The initial correctness execution warms the VM; the reference's recovered limit
can permit deeper later calls. Timing includes host entry/return, closure creation,
recursive dispatch, register/frame growth when needed, logical call checks,
diagnostic construction and protected unwinding. Source compilation is outside
timing. Collection remains enabled for the other execution workloads.

| Script | Rust ms | C ms | Paired Rust/C | Rust ms confirmation | C ms confirmation | Paired confirmation |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `stack_overflow` | 1.982 | 0.924 | 2.123 | 1.788 | 0.795 | 2.149 |

This workload remains slower than C. It exercises a normal Lua overflow and
recovery; emergency handlers, hooks, GC shrinking and coroutine transitions have
separate correctness coverage. The logical capacity uses one `u16` per context.
Ordinary Lua calls use a length bound; only near-limit checks count visible frames.
Native callback entry performs its C-boundary check separately.

Rust allocation counters agree between runs. Totals cover three complete scripts
per sample:

| Sample | Allocation calls | Reallocation calls | Requested bytes |
| --- | ---: | ---: | ---: |
| 0 | 15 | 11 | 2,098,085 |
| 1 | 15 | 8 | 805 |
| 2 | 15 | 10 | 1,957 |
| 3 | 15 | 6 | 357 |
| 4 | 15 | 8 | 1,765 |

The first sample includes additional reusable-stack growth for the later
recovered depth. Counters include closures, diagnostics and control buffers;
requested bytes include replacement capacities and are not retained/peak bytes.

Heap estimates agree between runs:

| Phase | Live bytes | Reserved bytes | Observed reserved peak | Collections | Reclaimed objects |
| --- | ---: | ---: | ---: | ---: | ---: |
| before collect | 3,166,026 | 3,172,434 | 3,172,434 | 0 | 0 |
| after collect | 3,168,428 | 3,176,468 | 3,176,468 | 1 | 49 |

Both phases retain three standard userdata. The reference reports 333,875 bytes
through its own accounting after collection. Rust retains about 3.18 MB of
reserved storage in this case, including reusable register/frame backing vectors.
The logical GC-capacity model does not shrink those allocations. Reducing retained
physical stack storage remains performance work. C accounting and Rust capacity
estimates differ; this is not a comparison of exact allocator bytes or transient
peaks. Values remain 16 bytes and frames 64 bytes on this target.

An auxiliary parent/current comparison links both zero-dependency engines into
one executable. It checks returned numbers, performs five warmups, then runs nine
alternating pairs. Numeric loops repeat twenty times per sample; Fibonacci repeats
ten times. Direct rustc builds use optimization level 3, static CRT and final fat
LTO, so this protocol differs from the Cargo reports.

| Auxiliary workload | Current/parent paired ratio |
| --- | ---: |
| `for` | 0.973 |
| `while` | 0.975 |
| `fib` | 1.003 |

This single auxiliary run does not establish an overall speedup or exclude
build-specific changes. The archived driver, samples, commands and source hashes
make its scope explicit. Its layout check reports unchanged 56-byte host errors
and an 8-byte increase in the VM struct on this target. The production Cargo
samples below retain their independent reference comparison.

| Execution workload | Current paired Rust/C | Confirmation |
| --- | ---: | ---: |
| `array_table` | 2.340 | 2.371 |
| `byte_concat` | 4.850 | 4.909 |
| `closure_calls` | 3.682 | 3.601 |
| `coroutine_deep_yield` | 3.014 | 2.846 |
| `coroutine_resume` | 2.751 | 2.631 |
| `coroutine_reuse` | 3.270 | 3.412 |
| `coroutine_wrap` | 3.877 | 3.538 |
| `debug_hook` | 1.866 | 1.811 |
| `debug_info` | 1.263 | 1.212 |
| `debug_locals` | 2.659 | 2.477 |
| `debug_traceback` | 1.494 | 1.347 |
| `gc_cycles` | 1.346 | 1.306 |
| `gc_weak_values` | 1.143 | 1.212 |
| `io_tmpfile` | 1.247 | 1.221 |
| `loadfile_text` | 1.081 | 1.038 |
| `native_arg_error` | 0.468 | 0.461 |
| `numeric_for` | 3.926 | 4.003 |
| `numeric_while` | 3.845 | 3.638 |
| `pattern_callback` | 3.053 | 3.421 |
| `pattern_find` | 0.826 | 0.862 |
| `pattern_replace` | 2.853 | 3.035 |
| `reader_load_bytes` | 2.982 | 3.015 |
| `reader_load_single` | 2.466 | 2.392 |
| `recursive_fib` | 4.205 | 4.157 |
| `require_cached` | 2.820 | 3.059 |
| `require_preload` | 1.908 | 1.922 |
| `runtime_error` | 0.487 | 0.516 |
| `stack_overflow` | 2.123 | 2.149 |
| `string_format` | 1.174 | 1.138 |
| `syntax_error` | 0.431 | 0.474 |
| `table_sort` | 2.241 | 2.192 |
| `table_sort_callback` | 2.120 | 2.123 |
| `userdata_finalize` | 1.465 | 1.471 |
| `userdata_proxy` | 1.529 | 1.499 |

Lower ratios are better. Most workloads remain slower than C. Run-to-run
variation is visible on this shared machine; these samples do not identify the
cause of every timing difference or establish compatibility/performance parity.

Compilation includes destruction; source generation and correctness execution
are outside timing. Comment/quoted-string cases cover 10 KB, 100 KB and 1 MB;
dense-source cases contain 1,000/10,000 short scopes with locals and nested calls.

| Compilation workload | Current paired Rust/C | Confirmation |
| --- | ---: | ---: |
| `frontend_comment_10000` | 0.289 | 0.270 |
| `frontend_comment_100000` | 0.155 | 0.159 |
| `frontend_comment_1000000` | 0.118 | 0.119 |
| `frontend_quoted_10000` | 0.316 | 0.279 |
| `frontend_quoted_100000` | 0.260 | 0.254 |
| `frontend_quoted_1000000` | 0.305 | 0.293 |
| `frontend_scopes_1000` | 4.841 | 4.655 |
| `frontend_scopes_10000` | 4.770 | 4.886 |

Startup/drop averages 1,000 state constructions in a single sample per run:

| Engine | Current us | Confirmation us |
| --- | ---: | ---: |
| rust | 57.991 | 52.809 |
| lua51 | 136.851 | 131.867 |

Startup includes different standard-library sets. Execution and compilation keep
their separate paired protocol. The reference opens `ALL_SAFE | DEBUG` for the
four `debug_*` workloads only.

Allocation counters sum Rust `GlobalAlloc` allocation/reallocation requests over
all iterations in a sample. They exclude direct native C malloc and cannot compare
total allocation counts between engines. Heap figures use capacities; observed
peaks are sampled at GC checkpoints and may miss transient peaks. These are not
process RSS, exact live allocator bytes or exact peaks.

- [Current run](2026-10-25-rust-lua-call-limits.json)
- [Confirmation](2026-10-25-rust-lua-call-limits-confirm.json)
- [Auxiliary manifest](2026-10-25-rust-lua-call-limits-compare.json), [driver](2026-10-25-rust-lua-call-limits-compare.rs), [samples](2026-10-25-rust-lua-call-limits-compare.tsv)
- [Previous milestone](2026-10-25-rust-lua-native-errors.md)

Matching TSV/build logs retain samples and machine/build metadata. Both primary
benchmark reports match all 58 recorded final source hashes; the official report
matches all 55. Git revision and dirty state describe measurement before the
milestone commit. Small-chunk compilation samples remain in the raw data.

```sh
python scripts/benchmark_rust_lua.py --output docs/performance/rust-lua-current.json
```

The [validation report](../testing/2026-10-25-rust-lua-call-limits.md) records 259
passing tests and 47 new paired programs. Rust still passes ten of sixteen
unchanged official probes; fifteen reference probes pass. `errors.lua` now reaches
the upvalue-limit diagnostic at line 238. Wider compiler/resource/debug behavior,
allocation failure, binary chunks, remaining OS/base functions, native C modules,
incremental GC, persistent caches, later dialects and gameplay integration remain
unfinished. Engine code remains safe Rust with zero normal/build dependencies.
