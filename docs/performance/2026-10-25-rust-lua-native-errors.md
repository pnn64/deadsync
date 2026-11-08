# Native argument-diagnostic measurements

The harness measures 33 execution workloads and eight frontend compilation cases,
with five paired samples in alternating engine order. The reference is vendored
official Lua 5.1.5 on Windows x86-64 MSVC, static CRT and C locale. Automatic
collection remains enabled. The machine is not CPU-isolated.

The new `native_arg_error` script creates one Lua function that tail-calls
`math.sin(false)`, then invokes it through `pcall` 100 times. Each iteration checks
the complete error, including source line, caller name, argument number and
expected/actual types. It returns 100. Ten scripts run per sample. Timing includes
host entry/return, Lua function creation, Lua/native/protected dispatch, checking
the argument, recovering the caller name, diagnostic construction/interning,
locations, loops, comparisons and automatic collection. Source compilation stays
outside execution timing. This is not an isolated formatter benchmark.

| Script | Rust ms | C ms | Paired Rust/C | Rust ms confirmation | C ms confirmation | Paired confirmation |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `native_arg_error` | 0.183 | 0.400 | 0.446 | 0.180 | 0.392 | 0.462 |

Shared cold formatters use existing native frames and call metadata. Successful
calls retain their direct argument ranges and numeric native IDs. Method and
bad-self formatting, unknown callers and raw names share that path. This script
covers a named field and a simple type failure; it does not characterize all
call contexts, options, file errors or validation-order combinations.

The two runs agree on all Rust allocation counters for this script. Totals cover
ten complete scripts, or 1,000 protected native failures, per sample:

| Sample | Allocation calls | Reallocation calls | Requested bytes |
| --- | ---: | ---: | ---: |
| 0 | 6,000 | 6,004 | 412,056 |
| 1 | 6,000 | 6,002 | 412,408 |
| 2 | 6,000 | 6,000 | 411,000 |
| 3 | 6,000 | 6,002 | 413,816 |
| 4 | 6,000 | 6,000 | 411,000 |

Counters include function/error construction and collector/control storage, not
only formatting. They sum requested allocation/reallocation bytes, including
replacement capacities, rather than retained or peak bytes.

Heap estimates agree between runs:

| Phase | Live bytes | Reserved bytes | Observed reserved peak | Collections | Reclaimed objects |
| --- | ---: | ---: | ---: | ---: | ---: |
| before collect | 21,726 | 27,974 | 27,974 | 0 | 0 |
| after collect | 23,583 | 32,159 | 32,159 | 1 | 52 |

Both phases retain three standard userdata. The reference reports 30,499 bytes
through its own accounting after collection. First-collection scratch capacity
contributes to the Rust increase; capacity estimates and C accounting differ.
Values remain 16 bytes and frames 64 bytes on this target.

| Execution workload | Current paired Rust/C | Confirmation |
| --- | ---: | ---: |
| `array_table` | 2.227 | 2.226 |
| `byte_concat` | 4.781 | 4.736 |
| `closure_calls` | 3.671 | 3.713 |
| `coroutine_deep_yield` | 2.690 | 2.813 |
| `coroutine_resume` | 2.696 | 2.612 |
| `coroutine_reuse` | 3.530 | 3.289 |
| `coroutine_wrap` | 3.566 | 3.712 |
| `debug_hook` | 1.941 | 2.074 |
| `debug_info` | 1.095 | 1.117 |
| `debug_locals` | 2.826 | 2.683 |
| `debug_traceback` | 1.469 | 1.489 |
| `gc_cycles` | 1.361 | 1.326 |
| `gc_weak_values` | 1.122 | 1.148 |
| `io_tmpfile` | 1.236 | 1.170 |
| `loadfile_text` | 1.023 | 1.075 |
| `native_arg_error` | 0.446 | 0.462 |
| `numeric_for` | 4.065 | 4.079 |
| `numeric_while` | 3.825 | 3.998 |
| `pattern_callback` | 3.178 | 3.111 |
| `pattern_find` | 0.896 | 0.882 |
| `pattern_replace` | 2.990 | 2.959 |
| `reader_load_bytes` | 2.980 | 2.886 |
| `reader_load_single` | 2.464 | 2.260 |
| `recursive_fib` | 3.762 | 3.939 |
| `require_cached` | 2.666 | 2.824 |
| `require_preload` | 1.923 | 1.897 |
| `runtime_error` | 0.461 | 0.494 |
| `string_format` | 1.164 | 1.163 |
| `syntax_error` | 0.501 | 0.413 |
| `table_sort` | 2.156 | 2.124 |
| `table_sort_callback` | 2.080 | 2.148 |
| `userdata_finalize` | 1.528 | 1.415 |
| `userdata_proxy` | 1.420 | 1.428 |

Lower ratios are better. Most workloads remain slower than C. Differences between
runs include machine noise; these measurements do not establish the cause of
each timing change or broad compatibility/performance parity.

Compilation includes destruction; source generation and correctness execution
are outside timing. Comment/quoted-string cases cover 10 KB, 100 KB and 1 MB;
dense-source cases contain 1,000/10,000 short scopes with locals and nested calls.

| Compilation workload | Current paired Rust/C | Confirmation |
| --- | ---: | ---: |
| `frontend_comment_10000` | 0.285 | 0.314 |
| `frontend_comment_100000` | 0.164 | 0.166 |
| `frontend_comment_1000000` | 0.130 | 0.138 |
| `frontend_quoted_10000` | 0.314 | 0.328 |
| `frontend_quoted_100000` | 0.303 | 0.268 |
| `frontend_quoted_1000000` | 0.319 | 0.313 |
| `frontend_scopes_1000` | 4.586 | 4.641 |
| `frontend_scopes_10000` | 4.834 | 4.809 |

Startup/drop averages 1,000 state constructions in a single sample per run:

| Engine | Current us | Confirmation us |
| --- | ---: | ---: |
| rust | 49.968 | 55.086 |
| lua51 | 113.311 | 134.391 |

Startup includes different standard-library sets. Execution and compilation
retain their separate warmup/paired protocol. The reference opens `ALL_SAFE |
DEBUG` for the four `debug_*` workloads only.

Allocation counters sum Rust `GlobalAlloc` allocation/reallocation requests over
all iterations in a sample. They exclude direct native C malloc, so they cannot
compare total allocation counts between engines. Heap estimates use capacities;
observed peaks are sampled at GC checkpoints and may miss transient peaks. These
are not process RSS, exact live allocator bytes or exact peaks.

- [Current run](2026-10-25-rust-lua-native-errors.json)
- [Confirmation](2026-10-25-rust-lua-native-errors-confirm.json)
- [Previous milestone](2026-10-25-rust-lua-runtime-errors.md)

Matching TSV/build logs retain samples, counters and machine/build metadata.
Both benchmark reports match all 56 recorded final source hashes; the official
report matches all 53. Git revision and dirty state describe measurement before
the milestone commit. Small-chunk compilation samples remain in the raw data.

```sh
python scripts/benchmark_rust_lua.py --output docs/performance/rust-lua-current.json
```

The [validation report](../testing/2026-10-25-rust-lua-native-errors.md) records
243 passing tests and 362 new reference observations. Rust still passes ten of
sixteen unchanged official probes; fifteen reference probes pass. `errors.lua`
passes the initial native argument and line-error assertions, then reaches the
uncatchable Lua call-stack limit at line 142. Catchable overflow/depth/reserve
parity, binary chunks, complete diagnostics/debug behavior, remaining OS/base
functions, native C modules, incremental collection, persistent caches, later
dialects and gameplay integration remain unfinished. Engine code remains safe
Rust with no normal/build dependencies.
