# Index counts and synchronization state, 2026-10-28

The later [page metadata and checksum measurements](PAGE_WORK.md) cover word
packet counts, direct checksum-zero folding and shared lacing sums.

This pass follows [owned encoding and page views](ENCODING.md), using production
source `f5abd29fa` as its baseline. Three commits remove repeated lacing scans,
buffer moves and header checks. Production source grows by five lines; there
are no new fields, dependencies, static tables or unsafe blocks.

## Changes

`559009415` derives an indexed page's completed-packet count from the final
packet number already returned by demux and the stream's previous packet count.
Pages without completed packets use zero. Demux has already validated and
numbered every packet before the index entry is created, and a page completes
at most 255 packets. This removes a second lacing scan without another cache or
callback. Callback errors, counter checks, index metadata and granule landmarks
remain unchanged.

`e1b3d6df8` compacts synchronization storage only when the append cannot fit in
the existing tail capacity. Consumed prefixes can remain until that space is
needed. Logical buffering limits and capacity growth still use the unread byte
count, with reservation before any compaction. This removes repeated moves of a
large live window during consume-one/append-one workloads while retaining the
same capacities, allocation failures, page offsets and rejected-append state.

`4b5d14b17` returns `NeedMore` directly when a recognized page has a cached
declared length and the body remains incomplete. Appends preserve the recognized
header; successful advancement and reset clear its length. Partial-body polls
stop repeating the capture and minimum-header checks. Current-page invalidation
still precedes this return; CRC checking waits for the complete declared page.

Production retains `forbid(unsafe_code)`. The existing current-page bookmark is
unchanged. Windows x64 layouts remain **96 bytes for `SyncBuffer`** and **152
bytes for `SyncReader<&[u8]>`**. Every allocation, reallocation, free,
requested-byte and peak-heap counter in the measured work is identical across
every paired workload of all four accepted comparisons.

## Measurement boundaries

Windows thread CPU cycles/op, median over eight alternating process trials.
Each probe warms once and reports the median of eleven batches. All comparisons
use the same 107 probes and helpers, compiler, dependency locks, flags and
profiles. The shared Windows x64 Xeon E5-2696 v4 uses logical CPU 2, rustc 1.99.1 /
LLVM 23.1.1, repository release/bench opt-level 3 and LTO, bench debug info and
static CRT. Frequency and other machine activity are uncontrolled.

Dense index fixtures contain a header and 4,096 payload packets; index work
includes page reads, CRC, demux, construction and cleanup. Synchronization
partial-input probes construct a buffer, append/poll chunks of one page, then
check completion and clean up. No codec is interpreted.

Rolling-window probes use framing-valid page templates with bodies of 0, 17 or
4,096 bytes and windows of 8 or 128 pages. Each operation constructs a buffer,
fills twice the window, consumes half, then performs 1,024 consume-one/append-one
steps and drains the remaining window. Buffer capacity is fixed at its initial
allocation and checked at completion. Initial copying, page CRC, polling,
appends and cleanup are timed. Template/warm-input generation, file loading,
byte verification and heap counting are untimed.

## Isolated results

| Change / work | Before cycles | After cycles | CPU throughput |
| --- | ---: | ---: | ---: |
| Count reuse, dense empty-packet index | 55,005.32 | 44,366.44 | 1.24x |
| Count reuse, dense one-byte-packet index | 61,165.03 | 50,094.00 | 1.22x |
| Count reuse, dense 17-byte-packet index | 130,259.53 | 114,407.50 | 1.14x |
| Deferred moves, empty bodies / 128-page window | 242,437.50 | 91,202.50 | 2.66x |
| Deferred moves, 17-byte bodies / 128-page window | 368,101.50 | 125,335.00 | 2.94x |
| Deferred moves, 4 KiB bodies / eight-page window | 7,661,867.00 | 5,032,476.50 | 1.52x |
| Deferred moves, 4 KiB bodies / 128-page window | 75,352,924.00 | 7,776,446.50 | 9.69x |
| Cached wait, maximum body / one-byte pushes | 1,686,967.00 | 1,545,499.50 | 1.09x |
| Cached wait, maximum body / seven-byte pushes | 321,677.50 | 310,153.00 | 1.04x |
| Cached wait, maximum body / 4 KiB pushes | 143,772.50 | 136,529.00 | 1.05x |

The [count-reuse trials](validation/2026-10-28-index-count-reuse.json),
[deferred-compaction trials](validation/2026-10-28-deferred-compaction.json), and
[cached-wait trials](validation/2026-10-28-sync-waiting.json) retain every case,
per-trial cycles/ns, min/max/MAD, heap counters and source/executable identity.
Each corresponding TSV contains 1,712 rows.

## Combined result and limits

Eight final trials place each worker first four times. Against `f5abd29fa`:

| Work | Before cycles | Final cycles | CPU throughput |
| --- | ---: | ---: | ---: |
| Dense empty-packet index | 71,659.90 | 56,720.16 | 1.26x |
| Dense one-byte-packet index | 76,866.15 | 62,969.06 | 1.22x |
| Dense 17-byte-packet index | 152,902.35 | 139,265.84 | 1.10x |
| Empty bodies / 128-page window | 241,559.50 | 100,421.50 | 2.41x |
| 17-byte bodies / 128-page window | 373,698.50 | 135,870.50 | 2.75x |
| 4 KiB bodies / eight-page window | 7,665,927.50 | 5,759,460.50 | 1.33x |
| 4 KiB bodies / 128-page window | 74,285,276.50 | 8,410,032.50 | 8.83x |
| Maximum body / one-byte pushes | 1,530,464.00 | 1,417,091.50 | 1.08x |
| Maximum body / seven-byte pushes | 304,666.00 | 312,019.50 | 0.98x |
| Maximum body / 4 KiB pushes | 128,956.50 | 117,323.00 | 1.10x |

[Final rows](validation/2026-10-28-sync-cursors-final.tsv) and
[manifest/summary](validation/2026-10-28-sync-cursors-final.json) complete the
**6,848 accepted timing rows**. Other controls remain mixed. The unmodified
four-lace fragment kernel uses 27% more final cycles and the continued-fragment
kernel 16% more. Recovery of 1 MiB of repeated `Ogg` noise through synchronization
uses 9% more, while maximum-page synchronization with seven-byte pushes uses 2%
more. All slower results remain in the records. These measurements establish
gains for the named schedules and inputs, with no universal or full
audio-decoding throughput claim.

## Correctness and reproduction

All **53 Ogg tests** pass, including independent libogg corpus, historical,
capture and synchronization checks. The clean reference source remains pinned
at `be05b13e98b048f0b5a0f5fa8ce514d56db5f822`: 38 corpus files, 8,733 pages,
119,144 packets and 242,352 exact synchronization operations. Only manual golden
regeneration is skipped. All **113 regular Vorbis tests** pass; its 87
external/manual tests remain ignored. Ogg all-target/all-feature Clippy with
warnings denied, formatting, Python compilation and probe formatting pass.

The new rolling-buffer regression test specifies current-page bytes and
offsets, stable capacity, append rejection, repeated refills, draining, resets
and the last representable absolute position. Existing partial-page tests also
cover cached lengths across compaction, rejected appends and reset.

The [synchronization-state probe](validation/2026-10-28-sync-cursor-probe.rs)
compares **120 configurations** of body size, live-window size, polling mode and
absolute position. It records complete pages, every result, positions, buffered
bytes, storage capacity, hole state and current-page bytes/offsets. Cases include
rolling appends, capacity growth, rejection, partial headers/bodies, noise,
empty appends, resets and position overflow. Both traces contain **182,630,684
identical bytes**, SHA-256
`efc2f0e6574a05872307fcc522724f78381f2232ed75a6d707b90f48437c3f70`.

The [index/packet-origin probe](validation/2026-10-28-lookup-probe.rs) compares
784 policy/fixture configurations, complete demux/capture observations and index
metadata, including completed-packet counts, 2,180 packet-start queries and 220
missing-serial queries. Both traces remain **3,800,798 identical bytes**, SHA-256
`32b8e1e700432a1212f60a4eb415dd53f10385efba6934d488dc174889ffd46b`.
The [protocol record](validation/2026-10-28-sync-cursors-protocol.json) pins both
probes' commands, source/library/executable hashes, complete traces and layouts.
The [validation audit](validation/2026-10-28-sync-cursors-checks.json) pins tested
source/tests, logs, source diffs, build controls and timing records.

From the repository root, choose a fresh ignored directory:

```powershell
python crates/deadlib-audio/decode/scripts/benchmark_ogg_kernels.py --root target/ogg-state-repeat --capture before --source-revision f5abd29fa
python crates/deadlib-audio/decode/scripts/benchmark_ogg_kernels.py --root target/ogg-state-repeat --capture after --stage before --stage after --runs 8 --output target/ogg-state-repeat/results.tsv
```

For isolated comparisons, capture `559009415`, `e1b3d6df8` and `4b5d14b17` as
additional stages with `--source-revision`. Historical extraction leaves the
working tree in place and uses the same current probes for every stage. Saved
workers and traces remain under ignored `target/ogg-sync-cursors-20261028/`.
