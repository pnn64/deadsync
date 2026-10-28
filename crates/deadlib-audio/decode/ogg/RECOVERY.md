# Buffered verification and recovery scans, 2026-10-28

This pass follows [word loops and page bounds](WORD_LOOPS.md), using production
source `dbd98f729` as its baseline. Three commits reduce repeated validation and
byte-at-a-time transport searches while preserving parser and IO observations.

## Changes

`8088086b4` verifies buffered pages' CRC directly after their header and exact
length have already been checked. Pull and incremental readers stop calling the
full public parser again, removing a second lacing sum and repeated header
checks. The public parser shares the same checksum check and retains its full
validation. Input buffering, error priority and terminal failure behavior stay
the same.

`552f25001` handles the next three possible `O` offsets directly in buffered
synchronization, then searches eight bytes at a time in a separate leaf
function. A little-endian XOR and zero-byte mask identify the first matching
byte. Subtraction may mark later bytes through borrow propagation; its first
match is exact. Synchronization still skips precisely to the next `O`, retaining
libogg's partial-capture and hole-notification behavior.

`425153947` filters pull-reader recovery candidates by `Og` prefixes in
eight-byte words. Each candidate still receives a complete `OggS` comparison;
the final byte can pair with the next word. A word with multiple candidates
switches to the original scalar search for the remaining input, avoiding extra
work on dense false prefixes. The recovery loop still retains three trailing
bytes and searches inside failed CRC candidates before reading farther.

Production source grows by 72 lines, including comments and spacing. The added
word scans remove repeated runtime byte tests; buffered verification removes a
complete parse step. There are no new fields, tables, dependencies, allocations
or unsafe blocks. Production retains `forbid(unsafe_code)`.

## Measurement boundaries

Every accepted comparison uses the same 85 probes, helpers, dependency lock and
build settings. Eight trials alternate worker order, placing each first four
times. Each probe warms once and reports the median of eleven measured batches.
Noise probes use 64 iterations per batch for 4 KiB prefixes and eight for 1 MiB
prefixes. CPU throughput is before/after Windows thread CPU cycles/op.

The shared Windows x64 Xeon E5-2696 v4 host uses logical CPU 2, rustc 1.99.1 /
LLVM 23.1.1, repository release/bench profiles with opt-level 3, LTO, bench debug
info and static CRT. Frequency and other machine activity are uncontrolled.
File loading, fixture generation, byte verification and heap counting stay
outside timing. All min/max/MAD values remain in the saved summaries.

Noise fixtures prepend 4 KiB or 1 MiB to the same valid container stream. Cases
are `%` bytes without `O` (ID 0), `O` every 31 bytes (31), all `O` (1), repeated
`Og` (2), and repeated `Ogg` (3). Synchronization includes buffer construction,
append/copy, every page-out/hole operation, CRC and cleanup. Pull recovery
includes construction, source reads, every page return, CRC and cleanup. Both
verify the number of returned pages. Thus these are complete recovery probes,
including the common valid suffix, rather than isolated byte-search kernels.

The ordinary one-byte-packet reader consumes a header and 2,048 packets from
resident input, including reader construction and cleanup. Incremental probes
construct a decoder, feed one page in the stated chunks and check completion.

## Isolated and combined results

| Change / work | Before cycles | After cycles | CPU throughput |
| --- | ---: | ---: | ---: |
| Verification, one-byte-packet reader | 11,154.99 | 10,364.79 | 1.08x |
| Verification, maximum page / 4 KiB decoder chunks | 136,090.00 | 89,556.00 | 1.52x |
| Verification, maximum page / whole-input decoder | 90,763.50 | 111,286.50 | 0.82x |
| Sync scan, 1 MiB without O | 2,695,981.44 | 1,833,785.19 | 1.47x |
| Sync scan, 1 MiB / O every 31 bytes | 2,816,774.94 | 2,420,426.44 | 1.16x |
| Sync scan, 1 MiB / all O | 6,584,396.44 | 5,583,640.94 | 1.18x |
| Sync scan, 1 MiB / repeated Og | 4,882,845.88 | 3,777,279.50 | 1.29x |
| Sync scan, 1 MiB / repeated Ogg | 4,181,694.56 | 3,273,362.19 | 1.28x |
| Capture scan, 1 MiB without O | 1,818,132.25 | 1,280,480.69 | 1.42x |
| Capture scan, 1 MiB / O every 31 bytes | 1,752,707.50 | 1,476,974.38 | 1.19x |
| Capture scan, 1 MiB / all O | 1,842,633.88 | 1,683,263.00 | 1.09x |
| Capture scan, 1 MiB / repeated Og | 1,804,317.31 | 1,954,208.50 | 0.92x |
| Capture scan, 1 MiB / repeated Ogg | 1,751,212.12 | 2,062,051.63 | 0.85x |

The accepted trial records are:

- [Buffered verification](validation/2026-10-28-reader-checks.json).
- [Synchronization byte scan](validation/2026-10-28-sync-byte-scan.json).
- [Recovery capture scan](validation/2026-10-28-recovery-capture-scan.json).
- [Combined comparison](validation/2026-10-28-recovery-final.json).

Each record retains its paired source/executable identities, build controls,
all per-trial cycles/ns and heap counters. The four corresponding TSV files
contain 1,360 rows each, **5,440 rows total**.

Combined medians against `dbd98f729`:

| Work | Before cycles | Final cycles | CPU throughput |
| --- | ---: | ---: | ---: |
| One-byte-packet reader | 10,647.95 | 8,709.76 | 1.22x |
| Maximum page / 4 KiB decoder chunks | 118,420.50 | 88,349.00 | 1.34x |
| Maximum page / whole-input decoder | 93,617.00 | 114,688.50 | 0.82x |
| Sync, 1 MiB without O | 2,631,338.56 | 1,721,236.69 | 1.53x |
| Sync, 1 MiB / O every 31 bytes | 2,795,017.00 | 2,288,424.69 | 1.22x |
| Sync, 1 MiB / all O | 6,429,772.38 | 5,207,115.94 | 1.23x |
| Sync, 1 MiB / repeated Og | 4,799,915.94 | 3,475,810.00 | 1.38x |
| Sync, 1 MiB / repeated Ogg | 4,011,870.25 | 3,065,592.00 | 1.31x |
| Pull recovery, 1 MiB without O | 1,753,462.00 | 1,228,321.94 | 1.43x |
| Pull recovery, 1 MiB / O every 31 bytes | 1,808,597.75 | 1,469,744.56 | 1.23x |
| Pull recovery, 1 MiB / all O | 1,765,438.50 | 1,564,129.56 | 1.13x |
| Pull recovery, 1 MiB / repeated Og | 1,782,477.13 | 1,835,349.25 | 0.97x |
| Pull recovery, 1 MiB / repeated Ogg | 1,788,293.87 | 1,948,871.81 | 0.92x |

Every allocation, reallocation, free, requested-byte and peak-heap counter is
identical in every paired workload. No type layout or static table size changes.

The whole-input maximum-page decoder uses about **23% more cycles**, despite
the improvement with 4 KiB chunks. That regression also appears in the isolated
verification comparison. Dense false-prefix pull recovery remains slower: the
Og/Ogg controls use about 8%/18% more cycles in isolation and 3%/9% more in the
combined comparison. Final 36-byte-body parsing uses 15% more cycles; empty-body
and 4 KiB-body whole-input synchronization use 12% more. Other controls are mixed,
including 7% more cycles for dense empty-packet demux/capture and 6% more for
index construction with 512 historical streams. Short noise prefixes show no
consistent benefit because the valid suffix and setup dominate their work.
All slower results remain in the trial records; no aggregate speedup is implied.

These measurements establish gains for the named inputs and operations on this
host; they do not establish a universal or full audio-decoding speedup.

## Correctness and reproduction

All **52 Ogg tests** pass, including the independent libogg corpus, historical,
capture and synchronization checks. The clean reference source remains pinned
at `be05b13e98b048f0b5a0f5fa8ce514d56db5f822`: 38 corpus files, 8,733 pages,
119,144 packets and 242,352 exact synchronization operations. Only manual golden
regeneration is skipped. All **113 regular Vorbis tests** pass; its 87
external/manual tests remain ignored. Ogg all-target/all-feature Clippy with
warnings denied, formatting, Python compilation and probe formatting pass.

Two new unit tests compare the word searches with independent scalar searches
for all 256 fill bytes, lengths 0..79 and every candidate position, plus repeated
and overlapping prefixes, borrow-propagation patterns and word/tail boundaries.

The [differential parser probe](validation/2026-10-28-recovery-probe.rs) compares
baseline and final libraries across **710 fixtures**, **17,040 pull-reader** and
**3,550 incremental-decoder configurations**. Inputs include every version/flag
byte, truncation at each byte of a short page, changed checksum/lacing/body bits,
nil and maximum-sized pages, noise at word/read boundaries, dense false prefixes
and a valid capture nested inside a failed CRC page. Pull readers use strict and
recovery modes, four read caps, interrupted reads, injected read errors and
successful/failed seeks. The probe records full returned/current pages, errors,
positions, discarded counts, decoder consumption, finish/reset behavior and
every underlying read request/result.

Both traces contain **56,896,181 identical bytes**, SHA-256
`154fadafd0e0dd6365060cbf6d8e9b84ab62d58102ae6d9446f4a1b2516da009`.
The [protocol record](validation/2026-10-28-recovery-protocol.json) pins commands,
source, libraries, executables and traces. The
[validation audit](validation/2026-10-28-recovery-checks.json) pins tested
source/tests, logs, isolated source diffs, benchmark controls and all timing
records. Saved workers and traces remain under ignored
`target/ogg-recovery-20261028/`.

From the repository root, choose a fresh ignored directory:

```powershell
python crates/deadlib-audio/decode/scripts/benchmark_ogg_kernels.py --root target/ogg-recovery-repeat --capture before --source-revision dbd98f729
python crates/deadlib-audio/decode/scripts/benchmark_ogg_kernels.py --root target/ogg-recovery-repeat --capture after --stage before --stage after --runs 8 --output target/ogg-recovery-repeat/results.tsv
```

For isolated comparisons, capture `8088086b4`, `552f25001` and `425153947` as
additional stages with `--source-revision`. Historical extraction leaves the
working tree in place and uses the same current probes for each stage.
