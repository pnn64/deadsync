# Lua 5.1 standalone entry measurements

Five alternating paired samples and a separate confirmation cover 65 runtime and
22 frontend workloads. The reference is vendored official Lua 5.1.5 through the
legacy development binding on Windows x86-64 MSVC, static CRT, binary64 and initial
C locale. The shared host is not CPU-isolated. No paired before/after speed claim
against the previous commit is established.

The new entry_args workload calls a retained chunk with three numeric values and
checks its sum of six. Each sample repeats 10,000 complete calls. Compilation and
the initial correctness/warmup call are excluded. Rust uses execute_with_args,
including validation, program-cache lookup, shared register entry and result
cleanup; C uses the binding's typed three-value Function::call. These results
include the respective host adapters and are not isolated interpreter dispatch
measurements. Public call can also invoke a retained function directly; this
fixture does not separately measure that path or Rust callback registration.

Median nanoseconds per complete script, first / confirmation. Ratios are medians
of five paired Rust/C durations:

| Workload | Rust ns | C binding ns | Rust/C duration |
| --- | ---: | ---: | ---: |
| `entry_args` | 190 / 178 | 147 / 166 | 1.136 / 1.036 |
| `byte_concat` | 3,275,290 / 3,379,716 | 511,006 / 515,386 | 6.386 / 6.588 |
| `function_dump` | 366,580 / 310,920 | 133,870 / 122,300 | 2.876 / 2.675 |
| `dump_load` | 2,322,180 / 2,178,320 | 451,920 / 443,800 | 5.041 / 4.999 |
| `concat_chain_128` | 1,005,740 / 1,084,940 | 192,910 / 199,580 | 5.210 / 5.399 |

Allocation counters per complete call/script; state measures startup and drop:

| Workload | Run | Allocations | Reallocations | Requested B |
| --- | --- | ---: | ---: | ---: |
| `entry_args` | first | 0.000 | 0.000 | 0.000 |
| `entry_args` | confirmation | 0.000 | 0.000 | 0.000 |
| `state` | first | 243.000 | 63.000 | 39,651.000 |
| `state` | confirmation | 243.000 | 63.000 | 39,651.000 |

These GlobalAlloc counters exclude native C allocation and allocator metadata.
Requested bytes include repeated requests, not live memory. Existing storage is
reused after warmup; raw host copies of object identities still require rooting
across other VM executions or collections. All 86 previous shared-VM instruction
and entry-register layouts remain unchanged. No stored VM field is added; Value
remains 16 bytes and the x86-64 frame remains 64 bytes.

First-run entry_args heap observations after samples and explicit full collection:

| Phase | Live B | Reserved B | Observed peak B | Collections | Reclaimed |
| --- | ---: | ---: | ---: | ---: | ---: |
| before_collect | 18,387 | 23,827 | 23,827 | 0 | 0 |
| after_collect | 23,827 | 29,267 | 29,267 | 1 | 0 |

Capacity estimates exclude VM headers, OS/CRT storage, transient compiler/export
buffers and allocator metadata; they are not exact allocator peaks or process RSS.
Counts include warmup and are cumulative. Confirmation snapshots remain in the
raw data. C allocation counts, process peak RSS, CPU cycles, other Rust engines,
full CLI startup/option costs and broader host-call scaling remain unmeasured.

Both runs have source digest d4b03440d8c9cc1aa32f40986df77cbcec7ce358ab8b009b7572e2ddab15d9d7; all recorded final hashes match
the tree. Raw data: [JSON](2026-10-26-rust-lua-cli.json), [TSV](2026-10-26-rust-lua-cli.tsv),
[build log](2026-10-26-rust-lua-cli.build.log), [confirmation JSON](2026-10-26-rust-lua-cli-confirm.json),
[confirmation TSV](2026-10-26-rust-lua-cli-confirm.tsv), [confirmation build log](2026-10-26-rust-lua-cli-confirm.build.log).
The [validation report](../testing/2026-10-26-rust-lua-cli.md) records 407 tests, 140 process probes,
17/18 isolated official-file passes in both engines and 32/43 CLI matches with
all remaining mismatches retained. The complete standalone and integration goal
remains open.
