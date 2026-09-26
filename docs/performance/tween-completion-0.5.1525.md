# Finalize completed tweens in place

Baseline: `f5328a610`.

`TweenSeq::update` previously took a completed `RuntimeSegment` out of its
`Option`, applied its prepared final values, and dropped it. The segment contains
inline source and prepared-operation buffers. The caller now applies the same
values through its existing mutable borrow, then clears the option, avoiding
the move of those buffers solely to read and destroy them.

No cache, representation, floating-point calculation, or queue policy changed.
This path runs as presentation tweens complete, including queued instant steps
and sleeps. It does not change how often animations update.

## Behavior checks

The new completion test passed before and after. It covers 0, 1, 12, 16, 17,
and 32 operations, crossing both inline-storage limits; ordered writes;
partial and complete updates; zero-duration steps; relative targets; and
passing leftover time to the next queued step.

The same snapshot fixture ran against separately built baseline and changed
release executables. All 4,050 records were byte-identical. Each record contains
the exact bits of all 30 floating-point state fields, all three Boolean state
fields, and whether the queue is empty. Cases include five easing functions,
sleep and instant steps, inline and spilled buffers, zero/finite/infinite
durations, and zero, negative, NaN, finite, and infinite time increments.
Targets include signed zero, NaN, and both infinities. The SHA-256 of each
capture was `FA5068254C8F158DCA8902A4AB0EDB8523985E6FA1795B666C3353C2165DA744`.

The full release crate suite passed. Debug validation passed all 171 library
tests and the new completion test. Clippy passed with performance lints denied;
existing unrelated style warnings remain. Commands after the production edit:

```powershell
cargo test --locked --release -p deadlib-present
cargo test --locked -p deadlib-present --lib --test tween_completion
cargo clippy --locked -p deadlib-present --lib -- -D clippy::perf
```

## Before/after measurements

Windows x86-64, Intel Xeon E5-2696 v4, Rust 1.98.1 / LLVM 22.1.8, repository
release profile with LTO. Three paired runs pinned to logical CPU 6, reversing
executable order in run 2. No builds or other tests from this pass ran during
the measurements. Both executables contain the same final benchmark source.

Each completion operation updates 64 sequences. Fixture cloning and destruction
are outside timing; destruction caused by completion is inside. Seven batches
of 512 operations are measured. Positive-duration segments are prepared with a
0.0625-second update before measurement. Chain cases complete 32 segments per
sequence; single cases complete one. Each segment contains the indicated number
of ordered X writes. Sleeps have no writes. Inputs and output state pass through
`black_box`.

Times below are medians of run medians, divided by 64. Cycle reductions are
medians of paired reductions, measured with `QueryThreadCycleTime`.

| Fixture | Before ns/sequence | After ns/sequence | Cycle reduction | Paired range |
| --- | ---: | ---: | ---: | ---: |
| Complete sleep | 48.6 | 28.3 | 33.5% | 31.7% to 49.5% |
| Complete 1 inline operation | 51.6 | 29.3 | 39.0% | 29.8% to 47.8% |
| Complete 12 inline operations | 59.4 | 49.0 | 12.8% | 11.5% to 34.4% |
| Complete 32 spilled operations | 350.0 | 344.3 | 3.2% | -2.9% to 14.8% |
| Complete 32 instant segments | 3,604.2 | 3,182.6 | 11.7% | 5.9% to 23.3% |
| Complete 32 timed segments | 3,372.4 | 3,043.8 | 10.0% | 9.5% to 18.2% |
| Incomplete segment | 51.6 | 46.5 | 6.7% | 3.8% to 16.5% |
| Empty queue | 27.5 | 26.7 | 2.0% | -10.6% to 6.4% |

Inline-completion throughput increased 19.0% to 76.3%, depending on operation
count; chain throughput increased 10.8% to 13.2%. No reliable improvement is
claimed for spilled segments or empty queues.

Early controls using freshly cloned sequences were noisy, so a separate steady
control updates the same 64 sequences without cloning between operations. Its
long duration prevents completion during all seven batches of 32,768 operations.
Ongoing updates measured 61.8 to 54.8 ns/sequence, with 9.7% fewer cycles
(paired range 8.8% to 11.3%). Steady empty queues measured 31.3 to 31.2
ns/sequence, with cycle changes from a 0.7% increase to a 2.8% reduction.
These controls establish no observed steady-update regression on this build;
their incidental gains are not the intended optimization.

Allocation accounting runs separately from timing. All cases retain zero
allocations and reallocations. Spilled completion frees two existing buffers
per sequence (2,304 bytes total), exactly as before; other cases free nothing.

These are CPU microbenchmarks of tween updates, not whole-frame or GPU timings.
The completion harness includes per-batch timer overhead in elapsed times and
cycle counts; clock queries contribute additional overhead to cycle counts.
No overhead is subtracted, and no exact pure-function cycle count is claimed.
The [raw CSV](tween-completion-0.5.1525.csv) retains all 60 final measurements,
including timing ranges and allocation counts. CSV times, cycles, and churn
are per 64-sequence operation. Throughput units are completed segments for
completion cases and update calls for controls.

## Reproduce

Build the same test target against each production revision and save the
executables before running them serially. For the baseline, retain the new test
file while restoring `src/anim.rs` from the baseline commit above.

```powershell
cargo test --locked --release -p deadlib-present --test tween_completion --no-run
(Get-Process -Id $PID).ProcessorAffinity = 64
# Run the saved before/after executables with each filter:
# benchmark_tween_completion --ignored --nocapture --test-threads=1
# benchmark_tween_steady_updates --ignored --nocapture --test-threads=1
# snapshot_tween_completion --ignored --nocapture --test-threads=1
```

Capture snapshot stderr directly to a file and compare its bytes. The snapshot
does not include timing output. Raw logs and saved executables for this run are
also retained locally in the ignored `target/tween-completion-pass` directory.
