# Lua clock and rename measurements

The harness retains 45 runtime and twenty-two frontend workloads, with five
alternating paired samples per workload and a confirmation run. The oracle is
vendored official Lua 5.1.5 on Windows x86-64 MSVC, static CRT and C locale.
The shared machine is not CPU-isolated. No overall engine speedup or performance
parity is claimed.

`os_clock` makes one initial reading followed by 10,000 calls with ignored
arguments, verifies monotonic values and returns 10,000. Thirty complete scripts
run per sample. `os_rename` performs 32 round trips (64 moves) between two names
on the same volume and returns 32; five scripts run per sample. The host creates
the file outside timing, and each complete script restores the starting name.
Both engines use the same paths. Correctness checks and compilation are outside
execution timing; clock reads and native filesystem work are inside.

Durations are milliseconds per complete script, first run / confirmation.
Each engine duration is its median; the ratio is the median of paired ratios.

| Workload | Rust ms | C Lua 5.1 ms | Paired Rust/C duration |
| --- | ---: | ---: | ---: |
| `os_clock` | 1.872 / 2.469 | 0.842 / 1.039 | 2.243 / 2.399 |
| `os_rename` | 46.673 / 47.285 | 46.697 / 47.246 | 0.999 / 0.989 |
| `constant_left` | 0.878 / 0.920 | 0.210 / 0.226 | 4.168 / 4.063 |
| `os_execute` | 20.089 / 28.604 | 19.812 / 31.366 | 1.014 / 0.727 |

The clock kernel is still slower than C Lua. Rename is dominated by filesystem
work in this fixture and is close to C; it does not establish interpreter parity.
The unchanged shell kernel varies noticeably between runs. CPU scheduling,
frequency and process/filesystem behavior are not isolated. There is no paired
parent-engine timing comparison in this feature milestone.

Both new kernels request **zero warmed Rust allocations or reallocations** in
every sample of both runs. Clock takes no argument buffers. Windows rename uses
two initialized 128-unit UTF-16 stack buffers; the conservative WTF-8 byte-length
bound keeps common paths in those arrays. Longer names spill without truncation.
Recorded fixture lengths are 107 and 105 ASCII bytes, including their directory
prefixes, so these calls stay on the stack path. Valid longer paths and failed
oversized names have correctness coverage, but their allocation/throughput cost
is not benchmarked here. Native CRT/kernel allocation counts are not measured.

The new kernels emit 29 and 25
instructions with 11 and 9
entry registers, respectively. `constant_left` retains 21 instructions and eleven
entry registers. Both final runs have the same source digest; all 73 recorded
source/configuration hashes match the retained tree.

Heap estimates agree across both runs:

| Workload | Phase | Live B | Reserved B | Observed peak B | Collections | Reclaimed |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| `os_clock` | `before_collect` | 19,454 | 24,942 | 24,942 | 0 | 0 |
| `os_clock` | `after_collect` | 23,422 | 28,910 | 28,910 | 1 | 0 |
| `os_rename` | `before_collect` | 19,546 | 24,970 | 24,970 | 0 | 0 |
| `os_rename` | `after_collect` | 23,514 | 28,938 | 28,938 | 1 | 0 |

Both retain three standard-stream userdata objects. The first full collection
initializes tracing storage and reclaims no objects. Estimates exclude transient
stack/spill buffers, OS files, native handles and process-global stdio storage;
they are not exact allocator peaks or RSS.

Rust startup/drop takes 49.715 /
50.562 microseconds versus
116.837 /
116.669 for the oracle, with only one
1,000-state startup sample in each run. Rust requests 240 allocations,
62 reallocations and 39,122 bytes per state in both runs. Compared with the
retained process milestone, the two extra builtin names add two allocations and
eleven requested bytes; this is a historical counter comparison, not a paired
startup timing claim. Runtime compile/drop measurements and all twenty-two
frontend scaling fixtures remain in the raw data.

Raw data: [first JSON](2026-10-25-rust-lua-clock-rename.json), [first TSV](2026-10-25-rust-lua-clock-rename.tsv),
[first build log](2026-10-25-rust-lua-clock-rename.build.log), [confirmation JSON](2026-10-25-rust-lua-clock-rename-confirm.json),
[confirmation TSV](2026-10-25-rust-lua-clock-rename-confirm.tsv), [confirmation build log](2026-10-25-rust-lua-clock-rename-confirm.build.log).
Source digest: `3c4bd566dbf16b233a2c483da690b1f565c169ea4c9b91711bd6b763b43db579`.

The [validation report](../testing/2026-10-25-rust-lua-clock-rename.md) records the new official sort pass,
clock comparisons and rename/create races. Exact live/peak allocation counts,
RSS, cycles, other Rust engines, terminal/device behavior, cross-volume timing
and full library workloads remain unmeasured. The standalone engine, later
dialects, caches, native host APIs, JIT and integrations remain unfinished.
