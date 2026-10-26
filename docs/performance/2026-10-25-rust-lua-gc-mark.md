# Lua incremental marking measurements

The harness records 51 runtime and twenty-two frontend workloads, five
alternating paired Rust/C samples and a separate confirmation run. The oracle
is vendored official Lua 5.1.5 on Windows x86-64 MSVC, static CRT, binary64 and
C locale. The shared host is not CPU-isolated.

`gc_mark_mutation` stops collection, creates 512 retained parent tables and 512
weakly held children, and begins stepping. Each parent receives a child while
zero-budget steps run every eighth assignment. If collection cleared a weak
child, the script creates a replacement. Full collection precedes the final
sum, 131,328. Ten scripts run per sample. Compilation and scalar checks occur
outside timing; construction, mutations, all steps and full collection remain
inside. Weak fallback counts and collector schedules can differ between engines.
This measures whole-script throughput, not pause latency or identical GC work.

Durations are milliseconds per complete script, first run / confirmation.
Engine durations are medians; ratios are medians of the five paired ratios.

| Workload | Rust ms | C Lua 5.1 ms | Rust/C duration |
| --- | ---: | ---: | ---: |
| `gc_mark_mutation` | 3.538 / 3.154 | 0.525 / 0.456 | 6.800 / 7.009 |
| `gc_steps` | 0.457 / 0.435 | 0.395 / 0.398 | 1.157 / 1.102 |
| `gc_restart` | 1.495 / 1.485 | 1.701 / 1.666 | 0.870 / 0.876 |
| `gc_cycles` | 2.636 / 3.117 | 1.876 / 2.226 | 1.428 / 1.395 |
| `os_date_table` | 1.127 / 1.253 | 1.051 / 0.927 | 1.202 / 1.303 |
| `userdata_finalize` | 1.315 / 1.345 | 0.878 / 0.878 | 1.499 / 1.527 |
| `constant_left` | 0.954 / 1.135 | 0.230 / 0.268 | 4.006 / 4.244 |

The new mutation workload remains substantially slower than C. Incremental
propagation does not establish performance parity, equal step work or bounded
pauses. Existing workloads also include slower results; no overall speedup is
claimed. Historical in-process samples are not paired parent measurements.

A separate [retained-parent diagnostic](2026-10-25-rust-lua-gc-mark-parent.json) runs the same body
100 times inside one Lua function/program, with five alternating process samples.
Median bundle wall time is 621.007 ms for the retained
pre-mark CLI (`06860f340`), 373.933 ms for the current CLI
and 65.122 ms for the reference process runner. The median
paired current/parent ratio is 0.618.
This limited diagnostic includes process startup, compilation, execution, drop
and exit; it opens different libraries from the main benchmark, uses an existing
parent binary, and retains GC-schedule/fallback differences. It is evidence for
this process workload only, not an in-process regression threshold or general
engine speedup.

Steady mutation samples request 2,556 Rust allocations, fourteen reallocations
and 212,512 bytes per script. Initial capacity growth adds reallocations/bytes
in the first sample. Steady `gc_steps` remains 257 allocations, six reallocations
and 24,512 bytes. Calendar tables remain 2,000 allocations, zero reallocations
and 352,000 bytes per 1,000-table script in every sample. Counts cover Rust
`GlobalAlloc`; native C allocations and allocator metadata are excluded.

Heap observations match across both runs:

| Workload | Phase | Live B | Reserved B | Observed peak B | Collections | Reclaimed |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| `gc_mark_mutation` | before_collect | 306,893 | 604,469 | 686,549 | 1711 | 77054 |
| `gc_mark_mutation` | after_collect | 52,517 | 448,661 | 686,549 | 1712 | 78081 |
| `gc_steps` | before_collect | 73,787 | 447,347 | 467,827 | 404 | 232548 |
| `gc_steps` | after_collect | 30,691 | 426,867 | 467,827 | 405 | 232805 |
| `gc_restart` | before_collect | 153,529 | 494,177 | 527,105 | 410 | 560522 |
| `gc_restart` | after_collect | 81,457 | 477,633 | 527,105 | 411 | 561153 |

Snapshots follow all samples and then explicit full collection; counts include
warmup and are cumulative. Three standard-stream userdata remain. Estimates
include retained working buffers/capacities, not exact allocator peaks or RSS.
The earlier sweep report recorded 27,747 / 69,681 live bytes after collecting
`gc_steps` / `gc_restart`, compared with 30,691 / 81,457 here. The phase schedules
and retained working storage differ, so these are not matched-cycle snapshots.

The collector reuses the arena mark bitmaps and its frontier vector. Work tags
distinguish already marked barrier targets from ordinary tracing and refreshes;
there is no per-object color allocation or per-value reference count. A previous
builtin-mark mask adds 32 bytes to the collector header, which the heap estimate
excludes. Startup/drop still requests 242 allocations, 63 reallocations and
39,642 bytes per Rust state. `Value` remains sixteen bytes and the x86-64 frame
size assertion remains 64 bytes. All 72 prior program layouts match. The new
fixture emits 77 instructions and 10 entry registers.

Both benchmark runs have source digest `442885769476f77e75c6238d2899be480e53e1de7234eee1bfbaf505db8a71b2`; all
76 source/configuration hashes match the final tree.
Raw data: [JSON](2026-10-25-rust-lua-gc-mark.json), [TSV](2026-10-25-rust-lua-gc-mark.tsv),
[build log](2026-10-25-rust-lua-gc-mark.build.log), [confirmation JSON](2026-10-25-rust-lua-gc-mark-confirm.json),
[confirmation TSV](2026-10-25-rust-lua-gc-mark-confirm.tsv), [confirmation build log](2026-10-25-rust-lua-gc-mark-confirm.build.log).
The [validation report](../testing/2026-10-25-rust-lua-gc-mark.md) records 364 tests and 111 process probes.

Individual object scans, final root/weak processing, cache rebuilding and callback
batches retain atomic work. Mark-frontier and slot budgets do not bound elapsed
time. Pause distributions, exact allocator peaks, RSS, cycles, native C allocation
counts and other Rust engines remain unmeasured. Exact PUC work units, the full
standalone Lua 5.1-5.5 engine, caches, host APIs, JIT and integrations remain open.
