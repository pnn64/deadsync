# Container performance

The latest [packet-count lane and short-tail measurements](LANES.md) cover
once-per-page horizontal folding and direct short fallback scans, with paired
CPU/heap evidence and an investigation of earlier control regressions.

The earlier [empty-continuation and packet-writer measurements](FRAMING.md)
cover zero-body preflight, reused framing invariants and a rejected guard change,
with paired CPU measurements and exact state/IO comparisons.

The earlier [packet-metadata and continuation measurements](SUMMARY_REUSE.md)
cover reused demux metadata, direct short-fragment results and whole-body
preflight bounds, with CPU/heap comparisons and exact state checks.

The earlier [index-construction and short-page measurements](INDEX_BUILD.md)
cover stable page grouping, reused demux stream positions and direct short
lacing sums, with rotating CPU/heap trials and exact protocol comparisons.

The earlier [index-query and stream-storage measurements](PACKET_STATE.md) cover
early-exit packet-existence checks, endpoint stream selection and a smaller
stored granule, with paired query measurements and exact state comparisons.

The earlier [page metadata and checksum measurements](PAGE_WORK.md) cover word
packet counts, directly folded checksum zeros and shared word lacing sums,
with isolated CPU/heap comparisons and exact state and index checks.

The earlier [index-count and synchronization-state measurements](STATE_REUSE.md)
cover reused packet counts, deferred buffer moves and cached waiting-page polls,
with CPU/heap comparisons and exact state, capacity and index checks.

The earlier [encoding and page-view measurements](ENCODING.md) cover direct owned
encoding, a contiguous CRC update and smaller borrowed page values, including
CPU/heap comparisons and exact byte, debug-format and IO checks.

The earlier [page-buffer and packet-start measurements](BUFFERS.md) cover complete
page copies, short-read initialization and reused index searches, including CPU
cycles, allocation counters and exact behavior comparisons.

The earlier [buffered verification and recovery measurements](RECOVERY.md) cover
reused page validation and word scans for synchronization and pull recovery,
with exact parser/IO comparisons and isolated CPU/heap measurements.

The earlier [word-loop measurements](WORD_LOOPS.md) cover wide fragment scans,
sixteen-byte CRC updates and page-level packet bounds, including isolated and
combined CPU/heap results, slower controls and exact policy comparisons.

The earlier [lacing-result measurements](LACING.md) cover count reuse,
constant-time packet-origin tracking and cached synchronization lengths, with
CPU/heap comparisons and independent reference validation.

The earlier [direct streaming measurements](DIRECT_STREAMING.md) cover borrowed
packet writes, recent-stream searches and cached incremental page lengths.
They resolve the earlier large-writer CPU regression and include exact IO and
decoder-state comparisons, CPU cycles and heap counters.

The [2026-10-28 kernel measurements](KERNELS.md) cover fragment traversal,
packet-writing allocations and masked-header CRC, with isolated CPU-cycle
comparisons, complete-byte verification and an explicit large-writer CPU tradeoff.

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
Formatting, all-target Clippy and Python syntax checks pass. At this baseline
milestone, production source matched `013711c18`; only benchmark registration,
adapters and tooling changed.

## Eight-byte CRC update, 2026-10-27

CRC now processes eight input bytes per word. Starting with the original
one-byte remainder table, each additional row applies one more zero-byte
transition. CRC linearity permits XORing the eight contributions, after merging
the incoming state's four bytes with the first four input bytes. Short tails
use the byte transition. This preserves the direct polynomial, zero initial
state/xor-out, arbitrary seeded updates and page checksum-field masking.

All tables are generated at compile time. Logical table elements grow from
1 KiB to **8 KiB of shared read-only data**; there is no dynamic initialization,
new retained heap, API change, dependency, unsafe code or target-specific flag.
Independent bitwise tests cover every byte/table power, four initial states,
16 alignments, lengths 0..256, large/page-sized buffers and split updates down
to one-byte chunks. The benchmark runner also tightens source-count validation
to exact multiplication; timed adapters are unchanged.

```powershell
python crates/deadlib-audio/decode/scripts/benchmark_ogg.py --baseline-revision a95e01e8f --repeats 7
python crates/deadlib-audio/decode/scripts/benchmark.py --baseline-revision a95e01e8f --repeats 7
python crates/deadlib-audio/decode/scripts/benchmark_resources.py --baseline-revision a95e01e8f --repeats 7
```

The paired container run interleaves the old native source, current native and
libogg for **693 timing rows**. Every page field and complete packet byte agrees
across all three workers on all seven inputs. Generated Ogg fixture files also
reproduce their original complete-file hashes. Current adapters/profiles and
registry artifacts match; historical source is extracted without switching the
worktree. These remain shared, unlocked Windows measurements.

CRC median MiB/s:

| Bytes | Native before | Native after | libogg |
| --- | ---: | ---: | ---: |
| 64 | 313.2 | 1,545.1 | 1,022.3 |
| 256 | 288.2 | 1,348.0 | 1,228.3 |
| 4,096 | 288.9 | 1,328.5 | 1,309.9 |
| 65,025 | 286.1 | 1,345.0 | 1,339.4 |
| 1,048,576 | 291.0 | 1,375.7 | 1,348.4 |

The observed CRC gain is **4.60-4.93x**. This is the public-API kernel described
above, including libogg's header/body split and checksum zero/store operations.

Full packet-demux median MiB/s:

| Input | Native before | Native after | libogg |
| --- | ---: | ---: | ---: |
| `in_two` | 273.2 | 1,126.2 | 1,103.0 |
| `Goin' Under` | 274.2 | 1,065.7 | 1,054.0 |
| Bulk packets | 278.0 | 1,154.1 | 1,079.7 |
| Spans / mux / chain | 199.8 | 451.0 | 464.2 |
| Tiny packets | 256.0 | 888.1 | 885.7 |

Song demux improves **4.12x / 3.89x**. Native medians are close to libogg here;
the span/mux/chain case remains slower. Native song ranges are 4.149-4.192 /
2.045-2.123 ms versus baseline 17.143-17.544 / 8.055-8.320 ms. Maximum relative
MAD across all container cases is 22.05%, including tiny startup work. No
universal or cross-platform performance ranking is claimed.

## Effect on the Vorbis pipeline

The same paired five-file suite adds **420 full/fragmented/startup trials**.
The independent resource suite adds **280 seek/open trials** and **480 resource
rows**, checking the same 500 reference PCM seek windows on all workers.
Only `ogg/src/crc.rs` differs between native production source snapshots after
normalizing archive line endings; profiles and registry artifacts match.

Median milliseconds, native before -> after:

| Song | Full decode | Indexed open | 100-request seek batch |
| --- | ---: | ---: | ---: |
| `in_two` | 356.733 -> 343.314 | 17.479 -> 6.223 | 13.282 -> 12.338 |
| `Goin' Under` | 314.345 -> 301.740 | 10.296 -> 5.111 | 10.647 -> 9.596 |

Full decoding improves **1.04x / 1.04x**, indexed open **2.81x / 2.01x**, and
warmed seeks **1.08x / 1.11x**. Fragmented song decode changes
360.546 -> 345.007 / 304.643 -> 297.337 ms. Native full decode still trails
Symphonia by about 1.49x / 1.45x on these medians. Full-file ranges overlap:
322.290-347.382 versus 340.702-360.211 ms and 294.170-328.528 versus
297.952-331.479 ms. Indexed-open ranges do not overlap on these songs:
6.099-6.965 / 4.971-5.654 ms after versus 17.042-19.975 / 10.221-10.629 before.
Maximum relative MAD is 4.75% in pipeline groups and 5.90% in seek/open groups.

Small-file movements remain mixed. Mono full decode rises 0.634 -> 0.642 ms and
4 KiB reads 0.602 -> 0.634 ms; stereo full decode rises 1.580 -> 1.597 ms while
fragmented reads improve 1.582 -> 1.541 ms. Eight-channel-silence seek batches
move 16.193 -> 16.267 ms. The raw tables retain all cases and spreads.

Every native allocation/requested/peak/live heap field, source-IO counter and
exposed-PCM count matches its paired baseline across all resource groups.
Warmed native seeks allocate nothing on all five files. The unchanged heap
does not include the additional static table elements. Song open process-peak
medians move 12.953 -> 12.957 / 8.020 -> 8.020 MiB; these process measurements
do not establish an exact binary/static footprint. Indexing still reads the
complete source once and supplies stronger validation than the reference opens.

[Container summary](validation/2026-10-27-eight-byte-crc-container-summary.tsv),
[693 trials](validation/2026-10-27-eight-byte-crc-container-trials.tsv),
[manifest](validation/2026-10-27-eight-byte-crc-container.json),
[Vorbis summary](validation/2026-10-27-eight-byte-crc-pipeline-summary.tsv),
[420 trials](validation/2026-10-27-eight-byte-crc-pipeline-trials.tsv),
[seek summary](validation/2026-10-27-eight-byte-crc-seek-summary.tsv),
[280 trials](validation/2026-10-27-eight-byte-crc-seek-trials.tsv),
[resources](validation/2026-10-27-eight-byte-crc-resources.tsv), and
[checks](validation/2026-10-27-eight-byte-crc-checks.json) pin all inputs,
workloads, adapters, archives and saved workers. Binaries, wire dumps, generated
media and external corpora stay under ignored `target/`.

Validation passes **114 release tests** and **91 extracted offline tests**,
including independent libogg page/packet/write/capture comparisons, all-size
transforms, 38-file raw/full PCM, 1,900 corpus seeks, 528 upstream vectors /
2,640 packet seeks, historical floor-zero/legacy cases, the 144-case native
encoding matrix and allocation checks. No numerical tolerance changes.
Formatting and default/all-feature Clippy pass. Resumed container, indexed
streaming and encoding sanitizer targets run 300 seconds each, completing
**30,118 executions** with no failures or artifacts:
[results](validation/2026-10-27-eight-byte-crc-fuzz-results.tsv) and
[provenance](validation/2026-10-27-eight-byte-crc-fuzz-provenance.json).

Broader conformance/platform coverage, Ogg-only heap/index-seek comparisons,
truly cold startup, competitive Vorbis synthesis, index-free codec seeking and
encoder quality/rate control remain open. DeadSync integration is unchanged.
