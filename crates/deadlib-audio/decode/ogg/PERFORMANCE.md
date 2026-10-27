# Container performance

The initial Ogg-only baseline exposes a CRC bottleneck: native CRC reaches about
341 MiB/s on 4 KiB input versus libogg's 1,558 MiB/s, and native song packet
demux is about four times slower. This is a valid-input, warm-process API
comparison on one shared Windows machine, not a conformance or platform ranking.

## Reproduce and validate

```powershell
python crates/deadlib-audio/decode/scripts/benchmark_ogg.py --repeats 7
# Optional historical production source, with today's identical adapters:
python crates/deadlib-audio/decode/scripts/benchmark_ogg.py --baseline-revision 013711c18 --repeats 7
# Independent package/worker discovery; pass an absolute file path:
cargo bench -p deadlib-audio-decode-ogg --bench container -- demux 100 C:/path/input.ogg
```

The script builds the native worker and unmodified, pinned libogg 1.3.6 commit
`be05b13e98b048f0b5a0f5fa8ce514d56db5f822`. C/C++ is built separately and is never
linked to the Rust crate. Optional baseline extraction changes no checkout;
current manifests/adapters compile the historical Ogg source in ignored storage.
Artifact profiles and registry dependency identities must match before timing.

Default inputs are short mono/stereo Vorbis fixtures, the two existing song
files when available, and three deterministic native-writer fixtures:
8.43 MB of bulk packets, 100,001 mostly tiny packets with nil EOS, and spanning
packets interleaved across two serials followed by another chain. They exercise
the container without interpreting a codec. For every input, an untimed wire
protocol compares every page's serial, sequence, flags, granule and length plus
every packet's serial, granule, BOS/EOS, length and complete payload byte for
byte across all workers. The seven-file baseline passes every comparison.

## Timing boundaries

* `pages`: pull and CRC-check every page; no packet assembly or retained output.
* `demux`: pull all pages and assemble every packet, consuming its length and
  borrowed payload without retaining it. Construction and cleanup are included.
* `stream4096`: the same packet work with underlying reads capped at 4,096 bytes.
* `startup`: construct and stop after the first complete container packet,
  including cleanup. This is not first audio PCM or cold executable startup.
* `crc`: identical deterministic input with checksum bytes zero. Native calls
  contiguous `crc32`; libogg calls `ogg_page_checksum_set` over a 27-byte header
  and the remaining body, including checksum zero/store work. Large kernel
  inputs need not be legal Ogg pages; this isolates CRC arithmetic.

Compressed input stays resident. File loading, executable launch, fixture
generation and byte verification are excluded. Each worker warms once. Seven
trials shuffle engine order; calibration targets 150 ms, capped at one million
iterations. Counts/checksums must agree, and all source/input/binary hashes are
rechecked at completion. Payload bytes are not hashed by the timed consumer.

Native uses its normal strict `PageReader`/`Demuxer`; libogg uses sync/pageout and
one stream state per serial, feeding 8 KiB normally or 4 KiB in fragmented mode.
Buffering and validation guarantees differ: native also enforces physical
stream grammar that libogg does not enforce. Valid inputs are verified first;
these timings do not compare malformed-file acceptance. The codec crate's
[pipeline](../vorbis/PERFORMANCE.md) and [resource/seek](../vorbis/RESOURCES.md)
benchmarks remain separate. Ogg-only allocation/process-memory/index-seek
measurement and truly cold startup remain open.

## Initial Windows baseline, 2026-10-27

Xeon E5-2696 v4, 22 cores/44 logical CPUs, reported 2.20 GHz; rustc 1.99.1;
normal repository bench settings (opt-level 3, LTO, bench debug info, CRT flags).
C/C++ Release uses `/O2 /Ob2 /DNDEBUG` without LTO. The machine is shared and
not frequency locked or CPU isolated. The runner retains actual flags/profiles.

Median MiB/s for CRC:

| Bytes | Native byte table | libogg |
| --- | ---: | ---: |
| 64 | 378.9 | 1,230.6 |
| 256 | 347.3 | 1,482.2 |
| 4,096 | 341.2 | 1,558.4 |
| 65,025 | 343.1 | 1,574.8 |
| 1,048,576 | 349.0 | 1,562.2 |

Median milliseconds, native / libogg:

| File | Page scan | Packet demux | 4 KiB packet reads |
| --- | ---: | ---: | ---: |
| `in_two` | 16.490 / 3.758 | 16.696 / 4.163 | 14.816 / 3.768 |
| `Goin' Under` | 7.200 / 1.764 | 8.092 / 1.983 | 7.363 / 1.871 |

Native packet demux reaches 282.1 / 277.0 MiB/s versus libogg 1,131.5 / 1,130.5.
The proximity to native CRC throughput and the small scan/demux difference make
CRC a strong optimization candidate; this is an inference, not direct sampling.
Bulk/native demux reaches 329.9 MiB/s; tiny-packet/native demux reaches 310.5.
The mux/span/chain case reaches 234.7 MiB/s natively versus libogg 545.3.

Song first-container-packet medians are 0.585 / 0.584 microseconds natively and
9.988 / 10.386 for libogg. Native reads 58 source bytes, libogg 8,192. This phase
is sensitive to buffering, allocation and machine activity: maximum relative
MAD reaches 55.66% on one libogg synthetic startup case. Other phase maxima are
3.05% for CRC, 9.62% for page scan, 5.06% for demux and 5.48% for 4 KiB reads.
All individual medians/minima/maxima/MADs remain in the summary; no universal
startup speedup is claimed.

[Manifest](validation/2026-10-27-container-baseline.json),
[462 trials](validation/2026-10-27-container-baseline-trials.tsv),
[summary](validation/2026-10-27-container-baseline-summary.tsv), and
[checks](validation/2026-10-27-container-baseline-checks.json) pin the evidence.
The three-engine historical-source smoke run adds 189 functional timing rows
and exact wire comparisons on four inputs; it is not a performance estimate.
Formatting, all-target Clippy and Python syntax checks pass. Production codec
source matches `013711c18`; only benchmark registration/adapters/tooling change.
