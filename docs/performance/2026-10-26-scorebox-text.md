# Scorebox decimal/rank encoding and username matching

Baseline runtime source: `48a969486`; measured on 2026-10-26. Three changes improve common score-value formatting **3.786x**, small rank formatting **1.678x**, and username mismatches **2.406x**. Composed row text improves **2.724x**, with 45.9% fewer requested heap bytes. Confirmed username matches are slower (0.941x throughput); the complete matrix and limitations follow. These are warmed text-processing kernels and composed operations, with no claim of whole-screen, network or gameplay latency gains.

## Retained changes

1. Both fixed-two-decimal score formatters share a bounded binary64-to-decimal renderer. The existing `scorebox_score_percent` still performs its original division, finite check and clamp. The new helper rounds the resulting binary fraction exactly using integer arithmetic, writes ASCII into an eight-byte stack buffer and allocates only the final text. It bypasses general float formatting while preserving standard-format ties to even, signed zero, trailing zeros, clamp boundaries and nonfinite handling.
2. `format_scorebox_rank` writes base-100 digit pairs backward into an eleven-byte buffer, including the trailing dot. The buffer fits every `u32`, and the resulting string allocates its exact length. This removes general integer-format dispatch and growth for wide ranks. A single 200-byte private digit-pair table serves both numeric paths.
3. `leaderboard_username_matches` checks ASCII-insensitive equality before testing whether the username trims to empty. The two pure predicates are unchanged; reordering avoids Unicode whitespace scans on mismatches. Raw username whitespace is still compared as before, and equal whitespace-only strings still fail.

The production diff has **74 inserted / four removed lines**, covering four API bodies plus one private helper and one constant. The extra code is bounded numeric conversion, rather than an abstraction layer. It removes three general `format!` paths and repeated mismatch trimming. There is no new dependency, public type or unsafe Rust.

## Exact rounding argument

Multiplying the normalized `f64` by 100 and rounding that new float can round twice. The implementation instead uses the exact stored binary value **after the unchanged normalization**.

For a normal nonnegative binary64 value, let `E` be its biased exponent and `M` its 53-bit significand. Its value is `M / 2^(1075-E)`, because 1075 = 1023 + 52. With `d = 1075-E`, the exact number of displayed hundredths is `S / 2^d`, where `S = 100*M`. The normalized magnitude is at most 100, so `d >= 46`; `S < 2^60` fits in `u64`.

If `d >= 61`, even the largest `S` is below half an output unit and rounds to zero. This also covers zero and all subnormals without needing their hidden-significand special case. Otherwise `46 <= d <= 60`, so the quotient, remainder, and half threshold are exact, defined shifts. Increment the quotient only when the remainder exceeds half, or equals half with an odd quotient. That is nearest rounding with ties to even, matching Rust's fixed-decimal formatting. The normalized sign bit is retained separately for negative zero. The result is in 0..=10000 and renders as a whole part in 0..=100 plus two fractional digits, with an optional percent sign. All buffers and table indices are bounded by those ranges.

The local Rust 1.99.1 standard-library source (`core/src/num/imp/flt2dec/strategy/dragon.rs`) was inspected for its exact-format tie rule. Independent frozen standard-format references and extensive boundary/bit-pattern checks verify the complete public conversion, including normalization effects around half cents. The renderer uses no approximation or epsilon adjustment.

The rank encoder follows ordinary base-100 quotient/remainder decomposition. `u32::MAX` has ten decimal digits; four loop iterations plus the leading one/two digits and dot fit the eleven-byte buffer. Both numeric paths validate their generated ASCII through safe `str::from_utf8` before creating the owned string.

## Consecutive before/after measurements

Each step uses eight fresh processes per version, alternating execution order. Values are median cycles per complete fixture. CPU throughput is median-before / median-after; ranges contain every paired process ratio.

### Decimals step

| Fixture | Before cycles | After cycles | CPU throughput | Paired range |
| --- | ---: | ---: | ---: | ---: |
| `value/common` | 333,031.22 | 86,837.98 | 3.835x | 3.319-4.213x |
| `percent/common` | 340,158.12 | 87,308.70 | 3.896x | 3.654-4.386x |
| `value/special` | 179,329.78 | 96,995.85 | 1.849x | 1.772-2.013x |
| `percent/special` | 186,226.89 | 97,458.00 | 1.911x | 1.623-2.089x |
| `value/boundary` | 401,041.93 | 85,513.25 | 4.690x | 4.318-5.378x |

### Ranks step

| Fixture | Before cycles | After cycles | CPU throughput | Paired range |
| --- | ---: | ---: | ---: | ---: |
| `rank/small` | 107,455.53 | 64,612.74 | 1.663x | 1.504-1.788x |
| `rank/large` | 198,324.25 | 72,127.18 | 2.750x | 2.300-2.852x |
| `rank/mixed` | 125,744.36 | 71,704.48 | 1.754x | 1.513-1.791x |

### Names step

| Fixture | Before cycles | After cycles | CPU throughput | Paired range |
| --- | ---: | ---: | ---: | ---: |
| `name/miss` | 8,237.25 | 3,563.44 | 2.312x | 1.910-2.568x |
| `name/match` | 23,239.57 | 25,175.62 | 0.923x | 0.763-0.968x |
| `name/blank` | 7,463.85 | 1,190.10 | 6.272x | 4.970-6.465x |
| `name/unicode` | 11,406.28 | 8,702.83 | 1.311x | 1.033-1.370x |
| `find/late` | 2,074.10 | 841.99 | 2.463x | 1.915-2.701x |
| `find/miss` | 1,563.94 | 355.84 | 4.395x | 3.434-5.529x |

Raw data retain every workload and control: [decimal rendering](2026-10-26-scorebox-decimals.tsv), [rank rendering](2026-10-26-scorebox-ranks.tsv), and [name predicates](2026-10-26-scorebox-names.tsv).

The numeric stages improve all of their measured targets. The name-only stage improves mismatches 2.312x and late/missing lookups 2.463x/4.395x while confirmed matches measure 0.923x. Its composed row fixture improves 1.074x. Unchanged routines vary too; the data do not establish the cause of every build/process shift.

## Combined original/final measurements

Twelve fresh processes per version compare all three changes against the baseline. [Complete data](2026-10-26-scorebox-final.tsv).

| Fixture | Before cycles | After cycles | CPU throughput | Before -> after ns | Paired range |
| --- | ---: | ---: | ---: | ---: | ---: |
| `value/common` | 331,066.03 | 87,453.61 | 3.786x | 150,989.07 -> 39,864.85 | 3.561-3.872x |
| `percent/common` | 339,338.42 | 88,269.01 | 3.844x | 154,743.75 -> 40,269.15 | 3.359-3.908x |
| `value/special` | 179,605.02 | 96,828.65 | 1.855x | 81,922.26 -> 44,161.33 | 1.816-2.049x |
| `percent/special` | 185,435.48 | 98,086.49 | 1.891x | 84,587.11 -> 44,738.28 | 1.788-2.022x |
| `value/boundary` | 400,526.63 | 89,542.29 | 4.473x | 182,692.19 -> 40,867.97 | 4.293-4.749x |
| `rank/small` | 107,790.79 | 64,244.05 | 1.678x | 49,146.10 -> 29,293.36 | 1.421-1.741x |
| `rank/large` | 200,103.40 | 68,951.29 | 2.902x | 91,259.38 -> 31,429.29 | 2.634-3.127x |
| `rank/mixed` | 127,347.74 | 70,107.11 | 1.816x | 58,079.69 -> 31,959.76 | 1.739-1.845x |
| `name/miss` | 8,192.67 | 3,405.68 | 2.406x | 3,726.56 -> 1,550.39 | 2.235-2.635x |
| `name/match` | 23,085.22 | 24,533.42 | 0.941x | 10,517.19 -> 11,171.88 | 0.887-1.024x |
| `name/blank` | 7,426.99 | 1,170.38 | 6.346x | 3,400.78 -> 529.30 | 5.037-7.567x |
| `name/unicode` | 10,621.74 | 8,546.77 | 1.243x | 4,850.39 -> 3,889.06 | 1.186-1.342x |
| `find/late` | 2,078.39 | 836.84 | 2.484x | 942.19 -> 376.95 | 2.318-3.069x |
| `find/miss` | 1,642.82 | 345.54 | 4.754x | 744.14 -> 151.95 | 4.356-5.843x |
| `compose/row` | 121,588.43 | 44,628.81 | 2.724x | 55,600.78 -> 20,350.00 | 2.490-3.111x |
| `control/tag` | 81,941.24 | 84,764.73 | 0.967x | 37,371.49 -> 38,697.26 | 0.900-1.050x |

All numeric targets, mismatches, blank/unicode name fixtures, finder kernels and composed rows have paired ranges above 1x. The large-rank fixture near `u32::MAX` is a bound stress, not a typical displayed rank. The 4.473x boundary result uses neighboring half-cent values; common value/percent formatting improves 3.786x/3.844x.

Confirmed matches have a real slower median: **6.3% more cycles**, or 0.941x throughput. Both predicates must still run on matches. The unchanged tag control also measures 0.967x (3.4% more cycles), with a paired range spanning 1x. Neither result is omitted. The reordered predicate is retained for the stronger mismatch/search results and composed-row improvement; it is not universally faster for every input or calling context.

## Allocations

The existing scoped System-allocator wrapper probes one complete fixture independently of timing. Counts are identical across every process run for each version/workload. Allocations equal frees, and requested bytes equal freed bytes, within each fixture; each corresponding table cell gives that common count.

| Fixture | Allocations/frees before -> after | Reallocations before -> after | Requested/freed bytes before -> after | Scoped peak bytes before -> after |
| --- | ---: | ---: | ---: | ---: |
| `value/common` | 512 -> 512 | 0 -> 0 | 4,096 -> 2,560 | 8 -> 5 |
| `percent/common` | 512 -> 512 | 0 -> 0 | 4,096 -> 3,072 | 8 -> 6 |
| `value/special` | 512 -> 512 | 0 -> 0 | 4,096 -> 2,355 | 8 -> 6 |
| `percent/special` | 512 -> 512 | 0 -> 0 | 4,096 -> 2,867 | 8 -> 7 |
| `value/boundary` | 512 -> 512 | 0 -> 0 | 4,096 -> 2,048 | 8 -> 4 |
| `rank/small` | 512 -> 512 | 0 -> 0 | 4,096 -> 1,938 | 8 -> 4 |
| `rank/large` | 512 -> 512 | 512 -> 0 | 15,360 -> 5,632 | 20 -> 11 |
| `rank/mixed` | 512 -> 512 | 56 -> 0 | 5,328 -> 2,269 | 20 -> 11 |
| `compose/row` | 256 -> 256 | 0 -> 0 | 2,048 -> 1,108 | 16 -> 10 |
| `control/tag` | 512 -> 512 | 0 -> 0 | 8,192 -> 8,192 | 16 -> 16 |

All name and finder workloads retain zero allocations, reallocations, frees and requested/freed/peak bytes. Numeric outputs remain owned strings, so ordinary formatting retains one allocation per result. Exact-sized output reduces requested bytes and scoped live requests. The large-rank fixture removes all 512 reallocations; the mixed rank fixture removes 56. Composed rows retain 256 allocations/frees but requested bytes fall from 2,048 to 1,108 (45.9%) and scoped peak falls from 16 to 10. These are requested bytes, rather than allocator size classes or process RSS.

## Method, checks and limits

Windows x86_64 MSVC, Intel Xeon E5-2696 v4, rustc 1.99.1 / LLVM 23.1.1. Archived integration binaries use `--release --locked`, repository-default LTO, the baseline CPU target and static MSVC runtime settings. The existing helper pins the test thread to logical processor 2 and reads `QueryThreadCycleTime`. Each process discards one warmup sample and reports the median of eleven measured samples, each repeating its complete fixture **128 times**. The driver alternates process order and reports medians of process medians. Four TSVs retain all **1,152 process/workload records**, elapsed time, allocation counters and executable SHA-256 values. No compilation started by this task overlaps retained comparisons; other shared-machine activity is uncontrolled.

Inputs/results use `black_box` and fixture construction is outside timing. Numeric/name fixtures make 512 calls. Common raw scores range from 9876 through 9925.5; special inputs cycle zeros, nonfinite values, negative/over-limit values and signed minimum subnormals. Boundary inputs use the immediate raw-bit neighbors of `i + 0.5`. Rank fixtures use 0..511, the last 512 `u32` values, and mixed decimal boundaries. Name fixtures cover mismatches, matches with differing ASCII case, blank names and Unicode cases. Finder kernels search 128 strings with the match at the end or absent. They blackbox individual comparisons, so they do not claim identical optimization behavior in every application loop. A separate unbarriered-loop screening also showed improvement but is not part of the retained production tables. Composed rows create 128 rank/score/self-match tuples, alternating percent and plain score labels. The existing Unicode tag formatter is the control.

All measured binaries use the same scoped System-allocator wrapper. Tracking is enabled only for the untimed counter probe, then disabled during timing; allocator calls still pass through that wrapper.

The floating-point check compares both public formats with frozen standard-format bodies on **over 1.36 million raw inputs**: both signs, every exponent, representative significand extremes, signed zeros, nonfinite encodings, seventeen raw-bit neighbors of every integer/half unit across the score range, and one million deterministic random/uniform-range values. Rank checks cover the entire 0..999999 domain, decimal-power boundaries, 100,000 generated full-width values and the last 1,025 `u32` values. The third check compares composed outputs and exact name-search positions, including raw whitespace, controls, Unicode and case differences. The public normalization body is unchanged and verified by the full-file source audit.

Validation passes: **1,069 release tests** across score/online/profile (39 ignored), **830 debug tests** across score/online (35 ignored), scoped rustfmt for all new runtime code and both new Rust files, and score/online Clippy with `--no-deps`. Each default-LTO measurement archive passes all three new correctness tests. Clippy retains existing warnings and one test-only `needless_range_loop` warning in the fixed-domain composed-row check; it reports no diagnostics in changed runtime code.

The dependency-inclusive score/online Clippy run stops at pre-existing `clippy::while_immutable_condition` in [cmd.rs](../../crates/deadsync-song-lua/src/cmd.rs), line 349. That file exactly matches baseline `48a969486`. It was left untouched. Score-only Clippy and score/online `--no-deps` both pass; the latter checks the requested crates while excluding dependency lint failures. Broad release/debug tests still compile and exercise the dependency graph.

Source audits verify the four frozen API bodies, reconstruct the entire starting source by reversing only those bodies and removing the helper/table, verify fixed harness/reference/shared-helper sources, compare final production bytes with the measured snapshot and check every raw record's executable digest. Build configuration matches the baseline. Sources, screening tests, logs and executable archives remain under `target/scorebox-text/`.

```powershell
cargo test --release --locked -p deadsync-score --test scorebox_text --no-run --message-format=json
python scripts/benchmark_sampling.py --runs 8 --test-filter benchmark_scorebox_text --log-dir target/scorebox-text/runs --before target/scorebox-text/before/score.exe --after target/scorebox-text/decimals/score.exe --output docs/performance/2026-10-26-scorebox-decimals.tsv
python scripts/benchmark_sampling.py --runs 8 --test-filter benchmark_scorebox_text --log-dir target/scorebox-text/runs --before target/scorebox-text/decimals/score.exe --after target/scorebox-text/ranks/score.exe --output docs/performance/2026-10-26-scorebox-ranks.tsv
python scripts/benchmark_sampling.py --runs 8 --test-filter benchmark_scorebox_text --log-dir target/scorebox-text/runs --before target/scorebox-text/ranks/score.exe --after target/scorebox-text/after/score.exe --output docs/performance/2026-10-26-scorebox-names.tsv
python scripts/benchmark_sampling.py --runs 12 --test-filter benchmark_scorebox_text --log-dir target/scorebox-text/runs --before target/scorebox-text/before/score.exe --after target/scorebox-text/after/score.exe --output docs/performance/2026-10-26-scorebox-final.tsv
cargo test --release --locked --config profile.release.lto=false -p deadsync-score -p deadsync-online -p deadsync-profile -- --test-threads=1
cargo test --locked -p deadsync-score -p deadsync-online -- --test-threads=1
cargo clippy --release --locked --config profile.release.lto=false -p deadsync-score --all-targets
cargo clippy --no-deps --release --locked --config profile.release.lto=false -p deadsync-score -p deadsync-online --all-targets
```

Keep the committed integration harness fixed. Restore the four API bodies from `48a969486` and remove the private helper/table for `before`. Apply the decimal helper, table and two score wrappers for `decimals`; apply rank encoding for `ranks`; apply the predicate reorder for `after`. Build/archive the single reported integration executable and run its three correctness checks at each stage. Consecutive comparisons use eight paired runs; original/final uses twelve. Broader validation disables release LTO to reduce linking time; all performance archives retain default LTO.
