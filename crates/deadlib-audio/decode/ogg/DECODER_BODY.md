# Incremental bodies and empty polls, 2026-10-29

This pass starts at `7f513dd3a`, following the [lane-folding pass](LANES.md).
Three safe Rust changes shorten incremental decoding: a partial-body bypass,
a direct empty-poll return and one combined fast-path state check.

## Work removed

Once the page length is known, an input shorter than the remaining body cannot
finish the page. The decoder reserves for that exact input, appends it directly
and returns its complete consumed length. It skips the complete-page probe,
header/target planning loop, minimum and input-subslice calculations, and final CRC
completion test. Header and completing-body inputs keep the existing path.
Fallible reservation still precedes copying, with the same byte count and
terminal error handling. There is no advance allocation or new buffer policy.

An empty push now returns zero consumed bytes and no page after the existing
failed-state check and ready-page invalidation. All framing checks permitted
by the available bytes already ran before the preceding push returned: a new invalid fixed
header or checksum is reported in the push that completes it. Empty input
cannot add bytes or complete an incomplete page, so repeating framing checks,
zero-byte reservation and copying adds no information. Clearing a prior ready
page still happens first, and failed decoders still return `Error::Failed`.

The normal active path now combines failed/ready predicates in one boolean
check. When either state is set, failure returns immediately; the ready page
is cleared only when no failure is set. Both predicates are
side-effect-free. This removes one normal-path conditional branch while
retaining the error and page-lifetime rules.

Production adds 13 lines. No field, public signature, dependency or unsafe
code is added. `PageDecoder` remains 40 bytes on this Windows x64 build.

## Measurement boundaries

The full worker uses the same 207 probes, inputs, helpers, compiler profile and
dependency lock across four saved versions: before, partial-body bypass, empty
poll return, and combined statuses (retained). Eight trials rotate
the first stage; each runs first twice. Every observation uses the median of
eleven measured batches after warmup. Heap counters come from a separate
untimed operation. All 6,624 CPU/heap observations are in the
[raw data](validation/2026-10-29-decoder-body.tsv); the
[record](validation/2026-10-29-decoder-body.json) pins sources, binaries,
compiler/profile, cycle/ns medians, ranges and median absolute deviations.

Windows QueryThreadCycleTime measures a benchmark thread pinned to logical
CPU 2 on the shared Xeon E5-2696 v4 host. Frequency and other activity are
uncontrolled. Builds use rustc 1.99.1 / LLVM 23.1.1, opt-level 3, LTO, benchmark
debug symbols and static CRT. Inputs stay resident and generation/encoding is
outside timing. These measurements describe the recorded builds and fixtures.

The [paired driver](validation/2026-10-29-decoder-body-paired.rs) links the four
versions with matching optimized LTO flags in one process. Twelve batches
alternate before/after; warmup is discarded. Eight processes yield
[672 CPU observations and 96 polling heap comparisons](validation/2026-10-29-decoder-body-paired.json).

Factory operations construct a decoder, feed a framing-valid page in chunks,
consume exactly one page, finish and free its buffer. Bodies are 17, 4,096 or
65,025 bytes; chunks are 1, 7, 4,096 or unlimited. A batch has 500 small-page
operations or eight large-page operations. Header operations warm and retain a
decoder's capacity, reset it, and feed the complete fixed header plus 1, 17,
128 or 255 laces containing 255, in chunks of 1, 7 or 27. The body is absent.
Each header batch has 1,000 operations.

Polling batches contain 65,536 operations on prepared decoder states. State 0
is new/empty, 1 holds one fixed-header byte, 2 holds the 27-byte fixed header
with missing laces, 3 holds a complete 282-byte header plus one body byte, and
5 is terminal after an invalid capture. State 4 is a **ready-page cycle**:
poll with empty input, then feed a complete 17-byte-body page to restore ready
state for the next operation. Its cost includes that page copy and CRC. Other
polling states persist through repeated empty pushes. Allocation counters use
one separate operation in each prepared state, after timing, and exclude setup.
Paired throughput is the median of each process's cycle ratio; cycle columns
are separate medians.

## CPU and heap results

The partial-body bypass gains **8.4% to 13.2%** paired CPU throughput on
4,096- and 65,025-byte bodies fed in one- or seven-byte chunks, winning all
eight processes in each case. Empty polls improve by **16.9% to 65.2%** in the
prepared incomplete states. The extra empty-input check costs throughput on
some nonempty inputs; these controls are included below.

| Paired body case: bytes / chunk | Before cycles | Partial bypass cycles | Paired CPU throughput | Faster processes |
| --- | ---: | ---: | ---: | ---: |
| `17/1` | 2,338.11 | 2,342.50 | 1.032x | 6/8 |
| `17/7` | 1,094.42 | 1,075.11 | 1.024x | 7/8 |
| `17/4096` | 244.09 | 288.43 | 0.820x | 0/8 |
| `17/18446744073709551615` | 242.99 | 276.35 | 0.860x | 0/8 |
| `4096/1` | 153,087.57 | 129,559.87 | 1.123x | 8/8 |
| `4096/7` | 25,105.31 | 23,129.87 | 1.084x | 8/8 |
| `4096/4096` | 5,844.12 | 5,899.00 | 0.995x | 0/8 |
| `4096/18446744073709551615` | 5,460.07 | 5,446.32 | 1.000x | 1/8 |
| `65025/1` | 2,294,803.94 | 1,998,067.37 | 1.132x | 8/8 |
| `65025/7` | 408,119.12 | 367,017.69 | 1.093x | 8/8 |
| `65025/4096` | 163,225.68 | 167,698.00 | 0.984x | 1/8 |
| `65025/18446744073709551615` | 74,369.32 | 72,448.68 | 1.000x | 5/8 |

| Paired empty-check control: bytes / chunk | Partial bypass cycles | Empty return cycles | Paired CPU throughput | Faster processes |
| --- | ---: | ---: | ---: | ---: |
| `17/1` | 2,149.35 | 2,126.95 | 1.001x | 4/8 |
| `17/7` | 982.48 | 971.29 | 0.984x | 2/8 |
| `17/18446744073709551615` | 272.18 | 260.99 | 1.083x | 5/8 |
| `4096/1` | 140,699.50 | 141,179.62 | 0.996x | 3/8 |
| `4096/7` | 23,856.88 | 24,062.68 | 0.984x | 2/8 |
| `65025/1` | 1,917,112.94 | 1,961,438.38 | 0.958x | 1/8 |
| `65025/7` | 374,192.62 | 367,648.75 | 0.988x | 3/8 |
| `65025/4096` | 155,625.44 | 153,512.75 | 1.005x | 5/8 |

| Paired polling state | Partial bypass cycles | Empty return cycles | Paired CPU throughput | Faster processes |
| --- | ---: | ---: | ---: | ---: |
| `0` | 16.47 | 11.58 | 1.408x | 8/8 |
| `1` | 16.45 | 11.89 | 1.409x | 8/8 |
| `2` | 18.89 | 11.40 | 1.652x | 8/8 |
| `3` | 13.96 | 12.13 | 1.169x | 8/8 |
| `4` | 155.74 | 87.90 | 1.735x | 8/8 |
| `5` | 11.84 | 11.16 | 1.062x | 8/8 |

State 4 includes refeeding a complete page; it is not an isolated empty-call
cost. The failed-state control still takes the original failure branch: its
timing difference is not work removed by the empty return. Compiler/layout and
shared-host effects are not separated by these comparisons. The 65,025-byte,
one-byte nonempty case loses **4.2%** throughput after the empty-check stage.
The benefit depends on how callers poll and feed input; this is not a general
incremental-decoder speedup for every chunk pattern.

| Full-worker workload | Baseline cycles | Partial body | Empty poll | Combined status (retained) | Retained CPU throughput |
| --- | ---: | ---: | ---: | ---: | ---: |
| `incremental/17/1` | 2,460.60 | 2,332.84 | 2,387.28 | 2,427.23 | 1.014x |
| `incremental/17/7` | 1,092.78 | 1,074.56 | 1,076.10 | 1,079.94 | 1.012x |
| `incremental/17/4096` | 253.52 | 260.11 | 261.42 | 260.44 | 0.973x |
| `incremental/17/18446744073709551615` | 254.07 | 260.65 | 261.86 | 264.94 | 0.959x |
| `incremental/65025/1` | 2,469,203.41 | 2,146,689.53 | 2,242,020.94 | 2,223,761.29 | 1.110x |
| `incremental/65025/7` | 476,678.57 | 434,088.66 | 447,512.50 | 448,383.62 | 1.063x |
| `incremental/65025/4096` | 104,921.00 | 105,572.62 | 107,397.22 | 107,006.28 | 0.981x |
| `incremental/65025/18446744073709551615` | 89,576.56 | 89,967.53 | 89,768.63 | 92,011.68 | 0.974x |
| `decoder-header/255/1` | 10,079.44 | 10,290.71 | 11,373.83 | 10,768.23 | 0.936x |
| `decoder-header/255/7` | 1,581.39 | 1,609.60 | 1,805.61 | 1,693.00 | 0.934x |
| `decoder-header/255/27` | 460.51 | 477.30 | 493.22 | 468.19 | 0.984x |
| `reader/8192` | 24,847,181.00 | 23,275,122.00 | 24,657,860.50 | 25,018,500.00 | 0.993x |
| `capped-reader/65025/1` | 438,142.53 | 436,132.75 | 436,612.94 | 440,323.81 | 0.995x |
| `crc/4096` | 4,111.45 | 4,105.09 | 4,114.19 | 4,110.36 | 1.000x |

All six heap counters are identical across all four stages for all 207 full
workloads: allocations, reallocations, frees, cumulative allocated/freed bytes
and peak live bytes. All 96 focused polling before/after heap comparisons are
zero for every counter, including the ready-page cycle after capacity warmup.
The decoder retains its buffer, allocation order and growth policy.

The largest slower full-worker controls for the retained build are below.
Their costs remain observations of these builds even where source is unchanged.
The paired results support narrower per-change conclusions; every full-worker
row remains available for further investigation.

| Slower retained control | Baseline cycles | Retained cycles | CPU cost increase |
| --- | ---: | ---: | ---: |
| `mux-index/2/true` | 37,356.18 | 42,102.88 | 12.7% |
| `mux-index/2/false` | 35,888.32 | 40,086.12 | 11.7% |
| `chained-index/512` | 1,216,331.88 | 1,336,288.62 | 9.9% |
| `noise-sync/1048576/3` | 3,693,965.38 | 4,027,001.87 | 9.0% |
| `chained-index/64` | 723,170.25 | 784,465.50 | 8.5% |
| `chained-index/0` | 511,544.62 | 552,371.88 | 8.0% |
| `mux-index/16/false` | 424,238.62 | 456,957.82 | 7.7% |
| `mux-index/16/true` | 431,276.32 | 462,609.93 | 7.3% |
| `decoder-header/255/7` | 1,581.39 | 1,693.00 | 7.1% |
| `decoder-header/255/1` | 10,079.44 | 10,768.23 | 6.8% |
| `decoder-header/128/1` | 5,492.33 | 5,837.50 | 6.3% |
| `lacing-header/8` | 41.28 | 43.66 | 5.8% |

The combined status check gains **11.1% to 13.0%** on normal empty polls
(states 0 through 3), winning all eight paired processes in every state. The
ready-page cycle loses **2.4%** throughput and terminal failed polls lose
**6.8%**. Nonempty body cases are nearly flat. The full worker recovers much of
the header-only cost introduced by the earlier checks; these are workload
tradeoffs, not a universal speedup.

| Combined status case | Before cycles | After cycles | Paired CPU throughput | Faster processes |
| --- | ---: | ---: | ---: | ---: |
| `status/17/18446744073709551615` | 241.01 | 245.84 | 0.983x | 3/8 |
| `status/65025/1` | 2,066,990.37 | 2,027,960.50 | 1.000x | 4/8 |
| `status/65025/7` | 363,629.25 | 374,343.50 | 1.001x | 5/8 |
| `status-poll/0` | 12.46 | 11.01 | 1.129x | 8/8 |
| `status-poll/1` | 12.12 | 10.76 | 1.130x | 8/8 |
| `status-poll/2` | 12.00 | 10.75 | 1.130x | 8/8 |
| `status-poll/3` | 12.59 | 11.31 | 1.111x | 8/8 |
| `status-poll/4` | 87.87 | 95.86 | 0.976x | 1/8 |
| `status-poll/5` | 11.18 | 11.71 | 0.932x | 1/8 |


## Verification and rejected experiments

The new regression test covers an empty-lacing page and a page with a 255-byte
lace plus a zero-byte terminator at every byte prefix. Three empty polls must
consume zero, preserve an incomplete prefix, invalidate a returned page
correctly, and allow the rest of the page or a new page to decode. Invalid capture and CRC cases fail when the
27th byte arrives; later empty pushes and finish remain terminal until reset.
The same test passes against the saved baseline library, proving this is
existing behavior. Its commands, source/library/executable hashes and raw log
are pinned in the [checks](validation/2026-10-29-decoder-body-checks.json).

Release validation passes 59 Ogg tests and 113 Vorbis tests (87 ignored), Clippy
with warnings denied, Cargo/rustfmt checks and Python compilation. A clean
libogg reference revision and oracle executable hash are recorded. Six
[protocol comparisons](validation/2026-10-29-decoder-body-protocol.json) preserve
complete serialized traces, including errors, progress, finish/reset/reuse,
encoded pages, callbacks, Debug output, IO and source positions. Probe
layout/diagnostic output also matches.

| Protocol | Equal bytes |
| --- | ---: |
| Index and packet origins | 3,649,232 |
| Incremental decoder | 5,808,523 |
| Index lookup | 3,800,798 |
| Capture state | 78,020,534 |
| Packet writer | 46,897,193 |
| Public page encoding and IO | 26,967,237 |

Forcing `fill_page` inline was rejected: optimized assembly already inlines it
in the baseline, while the annotation enlarged the public push body and slowed
complete small pages. Splitting header/body loops offered only a small benefit
on selected one-byte cases and slowed other controls. A specialized single-byte
append saved little and harmed seven-byte chunks. The final paired and full
controls support retaining the combined status check: normal empty polls improve in all eight processes, and header-only
controls recover much of the preceding overhead. Failed polling and the ready
cycle remain slower controls, as reported above.

## Reproduce

Save the same committed benchmark controls for each production revision:

```powershell
python crates/deadlib-audio/decode/scripts/benchmark_ogg_kernels.py --root target/ogg-decoder-body-replay --capture before --source-revision 7f513dd3a
python crates/deadlib-audio/decode/scripts/benchmark_ogg_kernels.py --root target/ogg-decoder-body-replay --capture partial --source-revision 541c4b602
python crates/deadlib-audio/decode/scripts/benchmark_ogg_kernels.py --root target/ogg-decoder-body-replay --capture empty --source-revision 027274606
python crates/deadlib-audio/decode/scripts/benchmark_ogg_kernels.py --root target/ogg-decoder-body-replay --capture status --source-revision fa64cd4e1
python crates/deadlib-audio/decode/scripts/benchmark_ogg_kernels.py --root target/ogg-decoder-body-replay --stage before --stage partial --stage empty --stage status --runs 8 --output target/ogg-decoder-body-replay/results.tsv
```

Paired/protocol records retain exact rustc commands, source paths, linked-library
hashes and compiler details. The audit verifies all three production commits
against their measured snapshots, unchanged benchmark controls, every heap
counter and recomputed summaries.

The subsequent [hardware CRC pass](HARDWARE_CRC.md) accelerates large checksums
with runtime CPU checks and an unchanged scalar fallback, including native,
Miri, cross-target and exact protocol validation.
