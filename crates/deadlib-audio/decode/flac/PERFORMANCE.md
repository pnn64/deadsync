# FLAC decoder performance work

Decoding is now competitive with established decoders on the measured Intel
Broadwell x86-64 host, while preserving exact PCM, format coverage, resource
limits and checksums. Across 27 raw classes, median time relative to the fastest
CRC-checking reference is 0.805x on native Windows and 0.912x on Linux/WSL.
Windows leads 26 classes and is 1.7% behind Symphonia on block-size 16. Linux
retains two roughly 12% raw gaps and a 16.5% common-stereo reader-plus-MD5 gap.
The final platform evaluation below records these differences, startup, seeks
and memory. Encoding performance remains a separate part of the earlier plan.

## Reproducible benchmark

`cargo bench --locked -p deadlib-audio-decode-flac --bench decode -- MODE INPUT ITERATIONS`
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
native readers retain metadata while libFLAC's callback ignores it. The dense
storage milestone below reports requested-heap measurements separately from
throughput; full memory parity remains under evaluation.

The Linux/WSL reference build enables x86 assembly and runtime SIMD, including
AVX2, and LTO. It does not use the assembly-disabled correctness oracle. Rust
uses the workspace bench profile (optimized, LTO, debug line information) and
the portable x86-64 target; no `target-cpu=native` override is used.

Build prerequisites: GCC/G++, make, CMake, NASM and stable Rust. Prepare pinned
reference checkouts as described in COVERAGE.md and OGG.md, and the verified
FFmpeg archive from PARITY.md. Example Linux/WSL commands:

```sh
bash crates/deadlib-audio/decode/flac/benches/build.sh /tmp/flac-perf "$PWD" /path/to/ffmpeg-9.0.2.tar.xz
cargo bench --locked -p deadlib-audio-decode-flac --bench decode --no-run --target-dir /tmp/flac-perf-rust
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

## Polynomial CRC folding, 2026-10-27

Frames of at least 256 bytes use four independent polynomial folds on CPUs
with PCLMULQDQ and SSSE3. Constants are derived at compile time from FLAC's
unreflected polynomial. Each fold preserves congruence modulo that polynomial;
the existing tables reduce the final state and exact tail. Other CPUs and
short frames retain the table implementation. The private unsafe module checks
CPU features and loads only complete 16-byte fields within checked 64-byte
chunks. Frame checksums remain mandatory.

An independent bitwise oracle tests all 16 input alignments, every short length,
block/tail boundaries and long inputs. The initial version left constant
derivation at runtime. Eleven-trial repeats and instruction counts exposed the
resulting short-frame regressions. Explicit `const` evaluation removes that
work; initial evidence remains beside the corrected measurements.

Five interleaved trials across 27 raw cases give a median 1.060x speedup over
the dense-storage milestone. The median time is 0.919x the fastest CRC-checking
reference, with the largest remaining ratio 1.165x on block-size 512. Selected
complete-file milliseconds follow. Symphonia's CRC pass uses the candidate's
public CRC helper and also benefits from folding; its known 32-bit stereo PCM
mismatch remains an explicit failed comparison, excluded from timings.

| Case | Previous raw | Current raw | libFLAC | FFmpeg | Symphonia + CRC |
| --- | ---: | ---: | ---: | ---: | ---: |
| 01: block 4096 | 6.554 | 6.172 | 5.671 | 8.755 | 7.086 |
| 06: block 512 | 6.679 | 6.312 | 5.416 | 8.604 | 7.212 |
| 12: QLP precision 15 | 4.889 | 4.434 | 3.998 | 6.031 | 4.944 |
| 14: wasted bits | 4.550 | 4.386 | 4.414 | 6.001 | 4.715 |
| 15: verbatim | 0.723 | 0.158 | 4.651 | 2.364 | 2.018 |
| 28: high resolution | 20.339 | 17.999 | 16.937 | 22.894 | 19.871 |
| 44: 8-channel LPC-32 | 372.530 | 344.393 | 574.463 | 600.872 | 469.471 |

The initial whole-input CRC pilot improves approximately 10x. Full pipelines
still have gaps: common stereo native plus MD5 takes 10.030 ms against libFLAC's
8.068 ms; high-resolution takes 36.273 versus 28.780 ms. Large LPC-32 remains
faster at 564.594 versus 766.326 ms. Short order LPC and incomplete-plane replay
remain follow-up targets; complete performance parity is not claimed.

Reports: [27-case trials](benches/validation/2026-10-27-folded-crc/broad-raw.tsv),
[pipeline trials](benches/validation/2026-10-27-folded-crc/pipelines-raw.tsv),
[short-frame repeat](benches/validation/2026-10-27-folded-crc/repeat-raw.tsv).
The runner now handles zero-sample streams using time per iteration and honors
explicit cases outside the default set; both are exercised in the retained
empty-stream check. Build provenance includes the embedded README and toolchain
configuration, and queries compiler versions from the repository directory.
All 11 unit tests, 43 integration/external gates, three doctests and Clippy pass.
Four fresh 60-second ASan campaigns complete 134,345 executions without failure.
Test, fuzz and benchmark codec hashes agree in the
[validation record](benches/validation/2026-10-27-folded-crc/final-validation.json).

## Residual checkpoints and short LPC, 2026-10-27

Streaming preserves completed residual samples, the current partition code and
the start of an incomplete Rice symbol. Verbatim/escape fields retain complete
sample prefixes. Predictor reconstruction runs once after the complete residual
arrives; wasted bits, stereo decorrelation and CRC policies are unchanged.
Completed buffered frames can use the raw path. STREAMINFO's maximum frame size
only selects an optimistic attempt: an incorrect small hint falls back to normal
resumption. Tests append every byte prefix with both normal and false hints,
including 33-bit stereo, reset/recovery and bad CRCs. A separate Rice test covers
every parameter and bit offset while preserving completed samples.

Raw Rice decoding retains its prior signature and loop. Resumable reads return
checkpoint information on failure, without passing a mutable progress record
through the raw hot loop. The initial shared-signature implementation regressed
raw throughput and short-block readers; its measurements and passing correctness
reports remain in the artifact directory. The retained version separates those
paths. AVX2 LPC now also handles order three; isolated real-subframe trials and
complete-file trials establish its benefit before combination with checkpoints.

Five interleaved trials over 27 raw cases give a median time of 0.912x the
fastest CRC-checking reference. Raw throughput is nearly unchanged overall from
the preceding milestone; the largest remaining ratio is 1.125x on block-size
512. The six fresh-reader classes below show complete-file milliseconds, with
MD5 enabled in both native and libFLAC columns.

| Case | Previous reader, MD5 off | Current reader, MD5 off | Previous + MD5 | Current + MD5 | libFLAC + MD5 |
| --- | ---: | ---: | ---: | ---: | ---: |
| 01: block 4096 | 6.759 | 6.321 | 9.879 | 9.344 | 8.019 |
| 03: block 16 | 9.322 | 9.528 | 14.470 | 14.572 | 17.775 |
| 15: verbatim | 0.275 | 0.295 | 2.393 | 2.398 | 6.510 |
| 28: high resolution | 21.017 | 18.031 | 34.913 | 32.044 | 28.893 |
| 44: 8-channel LPC-32 | 357.625 | 351.831 | 534.767 | 534.889 | 737.151 |
| uncommon 08: block 65535 | 11.866 | 10.008 | 16.740 | 14.900 | 14.409 |

High-resolution and large-block readers improve 14.2% and 15.7% with MD5 off,
and 8.2% and 11.0% with MD5 on. The table retains small changes in other classes.
At this milestone, the common-reader gap and the two short-LPC raw outliers kept
the performance evaluation open. Ogg continues to receive complete packets and
uses the raw path; its paired measurements are retained in the same report.

Reports: [27-case raw trials](benches/validation/2026-10-27-checkpoints/broad-raw.tsv),
[full pipelines](benches/validation/2026-10-27-checkpoints/pipelines-raw.tsv),
[LPC-3 trials](benches/validation/2026-10-27-checkpoints/lpc-three-raw.tsv),
[initial shared-signature trials](benches/validation/2026-10-27-checkpoints/initial-broad-raw.tsv).
All 12 unit tests, 43 integration/external gates, three doctests and Clippy pass.
Four fresh 60-second ASan campaigns complete 118,608 executions without failure.
Codec source hashes agree between tests, fuzzing and the preserved benchmark in
the [validation record](benches/validation/2026-10-27-checkpoints/final-validation.json).

## Native Windows and final decode evaluation, 2026-10-27

The same validated codec sources were built for native Windows, using Rust
1.99.1/LLVM 23.1.1 and MSVC 19.44.35229.0 for libFLAC 1.5.0/libogg 1.3.6.
The Rust bench profile uses LTO and the portable x86-64 target, with the workspace's
static CRT configuration. libFLAC uses Release optimization, static CRT, LTO,
and its default enabled assembly/AVX2 intrinsics. The C++ driver uses LTO and a
separate non-LTO PCM-address escape helper; libogg uses normal Release
optimization because its older CMake policy ignores the IPO setting. The saved
project settings and logs establish the actual flags. FFmpeg's optimized
comparison remains the Linux/WSL 27-class run above.

Five interleaved trials per class, virtual CPU 2 and seed 9639 give native Windows
a median 0.805x time relative to the fastest available CRC-checking reference.
Native wins 26 of 27 classes; block-size 16 takes 1.017x Symphonia's time.
Symphonia's known 32-bit stereo PCM mismatch is retained as an unsupported
comparison, with libFLAC still providing that class's reference. Selected median
milliseconds for complete raw-frame decoding:

| Case | Native | libFLAC | Symphonia + CRC |
| --- | ---: | ---: | ---: |
| 01: block 4096 | 5.963 | 7.502 | 7.000 |
| 03: block 16 | 9.195 | 15.313 | 9.044 |
| 06: block 512 | 6.603 | 8.606 | 7.938 |
| 12: QLP precision 15 | 4.442 | 5.986 | 5.054 |
| 14: wasted bits | 4.273 | 6.063 | 4.846 |
| 15: verbatim | 0.273 | 4.236 | 1.945 |
| 28: high resolution | 18.020 | 28.207 | 19.939 |
| 44: 8-channel LPC-32 | 325.981 | 474.812 | 447.780 |
| uncommon 08: block 65535 | 9.206 | 11.727 | 10.981 |

Fresh native and Ogg readers also lead libFLAC and Symphonia plus CRC in all six
selected classes with MD5 off. With MD5 enabled, both readers lead libFLAC in all
six classes. Complete-file milliseconds, including reader construction and
destruction, follow. MD5 remains enabled on every timed pass in both codecs.

| Case | Native + MD5 | libFLAC + MD5 | Native Ogg + MD5 | libFLAC Ogg + MD5 |
| --- | ---: | ---: | ---: | ---: |
| 01 | 9.096 | 10.849 | 9.159 | 11.091 |
| 03 | 14.538 | 19.656 | 17.815 | 21.141 |
| 15 | 2.537 | 6.961 | 3.271 | 7.717 |
| 28 | 32.535 | 42.809 | 36.223 | 46.616 |
| 44 | 566.671 | 729.997 | 635.556 | 764.337 |
| uncommon 08 | 15.797 | 16.847 | 16.336 | 18.215 |

First-frame startup includes fresh reader allocation, metadata parsing, decoding
one complete frame and destruction. Input-file reads remain outside the clock.
Both native and libFLAC verify frame CRCs. Symphonia's startup mode omits footer
CRC and remains explicitly identified in the raw reports. Median microseconds:

| Case | Windows native | Windows libFLAC | Linux native | Linux libFLAC |
| --- | ---: | ---: | ---: | ---: |
| 01 | 72 | 134 | 54 | 74 |
| 03 | 9 | 15 | 7 | 14 |
| 06 | 19 | 28 | 17 | 23 |
| 12 | 165 | 194 | 109 | 99 |
| 14 | 19 | 31 | 18 | 24 |
| 15 | 18 | 184 | 13 | 98 |
| 28 | 136 | 191 | 96 | 94 |
| 44 | 2,068 | 3,506 | 2,478 | 3,421 |
| uncommon 08 | 2,003 | 2,364 | 1,548 | 1,929 |

Exact-seek measurements initialize metadata-only readers outside the clock and
retain their seek state. Each trial makes two ascending cycles through 97 evenly
spaced targets and consumes the returned trimmed frame. Aggregate PCM endpoint
checksums agree between implementations and between warmup policies. Lazy mode
includes native frame-index discovery; primed mode explicitly visits all 97
targets before timing in both APIs. Both retain frame CRC checks, with
whole-stream MD5 unavailable after arbitrary seeks. Primed median microseconds:

| Case | Windows native | Windows libFLAC | Linux native | Linux libFLAC |
| --- | ---: | ---: | ---: | ---: |
| 01 | 78.4 | 203.3 | 85.8 | 160.4 |
| 03 | 1.61 | 6.25 | 1.70 | 8.64 |
| 28 | 79.8 | 273.5 | 95.1 | 238.7 |
| 44 | 2,016.7 | 4,204.1 | 2,077.7 | 5,019.5 |
| uncommon 08 | 1,420.0 | 2,842.1 | 1,396.0 | 2,501.3 |

Native lazy mode leads four of five classes on both platforms. Block-size 16
exposes its index-discovery cost: Windows averages 57.7 us against libFLAC's
6.33 us; Linux averages 57.4 against 9.07 us. Native discovers frames by checked
forward decoding and retains bounded landmarks; libFLAC uses byte search.
An individual first distant seek can therefore cost a sequential decode to its
target. Primed timings include the retained index's memory tradeoff; they do not
establish cold random-seek parity or physical-disk latency.

The final requested-heap probe confirms zero raw allocations after warmup and
unchanged native streaming peaks: 57,456 bytes for case 01 and 139,350 for case
43. Ogg peaks are 74,538 and 159,544 bytes, 1,376 bytes above the dense-storage
milestone because the resumable decoder state is larger. Fresh readers return
to their starting live allocation count. These two-file probes exclude stack,
RSS, allocator metadata and seek-index memory.

The measured Windows decode paths now meet or exceed the reference throughput
across the tested families, within 1.7% on the remaining raw class. Linux raw
median time is 0.912x the fastest CRC-checking reference; block-size 512 and QLP
precision 15 remain 12.5% and 11.8% behind. Linux common-stereo native plus MD5
remains 16.5% behind libFLAC, high resolution 10.9% behind. Large LPC-32 and
verbatim workloads are substantially faster. These results establish competitive
sequential decoding on this host. Other CPU architectures, encoding speed and
cold byte-search seeking remain outside that conclusion. Windows and WSL
absolute timings are separate populations on a shared host without a frequency
lock; the reports retain individual trial ranges.

The final codec source hashes exactly match the preceding milestone's 12 unit
tests, 43 integration/external gates, three doctests and 118,608 ASan executions.
Fresh Windows/Linux harness audits pass full native/Ogg endpoint checks,
first-frame work checks and negative MD5-after-warmup probes. Clippy includes the
new benchmark modes. Portable host adapters, the libFLAC-only C benchmark and
the separate primed-seek mode are now maintained under `benches/`.

Reports: [Windows 27-class trials](benches/validation/2026-10-27-platforms/windows-broad-raw.tsv),
[Windows pipelines](benches/validation/2026-10-27-platforms/windows-pipelines-raw.tsv),
[Windows startup](benches/validation/2026-10-27-platforms/windows-startup-raw.tsv),
[Linux startup](benches/validation/2026-10-27-platforms/linux-startup-raw.tsv),
[Windows lazy seeks](benches/validation/2026-10-27-platforms/windows-seek-raw.tsv),
[Windows primed seeks](benches/validation/2026-10-27-platforms/windows-seek-warm-raw.tsv),
[Linux lazy seeks](benches/validation/2026-10-27-platforms/linux-seek-raw.tsv),
[Linux primed seeks](benches/validation/2026-10-27-platforms/linux-seek-warm-raw.tsv),
[heap checks](benches/validation/2026-10-27-platforms/memory-checkpoints.json),
[provenance and validation](benches/validation/2026-10-27-platforms/final-validation.json).

## Exact encoder search and LPC work, 2026-10-28

Three safe-Rust changes reduce repeated work in `FrameEncoder`:

1. Combine the residual sum and escape-width scan. Zigzag bit length equals
   signed sample width, and an OR reduction retains the maximum bit length.
   This removes the separate per-sample width calculation and its branches.
2. Skip Rice method 1 when every method-0 partition chose `k < 14`. Convex
   Rice cost means those parameters are already unrestricted optima; method 1
   can only add one header bit per partition. Boundary parameters and escapes
   retain the full search, including the original tie decisions.
3. Compute Hann-weighted PCM once per plane, reusing it across LPC correlation
   lags. The cosine closure, sample conversion and window multiplication leave
   the inner loop. Arithmetic and summation order remain unchanged.

Each change was built separately and measured against the preceding preserved
binary. On the same 4,096-sample, stereo 16-bit smooth/noisy signal at level 5:

| Change | Before cycles/frame | After cycles/frame | CPU throughput | PCM Msamples/s, before → after |
| --- | ---: | ---: | ---: | ---: |
| Combined residual statistics | 18,479,892 | 13,070,740 | 1.414x | 0.83 → 1.18 |
| Skip redundant Rice method | 13,081,264 | 9,913,760 | 1.320x | 1.17 → 1.55 |
| Reuse Hann-weighted PCM | 9,908,080 | 6,303,152 | 1.572x | 1.55 → 2.44 |

Final build versus the original `9e8f72ac0` encoder:

| Case | Before cycles/frame | After cycles/frame | CPU throughput | PCM Msamples/s, before → after |
| --- | ---: | ---: | ---: | ---: |
| Smooth 16-bit stereo, level 0 | 897,148 | 583,497 | 1.538x | 17.15 → 26.37 |
| Smooth 16-bit stereo, level 2 | 5,901,446 | 2,505,130 | 2.356x | 2.59 → 6.08 |
| Smooth 16-bit stereo, level 5 | 18,475,349 | 6,299,192 | 2.933x | 0.83 → 2.44 |
| Smooth 16-bit stereo, level 8 | 78,509,418 | 33,193,964 | 2.365x | 0.20 → 0.46 |
| Noise 16-bit stereo, level 2 | 3,387,361 | 1,976,098 | 1.714x | 4.54 → 7.79 |
| Noise 24-bit stereo, level 2 | 3,665,044 | 2,256,545 | 1.624x | 4.20 → 6.82 |
| Smooth 32-bit mono, level 5 | 4,521,837 | 2,276,339 | 1.986x | 1.70 → 3.38 |
| Smooth 16-bit stereo, level 2, block 16 | 19,457 | 11,945 | 1.629x | 3.09 → 5.03 |

Host: Intel Core i7-1250U, Windows x86-64 MSVC, rustc 1.97.1, existing Cargo
bench profile and static CRT. The benchmark pins its thread to logical CPU 2.
Cycles are current-thread `QueryThreadCycleTime` differences; throughput uses
`Instant`. PCM generation, roundtrip verification and heap accounting are
outside timing. Each trial warms and reuses one encoder. Trials interleave the
two executables in seeded random order: 11 rounds at approximately 60 ms per
variant for individual changes, 15 rounds at approximately 100 ms for the final
comparison. Tables report medians. An initial unchanged-binary control stayed
within 0.24%. The host is not frequency-locked: absolute counts vary across
measurement sessions, so compare paired variants within each report.

The second change's initial wide-input differences of -3.5% and -1.4% did not
repeat: 21 rounds at 100 ms measured both within 0.1%. The initial Hann change
measured -1.8% on block-16 level-2 encoding, which does not enter the LPC path;
the final comparison retains a 1.629x gain on that case. All raw trials, including
these initial results, are retained.

All eight cases allocate and reallocate zero times per frame after warmup.
The first two changes leave cold heap counts unchanged. The LPC change adds
one reusable `Vec<f64>` allocation of `8 * block_size` bytes: 32,768 bytes for
these 4,096-sample blocks, shared across channels. At level 5 stereo, cold
allocation calls increase from 10 to 11 and cumulative requested bytes from
169,406 to 202,174. No buffer is allocated for levels 0–2. Requested bytes include
reallocation requests; they do not represent RSS or retained heap.

Correctness: each intermediate and final binary emits exactly the same 635,954
bytes as the baseline over 1,044 generated frames (depths 4–32, channels 1/2/8,
levels 0/2/5/8, block sizes 16/67/257). The larger benchmark frames additionally
match the baseline's encoded-frame digest and original integer PCM. Two new
tests check signed-width boundaries and 1,920 Rice plans against exhaustive
parameter, method and partition-order enumeration. Debug and release suites
pass all 14 unit tests, 35 integration tests and three doctests; Clippy and
format checks pass. The eight external corpus/oracle gates remain ignored and
were not rerun. No production unsafe code was added.

The evidence concerns synthetic resident-frame **encoding** on this host.
It does not establish full music-file encoding, decoder or playback speed.
The benchmark and preserved build manifests are reproducible with:

```powershell
python crates/deadlib-audio/decode/flac/benches/build_native.py --bench encoding --target-dir target --output target/flac-perf/current.exe
python crates/deadlib-audio/decode/flac/benches/encoding_perf.py --before target/flac-perf/baseline.exe --after target/flac-perf/current.exe --rounds 15 --milliseconds 100 --output target/flac-perf/comparison.json
target/flac-perf/current.exe matrix target/flac-perf/current-matrix.bin
fc.exe /B target\flac-perf\baseline-matrix.bin target\flac-perf\current-matrix.bin
```

To rebuild the baseline, use `src/encode.rs` from `9e8f72ac0` in an isolated
checkout with the current encoding benchmark, shared test support and benchmark
registration. Use the same Cargo profile and target configuration for both.
`--case` selects individual cases; the cycle counter currently supports Windows.

Evidence: [per-change residual statistics](benches/validation/2026-10-28-encoding/width.json),
[Rice method](benches/validation/2026-10-28-encoding/rice.json),
[Rice repeat](benches/validation/2026-10-28-encoding/rice-repeat.json),
[Hann reuse](benches/validation/2026-10-28-encoding/window.json),
[final trials](benches/validation/2026-10-28-encoding/final.json),
[unchanged control](benches/validation/2026-10-28-encoding/control.json),
[validation scope](benches/validation/2026-10-28-encoding/validation.json).
The same directory preserves source/toolchain hashes and build logs for every
binary. Executables and generated PCM/output remain under `target/flac-perf`.

## Direct frame output and word packing, 2026-10-28

The next pass uses `55ea6aaa2` as its baseline and retains three safe-Rust changes:

1. Write headers, subframes and CRC into the bit writer's single buffer. This
   removes `FrameEncoder::output`, its allocation and the completed-payload copy.
2. Emit each unary tail and its terminating one bit with one `write` call.
   This removes the second field write and its capacity check.
3. Align each field in a `u64` once and append its big-endian bytes, replacing
   the byte-at-a-time packing loop. Zero/one-byte appends have direct paths.
   An unaligned 64-bit field explicitly emits its ninth byte; zero-width writes
   return immediately without reserving storage. All stores remain checked.

Each change was measured against its predecessor. The allocation improvement
does not depend on a CPU-speed claim; its measured CPU cost is largely unchanged.

| Change and case | Before cycles/frame | After cycles/frame | CPU throughput | PCM Msamples/s, before → after |
| --- | ---: | ---: | ---: | ---: |
| Single buffer, 24-bit noise, level 2 | 2,253,080 | 2,259,333 | 0.997x | 6.83 → 6.81 |
| Fused unary, smooth 16-bit, level 0 | 586,126 | 514,653 | 1.139x | 26.25 → 29.90 |
| Word packing, 32-bit noise, level 0 | 467,208 | 404,297 | 1.156x | 32.91 → 38.08 |

The single-buffer case reduces cold allocation calls from 8 to 7, reallocations
from 14 to 12, and cumulative requested bytes from 235,976 to 213,024. Every one
of the nine final cases removes one cold allocation. All retain zero allocations
and reallocations per frame after warmup. Requested-byte totals include repeated
reallocation requests and do not measure RSS or retained heap. The existing Hann
scratch buffer from the preceding pass is unchanged.

Final comparison against the starting encoder, with 4,096 samples/channel except
the explicitly short case:

| Case | Before cycles/frame | After cycles/frame | CPU throughput |
| --- | ---: | ---: | ---: |
| Smooth 16-bit stereo, level 0 | 582,899 | 502,612 | 1.160x |
| Smooth 16-bit stereo, level 2 | 2,381,354 | 2,122,344 | 1.122x |
| Smooth 16-bit stereo, level 5 | 6,306,574 | 6,217,710 | 1.014x |
| Smooth 16-bit stereo, level 8 | 33,220,100 | 33,431,378 | 0.994x |
| Noise 16-bit stereo, level 2 | 1,969,506 | 1,964,086 | 1.003x |
| Noise 24-bit stereo, level 2 | 2,256,599 | 2,117,926 | 1.065x |
| Smooth 32-bit mono, level 5 | 2,271,061 | 2,211,086 | 1.027x |
| Smooth 16-bit stereo, level 2, block 16 | 11,964 | 11,797 | 1.014x |
| Noise 32-bit stereo, level 0 | 604,978 | 404,809 | 1.494x |

The largest final gains raise PCM throughput from 26.39 to 30.62 Msamples/s on
smooth 16-bit level 0 and from 25.43 to 38.00 Msamples/s on 32-bit noise level 0.
LPC-heavy level 8 remains dominated by analysis; its -0.6% result is recorded.

These measurements use the preceding host, compiler, Cargo profile and logical
CPU 2. Initial process comparisons varied substantially with CPU power state.
The maintained harness now performs a 200 ms untimed encoding warmup and gives
both variants the same iteration count. Per-change trials use 11 rounds at about
100 ms per variant. The added 32-bit noise case uses 21 rounds at 150 ms; the
final nine-case comparison uses 15 rounds at 150 ms. Windows thread cycles are
the primary metric; PCM throughput uses elapsed time. Each process independently
checks original PCM and encoded-frame digests outside timing. The host still has
variable frequency and raw rows retain outliers; these are paired comparisons,
not portable absolute performance claims.

The first word-packing implementation regressed smooth 16-bit level 0 by 4.8%.
It was discarded. Direct zero/one-byte appends remove that regression; the
revised comparison is 1.004x on that case and improves wide-field encoding.
Initial measurements and the rejected candidate's report remain in the evidence.

Correctness: the 1,044-frame matrix is still byte-identical (635,954 bytes with
the same SHA-256 as the preceding pass). A new independent bit-at-a-time test
checks 7,800 fields over widths 0–64, every initial bit offset, high discarded
bits, three clear/reuse passes, long unary prefixes, padding and invalid-width
failure state. Debug and release pass all 14 unit tests, 36 integration tests and
three doctests. Clippy and formatting pass. Eight external oracle/corpus gates
remain ignored and were not rerun. No production unsafe code was added.

Reproduce with the preceding commands and the updated encoding harness. To
rebuild the baseline, check out `55ea6aaa2` in isolation and copy the current
`benches/encoding.rs` and `benches/encoding_perf.py` into that checkout before
building. The benchmark is already registered at that revision. `--case
noise32-l0` selects the new wide-field case. Saved binaries and complete encoded
matrices remain under `target/flac-pass2`.

Evidence: [single-buffer trials](benches/validation/2026-10-28-bitwriter/direct-warm.json),
[unary fusion](benches/validation/2026-10-28-bitwriter/unary.json),
[rejected word packing](benches/validation/2026-10-28-bitwriter/word.json),
[small-field append paths](benches/validation/2026-10-28-bitwriter/word-small.json),
[wide-field trials](benches/validation/2026-10-28-bitwriter/word-wide.json),
[final comparison](benches/validation/2026-10-28-bitwriter/final.json),
[validation scope](benches/validation/2026-10-28-bitwriter/validation.json).
Build manifests, compiler logs, complete check results and the initial protocol
diagnostics are preserved in the same directory. These results measure synthetic
resident-frame encoding; they do not establish decoder or playback throughput.
