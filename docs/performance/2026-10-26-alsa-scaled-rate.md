# Exact cached ALSA frame conversion

This pass retains one optimization: prepare an integer multiplier and shift from
the negotiated sample rate during PCM configuration, then reuse them for
frame-to-nanosecond conversion. This removes division from the timing-query path.
The existing zero-frame/rate behavior and exact truncation are preserved for all
`u32` rates and frame counts. No production unsafe code or dependencies are added.

The private runtime parameters gain two primitive fields. Their nominal size
changes from 24 to 32 bytes on the measured target; `AlsaOutputPrep` remains
72 bytes and `AlsaHostClock` one byte. These are compiler-measured type sizes,
not RSS or a claim about optimized stack placement. Configuration performs the
wide division used to prepare the scale. Its isolated startup cost was not timed.

## Measurements

The final comparison freezes all Rust dependency artifacts, including their
metadata and proc-macro libraries, and verifies their hashes after measurement.
Both test binaries use identical Rust 1.99.1 / LLVM 23.1.1 flags: opt-level 3,
fat LTO, 16 codegen units, disabled overflow/debug assertions and stripped debug
information. Target CPU is the default x86-64 baseline. Tests run under Debian
WSL2 on a Xeon E5-2696 v4 with libasound 1.2.16.1.

Seven alternating runs per variant are pinned to CPU 0. Results are medians;
throughput is before cycles divided by after cycles.

| Workload | Before cycles/op | After cycles/op | CPU throughput |
| --- | ---: | ---: | ---: |
| Convert frames, 48 kHz | 31.27 | 9.41 | 3.323x |
| Convert frames, 44.1 kHz | 31.23 | 9.41 | 3.318x |
| Convert frames, 96 kHz | 31.21 | 9.42 | 3.314x |
| Convert frames, mixed rates | 31.51 | 10.72 | 2.940x |
| Full timing query, 48 kHz | 414.09 | 396.55 | 1.044x |
| Full timing query, 44.1 kHz | 414.67 | 395.84 | 1.048x |
| Full timing query, 96 kHz | 414.45 | 395.88 | 1.047x |
| Status-error fallback | 269.52 | 256.37 | 1.051x |
| Monotonic-clock fallback | 410.24 | 390.89 | 1.050x |
| Empty queue | 399.68 | 393.36 | 1.016x |

Each workload allocates and reallocates zero Rust objects per warmed operation.
No algorithmic improvement is claimed for empty queues: their early return is
unchanged, and other comparisons range from essentially equal to a small gain.
Separate Cargo release comparisons show 2.8–3.0% full-query throughput gains.

Frame inputs vary each iteration; the mixed workload rotates 48/44.1/96/192 kHz.
Scale preparation is outside timing, matching its lifetime in the render loop.
Clock reads and PCM status calls remain real; the shared preload fixture supplies
a 240-frame delay, zero delay, status failure, or rejected raw-clock configuration
as appropriate. Both compared binaries load the same fixture binary.

Hardware userspace cycles/instructions and task-clock cover the entire test
process, divided by its operation count. Loop wall time is recorded separately.
Batches contain 40 million conversions or one million timing queries. Rust
allocation counting covers a separate warmed operation; increments are disabled
during timing while its enabled-state check remains. C allocations are excluded.
These are backend measurements, not physical-driver or whole-game results.

An unrelated audio-core commit landed during exploratory measurements. The final
retention decision uses the frozen-dependency comparison above. Earlier Cargo
and exact-starting-source comparisons remain archived with their identities.
The frozen before source includes the cache-ready call interface but retains
the original integer division. Its exact adapter patch from `02183e7c1` is saved.

## Exactness

Let `n = frames * 1_000_000_000`, so `n < 2^62`. For a rate `r > 1`, prepare:

```text
L = floor(log2(r))
s = L - 1
m = ceil(2^(64+s) / r)
nanoseconds = high64(n * m) >> s
```

Here `0 <= s <= 30`, `m <= 2^63`, and the product is below `2^125`, so every
intermediate fits its Rust integer type. The upward reciprocal error after
multiplication is strictly less than `2^(-L-1)`, which is less than `1/r`.
Fractional quotients are spaced by `1/r`; this error cannot cross the next
integer. Truncating the product therefore gives exactly `floor(n/r)` without
a correction branch. Rates zero and one use explicit original-result paths.
The multiplier and shift are derived together from the same negotiated rate.

Release, debug and frozen-division reference builds pass seven unit checks,
42 fixture scenarios and all 2,048 common-rate support masks. The arithmetic
check covers 1,636,121 rate/frame pairs against the original division formula,
including every rate through 384 kHz, zero/one/maximal values and random full
32-bit inputs. Another 100,729 cases compare clock mapping with a wide-integer
reference. Fixtures exercise both monotonic clocks, raw-clock rejection,
status failures, negative/zero/maximal delays and extreme clock-health history.
Both final fixed-dependency binaries also pass the full fixture set.

Scoped rustfmt, Clippy on all ALSA targets, Python compilation and C compilation
with `-Wall -Wextra -Werror` pass. The ALSA call order, clock sampling, timestamp
sanity policy, routing and sample-rate selection are preserved.

## Rejected changes and evidence

Replacing branch-proven saturating differences produced no measurable gain.
Removing zero-history guards changed cycles by about 1% with unchanged
instruction counts; that was insufficient evidence of reduced work. Storing
clock IDs directly in the enum did not improve clock-read throughput. All
three candidates were reverted.

[Provenance](2026-10-26-alsa-scaled-rate-provenance.json) retains source patches,
test/fixture versions, build identities, dependency hashes, validation logs and
all 802 observations with dataset hashes. The independent
[reference results](2026-10-26-alsa-scaled-rate-reference.json),
[type layouts](2026-10-26-alsa-scaled-rate-layout.json),
[fixed builds](2026-10-26-alsa-scaled-fixed-builds.json) and
[fixed-build checks](2026-10-26-alsa-scaled-fixed-checks.json) retain their commands
and results. Their verification/build scripts are embedded in the provenance.

The primary dataset is the
[frozen-dependency confirmation](2026-10-26-alsa-scaled-fixed-confirm.tsv).
[Cargo confirmation](2026-10-26-alsa-scaled-rate-final.tsv) and
[starting-source confirmation](2026-10-26-alsa-scaled-rate-parent-confirm.tsv)
are retained alongside every pre-edit and rejected-candidate measurement.
Binaries, snapshots and raw subprocess logs remain in `target/alsa-performance`;
the immutable dependency snapshot remains in the Linux target directory.

For another pass, archive the before and after sources under fresh names with
`scripts/benchmark_alsa.py --build NAME`, then compare with `--runs 7` and
`--fixture-stage AFTER`. Add `--debug` to build and verify a debug archive.
