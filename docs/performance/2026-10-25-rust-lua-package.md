# Initial package-loading measurements

The harness measures thirty execution workloads and eight frontend compilation
cases, with five paired samples in alternating engine order. The reference is
vendored official Lua 5.1.5 on Windows x86-64 MSVC, static CRT and C locale.
Automatic collection remains enabled. The machine is not CPU-isolated.

Two new scripts exercise module loading. `require_cached` imports `math`, then
performs 1,000 cached imports, checking object identity and returning 1,000.
`require_preload` installs a Lua provider, then clears its cache entry and imports
it 100 times, checking each returned table's name and returning 100. Ten complete
scripts run per sample. Timing includes host entry/return, native phases, loops,
comparisons and automatic collection. The preload script also includes provider
creation, cache mutation, Lua calls and returned-table allocation.

| Script | Rust ms | C ms | Paired Rust/C | Rust ms confirmation | C ms confirmation | Paired confirmation |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `require_cached` | 0.291 | 0.110 | 2.594 | 0.287 | 0.108 | 2.662 |
| `require_preload` | 0.137 | 0.076 | 1.812 | 0.143 | 0.076 | 1.823 |

Every warmed cached-import sample in both runs records zero Rust allocation
calls, reallocation calls and requested bytes. String names reuse their interned
identity; continuation state uses reusable registers and frames. This result
covers the measured script, not all import/name/metatable configurations. Cached
imports still schedule native phases and perform registry/table lookups; reducing
that dispatch work remains an optimization target.

Ten-script preload samples record allocation totals of 2003 / 2000 / 2000 / 2000 / 2000,
and reallocation totals of 13 / 0 / 0 / 0 / 0, in both runs. Requested bytes
are 193,232 in the first sample and 160,000 in later samples. These include
returned tables and provider closures; they are not isolated loader allocations.

Heap estimates match between runs:

| Script | Live before GC | Reserved before GC | Live after GC | Reserved after GC | Observed reserved peak | Collections after GC | Reclaimed objects |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Cached imports | 18,644 | 24,324 | 22,612 | 28,292 | 28,292 | 1 | 0 |
| Preloaded imports | 41,818 | 62,986 | 24,938 | 52,122 | 78,346 | 32 | 5,150 |

Both retain three standard userdata. The reference reports 30,105 and 31,013
bytes respectively after collection through its own accounting; the methods
differ. First-collection collector scratch capacity contributes to the cached
script's increase. Light userdata owns no object storage. Values remain 16 bytes
and frames 64 bytes on this target.

State initialization now includes the package namespace, loader/preload tables,
path/config fields and environment lookup. The raw startup/drop row averages
1,000 state constructions per run:

| Engine | Current | Confirmation |
| --- | ---: | ---: |
| rust | 48.790 us | 49.839 us |
| lua51 | 115.734 us | 128.681 us |

These are single startup samples with different library sets, not an isolated
measurement of package initialization. Execution and compilation samples retain
their separate warmup/paired protocol.

| Execution workload | Current paired Rust/reference | Confirmation |
| --- | ---: | ---: |
| `array_table` | 2.216 | 2.184 |
| `byte_concat` | 4.864 | 4.847 |
| `closure_calls` | 3.630 | 3.593 |
| `coroutine_deep_yield` | 2.819 | 2.833 |
| `coroutine_resume` | 2.799 | 2.675 |
| `coroutine_reuse` | 3.516 | 3.552 |
| `coroutine_wrap` | 3.683 | 3.594 |
| `debug_hook` | 1.872 | 1.906 |
| `debug_info` | 1.008 | 0.985 |
| `debug_locals` | 2.799 | 2.783 |
| `debug_traceback` | 1.535 | 1.558 |
| `gc_cycles` | 1.338 | 1.346 |
| `gc_weak_values` | 1.119 | 1.130 |
| `io_tmpfile` | 1.203 | 1.177 |
| `loadfile_text` | 1.092 | 1.068 |
| `numeric_for` | 4.265 | 4.340 |
| `numeric_while` | 3.802 | 3.837 |
| `pattern_callback` | 2.996 | 3.007 |
| `pattern_find` | 1.047 | 0.851 |
| `pattern_replace` | 2.922 | 2.972 |
| `reader_load_bytes` | 2.800 | 2.700 |
| `reader_load_single` | 2.325 | 2.400 |
| `recursive_fib` | 3.941 | 3.806 |
| `require_cached` | 2.594 | 2.662 |
| `require_preload` | 1.812 | 1.823 |
| `string_format` | 1.142 | 1.169 |
| `table_sort` | 2.163 | 2.228 |
| `table_sort_callback` | 2.046 | 2.069 |
| `userdata_finalize` | 1.445 | 1.423 |
| `userdata_proxy` | 1.402 | 1.408 |

Lower ratios are better. Most scripts remain slower than C. The pattern-search
ratio varies between the two runs, reinforcing the limits of measurements on a
shared, non-isolated machine. These runs do not identify the cause of each timing
change or establish full compatibility/performance parity.

Compilation includes destruction; source generation and correctness execution
are outside timing. Comment/quoted-string cases cover 10 KB, 100 KB and 1 MB;
dense-source cases contain 1,000/10,000 short scopes with locals and nested calls.

| Compilation workload | Current paired Rust/reference | Confirmation |
| --- | ---: | ---: |
| `frontend_comment_10000` | 0.448 | 0.445 |
| `frontend_comment_100000` | 0.308 | 0.320 |
| `frontend_comment_1000000` | 0.282 | 0.279 |
| `frontend_quoted_10000` | 0.324 | 0.322 |
| `frontend_quoted_100000` | 0.261 | 0.262 |
| `frontend_quoted_1000000` | 0.285 | 0.279 |
| `frontend_scopes_1000` | 4.670 | 4.841 |
| `frontend_scopes_10000` | 4.672 | 4.715 |

Allocation counters sum Rust `GlobalAlloc` allocation/reallocation requests over
all iterations in a sample. They exclude direct native C malloc and cannot compare
total allocation counts between engines. Heap figures use capacity estimates;
observed peaks are sampled at GC checkpoints and can miss transient peaks. These
are not RSS, exact live allocator bytes or exact peaks. The reference opens
`ALL_SAFE | DEBUG` for the four `debug_*` execution workloads only.

- [Current run](2026-10-25-rust-lua-package.json)
- [Confirmation](2026-10-25-rust-lua-package-confirm.json)
- [Previous milestone](2026-10-25-rust-lua-file-load.md)

Matching TSV/build logs retain samples, counters and machine/build metadata. Both
runs and the official report match the final source hashes. Git revision and
dirty state describe measurement before the milestone commit. Small-chunk
compilation samples remain in the raw data.

```sh
python scripts/benchmark_rust_lua.py --output docs/performance/rust-lua-current.json
```

The [validation report](../testing/2026-10-25-rust-lua-package.md) records 210
passing tests and ten of sixteen passing unchanged official probes, including
`math.lua`. Fifteen reference probes pass; `files.lua` retains its Windows rename
failure. Native C libraries, binary chunks, full diagnostics, remaining OS/base
functions, incremental collection, persistent caches and later dialects remain
unfinished. The engine remains safe Rust with no normal/build dependencies.
