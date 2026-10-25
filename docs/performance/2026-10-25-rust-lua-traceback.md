# Traceback measurements

The harness measures 25 execution workloads and eight frontend compilation cases,
with five paired wall-time samples in alternating engine order. The reference is
vendored official Lua 5.1.5 on Windows x86-64 MSVC, static CRT and C locale.
Automatic collection remains enabled. The machine is not CPU-isolated.

The new `debug_traceback` script produces 200 traces through a named Lua function.
Each trace includes a different numeric message and is compared against its full
expected bytes; the checked result is 200. Both engines use the same explicit
`=(traceback-bench)` chunk name for this workload. Other workloads retain their
previous chunk names. Ten complete scripts run per timing sample. The reference
opens `ALL_SAFE | DEBUG` for the three `debug_*` workloads only.

| Traceback script | Current | Confirmation |
| --- | ---: | ---: |
| Rust median | 0.782 ms | 0.875 ms |
| Reference median | 0.610 ms | 0.631 ms |
| Median paired Rust/reference ratio | 1.313 | 1.415 |

These include the Lua loop, string equality checks, calls and host entry/return.
They are not isolated formatter timings. Ordinary scalar formatting reuses flat
job vectors and byte buffers, but remains allocation-heavy. Each ten-script Rust
sample records 24,000 allocation calls, except the first with 24,503. Reallocation
totals are 14,004, 14,002, 14,000, 14,002 and 14,000 in both runs. Requested bytes
range from 800,760 to 831,476 per sample. This is a remaining optimization target;
callback-visible intermediate string pieces must preserve the reference behavior.
The Rust allocator counters exclude direct C malloc and cannot compare the two
engines' total allocation counts.

| Execution workload | Current paired Rust/reference | Confirmation |
| --- | ---: | ---: |
| `array_table` | 2.266 | 2.372 |
| `byte_concat` | 5.240 | 5.373 |
| `closure_calls` | 3.531 | 3.681 |
| `coroutine_deep_yield` | 2.517 | 2.601 |
| `coroutine_resume` | 2.550 | 2.508 |
| `coroutine_reuse` | 3.204 | 3.151 |
| `coroutine_wrap` | 3.558 | 3.405 |
| `debug_info` | 0.972 | 0.975 |
| `debug_locals` | 2.389 | 2.578 |
| `debug_traceback` | 1.313 | 1.415 |
| `gc_cycles` | 1.262 | 1.224 |
| `gc_weak_values` | 1.090 | 1.117 |
| `numeric_for` | 5.220 | 5.326 |
| `numeric_while` | 4.337 | 4.256 |
| `pattern_callback` | 2.831 | 3.000 |
| `pattern_find` | 0.894 | 0.845 |
| `pattern_replace` | 2.845 | 2.907 |
| `reader_load_bytes` | 2.638 | 2.687 |
| `reader_load_single` | 2.260 | 2.183 |
| `recursive_fib` | 3.641 | 3.577 |
| `string_format` | 1.106 | 1.139 |
| `table_sort` | 2.079 | 2.226 |
| `table_sort_callback` | 2.028 | 1.933 |
| `userdata_finalize` | 1.286 | 1.244 |
| `userdata_proxy` | 1.248 | 1.259 |

Lower ratios are better. Most workloads remain slower than the reference. These
runs establish the added formatter's baseline and check existing workloads; they
do not establish an overall speed improvement or engine performance parity.

Compilation includes destruction. Source generation and correctness execution are
outside timing. The comment/quoted-string sizes remain 10 KB, 100 KB and 1 MB;
dense-source cases contain 1,000 or 10,000 short scopes with locals and nested calls.

| Compilation workload | Current paired Rust/reference | Confirmation |
| --- | ---: | ---: |
| `frontend_comment_10000` | 0.436 | 0.417 |
| `frontend_comment_100000` | 0.310 | 0.311 |
| `frontend_comment_1000000` | 0.280 | 0.290 |
| `frontend_quoted_10000` | 0.315 | 0.311 |
| `frontend_quoted_100000` | 0.281 | 0.289 |
| `frontend_quoted_1000000` | 0.317 | 0.301 |
| `frontend_scopes_1000` | 4.657 | 4.558 |
| `frontend_scopes_10000` | 4.668 | 4.780 |

The traceback workload reports the same heap estimates in both runs. Before full
collection: 61,773 estimated live bytes and 74,653 reserved bytes. After collection:
19,276 live and 46,212 reserved bytes, two cumulative collections and 1,161 reclaimed
objects. The reference reports 34,453 bytes after collection through its own
accounting API; the accounting methods differ. Rust estimates include reusable
job capacity and collector storage. Observed reserved peaks are sampled at GC
checkpoints and can miss transient peaks. These are not RSS or exact allocator
peaks. Values remain 16 bytes and frames 64 bytes on this target.

- [Current run](2026-10-25-rust-lua-traceback.json)
- [Confirmation](2026-10-25-rust-lua-traceback-confirm.json)
- [Previous milestone](2026-10-25-rust-lua-debug-locals.md)

Each JSON has matching TSV/build logs, source hashes and machine/build metadata.
Both runs and the official-suite report match the final source hashes. Recorded
Git revision/dirty state describes measurement before the milestone commit.
Startup/drop and small-chunk compilation samples remain in the raw data; startup
opens different library sets between engines.

```sh
python scripts/benchmark_rust_lua.py --output docs/performance/rust-lua-current.json
```

The [validation report](../testing/2026-10-25-rust-lua-traceback.md) records 129
passing tests and eight of fifteen passing unchanged official probes. Hooks,
private native/parser scratch slots, complete diagnostics, remaining libraries,
binary chunks, incremental collection pauses and later dialects remain unfinished.
The crate remains safe Rust with no normal/build dependencies.
