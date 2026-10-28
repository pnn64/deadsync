# Ogg kernel measurements, 2026-10-28

The subsequent [direct streaming pass](DIRECT_STREAMING.md) removes the large
writer regression recorded below and adds recent-stream and incremental-input
optimizations. This document retains the first pass's historical measurements.

Three retained changes preserve page bytes, packet boundaries and validation:

* `bcc8a889f`: fragment iteration derives length from the terminator and the
  preceding 255-byte segments. This removes the second lacing traversal for
  every fragment, including each preflight traversal in demux and capture.
* `b8775edda`: packet writing reserves and copies a page's payload together and
  fills full laces together. A 65,024-byte packet takes one assembly iteration
  instead of 255. Full pages still defer the zero-length terminator to the next
  page, and BOS, continuation, flush, nil EOS and IO behavior are preserved.
* `d179713c9`: the page CRC uses a 26-byte stack header with its checksum field
  zeroed. This removes one update call and eight dependent byte transitions by
  allowing another eight-byte table update. The existing 8 KiB tables, seeded
  update algorithm, dependencies and `forbid(unsafe_code)` remain unchanged.

## Measurements before and after each change

Windows x64, Xeon E5-2696 v4, rustc 1.99.1 / LLVM 23.1.1. Each worker inherits
the repository release/bench profiles (optimization 3, LTO, bench debug info)
and Windows static CRT configuration. Each benchmark thread is pinned to logical
CPU 2. This shared machine has neither isolated CPUs nor locked frequency.

The probes use Windows `QueryThreadCycleTime`, report throughput as work per CPU
cycle, and collect allocation/requested/peak bytes separately from timing.
Each invocation warms once and reports the median of eleven measured batches.
Five process trials alternate/rotate engine order for the first two changes;
the CRC and final comparisons use seven. Inputs and encoded output remain
resident. Setup, file IO and verification are excluded; writer/reader/demux
construction and cleanup are included. Writer operations submit a six-byte
header and eight payload packets to a non-retaining sink.

| Isolated comparison | Before cycles/op | After cycles/op | CPU throughput |
| --- | ---: | ---: | ---: |
| Tiny-packet fragment page | 2,089.75 | 1,695.07 | 1.23x |
| One bulk fragment | 257.28 | 171.06 | 1.50x |
| Entirely continued fragment | 455.90 | 277.12 | 1.65x |
| Eight 8 KiB packets, writer | 121,863.62 | 105,195.38 | 1.16x |
| Empty-packet page parse, CRC change | 95.28 | 59.33 | 1.61x |
| 64-byte page parse, CRC change | 174.31 | 131.61 | 1.32x |
| 256-byte page parse, CRC change | 484.00 | 437.00 | 1.11x |

The accepted writer variant avoids division/filling for payloads shorter than
255 bytes. Its [three-worker trial](validation/2026-10-28-kernels-writer-short.tsv)
retains the original writer, the initial bulk variant and this variant.
The [fragment trials](validation/2026-10-28-kernels-fragments.tsv) and
[CRC trials and provenance](validation/2026-10-28-kernels-crc.json) retain the
other isolated comparisons.

Writer allocation counts per complete operation:

| Packet bytes | Reallocations before / after | Requested bytes before / after | Peak bytes before / after |
| --- | ---: | ---: | ---: |
| 8,192 | 7 / 1 | 32,393 / 8,200 | 16,320 / 8,192 |
| 65,024 | 9 / 1 | 130,313 / 65,032 | 65,280 / 65,024 |
| 130,050 | 9 / 1 | 130,313 / 65,033 | 65,280 / 65,025 |

All these operations also have one allocation and one free. Fragment traversal
and page parsing allocate nothing. Reader/demux heap counts stay unchanged.

## Combined result and limitations

The final comparison extracts production source from `c059920e4` and builds it
with the identical current probes, helpers and profiles. Current production
source matches the tested snapshots. Median cycles/op:

| Work | Original | Final | CPU throughput |
| --- | ---: | ---: | ---: |
| Empty-packet page parse | 93.73 | 57.99 | 1.62x |
| 64-byte page parse | 170.55 | 125.27 | 1.36x |
| 256-byte page parse | 465.32 | 422.01 | 1.10x |
| Tiny-packet fragment page | 2,253.61 | 1,894.77 | 1.19x |
| Bulk fragment | 262.92 | 185.13 | 1.42x |
| Eight 8 KiB packets, writer | 121,891.09 | 106,546.66 | 1.14x |
| Eight 65,024-byte packets, writer | 880,489.94 | 986,508.44 | 0.89x |
| Eight 130,050-byte packets, writer | 1,786,050.91 | 1,859,844.00 | 0.96x |
| 2,048 one-byte packets, full demux | 92,339.26 | 82,272.98 | 1.12x |
| 2,048 seventeen-byte packets, full demux | 144,084.20 | 136,379.72 | 1.06x |

The writer is retained for the substantial allocation reduction and the 8 KiB
throughput improvement. The near-64-KiB writer case uses **12% more cycles**;
130,050-byte packets use **4% more**. This is an explicit resource/CPU tradeoff.
Small writer movements are mixed. Large-page CRC and large-payload demux changes
are small; these results establish no general audio-decoding speedup.

A reader refactor removing repeated length validation was rejected: its roughly
4% small-case gain did not establish a consistent broader benefit. Limiting
bulk copies to 8 KiB was also rejected because it did not resolve the writer
regression. Their raw results remain alongside the accepted trials.

[Final 336 rows](validation/2026-10-28-kernels-final.tsv) and
[manifest/summary](validation/2026-10-28-kernels-final.json) include every case,
per-trial values, median/min/max/MAD, allocation counts, source and binary hashes,
compiler identity, profiles and flags. Earlier 19-case probes omit the five
later page-parse probes; the controls match within each comparison.

## Correctness and reproduction

All **44 Ogg tests** pass, including independent libogg comparisons over 38
corpus files (8,733 pages / 119,144 packets), three historical files, 114 corpus
captures, generated page-boundary captures, and 242,352 synchronization
operations. Only manual golden regeneration is skipped. The **113 normal
Vorbis tests** pass; 87 external/manual tests remain ignored in that codec run.
Clippy for all Ogg targets/features with warnings denied, formatting and Python
compilation pass. No tolerance changes or new production unsafe code are used.

New independent checks accumulate every lace into expected fragment ranges for
all 256 table lengths and continuation states. The CRC test compares bitwise
polynomial results across 16 alignments, short/page-sized inputs and four stored
checksum values. The original/final workers also emit identical complete writer
bytes for six page targets, boundary sizes, empty packets and explicit flushes.
The [validation record](validation/2026-10-28-kernels-checks.json) pins tested
source, reference executable, commands and logs.

From the repository root, use a new ignored output directory:

```powershell
python crates/deadlib-audio/decode/scripts/benchmark_ogg_kernels.py --root target/ogg-kernels-repeat --capture before --source-revision c059920e4
python crates/deadlib-audio/decode/scripts/benchmark_ogg_kernels.py --root target/ogg-kernels-repeat --capture after --stage before --stage after --runs 7 --output target/ogg-kernels-repeat/results.tsv
```

For isolated changes, capture consecutive production revisions `bcc8a889f^`,
`bcc8a889f`, `b8775edda` and `d179713c9` with `--source-revision`. All use the
current identical adapter. `cargo bench -p deadlib-audio-decode-ogg --bench
kernels` runs just the current worker. Saved binaries/source snapshots stay
under ignored `target/`; committed raw tables and manifests retain the evidence.
