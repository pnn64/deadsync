# Song Lua operator names, option-array capacity and availability inputs

This pass starts from `af4d42368`, following the [query-encoding pass](2026-10-19-song-lua-query-encode.md). It makes three optimizations guided by M-HOTPATH, M-INITIAL-CAPACITY and M-MEM-REUSE in [rust-performance.md](../../rust-performance.md):

1. Reuse the existing 64-byte stack folding helper for operator option names. The match arms move to a private normalized-key function with unchanged row names, choices, values, flags and preference keys. Longer names retain the existing heap-folding fallback and unknown rows retain their original spelling.
2. Reserve dense Lua option arrays before populating them. Empty arrays retain zero capacity; nonempty arrays reserve the next power of two, matching Lua 5.1's previous dense insertion growth. This removes growth reallocations while preserving the final table shape and length searches after authored code creates holes.
3. Borrow valid UTF-8 names in DoesThemeExist, IsThemeSelectable and DoesLanguageExist. Theme matching keeps its ASCII-insensitive comparison. Language matching directly compares en/english with the same ASCII case rules, removing the owned input and lowercase copies.

No dependency, unsafe code, mutable cache or public API is added. Scalar conversion rules, whitespace, invalid UTF-8 rejection, receiver dispatch, row callbacks and preference behavior remain unchanged. Array entries remain one-based, dense and nonnil for every supported primitive slice, including Boolean false. Strings are still copied/interned by the existing Lua conversion when inserted into arrays.

## Measurements

The [raw measurements and validation receipt](2026-10-19-song-lua-option-inputs.json) retain six alternating-order rounds and 54 samples per variant/workload: 2,700 retained samples across 25 workloads. Tables show medians before → after. Both variants run in one release executable with the counted System allocator. The baseline freezes option_rows.rs from `af4d42368`, with parent visibility added to two private helpers. Its theme snapshot freezes the three availability registrations. During fixture setup, baseline registrations replace those three methods on an otherwise complete current theme table; source verification proves the rest of that constructor is unchanged. Garbage collection runs before warming, removing replaced callbacks and the temporary registration table. Timed availability calls therefore use the frozen old bodies or actual current production callbacks.

| Case | CPU cycles/call | Allocations/call | Reallocations/call | Requested bytes/call | Calls/second | Throughput change |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Theme | 479.63 → 361.52 | 3.00 → 2.00 | 0.0000 → 0.0000 | 15.00 → 10.00 | 4,567,983 → 6,055,101 | +32.6% |
| CustomSongsMaxMegabytes | 502.13 → 390.13 | 3.00 → 2.00 | 0.0000 → 0.0000 | 69.00 → 46.00 | 4,361,194 → 5,609,469 | +28.6% |
| LongAndMarathonTime / mArAtHoN | 832.38 → 721.39 | 3.00 → 2.00 | 1.0000 → 1.0000 | 65.00 → 46.00 | 2,633,554 → 3,035,961 | +15.3% |
| Unknown short name | 337.08 → 221.76 | 2.00 → 1.00 | 0.0000 → 0.0000 | 14.00 → 7.00 | 6,495,615 → 9,889,241 | +52.2% |
| Empty name | 33.28 → 55.16 | 0.00 → 0.00 | 0.0000 → 0.0000 | 0.00 → 0.00 | 65,573,770 → 39,651,071 | -39.5% |
| 64-byte unknown name | 340.52 → 222.74 | 2.00 → 1.00 | 0.0000 → 0.0000 | 128.00 → 64.00 | 6,434,799 → 9,823,183 | +52.7% |
| 65-byte heap fallback | 350.59 → 365.02 | 2.00 → 2.00 | 0.0000 → 0.0000 | 130.00 → 130.00 | 6,238,887 → 5,995,563 | -3.9% |

One spec operation accepts a prepared Rust str and optional kind, constructs the full spec and drops its owned row/preference strings. Recognized and unknown nonempty names up to 64 bytes remove one input allocation. Empty input was already allocation-free. Its direct spec cost increases from 15.25 to 25.22 ns, with per-round throughput changes from -41.6% to -39.2%; the cause of this regression within the new folding wrapper was not isolated. The 65-byte unknown name retains both heap normalization and owned fallback output; it is a control for the stack boundary. The Marathon input retains the original mArAtHoN spelling in its row name and still allocates that owned output. CustomSongsMaxMegabytes changes throughput by +28.6% and cycles by -22.3%; per-round throughput changes range from +27.6% to +32.1%. The 65-byte fallback changes from 160.28 to 166.79 ns, with per-round throughput changes from -6.4% to -0.8%.

| Case | CPU cycles/call | Allocations/call | Reallocations/call | Requested bytes/call | Calls/second | Throughput change |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Empty integer slice | 596.93 → 586.72 | 1.00 → 1.00 | 0.0000 → 0.0000 | 64.00 → 64.00 | 3,639,010 → 3,722,315 | +2.3% |
| One integer | 1,163.90 → 1,027.81 | 2.00 → 2.00 | 0.0000 → 0.0000 | 80.00 → 80.00 | 1,880,760 → 2,129,699 | +13.2% |
| Three integers | 2,424.38 → 1,495.89 | 2.00 → 2.00 | 2.0000 → 0.0000 | 176.00 → 128.00 | 900,617 → 1,463,486 | +62.5% |
| Ten integers | 4,901.22 → 2,930.22 | 2.00 → 2.00 | 4.0000 → 0.0000 | 560.00 → 320.00 | 446,867 → 745,712 | +66.9% |
| 65 integers | 18,079.78 → 14,386.36 | 2.00 → 2.00 | 7.0000 → 0.0000 | 4,144.00 → 2,112.00 | 121,116 → 152,249 | +25.7% |
| Five offset labels | 3,600.46 → 2,113.13 | 2.00 → 2.00 | 3.0000 → 0.0000 | 304.00 → 192.00 | 607,644 → 1,033,058 | +70.0% |
| False and true | 1,734.05 → 1,233.59 | 2.00 → 2.00 | 1.0000 → 0.0000 | 112.00 → 96.00 | 1,260,716 → 1,766,472 | +40.1% |
| Five offset numbers | 3,317.30 → 1,912.72 | 2.00 → 2.00 | 3.0000 → 0.0000 | 304.00 → 192.00 | 660,131 → 1,141,031 | +72.8% |

One array operation creates a Lua table, writes all primitive values with the unchanged raw_set loop and drops the returned handle. The integer workloads use 0 through N-1. String labels are -1000ms, -500ms, 0ms, 500ms and 1000ms; numeric values are -1, -0.5, 0, 0.5 and 1. Boolean values are false and true. String handles are rooted outside timing so the labels remain interned. Samples warm four array constructions and include any collection during timed work. Returned tables become collectible after their handle drops; final VM collection/teardown is excluded.

Ten integers remove four reallocations, changing requested allocation traffic from 560 to 320 bytes per table. The 65-integer stress case removes seven reallocations. Final capacities deliberately remain the same as the baseline: 16 slots for ten elements and 128 slots for 65 elements. This pass reduces allocation traffic and repeated copying, without claiming smaller retained tables. Ten-item throughput changes by +66.9% and cycles by -40.2%, with per-round throughput changes from +62.5% to +72.7%. Offset-label throughput changes by +70.0%, with per-round changes from +58.5% to +74.7%. Empty and one-element arrays have unchanged allocation counts and capacity.

| Case | CPU cycles/call | Allocations/call | Reallocations/call | Requested bytes/call | Calls/second | Throughput change |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Theme row | 17,063.12 → 16,902.36 | 26.00 → 25.00 | 0.0117 → 0.0117 | 2,207.50 → 2,202.50 | 128,279 → 129,228 | +0.7% |
| GlobalOffsetSeconds row | 23,501.93 → 20,532.68 | 28.00 → 27.00 | 6.0117 → 0.0117 | 2,791.50 → 2,548.50 | 93,088 → 106,658 | +14.6% |
| Marathon row | 24,825.36 → 21,994.59 | 30.00 → 29.00 | 7.0117 → 1.0117 | 2,826.50 → 2,583.50 | 88,151 → 99,387 | +12.7% |
| Unknown row | 15,193.09 → 14,620.33 | 20.00 → 19.00 | 1.0000 → 0.0000 | 2,006.00 → 1,983.00 | 143,901 → 149,585 | +3.9% |

These operations call actual operator-row constructor functions retrieved from create_operator_menu_option_rows_table, including argument conversion, spec lookup, row fields, choice/value arrays, callback construction and returned row-handle drop. The unknown method is resolved through the existing __index handler before timing. Method lookup and rows-factory setup are excluded. Marathon passes the prepared Marathon argument; other cases pass nil. The existing kind String copy remains. Row/callback cycles and Lua collection during construction are included.

Every row removes one operator-name allocation. The offset and Marathon rows also remove six array-growth reallocations across their five-element Choices and Values arrays. The unknown row removes its two-element array's single growth reallocation. The Theme row has one choice and no array-growth saving. Small shared reallocation costs are retained in the table and raw samples; their source was not isolated. GlobalOffsetSeconds changes throughput by +14.6% and cycles by -12.6%, with per-round changes from +9.7% to +32.6%. Row measurements combine the operator-key and array-capacity changes; these speedups are not independent additive estimates.

| Case | CPU cycles/call | Allocations/call | Reallocations/call | Requested bytes/call | Calls/second | Throughput change |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Language en | 2,098.23 → 1,602.16 | 4.00 → 2.00 | 0.0000 → 0.0000 | 106.00 → 96.00 | 1,043,583 → 1,363,954 | +30.7% |
| Language EnGlIsH | 2,109.37 → 1,611.95 | 4.00 → 2.00 | 0.0000 → 0.0000 | 111.00 → 96.00 | 1,037,990 → 1,358,096 | +30.8% |
| Unknown language | 2,110.96 → 1,589.92 | 4.00 → 2.00 | 0.0000 → 0.0000 | 111.00 → 96.00 | 1,037,412 → 1,377,482 | +32.8% |
| Invalid UTF-8 control | 2,545.98 → 2,417.05 | 4.00 → 4.00 | 1.0000 → 1.0000 | 178.00 → 178.00 | 859,771 → 905,859 | +5.4% |
| DoesThemeExist | 1,939.42 → 1,614.50 | 3.00 → 2.00 | 0.0000 → 0.0000 | 107.00 → 96.00 | 1,129,210 → 1,355,450 | +20.0% |
| IsThemeSelectable | 1,945.84 → 1,609.57 | 3.00 → 2.00 | 0.0000 → 0.0000 | 107.00 → 96.00 | 1,124,338 → 1,361,494 | +21.1% |

Availability operations clone prepared receiver/input handles, call the actual Lua callback and return a Boolean. Valid language inputs remove two temporary strings; valid theme inputs remove one. Both theme methods receive Simply Love, the current fixed theme name. The invalid-language control contains 32 bytes of 0xFF and retains the same discarded UTF-8 error construction, allocation count and reallocation count. English throughput changes by +30.8% and cycles by -23.6%, with per-round throughput changes from +15.4% to +32.2%. Theme-existence throughput changes by +20.0% and cycles by -16.8%, with per-round changes from +16.8% to +24.7%. Callback argument allocations remain.

Negative aggregated throughput changes: `spec_empty` -39.5% (rounds -41.6% to -39.2%); `spec_long65` -3.9% (rounds -6.4% to -0.8%)

Every sample builds a fresh fixture before timing. Array, row and availability fixtures collect existing garbage and warm four operations first. Spec calls construct/drop their owned output inside timing. Lua table/callback fixtures remain rooted throughout measurement. VM setup and final teardown are excluded; collection triggered by timed work is included. These helper and warmed-callback results do not establish whole-song load-time or gameplay FPS improvements.

## Behavior and validation

Five new release checks pass. Spec checks compare every field for all twelve registered names, lower/upper/mixed case, six kind arguments, unknown/empty/NUL/Unicode names and the 63/64/65-byte boundary. Array checks compare all four primitive variants, empty slices, UTF-8/NUL strings, duplicate strings, Boolean false, integer extremes, signed zero, NaN and infinities, plus independent mutation across garbage collection. All 32 hole masks on a five-item table preserve baseline length. Sixteen repeated hole patterns for each length 0 through 65 also preserve length before and after appending a new element.

The hole check caught a real compatibility difference during development: exact capacity five returned length five after deleting index four, while the old power-of-two table returned three. The committed implementation reserves the original growth capacity; that regression check now passes. This follows the configured vendored Lua 5.1 implementation's power-of-two dense growth and capacity-sensitive binary length search. Lua's documented flexibility for sparse lengths is not used to dismiss an observable change.

Availability checks compare colon and dot calls for mixed-case aliases, missing/wrong types, invalid UTF-8, whitespace, NUL, Unicode and roots across collection. Unsupported values are not coerced through __tostring. Operator-row checks exercise actual selection loading, preference saving, original kind spelling, Boolean values, dynamic unknown methods and enabled-player lists. Allocation checks verify removal of temporary key/name strings and four growth reallocations for a ten-element array.

The default-feature release library suite has 746 passes, 13 failures and seventeen ignored performance benchmarks. All thirteen failures match the preceding commit's normalized diagnostics, and the retained baseline log hash matches its committed receipt. All 16 prior value/argument checks, 23 draw checks and 7 state-reader checks pass. The JSON retains diagnostic hashes, source/executable hashes and the build-receipt hash. Normalization removes source locations, workspace prefixes, temporary process/thread identifiers and the backtrace hint. Static verification checks frozen code against git show af4d42368 and confirms unchanged operator arms, array writes, row/save/selection callbacks, surrounding theme code and neighboring production files. Existing fixtures, tolerances and acceptance criteria are unchanged.

## Reproduction

```powershell
cargo test -p deadsync-song-lua --lib --release --config profile.release.lto=false --no-run --message-format=json
python scripts/benchmark_song_lua_draw_capture.py --binary <test-executable> --baseline-commit af4d42368 --test-filter option_rows::input_perf::benchmark_option_inputs --out docs/performance/2026-10-19-song-lua-option-inputs.json
```

Run the executable with --test-threads=1 for the full suite. Each round has one warmup plus nine retained samples per variant/workload, pinned to logical CPU 2. Samples perform 10,000 spec constructions, 1,000 arrays, 256 row calls or 4,000 availability calls. Windows, Xeon E5-2696 v4 (44 logical CPUs), rustc 1.98.1, release optimization with LTO disabled. No build or other benchmark overlaps authoritative measurement.

QueryThreadCycleTime measures scheduled current-thread CPU cycles including kernel execution, not a user-only hardware PMU counter. Allocation accounting covers current-thread Rust GlobalAlloc, including mlua and its normal Rust-backed Lua allocator. The inspected mlua 0.12.2 MemoryState allocator delegates to std::alloc alloc/realloc/dealloc, so the Lua table growth shown here is counted. Direct native malloc outside that path is excluded. Accounting overhead is included. Requested bytes describe allocation/reallocation traffic rather than RSS; raw samples retain frees.
