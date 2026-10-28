# Owned encoding and compact page views, 2026-10-28

This pass follows [page buffers and packet starts](BUFFERS.md), using production
source `13407a274` as its baseline. Three separately measured commits remove
intermediate encoding work and cached copies of immutable page metadata.

## Changes and resource costs

`749fc9468` builds an owned page directly in its reserved final vector. It
appends a 27-byte header prefix, lacing and payload, then writes the checksum.
The owned path no longer constructs a 282-byte streaming header or enters the
`Write` implementation for `Vec`. Streaming output retains its separate header
and borrowed payload. Both paths share header serialization and validation;
size checks, allocation order, error priority and streaming write calls remain
the same. This first commit retains separate header/body CRC updates.

`ae3f3105f` computes the owned page's CRC over the complete vector in one update.
Its checksum field is still zero at that point. This removes the intermediate
CRC seed and partial-word handling between adjacent header and body slices.
Streaming output retains separate updates because its payload is borrowed.

`2eedcc74a` stores only the immutable byte slice in `Page`. Header fields and the
body boundary are read from those already validated bytes when requested.
Construction stops eagerly decoding and copying metadata, and page arguments,
returns and events carry smaller values. Public methods and borrowed ranges are
unchanged. An explicit debug formatter preserves the previous field names,
values, order, flat formatting and alternate formatting.

Windows x64 layouts from the separately linked baseline/final probes:

| Type | Before bytes | Final bytes |
| --- | ---: | ---: |
| `Page` | 48 | 16 |
| `Option<Page>` | 48 | 16 |
| `LocatedPage` | 56 | 24 |
| `PageProgress` | 56 | 24 |
| `IndexEvent` | 120 | 88 |

There are no new dependencies, static tables or heap allocations. Production
retains `forbid(unsafe_code)`. Header caches and an intermediate encoding path
are removed; the extra shared serialization and debug code preserves behavior.
Production source grows by 27 lines.

## Measurement boundaries

Each accepted comparison uses the same 101 probes and helpers, matching compiler,
dependency locks, flags and profiles. Eight process trials alternate worker
order, placing each first four times. A probe warms once and reports the median
of eleven measured batches. Maximum-page pull/incremental probes use sixteen
operations per batch to reduce sensitivity to individual allocation samples.

The shared Windows x64 Xeon E5-2696 v4 uses logical CPU 2, rustc 1.99.1 /
LLVM 23.1.1, repository release/bench opt-level 3 and LTO, bench debug info and
static CRT. Frequency and other activity are uncontrolled. CPU throughput is
before/after Windows thread CPU cycles/op; all ns, min/max/MAD values remain in
the saved records.

Encoding probes construct and drop one owned page from resident fields, lacing
and body; fixture generation and input allocation are untimed. Small cases use
10,000 operations per batch and large cases 100. Parse probes validate one
resident page and consume the returned view without decoding a codec. Dense
demux/capture/index, capped readers, incremental input, writer and recovery
controls retain their existing boundaries. Heap counting is outside CPU timing.

The [direct-encoding trials](validation/2026-10-28-direct-encode.json),
[contiguous-CRC trials](validation/2026-10-28-encoding-crc.json),
[page-view trials](validation/2026-10-28-page-view.json), and
[combined trials](validation/2026-10-28-page-encoding-final.json) retain every
paired case and source/executable identity. Each corresponding TSV contains
1,616 rows, **6,464 timing rows total**.

## Isolated and combined results

Separate comparisons before and after each commit:

| Change / work | Before cycles | After cycles | CPU throughput |
| --- | ---: | ---: | ---: |
| Direct encoding, zero-byte body | 281.88 | 256.39 | 1.10x |
| Direct encoding, 36-byte body | 356.24 | 336.31 | 1.06x |
| Direct encoding, 228-byte body | 587.38 | 560.10 | 1.05x |
| Contiguous CRC, 36-byte body | 331.84 | 275.51 | 1.20x |
| Contiguous CRC, 228-byte body | 559.64 | 499.90 | 1.12x |
| Compact page, parse zero-byte body | 66.48 | 50.54 | 1.32x |
| Compact page, parse 36-byte body | 119.37 | 100.22 | 1.19x |
| Compact page, dense empty-packet index | 67,455.07 | 71,536.40 | 0.94x |
| Compact page, maximum page / unlimited reads | 113,406.06 | 103,165.00 | 1.10x |

Eight combined trials place each worker first four times. Against `13407a274`:

| Work | Before cycles | Final cycles | CPU throughput |
| --- | ---: | ---: | ---: |
| Encode, zero-byte body | 248.16 | 227.04 | 1.09x |
| Encode, 36-byte body | 324.38 | 264.10 | 1.23x |
| Encode, 228-byte body | 522.30 | 483.55 | 1.08x |
| Encode, 4 KiB body | 5,252.63 | 4,863.02 | 1.08x |
| Encode, maximum body | 101,154.39 | 97,621.54 | 1.04x |
| Parse, zero-byte body | 67.03 | 53.64 | 1.25x |
| Parse, 36-byte body | 105.53 | 110.14 | 0.96x |
| Parse, 228-byte body | 302.92 | 302.43 | 1.00x |
| Maximum page / unlimited reads | 99,646.12 | 98,109.65 | 1.02x |
| Maximum page / whole incremental input | 89,233.62 | 83,142.54 | 1.07x |
| Dense empty-packet index | 61,439.44 | 69,080.75 | 0.89x |

Every allocation, reallocation, free, requested-byte and peak-heap counter in
the measured work is identical across every paired workload of all four
comparisons. Fixture allocation is outside those counters; collections of page
values use the smaller layouts above.

The smaller representation trades cached metadata for reads from immutable
bytes. It does not improve all consumers. In the final comparison, dense
empty-packet index construction uses **12% more cycles**, 36-byte-body parsing
uses 4% more, and normal 17-byte-packet reading uses 18% more. Other slower
controls include 4 KiB noise followed by synchronization with repeated `Ogg`
prefixes (33% more), a maximum-page decoder fed in 4 KiB chunks (21% more), and
empty-body whole-input synchronization (21% more). Tiny-fragment traversal uses
18% more and single-stream packet-start lookup 16% more. All results remain in
the [final TSV](validation/2026-10-28-page-encoding-final.tsv); no aggregate or
universal speedup is implied.

The earlier one-operation large-page samples are retained in ignored
`initial-measurements/`. The longer accepted batches do not reproduce their
large-read slowdown. These shared-host comparisons establish gains for the
named operations and inputs, with no full audio-decoding throughput claim.

## Correctness and reproduction

All **52 Ogg tests** pass, including independent libogg corpus, historical,
capture and synchronization checks. The clean reference source remains pinned
at `be05b13e98b048f0b5a0f5fa8ce514d56db5f822`: 38 corpus files, 8,733 pages,
119,144 packets and 242,352 exact synchronization operations. Only manual golden
regeneration is skipped. All **113 regular Vorbis tests** pass; its 87
external/manual tests remain ignored. Ogg all-target/all-feature Clippy with
warnings denied, formatting, Python compilation and probe formatting pass.

The [encoding probe](validation/2026-10-28-encoding-probe.rs) compares **12,288
encoding/validation cases**: all flag bytes, zero/high-bit header fields, empty
and maximum lacing tables, complete payloads, mismatched lengths and size limits.
It records complete owned/streamed bytes, exact errors and flat/alternate page
debug output. Another **45 partial-write cases** record every write request,
returned count, interruption/error and flush count. Both traces contain
**26,967,237 identical bytes**, SHA-256
`471276a1bdc460e6b992667e341f97e6252633c55a869a00b9fa1290cced2403`.
The probe's separate `--layout` operation records the type sizes above.

The existing [parser/IO probe](validation/2026-10-28-recovery-probe.rs) also
matches across 710 fixtures, **17,040 pull-reader** and **3,550 incremental-decoder
configurations**. Full pages, errors, positions, discarded counts, decoder
consumption, finish/reset behavior and every underlying read request/result
remain **56,896,181 identical bytes**, SHA-256
`154fadafd0e0dd6365060cbf6d8e9b84ab62d58102ae6d9446f4a1b2516da009`.

The [protocol record](validation/2026-10-28-encoding-protocol.json) pins commands,
source/library/executable hashes, complete traces and layouts. The
[validation audit](validation/2026-10-28-encoding-checks.json) pins tested
source/tests, logs, source diffs, build controls and timing records.

From the repository root, choose a fresh ignored directory:

```powershell
python crates/deadlib-audio/decode/scripts/benchmark_ogg_kernels.py --root target/ogg-encoding-repeat --capture before --source-revision 13407a274
python crates/deadlib-audio/decode/scripts/benchmark_ogg_kernels.py --root target/ogg-encoding-repeat --capture after --stage before --stage after --runs 8 --output target/ogg-encoding-repeat/results.tsv
```

For isolated comparisons, capture `749fc9468`, `ae3f3105f` and `2eedcc74a` as
additional stages with `--source-revision`. Historical extraction leaves the
working tree in place and uses the same current probes for every stage. Saved
workers and traces remain under ignored `target/ogg-page-encoding-20261028/`.
