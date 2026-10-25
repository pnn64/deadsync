# Initial I/O measurements

The harness measures 27 execution workloads and eight frontend compilation cases,
with five paired samples in alternating engine order. The reference is vendored
official Lua 5.1.5 on Windows x86-64 MSVC, static CRT and C locale. Automatic
collection remains enabled. The machine is not CPU-isolated.

The new `io_tmpfile` script creates a binary temporary file, writes integers 1
through 64 with newlines, seeks to zero, sums the numbers using `f:lines()` and
`tonumber`, and closes the file. Its checked result is 2,080. Ten complete scripts
run per timing sample. Execution includes host entry/return, OS temporary-file
creation/deletion, buffered I/O, numeric formatting/parsing, iterator calls and
userdata allocation. This is a small end-to-end script, not an isolated I/O or
numeric-scanner microbenchmark.

| Temporary-file script | Current | Confirmation |
| --- | ---: | ---: |
| Rust median | 0.884 ms | 0.863 ms |
| Reference median | 0.746 ms | 0.731 ms |
| Median paired Rust/reference | 1.185 | 1.183 |

Ten-script Rust samples record allocation-call totals of 2,068, 2,065, 2,063,
2,126 and 2,063, and reallocation totals of 2,013, 1,996, 1,990, 1,990 and 1,990
in both runs. Requested bytes range from 102,958 to 116,163 per sample. Returned
lines, conversions, native calls and temporary objects are included. These totals
still leave substantial allocation work to reduce.

The workload reports the same heap estimates in both runs. Before full
collection: 28,635 estimated live bytes and 36,915 reserved bytes, with fifteen
userdata. After collection: 25,540 live and 36,828 reserved bytes, seven userdata,
seven cumulative collections and 587 reclaimed objects. The observed reserved
peak is 41,307 bytes. Finalized userdata can survive until the following cycle;
this sample does not imply seven open OS handles. The separate reclamation test
collects twice after each batch and returns to the three standard file records.
The reference reports 35,142 bytes after collection through its own accounting;
the methods differ.

File userdata carries a generation-checked numeric identity into a flat stream
arena. Heap estimates include stream records and buffer capacities. Closing a
file releases its buffer allocations and OS resource, while its arena record
follows userdata lifetime. Earlier record reclamation and bounded streaming of
large writes remain optimization work. Values remain 16 bytes and frames 64 bytes
on this target. Standard input/output/error add three userdata and stream records
to each new VM; the shared registry and library environments also increase the
baseline root graph.

The existing workloads show mixed changes from the previous milestone. These
measurements include expanded library initialization and GC roots, and do not
isolate the cause of each timing change.

| Workload | Prior | Prior confirmation | Current | Confirmation |
| --- | ---: | ---: | ---: | ---: |
| `numeric_for` | 4.250 | 4.191 | 4.181 | 4.358 |
| `debug_traceback` | 1.281 | 1.291 | 1.549 | 1.592 |
| `gc_cycles` | 1.201 | 1.206 | 1.353 | 1.281 |
| `userdata_finalize` | 1.325 | 1.309 | 1.464 | 1.428 |
| `userdata_proxy` | 1.281 | 1.255 | 1.360 | 1.397 |

| Execution workload | Current paired Rust/reference | Confirmation |
| --- | ---: | ---: |
| `array_table` | 2.161 | 2.189 |
| `byte_concat` | 4.745 | 4.809 |
| `closure_calls` | 3.385 | 3.805 |
| `coroutine_deep_yield` | 2.738 | 2.756 |
| `coroutine_resume` | 2.731 | 2.749 |
| `coroutine_reuse` | 3.670 | 3.574 |
| `coroutine_wrap` | 3.385 | 3.510 |
| `debug_hook` | 1.994 | 2.207 |
| `debug_info` | 1.036 | 1.020 |
| `debug_locals` | 2.498 | 2.653 |
| `debug_traceback` | 1.549 | 1.592 |
| `gc_cycles` | 1.353 | 1.281 |
| `gc_weak_values` | 1.080 | 1.124 |
| `io_tmpfile` | 1.185 | 1.183 |
| `numeric_for` | 4.181 | 4.358 |
| `numeric_while` | 3.859 | 3.847 |
| `pattern_callback` | 3.059 | 2.978 |
| `pattern_find` | 0.842 | 0.856 |
| `pattern_replace` | 3.051 | 2.942 |
| `reader_load_bytes` | 2.765 | 2.812 |
| `reader_load_single` | 2.366 | 2.139 |
| `recursive_fib` | 3.906 | 4.159 |
| `string_format` | 1.135 | 1.138 |
| `table_sort` | 2.089 | 2.125 |
| `table_sort_callback` | 2.023 | 2.015 |
| `userdata_finalize` | 1.464 | 1.428 |
| `userdata_proxy` | 1.360 | 1.397 |

Lower ratios are better. Most workloads remain slower than reference Lua. The
new I/O result does not establish overall performance parity or complete I/O
compatibility.

Compilation includes destruction. Source generation and correctness execution
are outside timing. Comment/quoted-string cases cover 10 KB, 100 KB and 1 MB;
dense-source cases contain 1,000/10,000 short scopes with locals and nested calls.

| Compilation workload | Current paired Rust/reference | Confirmation |
| --- | ---: | ---: |
| `frontend_comment_10000` | 0.408 | 0.415 |
| `frontend_comment_100000` | 0.314 | 0.307 |
| `frontend_comment_1000000` | 0.290 | 0.290 |
| `frontend_quoted_10000` | 0.307 | 0.355 |
| `frontend_quoted_100000` | 0.274 | 0.290 |
| `frontend_quoted_1000000` | 0.306 | 0.319 |
| `frontend_scopes_1000` | 4.644 | 4.359 |
| `frontend_scopes_10000` | 4.922 | 4.858 |

Allocation counters sum Rust `GlobalAlloc` allocation/reallocation requests over
all iterations in a sample. They exclude direct native C malloc and cannot compare
total allocation counts between engines. Heap figures use capacity estimates;
observed peaks are sampled at GC checkpoints and can miss transient peaks. These
are not RSS, exact live allocator bytes or exact peaks. Startup opens different
library sets between engines. The reference opens `ALL_SAFE | DEBUG` for the
four `debug_*` execution workloads only.

- [Current run](2026-10-25-rust-lua-io.json)
- [Confirmation](2026-10-25-rust-lua-io-confirm.json)
- [Previous milestone](2026-10-25-rust-lua-hooks.md)

Matching TSV/build logs retain samples, counters and machine/build metadata. Both
runs and the official-suite report match the final source hashes. Git revision
and dirty state describe measurement before the milestone commit. Startup/drop
and small-chunk compilation samples remain in the raw data.

```sh
python scripts/benchmark_rust_lua.py --output docs/performance/rust-lua-current.json
```

The [validation report](../testing/2026-10-25-rust-lua-io.md) records 170 passing
tests and nine of sixteen passing unchanged official probes, including `db.lua`.
Fifteen reference probes pass; the retained `files.lua` reference failure is a
Windows open-file rename. Path encoding, exact standard-stream buffering,
additional devices/errors and blocking-I/O cancellation remain open, alongside
remaining libraries, binary chunks, incremental collection pauses and later
dialects. The engine remains safe Rust with no normal/build dependencies.
