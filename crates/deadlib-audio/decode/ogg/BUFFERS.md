# Complete pages, short reads and packet starts, 2026-10-28

This pass follows [buffered verification and recovery](RECOVERY.md), using
production source `414ad31ac` as its baseline. Three changes remove staged
buffer growth, repeated buffer initialization and repeated index searches.

## Changes and resource costs

`127dfa931`: when an incremental decoder starts with a complete page in its
input, it now copies that page once into a buffer sized for the final page
length. Header and lacing validation identify that exact length first; CRC is still checked after
copying. Partial input and invalid headers retain the existing staged path.
Only one page is consumed, including when more input follows it. Borrowed page
lifetime, terminal errors, finish and reset behavior stay the same.

`fcbf5b8a9`: the pull reader initializes a fill's destination once, retaining
the unread suffix across short reads. Previously each short read truncated the
vector and the next read resized it again, repeatedly zeroing the remaining space. For a
65,025-byte body returned one byte at a time, that wrote about 2 GiB of zeros.
The new loop initializes each destination byte once. A local count tracks valid
bytes and truncates correctly on completion, EOF and IO error. Recovery's batch
read still returns after one underlying read. Read order and requested lengths
remain unchanged; no persistent field or buffer is added.

`88f414e39`: packet-start lookup selects its stream once and reuses that
stream's page range. A packet beginning on the already selected page returns
that page directly.
Only an incoming continuation whose packet started earlier needs the second
binary search. This removes two linear serial searches and the common second
binary search, preserving the exact returned entry and continuation fallback.

Production remains safe Rust with `forbid(unsafe_code)`. There are no new fields,
dependencies or static tables; production source grows by 22 lines. Complete
small-page decoding changes from one allocation plus one reallocation to one
allocation: cumulative requested bytes
fall from **81 to 45**, and peak decoder heap from **54 to 45 bytes**. A complete
maximum page changes from one allocation plus two reallocations to one allocation;
requested bytes fall from **65,616 to 65,307**, with the same **65,307-byte** peak.
All other measured allocation, reallocation, free, requested-byte and peak-heap
counters are unchanged.

## Isolated measurements

Windows thread CPU cycles/op, median over eight alternating process trials.
Each probe warms once and reports the median of eleven batches, using logical
CPU 2 on the shared Windows x64 Xeon E5-2696 v4. Rustc 1.99.1 / LLVM 23.1.1,
repository release/bench optimization/LTO settings, bench debug info and static
CRT match across workers. Frequency and other machine activity are uncontrolled.
CPU throughput is before/after cycles; all ns, min/max/MAD and heap values remain
in the saved summaries.

| Change / work | Before cycles | After cycles | CPU throughput |
| --- | ---: | ---: | ---: |
| Complete page, 17-byte body / 4 KiB input chunks | 509.13 | 281.07 | 1.81x |
| Complete page, 17-byte body / whole input | 496.29 | 285.68 | 1.74x |
| Complete page, maximum body / whole input | 122,261.00 | 121,822.50 | 1.00x |
| Short reads, 17-byte body / one-byte reads | 1,739.64 | 877.78 | 1.98x |
| Short reads, maximum body / one-byte reads | 140,878,940.00 | 476,534.50 | 295.63x |
| Short reads, maximum body / seven-byte reads | 19,808,997.50 | 199,306.50 | 99.39x |
| Short reads, maximum body / 4 KiB reads | 149,370.00 | 118,749.50 | 1.26x |
| Packet start, no historical streams | 214.21 | 93.54 | 2.29x |
| Packet start, 64 historical streams | 561.67 | 235.53 | 2.38x |
| Packet start, 512 historical streams | 2,345.96 | 827.12 | 2.84x |

Incremental probes construct a decoder, feed one framing-valid page and check
completion, including allocation and cleanup. Pull probes construct a strict
reader over resident input, cap each underlying read, consume one framing-valid
page and EOF, then clean up. They measure an in-process short-read adapter; the
large ratios do not predict network or disk syscall throughput. Maximum pages
contain 65,025 body bytes and 65,307 total bytes. Fixture generation, file loading,
verification and heap counting are outside timing.

Lookup fixtures contain 0, 64 or 512 completed single-packet streams followed by
a 2,048-page stream. Index construction is untimed. Each operation looks up one
packet of the final stream; packet numbers advance by 17 modulo 2,048. The query
probes allocate nothing. Existing spanning-packet, multiplexing and nil-page
correctness cases exercise the earlier-page fallback separately.

The [complete-page trials](validation/2026-10-28-complete-page.json),
[read-initialization trials](validation/2026-10-28-read-initialization.json), and
[packet-start trials](validation/2026-10-28-packet-start-lookup.json) retain every
case and its source/executable identity. Each uses the same 96 probes and
helpers, matching dependency locks, profiles, flags and compiler identity. Each
corresponding TSV contains 1,536 rows.

## Combined result and limits

Eight final trials place each worker first four times. Against `414ad31ac`:

| Work | Before cycles | Final cycles | CPU throughput |
| --- | ---: | ---: | ---: |
| Decoder, 17-byte body / whole input | 512.75 | 294.35 | 1.74x |
| Decoder, maximum body / whole input | 127,749.00 | 115,128.00 | 1.11x |
| Pull reader, 17-byte body / one-byte reads | 1,691.25 | 858.25 | 1.97x |
| Pull reader, maximum body / one-byte reads | 141,342,304.50 | 466,327.50 | 303.10x |
| Pull reader, maximum body / seven-byte reads | 20,309,018.00 | 192,501.50 | 105.50x |
| Pull reader, maximum body / 4 KiB reads | 152,114.00 | 116,883.50 | 1.30x |
| Pull reader, maximum body / unlimited reads | 121,603.00 | 138,394.50 | 0.88x |
| Packet start, no historical streams | 217.28 | 90.51 | 2.40x |
| Packet start, 64 historical streams | 545.39 | 233.94 | 2.33x |
| Packet start, 512 historical streams | 2,309.14 | 812.88 | 2.84x |

The [final 1,536 rows](validation/2026-10-28-page-buffers-final.tsv) and
[manifest/summary](validation/2026-10-28-page-buffers-final.json) complete the
**6,144 accepted timing rows**. Allocation changes match the isolated decoder
comparison; all other paired heap counters remain identical in all four comparisons.

The complete-page change demonstrates CPU gains on small pages and fewer
reallocations on large pages. Its isolated maximum-page CPU result is unchanged;
the combined result improves against this pass's baseline but does not establish
that the earlier whole-page incremental regression is fixed.

Slower controls remain: unlimited-read maximum-page parsing uses about **14%
more cycles** in the final comparison, and the ordinary 2,048 one-byte-packet
reader uses 12% more. The unchanged mixed-fragment kernel uses 11% more, 17-byte
packet writing uses 7% more, and single-stream index construction uses 6% more.
Other controls are mixed; every result remains in the trial records. The large
short-read gains describe the named resident adapters and input schedules.
Named gains remain workload-specific, with no universal or full audio-decoding
throughput claim.

## Correctness and reproduction

All **52 Ogg tests** pass, including independent libogg corpus, historical,
capture and synchronization checks. The clean reference source is pinned at
`be05b13e98b048f0b5a0f5fa8ce514d56db5f822`: 38 corpus files, 8,733 pages,
119,144 packets and 242,352 exact synchronization operations. Only manual golden
regeneration is skipped. All **113 regular Vorbis tests** pass; its 87
external/manual tests remain ignored. Ogg all-target/all-feature Clippy with
warnings denied, formatting, Python compilation and probe formatting pass.

The existing [parser/IO probe](validation/2026-10-28-recovery-probe.rs) compares
baseline and final libraries across 710 fixtures: **17,040 pull-reader** and
**3,550 incremental-decoder configurations**. It records full returned/current
pages, errors, positions, discarded counts, decoder consumption, finish/reset
behavior and every underlying read request/result. Cases include all version
and flag bytes, truncated/changed headers and bodies, nil and maximum pages,
noise, nested captures, strict/recovery modes, short/interrupted reads, injected
IO errors and successful/failed seeks. Both traces remain **56,896,181 identical
bytes**, SHA-256 `154fadafd0e0dd6365060cbf6d8e9b84ab62d58102ae6d9446f4a1b2516da009`.

The [packet-origin probe](validation/2026-10-28-lookup-probe.rs) compares **784
policy/fixture configurations**, including multiplexed packet spans, nil
continued pages, malformed EOS and compatibility granules. It records full
demux/capture observations and index metadata, then **2,180 packet-start queries**
and **220 missing-serial queries**, including every packet in each successful
index and out-of-range/u64-maximum packet numbers. Both traces contain
**3,800,798 identical bytes**, SHA-256
`32b8e1e700432a1212f60a4eb415dd53f10385efba6934d488dc174889ffd46b`.
Existing tests independently compare indexed packet origins with full demux and
capture exact suffixes across generated and corpus page boundaries.

The [protocol record](validation/2026-10-28-page-buffers-protocol.json) pins both
probes' commands, source/library/executable hashes and complete traces.
The [validation record](validation/2026-10-28-page-buffers-checks.json) pins tested
source/tests, logs, isolated source diffs, build controls and timing rows.

From the repository root, choose a fresh ignored directory:

```powershell
python crates/deadlib-audio/decode/scripts/benchmark_ogg_kernels.py --root target/ogg-buffers-repeat --capture before --source-revision 414ad31ac
python crates/deadlib-audio/decode/scripts/benchmark_ogg_kernels.py --root target/ogg-buffers-repeat --capture after --stage before --stage after --runs 8 --output target/ogg-buffers-repeat/results.tsv
```

Historical extraction leaves the working tree in place and uses the same current
probes for each stage. For isolated comparisons, capture `127dfa931`, `fcbf5b8a9`
and `88f414e39` as additional stages with `--source-revision`.
Saved workers remain in ignored
`target/ogg-page-buffers-20261028/`.
