# Lua 5.1 live native argument measurements

Two runs use five alternating paired samples across 78 runtime and 22 frontend
workloads on shared Windows x64 MSVC hardware, static CRT, binary64 and C locale.
The development official Lua 5.1.5 binding is the reference. The host is not CPU-isolated;
these measurements do not establish a before/after speedup or rankings against
other Rust engines.

debug_sort_args sorts 100 three-element arrays per script, replacing its native
comparator after the first comparison. debug_foreach_args traverses 100 arrays
and replaces its callback with a callable table. Inputs, closures, tables and
debug mutation calls are included. The C adapter opens debug for debug_* fixtures;
both engines validate the aggregate result before measurement. Timings include
host entry/return and library adapters. Compilation and initial warmup are separate.

Median nanoseconds per complete call/script, first / confirmation; ratios are
medians of five paired Rust/C durations:

| Workload | Rust ns | C binding ns | Rust/C duration |
| --- | ---: | ---: | ---: |
| `debug_sort_args` | 130,875 / 126,345 | 72,790 / 65,125 | 1.789 / 1.941 |
| `debug_foreach_args` | 136,745 / 141,550 | 65,085 / 64,255 | 2.101 / 2.139 |
| `table_sort` | 6,755,775 / 6,478,000 | 2,716,300 / 2,600,330 | 2.482 / 2.496 |
| `native_result_error` | 508,350 / 483,690 | 2,240,405 / 2,262,655 | 0.227 / 0.214 |
| `table_concat_wide_error` | 880,780 / 873,645 | 3,023,240 / 3,029,375 | 0.295 / 0.288 |
| `entry_args` | 165 / 144 | 179 / 145 | 0.922 / 0.987 |

Rust GlobalAlloc events per complete call/script, ranges across samples:

| Workload | Run | Allocations | Reallocations | Requested B |
| --- | --- | ---: | ---: | ---: |
| `debug_sort_args` | first | 102.000-102.200 | 0.000-1.050 | 6,412.000-8,281.200 |
| `debug_sort_args` | confirmation | 102.000-102.200 | 0.000-1.050 | 6,412.000-8,281.200 |
| `debug_foreach_args` | first | 104.000-104.200 | 0.000-1.200 | 6,572.000-10,799.600 |
| `debug_foreach_args` | confirmation | 104.000-104.200 | 0.000-1.200 | 6,572.000-10,799.600 |
| `table_sort` | first | 100.000 | 500.000 | 403,200.000 |
| `table_sort` | confirmation | 100.000 | 500.000 | 403,200.000 |
| `native_result_error` | first | 1,000.000 | 0.000-0.100 | 26,000.000-26,588.800 |
| `native_result_error` | confirmation | 1,000.000 | 0.000-0.100 | 26,000.000-26,588.800 |
| `table_concat_wide_error` | first | 1,000.000 | 1,000.000-1,000.100 | 158,000.000-158,588.800 |
| `table_concat_wide_error` | confirmation | 1,000.000 | 1,000.000-1,000.100 | 158,000.000-158,588.800 |
| `entry_args` | first | 0.000 | 0.000 | 0.000 |
| `entry_args` | confirmation | 0.000 | 0.000 | 0.000 |

Counts exclude C malloc and allocator metadata. Requested bytes are requests,
not live memory; zero storage events do not imply zero logical object creation.
Sort jobs no longer store cached table/comparator fields or their stale GC roots.
Live native arguments remain rooted in existing registers; pending operands remain
in the flat job vector. Traversal reuses existing scratch capacity and places its
call area after persistent state. Error-origin work runs only on cold failures.
No VM-header, frame, Error or instruction field is added. Metadata records the VM
at 4,432 B and Value at 16 B; the x64 frame stays 64 B. All 98 prior program
instruction/register layouts match.

First-run capacity estimates after samples and explicit collection:

| Workload | Phase | Live B | Reserved B | Observed peak B | Collections |
| --- | --- | ---: | ---: | ---: | ---: |
| `debug_sort_args` | before_collect | 51,467 | 66,619 | 72,519 | 43 |
| `debug_sort_args` | after_collect | 29,575 | 57,511 | 72,519 | 44 |
| `debug_foreach_args` | before_collect | 29,339 | 79,467 | 94,695 | 42 |
| `debug_foreach_args` | after_collect | 27,011 | 78,531 | 94,695 | 43 |
| `table_sort` | before_collect | 66,405 | 73,333 | 73,333 | 560 |
| `table_sort` | after_collect | 27,957 | 36,469 | 73,333 | 561 |
| `native_result_error` | before_collect | 28,858 | 34,226 | 34,226 | 0 |
| `native_result_error` | after_collect | 25,410 | 40,178 | 40,178 | 1 |
| `table_concat_wide_error` | before_collect | 29,279 | 34,631 | 34,631 | 0 |
| `table_concat_wide_error` | after_collect | 25,753 | 40,537 | 40,537 | 1 |
| `entry_args` | before_collect | 18,387 | 23,827 | 23,827 | 0 |
| `entry_args` | after_collect | 23,827 | 29,267 | 29,267 | 1 |

These estimates exclude VM headers, transient compiler/export buffers, OS/CRT
storage and allocator metadata. They are not process RSS or exact allocator peaks.
Counts include warmup and are cumulative. Confirmation snapshots and all other
samples remain in JSON. C allocations, RSS, CPU cycles and broader host/native
scaling are unmeasured. Native loops run between driver budget checks.

```sh
python scripts/benchmark_rust_lua.py --output docs/performance/2026-10-26-rust-lua-native-args.json
python scripts/benchmark_rust_lua.py --output docs/performance/2026-10-26-rust-lua-native-args-confirm.json
```

Raw data: [JSON](2026-10-26-rust-lua-native-args.json), [TSV](2026-10-26-rust-lua-native-args.tsv), [build log](2026-10-26-rust-lua-native-args.build.log),
[confirmation JSON](2026-10-26-rust-lua-native-args-confirm.json), [confirmation TSV](2026-10-26-rust-lua-native-args-confirm.tsv),
[confirmation build log](2026-10-26-rust-lua-native-args-confirm.build.log). Both runs have source digest
27e5dab69509217b39933c352a28bcacf8f7458b00e5d49ad4cfe9a8e4110f81; all recorded final source hashes match the tree. Binary
hashes identify observed executables, which later Cargo builds can replace.
The [validation report](../testing/2026-10-26-rust-lua-native-args.md) retains 441 tests, 140 process passes,
17/18 official files in both engines and 124/162 raw CLI matches (162/162 after
only known leading banner removal). Full compatibility and integration remain open.
