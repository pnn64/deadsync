# Lua calendar measurements

The harness now contains 48 runtime and twenty-two frontend workloads. Each
runtime fixture retains five alternating paired Rust/C samples, correctness
checks and a confirmation run. The oracle is vendored official Lua 5.1.5 on
Windows x86-64 MSVC, static CRT, binary64 and C locale. `TZ` was unset, recorded
as `<system>`; local calendar conversion uses the host's timezone. The two date
output workloads explicitly request UTC. This shared machine is not CPU-isolated.

The three added kernels are:

| Kernel | Work per complete script | Scripts per sample |
| --- | --- | ---: |
| `os_time` | One initial calendar conversion, then 1,000 equal-result checks using a retained year-2000 input table. | 30 |
| `os_date_format` | Format 1,000 successive UTC timestamps using `!%Y-%m-%d %H:%M:%S`; total length must equal 19,000. | 30 |
| `os_date_table` | Build 1,000 UTC calendar tables and sum their year/month/day fields to 2,002,000. | 20 |

The calendar input table is created on the correctness/warmup execution and
retained globally; its initial construction is outside timing. Programs compile
before execution measurements. Native conversion/formatting, interpreted loops,
result construction and automatic GC remain inside timing. Heap snapshots occur
after all five execution samples and then after an explicit full collection.
They are cumulative workload snapshots, not one-script peaks.

Durations are milliseconds per complete script, first run / confirmation.
Each engine duration is its median; ratios are medians of paired ratios.

| Workload | Rust ms | C Lua 5.1 ms | Paired Rust/C duration |
| --- | ---: | ---: | ---: |
| `os_time` | 0.646 / 0.551 | 0.410 / 0.363 | 1.557 / 1.517 |
| `os_date_format` | 2.288 / 1.914 | 2.081 / 1.782 | 1.079 / 1.077 |
| `os_date_table` | 1.467 / 1.283 | 0.857 / 0.782 | 1.697 / 1.658 |
| `os_clock` | 2.043 / 1.877 | 0.927 / 0.849 | 2.204 / 2.212 |
| `constant_left` | 1.119 / 0.917 | 0.273 / 0.224 | 4.116 / 4.063 |

Calendar conversion remains about 1.52–1.56 times the reference duration,
formatted dates about 1.08 times, and date tables about 1.66–1.70 times.
The unchanged clock/arithmetic controls remain slower than C Lua. Absolute
durations decrease in the confirmation run for both engines, while the new
paired ratios are similar. Scheduling, frequency and native runtime costs are
not isolated. There is no paired parent-engine timing comparison, and this
feature milestone makes no overall engine speedup or parity claim.

## Allocations and layout

`os_time` requests **zero warmed Rust allocations and reallocations** in all ten
samples. Field lookup continuation state uses existing frames/registers; native
conversion uses an owned stack record.

`os_date_format` requests 858 allocations, zero reallocations and 28,552 bytes in
the first thirty-script sample of each run, averaging 28.6 allocations and
951.73 requested bytes per script. The remaining four samples in each run request
zero allocations/reallocations. These repeated timestamps exercise interning
and buffer reuse after warmup; they do not establish allocation-free formatting
for arbitrary distinct date strings.

`os_date_table` consistently requests 2,000 allocations, 3,000 reallocations and
992,000 bytes per 1,000-table script: two allocations and three reallocations
per result table. The current path grows the general table hash storage while
inserting its nine fields. This provides a concrete future preallocation target;
no table-storage optimization is included here. Requested bytes include all
reallocation requests and are not retained or peak heap bytes. Native CRT
allocation counts are not instrumented.

| Workload | Instructions | Entry registers |
| --- | ---: | ---: |
| `os_time` | 48 | 10 |
| `os_date_format` | 20 | 9 |
| `os_date_table` | 27 | 9 |

`constant_left` retains 21 instructions and eleven entry registers. Both final
runs have source digest `1c4a5764b3fef0e94af50ea768dbe554072bc7d6d5697c1da1a0ce5c1f4bb6fa`; all 76 recorded Rust,
manifest, lockfile and harness hashes match the final tree.

## Heap and startup

Rust capacity estimates agree across both runs:

| Workload | Phase | Live B | Reserved B | Observed peak B | Collections | Reclaimed |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| `os_time` | `before_collect` | 20,838 | 26,094 | 26,094 | 0 | 0 |
| `os_time` | `after_collect` | 24,767 | 30,071 | 30,071 | 1 | 2 |
| `os_date_format` | `before_collect` | 64,655 | 94,255 | 94,255 | 1 | 856 |
| `os_date_format` | `after_collect` | 29,655 | 75,255 | 94,255 | 2 | 1856 |
| `os_date_table` | `before_collect` | 30,426 | 43,946 | 74,474 | 1629 | 100990 |
| `os_date_table` | `after_collect` | 23,686 | 38,198 | 74,474 | 1630 | 101005 |

Each fixture retains three standard-stream userdata objects. Tracing storage can
grow at the first full collection, so a post-collection capacity estimate may
increase. These estimates exclude native CRT/global timezone storage, stack
records and temporary formatter buffers; observed capacity is not exact allocator
peak or process RSS. Table churn triggers many atomic collections and remains a
performance concern. C live/post-collection heap snapshots are retained in JSON;
their allocator accounting is not directly equivalent to the Rust estimates.

Rust startup/drop takes 51.813 /
50.310 microseconds versus
125.290 /
110.141 for the oracle, with one
1,000-state sample per run. Rust requests 242 allocations, 63 reallocations and
39,642 bytes per state in both runs. Relative to the retained clock/rename
milestone, these counters increase by two allocations, one reallocation and 520
requested bytes. This is a historical counter comparison, not a paired startup
timing claim. Runtime compile/drop data and all twenty-two frontend scaling
fixtures remain in the raw output.

Raw data: [first JSON](2026-10-25-rust-lua-calendar.json), [first TSV](2026-10-25-rust-lua-calendar.tsv),
[first build log](2026-10-25-rust-lua-calendar.build.log), [confirmation JSON](2026-10-25-rust-lua-calendar-confirm.json),
[confirmation TSV](2026-10-25-rust-lua-calendar-confirm.tsv), [confirmation build log](2026-10-25-rust-lua-calendar-confirm.build.log).
The [validation report](../testing/2026-10-25-rust-lua-calendar.md) records 340 tests, 93 process
probes and the unchanged twelve official Rust file passes.

Exact live/peak allocator usage, RSS, cycles, other Rust engines, native C
allocation counts, diverse timezone/locale workloads and full library behavior
remain unmeasured. The standalone engine, later dialects, caches, host APIs,
JIT and integrations remain unfinished.
