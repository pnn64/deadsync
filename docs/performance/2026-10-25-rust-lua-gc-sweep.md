# Lua phased sweeping measurements

The harness retains 50 runtime and twenty-two frontend workloads, five
alternating paired Rust/C samples and a confirmation run. The oracle is vendored
official Lua 5.1.5 on Windows x86-64 MSVC, static CRT, binary64 and C locale.
The shared host is not CPU-isolated. There is no paired parent timing claim.

`gc_steps` stops collection, retains 256 one-element tables and creates 2,048
temporary tables. It makes 64 explicit zero-budget step calls, sums the retained
values to 32,896, then requests full collection. Twenty scripts run per sample.
Compilation and scalar correctness checks occur outside timing; construction,
all step calls, interpreted work and final full collection remain inside.
This measures complete-script throughput, not individual pause latency.

Durations are milliseconds per complete script, first run / confirmation.
Each engine duration is its median; ratios are medians of paired ratios.

| Workload | Rust ms | C Lua 5.1 ms | Rust/C duration |
| --- | ---: | ---: | ---: |
| `gc_steps` | 0.366 / 0.389 | 0.335 / 0.360 | 1.078 / 1.076 |
| `gc_restart` | 1.229 / 1.209 | 1.496 / 1.500 | 0.819 / 0.802 |
| `os_date_table` | 1.012 / 1.049 | 0.878 / 0.874 | 1.141 / 1.191 |
| `userdata_finalize` | 1.153 / 1.211 | 0.768 / 0.808 | 1.475 / 1.488 |
| `constant_left` | 1.029 / 0.941 | 0.247 / 0.252 | 4.206 / 4.145 |

The collectors do different work per step. Rust currently marks and processes
weak references atomically, then sweeps a slot budget; PUC Lua divides its work
among incremental phases. Equal final scalar results do not establish equivalent
phase counts, pauses or collection timing. The restart kernel remains faster
than C here while other controls remain slower. No overall parity or latency
improvement is inferred from these means.

Steady `gc_steps` samples request 257 Rust allocations, six reallocations and
24,512 bytes per script. Initial arena growth adds requests in the first sample.
Raw rows retain all requests. Calendar results still request 2,000 allocations,
zero reallocations and 352,000 bytes per 1,000-table script in every sample.
Counters cover Rust `GlobalAlloc`; native C allocations and allocator headers
are excluded.

The new collector state reuses the mark bitmap and stores one protection flag
per arena, plus phase/cursor state in the collector. New objects and resurrected
interned strings are marked during sweeping. No per-slice job vector, dependency
or unsafe code is added. `Value` remains sixteen bytes and the x86-64 frame-size
assertion remains 64 bytes. Arena/VM header size is not included in the capacity
estimates below; protection flags do add header storage.

Heap observations agree across both runs:

| Workload | Phase | Live B | Reserved B | Observed peak B | Collections | Reclaimed |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| `gc_steps` | before_collect | 70,843 | 444,403 | 464,883 | 405 | 232548 |
| `gc_steps` | after_collect | 27,747 | 423,923 | 464,883 | 406 | 232805 |
| `gc_restart` | before_collect | 137,969 | 482,401 | 515,329 | 462 | 560565 |
| `gc_restart` | after_collect | 69,681 | 465,857 | 515,329 | 463 | 561153 |

Snapshots follow all execution samples and then an explicit full collection;
three standard-stream userdata remain. Counts are cumulative, including warmup.
New objects can survive the active sweep and die in a later cycle. These are
capacity estimates, not exact live/peak allocator usage or RSS. Sparse arena
slots, atomic graph/weak passes, cache rebuilding, string-cluster repair and
native file destruction can still contribute cost. Slot-budget tests establish
scan bounds, not a wall-clock pause bound.

Startup/drop requests the unchanged 242 allocations, 63 reallocations and
39,642 bytes per Rust state. All seventy-one parent program layouts match;
the added step fixture emits 51 instructions and
9 entry registers.
Both runs have source digest `a1ca9e53cd11101ab65f0a903a5d4740b7b9e56af8344cb8e5e4c07110eb05d4`; all 76 recorded
source/configuration hashes match the final tree.

Raw data: [JSON](2026-10-25-rust-lua-gc-sweep.json), [TSV](2026-10-25-rust-lua-gc-sweep.tsv), [build log](2026-10-25-rust-lua-gc-sweep.build.log),
[confirmation JSON](2026-10-25-rust-lua-gc-sweep-confirm.json), [confirmation TSV](2026-10-25-rust-lua-gc-sweep-confirm.tsv),
[confirmation build log](2026-10-25-rust-lua-gc-sweep-confirm.build.log).
The [validation report](../testing/2026-10-25-rust-lua-gc-sweep.md) records 356 tests, 109 process
probes and three corrected parent process cases.

Per-step latency distributions, exact allocator peaks, RSS, cycles, native C
allocation counts and other Rust engines remain unmeasured. Incremental marking,
bounded weak/cache work and callback batches remain unfinished, alongside the
full standalone Lua 5.1–5.5 engine, caches, host APIs, JIT and integrations.
