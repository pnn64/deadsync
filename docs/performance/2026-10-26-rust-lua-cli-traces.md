# Lua 5.1 standalone context measurements

Five alternating paired samples and a separate confirmation cover 66 runtime and
22 frontend workloads. The reference is vendored official Lua 5.1.5 through the
development binding on Windows x86-64 MSVC, static CRT, binary64 and initial C
locale. The shared host is not CPU-isolated. No paired before/after speed claim
against the previous commit is established.

standalone_entry adds a context to the existing three-argument sum fixture. A
sample repeats 10,000 complete calls. Compilation, main-context creation and the
initial correctness/warmup call are excluded; per-entry handler creation, GC safe
points, register entry and cleanup are included. C uses the same typed binding
call as entry_args, without the extra standalone main context. This measures the
chosen adapters and context cost; it is not an equal-context C CLI throughput
comparison or isolated interpreter-dispatch measurement.

Median nanoseconds per complete call/script, first / confirmation. Ratios are
medians of five paired Rust/C durations:

| Workload | Rust ns | C binding ns | Rust/C duration |
| --- | ---: | ---: | ---: |
| `entry_args` | 149 / 134 | 143 / 128 | 1.042 / 1.063 |
| `standalone_entry` | 256 / 238 | 139 / 122 | 1.892 / 1.876 |
| `debug_hook` | 81,180 / 82,490 | 36,150 / 35,430 | 2.189 / 2.268 |
| `function_dump` | 307,110 / 274,620 | 117,010 / 102,340 | 2.657 / 2.683 |
| `dump_load` | 2,114,350 / 1,852,150 | 415,560 / 370,010 | 5.219 / 4.912 |
| `concat_chain_128` | 942,620 / 859,350 | 180,990 / 161,720 | 5.208 / 5.308 |

Rust GlobalAlloc counts per complete call/script; state measures startup/drop:

| Workload | Run | Allocations | Reallocations | Requested B |
| --- | --- | ---: | ---: | ---: |
| `entry_args` | first | 0.000000 | 0.000000 | 0.000000 |
| `entry_args` | confirmation | 0.000000 | 0.000000 | 0.000000 |
| `standalone_entry` | first | 0.000000-0.000200 | 0.000000-0.003500 | 0.000000-20.711200 |
| `standalone_entry` | confirmation | 0.000000-0.000200 | 0.000000-0.003500 | 0.000000-20.711200 |
| `state` | first | 243.000000 | 63.000000 | 39,651.000000 |
| `state` | confirmation | 243.000000 | 63.000000 | 39,651.000000 |

Counts exclude native C allocations and allocator metadata. Requested bytes are
repeated requests, not live memory. A fresh message-handler closure is a logical
allocation per standalone entry and reuses native-arena slots after collection;
logical object creation is distinct from requesting memory from GlobalAlloc.
The main context is rooted until completion. Native closure variants reuse the
existing arena and environment tracing. One u64 context handle is added to the VM
header. No per-instruction or frame field is added; Value remains 16 bytes and the
x86-64 frame remains 64 bytes. All 87 previous instruction/register layouts match.

First-run heap observations after samples and explicit full collection:

| Workload | Phase | Live B | Reserved B | Observed peak B | Collections | Reclaimed |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| `entry_args` | before_collect | 18,387 | 23,827 | 23,827 | 0 | 0 |
| `entry_args` | after_collect | 23,827 | 29,267 | 29,267 | 1 | 0 |
| `standalone_entry` | before_collect | 50,835 | 127,955 | 127,955 | 48 | 49330 |
| `standalone_entry` | after_collect | 23,995 | 127,955 | 127,955 | 49 | 50001 |

Capacity estimates exclude VM headers, OS/CRT storage, transient compiler/export
buffers and allocator metadata. They are not exact allocator peaks or process RSS.
Counts include warmup and are cumulative. Confirmation snapshots remain in raw
data. C allocation counts, RSS, CPU cycles, other Rust engines, full CLI startup
and broader host-call scaling remain unmeasured.

Both runs have source digest e31d07ef939136faf8ffab134c82fe4ef2e8e52536fc39474e4000d414c53ad6; all recorded final hashes match
the tree. Raw data: [JSON](2026-10-26-rust-lua-cli-traces.json), [TSV](2026-10-26-rust-lua-cli-traces.tsv),
[build log](2026-10-26-rust-lua-cli-traces.build.log), [confirmation JSON](2026-10-26-rust-lua-cli-traces-confirm.json),
[confirmation TSV](2026-10-26-rust-lua-cli-traces-confirm.tsv), [confirmation build log](2026-10-26-rust-lua-cli-traces-confirm.build.log).
The [validation report](../testing/2026-10-26-rust-lua-cli-traces.md) records 414 tests, 140 process probes,
17/18 official-file passes in both engines and 60/66 CLI matches with all remaining
differences retained. Full standalone compatibility and the engine/integration
objective remain open.
