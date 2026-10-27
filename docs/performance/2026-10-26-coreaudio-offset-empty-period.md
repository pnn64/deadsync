# CoreAudio offsets, empty queries and cached periods

Baseline: `e3c563d78`. Three measured changes are retained: use a magnitude and
direction for the calibrated clock offset, return empty-query results directly,
and reuse the buffer duration for callbacks of the same size.

| Change and fixture | Before CPU cycles/update | After | CPU throughput |
| --- | ---: | ---: | ---: |
| Clock, positive offset, divisor 1 | 7.381 | 5.711 | 1.292x |
| Clock, negative offset, divisor 1 | 7.387 | 5.822 | 1.269x |
| Clock, positive offset, divisor 3 | 8.932 | 7.272 | 1.228x |
| Clock, negative offset, divisor 3 | 8.954 | 7.256 | 1.234x |
| Clock, positive offset, divisor 7 | 29.722 | 27.455 | 1.083x |
| Clock, negative offset, divisor 7 | 29.965 | 28.303 | 1.059x |
| Empty query, default available | 207.82 | 160.40 | 1.296x |
| Empty query, no default | 200.32 | 20.03 | 10.001x |
| Callback timing, period equals buffer size | 33.121 | 8.441 | 3.924x |

Clock and period comparisons use sixteen alternating before/after process pairs;
empty queries use twelve. These measure extracted production Rust on Windows
x86_64. OS calls, rendering and the real logging backend are excluded. Native Mac
performance and playback remain unverified; no whole-callback speedup is claimed.

The clock state shrinks from **32 to 24 bytes** on the measured target, verified
in every process. Clock and callback timing still allocate nothing. A successful
empty query with a default allocates **4 bytes instead of 48**; without a default
it allocates **zero bytes instead of 48**, eliminating one allocation and free.
Query failures retain their warning and the same result contents.

The tradeoffs remain visible. The variable-period control measures **34.254 ->
35.251 cycles/update**, 2.9% more cycles. The nonempty 11-rate setup fixture
measures **758.11 -> 803.56 cycles/call**, 6.0% more cycles. Other setup controls
range from small regressions to small gains; no general nonempty-query speedup is
claimed. The existing sample-rate processing path remains intact.

## Removed work and exactness

1. Calibration originally stored the difference of two `u64` timestamps in an
   `i128`. Its magnitude can never exceed `u64::MAX`, so the existing clock struct
   now stores `abs_diff` and a direction bit. For a nonnegative offset, clamping
   `timestamp - offset` is exactly `saturating_sub`. For a negative offset,
   clamping `timestamp + magnitude` is exactly `saturating_add`. This removes the
   callback's wide subtraction, wide clamp and narrowing conversion without
   assuming a small offset. Zero timestamps preserve both host-time reads and
   the fallback notification, including trace state.
2. A successful query with no ranges previously allocated capacity for twelve
   rates, added the optional default, ran the common-rate checks, sorted and
   deduplicated. It can only return the default or an empty list. That return is
   now shared with the query-error path, after the unchanged warning. Nonempty
   inputs retain their original validation, tolerance checks, ordering and
   deduplication. Returned vector capacity intentionally changes for empty input.
3. The immutable buffer duration was already computed before callback
   registration. When `period_frames == buffer_frames`, recomputing the period
   divides the same integer product by the same rate. Reusing `buffer_ns` removes
   that division. Other frame counts use the original conversion, including zero
   sample rates, smaller/larger periods and frame-count casts.

Production code changes are 23 added and 11 removed lines. No public interfaces,
dependencies or unsafe code are added. The clock remains the existing private
type; there is no new application abstraction or dynamic allocation.

## Rejected nominal-only shortcuts

An additional query shortcut recognized lists containing only repeated nominal
points. All three versions passed the differential checks and improved the
32-point nominal fixture by about four times, but slowed a single continuous
range. None is retained:

| Predicate implementation | Continuous range, before -> after cycles/call | CPU throughput |
| --- | ---: | ---: |
| Floating comparisons with `all` | 525.64 -> 573.34 | 0.917x |
| Bit comparisons with `all` | 561.90 -> 608.98 | 0.923x |
| Direct loop with early exit | 512.66 -> 555.81 | 0.922x |

The final production function contains only the empty-list shortcut. The three
trial TSVs and source patches are retained to distinguish their results from the
selected implementation.

## Verification and measurement

`benchmark_coreaudio.py --host-clock` additionally extracts the actual clock
struct, calibration field initializer and callback method. Deterministic host
reads and a fallback-notification recorder replace external facilities. The
ordinary timing and setup extraction drivers remain usable.

Nine timing/clock checks and seven setup checks pass in both optimized and debug
builds. Existing timing checks include one million randomized comparisons.
Added coverage checks the clock against the original `i128` formula over 882
explicit boundary combinations and 200,000 random calibrations/timestamps,
including underflow, overflow and either offset direction. It checks both
fallback host reads, the recorded timestamp/trace flag and zero allocation churn.
Eighty-four empty/nominal/mixed-range cases compare output and query counts with
the original rate logic; the prior adversarial float, log and range checks pass.

Both Apple ARM64 and Intel pass `cargo check --all-targets --locked --offline`.
ARM64 Clippy passes with the existing `collapsible_if` warning in untouched device
enumeration. Rust formatting and Python compilation pass.

Host: Intel Xeon E5-2696 v4, Windows 11, rustc 1.99.1 / LLVM 23.1.1, baseline CPU
target, optimization level 3, thin LTO, one codegen unit and static MSVC runtime.
The existing helper pins the benchmark thread to logical processor 2 and uses
`QueryThreadCycleTime`. It discards one warmup and reports the median of eleven
samples; the comparison driver alternates processes and reports medians of
process medians. Allocations, fixtures and compilation are outside CPU timing.
All compilations started by this task had finished before each comparison;
unrelated shared-machine work is uncontrolled.

Clock and timing TSV `cycles_per_op`/`ns_per_op` describe **256-update batches**,
with 1,024 batches per sample. Their table medians above are divided by 256.
Setup TSV values describe one call, with 8,192 calls per ordinary sample and
262,144 calls for the direct endpoint controls. Wall time remains secondary.

Three retained comparisons contain **912 records**, and the three rejected
comparisons contain **1,224 records**. The JSON records all variant patches,
source/executable hashes, compiler commands, check output, state sizes and
summaries. The audit verifies every row's executable identity, identical paired
fixture/support inputs, final source identity and all 32 state-size observations.
Raw sources, executables and logs remain under `target/coreaudio-offset/`,
`target/coreaudio-degenerate-rates/` and `target/coreaudio-period/`.

## Reproduction

Save the baseline `lib.rs` from `e3c563d78`. For the intermediate period baseline,
apply the clock-offset and empty-query changes, leaving period conversion
unchanged; exact patches are in the JSON. Use the committed fixtures unchanged
for both sides of every comparison.

```powershell
python scripts/benchmark_coreaudio.py --host-clock --source PATH_TO_BASELINE --archive target/coreaudio-offset/before
python scripts/benchmark_coreaudio.py --host-clock --archive target/coreaudio-offset/final-after
python scripts/benchmark_coreaudio_setup.py --source PATH_TO_BASELINE --archive target/coreaudio-degenerate-rates/final-before
python scripts/benchmark_coreaudio_setup.py --archive target/coreaudio-degenerate-rates/final-after
python scripts/benchmark_coreaudio.py --source PATH_TO_PERIOD_BASELINE --archive target/coreaudio-period/before
python scripts/benchmark_coreaudio.py --archive target/coreaudio-period/cached
python scripts/benchmark_sampling.py --before target/coreaudio-offset/before/timing.exe --after target/coreaudio-offset/final-after/timing.exe --test-filter benchmark_host_clock --runs 16 --log-dir target/coreaudio-offset/runs --output docs/performance/2026-10-26-coreaudio-offset-magnitude.tsv
python scripts/benchmark_sampling.py --before target/coreaudio-degenerate-rates/final-before/coreaudio.exe --after target/coreaudio-degenerate-rates/final-after/coreaudio.exe --test-filter benchmark_coreaudio_setup --runs 12 --log-dir target/coreaudio-degenerate-rates/runs --output docs/performance/2026-10-26-coreaudio-empty-ranges.tsv
python scripts/benchmark_sampling.py --before target/coreaudio-period/before/timing.exe --after target/coreaudio-period/cached/timing.exe --test-filter benchmark_coreaudio_timing --runs 16 --log-dir target/coreaudio-period/runs --output docs/performance/2026-10-26-coreaudio-cached-period.tsv
python scripts/benchmark_coreaudio.py --host-clock --debug --archive target/coreaudio-offset/debug
python scripts/benchmark_coreaudio_setup.py --debug --archive target/coreaudio-degenerate-rates/debug
```
