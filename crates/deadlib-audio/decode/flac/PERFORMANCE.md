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
copies. The original full-stream modes include libFLAC reset and metadata parsing;
native full-stream modes construct a new reader each iteration. The added
`libflac-new*` and `symphonia-new*` modes include allocation, initialization and
destruction on every pass, matching that lifecycle. Metadata APIs still differ:
native readers retain metadata while libFLAC's callback ignores it. No allocation
or memory-parity conclusion is drawn yet.

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

## First optimization: bit-cache and CRC, 2026-10-27

The retained change loads up to eight available bytes into the bit cache at a
time, without padding or unsafe reads. Exact tails retain the checked byte path.
CRC-16 uses eight slicing tables (4 KiB) to remove the serial per-byte dependency.
Format validation, frame checksums, sample precision and public APIs are unchanged.
An independent bit-polynomial test covers CRC chunk/tail boundaries and offsets.

Interleaved five-trial comparisons against the preserved baseline show these
ratios of median raw-frame times (larger is faster):

| Case | Decode speedup | CRC speedup |
| --- | ---: | ---: |
| 01: block 4096 | 1.134x | 4.50x |
| 03: block 16 | 1.188x | 4.59x |
| 15: verbatim | 2.231x | 4.64x |
| 17: fixed | 1.213x | 4.57x |
| 28: high resolution | 1.306x | 4.59x |
| 31: LPC-32 | 1.177x | 4.61x |
| 44: 8-channel LPC-32, nine-trial repeat | 1.134x | 4.44x in initial run |

The first case-44 run was noisy while correctness jobs ran, with paired ratios
from 0.789 to 2.281. A separate nine-trial run after those jobs finished produced
a ratio of medians of 1.134x and a median paired speedup of 1.143x. Eight pairs
improved; one remained a large scheduling outlier (0.518x). Both complete runs
are retained. This shared-host result supports improvement but is not a claim of
isolated-machine timing precision.

An additional forward predictor-traversal experiment was rejected: despite the
same CRC/cache changes, LPC-32 case 31 slowed to 0.884x. Restoring the original
predictor loop produced the 1.177x result above. Its source/binary hashes and
[raw rejected timings](benches/validation/2026-10-27-rejected-predictor-raw.tsv)
are retained so that gain is not confused with a predictor improvement.

Reports: [paired timings](benches/validation/2026-10-27-primitives-raw.tsv),
[summary](benches/validation/2026-10-27-primitives-summary.json),
[source/binary provenance](benches/validation/2026-10-27-primitives-manifest.json),
[LPC-32 repeat](benches/validation/2026-10-27-primitives-lpc32-repeat-raw.tsv).

All 41 integration/external gates plus three doctests pass in release mode,
including the official native/Ogg corpus, 1,279 explicit wire cases, 2,162
accepted fuzz frames and the generated bidirectional libFLAC matrices. Clippy
passes. Four fresh 60-second ASan campaigns add 54,689 executions without
failures; the compiler audit preserves native overflow checks, sanitizer and
feedback instrumentation. See the [validation record](benches/validation/2026-10-27-primitives-validation.json),
[fuzz results](benches/validation/2026-10-27-primitives-fuzz-results.json) and
[compiler audit](benches/validation/2026-10-27-primitives-compiler-audit.json).

The decoder remains about 1.7–2.1x slower than the fastest reference in several
compute-heavy classes. Rice/predictor kernels, output buffers, streaming, MD5 and
the broader benchmark remain active work; this milestone does not meet the full
performance goal.

## Scalar kernels and output, 2026-10-27

Rice unary reads now inline into residual decoding. Fixed predictors keep their
one to four preceding samples in rolling registers. Byte-aligned 8/16/24/32-bit
verbatim planes use checked bulk reads. Stereo decorrelation writes final PCM
directly, avoiding an extra scratch traversal. Independent-channel conversion
uses the range checks already performed on every reconstructed sample. New
negative tests cover fixed-predictor and stereo reconstruction overflow at every
declared depth. No validation or checksum is disabled.

Seven interleaved trials against the preserved first-optimization binary give
these median raw-frame times. All times are milliseconds per complete file;
Symphonia's direct mode still omits footer CRC as described above.

| Case | Previous | Current | Speedup | libFLAC | FFmpeg | Symphonia |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 01: block 4096 | 11.552 | 9.914 | 1.165x | 5.431 | 8.304 | 6.813 |
| 03: block 16 | 13.366 | 9.855 | 1.356x | 15.340 | 20.697 | 7.774 |
| 15: verbatim | 2.297 | 0.961 | 2.390x | 4.632 | 2.327 | 1.918 |
| 17: fixed | 7.731 | 4.976 | 1.554x | 4.141 | 4.413 | 4.030 |
| 28: high resolution | 35.530 | 30.596 | 1.161x | 17.074 | 22.926 | 19.256 |
| 31: LPC-32 | 58.971 | 58.228 | 1.013x | 39.843 | 39.632 | 31.258 |
| 44: 8-channel LPC-32 | 855.842 | 832.387 | 1.028x | 573.470 | 582.127 | 453.605 |

An initial combined version put the verbatim loops inside the predictor's hot
function and regressed LPC-32 despite unchanged arithmetic. Keeping verbatim in
a separate function restored its performance. The retained version improves
verbatim and fixed inputs substantially; the small LPC-32 differences above are
not evidence that its predictor kernel improved. LPC remains the main gap.

Reports: [raw trials](benches/validation/2026-10-27-scalar-raw.tsv),
[summary](benches/validation/2026-10-27-scalar-summary.json),
[provenance](benches/validation/2026-10-27-scalar-manifest.json).
All 43 integration/external gates and three doctests pass, as does Clippy.
Four fresh ASan campaigns complete 56,647 executions without a failure; see the
[validation record](benches/validation/2026-10-27-scalar-validation.json),
[fuzz results](benches/validation/2026-10-27-scalar-fuzz-results.json) and
[compiler audit](benches/validation/2026-10-27-scalar-compiler-audit.json).
Full performance parity remains unachieved.

## Rice and dispatched LPC kernels, 2026-10-27

Rice decoding consumes a cached unary prefix and remainder together, retaining
checked reads for long quotients and exact tails. On CPUs with both BMI2 and
LZCNT, the same safe loop is compiled with those instruction sets enabled.
Runtime detection selects it; other CPUs retain the portable implementation.
Short scalar LPC kernels specialize the order. AVX2 prepares the historical
terms for four outputs together, then adds newly reconstructed terms and applies
the original shift separately for each output. This preserves LPC rounding.
Every reconstructed sample remains range checked before reuse.

SIMD is limited to effective depths up to 32 bits, LPC orders 4–32 and blocks
of at least 64 samples. The 33-bit stereo side signal uses full-width scalar
arithmetic. Unsafe code is denied except in the two private CPU-dispatch modules;
the AVX2 loads/stores have local bounds and width preconditions. Input bit reads
remain safe Rust without padding requirements. The signed range test uses an
unsigned bias and one comparison; tests include i64 wrapping extremes.

Five interleaved trials over 27 files give a median 1.436x raw-frame speedup
against the preceding scalar milestone. Representative complete-file times in
milliseconds are below. The full report retains every case, including small
differences in the unchanged verbatim path.

| Case | Previous | Current | Speedup | libFLAC | FFmpeg | Symphonia | Symphonia + CRC |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 01: block 4096 | 9.687 | 6.747 | 1.436x | 5.336 | 8.277 | 6.744 | 7.083 |
| 03: block 16 | 9.781 | 9.221 | 1.061x | 15.357 | 20.429 | 7.755 | 8.498 |
| 15: verbatim | 0.981 | 0.992 | 0.989x | 4.708 | 2.388 | 1.946 | 2.549 |
| 17: fixed | 4.926 | 3.716 | 1.326x | 4.153 | 4.369 | 4.092 | 4.510 |
| 28: high resolution | 29.483 | 18.298 | 1.611x | 16.441 | 21.216 | 18.248 | 20.204 |
| 31: LPC-32 | 58.144 | 34.567 | 1.682x | 38.994 | 39.305 | 30.856 | 32.956 |
| 44: 8-channel LPC-32 | 861.301 | 510.691 | 1.687x | 595.747 | 598.535 | 463.106 | 503.064 |

The median time is still 1.142x the fastest CRC-checking reference across these
27 cases; the worst is 1.375x on case 43. This is substantial progress, not full
performance parity. Streaming restarts and PCM MD5 are now prominent gaps.
The baseline source driver is revision `5d6a0c00e`; the recorded native binaries
and source hashes identify the candidate separately. Reports:
[raw trials](benches/validation/2026-10-27-kernels/raw.tsv),
[summary](benches/validation/2026-10-27-kernels/summary.json),
[provenance](benches/validation/2026-10-27-kernels/manifest.json).

All seven new unit tests, 43 integration/external gates and three doctests pass.
Four fresh 120-second ASan campaigns complete 113,826 executions without failure.
The final Clippy check passes all targets. Its sole post-test source change is a
local lint attribute retaining constant tap indices in the measured kernel;
the [final validation record](benches/validation/2026-10-27-kernels/final-validation.json)
verifies that removing it reproduces every tested source hash. Full logs, exact
parity reports and the sanitizer compiler audit are retained beside that record.
`benches/validate.py --help` describes the immutable full-gate runner.

### MD5 benchmark correction

**All earlier `libflac-md5` timing columns are invalid.** libFLAC `finish()`
restores default settings, and the original driver failed to enable MD5 again
after its untimed warm-up. These columns remain in the raw historical artifacts
for audit, but must not be used for performance conclusions. Raw-frame, stream
modes with MD5 disabled, and native MD5 measurements are unaffected.

The corrected driver sets and asserts the policy at every initialization.
`benches/check_oracle.py` checks complete-file endpoints, first-frame work and
corrupts the expected digest only after warm-up to prove repeated passes still
verify it. It also tests invalid native and CRC-valid Ogg expected digests.
The [audit passes](benches/validation/2026-10-27-kernels/pipeline-oracle-audit.json).
New pipeline modes include Ogg readers and native/Symphonia/libFLAC construction
and destruction inside the timer. Their measured baseline is active work.

## Streaming buffers and PCM MD5, 2026-10-27

Native readers advance a buffer cursor and compact when more input is needed.
Completed channel planes survive input refills; incomplete planes restart from
their own bit position. Public raw decoding and seeking reset these checkpoints.
The complete frame CRC is checked before PCM is returned. Tests append every
prefix of representative frames, including 33-bit stereo, and exercise error
recovery and replacement input.

MD5 compression uses constant round indices and rotate counts. PCM serialization
specializes byte width and channel count in a separate private module, using a
4 KiB stack buffer. Tests compare it with direct interleaving at every depth and
channel count, batch boundaries and trims, alongside the million-`a` digest.

High-order AVX2 kernels combine four biased sample ranges with bitwise OR and
check the result before the next SIMD batch. Any invalid high bit remains set.
Dependent scalar terms use explicit wrapping after an invalid sample so that
even malformed batches reject without an arithmetic panic. Valid samples retain
the same exact i64 arithmetic. Negative tests cover every position in the batch
and maximum coefficients. Shorter kernels retain individual range branches.

Five interleaved trials compare the preserved preceding milestone with this
candidate. Full native decoding with MD5 improves 1.50–2.13x, and Ogg with MD5
improves 1.29–1.84x across the six selected classes. Times below are complete-file
milliseconds. libFLAC columns use the corrected fresh-decoder MD5 modes.

| Case | Raw speedup | Reader speedup, MD5 off | Native + MD5 | libFLAC + MD5 | Ogg + MD5 | libFLAC Ogg + MD5 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 01: block 4096 | 1.022x | 1.096x | 10.030 | 7.500 | 9.976 | 8.097 |
| 03: block 16 | 1.008x | 1.221x | 13.162 | 16.846 | 16.890 | 18.097 |
| 15: verbatim | 0.979x | 1.098x | 3.114 | 6.028 | 4.115 | 6.931 |
| 28: high resolution | 0.997x | 1.361x | 36.815 | 28.780 | 35.547 | 31.668 |
| 44: 8-channel LPC-32 | 0.956x | 1.771x | 681.617 | 739.869 | 741.707 | 780.032 |
| uncommon 08: block 65535 | 0.996x | 1.296x | 21.000 | 14.258 | 18.724 | 15.045 |

The initial version regressed raw LPC-32. A nine-trial repeat and hardware
counters confirmed extra branch misses with nearly unchanged instruction counts;
a cold-error experiment was rejected. The retained grouped checks reduce branch
pressure, but the final table still records a 4.6% raw slowdown on case 44.
Profiles also expose an indexed accumulator loop in LPC-32. Its unrolling and
the remaining common-file gaps are follow-up work. This milestone improves the
pipelines substantially and does not establish complete performance parity.

Reports: [paired timings](benches/validation/2026-10-27-pipelines/raw.tsv),
[summary](benches/validation/2026-10-27-pipelines/summary.json),
[manifest](benches/validation/2026-10-27-pipelines/manifest.json),
[initial raw repeat](benches/validation/2026-10-27-pipelines/initial-lpc32-repeat-raw.tsv).
The artifact directory retains the initial pipeline measurements and hardware
counters as well as the final results. Startup's first-frame work and MD5 after
warm-up pass the [oracle audit](benches/validation/2026-10-27-pipelines/oracle-audit.json).

All 10 unit tests, 43 integration/external gates and three doctests pass. Clippy
passes all targets. Four fresh 60-second ASan campaigns complete 121,315
executions without failures. Test, fuzz and benchmark build source hashes agree;
see the [final validation record](benches/validation/2026-10-27-pipelines/final-validation.json).

`benches/build_native.py --output /tmp/new-benchmark --target-dir /tmp/flac-build`
builds and preserves a Linux/WSL benchmark with a JSON sidecar containing compiler
versions, flags, workspace manifests/lockfile and codec/Ogg source hashes. It
requires a new output path and fails if inputs change while building. The runner
can pair any native pipeline mode with its `baseline-` equivalent. Allocation
and retained-buffer measurements remain in progress.

## LPC tap unrolling, 2026-10-27

Explicit constant taps keep the AVX2 predictor's four accumulators in registers.
The prior indexed loop generated stack loads and stores for every high-order
tap. Arithmetic, dispatch thresholds and all range checks are unchanged.
Disassembly and paired counters on case 44 confirm the change: instructions
fall from 40.14 billion to 24.53 billion and branches from 3.93 billion to
2.08 billion across the same ten complete passes (prepass, warm-up and eight
timed passes). Branch misses remain approximately 19 million.

Five interleaved trials against the preceding milestone give the following
complete-file milliseconds. References validate frame CRC; the Symphonia column
includes its explicit footer CRC pass.

| Case | Previous raw | Current raw | libFLAC | FFmpeg | Symphonia + CRC | Previous reader + MD5 | Current reader + MD5 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 01: block 4096 | 6.733 | 6.449 | 5.330 | 8.241 | 7.116 | 10.230 | 10.193 |
| 03: block 16 | 8.995 | 9.079 | 15.124 | 20.216 | 8.767 | 14.302 | 14.561 |
| 15: verbatim | 0.969 | 0.970 | 4.626 | 2.323 | 2.544 | 3.202 | 3.228 |
| 17: fixed | 3.694 | 3.700 | 4.111 | 4.410 | 4.438 | 6.360 | 6.485 |
| 28: high resolution | 20.705 | 20.884 | 17.033 | 22.398 | 21.906 | 39.741 | 39.808 |
| 31: LPC-32 | 34.686 | 26.956 | 40.489 | 39.498 | 33.401 | 53.379 | 44.496 |
| 44: 8-channel LPC-32 | 517.592 | 384.340 | 575.261 | 597.942 | 490.571 | 733.157 | 609.436 |

The two LPC-32 files improve 22.3% and 25.7% in raw decode time, overtaking all
three references in this run. Common blocks and high-resolution defaults still
trail libFLAC, so the overall performance goal remains open. A separate MD5
add-order experiment produced a byte-identical executable and was reverted.
Its measurements are preserved as an identical-binary control, with no claimed
optimization.

Reports: [paired timings](benches/validation/2026-10-27-taps/raw.tsv),
[provenance](benches/validation/2026-10-27-taps/manifest.json),
[before counters](benches/validation/2026-10-27-taps/before-stat.csv),
[after counters](benches/validation/2026-10-27-taps/after-stat.csv).
The directory also retains both disassemblies and the identical-binary control.
All 10 unit tests, 43 integration/external gates and three doctests pass, as
does Clippy. Four fresh 60-second ASan campaigns complete 109,156 executions
without failure. Test, benchmark and fuzz source hashes agree in the
[validation record](benches/validation/2026-10-27-taps/final-validation.json).

## Dense PCM storage, 2026-10-27

Subframes now decode directly into i32 PCM planes. A private storage trait keeps
all reconstruction arithmetic and range checks in i64; only a declared 33-bit
stereo side plane uses separate i64 storage. Stereo decorrelation happens in
place after frame CRC validation. Completed-plane checkpoints survive refills
as before. Grouped LPC checks latch invalid high bits before a narrow store and
reject the batch before reuse. Scalar/SIMD comparison and rejection tests now
exercise both storage types.

Separate allocation probes measure requested heap during each decoder region,
excluding resident input and the untimed prepass. Native fresh-reader peak heap
falls from 122,992 to 57,456 bytes on case 01, and from 401,494 to 139,350 bytes
on case 43. Corresponding Ogg peaks fall from 138,698 to 73,162 and 420,312 to
158,168 bytes. Warm raw decoding still performs zero allocations. Native readers
return to their starting live heap after each iteration. These are logical
requested bytes, excluding allocator metadata, stack and RSS; reallocations
count the full new request. Probe timings are never used for throughput claims.
Unchanged Symphonia modes reproduce the same allocation counts and peaks.

Five paired trials improve verbatim raw time from 0.981 to 0.722 ms (26.4%).
Other selected raw cases are close, except an initial apparent slowdown on
high-resolution case 28. A nine-trial repeat measures 20.395 ms before and
20.236 ms after; reader plus MD5 is 39.266 versus 38.064 ms. Paired counters
show similar instructions and branches and L1 load misses falling from 85.05
million to 29.07 million over 102 whole-file passes. The initial result remains
in the reports. Common stereo raw time remains 6.540 ms versus libFLAC's
5.474 ms in the initial run; this milestone reduces memory without closing that
remaining CPU gap. Common/high-resolution profiles attribute about 41–43% of
cycles to Rice decoding.

Reports: [paired timings](benches/validation/2026-10-27-dense/raw.tsv),
[nine-trial repeat](benches/validation/2026-10-27-dense/hires-repeat-raw.tsv),
[allocation phases](benches/validation/2026-10-27-dense/memory-comparison.json),
[reference allocation probes](benches/validation/2026-10-27-dense/memory-initial.json).
Probe sources, profiles, counters and build provenance are retained alongside
them. All 10 unit tests, 43 integration/external gates, three doctests and
Clippy pass. Four fresh 60-second ASan campaigns complete 117,483 executions
without failure. Test, fuzz, benchmark and memory-probe codec source hashes
agree in the [validation record](benches/validation/2026-10-27-dense/final-validation.json).
