# CoreAudio clock divisors and redundant rate probes

Baseline: `ff728c6a5`. This batch retains three measured optimizations in
`deadlib-audio-backend-coreaudio`: constant clock divisors, one insertion per
discrete endpoint, and skipping the common-rate scan when all common rates are
already present. The float-cast and partial-membership alternatives were rejected.

| Retained change and fixture | Before CPU cycles/update | After | CPU throughput |
| --- | ---: | ---: | ---: |
| Clock divisor 1, fitting product | 30.403 | 7.538 | 4.033x |
| Clock divisor 3, fitting product | 29.144 | 9.597 | 3.037x |
| Clock divisor 1, overflowing product | 44.974 | 8.268 | 5.440x |
| Skip duplicate endpoints, two discrete rates | 511.77 | 471.03 | 1.086x |
| Skip duplicate endpoints, 11 discrete rates | 1,400.10 | 1,060.47 | 1.320x |
| Skip complete common-rate scan, 11 discrete rates | 1,058.01 | 741.22 | 1.427x |

The two rate changes are measured consecutively, with twelve alternating
before/after process pairs each. The clock comparison uses sixteen pairs. A
separate sixteen-pair baseline/final rate comparison gives:

| Rate fixture | Before CPU cycles/call | After | CPU throughput |
| --- | ---: | ---: | ---: |
| One continuous range | 566.16 | 555.88 | 1.018x |
| Two discrete ranges | 531.83 | 485.38 | 1.096x |
| Eleven discrete ranges | 1,441.91 | 764.76 | 1.885x |
| Thirty-two continuous ranges | 3,005.27 | 3,075.95 | 0.977x |

These are Windows x86_64 measurements of extracted production Rust. They exclude
CoreAudio system calls and playback. No native Mac performance or end-to-end
audio speedup is claimed. Allocation counts and capacities are unchanged by this
batch. The 11-rate fixture still performs two allocations totaling 312 bytes,
including the property-query fixture's returned vector; there are no
reallocations. Clock and direct endpoint conversion remain allocation-free.

## Work removed and behavior preserved

1. `scale_mach_time` handles denominator one with `saturating_mul`, which is
   exactly the original 128-bit product clamped to `u64::MAX` after division by
   one. A fitting product with denominator three uses a constant quotient, letting
   LLVM remove hardware division. Overflow with denominator three and all other
   denominators retain the original wide calculation or checked narrow quotient.
   Division by zero still panics, including a zero numerator.
2. Discrete ranges have equal endpoints. Their minimum and maximum previously
   performed the same validation, rounding and insertion twice. Skipping the
   second insertion removes that work and reduces the final sort/dedup input.
   Equal infinities and signed zeros produce no valid endpoint either way;
   NaNs compare unequal and retain the original two calls. Continuous, reversed
   and fractional ranges preserve the original endpoint conversion.
3. When the endpoint/default list already contains every common rate, subsequent
   range probes can only append duplicates. A length check avoids testing this
   shortcut for short lists. Otherwise integer slice membership checks determine
   whether the complete common-rate loop is redundant. The existing final sort
   and dedup remain, preserving all extra endpoints and unsupported-but-explicit
   default rates. Partial lists retain the original tolerance checks.

The tradeoffs are visible. The generic denominator-seven clock control rises
from **29.097 to 32.420 cycles/update**: 11.4% more cycles, or 10.3% less throughput,
from the added dispatch. Its arithmetic remains exact. The overflowing
denominator-three control is essentially flat, **106.224 -> 106.791 cycles**.
The specialized divisor paths are retained for their substantial measured gains;
this is not a claim that every possible timebase is faster. The combined
32-range fixture is 2.4% higher in cycles, while the last isolated shortcut
comparison is within 0.4%; small incidental changes are not claimed as gains.

Production changes are 25 added and 8 removed lines. They add no application
types, dependencies, allocations or unsafe code. The removed work is division,
duplicate endpoint processing, redundant range scans and redundant sort inputs.

## Rejected trials

- Specializing only denominator one gave 4.11x throughput there but regressed the
  divide-by-three fixture. The retained version handles both measured divisors.
- Replacing the endpoint float clamp with Rust's saturating cast and integer
  maximum passed the correctness checks but did not improve the direct endpoint
  benchmarks: throughput was 0.975x and 0.968x. The original clamp is retained.
- Checking integer membership before every common-rate probe improved the
  11-rate fixture by 1.32x but slowed one continuous range to 0.869x and 32 ranges
  to 0.904x. Restricting that check to discrete-only lists still cost about 5.6%
  more cycles in the single-continuous-range fixture. Both variants were
  replaced by the complete-set shortcut, which tests once before the loop.

The rejected variants' raw TSVs and exact source patches are retained as evidence.
Their binaries used an earlier fixture revision; before/after inputs within each
individual comparison are identical. The final retained comparisons were rebuilt
with the same final fixtures and support files on both sides.

## Checks and measurement

The existing source-extraction drivers compile the actual timing functions and
sample-rate function, replacing only OS queries and the log sink. Six normal
timing checks and six setup checks pass in optimized and debug builds. They
include the existing million randomized timing comparisons and adversarial
floating range checks. Added coverage includes:

- Both zero-denominator panic cases, including numerator zero.
- 200,000 random floating endpoint values and explicit rounding/saturation
  boundaries against the original clamp formula, used to gate the rejected cast
  trial and retained for subsequent endpoint tuning.
- Forty-eight complete/missing-common-rate cases with half-Hz endpoints,
  additional continuous endpoints, reversed input order and four defaults.
  These exercise both the new shortcut and the original tolerance fallback.

Both `aarch64-apple-darwin` and `x86_64-apple-darwin` pass
`cargo check --all-targets --locked --offline`. ARM64 Clippy passes with the
existing `collapsible_if` warning in untouched device enumeration. Rust formatting
passes. Native Mac playback remains untested.

Host: Intel Xeon E5-2696 v4, Windows 11, rustc 1.99.1 / LLVM 23.1.1, baseline CPU
target, optimization level 3, thin LTO, one codegen unit, static MSVC runtime. The
existing helper pins the thread to logical processor 2 and reads
`QueryThreadCycleTime`. Each process discards one warmup sample and reports the
median of eleven measured samples. Driver order alternates, and tables use the
median of process medians. Compilation, setup and allocation probes are outside
timed regions. No compilation started by this task overlaps a reported comparison;
unrelated shared-machine work is uncontrolled.

Timing TSV values describe a **batch of 256 updates**; their cycle medians are
divided by 256 above. Each timing sample runs 1,024 such batches. Setup TSV values
describe **one call**; ordinary samples run 8,192 calls, and direct endpoint
controls run 262,144 calls. Wall time is secondary and remains in the raw files.

Four retained-comparison TSVs contain **1,408 records**, and four rejected-trial
TSVs contain **1,088 records**. `2026-10-26-coreaudio-rate-scans.json` records
executable/source hashes, compiler commands, exact variant patches, check output,
all measurement summaries and retained/rejected status. The audit verifies every
row's executable identity and identical paired fixture/support inputs. Source
archives, generated harnesses, executables and logs remain under
`target/coreaudio-clock-integer/` and `target/coreaudio-rates/`.

## Reproduction

Save `lib.rs` from `ff728c6a5` as the baseline. The intermediate discrete stage
adds the constant-divisor change and duplicate-endpoint check; the timing helpers
are not part of the setup benchmark. Exact stage patches are in the JSON. Keep
the final committed fixtures and support files identical across compared builds.

```powershell
python scripts/benchmark_coreaudio.py --source PATH_TO_BASELINE --archive target/coreaudio-clock-integer/final-before
python scripts/benchmark_coreaudio.py --archive target/coreaudio-clock-integer/final-after
python scripts/benchmark_coreaudio_setup.py --source PATH_TO_BASELINE --archive target/coreaudio-rates/final-before
python scripts/benchmark_coreaudio_setup.py --source PATH_TO_DISCRETE_STAGE --archive target/coreaudio-rates/final-discrete
python scripts/benchmark_coreaudio_setup.py --archive target/coreaudio-rates/final-after
python scripts/benchmark_coreaudio.py --debug --archive target/coreaudio-clock-integer/debug
python scripts/benchmark_coreaudio_setup.py --debug --archive target/coreaudio-rates/debug
python scripts/benchmark_sampling.py --before target/coreaudio-clock-integer/final-before/timing.exe --after target/coreaudio-clock-integer/final-after/timing.exe --test-filter benchmark_coreaudio_timing --runs 16 --log-dir target/coreaudio-clock-integer/runs --output docs/performance/2026-10-26-coreaudio-small-timebases.tsv
python scripts/benchmark_sampling.py --before target/coreaudio-rates/final-before/coreaudio.exe --after target/coreaudio-rates/final-discrete/coreaudio.exe --test-filter benchmark_coreaudio_setup --runs 12 --log-dir target/coreaudio-rates/runs --output docs/performance/2026-10-26-coreaudio-discrete-endpoints.tsv
python scripts/benchmark_sampling.py --before target/coreaudio-rates/final-discrete/coreaudio.exe --after target/coreaudio-rates/final-after/coreaudio.exe --test-filter benchmark_coreaudio_setup --runs 12 --log-dir target/coreaudio-rates/runs --output docs/performance/2026-10-26-coreaudio-complete-rates.tsv
python scripts/benchmark_sampling.py --before target/coreaudio-rates/final-before/coreaudio.exe --after target/coreaudio-rates/final-after/coreaudio.exe --test-filter benchmark_coreaudio_setup --runs 16 --log-dir target/coreaudio-rates/runs --output docs/performance/2026-10-26-coreaudio-rate-scans-final.tsv
```
