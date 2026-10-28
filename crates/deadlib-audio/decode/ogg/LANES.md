# Packet-count lanes and short fragment tails, 2026-10-29

Starting from `db753972f`, this pass retains two safe Rust changes to page
metadata traversal. A third candidate, a 128-bit scan of long lacing runs, is
rejected after paired controls expose medium-fragment slowdowns.

## Work removed

Packet counting already identifies each 255 byte with a bit in its byte lane.
It now adds those lane bits across the page and folds them horizontally once.
A page has at most 255 laces: at most 31 full words contribute one per lane,
and the sum of the eight lanes is at most 248. Neither the lanes nor their
horizontal total can carry into another byte. The scalar tail contributes at
most seven more. The code removes up to 30 multiplications and horizontal
shifts per page; it preserves the existing predicate for every byte value.

The fragment iterator already knows that its first four laces are 255 when it
enters its fallback. For five through seven laces it now searches only the
remaining one through three laces inline. This removes the outlined scan call
and its redundant prefix search. At eight or more laces, the original word
scan remains. Fragment lengths, slice origins and starts/ends flags are
unchanged. These changes add 11 production lines, without new fields,
allocations, dependencies or unsafe code.

## Earlier control-regression investigation

The [previous pass](FRAMING.md) reported slower full-worker CRC and fragment
controls despite unchanged source in those kernels. A separate
[control driver](validation/2026-10-29-codegen-controls.rs) links the saved
`64901881c` baseline and retained framing source in one process. CRC calls use
black-boxed function pointers, and fragment iteration uses matching outlined
wrappers over prevalidated pages. Eight processes produce
[208 observations](validation/2026-10-29-codegen-controls.json).

Both CRC pointers resolve to the same machine function in every run. The
emitted optimized assembly contains one CRC definition; both fragment wrappers
also call the same outlined lacing scan. The 4 KiB CRC comparison is 1.000x
paired throughput, 65,025 bytes is 1.002x, and the two 255-lace fragment
controls are 0.987x and 1.006x. These do not reproduce the previous 13.4% CRC
or 25.9% long-fragment cost increases in this controlled context.

This establishes that those reports were not changes to the intrinsic CRC or
scan implementation. It does not erase the costs measured in the larger
worker, isolate compiler layout from host effects, or establish a general
pipeline speedup. The record pins source, linked libraries, compiler flags,
assembly/executable hashes, function-alias evidence and every cycle/ns row.

## Measurement boundaries

The full worker uses the same 207 workloads and identical benchmark sources,
profiles and dependency lock as this pass baseline. Eight trials rotate four
saved versions: before, lane counts, short fallback, and the rejected wide-scan
candidate. Each worker runs first twice, producing 6,624 CPU/heap rows. Every
observation is the median of eleven measured batches after warmup. Heap
counters come from a separate untimed API operation. Generation and parsing
precede fragment/count timing; resident-input reader, demux, capture, index,
synchronization and writer controls include their existing API work.

Windows QueryThreadCycleTime measures a thread pinned to logical CPU 2 on a
shared Xeon E5-2696 v4 host. Frequency and other activity are uncontrolled.
Builds use rustc 1.99.1 / LLVM 23.1.1, opt-level 3, LTO, debug symbols and static
CRT. The [full record](validation/2026-10-29-lanes.json) pins build controls,
source and binary hashes, cycles, nanoseconds, min/max/MAD and heap counters;
all [raw rows](validation/2026-10-29-lanes.tsv) are retained.

The [paired driver](validation/2026-10-29-lanes-paired.rs) links the four source
versions in one process with matching optimized LTO flags. Twelve batches
alternate before/after; warmup is discarded. Eight processes produce
[1,176 observations](validation/2026-10-29-lanes-paired.json) across 147 cases:
45 packet-count, 48 short-fallback, 48 rejected-wide and six CRC controls. Each
count/fragment batch contains 65,536 operations on prevalidated page views.
Count pattern 0 is all 255, pattern 1 all 1, and pattern 2 is
`(position * 73 + 127) as u8`. Fragment cases contain a run of 255 laces, with
an optional final 17. Paired throughput is the median of each process's cycle
ratio; cycle columns are separate medians. Source hashes and exact compiler
commands distinguish the retained and rejected versions.

## CPU and allocation results

Packet-count lane folding gains **1.55x to 2.17x** paired throughput at 255
laces across the three patterns, winning all eight processes in each case.
The inline short fallback gains on five- and seven-lace fragments; the table
retains nearby controls as well. These are gains on resident page metadata,
not a claim of proportional full decoding speedup.

| Paired count: laces / pattern | Before cycles | After cycles | Paired CPU throughput | Faster processes |
| --- | ---: | ---: | ---: | ---: |
| `count/7/0` | 9.94 | 10.03 | 0.989x | 2/8 |
| `count/7/1` | 9.93 | 10.18 | 0.982x | 0/8 |
| `count/7/2` | 10.21 | 10.20 | 1.002x | 6/8 |
| `count/8/0` | 12.89 | 12.55 | 1.050x | 6/8 |
| `count/8/1` | 12.93 | 12.21 | 1.055x | 8/8 |
| `count/8/2` | 43.66 | 44.02 | 0.994x | 3/8 |
| `count/16/0` | 41.33 | 40.42 | 1.020x | 6/8 |
| `count/16/1` | 15.16 | 14.25 | 1.047x | 7/8 |
| `count/16/2` | 15.46 | 14.78 | 1.047x | 8/8 |
| `count/32/0` | 47.00 | 42.92 | 1.102x | 8/8 |
| `count/32/1` | 18.95 | 15.60 | 1.193x | 8/8 |
| `count/32/2` | 18.80 | 15.64 | 1.190x | 8/8 |
| `count/128/0` | 69.88 | 53.15 | 1.312x | 8/8 |
| `count/128/1` | 51.89 | 26.62 | 1.972x | 8/8 |
| `count/128/2` | 52.58 | 26.16 | 2.022x | 8/8 |
| `count/255/0` | 108.32 | 50.34 | 2.146x | 8/8 |
| `count/255/1` | 104.81 | 47.92 | 2.174x | 8/8 |
| `count/255/2` | 105.09 | 67.72 | 1.546x | 8/8 |

| Paired short fallback: laces / terminated | Before cycles | After cycles | Paired CPU throughput | Faster processes |
| --- | ---: | ---: | ---: | ---: |
| `short/5/false` | 25.49 | 18.92 | 1.340x | 8/8 |
| `short/5/true` | 26.12 | 18.70 | 1.391x | 8/8 |
| `short/7/false` | 26.76 | 20.81 | 1.288x | 8/8 |
| `short/7/true` | 26.65 | 21.07 | 1.252x | 8/8 |
| `short/8/true` | 23.17 | 23.51 | 0.992x | 2/8 |
| `short/17/false` | 25.63 | 25.12 | 1.024x | 6/8 |
| `short/128/false` | 55.02 | 54.36 | 1.015x | 7/8 |
| `short/255/true` | 71.22 | 71.29 | 1.003x | 5/8 |

| Full-worker workload | Baseline cycles | Folded counts | Short fallback (retained) | Wide scan (rejected) | Retained CPU throughput |
| --- | ---: | ---: | ---: | ---: | ---: |
| `completed/tiny` | 117.38 | 48.12 | 44.68 | 45.20 | 2.627x |
| `completed/mixed` | 115.76 | 54.44 | 45.56 | 46.22 | 2.541x |
| `completed/continued` | 109.34 | 48.50 | 47.08 | 54.33 | 2.322x |
| `fragment-prefix/5/false` | 20.14 | 22.18 | 13.92 | 15.38 | 1.447x |
| `fragment-prefix/255/true` | 81.53 | 67.86 | 63.58 | 60.36 | 1.282x |
| `dense-demux/0` | 63,960.24 | 62,121.92 | 63,565.83 | 66,834.29 | 1.006x |
| `dense-capture/0` | 72,393.83 | 66,601.11 | 58,194.93 | 62,629.50 | 1.244x |
| `dense-index/0` | 43,543.35 | 36,663.35 | 37,218.93 | 38,247.90 | 1.170x |
| `crc/4096` | 4,026.28 | 4,224.93 | 3,842.89 | 3,662.25 | 1.048x |
| `span-demux/4` | 31,758.91 | 29,025.46 | 26,041.62 | 27,139.13 | 1.220x |
| `nil-demux` | 100,242.88 | 111,259.06 | 97,897.00 | 106,855.25 | 1.024x |
| `chained-demux/64` | 154,569.12 | 149,040.50 | 149,987.13 | 131,768.57 | 1.031x |
| `reader/8192` | 22,568,332.00 | 20,897,607.00 | 20,797,844.50 | 22,185,853.00 | 1.085x |

All six heap counters are identical in every stage for all 207 workloads:
allocations, reallocations, frees, cumulative allocated/freed bytes and peak
live bytes. The metadata-only count and fragment operations remain
allocation-free. Demux, capture and index allocation policies are unchanged.

The largest slower controls in the retained build remain below. Compiler
layout and shared-host effects are not isolated by the full worker. The source
changes remove specific repeated work; the complete control data is retained
to limit the conclusions and guide further investigation.

| Slower retained control | Baseline cycles | Retained cycles | CPU cost increase |
| --- | ---: | ---: | ---: |
| `fragment-prefix/255/false` | 66.13 | 85.12 | 28.7% |
| `incremental/65025/7` | 363,656.66 | 435,145.00 | 19.7% |
| `fragment-prefix/17/false` | 20.77 | 24.54 | 18.1% |
| `decoder-header/128/27` | 220.59 | 257.69 | 16.8% |
| `decoder-header/128/7` | 787.89 | 913.12 | 15.9% |
| `fragments/continued` | 69.09 | 79.77 | 15.4% |
| `sync/65025/18446744073709551615` | 83,629.50 | 95,373.00 | 14.0% |
| `fragment-prefix/8/false` | 17.35 | 19.71 | 13.6% |
| `crc-tail/4` | 13.59 | 15.41 | 13.4% |
| `incremental/65025/1` | 2,061,187.25 | 2,268,731.41 | 10.1% |
| `capped-reader/17/18446744073709551615` | 581.79 | 637.75 | 9.6% |
| `noise-reader/1048576/2` | 1,764,615.25 | 1,932,025.25 | 9.5% |

Selected rejected-candidate results show the tradeoff that caused rejection:

| Rejected wide scan: laces / terminated | Before cycles | After cycles | Paired CPU throughput | Faster processes |
| --- | ---: | ---: | ---: | ---: |
| `wide192/16/true` | 24.82 | 26.91 | 0.920x | 0/8 |
| `wide192/17/false` | 25.81 | 30.45 | 0.847x | 0/8 |
| `wide192/128/false` | 54.73 | 56.50 | 0.976x | 2/8 |
| `wide192/192/true` | 54.66 | 48.55 | 1.115x | 8/8 |
| `wide192/193/true` | 64.27 | 65.69 | 0.979x | 3/8 |
| `wide192/200/false` | 58.59 | 52.34 | 1.134x | 8/8 |
| `wide192/254/false` | 72.61 | 63.23 | 1.149x | 8/8 |
| `wide192/255/true` | 74.46 | 66.47 | 1.118x | 8/8 |


## Rejected scan and exact behavior

The 128-bit candidate scans two words per loop using safe little-endian loads
and trailing-zero counts. Unconditional use harmed short runs. Restricting it
to at least 64 laces still hurt medium cases; a 192-lace cutoff gained on
selected long runs but retained medium-case regressions, including the
17-lace unterminated case. None of these variants is retained. The final
[rejected patch](validation/2026-10-29-wide-scan-rejected.patch), applied to the
retained short-fallback source, reproduces the measured `wide192` candidate.

Existing tests check every terminating position across 255 laces with multiple
terminating-byte bit patterns, exact fragment slices/pointers and continuation
flags. Packet-count tests cover every byte value across word/tail boundaries.
The [checks](validation/2026-10-29-lanes-checks.json) pin final production/test
hashes, a clean libogg reference revision, oracle hash, commands and untrimmed
logs. Release validation passes 58 Ogg tests and 113 Vorbis tests (87 ignored),
Clippy with warnings denied, formatting and Python compilation.

Six [full protocol comparisons](validation/2026-10-29-lanes-protocol.json)
compare the retained source against the baseline, including errors, callbacks,
state, Debug output, source positions, IO and encoded bytes:

| Protocol | Equal bytes |
| --- | ---: |
| Index and packet origins | 3,649,232 |
| Incremental decoder | 5,808,523 |
| Index lookup | 3,800,798 |
| Capture state | 78,020,534 |
| Packet writer | 46,897,193 |
| Public page encoding and IO | 26,967,237 |

## Reproduce

Use the committed benchmark controls for every source revision:

```powershell
python crates/deadlib-audio/decode/scripts/benchmark_ogg_kernels.py --root target/ogg-lanes-replay --capture before --source-revision db753972f
python crates/deadlib-audio/decode/scripts/benchmark_ogg_kernels.py --root target/ogg-lanes-replay --capture counts --source-revision 5dcc56094
python crates/deadlib-audio/decode/scripts/benchmark_ogg_kernels.py --root target/ogg-lanes-replay --capture short --source-revision e98e827e7
python crates/deadlib-audio/decode/scripts/benchmark_ogg_kernels.py --root target/ogg-lanes-replay --stage before --stage counts --stage short --runs 9 --output target/ogg-lanes-replay/results.tsv
```

The recorded four-stage experiment also includes the rejected patch; recreate
it from the short-fallback revision and capture `wide192` before running eight
rotating trials of all four stages. Paired/protocol JSON records retain exact
rustc commands, library hashes and source paths. The audit verifies that the
retained commits match their measured source snapshots and the rejected patch
matches its separate snapshot. Timings describe these builds and fixtures on
this host, not portable performance guarantees.
