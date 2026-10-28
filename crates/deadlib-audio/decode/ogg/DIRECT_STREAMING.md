# Direct writing, recent streams and incremental input, 2026-10-28

This pass retains three measured optimizations after the
[first kernel pass](KERNELS.md). All production code remains safe Rust with no
new dependency or static table.

## Changes and isolated measurements

`f0a0e1334` emits a ready page directly from the packet slice when the assembly
body is empty. The writer reserves/copies only payload that must stay buffered.
This removes the intermediate payload allocation and copy for isolated BOS
headers, full continuation pages and completed pages ready for flushing.
Terminated full lacing tables still wait for the next packet or explicit flush
when the configured body target has not been reached.

Seven interleaved trials of three workers, median cycles/op:

| Eight packet payloads | Original `c059920e4` | First pass `51b90527a` | Direct writer |
| --- | ---: | ---: | ---: |
| Empty | 626.01 | 644.89 | 515.39 |
| 8,192 bytes | 120,368.31 | 109,667.69 | 102,053.78 |
| 65,024 bytes | 875,894.22 | 979,683.38 | 807,197.56 |
| 130,050 bytes | 1,783,869.62 | 1,845,000.34 | 1,775,412.06 |

The near-64-KiB case gains **1.21x CPU throughput** over the first pass and
**1.09x** over the original writer, resolving its earlier measured regression.
These operations include a six-byte header, fresh writer construction, eight
packets and cleanup; the sink retains no output. The CRC still reads payloads.
The [independent pair](validation/2026-10-28-direct-writer-trial.json) and
[three-worker trial](validation/2026-10-28-direct-writer-original.json) retain all
cases and spreads.

Per operation, allocation/reallocation counts become **0/0** for empty, 8,192-
and 65,024-byte payloads, versus 1/0, 1/1 and 1/1 after the first pass. Their peak
heap becomes zero. Seventeen- and 255-byte payloads still buffer, with one
allocation and three reallocations instead of one and four. The 130,050-byte
case still needs assembly storage: one allocation and one reallocation, with
64,770 peak bytes versus 65,025.

`51454d996` changes two serial searches from `position` to `rposition`, in
demultiplexing and index construction. Serial uniqueness is already validated
across completed chains, so either direction finds the same entry. Recently
appended streams are found before scanning completed streams. No cache, map,
allocation or additional state is introduced.

| Completed chains before 2,048 active pages | Demux cycles before / after | CPU throughput | Index cycles before / after | CPU throughput |
| --- | ---: | ---: | ---: | ---: |
| 0 | 179,990 / 180,978 | 0.99x | 748,330 / 753,928 | 0.99x |
| 64 | 414,882 / 196,727 | 2.11x | 1,257,680 / 678,255 | 1.85x |
| 512 | 1,754,436 / 469,593 | 3.74x | 5,932,262 / 1,738,824 | 3.41x |

The demux probe uses pre-parsed pages; the index probe includes reading, CRC,
demux and index allocation. Both include all historical BOS/EOS pages and fresh
context construction. Heap counters are identical. These are synthetic chained
streams; the [seven-trial record](validation/2026-10-28-recent-streams.json)
does not establish an ordinary song-decoding speedup.

`98a71023f` caches the declared page length once its complete lacing table is
available. Incremental body pushes stop rescanning that immutable table. The
fill loop removes a duplicate validation block and its separate input-exhaustion
exit, becoming two source lines shorter. Header validation and the final public
page parser preserve error ordering and CRC checks. Reset and ready-page reuse
clear the cached length.

| Maximum page input chunks | Cycles before | Cycles after | CPU throughput |
| --- | ---: | ---: | ---: |
| 1 byte | 10,240,114 | 1,986,914 | 5.15x |
| 7 bytes | 1,547,037 | 407,831 | 3.79x |
| 4,096 bytes | 166,820 | 142,456 | 1.17x |
| Whole page | 110,189 | 97,238 | 1.13x |

These [seven-trial measurements](validation/2026-10-28-incremental-length.json)
include construction, every push, final CRC and cleanup of a 65,307-byte page.
Heap counters stay identical. `PageDecoder` grows from **32 to 40 bytes** on
Windows x64, retaining one additional `usize` and no additional heap storage.

## Combined measurements and limits

The benchmark uses repository optimization/LTO/bench profiles and static CRT,
rustc 1.99.1 / LLVM 23.1.1, and the shared Xeon E5-2696 v4 Windows x64 host.
Windows thread CPU cycles are measured on logical CPU 2. Inputs remain resident;
fixture generation, file loading, byte comparisons and allocation counting stay
outside timing. Each invocation reports the median of eleven measured batches
after warmup. The host is neither CPU-isolated nor frequency-locked.

The combined comparison uses production source `51b90527a` and the final source
with identical current adapters. Two seven-trial runs reverse the initial order,
balancing first position seven times per worker. Median of fourteen trial
medians:

| Work | First pass cycles | Final cycles | CPU throughput |
| --- | ---: | ---: | ---: |
| Maximum page, one-byte chunks | 9,793,871 | 2,393,318 | 4.09x |
| Maximum page, seven-byte chunks | 1,512,356 | 459,084 | 3.29x |
| 512-chain demux | 1,640,186 | 543,537 | 3.02x |
| 512-chain index | 5,651,713 | 1,632,504 | 3.46x |
| Eight 65,024-byte packets | 838,010 | 782,178 | 1.07x |
| Eight 8,192-byte packets | 104,118 | 102,966 | 1.01x |

Small control movements remain mixed. The pooled one-byte-packet page reader
uses 7% more cycles and its full demux uses 5% more; seventeen-byte-packet
reader/demux use 4%/5% more. In the reversed-order repeat these controls are
within roughly 0-1% of each other. All four generated reader/demux input files
are byte-identical across the original, first-pass and direct writers. These
results do not support a universal throughput claim or attribute those smaller
movements to a particular change. Large-page gains under whole-page/4-KiB input
also shrink to about 4%/3% in the pooled comparison.

[First 532 rows](validation/2026-10-28-direct-streaming-final.tsv),
[reversed-order 532 rows](validation/2026-10-28-direct-streaming-repeat.tsv), and
[pooled medians/min/max/MAD/heap](validation/2026-10-28-direct-streaming-summary.json)
retain the full result. Each raw comparison's JSON pins source/adapter/binary
hashes, compiler identity, flags and profiles. Earlier writer comparisons have 24 probes;
the later comparisons add eight incremental and six chained-stream probes.

## Correctness and reproduction

All **46 Ogg tests** pass, including independent libogg comparisons over 38
corpus files, three historical files, page-boundary and corpus captures, and
242,352 synchronization operations. Only manual golden regeneration is skipped.
All **113 normal Vorbis tests** pass; 87 external/manual Vorbis tests remain
ignored. Ogg Clippy for every target/feature with warnings denied, Rust formatting,
probe formatting and Python compilation pass.

The [writer probe](validation/2026-10-28-direct-writer-probe.rs) compares every
callback request length, partial/Interrupted/failed write result, emitted byte,
public return value and output position across 168 scenarios. It covers six
page targets, two write caps, seven failure positions, inline/nil EOS,
dense zero-length packets, flushes and terminal errors. It also emits the four
complete reader/demux benchmark fixtures. Original, first-pass and direct
libraries emit **46,897,193 identical trace bytes**:
[commands and hashes](validation/2026-10-28-direct-writer-protocol.json).

The [incremental probe](validation/2026-10-28-incremental-probe.rs) compares
consumed-byte counts, complete page bytes, errors, empty polls, finish and reset
across 435 schedules. It covers all 256 lacing-table lengths, truncated and corrupt
input, version/flag failures and maximum pages. Before/after traces contain
**5,808,523 identical bytes**:
[commands, sizes and hashes](validation/2026-10-28-incremental-protocol.json).
Regression tests additionally check deferred full-lacing flushes and decoder
reuse after partial body input, failure, reset and ready-page polling.

The [validation record](validation/2026-10-28-direct-streaming-checks.json) pins
tested production/tests, independent reference executable, source diffs, commands
and logs. No PCM tolerance, acceptance policy or dependency changes were made.

From the repository root, choose a new ignored directory:

```powershell
python crates/deadlib-audio/decode/scripts/benchmark_ogg_kernels.py --root target/ogg-streaming-repeat --capture before --source-revision 51b90527a
python crates/deadlib-audio/decode/scripts/benchmark_ogg_kernels.py --root target/ogg-streaming-repeat --capture after --stage before --stage after --runs 7 --output target/ogg-streaming-repeat/results.tsv
```

To isolate changes, capture `f0a0e1334`, `51454d996` and `98a71023f` as additional
stages with `--source-revision`. Historical source extraction never switches the
working tree. Saved executable/source snapshots and complete protocol output
stay under ignored `target/`.
