# Index construction and short pages, 2026-10-28

This pass starts at `7da11090a`, following the
[index-query measurements](PACKET_STATE.md). Three changes replace comparison
sorting, reuse a stream lookup and sum short lacing tables directly.

The ordering pass first checks whether pages are already grouped by stream. If
needed, it counts pages into the fresh private stream ranges, computes stable
destinations in the existing `landmark` fields, then swaps each page into place.
Input offsets increase, so stable grouping by stream gives exactly the former
`(stream, offset)` ordering. Destinations are a permutation of `0..pages.len()`;
each swap places a page in its final slot. The following granule pass assigns
the ranges and landmarks their final values. The pass takes O(pages + streams)
work with no added heap allocation or scratch array.

Index construction now receives the stream position found by demultiplexing.
Both stream arrays grow in physical BOS order, so the position already identifies
the matching index stream. This removes a second reverse serial scan on each
non-BOS page. The public demultiplexer API still returns `Result<(), Error>`;
only the crate-private indexing path returns the existing position. Validation,
packet callbacks, callback failures and page limits keep their previous order.

Lacing sums for fewer than eight segments run directly in the caller. Larger
lacing tables retain the existing word-sum routine outside the caller's hot code.
This avoids a call to the wide routine for short tables without changing the
sum, page-size checks or checksum boundaries.

There are no new dependencies, static tables or unsafe blocks.

## Measurement boundaries

The worker covers 166 workloads. Six new multiplexed-index probes build 64
pages per stream with 2, 16 or 128 streams, in round-robin or changing permutation
order. They include CRC verification, demux, callbacks, allocation, grouping,
granule construction and cleanup; input generation is outside timing. Two
spanning-index probes use sixteen 65,025- or 130,050-byte packets after a header.

Twelve fragmented-header controls supply valid incomplete page prefixes with
1, 17, 128 or 255 laces, in 1-, 7- or 27-byte chunks. Each operation resets a
preallocated decoder, copies the prefix and checks progress while waiting for
the absent body. They retain coverage for rejected decoder-header experiments.
The existing complete-page and body-fragmentation probes remain as controls.

Eight trials run each of four saved workers once, rotating the first stage so
each runs first twice. Each observation is the median of eleven CPU-cycle
batches after warmup. Windows `QueryThreadCycleTime` measures the benchmark
thread, pinned to logical CPU 2; allocator counters come from a separate untimed
operation. The host is a shared Windows x64 Xeon E5-2696 v4, with rustc 1.99.1 /
LLVM 23.1.1, optimization level 3, LTO, benchmark debug symbols and static CRT.
The machine is neither isolated nor frequency locked. These are resident-input
API measurements, without filesystem IO or audio decoding.

The [record](validation/2026-10-28-index-build.json) pins compiler, flags,
profiles, executable and source hashes, with medians, minima, maxima, median
absolute deviations and nanoseconds. All 5,312 CPU/heap observations are in the
[raw data](validation/2026-10-28-index-build.tsv).

## CPU and heap results

For 16 and 128 interleaved streams, stable grouping improves CPU throughput
by **4.7-8.7%** over the baseline. Small two-stream controls range from 1.4%
slower to 1.2% faster. Reusing the demux stream position then improves the
128-stream cases by **21.9-26.5%** over grouping alone, winning all eight
process comparisons in both input orders. Combined 128-stream throughput is
**27.8-34.0%** higher than the baseline. These timings include the entire index
build and its cleanup.

| Workload | Baseline cycles | Grouping | Reused stream | Short sum | Combined CPU throughput |
| --- | ---: | ---: | ---: | ---: | ---: |
| 2 streams, round-robin | 42,075.4 | 41,567.8 | 43,076.9 | 36,190.1 | 1.163x |
| 2 streams, permuted | 44,887.8 | 45,546.2 | 36,615.3 | 40,923.0 | 1.097x |
| 16 streams, round-robin | 485,904.3 | 463,940.7 | 418,778.6 | 422,853.0 | 1.149x |
| 16 streams, permuted | 492,407.1 | 455,394.0 | 409,257.8 | 422,990.2 | 1.164x |
| 128 streams, round-robin | 6,338,831.0 | 5,831,566.0 | 4,609,280.0 | 4,730,335.0 | 1.340x |
| 128 streams, permuted | 5,912,561.5 | 5,607,018.0 | 4,601,268.0 | 4,624,644.0 | 1.278x |
| 65,025-byte spans | 1,748,537.0 | 1,768,840.5 | 1,730,428.5 | 1,713,966.0 | 1.020x |
| 130,050-byte spans | 3,949,573.5 | 3,866,712.5 | 3,949,793.0 | 3,930,257.5 | 1.005x |
| Dense empty-packet index | 51,054.3 | 51,417.9 | 53,852.9 | 47,940.2 | 1.065x |

The large spanning-packet control remains within 0.5% of its baseline CPU cost.
Every allocation, reallocation, free, cumulative byte and peak-byte counter is
**identical across all four stages and all 166 workloads**. For example, the
128-stream index retains 5 allocations, 27 reallocations, 1,342,737 cumulative
allocated bytes and a 671,798-byte peak. No persistent field or scratch array
was added.

A separate [paired worker](validation/2026-10-28-short-lacing-paired.rs) compares
only stream-position reuse versus the additional short-sum change. Both versions
are linked under distinct crate names into one process, with identical flags,
one codegen unit, LTO, static CRT, debug symbols and aborting panics. Each case
uses a verified page with one-byte laces and a matching body, generated outside
timing. Each process alternates the two implementations across twelve batches
of 100,000 parses, discards warmup and selects the median of eleven batches.
Eight processes produce [80 paired observations](validation/2026-10-28-short-lacing-paired.json).
The throughput column is the median of each process's before/after ratio;
cycle columns are separate medians.

| Laces | Before cycles | After cycles | Paired CPU throughput | Faster processes |
| ---: | ---: | ---: | ---: | ---: |
| 0 | 36.40 | 34.36 | 1.087x | 8/8 |
| 1 | 44.50 | 41.12 | 1.090x | 6/8 |
| 2 | 53.45 | 50.11 | 1.074x | 7/8 |
| 3 | 63.53 | 61.81 | 1.011x | 5/8 |
| 4 | 47.41 | 44.01 | 1.066x | 8/8 |
| 7 | 75.40 | 71.94 | 1.046x | 7/8 |
| 8 | 55.23 | 53.98 | 1.028x | 6/8 |
| 9 | 62.98 | 62.95 | 1.036x | 5/8 |
| 17 | 83.00 | 80.48 | 1.036x | 7/8 |
| 255 | 664.29 | 662.74 | 1.000x | 4/8 |

The short-table benefit is modest and workload-dependent. The full-worker
36-byte parse control is 7.2% slower despite the focused gains. The largest
slower full-worker controls are included below; the raw record retains every
workload and stage. Compiler-layout and shared-host effects are not separated
by this run, so these results do not establish a general parser or CRC speedup.

| Slower control | Baseline cycles | Final cycles | CPU cost increase |
| --- | ---: | ---: | ---: |
| `lacing-header/17` | 36.50 | 45.02 | 23.4% |
| `batched-capture/0` | 88,362.50 | 104,934.75 | 18.8% |
| `crc/256` | 224.11 | 260.99 | 16.5% |
| `parse/4096` | 4,197.94 | 4,802.66 | 14.4% |
| `crc/1048576` | 982,153.00 | 1,105,621.50 | 12.6% |
| `writer/0` | 439.66 | 487.29 | 10.8% |

The accepted production changes add 36 lines overall. They remove comparison
sorting, a redundant serial scan and an out-of-line call for short lacing sums.
The grouping kernel supplies the stable destination computation and permutation
that replace the sort; it adds no allocation or unsafe code.

Rejected experiments remain in the ignored snapshot directory. Replacing the
parallel packet-origin vector saved one allocation but slowed the 130,050-byte
span index from 3.884M to 6.887M cycles in the first eight-trial comparison.
Deriving origins from previous page entries did not resolve that regression.
Two decoder-header caching/validation variants showed no consistent CPU benefit.
Capture cursor variants improved some batched-pause cases but also showed a
large span-control regression. The accepted set restores those paths.

## Correctness

All **56 Ogg tests** and **113 regular Vorbis tests** pass; 87 external/manual
Vorbis tests remain ignored. Ogg includes the independent libogg oracle tests,
skipping only manual synchronization-golden regeneration. The clean reference
is pinned to `be05b13e98b048f0b5a0f5fa8ce514d56db5f822`. Corpus checks cover
38 files, 8,733 pages and 119,144 packets, plus 242,352 exact synchronization
operations and the full transport version/flag matrix. The new test covers 54
stream-count, page-count and ordering combinations, checking physical callback
order, BOS/EOS endpoints, packet origins, stream ranges and every page's packet
and granule queries. All-target/all-feature Ogg Clippy with warnings denied,
formatting and Python compilation pass.

The new [index-build probe](validation/2026-10-28-index-build-probe.rs) compares
192 combinations of stream count, page target, input ordering, page limits and
callback failures. It includes large spans, mixed continuation pages and nil
EOS pages. It records callback metadata, packet/page checksums, reader positions,
full public index debug values, retained storage, errors and lookup results.
Both executions produce **1,978,621 identical bytes**, SHA-256
`a1ed995128b0b6ef694070d55813f3ffa2cad1a9607f519fdeb5bfc1fd9fb89d`.

The unchanged [incremental probe](validation/2026-10-28-incremental-probe.rs)
covers all 256 lacing counts, selected chunk sizes, empty polling, truncation,
corruption, maximum pages, finish, reset and decoder reuse. Consumed counts,
returned bytes and errors match in **5,808,523 identical bytes**, SHA-256
`2e0fc461ed8c30e0c28bb58aa21e65a47efdbc6cf376503578f6dd44eb2f49cb`.
The existing demux/capture/index and packet-origin probe also matches across
784 configurations, with **3,800,798 identical bytes**, SHA-256
`32b8e1e700432a1212f60a4eb415dd53f10385efba6934d488dc174889ffd46b`.
The unchanged [capture-state probe](validation/2026-10-28-capture-state-probe.rs)
adds 1,440 raw/indexed, paused/draining, limit, compatibility and reset cases,
plus stream and granule queries. Both executions produce **78,020,534 identical
bytes**, preserving complete packets, metadata, errors and capture state.

The [protocol record](validation/2026-10-28-index-build-protocol.json) pins
commands, source/library/executable hashes, trace sizes and checksums. The
[audit](validation/2026-10-28-index-build-checks.json) pins tested sources,
tests, commands, logs, benchmark controls and recomputed raw summaries.

From the repository root, choose a fresh ignored directory:

```powershell
python crates/deadlib-audio/decode/scripts/benchmark_ogg_kernels.py --root target/ogg-build-repeat --capture before --source-revision 7da11090a
python crates/deadlib-audio/decode/scripts/benchmark_ogg_kernels.py --root target/ogg-build-repeat --capture after --stage before --stage after --runs 8 --output target/ogg-build-repeat/results.tsv
```

For isolated comparisons, capture `05da6dd48`, `f3a0b170e` and `439417705`
as additional stages with `--source-revision`. The paired record contains the
complete compiler commands for its two libraries and worker, plus every raw
paired observation and source/executable hash.

Saved workers and complete traces remain under ignored
`target/ogg-index-build-20261028/`.

The subsequent [packet-metadata and continuation measurements](SUMMARY_REUSE.md)
cover reused demux metadata, short unfinished fragments and preflight bounds.
