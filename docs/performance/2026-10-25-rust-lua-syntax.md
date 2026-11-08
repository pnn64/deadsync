# Source-diagnostic and long-token measurements

The harness measures 31 execution workloads and eight frontend compilation cases,
with five paired samples in alternating engine order. The reference is vendored
official Lua 5.1.5 on Windows x86-64 MSVC, static CRT and C locale. Automatic
collection remains enabled. The machine is not CPU-isolated.

The new `syntax_error` script calls `loadstring` 100 times with an unfinished
multiline table constructor. It checks the complete diagnostic, including the
closing symbol, opening line and EOF context, then returns 100. Ten complete
scripts run per sample. Timing includes host entry/return, parsing, constructing
and interning diagnostics, loops, comparisons and automatic collection. Failed
parses do not reach bytecode compilation or verification; this is not a measure
of compiler-error paths in general.

| Script | Rust ms | C ms | Paired Rust/C | Rust ms confirmation | C ms confirmation | Paired confirmation |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `syntax_error` | 0.264 | 0.554 | 0.458 | 0.260 | 0.554 | 0.460 |

Every Rust syntax-error sample in both runs records 15,000 allocation calls,
6,000 reallocation calls and 1,671,000 requested bytes across 1,000 failed loads.
These totals include parser storage and intermediate diagnostics, not just final
formatting. Success paths retain cold token-span descriptors instead of copying
extra diagnostic text; completed strings are decoded again only for a failure.

Heap estimates agree between runs:

| Phase | Live bytes | Reserved bytes | Observed reserved peak | Collections | Reclaimed objects |
| --- | ---: | ---: | ---: | ---: | ---: |
| before collect | 18,970 | 24,602 | 24,602 | 0 | 0 |
| after collect | 22,938 | 28,570 | 28,570 | 1 | 0 |

Both phases retain three standard userdata. The reference reports 30,394 bytes
through its own accounting after collection. First-collection scratch capacity
contributes to the Rust increase. Capacity estimates and C accounting differ.
Values remain 16 bytes and frames 64 bytes on this target.

Long strings/comments use safe eight-byte delimiter checks. XOR against repeated
delimiter bytes and zero-byte detection skips chunks without boundaries; a marked
chunk and the final tail use byte lookup. Newlines and zero-level opening brackets
still stop scanning at their exact positions. The reference tests cover every
byte at each of eight lane positions, including line diagnostics and delimiters.

The 1 MB levelled-comment case compiles the complete chunk and drops its program;
its generated comment body contains no delimiters. Comparing with the preceding
package milestone:

| Run | Rust ms | C ms | Paired Rust/C |
| --- | ---: | ---: | ---: |
| Package | 0.568 | 2.059 | 0.282 |
| Package confirmation | 0.610 | 2.176 | 0.279 |
| Current | 0.270 | 2.009 | 0.137 |
| Current confirmation | 0.281 | 2.145 | 0.131 |

This workload takes about half the Rust time recorded at the previous milestone.
That comparison is specific to long delimiter-free comments and includes other
frontend changes; it does not establish an engine-wide speedup. Dense-source
compilation and most runtime scripts remain slower than C.

| Execution workload | Current paired Rust/C | Confirmation |
| --- | ---: | ---: |
| `array_table` | 2.228 | 2.207 |
| `byte_concat` | 4.695 | 4.688 |
| `closure_calls` | 3.584 | 3.584 |
| `coroutine_deep_yield` | 2.743 | 2.677 |
| `coroutine_resume` | 2.600 | 2.777 |
| `coroutine_reuse` | 3.504 | 3.339 |
| `coroutine_wrap` | 3.735 | 3.497 |
| `debug_hook` | 1.898 | 2.041 |
| `debug_info` | 1.067 | 1.055 |
| `debug_locals` | 2.519 | 2.721 |
| `debug_traceback` | 1.567 | 1.518 |
| `gc_cycles` | 1.372 | 1.415 |
| `gc_weak_values` | 1.148 | 1.079 |
| `io_tmpfile` | 1.199 | 1.153 |
| `loadfile_text` | 1.055 | 1.036 |
| `numeric_for` | 3.937 | 4.088 |
| `numeric_while` | 3.740 | 4.056 |
| `pattern_callback` | 3.145 | 3.435 |
| `pattern_find` | 0.900 | 0.846 |
| `pattern_replace` | 3.007 | 3.043 |
| `reader_load_bytes` | 2.802 | 2.803 |
| `reader_load_single` | 2.364 | 2.386 |
| `recursive_fib` | 3.728 | 3.700 |
| `require_cached` | 2.598 | 2.662 |
| `require_preload` | 1.849 | 1.804 |
| `string_format` | 1.155 | 1.211 |
| `syntax_error` | 0.458 | 0.460 |
| `table_sort` | 2.177 | 2.155 |
| `table_sort_callback` | 2.045 | 2.104 |
| `userdata_finalize` | 1.384 | 1.432 |
| `userdata_proxy` | 1.437 | 1.408 |

Lower ratios are better. Changes between runs include machine noise; these
samples do not identify the cause of each timing difference. The cached-import
workload still allocates nothing after warmup but retains native dispatch and
lookup overhead. No broad compatibility or performance parity is claimed.

Compilation includes destruction; source generation and correctness execution
are outside timing. Comment/quoted-string cases cover 10 KB, 100 KB and 1 MB;
dense-source cases contain 1,000/10,000 short scopes with locals and nested calls.

| Compilation workload | Current paired Rust/C | Confirmation |
| --- | ---: | ---: |
| `frontend_comment_10000` | 0.311 | 0.308 |
| `frontend_comment_100000` | 0.171 | 0.151 |
| `frontend_comment_1000000` | 0.137 | 0.131 |
| `frontend_quoted_10000` | 0.308 | 0.311 |
| `frontend_quoted_100000` | 0.276 | 0.268 |
| `frontend_quoted_1000000` | 0.313 | 0.318 |
| `frontend_scopes_1000` | 4.790 | 4.501 |
| `frontend_scopes_10000` | 4.819 | 4.922 |

Startup/drop averages 1,000 state constructions in a single sample per run:

| Engine | Current us | Confirmation us |
| --- | ---: | ---: |
| rust | 47.444 | 47.993 |
| lua51 | 112.880 | 113.416 |

Startup includes different standard-library sets and is not an isolated measure
of diagnostic setup. Execution/compilation retain their separate warmup protocol.
The C reference opens `ALL_SAFE | DEBUG` for the four `debug_*` workloads only.

Allocation counters sum Rust `GlobalAlloc` allocation/reallocation requests over
all iterations in a sample. They exclude direct native C malloc, so they cannot
compare total allocation counts between engines. Heap estimates use capacities;
observed peaks are sampled at GC checkpoints and may miss transient peaks. These
are not process RSS, exact live allocator bytes or exact peaks.

- [Current run](2026-10-25-rust-lua-syntax.json)
- [Confirmation](2026-10-25-rust-lua-syntax-confirm.json)
- [Previous milestone](2026-10-25-rust-lua-package.md)

Matching TSV/build logs retain samples, counters and machine/build metadata.
Both benchmark reports match all 53 recorded final source hashes; the official
report matches all 50. Git revision and dirty state describe measurement before
the milestone commit. Small-chunk compilation samples remain in the raw data.

```sh
python scripts/benchmark_rust_lua.py --output docs/performance/rust-lua-current.json
```

The [validation report](../testing/2026-10-25-rust-lua-syntax.md) records 226
passing tests, 13,439 new reference observations and ten of sixteen passing
unchanged official probes. Fifteen reference probes pass; `files.lua` retains its
Windows rename failure. `errors.lua` passes the initial syntax assertions and
stops on runtime operand-name diagnostics. Binary chunks, complete diagnostics,
remaining OS/base functions, native C modules, incremental collection, persistent
caches, later dialects and gameplay integration remain unfinished. Engine code
remains safe Rust with no normal/build dependencies.
