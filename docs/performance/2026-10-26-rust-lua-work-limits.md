# Lua 5.1 native work measurements

Two runs use five alternating paired samples across 76 runtime and 22 frontend
workloads on shared Windows x64 MSVC hardware, static CRT, binary64 and C locale.
The development official Lua 5.1.5 binding is the reference. The host is not CPU-isolated;
these measurements do not establish a before/after throughput gain or
performance against other Rust engines.

Four additions measure a 32,768-byte scan with a 32-atom pattern, anchored greedy
backtracking on 400 bytes, 100 insert/clear operations across a 1,024-slot gap and
1,000 protected concat failures over full-width integer intervals. Persistent
input strings/tables are created during the excluded warmup; script-local pattern
construction, cache lookup, callbacks, copies, exceptions and host entry/return
remain included. Rust compiles cached pattern instructions; C interprets its
patterns. These are complete scripts rather than isolated matcher operations.

Median nanoseconds per complete call/script, first / confirmation. Ratios are
medians of five paired Rust/C durations:

| Workload | Rust ns | C binding ns | Rust/C duration |
| --- | ---: | ---: | ---: |
| `pattern_scan_long` | 4,041,330 / 4,394,810 | 5,480,840 / 6,002,720 | 0.744 / 0.749 |
| `pattern_backtrack_long` | 104,386,633 / 110,884,466 | 79,828,100 / 86,309,266 | 1.303 / 1.301 |
| `table_insert_gap` | 3,338,280 / 3,908,110 | 2,900,060 / 3,385,070 | 1.146 / 1.165 |
| `table_concat_wide_error` | 918,880 / 933,565 | 3,008,945 / 3,065,225 | 0.298 / 0.302 |
| `table_sort` | 6,215,300 / 6,405,240 | 2,636,590 / 2,618,510 | 2.344 / 2.436 |
| `entry_args` | 173 / 198 | 166 / 146 | 1.036 / 1.385 |

Rust GlobalAlloc events per complete call/script, ranges across samples:

| Workload | Run | Allocations | Reallocations | Requested B |
| --- | --- | ---: | ---: | ---: |
| `pattern_scan_long` | first | 2.000 | 1.000 | 95.000 |
| `pattern_scan_long` | confirmation | 2.000 | 1.000 | 95.000 |
| `pattern_backtrack_long` | first | 0.000 | 0.000 | 0.000 |
| `pattern_backtrack_long` | confirmation | 0.000 | 0.000 | 0.000 |
| `table_insert_gap` | first | 0.000 | 0.000 | 0.000 |
| `table_insert_gap` | confirmation | 0.000 | 0.000 | 0.000 |
| `table_concat_wide_error` | first | 1,000.000 | 1,000.000-1,000.100 | 158,000.000-158,588.800 |
| `table_concat_wide_error` | confirmation | 1,000.000 | 1,000.000-1,000.100 | 158,000.000-158,588.800 |
| `table_sort` | first | 100.000 | 500.000 | 403,200.000 |
| `table_sort` | confirmation | 100.000 | 500.000 | 403,200.000 |
| `entry_args` | first | 0.000 | 0.000 | 0.000 |
| `entry_args` | confirmation | 0.000 | 0.000 | 0.000 |

Counts exclude C malloc and allocator metadata; requested bytes are not live
memory. Zero storage requests do not imply zero logical object creation. The
per-pattern-instruction counter/check and reset calls are removed. One u64 field
is removed from the pattern work storage inside the VM. The [layout probe](2026-10-26-rust-lua-work-limits-layout.json)
records 4,440 bytes before and 4,432 after, using the same `size_of::<Vm>` source,
compiler and target; release benchmark metadata independently records 4,432 B.
This is struct layout, not owned heap memory. Value stays 16 B and the x64 frame
stays 64 B. All 94 prior instruction/register layouts match; no frame/instruction
field or dispatch abstraction is added.

First-run capacity estimates after samples and explicit collection:

| Workload | Phase | Live B | Reserved B | Observed peak B | Collections |
| --- | --- | ---: | ---: | ---: | ---: |
| `pattern_scan_long` | before_collect | 85,777 | 91,121 | 91,121 | 0 |
| `pattern_scan_long` | after_collect | 90,833 | 96,225 | 96,225 | 1 |
| `pattern_backtrack_long` | before_collect | 20,603 | 25,979 | 25,979 | 0 |
| `pattern_backtrack_long` | after_collect | 26,043 | 31,419 | 31,419 | 1 |
| `table_insert_gap` | before_collect | 20,011 | 25,331 | 25,331 | 0 |
| `table_insert_gap` | after_collect | 25,451 | 30,771 | 30,771 | 1 |
| `table_concat_wide_error` | before_collect | 29,279 | 34,631 | 34,631 | 0 |
| `table_concat_wide_error` | after_collect | 25,753 | 40,537 | 40,537 | 1 |
| `table_sort` | before_collect | 66,501 | 73,429 | 73,429 | 560 |
| `table_sort` | after_collect | 28,053 | 36,565 | 73,429 | 561 |
| `entry_args` | before_collect | 18,387 | 23,827 | 23,827 | 0 |
| `entry_args` | after_collect | 23,827 | 29,267 | 29,267 | 1 |

Capacity estimates exclude VM headers, transient compiler/export buffers, OS/CRT
memory and allocator metadata. They are not RSS or exact allocation peaks. Counts
include warmup and are cumulative. Confirmation snapshots and all other workload
samples remain in JSON. C allocations, RSS, CPU cycles and broad resource scaling
are unmeasured. Native loops finish before driver quotas are checked again.

```sh
python scripts/benchmark_rust_lua.py --output docs/performance/2026-10-26-rust-lua-work-limits.json
python scripts/benchmark_rust_lua.py --output docs/performance/2026-10-26-rust-lua-work-limits-confirm.json
```

Raw data: [JSON](2026-10-26-rust-lua-work-limits.json), [TSV](2026-10-26-rust-lua-work-limits.tsv), [build log](2026-10-26-rust-lua-work-limits.build.log),
[confirmation JSON](2026-10-26-rust-lua-work-limits-confirm.json), [confirmation TSV](2026-10-26-rust-lua-work-limits-confirm.tsv),
[confirmation build log](2026-10-26-rust-lua-work-limits-confirm.build.log). Both runs have source digest
4703f4d064db96a61063da3dab059f2fd7e6dde9d4f8274237f4a550be6abb3e; all recorded final source hashes match the tree. Binary
hashes identify observed executables, which later Cargo builds can replace.
The [validation report](../testing/2026-10-26-rust-lua-work-limits.md) retains 430 tests, 140 process passes,
17/18 official files in both engines and 112/150 raw CLI matches (150/150 after
only leading version-banner removal). Complete compatibility and integration
remain unfinished.
