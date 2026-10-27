# Decoding performance

The initial baseline and subsequent measured optimizations are recorded below.
Native full-file synthesis remains slower than both references on the two longer
stereo inputs. These shared-machine measurements do not establish competitive
performance across platforms.

## Reproduce

Use the pinned reference source checkouts described in
[the oracle README](tests/oracle/README.md), Python 3.11+, CMake and a C/C++
compiler. The script verifies reference commits and rejects modified tracked
reference sources, then builds both timing workers:

```powershell
python crates/deadlib-audio/decode/scripts/benchmark.py
# Or supply files explicitly:
python crates/deadlib-audio/decode/scripts/benchmark.py --repeats 5 --min-seconds 0.15 path/to/valid-single-stream.ogg
```

Each run creates a fresh ignored `target/deadlib-audio-reference/benchmark-<id>`
directory with per-trial TSV, summary TSV, source/input/binary hashes, compiler
information and build logs. Worker order is shuffled deterministically within
each trial. Calibration chooses enough whole iterations for the requested
minimum duration, up to 10,000 iterations. Every worker performs an untimed warm
decode before recording its repeated operations. Five trials report median,
minimum, maximum and median absolute deviation; process launch and file loading
are outside the worker timers. Inputs remain resident in memory.

The Rust worker is independently discoverable by package name:

```powershell
$file = (Resolve-Path path/to/file.ogg).Path
cargo bench -p deadlib-audio-decode-vorbis --bench pipeline -- native full 10 $file
cargo bench -p deadlib-audio-decode-vorbis --bench pipeline -- symphonia startup 100 $file
```

Pass an absolute input path: Cargo runs a benchmark in its package directory.

The C worker is source-built separately under `vorbis/benches/oracle`, linking
only pinned libogg/libvorbis/libvorbisfile. It emits the same count/timing protocol
and never emits PCM to stdout inside the timed work.

## What is timed

* `full`: construct a forward-only stream reader and codec, decode to EOF,
  consume borrowed planar float output, and destroy the stream/codec. Setup,
  container validation and final trimming are included. Input allocation and
  loading are excluded. No output PCM is retained.
* `stream4096`: the same work with each underlying source read capped at 4,096
  bytes. This measures fragmented resident-input throughput, not network latency
  or an asynchronous producer/consumer pipeline.
* `startup`: construct a fresh stream/codec and measure time to the first
  nonempty PCM block, before destruction. Lazy process-wide registries and caches
  have been warmed. Bytes/read calls at that point expose read-ahead differences.
  This is warm-process stream startup, not cold executable startup.

All sources are forward-only, including libvorbisfile callbacks and Symphonia's
`MediaSource`. This avoids hiding a duration scan or full-file index in one
decoder's startup. Each library retains its own normal internal buffering.
Rust's source read statistics use relaxed atomics; the C callback uses integer
counters. The results compare these complete APIs, not isolated codec kernels.
Native strict physical/mapping validation remains enabled. Complete decode
counts must match across all engines and both full/fragmented phases; a mismatch
fails the run. PCM numerical parity is established by separate conformance tests,
not inferred from timing or count agreement.

Current workload scope is one nonempty, valid logical stream per file. No
automatic malformed-file repair is enabled. The dependent eight-channel fixture
is retained for timing/count coverage, with its known Symphonia 0.6.1 coupling
discrepancy documented in [COVERAGE.md](COVERAGE.md). Its output is not presented
as PCM parity. The two headline songs below are stereo and pass numerical parity.

## Recorded Windows baseline, 2026-10-26

Intel Xeon E5-2696 v4 at reported 2.20 GHz, 22 cores/44 logical processors;
Windows x86-64; rustc 1.99.1. Rust uses the repository bench/release settings:
optimization level 3, LTO, no debug assertions/overflow checks, bench debug info.
C/C++ uses MSVC 19.44, Release `/O2 /Ob2 /DNDEBUG`, without LTO. References:
libogg 1.3.6, libvorbis/libvorbisfile 1.3.7, Symphonia 0.6.1. Different compilers
and library defaults are part of this baseline. The shared machine was not CPU
isolated or frequency-locked; spreads are retained and results are not a
cross-platform ranking.
The repository Windows Rust configuration selects `target-feature=+crt-static`;
the CMake build uses its normal MSVC runtime setting. No custom target CPU or
Rust flags were supplied through environment variables.

Five trials, shuffled engine order, requested minimum 150 ms per trial. Short
mono/stereo/eight-channel fixtures and two longer stereo tracks produced 225
recorded trial rows, with exact decoded counts throughout.

| File / phase | Native | Symphonia | libvorbisfile |
| --- | ---: | ---: | ---: |
| `in_two (loop).ogg`, full (ms) | 655.272 | 204.673 | 330.576 |
| `in_two (loop).ogg`, 4 KiB reads (ms) | 666.490 | 210.923 | 339.820 |
| `in_two (loop).ogg`, startup (ms) | 0.843 | 1.887 | 0.718 |
| `Goin' Under.ogg`, full (ms) | 655.501 | 188.797 | 244.265 |
| `Goin' Under.ogg`, 4 KiB reads (ms) | 635.324 | 190.694 | 251.648 |
| `Goin' Under.ogg`, startup (ms) | 1.170 | 2.065 | 0.679 |

Full-decode throughput in million **channel samples** per second:

| File | Native | Symphonia | libvorbisfile |
| --- | ---: | ---: | ---: |
| `in_two (loop).ogg` | 13.568 | 43.438 | 26.894 |
| `Goin' Under.ogg` | 16.608 | 57.662 | 44.568 |

Native full decode is 3.2–3.5 times slower than Symphonia and 2.0–2.7 times slower
than libvorbisfile on these two files. Native startup is faster than Symphonia
here, and slower than libvorbisfile. `in_two` full-decode min/median/max is
627.59/655.27/666.95 ms natively; Goin' Under is 623.86/655.50/681.14 ms. The
summary retains all reference spreads as well.

Native startup consumes 8,413 / 8,773 input bytes for these songs; Symphonia
consumes 15,360 / 15,360 and libvorbisfile 10,240 / 10,240. These are actual
resident source bytes read, not a simulated delivery latency.

[Summary](validation/2026-10-26-performance-summary.tsv),
[all trials](validation/2026-10-26-performance-trials.tsv), and
[manifest](validation/2026-10-26-performance.json) pin the evidence. The initial
manifest's build artifact profile comes from its retained Cargo build log;
unchanged profile/CMake settings were added after timing, as its note records.
The benchmark harness was uncommitted during timing; source hashes identify it.

## Measured N/4 transform optimization

The isolated transform worker measures `Imdct::inverse` and libvorbis's internal
`mdct_backward` on identical deterministic dense spectra. Construction and input
generation are excluded, output is consumed, and each context is reused. It uses
the same five shuffled trials and calibration cap of 10,000 iterations; small
sizes can reach that cap before the requested 150 ms. Its output-value throughput
is a kernel metric, not decoded audio throughput.

```powershell
python crates/deadlib-audio/decode/scripts/benchmark.py --imdct
cargo bench -p deadlib-audio-decode-vorbis --bench pipeline -- native imdct 10000 2048
```

The original length-N FFT measured about five times slower than libvorbis at
every legal size. Pairing the spectrum into a length-N/4 FFT follows the same
DCT-IV equation; [MDCT.md](MDCT.md) derives it. This removes the zero fill and
reduces owned transform table/scratch elements from **20N to 7N bytes (65%)**.
Decoder resource preflight includes the new sizes, with an exact-budget boundary
test. Allocation-free decode/reset/seek behavior is preserved.

Median microseconds per inverse transform:

| Block N | Native before | Native after | libvorbis before / after |
| --- | ---: | ---: | ---: |
| 64 | 0.796 | 0.252 | 0.163 / 0.161 |
| 128 | 1.809 | 0.532 | 0.374 / 0.354 |
| 256 | 3.909 | 1.127 | 0.774 / 0.716 |
| 512 | 7.950 | 2.289 | 1.626 / 1.515 |
| 1024 | 17.857 | 5.146 | 3.636 / 3.595 |
| 2048 | 36.142 | 10.832 | 7.270 / 7.705 |
| 4096 | 84.346 | 22.235 | 16.294 / 16.885 |
| 8192 | 178.346 | 47.035 | 34.100 / 35.775 |

The native kernel is **3.2–3.8x faster** in these measurements, while still
slower than libvorbis's transform. Before/after source, binary and trial hashes
are retained with the [before summary](validation/2026-10-26-mdct-before-summary.tsv),
[before trials](validation/2026-10-26-mdct-before-trials.tsv),
[before manifest](validation/2026-10-26-mdct-before.json),
[after summary](validation/2026-10-26-mdct-after-summary.tsv),
[after trials](validation/2026-10-26-mdct-after-trials.tsv), and
[after manifest](validation/2026-10-26-mdct-after.json).

The same five-file pipeline suite was rerun after correctness/sanitizer work
completed. Median milliseconds on the two longer stereo tracks:

| File / phase | Native before | Native after | Symphonia after | libvorbisfile after |
| --- | ---: | ---: | ---: | ---: |
| `in_two`, full | 655.272 | 483.843 | 226.859 | 371.883 |
| `in_two`, 4 KiB reads | 666.490 | 486.141 | 222.901 | 368.172 |
| `in_two`, startup | 0.843 | 0.885 | 2.187 | 0.828 |
| `Goin' Under`, full | 655.501 | 393.188 | 207.770 | 269.392 |
| `Goin' Under`, 4 KiB reads | 635.324 | 409.645 | 210.553 | 274.659 |
| `Goin' Under`, startup | 1.170 | 1.254 | 2.269 | 0.756 |

Observed full-file speedup is **1.35x / 1.67x**. Reference full-file times also
rose about 10–12% between runs, illustrating machine/frequency variability;
these ratios are observations, not controlled causal estimates. Startup did
not improve in the absolute measured values. The native decoder remains about
1.9–2.1x slower than Symphonia and 1.3–1.5x slower than libvorbisfile on these
post-change full-file runs. See the
[pipeline summary](validation/2026-10-26-quarter-mdct-pipeline-summary.tsv),
[trials](validation/2026-10-26-quarter-mdct-pipeline-trials.tsv), and
[manifest](validation/2026-10-26-quarter-mdct-pipeline.json).

Validation kept every numerical gate unchanged: direct cosine/all legal sizes,
libvorbis MDCT (observed peak <=3.874301910e-7), 38-file raw/full-container PCM,
1,900 corpus seeks, 528 upstream coded vectors, historical/ITGmania containers,
release/Clippy checks, zero-allocation tests and extracted offline builds.
Resumed raw-packet and streaming/indexed AddressSanitizer targets completed
another 300 seconds each, **16,816 executions**, without artifacts or failures;
[statistics](validation/2026-10-26-quarter-mdct-fuzz.tsv),
[provenance](validation/2026-10-26-quarter-mdct-fuzz-provenance.json),
[binary hashes/results](validation/2026-10-26-quarter-mdct-fuzz-results.json), and
[initial corpus hashes](validation/2026-10-26-quarter-mdct-fuzz-initial.sha256.json)
preserve this campaign. Corpus hashes from the original reports are unchanged.

## Bounded Huffman lookup and interleaved baselines

The next measured change appends an adaptive two-to-eight-bit prefix lookup to
each eligible book's existing tree allocation. [HUFFMAN.md](HUFFMAN.md) describes
the packing, exact tail behavior and optional memory policy. Table elements cost
at most 1 KiB per book, use only the budget left after the complete setup grammar,
and fall back to the tree if memory is unavailable. Decode/reset remain free of
allocations. No floating-point synthesis code or PCM tolerance changed.

The isolated workload uses libvorbis-encoded codewords from the five real setup
headers, one occurrence of every active entry in entry order. It excludes
construction and does **not** model actual encoder symbol frequencies. Both
workers check every decoded entry and exact bit consumption before timing.

To reduce drift between separate before/after runs, the runner can extract native
production sources from a commit into an ignored workspace and build them with
the current benchmark adapters. Seven shuffled trials interleave current native,
native baseline `6c5c948ed`, and the references. Cargo artifact profiles and all
17 registry dependency artifact IDs match between the Rust builds. Source,
archive, input and binary hashes are retained. No branch or original source file
is changed by baseline construction.

```powershell
python crates/deadlib-audio/decode/scripts/benchmark.py --huffman --baseline-revision 6c5c948ed --repeats 7
python crates/deadlib-audio/decode/scripts/benchmark.py --baseline-revision 6c5c948ed --repeats 7
```

Paired scalar throughput, million **codewords** per second:

| Setup source | Native tree | Native lookup | libvorbis |
| --- | ---: | ---: | ---: |
| Mono fixture | 29.969 | 45.368 | 23.878 |
| Stereo fixture | 28.712 | 50.790 | 27.909 |
| Eight-channel fixture | 27.039 | 43.346 | 24.894 |
| `in_two (loop).ogg` | 32.043 | 55.269 | 28.252 |
| `Goin' Under.ogg` | 33.079 | 54.905 | 28.852 |

This is an observed **1.51–1.77x** scalar improvement. The original tree already
outperformed libvorbis on this uniform-entry workload, so it did not demonstrate
that Huffman decoding was the pipeline's dominant bottleneck. The paired
whole-file trials assess the practical effect:

| File / phase | Native tree (ms) | Native lookup (ms) | Symphonia (ms) | libvorbisfile (ms) |
| --- | ---: | ---: | ---: | ---: |
| `in_two`, full | 462.118 | 387.280 | 218.046 | 365.306 |
| `in_two`, 4 KiB reads | 451.332 | 378.270 | 215.398 | 337.640 |
| `in_two`, startup | 0.810 | 0.858 | 1.933 | 0.745 |
| `Goin' Under`, full | 362.770 | 336.442 | 191.966 | 246.633 |
| `Goin' Under`, 4 KiB reads | 378.519 | 343.167 | 187.984 | 268.415 |
| `Goin' Under`, startup | 1.251 | 1.326 | 2.366 | 0.777 |

Median full-file throughput improves **1.19x / 1.08x**, and 4 KiB reads improve
1.19x / 1.10x. First-PCM time increases about **6%** for both longer tracks.
The three short fixtures' full decode times increase about **2–7%** because
construction dominates their few output blocks. This is a throughput/startup
tradeoff. Native `in_two` full-decode min/median/max is
378.77/387.28/419.59 ms with lookup versus 439.30/462.12/486.94 ms for the tree.
Goin' Under is 326.56/336.44/380.47 versus 343.25/362.77/400.38 ms, with overlapping
trial ranges. Results remain from a shared, unlocked machine.

Native full decoding still trails Symphonia by about 1.8x, and libvorbisfile by
about 1.06–1.36x, on these two tracks. Faster scalar decoding is not an overall
competitive-performance claim. The new benchmark mode's manifest unit is
`codewords`; historical TSV count/rate column names retain `samples` spelling.

The [paired scalar summary](validation/2026-10-26-huffman-scalar-summary.tsv),
[trials](validation/2026-10-26-huffman-scalar-trials.tsv), and
[manifest](validation/2026-10-26-huffman-scalar.json) include reference-generated
codeword corpus hashes. The [paired pipeline summary](validation/2026-10-26-huffman-pipeline-summary.tsv),
[trials](validation/2026-10-26-huffman-pipeline-trials.tsv), and
[manifest](validation/2026-10-26-huffman-pipeline.json) retain all 420 whole-file
trial rows. For that initial paired run, registry artifact equality was checked
from the retained build logs after timing; subsequent runs enforce it beforehand.

Validation: complete 1,611-book/155,478-entry reference codeword/VQ corpus, 38-file
raw/full PCM, 1,900 corpus seeks, 528 upstream coded vectors, four historical/song
files, all-prefix/alignment/tail/32-bit-tree tests, resource fallback tests,
release/Clippy, zero-allocation and extracted offline checks pass. Resumed
header/raw-packet/streaming sanitizer campaigns add **141,962 executions** with
no artifacts/failures: [statistics](validation/2026-10-26-huffman-fuzz.tsv),
[provenance](validation/2026-10-26-huffman-fuzz-provenance.json),
[binary hashes/results](validation/2026-10-26-huffman-fuzz-results.json), and
[initial corpus hashes](validation/2026-10-26-huffman-fuzz-initial.sha256.json).

## Unit-twiddle FFT butterflies, 2026-10-27

The FFT's width-two stage is now a contiguous sum/difference pass. Every later
group also handles its first pair without complex multiplication by the unit
twiddle. This removes L-1 complex multiplications for FFT length L=N/4;
[MDCT.md](MDCT.md#unit-fft-twiddles) describes the arithmetic and signed-zero
qualification. Transform storage stays at 7N bytes. Both synthesis and native
analysis use this helper; no public API, dependency or allocation changes.

Seven shuffled trials interleave native baseline `046b21236`, the candidate and
libvorbis for the isolated kernel; the pipeline also includes Symphonia. The
current adapter is compiled against both native source snapshots with identical
Cargo artifact profiles and all 17 registry dependency artifact IDs. No custom
CPU flags are added. These remain shared, unlocked Windows desktop measurements
on the same Xeon system. The 10,000-iteration cap still limits short kernel trials.

```powershell
python crates/deadlib-audio/decode/scripts/benchmark.py --imdct --baseline-revision 046b21236 --repeats 7
python crates/deadlib-audio/decode/scripts/benchmark.py --baseline-revision 046b21236 --repeats 7
```

Median microseconds per inverse transform:

| Block N | Native before | Native after | Speedup | libvorbis |
| --- | ---: | ---: | ---: | ---: |
| 64 | 0.245 | 0.207 | 1.19x | 0.155 |
| 128 | 0.483 | 0.423 | 1.14x | 0.339 |
| 256 | 1.067 | 0.879 | 1.21x | 0.726 |
| 512 | 2.484 | 2.050 | 1.21x | 1.736 |
| 1024 | 5.233 | 4.345 | 1.20x | 3.679 |
| 2048 | 11.378 | 8.931 | 1.27x | 7.879 |
| 4096 | 21.280 | 18.629 | 1.14x | 15.459 |
| 8192 | 46.880 | 38.633 | 1.21x | 34.741 |

The isolated gain is 1.14-1.27x across every legal size. Native still trails
libvorbis's kernel. The same five-file pipeline suite gives these song medians
in milliseconds:

| File / phase | Native before | Native after | Symphonia | libvorbisfile |
| --- | ---: | ---: | ---: | ---: |
| `in_two`, full | 331.405 | 321.578 | 186.777 | 315.343 |
| `in_two`, 4 KiB reads | 336.578 | 311.613 | 191.022 | 302.333 |
| `in_two`, startup | 0.777 | 0.771 | 1.813 | 0.661 |
| `Goin' Under`, full | 324.450 | 291.895 | 181.931 | 237.974 |
| `Goin' Under`, 4 KiB reads | 336.497 | 308.726 | 189.647 | 252.359 |
| `Goin' Under`, startup | 1.095 | 1.096 | 2.001 | 0.615 |

Observed full-file speedup is **1.03x / 1.11x**, and fragmented reads improve
**1.08x / 1.09x**. Startup is essentially unchanged. The shorter mono/stereo
fixtures have mixed small movements: full-decode throughput improves 2.6-3.7%,
fragmented decode takes 2.8-6.2% longer and startup 2.2-2.5% longer. The
eight-channel fixture's full/fragmented throughput improves 8.8% / 10.3%.
These small differences
should be read with the retained spreads. Maximum relative MAD is 7.80% for
kernel groups and 7.42% for pipeline groups. Song full-file trial ranges overlap:
`in_two` is 301.93-331.37 ms after versus 321.16-393.24 ms before; Goin' Under is
273.86-342.02 versus 294.99-348.54 ms. No universal speedup is claimed.

Native song full decoding still trails Symphonia by 1.72x / 1.60x and
libvorbisfile by 1.02x / 1.23x. This small FFT optimization does not explain or
close the complete pipeline gap. Floor/residue profiling remains necessary.

The [kernel summary](validation/2026-10-27-unit-twiddle-kernel-summary.tsv),
[168 trials](validation/2026-10-27-unit-twiddle-kernel-trials.tsv), and
[manifest](validation/2026-10-27-unit-twiddle-kernel.json), plus the
[pipeline summary](validation/2026-10-27-unit-twiddle-pipeline-summary.tsv),
[420 trials](validation/2026-10-27-unit-twiddle-pipeline-trials.tsv), and
[manifest](validation/2026-10-27-unit-twiddle-pipeline.json), retain the measurements.
The [validation record](validation/2026-10-27-unit-twiddle-checks.json) pins source,
input and worker hashes, full conformance checks and sanitizer results.

The real-PCM encoder check also reproduces all 24 prior pair-encoder file hashes
exactly, across six inputs and all four precision settings. The current native
decoder and Symphonia pass the existing PCM gates against the retained,
hash-verified libvorbis PCM for those identical files: 3,658,112 channel samples
per decoder. [Encoding checks](validation/2026-10-27-unit-twiddle-encoding.json)
record the individual errors and artifact hashes. This does not claim all
possible forward-transform inputs produce identical encoded bytes.

Validation passes **108 release tests** and **85 extracted offline tests**,
including all-size direct cosine and libvorbis forward/inverse checks, the
38-file raw/full PCM corpus, 1,900 repository-corpus seeks, 528 upstream vectors,
historical/floor-zero media, the 144-case native encoding matrix and warmed
allocation checks. No numerical tolerance is increased. Default/all-feature
Clippy and formatting pass. Resumed raw-packet, streaming/indexed and encoding
AddressSanitizer targets run 300 seconds each, completing **18,658 executions**
with no failures or artifacts. The
[fuzz results](validation/2026-10-27-unit-twiddle-fuzz-results.tsv) and
[provenance](validation/2026-10-27-unit-twiddle-fuzz-provenance.json) retain this
campaign; sanitizer memory is not a production memory measurement.

## Remaining measurements and next optimization gate

[RESOURCES.md](RESOURCES.md) adds separate requested-heap counters, uninstrumented
process memory, indexed construction and validated 100-request seek batches for
five files. It retains the initial page-API baseline, the packet-read update,
and the compact mode-index update separately. The latest paired run against
`af518655b` improves longer-file seek batches 1.99x/3.18x, using two mode bits per
packet to choose the nearest overlap pre-roll. Native beats both references on
those two workloads in this run, while the short fixtures still trail them.
The additional live requested heap is 4,224 bytes per longer track; all five
warmed native batches allocate nothing. Indexed-open time remains largely
unchanged and still requires two complete source passes. These scoped results
do not establish overall or cross-platform competitive performance.
Ogg-only container throughput and truly cold startup still need comparative
measurements. Sanitizer RSS is not a production memory result.

Add actual symbol-frequency/profile evidence and floor/residue component
measurements to explain the remaining pipeline gap. Optimize a measured bottleneck,
then rerun the same corpus/count/PCM gates and benchmark manifest. Broader files,
platforms and controlled machines remain necessary before performance claims.
