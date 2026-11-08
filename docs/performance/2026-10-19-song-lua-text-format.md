# Song Lua time, ordinal and color-hex formatting

This pass starts from `95f55da32`, following the [borrowed-argument and version-check pass](2026-10-19-song-lua-compat-args.md). It makes three optimizations guided by M-HOTPATH in [rust-performance.md](../../rust-performance.md):

1. Format time components into stack integer buffers, compute the exact output size, and append the original padded components to one Rust String. The five public time helpers continue to use the unchanged time-parts calculation.
2. Format an ordinal's integer into a stack buffer and reserve exactly its digits plus the existing suffix. Suffix selection is unchanged, including negative values and teen exceptions. Large ordinals avoid the former String resizes.
3. Write the four RGBA bytes through an uppercase hex lookup table, then consume the eight-byte output using safe `String::from_utf8`. The original clamp, multiplication and truncating cast are unchanged. Output remains eight digits in RGBA order, without a prefix.

The first two changes use itoa's safe Buffer API. The crate adds a direct `itoa = "1.0.18"` dependency on the package already present transitively in Cargo.lock. The lockfile adds only that dependency edge; no package versions or checksums change. No unsafe code is added to this crate. Existing callbacks, color input parsing, time arithmetic and fixtures are unchanged.

## Measurements

The [raw results and validation receipt](2026-10-19-song-lua-text-format.json) retain six alternating-order rounds and 54 samples per variant/workload: 3,132 retained samples across 29 workloads. Tables show medians before → after. Both variants run in one release executable and allocator. The frozen baseline contains the original time, ordinal and hex function bodies from `95f55da32`, with must_use attributes omitted. The direct rows include function-pointer dispatch, String construction and destruction. Callback rows use the production helper/output-conversion patterns and include returned Lua handle drop.

Ordinary time input is 65.75 seconds:

| Case | CPU cycles/call | Allocations/call | Reallocations/call | Requested bytes/call | Calls/second | Throughput change |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| HH:MM:SS | 570.17 → 186.34 | 1.00 → 1.00 | 0.00 → 0.00 | 8 → 8 | 3,840,614 → 11,693,855 | +204.5% |
| M:SS | 487.69 → 184.93 | 1.00 → 1.00 | 0.00 → 0.00 | 8 → 4 | 4,478,180 → 11,799,410 | +163.5% |
| MM:SS | 453.64 → 185.39 | 1.00 → 1.00 | 0.00 → 0.00 | 8 → 5 | 4,830,218 → 11,755,716 | +143.4% |
| M:SS.cc | 573.31 → 195.48 | 1.00 → 1.00 | 0.00 → 0.00 | 8 → 7 | 3,823,068 → 11,180,680 | +192.5% |
| MM:SS.cc | 574.85 → 198.52 | 1.00 → 1.00 | 0.00 → 0.00 | 8 → 8 | 3,811,484 → 11,015,642 | +189.0% |

The outputs are `00:01:05`, `1:05`, `01:05`, `1:05.75` and `01:05.75`. The M:SS.cc helper changes throughput by +192.5% and cycles by -65.9%, with per-round throughput changes from +171.0% to +210.5%. Every helper still allocates one String; exact reservation reduces requested bytes for shorter forms.

Large time input is 1e12 seconds:

| Case | CPU cycles/call | Allocations/call | Reallocations/call | Requested bytes/call | Calls/second | Throughput change |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| HH:MM:SS | 639.91 → 195.53 | 1.00 → 1.00 | 1.00 → 0.00 | 27 → 15 | 3,423,954 → 11,087,704 | +223.8% |
| M:SS | 563.96 → 188.87 | 1.00 → 1.00 | 1.00 → 0.00 | 33 → 14 | 3,885,004 → 11,564,705 | +197.7% |
| MM:SS | 544.48 → 194.65 | 1.00 → 1.00 | 1.00 → 0.00 | 33 → 14 | 3,984,461 → 11,261,895 | +182.6% |
| M:SS.cc | 790.36 → 209.45 | 1.00 → 1.00 | 1.00 → 0.00 | 33 → 17 | 2,769,201 → 10,153,315 | +266.7% |
| MM:SS.cc | 790.87 → 208.61 | 1.00 → 1.00 | 1.00 → 0.00 | 33 → 17 | 2,772,080 → 10,472,849 | +277.8% |

The baseline performs one resize for each large output. Exact output reservation removes those reallocations and reduces requested bytes. This input tests expansion beyond normal clock widths; it is not a claim about production song duration or input frequency.

Negative time input is -65.75 seconds:

| Case | CPU cycles/call | Allocations/call | Reallocations/call | Requested bytes/call | Calls/second | Throughput change |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| HH:MM:SS | 565.03 → 182.09 | 1.00 → 1.00 | 0.00 → 0.00 | 8 → 8 | 3,846,672 → 12,014,898 | +212.3% |
| M:SS | 359.12 → 186.15 | 1.00 → 1.00 | 0.00 → 0.00 | 8 → 5 | 6,081,061 → 11,774,402 | +93.6% |
| MM:SS | 356.73 → 180.53 | 1.00 → 1.00 | 0.00 → 0.00 | 8 → 5 | 6,091,989 → 12,095,555 | +98.5% |
| M:SS.cc | 574.63 → 204.13 | 1.00 → 1.00 | 0.00 → 0.00 | 8 → 8 | 3,789,386 → 10,724,436 | +183.0% |
| MM:SS.cc | 571.31 → 203.54 | 1.00 → 1.00 | 0.00 → 0.00 | 8 → 8 | 3,836,268 → 10,740,562 | +180.0% |

Signed component display, field widths and centisecond clamping are preserved. For example, M:SS.cc remains `-1:-5.00`. The existing time-parts helper retains its build-profile-dependent integer overflow behavior for extreme time inputs. Extreme-time differential checks run under the release profile used here; the arithmetic is unchanged.

| Case | CPU cycles/call | Allocations/call | Reallocations/call | Requested bytes/call | Calls/second | Throughput change |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| HH:MM:SS Lua callback | 1,328.25 → 909.99 | 1.00 → 1.00 | 0.00 → 0.00 | 8 → 8 | 1,616,750 → 2,404,713 | +48.7% |
| M:SS Lua callback | 1,206.92 → 875.75 | 1.00 → 1.00 | 0.00 → 0.00 | 8 → 4 | 1,804,728 → 2,492,367 | +38.1% |
| MM:SS Lua callback | 1,225.52 → 882.77 | 1.00 → 1.00 | 0.00 → 0.00 | 8 → 5 | 1,761,494 → 2,419,404 | +37.3% |
| M:SS.cc Lua callback | 1,300.15 → 902.53 | 1.00 → 1.00 | 0.00 → 0.00 | 8 → 7 | 1,661,337 → 2,412,254 | +45.2% |
| MM:SS.cc Lua callback | 1,336.76 → 892.60 | 1.00 → 1.00 | 0.00 → 0.00 | 8 → 8 | 1,636,661 → 2,453,837 | +49.9% |

Time callbacks take f64 input, call the selected helper and create the returned Lua string using the existing host-binding pattern. All use 65.75 seconds. The M:SS.cc callback changes throughput by +45.2% and cycles by -30.6%, with per-round throughput changes from +34.3% to +50.0%.

| Case | CPU cycles/call | Allocations/call | Reallocations/call | Requested bytes/call | Calls/second | Throughput change |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Ordinal 1 | 275.02 → 139.24 | 1.00 → 1.00 | 0.00 → 0.00 | 8 → 3 | 7,959,565 → 15,648,228 | +96.6% |
| Ordinal 12345678 | 461.36 → 144.64 | 1.00 → 1.00 | 1.00 → 0.00 | 24 → 10 | 4,677,487 → 15,096,618 | +222.8% |
| Ordinal i64::MIN | 655.25 → 153.32 | 1.00 → 1.00 | 2.00 → 0.00 | 68 → 22 | 3,337,004 → 14,238,929 | +326.7% |
| Ordinal i64::MAX | 482.47 → 152.85 | 1.00 → 1.00 | 1.00 → 0.00 | 57 → 21 | 4,538,235 → 14,313,319 | +215.4% |
| Ordinal 1 through Lua | 995.93 → 815.11 | 1.00 → 1.00 | 0.00 → 0.00 | 8 → 3 | 2,199,373 → 2,678,990 | +21.8% |
| Ordinal 12345678 through Lua | 1,218.99 → 834.48 | 1.00 → 1.00 | 1.00 → 0.00 | 24 → 10 | 1,795,735 → 2,616,945 | +45.7% |

The small ordinal changes helper throughput by +96.6%. An eight-digit value changes throughput by +222.8% and cycles by -68.6%, while removing one resize. The minimum signed integer retains its exact decimal text and `th` suffix while removing two resizes. Its requested bytes change from 68 to 22. All new ordinal outputs use one allocation of exactly the output length. The ordinary callback changes throughput by +21.8%; the eight-digit callback changes by +45.7%. Integer extremes are measured through the Rust API. The existing Lua 5.1 numeric argument conversion rejects the rounded representation of i64::MAX before the formatter runs; callback contract checks retain that error.

| Case | CPU cycles/call | Allocations/call | Reallocations/call | Requested bytes/call | Calls/second | Throughput change |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| RGBA .25/.5/.75/1 | 389.79 → 146.59 | 1.00 → 1.00 | 0.00 → 0.00 | 8 → 8 | 5,583,629 → 14,879,845 | +166.5% |
| NaN/+infinity/-infinity/-zero | 553.58 → 152.50 | 1.00 → 1.00 | 0.00 → 0.00 | 8 → 8 | 3,958,123 → 14,307,175 | +261.5% |
| ColorToHex callback, numeric table | 2,165.31 → 1,784.26 | 3.00 → 3.00 | 0.00 → 0.00 | 64 → 64 | 1,008,522 → 1,226,505 | +21.6% |

Ordinary RGBA input produces `3F7FBFFF`; the nonfinite input produces `00FF0000`. Both versions keep one eight-byte Rust allocation. Ordinary helper throughput changes by +166.5%, with cycles reduced by 62.4%; per-round throughput changes range from +141.2% to +169.3%. The callback includes the unchanged numeric-table color parser, MultiValue conversion and String-to-Lua conversion. Its throughput changes by +21.6% and cycles by -17.6%. Its allocation traffic remains identical, including the parser/callback overhead.

Negative aggregated throughput changes: None in the aggregated medians.

Every sample warms its exact helper/callback path before timing. VM/fixture creation and final VM teardown are excluded. Temporary Rust String destruction and returned-handle drops are included; any Lua collection during calls is timed. These results describe the tested helpers and warmed callbacks, without a whole-song initialization or gameplay-FPS claim. One allocation per formatter remains: the changes reduce formatting work, requested capacity and resizes rather than eliminating the returned String.

## Behavior and validation

Four new release checks pass. Time comparison covers all five formats over 8,193 signed fractional inputs, explicit second/minute/hour boundaries, negative zero, NaN, release infinities/extreme finite values and 1,024 generated f64 bit patterns per format. Explicit expected strings retain padding and negative-component behavior. Ordinal checks cover 8,193 consecutive signed values, teen and negative suffix rules, decimal digit-length boundaries and both i64 extremes. RGBA checks compare all 20,736 combinations of twelve representative components, adjacent f32 bit patterns around every byte threshold in all four channels, and explicit clamp/truncation/alpha/nonfinite expectations. Allocation checks verify one exact-sized output allocation, no new reallocations and the removed ordinal resizes. Lua callback comparisons retain successful values, numeric-string coercion, missing/invalid argument diagnostics, color fallbacks and the i64::MAX conversion error.

The default-feature release library suite has 733 passes, 13 failures and fourteen ignored performance benchmarks. All thirteen failures match the preceding commit's normalized diagnostics, and the retained baseline log hash matches its committed receipt. All 12 prior value/argument checks, 23 draw checks and 7 state-reader checks pass. The JSON retains individual failure/diagnostic hashes, source/executable hashes and the build-receipt hash. Normalization removes Rust source locations, workspace prefixes, temporary process/thread identifiers and the backtrace hint. Source verification checks every frozen body against `git show 95f55da32`, and confirms unchanged time arithmetic, suffix selection, RGBA conversion, existing tests, callbacks and embedded Lua. Manifest/lockfile verification confirms that the only dependency change is the direct edge to the existing itoa package. Fixture data, tolerances and acceptance criteria are unchanged.

## Reproduction

```powershell
cargo test -p deadsync-song-lua --lib --release --config profile.release.lto=false --no-run --message-format=json
python scripts/benchmark_song_lua_draw_capture.py --binary <test-executable> --baseline-commit 95f55da32 --test-filter tables::text_perf::benchmark_text_format --out docs/performance/2026-10-19-song-lua-text-format.json
```

Use the new executable path emitted by Cargo; the added direct dependency changes its artifact identity. Run it with `--test-threads=1` for the full suite. Each round has one warmup plus nine retained samples per variant/workload, pinned to logical CPU 2. Every sample contains 10,000 direct formatter calls or 2,000 Lua callbacks. Windows, Xeon E5-2696 v4 (44 logical CPUs), rustc 1.98.1, release optimization with LTO disabled. Both variants use the counted System allocator. No builds or other benchmarks overlap measurements.

QueryThreadCycleTime measures scheduled current-thread CPU cycles including kernel execution, not a user-only hardware PMU counter. Allocation accounting covers current-thread Rust GlobalAlloc, including mlua, and excludes direct native malloc. Accounting overhead is included. Requested bytes describe allocation/reallocation traffic rather than RSS; raw samples retain frees.
