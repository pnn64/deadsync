# Initial decoding baseline

This baseline measures the current implementation before optimization. It does
not establish competitive performance. Native full-file synthesis is materially
slower than both references on the two longer stereo inputs below.

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

## Remaining measurements and next optimization gate

Allocation counts/requested-byte peaks, process memory, indexed construction,
arbitrary seeking, Ogg-only container throughput and cold startup still need
comparative measurements. Existing zero-allocation tests cover specific warmed
native decode/reset/seek paths; sanitizer RSS is not a production memory result.

Before changing hot code, add component/profile evidence for bit/Huffman decode,
floor/residue synthesis and the current full-length complex FFT MDCT. The
throughput gap alone does not identify its cause. Optimize a measured bottleneck,
then rerun the same corpus/count/PCM gates and benchmark manifest. Broader files,
platforms and controlled machines remain necessary before performance claims.
