# Runtime operand-diagnostic measurements

The harness measures 32 execution workloads and eight frontend compilation cases,
with five paired samples in alternating engine order. The reference is vendored
official Lua 5.1.5 on Windows x86-64 MSVC, static CRT and C locale. Automatic
collection remains enabled. The machine is not CPU-isolated.

The new `runtime_error` script creates one Lua function that attempts arithmetic
on its boolean local `target`, then calls it through `pcall` 100 times. Every
iteration checks the complete error bytes, including the source line and local
name. The script returns 100. Ten scripts run per sample. Timing includes host
entry/return, function creation, native/protected dispatch, executing the failed
operation, reconstructing its name, error construction/interning, locations,
loops, comparisons and automatic collection. Source compilation is outside this
execution timing; this is not an isolated measurement of name lookup.

| Script | Rust ms | C ms | Paired Rust/C | Rust ms confirmation | C ms confirmation | Paired confirmation |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `runtime_error` | 0.153 | 0.306 | 0.487 | 0.155 | 0.302 | 0.508 |

Name recovery uses verified instructions and existing debug metadata only when a
failure occurs. There is no additional per-instruction provenance storage and no
extra name tracking in successful dispatch. The lookup can scan an instruction
prefix and follow decreasing register moves; its cost depends on the code and
operand origin. This case uses a direct local and does not characterize long
producer chains or all diagnostic forms.

The two runs agree on all Rust allocation counters for this script. Totals cover
ten complete scripts, or 1,000 protected failures, per sample:

| Sample | Allocation calls | Reallocation calls | Requested bytes |
| --- | ---: | ---: | ---: |
| 0 | 5,000 | 5,004 | 450,056 |
| 1 | 5,000 | 5,002 | 450,408 |
| 2 | 5,000 | 5,000 | 449,000 |
| 3 | 5,000 | 5,002 | 451,816 |
| 4 | 5,000 | 5,000 | 449,000 |

Counters include function/error construction and collector/control storage,
not only diagnostic formatting. They count requested allocation/reallocation
bytes, including replacement capacities, rather than retained or peak bytes.

Heap estimates agree between runs:

| Phase | Live bytes | Reserved bytes | Observed reserved peak | Collections | Reclaimed objects |
| --- | ---: | ---: | ---: | ---: | ---: |
| before collect | 21,643 | 27,891 | 27,891 | 0 | 0 |
| after collect | 23,490 | 32,066 | 32,066 | 1 | 52 |

Both phases retain three standard userdata. The reference reports 30,532 bytes
through its own accounting after collection. First-collection scratch capacity
contributes to the Rust increase; capacity estimates and C accounting differ.
Values remain 16 bytes and frames 64 bytes on this target.

| Execution workload | Current paired Rust/C | Confirmation |
| --- | ---: | ---: |
| `array_table` | 2.248 | 2.259 |
| `byte_concat` | 4.862 | 5.024 |
| `closure_calls` | 3.628 | 3.667 |
| `coroutine_deep_yield` | 2.727 | 2.706 |
| `coroutine_resume` | 2.776 | 2.725 |
| `coroutine_reuse` | 3.513 | 3.368 |
| `coroutine_wrap` | 3.532 | 3.582 |
| `debug_hook` | 1.859 | 1.832 |
| `debug_info` | 1.073 | 1.040 |
| `debug_locals` | 2.712 | 2.566 |
| `debug_traceback` | 1.496 | 1.516 |
| `gc_cycles` | 1.300 | 1.349 |
| `gc_weak_values` | 1.130 | 1.127 |
| `io_tmpfile` | 1.195 | 1.233 |
| `loadfile_text` | 1.050 | 1.068 |
| `numeric_for` | 4.241 | 4.186 |
| `numeric_while` | 3.778 | 3.777 |
| `pattern_callback` | 3.013 | 3.044 |
| `pattern_find` | 0.843 | 0.865 |
| `pattern_replace` | 2.868 | 2.891 |
| `reader_load_bytes` | 3.029 | 2.975 |
| `reader_load_single` | 2.404 | 2.393 |
| `recursive_fib` | 3.965 | 3.809 |
| `require_cached` | 2.596 | 2.630 |
| `require_preload` | 1.793 | 1.795 |
| `runtime_error` | 0.487 | 0.508 |
| `string_format` | 1.193 | 1.149 |
| `syntax_error` | 0.452 | 0.439 |
| `table_sort` | 2.150 | 2.120 |
| `table_sort_callback` | 2.075 | 2.049 |
| `userdata_finalize` | 1.445 | 1.487 |
| `userdata_proxy` | 1.427 | 1.484 |

Lower ratios are better. Most workloads remain slower than C. Changes between
runs include machine noise; these measurements do not identify the cause of
each timing difference or establish broad compatibility/performance parity.

Compilation includes destruction; source generation and correctness execution
are outside timing. Comment/quoted-string cases cover 10 KB, 100 KB and 1 MB;
dense-source cases contain 1,000/10,000 short scopes with locals and nested calls.

| Compilation workload | Current paired Rust/C | Confirmation |
| --- | ---: | ---: |
| `frontend_comment_10000` | 0.300 | 0.292 |
| `frontend_comment_100000` | 0.148 | 0.156 |
| `frontend_comment_1000000` | 0.127 | 0.127 |
| `frontend_quoted_10000` | 0.325 | 0.323 |
| `frontend_quoted_100000` | 0.257 | 0.264 |
| `frontend_quoted_1000000` | 0.317 | 0.328 |
| `frontend_scopes_1000` | 4.791 | 4.723 |
| `frontend_scopes_10000` | 4.997 | 4.825 |

Startup/drop averages 1,000 state constructions in a single sample per run:

| Engine | Current us | Confirmation us |
| --- | ---: | ---: |
| rust | 47.010 | 45.995 |
| lua51 | 112.333 | 106.945 |

Startup includes different standard-library sets. Execution and compilation
retain their separate warmup/paired protocol. The reference opens `ALL_SAFE |
DEBUG` for the four `debug_*` workloads only.

Allocation counters sum Rust `GlobalAlloc` allocation/reallocation requests over
all iterations in a sample. They exclude direct native C malloc, so they cannot
compare total allocation counts between engines. Heap estimates use capacities;
observed peaks are sampled at GC checkpoints and may miss transient peaks. These
are not process RSS, exact live allocator bytes or exact peaks.

- [Current run](2026-10-25-rust-lua-runtime-errors.json)
- [Confirmation](2026-10-25-rust-lua-runtime-errors-confirm.json)
- [Previous milestone](2026-10-25-rust-lua-syntax.md)

Matching TSV/build logs retain samples, counters and machine/build metadata.
Both benchmark reports match all 55 recorded final source hashes; the official
report matches all 52. Git revision and dirty state describe measurement before
the milestone commit. Small-chunk compilation samples remain in the raw data.

```sh
python scripts/benchmark_rust_lua.py --output docs/performance/rust-lua-current.json
```

The [validation report](../testing/2026-10-25-rust-lua-runtime-errors.md) records
236 passing tests and 977 new reference comparisons. Rust still passes ten of
sixteen unchanged official probes; fifteen reference probes pass. `errors.lua`
gets through the operand-name assertions and stops at a native argument-name
check. Binary chunks, complete diagnostics/debug behavior, remaining OS/base
functions, native C modules, incremental collection, persistent caches, later
dialects and gameplay integration remain unfinished. Engine code remains safe
Rust with no normal/build dependencies.
