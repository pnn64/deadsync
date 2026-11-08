# Text file-loading measurements

The harness measures 28 execution workloads and eight frontend compilation cases,
with five paired samples in alternating engine order. The reference is vendored
official Lua 5.1.5 on Windows x86-64 MSVC, static CRT and C locale. Automatic
collection remains enabled. The machine is not CPU-isolated.

The new `loadfile_text` workload loads and executes an owned named text file.
The source contains an executable header, a 4,096-byte comment and a loop summing
integers 1 through 64. Its checked result is 2,080. Fixture creation and the global
filename setup are outside timing. Ten complete scripts run per timing sample.
Each execution opens, reads, parses, compiles and closes the file, then executes
the loaded function. There is no persistent bytecode cache. Timing also includes
host entry/return, `assert`, function creation and automatic collection.

| File-loading script | Current | Confirmation |
| --- | ---: | ---: |
| Rust median | 0.133 ms | 0.134 ms |
| Reference median | 0.127 ms | 0.122 ms |
| Median paired Rust/reference | 1.088 | 1.125 |

Ten-script Rust samples record allocation-call totals of 453, 450, 450, 450, 450,
and reallocation totals of 311, 300, 300, 300, 300, in both runs.
Requested bytes range from 121,140 to 129,572 per sample. The totals include
file acquisition, retained source, parser/compiler storage, code and closures;
they do not isolate allocator work inside I/O.

The workload reports the same heap estimates in both runs. Before full
collection: 33,763 estimated live bytes and 38,619 reserved bytes. After collection:
30,388 live and 35,484 reserved bytes, nine cumulative collections and 102 reclaimed
objects. The observed reserved peak is 46,067 bytes. Three standard userdata
remain. Source readers use arena records without allocating Lua file userdata,
and release their records on completion or abort. Reusable source/parser/control
capacity contributes to these figures. The reference reports 29,963 bytes after
collection through its own accounting; the methods differ.

The reader job stores a numeric stream key, and the collector traces it through
the owning thread. Standard input has an independent stream root so I/O lookahead
survives removal of its Lua userdata. Named source readers close before the loaded
chunk executes. Input and executable-header work resume in bounded chunks; the
final compiler pass and blocking OS operations are not quota-preemptible. Values
remain 16 bytes and frames 64 bytes on this target.

| Execution workload | Current paired Rust/reference | Confirmation |
| --- | ---: | ---: |
| `array_table` | 2.207 | 2.358 |
| `byte_concat` | 4.778 | 5.028 |
| `closure_calls` | 3.294 | 3.477 |
| `coroutine_deep_yield` | 2.758 | 2.877 |
| `coroutine_resume` | 2.806 | 2.728 |
| `coroutine_reuse` | 3.726 | 3.520 |
| `coroutine_wrap` | 3.564 | 3.591 |
| `debug_hook` | 2.351 | 2.080 |
| `debug_info` | 1.025 | 1.035 |
| `debug_locals` | 2.303 | 2.766 |
| `debug_traceback` | 1.543 | 1.561 |
| `gc_cycles` | 1.285 | 1.317 |
| `gc_weak_values` | 1.170 | 1.094 |
| `io_tmpfile` | 1.218 | 1.220 |
| `loadfile_text` | 1.088 | 1.125 |
| `numeric_for` | 4.122 | 4.094 |
| `numeric_while` | 3.863 | 3.763 |
| `pattern_callback` | 3.083 | 3.211 |
| `pattern_find` | 0.870 | 0.887 |
| `pattern_replace` | 3.059 | 2.824 |
| `reader_load_bytes` | 2.759 | 2.711 |
| `reader_load_single` | 2.407 | 2.303 |
| `recursive_fib` | 3.854 | 4.068 |
| `string_format` | 1.151 | 1.142 |
| `table_sort` | 2.111 | 2.150 |
| `table_sort_callback` | 2.119 | 2.024 |
| `userdata_finalize` | 1.369 | 1.372 |
| `userdata_proxy` | 1.398 | 1.375 |

Lower ratios are better. Most workloads remain slower than reference Lua. The
file-loading workload combines filesystem, frontend and execution costs, so its
result does not establish interpreter throughput parity or complete compatibility.
Existing workload changes are mixed; these runs do not isolate their causes.

Compilation includes destruction. Source generation and correctness execution
are outside timing. Comment/quoted-string cases cover 10 KB, 100 KB and 1 MB;
dense-source cases contain 1,000/10,000 short scopes with locals and nested calls.
The small `compile_drop/loadfile_text` row in the raw data measures only the
wrapper source; loading/compiling the file occurs in its execution row above.

| Compilation workload | Current paired Rust/reference | Confirmation |
| --- | ---: | ---: |
| `frontend_comment_10000` | 0.435 | 0.431 |
| `frontend_comment_100000` | 0.319 | 0.318 |
| `frontend_comment_1000000` | 0.268 | 0.292 |
| `frontend_quoted_10000` | 0.313 | 0.306 |
| `frontend_quoted_100000` | 0.253 | 0.249 |
| `frontend_quoted_1000000` | 0.306 | 0.299 |
| `frontend_scopes_1000` | 4.418 | 4.660 |
| `frontend_scopes_10000` | 4.878 | 4.828 |

Allocation counters sum Rust `GlobalAlloc` allocation/reallocation requests over
all iterations in a sample. They exclude direct native C malloc and cannot compare
total allocation counts between engines. Heap figures use capacity estimates;
observed peaks are sampled at GC checkpoints and can miss transient peaks. These
are not RSS, exact live allocator bytes or exact peaks. Startup opens different
library sets between engines. The reference opens `ALL_SAFE | DEBUG` for the
four `debug_*` execution workloads only.

- [Current run](2026-10-25-rust-lua-file-load.json)
- [Confirmation](2026-10-25-rust-lua-file-load-confirm.json)
- [Previous milestone](2026-10-25-rust-lua-io.md)

Matching TSV/build logs retain samples, counters and machine/build metadata.
Recorded source hashes identify the measured files; Git revision and dirty state
describe measurement before the milestone commit. Startup/drop and small-chunk
compilation samples remain in the raw data.

```sh
python scripts/benchmark_rust_lua.py --output docs/performance/rust-lua-current.json
```

The [validation report](../testing/2026-10-25-rust-lua-file-load.md) records 185
passing tests and nine of sixteen passing unchanged official probes. Fifteen
reference probes pass; `files.lua` retains its Windows open-file rename failure.
Binary chunks, full diagnostics, compiler/resource bounds, remaining libraries,
incremental collection and later dialects remain unfinished. The engine remains
safe Rust with no normal/build dependencies.
