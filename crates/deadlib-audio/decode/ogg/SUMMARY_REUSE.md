# Packet metadata and continuation work, 2026-10-28

This pass starts at `a59bb2f10`, following the
[index-construction measurements](INDEX_BUILD.md). Three changes reuse validated
packet metadata, finish short unterminated fragments directly and avoid a
redundant fragment scan during demux preflight.

The demultiplexer returns its existing stream position, incoming packet origin,
completed-packet count and final packet count to the index constructor. Indexing
no longer allocates or grows a parallel origin vector, checks the last lace to
maintain it, or saves the last packet's metadata on every callback. It obtains
the chain from the already validated demux state and uses the packet count that
demux already checked for overflow. Packet and page observations, callback
failure handling and page-limit checks retain their order. The public demux API
still returns `Result<(), Error>`; the summary is private and adds no stored
field, dependency or heap allocation.

The fragment iterator already tests the first four laces for terminators. If
the fallback has at most four laces, all of them must be 255: the fragment's
length and unfinished status are known. It returns those values directly,
avoiding another function call and scan. Longer fragments retain the existing
word scan, including its eight-byte boundary behavior. This change adds two
production lines.

Demux preflight already knows the number of completed packets. When that count
is zero, a nonempty page body belongs to one unfinished packet. The packet-size
and pending-byte limits can be checked against the complete body length without
walking its laces again. Pages that complete packets retain per-fragment checks,
so large pages with many individually small packets remain valid. Nil pages,
limit failures, EOS checks and callback ordering retain their behavior. There
is no new unsafe code.

## Measurement boundaries

The full worker covers 194 workloads: the previous 166 controls plus 16
fragment-prefix cases, three spanning demux cases and nine index-query controls.
Fragment cases use 1, 2, 3, 4, 5, 8, 17 or 255 laces, either all 255 or ending
in 17. Page generation and validation precede timing. The measured operation
iterates the fragments and consumes each returned value.

The spanning fixtures contain a BOS packet followed by 64 packets. Each packet
has 1, 4 or 64 pages containing a 255-byte continuation, followed by a zero-byte
terminating lace. Demux timing uses prevalidated page views and includes demux
construction, grammar and limit checks, packet assembly, callbacks, finish and
cleanup. Query timing uses a prebuilt index. The old complete-page, large-span,
CRC, synchronization, capture, writer and multiplexed-index controls remain.

Eight trials run each of four saved workers once, rotating the first stage so
each runs first twice. Each observation selects the median of eleven CPU-cycle
batches after warmup. Windows `QueryThreadCycleTime` measures the benchmark
thread pinned to logical CPU 2; heap counters come from a separate untimed API
operation. The shared Windows x64 Xeon E5-2696 v4 host is neither isolated nor
frequency locked. Builds use rustc 1.99.1 / LLVM 23.1.1, optimization level 3,
LTO, benchmark debug symbols and static CRT. Inputs are resident; file loading,
fixture generation and audio decoding are outside timing.

The [record](validation/2026-10-28-demux-summary.json) pins profiles, compiler,
flags, source and executable hashes, medians, minima, maxima, median absolute
deviations and nanoseconds. All 6,208 CPU/heap observations are retained in the
[raw data](validation/2026-10-28-demux-summary.tsv).

The [paired worker](validation/2026-10-28-summary-paired.rs) links all four
versions under distinct crate names with matching flags, one codegen unit,
LTO, debug symbols, aborting panics and static CRT. It compares adjacent changes
within one process, alternating their order across twelve batches and discarding
warmup. Index cases build 4,097-packet streams with page targets 0 or 8,192 and
payload sizes 0, 1 or 17. Fragment cases match the full worker. Demux cases use
the same spanning fixture. Index batches contain 32 operations, fragment batches
131,072 and demux batches eight. Eight processes produce
[200 paired observations](validation/2026-10-28-summary-paired.json). Reported
paired throughput is the median of each process's before/after cycle ratio;
cycle columns are separate medians.

## CPU and heap results

The paired metadata cases improve CPU throughput by **3.3% to 16.8%**. The
short unterminated-fragment cases improve by **46.9% to 71.8%**, winning all
eight paired processes at each of one through four laces. Buffered-span preflight
improves by **14.3%** at four pages per packet and **8.7%** at 64 pages, winning
all eight paired processes in both cases. Its one-page control, which has no
buffered nonterminating page to accelerate, is about 2% slower.

| Paired metadata case: page target / payload | Before cycles | After cycles | Paired CPU throughput | Faster processes |
| --- | ---: | ---: | ---: | ---: |
| 0/0 | 1,609,236.81 | 1,554,063.41 | 1.036x | 8/8 |
| 0/1 | 1,253,626.22 | 1,177,367.11 | 1.059x | 8/8 |
| 0/17 | 1,225,691.39 | 1,177,003.61 | 1.044x | 7/8 |
| 8192/0 | 52,741.71 | 45,422.78 | 1.168x | 8/8 |
| 8192/1 | 59,052.36 | 56,895.08 | 1.033x | 6/8 |
| 8192/17 | 144,612.77 | 139,358.48 | 1.034x | 7/8 |

| Paired fragment case: laces / terminated | Before cycles | After cycles | Paired CPU throughput | Faster processes |
| --- | ---: | ---: | ---: | ---: |
| 1/false | 21.70 | 14.53 | 1.469x | 8/8 |
| 1/true | 13.80 | 14.13 | 0.971x | 2/8 |
| 2/false | 30.14 | 18.19 | 1.633x | 8/8 |
| 2/true | 15.05 | 15.62 | 0.949x | 1/8 |
| 3/false | 30.74 | 18.93 | 1.646x | 8/8 |
| 3/true | 17.01 | 16.55 | 1.031x | 6/8 |
| 4/false | 32.25 | 19.48 | 1.718x | 8/8 |
| 4/true | 17.91 | 18.27 | 0.998x | 4/8 |
| 5/false | 30.23 | 28.45 | 1.072x | 7/8 |
| 5/true | 31.77 | 29.12 | 1.087x | 8/8 |
| 8/false | 27.84 | 27.34 | 1.010x | 4/8 |
| 8/true | 25.58 | 23.48 | 1.047x | 7/8 |
| 17/false | 29.30 | 26.86 | 1.091x | 8/8 |
| 17/true | 27.27 | 26.63 | 1.033x | 8/8 |
| 255/false | 79.74 | 79.85 | 1.008x | 5/8 |
| 255/true | 80.69 | 80.64 | 1.000x | 4/8 |

| Paired preflight case: pages per packet | Before cycles | After cycles | Paired CPU throughput | Faster processes |
| ---: | ---: | ---: | ---: | ---: |
| 1 | 11,029.81 | 11,235.63 | 0.980x | 1/8 |
| 4 | 31,265.06 | 27,437.50 | 1.143x | 8/8 |
| 64 | 413,455.62 | 382,300.37 | 1.087x | 8/8 |

The whole-worker stages include construction and cleanup. The final 64-page
span demux case has **22.8%** higher CPU throughput than the baseline. Dense
indexing improves by 7.2% to 16.4% in the final worker. The 130,050-byte span
index is 5.1% slower; this pass does not establish a general indexing speedup.

| Full-worker workload | Baseline cycles | Metadata reuse | Short fragments | Preflight | Combined CPU throughput |
| --- | ---: | ---: | ---: | ---: | ---: |
| `dense-index/0` | 52,055.81 | 44,784.84 | 47,219.94 | 47,021.04 | 1.107x |
| `dense-index/1` | 60,046.93 | 48,914.19 | 55,060.22 | 51,582.50 | 1.164x |
| `dense-index/17` | 140,658.35 | 135,554.97 | 134,402.60 | 131,226.68 | 1.072x |
| `span-demux/1` | 11,537.47 | 13,307.19 | 12,206.26 | 12,000.49 | 0.961x |
| `span-demux/4` | 34,746.15 | 37,802.01 | 33,977.90 | 32,499.72 | 1.069x |
| `span-demux/64` | 503,797.09 | 507,696.64 | 464,029.88 | 410,310.66 | 1.228x |
| `span-index/130050` | 3,858,042.00 | 4,009,936.00 | 4,075,237.50 | 4,055,921.50 | 0.951x |
| `chained-index/512` | 1,188,784.88 | 1,118,077.88 | 1,158,575.75 | 1,144,143.75 | 1.039x |
| `mux-index/128/false` | 4,141,525.00 | 4,315,150.50 | 4,256,983.50 | 4,025,630.00 | 1.029x |
| `mux-index/128/true` | 3,661,809.00 | 3,775,181.00 | 3,780,228.50 | 3,616,042.50 | 1.013x |

A successful index build removes one temporary allocation and free. The
origin-vector growth also disappears for more than four streams. Fourteen
full-worker workloads show lower heap counters; no counter increases in any
workload. The later fragment and preflight stages have identical counters to
metadata reuse. Retained index storage and public layouts remain unchanged.

| Index case | Allocations, before / after | Reallocations | Cumulative allocated bytes | Peak bytes |
| --- | ---: | ---: | ---: | ---: |
| `dense-index/0` | 5 / 4 | 5 / 5 | 5,675 / 5,643 | 3,354 / 3,322 |
| `mux-index/128/true` | 5 / 4 | 27 / 22 | 1,342,737 / 1,340,721 | 671,798 / 670,774 |
| `chained-index/512` | 5 / 4 | 35 / 27 | 916,753 / 900,401 | 458,806 / 450,614 |

The largest slower full-worker controls are retained below. In particular,
`chained-demux/64` is 24.0% slower, beginning with the metadata stage. That is a
remaining regression to investigate; these results do not establish a general
public-demux speedup. Compiler/layout effects and shared-host variability are
not separated by the full-worker run. The paired data supplies the narrower
per-change evidence; every full-worker observation remains available.

| Slower control | Baseline cycles | Final cycles | CPU cost increase |
| --- | ---: | ---: | ---: |
| `chained-demux/64` | 133,113.07 | 165,077.68 | 24.0% |
| `writer/0` | 416.62 | 494.31 | 18.7% |
| `fragment-prefix/17/false` | 21.45 | 25.27 | 17.8% |
| `granule-search/0` | 7,641.34 | 8,845.16 | 15.8% |
| `paused-capture/0` | 219,664.68 | 252,918.88 | 15.1% |
| `crc/65025` | 58,277.00 | 66,948.00 | 14.9% |
| `fragment-prefix/255/true` | 73.72 | 83.70 | 13.5% |
| `paused-capture/1` | 224,438.75 | 252,164.32 | 12.4% |

Production code adds 14 lines overall. The index loses its origin vector,
per-page origin maintenance and per-packet last-metadata assignment. The two-line
fragment fast path removes a repeated call/scan; the preflight path replaces a
fragment walk with direct whole-body bounds checks.

Rejected experiments remain in the ignored directory. A query variant combined
underflow and slice bounds checks, but paired granule/offset queries showed no
consistent benefit. A broader fragment variant skipped four already checked
laces in long scans; the eight-lace terminated case slowed by about 17% in its
paired run. The accepted patch preserves the original long scan. Returning only
origins was also measured separately; origin and count reuse are retained as
one cohesive change, removing both duplicate state and callback bookkeeping.

## Correctness

All 56 Ogg tests and 113 regular Vorbis tests pass; 87 external/manual Vorbis
tests remain ignored. Ogg includes the independent libogg oracle tests, skipping
only manual synchronization-golden regeneration. The clean reference is pinned
to `be05b13e98b048f0b5a0f5fa8ce514d56db5f822`. Corpus checks cover 38 files,
8,733 pages and 119,144 packets, plus 242,352 exact synchronization operations
and the transport version/flag matrix. All-target/all-feature Ogg Clippy with
warnings denied, formatting and Python compilation pass.

The new [index probe](validation/2026-10-28-demux-summary-probe.rs) covers 192
combinations of stream count, page target, physical ordering, page limits and
callback failures. It records callback metadata and checksums, reader positions,
errors, complete index debug values and retained storage. Additional checks
compare packet origins with demux callbacks and all page/granule query results
with linear reference searches. Targets include positions before a stream's
BOS, exact page boundaries, nonzero first granules, missing serials, maximum
integers and invalid packet numbers. Both versions produce 3,649,232 identical
bytes.

The unchanged incremental probe covers all 256 lacing counts, partial input,
truncation, corruption, empty polls, finish, reset and reuse. It produces
5,808,523 identical bytes. The existing demux/capture/index probe matches across
784 limit and compatibility configurations with 3,800,798 identical bytes.
The capture-state probe adds 1,440 raw/indexed, paused/draining, limit,
compatibility and reset cases, plus stream and granule queries, with
78,020,534 identical bytes containing complete packets and state transitions.

The [protocol record](validation/2026-10-28-demux-summary-protocol.json) pins
commands, libraries, executables, source hashes and complete trace checksums.
The [audit](validation/2026-10-28-demux-summary-checks.json) pins the tested
sources, commands, logs, reference identity, benchmark controls and recomputed
measurement summaries.

## Reproduction

Choose a fresh ignored directory from the repository root:

```powershell
python crates/deadlib-audio/decode/scripts/benchmark_ogg_kernels.py --root target/ogg-summary-repeat --capture baseline --source-revision a59bb2f10
python crates/deadlib-audio/decode/scripts/benchmark_ogg_kernels.py --root target/ogg-summary-repeat --capture after --stage baseline --stage after --runs 8 --output target/ogg-summary-repeat/results.tsv
```

For isolated comparisons, capture `98a4ff10a`, `8f9fe720b` and `1a2d2af46`
as additional stages with `--source-revision`.

The paired record includes complete compiler commands for its libraries and
worker. Saved sources, workers, rejected experiments and complete protocol traces
remain under ignored `target/ogg-demux-summary-20261028/`.
