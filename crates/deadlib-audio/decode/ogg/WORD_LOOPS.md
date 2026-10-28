# Word scans and page bounds, 2026-10-28

The later [buffered verification and recovery pass](RECOVERY.md) measures reused
page validation and word scans for transport recovery against this baseline.

This pass follows [lacing-result reuse](LACING.md). Three separate production
commits reduce fragment scanning, CRC dependency chains and packet preflight
work. Each has an isolated comparison; a fourth comparison measures their
combined result against production source `97a3a147a`.

## Changes and execution cost

`8efaef1ea` handles fragments ending within four laces directly, then scans
longer continuations eight bytes at a time. Complementing a little-endian word
and counting trailing zero bits locates its first byte below 255. Safe array
loads replace the byte-at-a-time search. The iterator stays inline; the wide
scan is a separate leaf function to keep dense packet loops small. Fragment
length still follows directly from the number of 255-byte continuation laces.

`bcd907ff4` processes CRC in sixteen-byte words with sixteen compile-time table
rows. This halves the state-dependent loop iterations for large buffers. The
existing eight-byte update handles the short remainder, followed by scalar
bytes. Polynomial, seed handling, split updates and checksum-field masking are
unchanged. The tables grow from 8 to **16 KiB of shared read-only data**.

`72c067d53` skips per-fragment resource preflight when no packet bytes are
buffered and the whole page body proves both packet and pending-byte limits.
Demux includes pending bytes belonging to other streams in that bound. Capture
derives outgoing continuation from the final lace. Tight limits, existing
partial packets and arithmetic overflow retain the original checking loop.
The bound is conservative: completed packets within a page still work with a
zero pending-byte limit through that fallback. Error ordering and the later
packet emission loop remain intact.

These changes add 79 production lines, mainly explicit short paths and CRC
contributions. They remove work from repeated execution rather than reducing
source length: byte-by-byte continuation tests, half the wide CRC iterations,
and a complete preflight fragment walk on eligible pages. There are no new
fields, dynamic allocations, dependencies or unsafe blocks; production retains
`forbid(unsafe_code)`.

## Isolated measurements

Windows thread CPU cycles/op, median over eight alternating process trials.
Each worker warms up and reports the median of eleven measured batches, pinned
to logical CPU 2. Inputs stay resident; generation, verification and allocation
counting are outside timing. Profiles match the repository's release/bench
settings: opt-level 3, LTO, bench debug info and static CRT. The shared Windows
x64 Xeon E5-2696 v4 host uses rustc 1.99.1 / LLVM 23.1.1; frequency and other
machine activity are uncontrolled. CPU throughput is before/after cycles.

| Change / work | Before cycles | After cycles | CPU throughput |
| --- | ---: | ---: | ---: |
| Fragment scan, tiny packets | 1,770.04 | 1,176.77 | 1.50x |
| Fragment scan, three laces | 849.22 | 555.62 | 1.53x |
| Fragment scan, four laces | 664.44 | 476.11 | 1.40x |
| Fragment scan, eight laces | 451.13 | 499.41 | 0.90x |
| Fragment scan, bulk | 179.86 | 40.38 | 4.45x |
| Fragment scan, continued | 283.27 | 63.61 | 4.45x |
| Fragment scan, dense demux / one-byte packets | 126,634.35 | 85,529.57 | 1.48x |
| CRC, 64 bytes | 96.47 | 73.53 | 1.31x |
| CRC, 4,096 bytes | 7,143.85 | 4,315.15 | 1.66x |
| CRC, 65,025 bytes | 110,957.00 | 64,752.50 | 1.71x |
| CRC, 1 MiB | 1,883,530.00 | 1,061,611.50 | 1.77x |
| CRC, parse 65,025-byte body | 99,053.76 | 66,931.04 | 1.48x |
| Preflight, dense demux / empty packets | 87,285.56 | 65,318.39 | 1.34x |
| Preflight, dense demux / one-byte packets | 88,455.08 | 65,740.25 | 1.35x |
| Preflight, dense demux / 17-byte packets | 84,946.51 | 64,731.92 | 1.31x |
| Preflight, dense capture / one-byte packets | 100,884.27 | 75,343.36 | 1.34x |
| Preflight, dense index / empty packets | 96,641.72 | 71,186.62 | 1.36x |

Dense fixtures contain an initial header and 4,096 payload packets. Demux and
capture consume pre-parsed pages, including context construction and cleanup.
Index includes page reads, CRC, demux and index construction. Fragment probes
visit all fragments of one page and consume lengths and packet flags. The
three/four/eight-lace controls repeat fixed segment patterns within 255 laces;
bulk and continued probes span long runs of 255-byte segments.

Each comparison uses the same 65 probes, adapters, helpers and dependency lock.
The [fragment trials](validation/2026-10-28-fragment-word-four.json),
[CRC trials](validation/2026-10-28-crc16.json) and
[preflight trials](validation/2026-10-28-preflight-bounds.json) retain every
per-trial measurement, min/max/MAD, heap counters, source/binary hashes and build
controls. Each corresponding TSV contains 1,040 rows.

## Combined result and limits

Eight final trials place each worker first four times. Against `97a3a147a`:

| Work | Before cycles | Final cycles | CPU throughput |
| --- | ---: | ---: | ---: |
| Fragment scan, tiny packets | 1,850.74 | 1,281.66 | 1.44x |
| Fragment scan, bulk | 183.36 | 45.05 | 4.07x |
| Fragment scan, continued | 319.97 | 72.37 | 4.42x |
| CRC, 65,025 bytes | 111,835.50 | 58,497.00 | 1.91x |
| CRC, 1 MiB | 1,843,581.00 | 1,101,122.50 | 1.67x |
| Parse 65,025-byte body | 98,816.71 | 64,200.46 | 1.54x |
| Dense demux, one-byte packets | 124,710.29 | 69,392.88 | 1.80x |
| Dense capture, one-byte packets | 115,117.46 | 78,697.59 | 1.46x |
| Dense index, empty packets | 110,772.07 | 73,676.57 | 1.50x |
| Writer, 8,192-byte packets | 101,378.12 | 75,336.54 | 1.35x |
| Reader + demux, 8,192-byte packets | 29,858,035.50 | 23,614,139.50 | 1.26x |

Every allocation, reallocation, free, requested-byte and peak-heap counter is
identical in every paired workload of all four comparisons. No type layout
changes; the additional 8 KiB CRC table is the explicit retained resource cost.

The eight-lace fragment control uses about 11% more cycles in isolation and 7%
more in the final comparison. The four-lace control improves in isolation but
uses 5% more in the final comparison. Empty-packet writing and whole-input
synchronization of an empty-body page use about 10% and 12% more final cycles;
the small 17-byte incremental decoder with 4 KiB chunks uses 9% more. Other
controls are mixed. All results, including these regressions, remain in the
[final 1,040 rows](validation/2026-10-28-word-loops-final.tsv) and
[manifest/summary](validation/2026-10-28-word-loops-final.json). The named gains
justify the changes on this host; no universal or full audio-decoding speedup
is claimed. The larger table can affect cache behavior on other workloads.

## Correctness and reproduction

All **50 Ogg tests** pass, including the independent libogg corpus, capture,
historical and synchronization tests. The clean reference source is pinned at
`be05b13e98b048f0b5a0f5fa8ce514d56db5f822`: 38 corpus files, 8,733 pages,
119,144 packets and 242,352 exact synchronization operations. Only manual golden
regeneration is skipped. All **113 regular Vorbis tests** pass; its 87
external/manual tests remain ignored. Ogg all-target/all-feature Clippy with
warnings denied, formatting and Python compilation pass.

The new fragment test compares independent scalar lacing sums at every terminal
position, for nine terminal-byte bit patterns and both incoming continuation
states. CRC tests compare all sixteen table powers against bitwise polynomial
division, plus alignment, tails, nonzero seeds, split updates and masked headers.
The resource-limit test checks completed in-page packets with zero pending
allowance, error priority and the absence of callbacks before preflight errors.

An additional [differential probe](validation/2026-10-28-preflight-probe.rs)
compares the baseline and final libraries over **784 policy/fixture
configurations**. It covers tight packet/pending limits, multiplexed partial
packets, nil continued pages, skipped initial continuations, legacy granules and
malformed EOS. Each configuration observes demux, four capture runs and index
construction, including complete payloads, callback order, errors, public state
and index metadata. Both traces are **3,461,242 identical bytes**, SHA-256
`0be5d959956902afa396f58f6f35befbb8a93d798088e8d25306767979e77a9a`.
The [protocol record](validation/2026-10-28-preflight-protocol.json) pins commands
and library/executable/trace hashes. No acceptance policy or API changes.

The [validation audit](validation/2026-10-28-word-loops-checks.json) pins tested
source/tests, isolated source diffs, commands/logs, all 4,160 timing rows and
matching dependency/build controls. Saved workers and traces stay under ignored
`target/ogg-word-scans-20261028/`.

From the repository root, choose a fresh ignored directory:

```powershell
python crates/deadlib-audio/decode/scripts/benchmark_ogg_kernels.py --root target/ogg-word-repeat --capture before --source-revision 97a3a147a
python crates/deadlib-audio/decode/scripts/benchmark_ogg_kernels.py --root target/ogg-word-repeat --capture after --stage before --stage after --runs 8 --output target/ogg-word-repeat/results.tsv
```

For isolated comparisons, capture `8efaef1ea`, `bcd907ff4` and `72c067d53` as
additional stages with `--source-revision`. Historical extraction leaves the
working tree in place and uses the same current probes for every stage.
