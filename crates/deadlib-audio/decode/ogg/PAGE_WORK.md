# Page metadata and checksum work

The later [index-query and stream-storage measurements](PACKET_STATE.md) cover
early-exit granule checks, endpoint stream selection and smaller demux state.

This pass starts at `b776787db`. The measurements isolate three source changes
using the same current probes, compiler, dependencies and build flags.
Production grows by 32 lines; the added word arithmetic removes byte-loop work
and the temporary checksum header, with no new retained fields.

`Page::completed_packets` counts continuation bytes eight at a time. Adding one
to each byte's low seven bits identifies `0xff` without carry between bytes;
multiplication adds the eight one-bit flags. At most seven trailing laces use
the scalar count. Subtracting continuations from the lacing length gives the
same number of completed packets. This removes most per-lace loop iterations
without cached metadata, a lookup table or unsafe code. Short lacing tables use
the existing scalar count; the wide loop stays out of their calling code.

Page CRC verification folds bytes 16 through 21 followed by the four implicit
zero checksum bytes in one table step. It removes the 26-byte temporary header,
its initialization/copy and the dependent two-byte tail. The existing table
powers express those zero transitions directly; the public `crc32` operation
and tables stay the same. A prototype that copied a 32-byte prefix regressed
small pages and was discarded.

Page-size calculations sum eight laces at a time into four 16-bit lanes, then
multiply once per page to add those lanes. At most 255 laces contribute a total
of 65,025, so lanes cannot carry into each other. This replaces five duplicated scalar
sums in parsing, encoding validation, pull/incremental reading and buffered
synchronization with one shared loop and a scalar tail of at most seven bytes.
There is no new allocation, retained state, dependency or unsafe code.
The sum's wide loop also stays separate from the small parser paths: allowing
both loops to expand into their callers caused large regressions in a rejected
prototype.

A single-stream sort shortcut was also tested, but its small improvement was
within the observed trial spread, so it was discarded.

## CPU and allocations

Cycle medians below compare adjacent saved stages: `before` to `counts`,
`counts` to `crc`, then `crc` to `sum-lanes`. Throughput is the inverse cycle
ratio for the same operation, rather than audio throughput.

| Change | Operation | Before cycles/op | After cycles/op | CPU throughput |
| --- | --- | ---: | ---: | ---: |
| Word counts | 255 one-lace packets | 416.16 | 106.22 | 3.918x |
| Word counts | 129-lace bulk fragment | 198.21 | 57.00 | 3.477x |
| Word counts | 255 continuation laces | 404.22 | 106.74 | 3.787x |
| Folded CRC | Parse one empty-packet page | 52.91 | 39.82 | 1.329x |
| Folded CRC | Parse a 36-byte body | 105.99 | 87.59 | 1.210x |
| Folded CRC | Parse a 228-byte body | 317.83 | 296.67 | 1.071x |
| Word sums | Poll a 255-lace header | 165.00 | 92.09 | 1.792x |
| Word sums | Build the zero-byte-packet dense index | 58,695.69 | 55,485.50 | 1.058x |

The complete before-to-after comparison includes all three changes:

| Operation | Baseline cycles/op | Final cycles/op | CPU throughput |
| --- | ---: | ---: | ---: |
| Count 255 one-lace packets | 416.16 | 114.46 | 3.636x |
| Parse one empty-packet page | 51.69 | 43.55 | 1.187x |
| Parse a 36-byte body | 103.64 | 89.36 | 1.160x |
| Poll a 128-lace header | 107.89 | 94.53 | 1.141x |
| Poll a 255-lace header | 163.13 | 92.09 | 1.771x |
| Build the zero-byte-packet dense index | 59,306.16 | 55,485.50 | 1.069x |

All paired heap counters match across all 119 workloads and every trial. Counts,
page parses and preallocated header polls allocate nothing. The dense zero-byte
index still makes five allocations and five reallocations, with 5,707 cumulative
allocated bytes and a 3,386-byte peak.

The final result also has slower controls. An eight-lace header poll rises from
34.80 to 38.56 cycles (+10.8%); chained-index histories 0 and 64 rise by 10.0%
and 9.8%. The unchanged eight-lace fragment loop rises by 8.6%, the capped-reader
17-byte/full-read case by 6.9%, and a maximum-body page parse by 3.8%. These
observations remain in the result. The reduced large-header work does not imply
a speedup for every header, index, large body or complete audio decode.

## Measurement boundaries

The resident-input worker includes 119 workloads. Eight new count probes reuse
the existing tiny, mixed, two/three/four/eight-lace, bulk and continuation-only
fragment fixtures. Fixture encoding and CRC verification happen before timing
those count calls. Existing parse, synchronization, reader, demux, capture,
index and writer probes remain in the comparison.

Four header-poll probes push a complete header/lacing table with 8, 17, 128 or
255 continuation laces, then poll while waiting for the declared body. Each
operation resets a preallocated `SyncBuffer`, copies the prefix and computes
its page size. It includes reset, copying and polling, with no CRC or allocation
to mask that scan; the input is a valid incomplete transport page.

Eight trials run each of four workers once, rotating the first stage so each
runs first twice. Each observation is the median CPU-cycle sample from eleven batches
after warmup. Windows `QueryThreadCycleTime` measures the benchmark thread,
pinned to logical CPU 2. Allocator counters come from a separate untimed
operation of each workload. The worker includes public API setup and cleanup
where specified by each probe; these measurements do not include audio decoding
or filesystem IO.

The host is a shared Windows x64 Xeon E5-2696 v4, with rustc 1.99.1 / LLVM 23.1.1,
optimization level 3, LTO, benchmark debug symbols and static CRT. The machine
is neither isolated nor frequency locked. Per-stage minima, maxima, median
absolute deviations and nanoseconds accompany the cycle medians in the
[record](validation/2026-10-28-page-work.json); all 3,808 observations are in the
[raw data](validation/2026-10-28-page-work.tsv). Saved sources, executable hashes,
compiler identity, flags, profiles and probe hashes are included in that record.

## Correctness

All **54 Ogg tests** pass, including the independent libogg oracle tests; only
the manual golden-file regeneration command is skipped. The clean reference
is pinned to `be05b13e98b048f0b5a0f5fa8ce514d56db5f822`. Corpus checks cover
38 files, 8,733 pages and 119,144 packets; synchronization comparisons cover
242,352 operations plus all transport version/flag pairs and lacing layouts.
The new packet-count test covers every byte value across word and tail lengths.
Existing bitwise CRC tests cover checksum masking, unaligned input, lengths
27 through 300 and large pages. All **113 regular Vorbis tests** pass; 87
external/manual tests remain ignored. Ogg all-target/all-feature Clippy with
warnings denied, formatting and Python compilation also pass.

The existing full synchronization-state probe compares 120 configurations,
including current pages, offsets, capacity, rejection, overflow and hole state:
both executions produce **182,630,684 identical bytes**, SHA-256
`efc2f0e6574a05872307fcc522724f78381f2232ed75a6d707b90f48437c3f70`.
The full demux/capture/index and packet-origin probe compares 784 configurations:
both executions produce **3,800,798 identical bytes**, SHA-256
`32b8e1e700432a1212f60a4eb415dd53f10385efba6934d488dc174889ffd46b`.
The [protocol record](validation/2026-10-28-page-work-protocol.json) pins commands,
source/library/executable hashes and trace sizes. The
[audit](validation/2026-10-28-page-work-checks.json) pins tested sources, tests,
commands, logs and benchmark controls, and checks the raw summaries.

From the repository root, choose a fresh ignored directory:

```powershell
python crates/deadlib-audio/decode/scripts/benchmark_ogg_kernels.py --root target/ogg-page-repeat --capture before --source-revision b776787db
python crates/deadlib-audio/decode/scripts/benchmark_ogg_kernels.py --root target/ogg-page-repeat --capture after --stage before --stage after --runs 8 --output target/ogg-page-repeat/results.tsv
```

For isolated comparisons, capture `0f842a9a0`, `27b135e95` and `edc9dddec` as
additional stages with `--source-revision`. Historical extraction leaves the
working tree in place and uses the same current probes for each stage.

Saved workers and complete traces remain under ignored
`target/ogg-metadata-20261028/`.
