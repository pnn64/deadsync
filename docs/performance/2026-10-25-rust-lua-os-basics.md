# Lua OS basics measurements

The harness measures 41 runtime and twenty-two frontend workloads with five
paired samples in alternating engine order, followed by a second run. The oracle
is vendored official Lua 5.1.5 on Windows x86-64 MSVC, static CRT and C locale.
The shared machine and workspace are not CPU-isolated. These are compatibility
feature baselines; no overall execution speedup is claimed.

`os_difftime` calls a local alias 10,000 times with `i+.75,i-1`, accumulates
10,000 and repeats fifty complete scripts per sample. It exercises numeric
validation, fractional truncation, native calls and accumulation. `os_getenv`
looks up an absent 27-byte name 1,000 times and repeats thirty scripts per
sample. Each engine first verifies the return value outside timing. The latter
measures this process environment and a missing name; positive lookups,
different environment sizes, encodings and concurrent host changes can differ.
Rust uses the OS process environment; the CRT oracle uses its own environment
representation. These results do not establish equivalent costs for every host.

All durations are milliseconds per complete script. Each engine duration is its
median; the ratio is the median of paired ratios, which can differ from the ratio
of the two medians. Each cell gives first run / confirmation.

| Workload | Rust ms | C Lua 5.1 ms | Paired Rust/C duration |
| --- | ---: | ---: | ---: |
| `os_difftime` | 1.481 / 1.422 | 0.566 / 0.582 | 2.597 / 2.425 |
| `os_getenv` | 0.640 / 0.602 | 4.107 / 3.973 | 0.157 / 0.152 |
| `constant_left` | 1.151 / 1.129 | 0.278 / 0.277 | 4.131 / 4.089 |
| `loadfile_text` | 0.143 / 0.152 | 0.126 / 0.134 | 1.143 / 1.142 |

The new `difftime` and environment kernels emit 20
and 25 instructions respectively; each has
nine entry registers. Both runs use identical source digests and per-file hashes.
All 68 recorded source/configuration hashes match the retained tree.
The unchanged `constant_left` kernel still emits 21 instructions and eleven
entry registers. There is no paired parent-engine comparison in this feature
milestone. The broad results include 5/41 and 5/41 workloads
with paired ratios below one; substantial speed work remains.

The benchmark allocator observes Rust/global-allocator requests, including calls
made through the Rust reference binding; it does not count native C allocations.
Warmed `os_difftime` has no allocation or reallocation requests. `os_getenv`
has 1,000 allocations and 56,000 requested bytes per script: one temporary
allocation per Windows name lookup. It has no reallocations. Counts agree in
every sample and both runs. The current safe standard-library API retains that
temporary conversion; these are not retained heap bytes.

Startup/drop takes 61.122 /
61.970 microseconds per Rust VM,
compared with 128.655 /
133.495 for the oracle.
Each run has only one 1,000-state startup sample. Rust requests 236 allocations,
60 reallocations and 38,780 bytes per startup/drop. Relative to the retained
previous milestone this adds six allocations, one reallocation and 695 requested
bytes for the OS module. These deterministic counts are a historical comparison,
not a paired startup timing claim.

Heap estimates agree across both runs:

| Workload | Phase | Live estimate B | Reserved estimate B | Observed peak B | Collections | Reclaimed objects |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| `os_difftime` | `before_collect` | 18,755 | 24,307 | 24,307 | 0 | 0 |
| `os_difftime` | `after_collect` | 22,723 | 28,275 | 28,275 | 1 | 0 |
| `os_getenv` | `before_collect` | 18,967 | 24,503 | 24,503 | 0 | 0 |
| `os_getenv` | `after_collect` | 22,935 | 28,471 | 28,471 | 1 | 0 |

Each kernel retains three standard-stream userdata objects. The first explicit
collection initializes reusable tracing storage, explaining the increase despite
zero reclaimed objects. Estimates cover VM-owned storage, not exact allocator
peaks, process RSS or temporary standard-library environment conversions.

I/O and OS file paths now share borrowed path conversion when byte encoding
permits it. This removes the prior unconditional owned path copy in file opening.
Ordinary `loadfile_text` scripts request 52 allocations and 35 reallocations,
compared with the historical 53/35, saving one allocation and 88 requested bytes
for this fixture path. Samples crossing collection thresholds retain the same
one-allocation difference. These counts agree in both current runs.
The full `loadfile_text` pipeline remains in the harness; filesystem and source
parsing work dominate its timing. Allocation details for historical and current
samples are retained in JSON; repeated dynamic loading can cross GC thresholds.

All runtime kernels retain compile/drop measurements, and the twenty-two
frontend fixtures retain scaling, layout and allocation samples. Raw data:
[first JSON](2026-10-25-rust-lua-os-basics.json), [first TSV](2026-10-25-rust-lua-os-basics.tsv),
[first build log](2026-10-25-rust-lua-os-basics.build.log),
[confirmation JSON](2026-10-25-rust-lua-os-basics-confirm.json),
[confirmation TSV](2026-10-25-rust-lua-os-basics-confirm.tsv),
[confirmation build log](2026-10-25-rust-lua-os-basics-confirm.build.log).
Source digest: `23fcc7fbd2b1959c7026bd327e0458fc5fa3da8b582a25cccb6a9425bafa5735`.

File-removal and temporary-name correctness are covered by
[differential tests](../testing/2026-10-25-rust-lua-os-basics.md); this report adds no isolated throughput
claim for those operations. Exact live/peak allocation accounting, RSS, cycles,
native C allocation counts, other Rust engines and full library workloads remain
unmeasured. Full Lua compatibility, later dialects, persistent caches, native host
callbacks, JIT and gameplay integrations remain unfinished.
