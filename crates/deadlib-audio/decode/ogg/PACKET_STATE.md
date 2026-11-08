# Index queries and stream storage, 2026-10-28

This pass starts at `a1b778586`, following the
[page metadata measurements](PAGE_WORK.md). Three changes target packet-existence
checks, stream selection and storage of the last known granule.

Granule search needs to know whether a page completes any packet, rather than
how many it completes. It stops at the first terminating lace instead of
computing the full count. The explicit empty-lacing case remains, preserving
nil EOS landmarks and excluding nonempty pages with no completed packet.

`OggIndex::stream` checks the first stream, then the last, before scanning the
interior. This keeps the usual first-stream lookup short and makes selection of
the newest chained stream constant-time. Interior and missing queries still
use the same immutable list; each serial is unique after validation. No map,
cache, reordered public entries or allocation is introduced.

The demultiplexer uses Ogg's reserved `u64::MAX` unknown-granule value for its
private absence state. This removes the stored `Option<u64>` tag; the format
never treats that value as a known granule. It converts the stored marker to the
existing validator input at the call boundary. Unknown and explicitly ignored
zero-span pages leave the last known value untouched, and reset restores the
absence state. Capture, validation and public packet metadata retain their
existing `Option<u64>` interfaces.

On this Windows x64 build, private stream slots shrink from **72 to 64 bytes**.
There are no new fields, dependencies, static tables or unsafe blocks.
Cursor-arithmetic, smaller-bookmark, capture-granule and zero-event-counter
experiments were discarded because their CPU results did not establish a
benefit. The original pause cursor, capture layout and counter updates remain.

## CPU and allocations

The focused granule comparison isolates the packet-existence change. Cycle
columns are medians across eight workers; paired throughput is the median of
each worker's before/after ratio, which avoids comparing different workers'
clock conditions.

| Query input | Before cycles/query | After cycles/query | Paired throughput | Improved trials |
| --- | ---: | ---: | ---: | ---: |
| Zero-byte packets | 8,124.50 | 7,535.45 | 1.072x | 8/8 |
| One-byte packets | 12,509.36 | 12,129.52 | 1.044x | 8/8 |
| Seventeen-byte packets | 87,868.59 | 87,508.05 | 1.006x | 7/8 |

The last result is small enough to treat cautiously. In the full-suite isolated
comparison, zero- and one-byte granule searches improve by 2.0%, while the
seventeen-byte case takes 10.7% more cycles. The focused result does not imply
that CRC-heavy queries or every compiled calling context improve.

The isolated endpoint change (`query-any` to `query-lookup`) gives:

| Operation, with 513 streams | Before cycles/op | After cycles/op | CPU throughput |
| --- | ---: | ---: | ---: |
| Last-stream lookup | 593.24 | 2.86 | 207.790x |
| Packet-start query in the last stream | 808.16 | 76.21 | 10.604x |
| First-stream lookup | 2.81 | 2.19 | 1.286x |
| Middle-stream lookup | 305.90 | 294.11 | 1.040x |
| Missing-stream lookup | 603.98 | 552.02 | 1.094x |

The stored-granule change (`query-lookup` to `query-streams`) reduces the
513-stream demux probe's peak allocation from **73,728 to 65,536 bytes** and
cumulative allocated bytes from **147,168 to 130,816**, both **11.1% lower**.
It still makes one allocation, eight reallocations and one free. All paired
allocation/reallocation/free counts match throughout the suite; allocated and
peak bytes never increase. Other capture and query allocations remain the same.
That isolated demux control takes 4.6% more cycles, so the measured benefit here
is fewer allocated bytes rather than a claim of faster demux on every fixture.

With all three changes, the full-suite medians are:

| Operation | Baseline cycles/op | Final cycles/op | CPU throughput |
| --- | ---: | ---: | ---: |
| Zero-byte-packet granule search | 7,493.88 | 6,845.66 | 1.095x |
| One-byte-packet granule search | 11,695.24 | 11,194.52 | 1.045x |
| Last-stream lookup, 513 streams | 591.06 | 2.90 | 203.814x |
| Packet-start query, 65 streams | 199.28 | 76.72 | 2.598x |
| Packet-start query, 513 streams | 782.56 | 82.63 | 9.471x |

Slower controls remain in the record: the unchanged continuation-fragment loop
rises from 70.92 to 89.62 cycles (+26.4%), the maximum-page seven-byte capped
reader by 19.1%, the 4 KiB encoder by 19.0%, and the unchanged 65,025-byte CRC
probe by 16.2%. The combined seventeen-byte granule search is 4.1% slower. These
observations constrain the performance claim to the named queries and storage
reduction. Production source grows by eleven lines.

## Measurement boundaries

The worker covers 146 workloads. New stream probes select the first, middle,
last and missing serial with 1, 65 and 513 indexed streams. Capture probes
include both a pause after every packet and a pause after every sixteen packets.
The latter keeps a live cursor across a batch; full-page capture remains a
separate control. Three granule-search probes query resident dense streams with
zero-, one- and seventeen-byte packet bodies, cycling numeric targets.

Six runtime-policy probes demux or capture the existing chained fixtures with
both compatibility options supplied through `black_box`, then observe the
counters. Valid pages contribute zero events. This keeps the policy and counter
work observable to the compiler instead of letting it specialize an unused
default-policy counter away. These remain controls for the stored-granule
change. Construction, iteration, callbacks and cleanup are
timed; input creation, page verification and index creation for lookup probes
are outside timing.

Eight trials run each of four saved workers once, rotating the first stage so
each runs first twice. Each observation is the median CPU-cycle sample from
eleven batches after warmup. `QueryThreadCycleTime` measures the benchmark
thread, pinned to logical CPU 2; allocation counters come from a separate
untimed operation. The host is a shared Windows x64 Xeon E5-2696 v4, with rustc
1.99.1 / LLVM 23.1.1, optimization level 3, LTO, benchmark debug symbols and static
CRT. The machine is neither isolated nor frequency locked. These resident-input
measurements do not establish full audio decode or filesystem throughput.

The [record](validation/2026-10-28-packet-state.json) includes source, executable,
compiler, flags, profiles and probe hashes, medians, minima, maxima, median
absolute deviations and nanoseconds. All 4,672 CPU/heap observations are in the
[raw data](validation/2026-10-28-packet-state.tsv).

The [focused query worker](validation/2026-10-28-granule-pair.rs) links the
baseline and packet-existence snapshots as distinct crates in one process.
It alternates which query runs first in each batch and uses the same 32 targets
in every batch. Setup compares full packet counts, query results and source
positions. Eight process trials provide 48 observations in the
[paired data](validation/2026-10-28-granule-pairs.tsv), with commands, library,
executable, source and helper hashes in the
[paired record](validation/2026-10-28-granule-pairs.json). This isolates the
smaller query change from variation between the full-suite processes.

## Correctness

All **55 Ogg tests** and **113 regular Vorbis tests** pass; 87 external/manual
Vorbis tests remain ignored. Ogg includes the independent libogg oracle tests,
skipping only manual synchronization-golden regeneration. The clean reference
is pinned to `be05b13e98b048f0b5a0f5fa8ce514d56db5f822`. Corpus checks cover
38 files, 8,733 pages and 119,144 packets, plus 242,352 exact synchronization
operations and the full transport version/flag matrix. New cases distinguish
the reserved unknown value from `u64::MAX - 1`, retain the previous timeline
across unknown spans, verify allowed/rejected EOS decreases and reset to zero
after success or failure. All-target/all-feature Ogg Clippy with warnings denied,
formatting and Python compilation pass.

The new [capture-state probe](validation/2026-10-28-capture-state-probe.rs)
compares **1,440 configurations** of page targets, starting pages, raw/indexed
capture, pause intervals, mixed draining, limits and compatibility options.
It records complete packet bytes and metadata, every result, pending bytes,
ended state, compatibility counters, terminal failure, finish and reset. It
also checks 584 stream lookups with nonmonotonic serials and five missing queries,
including reference identity, plus 126 granule-search results and source
positions. Both executions produce **78,020,534 identical bytes**. The existing full demux/capture/index and packet-origin probe compares
784 configurations and produces **3,800,798 identical bytes**.

The [protocol record](validation/2026-10-28-packet-state-protocol.json) pins
commands, source/library/executable hashes, layouts, trace sizes and checksums.
The [audit](validation/2026-10-28-packet-state-checks.json) pins tested sources,
tests, commands, logs, benchmark controls and recomputed raw summaries.

From the repository root, choose a fresh ignored directory:

```powershell
python crates/deadlib-audio/decode/scripts/benchmark_ogg_kernels.py --root target/ogg-packet-repeat --capture before --source-revision a1b778586
python crates/deadlib-audio/decode/scripts/benchmark_ogg_kernels.py --root target/ogg-packet-repeat --capture after --stage before --stage after --runs 8 --output target/ogg-packet-repeat/results.tsv
```

For isolated comparisons, capture `2d6e81461`, `73fa24039` and `d8336a3f1` as
additional stages with `--source-revision`. The focused worker's exact `rustc`
commands are in its paired record; both libraries use distinct crate names,
optimization level 3, one codegen unit, LTO, debug symbols, aborting panics and
the static CRT.

Saved workers and traces remain under ignored `target/ogg-packet-state-20261028/`.

The subsequent [index-construction and short-page measurements](INDEX_BUILD.md)
cover stable page grouping, reused demux stream positions and short lacing sums.
