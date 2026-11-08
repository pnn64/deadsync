# Final platform evaluation

Codec sources match the preceding checkpoint milestone exactly. That milestone
contains the complete 12-unit/43-integration-and-external/3-doctest validation and
four ASan campaigns totaling 118,608 executions. This directory adds native
Windows throughput, Windows/Linux startup, exact seeks and a current heap check.
`final-validation.json` hashes every retained report and source snapshot.

Windows `raw` is the first nine-class check. `broad` is a later 27-class check
using the maintained libFLAC-only driver. `pipelines` includes new-reader
construction/destruction, native/Ogg container work and MD5 on every enabled
pass. The one unsupported broad comparison is Symphonia's already documented
32-bit stereo PCM defect; native and libFLAC still compare that file.

`seek` starts with metadata-only readers and includes lazy discovery. `seek-warm`
primes the identical 97-target request set in both implementations before timing.
Every trial makes 194 requests. Both policies verify equal trimmed PCM endpoint
checksums. Their initialization, index memory and cold-seek tradeoffs are
described in PERFORMANCE.md; primed results do not replace the lazy results.

All inputs are resident before timing. Each process inherits virtual CPU 2
affinity; mode order uses seed 9639, with five trials per cell. The shared
Broadwell host has no frequency lock. Keep Windows and WSL timing populations
separate. Symphonia startup omits footer CRC; its throughput comparisons include
CRC. libFLAC/native retain CRC, and enabled full-stream MD5 is audited after
warmup using corrupted expected digests.

MSVC 19.44 uses Release, static CRT and LTO for libFLAC and the C++ driver, with
default runtime SIMD/AVX2. The separate PCM-address escape helper excludes LTO.
libogg uses ordinary Release optimization: its older CMake policy ignores IPO.
The saved project settings and configure/build logs record the actual flags.
Linux uses GCC 14.2 Release/LTO. Rust 1.99.1 uses the workspace bench profile and
portable x86-64; there is no target-cpu=native override.

Reproduce with pinned reference checkouts from COVERAGE.md and OGG.md. From the
repository root, build the maintained libFLAC-only driver on Windows:

```powershell
cmake -S crates/deadlib-audio/decode/flac/benches/libflac -B target/flac-perf-windows -A x64 -DCMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded -DREFERENCE_ROOT=C:/GitHub/deadsync/target/deadlib-flac-reference/flac -DOGG_REFERENCE_ROOT=C:/GitHub/deadsync/target/deadlib-audio-reference/ogg
cmake --build target/flac-perf-windows --config Release --target flac_latency_benchmark --parallel 4
python crates/deadlib-audio/decode/flac/benches/build_native.py --output target/flac-perf-windows/decode.exe --target-dir target/deadlib-flac
python crates/deadlib-audio/decode/flac/benches/run.py --native target/flac-perf-windows/decode.exe --oracle target/flac-perf-windows/Release/flac_latency_benchmark.exe --corpus target/deadlib-flac-reference/conformance --output target/flac-raw-new --modes frames libflac-frames symphonia-crc
python crates/deadlib-audio/decode/flac/benches/run_seek.py --native target/flac-perf-windows/decode.exe --oracle target/flac-perf-windows/Release/flac_latency_benchmark.exe --corpus target/deadlib-flac-reference/conformance --output target/flac-seek-new --case subset/01 --case subset/03 --case subset/28 --case subset/44 --case uncommon/08
```

Repeat the seek command with `--warm-index` and a fresh output directory for the
primed comparison. Select `startup libflac-new-startup symphonia-new-startup` for
startup. Pipeline modes are listed in `windows-pipelines-manifest.json`.

On Linux, configure the same CMake source with `-DCMAKE_BUILD_TYPE=Release`, omit
`-A x64` and the MSVC CRT option, and use native absolute reference paths. Build
Rust with `build_native.py`, then run the same Python runners. Full FFmpeg
throughput reproduction uses the separate optimized build.sh workflow in
PERFORMANCE.md; the assembly-disabled correctness oracle is never a timing
reference.

`final-*`, `seek-initial-*`, `latency-initial-*` and `windows-original-*` source
files preserve the exact historical drivers. Private build/orchestration scripts
are audit snapshots with their original target-directory layout, rather than
the maintained entry points above. Initial and final native binaries have
separate hashes because benchmark modes changed; their production codec source
hashes agree. The final source snapshot precedes this documentation update.

The heap probe uses the counting allocator and shared driver retained in the
dense-storage milestone. Its counts cover two files, exclude stack/RSS/allocator
metadata, and make no throughput claim. Native raw decoding allocates nothing
after warmup; fresh readers return to their original live allocation count.
