# Audio gain and validation math, 2026-11-04

Three retained changes in `deadlib-audio-core`, starting at `490a84271` on
isolated branch `perf/non-lua-kernels-20261104`. Lua and the main worktree's
existing edits are outside this batch.

## Changes and exactness

1. `advance_gain` compares the difference's unsigned magnitude bits with the
   existing step's bits. For nonnegative finite floats, bit order is numeric
   order; NaN patterns are above infinity and cannot pass the settling check.
   Non-NaN differences add a signed copy of the existing rounded step. The
   original sign multiplier is exactly `+1` or `-1`, so its product introduces
   no rounding and FMA gives the same result as that addition. NaN differences
   retain the original `signum().mul_add(...)` expression. Settling, returned
   booleans, signed zeros, infinities and sequential ramp state are preserved.
2. `normalized_music_rate` recognizes positive finite `f32` values with one
   unsigned interval check. Their patterns are `1..0x7f80_0000`; wrapping
   subtraction by one and comparison with `0x7f7f_ffff` accepts exactly that set.
   Both zeros, negatives, infinities and NaNs still return `1.0`; accepted
   values retain their exact bits, including positive subnormals.
3. Mixer gain validation recognizes nonnegative finite patterns below
   `0x7f80_0000`, plus the separate `0x8000_0000` negative-zero pattern. This
   preserves the original `>= 0.0` contract, which differs from the rate guard.
   Stream/bus gain setters keep their atomic ordering and output bits.

The crate's `forbid(unsafe_code)` remains in place. There are no new dependencies,
state fields, caches or public interfaces. The three production function-body
changes add 14 and remove four lines; surrounding production code is unchanged.
Normal ramp updates avoid the signum/FMA path, and validation avoids separate
floating-point finiteness/sign comparisons. The simple PCM-conversion loop is
retained after an unsuccessful batching experiment.

## Final measurements

| Workload | Before cycles/batch | After cycles/batch | CPU throughput |
| --- | ---: | ---: | ---: |
| Gain step, rising (256 updates) | 3,128.53 | 2,621.12 | 1.194x |
| Gain step, falling (256 updates) | 3,070.37 | 2,602.39 | 1.180x |
| Gain step, settling (256 updates) | 2,033.89 | 1,659.79 | 1.225x |
| Gain step, steady (256 updates) | 1,866.55 | 1,458.21 | 1.280x |
| Stereo ramp conversion (512 frames) | 4,416.78 | 3,447.47 | 1.281x |
| Rate validation, valid (256 calls) | 2,233.56 | 1,293.15 | 1.727x |
| Rate validation, mixed (256 calls) | 1,968.48 | 1,391.04 | 1.415x |
| Gain set/get, valid (256 calls) | 1,562.18 | 908.58 | 1.719x |
| Gain set/get, mixed (256 calls) | 1,604.18 | 943.92 | 1.699x |

All nine workloads assert zero allocations, reallocations, frees and byte
churn for both variants in every process. All five runs improve rising/falling
updates, complete conversion, and both validation paths. Paired ranges:

- Gain step, rising (256 updates): 1.156-1.205x.
- Gain step, falling (256 updates): 1.141-1.200x.
- Stereo ramp conversion (512 frames): 1.281-1.371x.
- Rate validation, valid (256 calls): 1.655-1.729x.
- Gain set/get, valid (256 calls): 1.647-1.797x.

## Behavior verification

All 132 final release and 131 debug test executions pass. One existing
overflow-history test is gated by `cfg(not(debug_assertions))`. The existing
200,000 gain-pair differential cases compare four successive updates by exact
bits. Additional checks exercise explicit step boundaries, signed zeros,
subnormals, extrema, infinities and NaN payloads, plus 200,000 random rate
patterns and 200,000 random gain patterns. Gain setters are checked through
both stream and bus APIs, including preservation of negative zero.

Every archived stage captures the existing deterministic render trace in a
fresh process. It covers i16/f32 callbacks, device-channel normalization,
unaligned buffers, gain changes, generation transitions, SFX scheduling and
voice state, underruns, frame counters and played-map records. All traces are
byte-identical to the pre-batch baseline: **2,588,580 bytes**, SHA-256
`d20cd6571f3061a9330779228c16490a3e8ce04137a1c0144961c8defa255389`.
Debug and release agree as well; no physical audio device is opened.

## Evidence and rejected candidates

Adjacent TSVs/manifests retain each measured stage:

- `baseline`: unchanged production math with the initial 13 workloads.
- `gain-step`: discarded floating NaN guard plus signed addition. Complete
  stereo conversion regressed to 0.908x despite correct output.
- `gain-bits`: retained shared magnitude classification and signed addition;
  complete conversion improved 1.346x in this probe.
- `pcm-arrays`: discarded eight-sample array mapping. PCM workloads regressed
  to roughly 0.973-0.986x, so the original scalar-source loop was restored.
- `rate-bits`: retained rate guard and pre-change gain-setter measurements.
- `gain-validation`: retained nonnegative gain classification.
- `control`: exact pre-batch production sources with the final nine workloads.
- `final`: all three changes, five process runs.
- `debug`: complete correctness suite and trace in the unoptimized profile.

Early harnesses included six PCM workloads and a test for the proposed array
boundaries. Those were removed after rejecting the array change; existing PCM
tests remain. Gain-setter cases were added before changing gain validation.
Final tests and timed functions are captured exactly in the manifests.

The unchanged-source control's medians range from 0.939x to 1.050x. Its
complete-conversion ratio is 1.014x; gain-control ratios are 1.036x/1.050x and
rate ratios are 0.985x/1.027x. Absolute cycle levels vary substantially across
stages, so use paired measurements and interpret small differences cautiously.
These results describe CPU kernels, not end-to-end audio latency or a
whole-application performance gain.

## Method and reproduction

Windows x86-64, Rust 1.99.1 / LLVM 23.1.1, CPU identifier
`Intel64 Family 6 Model 79 Stepping 1, GenuineIntel`. The runner builds exact
crate sources in a small offline workspace at optimization level 3 with LTO,
no incremental compilation and the static MSVC runtime flag. Source,
executable, trace and correctness-log hashes, dependency lockfile, compiler,
flags and commands are recorded. Exact sources/binaries/logs are archived at
`D:\deadsync-audio-math-20261104` on the measurement machine.

Each benchmark thread is pinned to CPU 2. Twelve rounds alternate variants;
the warmup is discarded and the median of 11 `QueryThreadCycleTime` samples is
reported. The table uses medians of five process runs. Gain/rate batches use
3,000 iterations per sample, complete conversion 500. Fixture preparation and
allocation counting stay outside timing. Both variants use the existing
thread-local counted system allocator and black-boxed inputs/results.

Gain-step batches perform 256 sequential updates from the named starting
state. Conversion processes 512 stereo frames at volume 0.8, ramping 0 to 1.
Rate batches normalize 256 values. Gain-control batches validate, store with
Release, and read with Acquire for 256 values. Their frozen reference uses the
original predicate and a single `AtomicU32`; the live path uses `MixControls`.
The unchanged control calibrates this reference's calling/storage context.

From the repository root in PowerShell, with cached dependencies available:

```powershell
$env:DEADSYNC_AUDIO_MATH_ARCHIVE = 'D:\deadsync-audio-math-reproduction'
python scripts/benchmark_audio_math.py baseline --baseline 490a8427123d31d342131c0671febfd2ca384683
python scripts/benchmark_audio_math.py control --baseline 490a8427123d31d342131c0671febfd2ca384683
python scripts/benchmark_audio_math.py final --runs 5
python scripts/benchmark_audio_math.py debug --debug
$env:RUSTFLAGS = '-C target-feature=+crt-static'
$env:TEMP = "$env:DEADSYNC_AUDIO_MATH_ARCHIVE\tmp"
$env:TMP = $env:TEMP
cargo clippy --offline --manifest-path "$env:DEADSYNC_AUDIO_MATH_ARCHIVE\workspace\Cargo.toml" --all-targets --no-deps
```

The embedded manifest lockfile can pin the recorded dependency resolution.

Clippy completes with three existing warnings: constant-sized `chunks_exact`
and `chunks_exact_mut` in the unchanged panned fallback, and an eight-argument
telemetry method. Included copies of those sources repeat the same warnings.
Modified helpers and test code add none. Changed production helper fragments
and test/support files match rustfmt; the Python runner compiles and whitespace
checks pass. The checks manifest verifies all archived source, executable,
trace and correctness hashes, plus the restriction to three production function
bodies. Final/debug source hashes match the measured working-tree files.
