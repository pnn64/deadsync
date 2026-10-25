# Lua GC control measurements

The harness now retains 49 runtime and twenty-two frontend fixtures, five
alternating paired Rust/C samples per runtime fixture and a confirmation run.
The oracle remains vendored official Lua 5.1.5 on Windows x86-64 MSVC, static
CRT, binary64 and C locale. The shared machine is not CPU-isolated.

`gc_restart` stops collection, creates 1,000 tables held both strongly and
through weak values, explicitly collects, releases the strong keeper and creates
10,000 temporary tables. The weak table must become empty and the script returns
1. Ten complete scripts run per sample. Compilation and correctness checks are
outside timing; construction, explicit/full collection, automatic collection and
interpreted loops remain inside. The pre-fix engine returns 0, so parent/current
throughput on this kernel would compare different behavior.

Durations are milliseconds per complete script, first run / confirmation.
Each engine duration is its median; ratios are medians of paired ratios.

| Workload | Rust ms | C Lua 5.1 ms | Rust/C duration |
| --- | ---: | ---: | ---: |
| `gc_restart` | 1.159 / 1.239 | 1.451 / 1.498 | 0.820 / 0.827 |
| `os_date_table` | 0.945 / 0.950 | 0.794 / 0.802 | 1.188 / 1.188 |
| `userdata_finalize` | 1.123 / 1.150 | 0.765 / 0.766 | 1.456 / 1.503 |
| `constant_left` | 0.958 / 0.980 | 0.228 / 0.237 | 4.161 / 4.143 |

The new kernel is faster than C in these samples, with different collector
algorithms and work schedules: Rust completes whole cycles while C performs
incremental phases. This does not establish equivalent pauses, phase behavior,
general GC throughput or overall engine parity. A retained finalizer-error/large-
pause reproducer still differs; see the [validation report](../testing/2026-10-25-rust-lua-gc-controls.md).
Other controls remain slower than C. There is no paired parent timing claim.

The new kernel records four Rust allocation requests per script. Steady samples
request sixteen reallocations and 65,568 bytes per script; initial arena growth
adds requests in the first sample. Raw totals retain that growth. These counters
cover Rust `GlobalAlloc`, excluding native C allocation calls, allocator headers
and exact live/peak memory. The control checkpoints use existing continuation
frame fields without another vector or allocation. `Value` remains sixteen
bytes and the x86-64 frame size assertion remains 64 bytes.

The calendar result fixture retains 2,000 allocations, **zero reallocations** and
352,000 requested bytes per 1,000-table script in every sample of both runs.
All seventy pre-existing program layouts match the parent report; the new
restart fixture uses 46 instructions and
8 entry registers.

The new kernel's Rust heap estimates agree across both runs:

| Phase | Live B | Reserved B | Observed peak B | Collections | Reclaimed |
| --- | ---: | ---: | ---: | ---: | ---: |
| before_collect | 170,001 | 482,401 | 515,329 | 463 | 560201 |
| after_collect | 69,681 | 465,857 | 515,329 | 464 | 561153 |

Snapshots follow all fifty timed scripts plus one warmup, then an explicit full
collection. Three standard-stream userdata remain. Tracing/arena capacities can
remain reserved after objects die. These observations are cumulative VM capacity
estimates, not exact allocator peaks or process RSS; native/CRT memory is excluded.

Rust startup/drop takes 58.987 /
60.434 microseconds versus
133.425 /
123.463 for the oracle, using one
1,000-state sample per run. Rust requests the unchanged 242 allocations,
63 reallocations and 39,642 bytes per state. Runtime compile/drop and all
twenty-two frontend scaling measurements remain in the raw reports.

Both runs have source digest `13c4c61db61d37ec103db4f5af4b96a76d80f6b0faaa1ae0af37b1c60ceca68b`; all 76 recorded
source/configuration hashes match the final tree.
Raw data: [JSON](2026-10-25-rust-lua-gc-controls.json), [TSV](2026-10-25-rust-lua-gc-controls.tsv), [build log](2026-10-25-rust-lua-gc-controls.build.log),
[confirmation JSON](2026-10-25-rust-lua-gc-controls-confirm.json), [confirmation TSV](2026-10-25-rust-lua-gc-controls-confirm.tsv),
[confirmation build log](2026-10-25-rust-lua-gc-controls-confirm.build.log).

Exact allocator peaks, RSS, CPU cycles, native C allocation counts, other Rust
engines, full standard-library workloads and incremental phase/pause parity
remain unmeasured or unfinished. The full standalone Lua 5.1–5.5 engine,
caches, host APIs, optional JIT and song/noteskin integrations remain unfinished.
