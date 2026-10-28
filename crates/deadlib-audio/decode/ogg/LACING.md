# Reusing lacing results, 2026-10-28

This pass follows [direct writing and incremental input](DIRECT_STREAMING.md).
It retains three optimizations with separate measurements before and after each
change, plus a balanced comparison against production source `8668ce2dd`.

## Changes

`916f6ac4c` passes the already computed completed-packet count to granule
validation. Demux and capture both need that count before validation; the helper
previously scanned the same lacing table again. This removes one scan without
adding a cache, allocation or public parameter. Validation conditions, ordering,
compatibility accounting and errors use the same value as before.

`3c21cd945` removes the index builder's walk over every fragment to carry packet
origins forward. Only an unfinished trailing packet needs its start saved. A
page without an incoming continuation starts that packet locally; a continued
page starts a new unfinished packet only after completing the incoming packet.
The existing last-completed-packet observation provides that fact. One page-level
check replaces the fragment loop, with no additional storage or lacing scan.

`2dff293a5` remembers the declared length of a pending synchronized page after
its full lacing table arrives. Body pushes and repeated polls stop resumming
that immutable table. Successful byte advancement and reset clear the cache;
appends/compaction retain it because the candidate prefix is unchanged. Rejected
appends and failed position advances preserve state. Synchronization still
waits for the complete declared page before CRC checking, including unknown
versions and flag bits.

All production code remains safe Rust with `forbid(unsafe_code)`, no new
dependency and no additional dynamic allocation.

## Measurements before and after each change

Median Windows thread CPU cycles/op over seven interleaved process trials.
Each trial reports a median of eleven measured batches after warmup. The
resident-input probes use the repository bench/release optimization/LTO profiles,
static CRT, rustc 1.99.1 / LLVM 23.1.1 and logical CPU 2 on the shared Xeon
E5-2696 v4 host. Frequency and other CPU activity are uncontrolled.

| Change / work | Before cycles | After cycles | CPU throughput |
| --- | ---: | ---: | ---: |
| Count reuse: capture, empty packets | 124,682.84 | 116,458.47 | 1.07x |
| Count reuse: capture, one-byte packets | 125,121.88 | 115,031.72 | 1.09x |
| Count reuse: capture, 17-byte packets | 126,212.50 | 114,997.41 | 1.10x |
| Packet origins: index, empty packets | 154,432.00 | 116,499.69 | 1.33x |
| Packet origins: index, one-byte packets | 157,738.19 | 119,380.56 | 1.32x |
| Packet origins: index, 17-byte packets | 251,053.12 | 226,963.00 | 1.11x |
| Sync length: maximum page, one-byte chunks | 10,599,875 | 1,725,050 | 6.15x |
| Sync length: maximum page, seven-byte chunks | 1,700,467 | 421,879 | 4.03x |
| Sync length: 4 KiB body, seven-byte chunks | 42,803 | 35,778 | 1.20x |

Dense fixtures contain an initial header and 4,096 payload packets in one
logical stream. Capture and demux use pre-parsed pages, including fresh context
construction and cleanup. Index measurements include page reads, CRC, demux and
index construction. Sync measurements include construction, each append/poll,
final CRC and cleanup of one page. Maximum pages contain 65,307 wire bytes;
transport synchronization applies no packet/granule grammar. Fixture generation,
file loading, byte verification and heap counting are outside timing.

[Count-reuse trials](validation/2026-10-28-lacing-counts.json),
[packet-origin trials](validation/2026-10-28-index-packet-starts.json), and
[sync-length trials](validation/2026-10-28-sync-length.json) retain every case,
per-trial cycles/ns and heap counters, min/max/MAD, source and executable hashes,
compiler identity, flags and profiles. Their TSV files contain 798 rows each.

## Combined result and resource cost

Eight final trials alternate the initial engine order, placing each worker
first four times. Both workers use the same current 57 probes and helpers.
Median cycles/op against `8668ce2dd`:

| Work | Before | Final | CPU throughput |
| --- | ---: | ---: | ---: |
| Capture, empty packets | 133,065.02 | 119,682.39 | 1.11x |
| Capture, one-byte packets | 129,940.54 | 119,366.85 | 1.09x |
| Capture, 17-byte packets | 129,299.21 | 117,888.62 | 1.10x |
| Index, empty packets | 149,390.35 | 115,402.12 | 1.29x |
| Index, one-byte packets | 158,684.82 | 115,470.72 | 1.37x |
| Index, 17-byte packets | 267,282.43 | 229,747.90 | 1.16x |
| Sync, maximum page / one-byte chunks | 10,749,133 | 1,853,458 | 5.80x |
| Sync, maximum page / seven-byte chunks | 1,704,527.50 | 448,658.50 | 3.80x |
| Sync, 4 KiB body / seven-byte chunks | 42,802.50 | 35,449.50 | 1.21x |

Every allocation, reallocation, free, requested-byte and peak-heap counter is
identical in all paired cases of all four comparisons. On Windows x64,
`SyncBuffer` grows from **88 to 96 bytes** and `SyncReader<&[u8]>` from **144 to
152 bytes**. Demux, capture and the incremental decoder retain their sizes:
[layout commands and outputs](validation/2026-10-28-lacing-layout.json).

Whole-page synchronization has no demonstrated benefit: the maximum-page case
uses about 4% more cycles in the final comparison, while 4 KiB chunking is
unchanged. Other controls remain mixed; the bulk-fragment kernel uses about 8%
more cycles despite unchanged production fragment code. Normal one-byte-packet
demux improves about 4%; other reader/demux movements are small. These shared-host
measurements establish gains for the named operations and input schedules, with
no universal or full-audio-decoding throughput claim.

[Final 912 rows](validation/2026-10-28-lacing-final.tsv) and
[manifest/summary](validation/2026-10-28-lacing-final.json) retain all results.
Earlier probes remain available, alongside nine new dense-packet and ten new
synchronization cases. The source/adapter/binary and build-control audit checks
every saved comparison, including matching isolated dependency locks.

## Correctness and reproduction

All **48 Ogg tests** pass. The independent libogg run covers 38 corpus files
(8,733 pages / 119,144 packets), three historical files, generated and corpus
captures, and 242,352 exact synchronization operations. It includes every
version/flag byte pair, 1,024 lacing layouts, nested/false captures, partial
headers/bodies, resets, EOF, read boundaries, repeated seeks and injected IO
failures. Only manual golden regeneration is skipped. The reference source is
clean at pinned commit `be05b13e98b048f0b5a0f5fa8ce514d56db5f822`.

The new index test specifies packet origins across seven hand-built pages,
including nil continued pages and a continued page completing two packets
before starting another unfinished packet. The new synchronization test checks
compaction with a cached maximum-page length, transactional append rejection,
and reset to a smaller page. Existing strict/compatibility, corruption, seeking,
callback and resource-limit checks pass unchanged.

All **113 regular Vorbis tests** pass; its 87 external/manual tests remain
ignored. Ogg all-target/all-feature Clippy with warnings denied, formatting and
Python compilation pass. No acceptance policy, PCM tolerance or public API
changes were made. The [validation record](validation/2026-10-28-lacing-checks.json)
pins tested source/tests, exact source diffs, reference identity, commands and logs.

From the repository root, choose a fresh ignored directory:

```powershell
python crates/deadlib-audio/decode/scripts/benchmark_ogg_kernels.py --root target/ogg-lacing-repeat --capture before --source-revision 8668ce2dd
python crates/deadlib-audio/decode/scripts/benchmark_ogg_kernels.py --root target/ogg-lacing-repeat --capture after --stage before --stage after --runs 8 --output target/ogg-lacing-repeat/results.tsv
```

For isolated comparisons, capture `916f6ac4c`, `3c21cd945` and `2dff293a5` as
additional stages with `--source-revision`. Historical extraction leaves the
working tree in place. Saved source/executable snapshots stay under `target/`.
