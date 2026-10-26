# Decoding performance

The initial baseline and subsequent transform optimization are recorded below.
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

## Remaining measurements and next optimization gate

Allocation counts/requested-byte peaks, process memory, indexed construction,
arbitrary seeking, Ogg-only container throughput and cold startup still need
comparative measurements. Existing zero-allocation tests cover specific warmed
native decode/reset/seek paths; sanitizer RSS is not a production memory result.

Add component/profile evidence for bit/Huffman decode and floor/residue
synthesis to explain the remaining pipeline gap. Optimize a measured bottleneck,
then rerun the same corpus/count/PCM gates and benchmark manifest. Broader files,
platforms and controlled machines remain necessary before performance claims.
