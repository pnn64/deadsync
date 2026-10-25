# Lua calendar table allocation measurements

Calendar result construction reserves eight or nine record entries and enough
hash buckets for the complete field set. The GC receives the initial capacity
charge before insertion. General tables, insertion order and subsequent growth
use their existing representations. No new unsafe code or dependency is added.

## Paired parent/current comparison

The [reproducible harness](../../scripts/benchmark_rust_lua_records.py) extracts
the committed parent `6f280812ecd0b4ddec57516da9ffab25ed082b6d` with `git archive`, compiles parent
and current independent-engine libraries under distinct crate names, and links
both into one executable. Both use identical Rust flags: optimization level 3,
sixteen codegen units, debug info level 1, aborting panics, fat LTO at the final
link and static MSVC CRT. This is a separate harness from the Cargo C-reference
benchmark. No C Lua library is involved in this comparison.

Each workload has nine paired samples alternating parent/current order, with a
second independently built run. Compilation, VM setup and scalar correctness
checks occur outside timing. `os_date_table` builds and reads 1,000 UTC tables
per script, with fifty scripts per sample. The stopped-GC variant uses thirty
scripts per sample, with manual full collection and another explicit stop
outside each timed window. The extension fixture adds one `extra` field to each
result and checks the resulting sum, with fifty scripts per sample. The unchanged
arithmetic control also runs fifty scripts per sample.

Durations are milliseconds per complete script, first run / confirmation.
Each duration is its engine's median; the ratio is the median of paired ratios.

| Workload | Parent ms | Current ms | Current/parent duration |
| --- | ---: | ---: | ---: |
| `os_date_table` | 1.363 / 1.381 | 0.975 / 0.956 | 0.729 / 0.688 |
| `os_date_table_gc_stopped` | 1.328 / 1.411 | 0.905 / 0.973 | 0.690 / 0.681 |
| `os_date_table_extended` | 1.414 / 1.457 | 1.281 / 1.207 | 0.907 / 0.848 |
| `constant_left` | 0.875 / 0.800 | 0.878 / 0.797 | 0.994 / 0.987 |

The new path improves the measured construction kernels with automatic GC and
with GC stopped. The extension fixture also improves in both final runs, while
the arithmetic control remains close to unchanged. The shared machine is not
CPU-isolated; absolute timings and extension ratios vary. This establishes a
workload-specific improvement, not overall engine performance parity.

## Allocations and the extension tradeoff

For each 1,000-table script with automatic GC, both paired and C-reference
harnesses record the following Rust requests in every sample of both runs:

| Counter | Parent | Current |
| --- | ---: | ---: |
| Allocation calls | 2,000 | 2,000 |
| Reallocation calls | 3,000 | 0 |
| Requested bytes | 992,000 | 352,000 |

Requested bytes fall by 64.5%. Each ordinary result uses 288 entry bytes and
64 bucket bytes, versus 512 entry bytes and 64 bucket bytes previously: a 38.9%
payload-capacity reduction. These exclude the table object and arena/tracing
storage. Reallocation requests count their complete new size, so requested bytes
are neither retained bytes nor an allocator peak. The GC-stopped fixture also
records occasional arena growth and control allocations; its raw counters retain
those requests rather than reporting only the two table buffers.

The parent's canonical counters are retained in the
[calendar feature report](2026-10-25-rust-lua-calendar.md); the direct paired
harness measures both implementations in this milestone.

Exact entry capacity has a cost when extending a result. A tenth field grows
the entry vector from nine to eighteen slots. That fixture uses 640 payload
bytes versus the parent's 576, requests 2,000 allocations and 1,000 reallocations
totalling 928,000 bytes per script, and completes more automatic collections.
It still runs faster in the retained pairs, but the increased retained capacity
and GC pressure are real. No general table-reservation policy is changed.

Heap snapshots agree across the two paired runs:

| Workload | Engine | Reserved B | Observed peak B | Collections | Reclaimed |
| --- | --- | ---: | ---: | ---: | ---: |
| `os_date_table` | parent | 49,754 | 74,522 | 7274 | 450980 |
| `os_date_table` | current | 71,578 | 71,930 | 4797 | 450906 |
| `os_date_table_gc_stopped` | parent | 3,176,764 | 20,456,784 | 25 | 271047 |
| `os_date_table_gc_stopped` | current | 3,176,764 | 13,736,784 | 19 | 271047 |
| `os_date_table_extended` | parent | 49,924 | 74,692 | 7274 | 450980 |
| `os_date_table_extended` | current | 53,764 | 75,236 | 7912 | 450976 |

The automatic-GC rows cover 451,000 table results including warmup; snapshots can
land at different points in a collection cycle, so final reserved/live sizes
must not be interpreted as per-result footprint. Stopped-GC snapshots include
warmup collections and nine manual cycles, with 30,000 tables between manual
collections. The lower stopped-GC peak reflects reduced payload storage. These
are VM capacity estimates, not exact allocator live/peak usage or RSS.

## Reference harness and provenance

The canonical benchmark retains 48 runtime and twenty-two frontend fixtures,
five alternating Rust/C pairs per runtime fixture, and a confirmation run.
The oracle remains vendored official Lua 5.1.5 on Windows x86-64 MSVC, static
CRT, binary64 and C locale. Date output fixtures request UTC. Compilation and
correctness are outside execution timing; native conversion, table insertion,
interpreted work and automatic collection remain inside.

| Workload | Rust ms | C Lua 5.1 ms | Rust/C duration |
| --- | ---: | ---: | ---: |
| `os_date_table` | 0.939 / 0.894 | 0.817 / 0.784 | 1.152 / 1.145 |
| `os_date_format` | 2.005 / 1.944 | 1.884 / 1.854 | 1.076 / 1.063 |
| `os_time` | 0.579 / 0.576 | 0.379 / 0.368 | 1.549 / 1.521 |
| `constant_left` | 1.024 / 0.889 | 0.255 / 0.217 | 3.983 / 4.061 |

`os_time` retains zero warmed Rust allocation/reallocation requests. Startup
allocation counters are unchanged at 242 allocations, 63 reallocations and
39,642 requested bytes per state. Calendar-table bytecode remains 27 instructions
and nine entry registers; the extension fixture has 33 and nine. The arithmetic
control remains 21 instructions and eleven registers. Compiler output is
unchanged by this allocation milestone.

Both canonical runs have digest `6a94af14622b8c07e5659ebbc68cab096606d29c000df476068eaa23cae287b3`. All recorded current
source/configuration hashes match the final tree; the paired harness also records
all 46 archived parent implementation hashes and each executable's SHA-256.
Its emitted Rust source, flags and raw counters are retained for reproduction.

```sh
python scripts/benchmark_rust_lua_records.py --parent 6f280812e --output docs/performance/2026-10-25-rust-lua-calendar-records-compare.json
python scripts/benchmark_rust_lua.py --output docs/performance/2026-10-25-rust-lua-calendar-records.json
```

Raw paired data: [JSON](2026-10-25-rust-lua-calendar-records-compare.json), [TSV](2026-10-25-rust-lua-calendar-records-compare.tsv),
[Rust harness](2026-10-25-rust-lua-calendar-records-compare.rs), [build log](2026-10-25-rust-lua-calendar-records-compare.build.log),
[confirmation JSON](2026-10-25-rust-lua-calendar-records-compare-confirm.json), [confirmation TSV](2026-10-25-rust-lua-calendar-records-compare-confirm.tsv),
[confirmation harness](2026-10-25-rust-lua-calendar-records-compare-confirm.rs), [confirmation build log](2026-10-25-rust-lua-calendar-records-compare-confirm.build.log).
Canonical data: [JSON](2026-10-25-rust-lua-calendar-records.json), [TSV](2026-10-25-rust-lua-calendar-records.tsv), [build log](2026-10-25-rust-lua-calendar-records.build.log),
[confirmation JSON](2026-10-25-rust-lua-calendar-records-confirm.json), [confirmation TSV](2026-10-25-rust-lua-calendar-records-confirm.tsv),
[confirmation build log](2026-10-25-rust-lua-calendar-records-confirm.build.log).

The [validation report](../testing/2026-10-25-rust-lua-calendar-records.md) records 341 passing tests,
93 process cases, the unchanged official-suite stops and a separately confirmed
GC restart mismatch. Exact allocator peaks, RSS, cycles, native CRT allocations,
other Rust engines and full library workloads remain unmeasured. The standalone
engine and the complete Lua 5.1–5.5/integration objective remain unfinished.
