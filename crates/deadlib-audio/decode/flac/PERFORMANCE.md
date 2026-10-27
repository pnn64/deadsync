# FLAC decoder performance work

The current goal is decode performance competitive with established decoders,
while preserving exact PCM, format coverage, resource limits and checksums.
The initial measurements below establish a baseline; **performance parity has
not been achieved**. Encoding performance remains a separate part of the earlier
implementation plan.

## Reproducible benchmark

`cargo bench -p deadlib-audio-decode-flac --bench decode -- MODE INPUT ITERATIONS`
runs the Rust benchmark CLI. The optional fourth argument writes a verified
frame-boundary index for the C reference benchmark. Inputs are resident before
timing, and no PCM is written to disk. Each invocation performs one warm-up pass.
Timed consumption touches frame endpoints and uses compiler memory barriers;
complete PCM comparisons remain separate correctness gates in PARITY.md.

Modes include reusable raw frame decoding, borrowed native decoding, native
streaming with/without MD5, first-frame startup, indexed seeking, and CRC alone.
Symphonia raw frames are measured both directly and with a separate frame CRC
pass: its raw decoder does not verify the frame footer. The native decoder,
libFLAC and the FFmpeg reference always check frame CRCs. Symphonia's direct mode
is still retained as a useful throughput target, with the policy difference
explicit rather than disabling native checksums.

The C++ reference CLI links pinned libFLAC 1.5.0 and FFmpeg 9.0.2, using their
public APIs. FFmpeg receives padded, prebuilt frame packets and runs with one
decoder thread and CRC checking. libFLAC uses a resident-input callback; its
raw-frame mode excludes metadata/reset time but includes the bit-reader's input
copies. Full-stream modes include libFLAC reset and metadata parsing; native
full-stream modes currently construct a new reader each iteration. This lifecycle
difference must be considered before comparing startup, allocations or full
pipeline results. No allocation or memory-parity conclusion is drawn yet.

The Linux/WSL reference build enables x86 assembly and runtime SIMD, including
AVX2, and LTO. It does not use the assembly-disabled correctness oracle. Rust
uses the workspace bench profile (optimized, LTO, debug line information) and
the portable x86-64 target; no `target-cpu=native` override is used.

Build prerequisites: GCC/G++, make, CMake, NASM and stable Rust. Prepare pinned
reference checkouts as described in COVERAGE.md and OGG.md, and the verified
FFmpeg archive from PARITY.md. Example Linux/WSL commands:

```sh
bash crates/deadlib-audio/decode/flac/benches/build.sh /tmp/flac-perf "$PWD" /path/to/ffmpeg-9.0.2.tar.xz
cargo bench -p deadlib-audio-decode-flac --bench decode --no-run --target-dir /tmp/flac-perf-rust
# Use the executable path printed by Cargo. Preserve a baseline copy before edits.
python3 crates/deadlib-audio/decode/flac/benches/run.py --native /path/to/decode-benchmark --oracle /tmp/flac-perf/oracle/flac_decode_benchmark --corpus target/deadlib-flac-reference/conformance --output /tmp/flac-baseline --quick --rounds 5 --milliseconds 80
```

Output directories must be new. The runner pins itself and children to virtual
CPU 2 by default, randomizes mode order in each trial (seed 9639), adapts the
iteration count from a pilot and records every raw timing, source/input/binary
hash, failure and summary. Use `--cpu` for another allowed core, omit `--quick`
for the broader 27-file selection, or use `--all` for every native official file.
`--baseline` plus `baseline-frames`/`baseline-crc` modes enables interleaved
before/after comparisons. Short pilot workloads and extreme metadata files must
not be substituted for the full performance evaluation.

## Initial seven-class baseline, 2026-10-27

Host: Intel Xeon E5-2696 v4, 22 cores/44 logical processors, Windows with Debian
WSL. Rust 1.99.1/LLVM 23.1.1, GCC 14.2.0, NASM 2.16.03. Five trials per cell,
at least 80 ms requested timed work (one complete pass minimum). The host is
shared, frequency is not fixed, and virtual CPU affinity does not isolate a
physical core. Raw trial ranges are retained; absolute timings can vary.

Median milliseconds to decode the complete file through raw frame APIs:

| Official subset case | Native | Symphonia, no footer CRC | libFLAC | FFmpeg |
| --- | ---: | ---: | ---: | ---: |
| 01: stereo, block 4096 | 12.487 | 6.637 | 5.236 | 8.031 |
| 03: block 16 | 15.732 | 7.644 | 15.294 | 20.129 |
| 15: verbatim | 5.227 | 2.049 | 4.616 | 2.352 |
| 17: fixed predictors | 9.452 | 4.004 | 4.093 | 4.291 |
| 28: high resolution | 43.724 | 18.896 | 16.180 | 22.013 |
| 31: high resolution, LPC-32 | 70.609 | 30.892 | 41.047 | 39.606 |
| 44: 8-channel 24-bit LPC-32 | 969.162 | 457.524 | 583.392 | 589.535 |

Reports: [raw timings](benches/validation/2026-10-27-baseline-raw.tsv),
[summaries](benches/validation/2026-10-27-baseline-summary.json),
[provenance](benches/validation/2026-10-27-baseline-manifest.json).
The preserved baseline executable is `/tmp/deadlib-flac-decode-baseline` in the
recorded WSL environment; reference libraries/builds and full artifacts remain
under `/tmp/deadlib-flac-perf-20261027`. Codec hashes were frozen before source
optimization. The runner subsequently gained paired-baseline support and earlier
source-provenance capture; the original run's script hash remains in its report.

`perf record` sampled 1,000 common-file raw decode passes without lost samples.
The largest hot region is predictor reconstruction, followed by Rice/bit reads
and CRC. A separate checksum microbenchmark accounts for roughly 13% of the
common-file raw decode time. Source-line samples are retained in the
[baseline profile](benches/validation/2026-10-27-baseline-profile.txt).

Next work includes measured bit/Rice and predictor improvements, output/buffer
reuse, streaming restart costs, MD5, Ogg pipelines, seeking, allocation/peak memory
measurement, broader input classes and representative audio. SIMD changes require
measured benefit, guarded dispatch, scalar fallback and the same exact parity
gates. A regression in any class must be investigated rather than hidden in an
aggregate speedup.
