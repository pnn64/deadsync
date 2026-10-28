# Empty continuation and packet-writer work, 2026-10-28

This pass starts at `64901881c`, following the
[packet-metadata measurements](SUMMARY_REUSE.md). It retains two safe Rust
optimizations and rejects a third after the full controls expose a regression.
No public API, stored layout, dependency or allocation policy changes.

Demux preflight skips packet-size arithmetic and fragment traversal for an
empty page body. Previously validated cached bytes cannot grow on such a page.
A zero lace completes the cached packet; a nil page leaves it pending. The EOS
check, assembly loop, callbacks and limit-error ordering remain in place.

`PacketWriter` builds bounded lacing and matching payloads itself. Its private
page-emission path now reuses the existing encoding/CRC/IO implementation
directly, avoiding a second lacing sum and framing check.
Public `write_page` still validates arbitrary input before any destination IO.
The header/body `write_all` calls and partial/Interrupted IO handling are unchanged.

The combined writer error predicate was reverted in `7a0c73e9e`. It improved
one sustained small-packet case but made a long-fragment control slower. The
original three ordered writer checks remain; their new regression test is kept.
Retained production source at `7a0c73e9e` matches the measured `writer` snapshot
at `db9d84cf1`, with both new regression tests.

## Measurement boundaries

The worker retains the previous 194 workloads and adds three prevalidated span
capture cases, nil demux/capture and eight CRC-tail controls: 207 in total.
The nil fixture contains a BOS packet followed by 64 cached 255-byte packets,
each with 32 nil continuation pages and a zero-lace terminator. Span fixtures
use 1, 4 or 64 continuation pages per packet. Construction and parsing precede
timing; demux/capture construction, callbacks, finish and cleanup are timed.

Eight trials run four saved workers in rotating order, making 6,624 CPU/heap
observations. Every observation uses the median of eleven batches after warmup.
Windows QueryThreadCycleTime measures the benchmark thread pinned to logical
CPU 2. Heap counters use a separate untimed operation. The shared Windows x64
Xeon E5-2696 v4 host is not isolated or frequency locked. The builds use rustc
1.99.1 / LLVM 23.1.1, opt-level 3, LTO, benchmark debug symbols and static CRT.
The [record](validation/2026-10-28-framing.json) retains compiler/profile, source
and executable hashes, cycle/nanosecond medians, ranges, median absolute
deviations and heap counters. The [raw rows](validation/2026-10-28-framing.tsv)
retain every observation.

The [paired worker](validation/2026-10-28-framing-paired.rs) links the four
versions plus historical `a59bb2f10` in one process with matching optimized LTO
flags. It alternates before/after across twelve batches and uses black-boxed,
typed function-pointer callbacks for demux. Eight processes produce
[336 paired observations](validation/2026-10-28-framing-paired.json). The
`guards` stage is a rejected candidate, and `writer` is the retained final stage. Historical-final paired labels refer to the rejected guards build. Paired
throughput is the median of each process's cycle ratio; cycle columns are
separate medians. Writer factory operations construct a sink/writer, emit a BOS
header, then eight payload packets ending in EOS. Sustained writes warm and
retain the two writers, flush their warmup pages, and time 131,072 additional
non-EOS packets per batch with reused capacity.

## CPU and heap results

The changes target different small-packet paths; improvements are workload-specific.

| Paired case | Before cycles | After cycles | Paired CPU throughput | Faster processes |
| --- | ---: | ---: | ---: | ---: |
| `empty/nil` | 120,258.56 | 111,890.06 | 1.077x | 8/8 |
| `empty/1` | 11,962.75 | 11,441.38 | 1.040x | 8/8 |
| `empty/4` | 28,864.25 | 28,411.56 | 1.018x | 8/8 |
| `empty/64` | 384,262.12 | 381,161.75 | 1.008x | 7/8 |
| `empty/completed` | 182,239.87 | 183,405.94 | 0.992x | 0/8 |
| `writer/0/0` | 1,115.72 | 1,096.84 | 1.026x | 6/8 |
| `writer/0/1` | 1,154.13 | 1,131.52 | 1.035x | 6/8 |
| `writer/0/17` | 1,296.81 | 1,280.12 | 1.027x | 5/8 |
| `writer/8192/0` | 484.66 | 462.93 | 1.043x | 7/8 |
| `writer/8192/17` | 1,526.40 | 1,519.60 | 1.005x | 6/8 |
| `writer/8192/130050` | 1,297,080.50 | 1,299,440.00 | 0.998x | 4/8 |
| `guards/0/0` | 1,084.55 | 1,087.62 | 0.992x | 1/8 |
| `guards/8192/17` | 1,513.67 | 1,500.72 | 1.009x | 8/8 |
| `guard-steady/0` | 30.50 | 30.80 | 0.991x | 2/8 |
| `guard-steady/1` | 36.64 | 35.34 | 1.037x | 7/8 |
| `guard-steady/17` | 54.32 | 53.97 | 1.006x | 6/8 |

The nil-page preflight gains **7.7%** paired CPU throughput (8/8 processes).
Skipping duplicate writer validation gains **2.6% to 4.3%** in selected small
factory cases, while large payload controls are nearly unchanged. The combined
error predicates gained **3.7%** on sustained one-byte packets (7/8) but only
**0.6%** on 17-byte packets and lost **0.9%** on empty packets. The full worker's
long-fragment case rose from 73.35 to 88.89 cycles after that change, **21.2%**
more CPU. That tradeoff did not justify retaining the guard change.

| Full-worker workload | Baseline cycles | Empty body | Trusted writer | Rejected guards | Retained CPU throughput |
| --- | ---: | ---: | ---: | ---: | ---: |
| `nil-demux` | 126,130.18 | 119,833.31 | 113,001.38 | 117,363.94 | 1.116x |
| `nil-capture` | 118,763.18 | 121,945.88 | 113,920.50 | 118,283.12 | 1.043x |
| `span-demux/1` | 13,176.86 | 12,398.31 | 11,835.86 | 12,117.10 | 1.113x |
| `span-demux/4` | 33,007.30 | 33,261.11 | 31,474.26 | 33,134.21 | 1.049x |
| `span-demux/64` | 417,341.55 | 424,437.56 | 417,893.71 | 418,854.02 | 0.999x |
| `writer/0` | 502.00 | 470.17 | 480.49 | 451.95 | 1.045x |
| `writer/17` | 1,512.13 | 1,464.94 | 1,490.41 | 1,493.69 | 1.015x |
| `writer/8192` | 76,180.24 | 74,780.90 | 76,873.01 | 76,087.61 | 0.991x |
| `chained-demux/64` | 151,208.06 | 156,023.38 | 152,771.93 | 157,120.94 | 0.990x |
| `dense-index/1` | 49,510.94 | 49,716.72 | 50,066.60 | 50,464.40 | 0.989x |

All six heap counters are identical across all four stages for every workload: allocations,
reallocations, frees, cumulative allocated/freed bytes and peak live bytes.
For nil demux that is two allocations, zero reallocations and 511 allocated/peak
bytes per operation. `writer/0` remains allocation-free. Production code adds
12 lines overall, removing repeat validation and preflight scans. No unsafe code is added.

The largest slower controls for the retained `writer` stage are below. They
remain measured costs of these builds even when their source is unchanged.
Shared-host variability and compiler/layout effects are not separated here.
The paired comparisons establish narrower gains, not a general Ogg speedup.
The 4 KiB CRC control is 13.4% slower; this remains a regression to investigate.

| Slower retained control | Baseline cycles | Retained cycles | CPU cost increase |
| --- | ---: | ---: | ---: |
| `fragment-prefix/255/true` | 70.66 | 88.99 | 25.9% |
| `crc-tail/12` | 22.70 | 25.92 | 14.2% |
| `crc/4096` | 3,670.59 | 4,164.02 | 13.4% |
| `crc-tail/14` | 30.92 | 33.75 | 9.2% |
| `sync/65025/7` | 213,244.50 | 229,377.50 | 7.6% |
| `crc/65025` | 62,557.50 | 66,947.00 | 7.0% |
| `crc-tail/13` | 25.93 | 27.70 | 6.8% |
| `crc-tail/6` | 21.05 | 22.40 | 6.4% |
| `crc-tail/5` | 17.37 | 18.46 | 6.3% |
| `crc-tail/15` | 34.72 | 36.89 | 6.3% |
| `span-query/4/offset` | 56.28 | 59.44 | 5.6% |
| `fragment-prefix/17/false` | 21.05 | 22.21 | 5.5% |

The historical paired controls compare `a59bb2f10` with this pass baseline
and rejected guard source using matching callback types and fixtures:

| History count | Old -> baseline cycles | Paired throughput | Old -> rejected guards cycles | Paired throughput |
| ---: | ---: | ---: | ---: | ---: |
| 0 | 166,312.37 -> 172,019.38 | 0.962x | 166,175.25 -> 158,863.12 | 1.037x |
| 64 | 181,567.68 -> 181,814.62 | 0.979x | 179,386.38 -> 171,072.82 | 1.051x |
| 512 | 509,061.57 -> 518,486.50 | 0.982x | 508,019.07 -> 497,949.50 | 1.023x |


## Correctness and rejected candidates

The [checks](validation/2026-10-28-framing-checks.json) pin final source and test
hashes, commands, raw logs, a clean libogg reference revision and executable
hash. Final validation passes 58 Ogg tests and 113 Vorbis tests (87 ignored),
Clippy with warnings denied, Cargo/rustfmt checks and Python compilation.
The two new tests also run against the saved baseline library: they check
nil and zero-lace pages at exact packet/pending limits, EOS failures before
callbacks, packet origins and granules, failed/ended error priority, destination
IO count and recovery after an invalid granule.

The [protocol comparisons](validation/2026-10-28-framing-protocol.json) compare
complete serialized baseline/final traces, including public Debug output,
errors, callbacks, state, source positions, bytes and partial/failing IO:

| Protocol | Equal bytes |
| --- | ---: |
| Index and packet origins | 3,649,232 |
| Incremental decoder | 5,808,523 |
| Index lookup | 3,800,798 |
| Capture state | 78,020,534 |
| Packet writer | 46,897,193 |
| Public page encoding and IO | 26,967,237 |

Generic result adapters and forced inlining aimed at the earlier public-demux
regression were rejected because they slowed indexing controls. Capture fast
paths did not improve the controls consistently. Four-byte CRC-tail kernels,
outlined tails and scalar special cases improved selected short CRC inputs but
regressed bulk or single-byte controls; CRC source remains unchanged.

The historical paired controls investigate the earlier reported 24% chained
public-demux cost increase. They do not reproduce a 24% intrinsic cost of the
private packet summary: source versions compared in a shared process give much
smaller differences, and the candidate builds also include unrelated writer
changes that can alter library code layout. These measurements establish sensitivity to benchmark/codegen
context; they do not establish a general public-demux speedup or a definitive
cause for the earlier regression.

## Reproduce

Use the committed benchmark/probe sources and flags pinned in the JSON records.
Save production snapshots at the recorded revisions with identical benchmark
controls:

```powershell
python crates/deadlib-audio/decode/scripts/benchmark_ogg_kernels.py --root target/ogg-framing-replay --capture before --source-revision 64901881c
python crates/deadlib-audio/decode/scripts/benchmark_ogg_kernels.py --root target/ogg-framing-replay --capture empty --source-revision c2775a16d
python crates/deadlib-audio/decode/scripts/benchmark_ogg_kernels.py --root target/ogg-framing-replay --capture writer --source-revision db9d84cf1
python crates/deadlib-audio/decode/scripts/benchmark_ogg_kernels.py --root target/ogg-framing-replay --capture guards --source-revision 0a950efa9
python crates/deadlib-audio/decode/scripts/benchmark_ogg_kernels.py --root target/ogg-framing-replay --stage before --stage empty --stage writer --stage guards --runs 8 --output target/ogg-framing-replay/results.tsv
```

The paired and protocol records retain exact rustc commands, source paths and
linked library hashes. Correctness logs preserve untrimmed tool output. Shared
host timings are observations of these fixtures and builds, not portable
performance guarantees.
