# Song Lua ASCII enums and equality dispatch

This pass starts from `e400088b0`, following the [value-conversion pass](2026-10-19-song-lua-values.md). It makes three production optimizations guided by M-HOTPATH in [rust-performance.md](../../rust-performance.md):

1. Compare span-mode names directly as bytes. Only ASCII `len` and `end`, ignoring ASCII case, are accepted. Byte comparison avoids validating unrelated UTF-8 and constructing errors that the Option-returning API discarded.
2. Compare player names directly as bytes. The existing spellings `P1`, `P2`, `PlayerNumber_P1` and `PlayerNumber_P2` remain ASCII-case-insensitive. No trimming or Unicode case folding is introduced. Numeric indices retain the existing zero-based rules.
3. Inline the small equality dispatch while keeping Lua string borrowing and UTF-8 work in a separate non-inlined helper. This targets the preceding pass's numeric comparison regression and repeated search-loop dispatch. The string algorithm and all numeric, boolean, nil and table rules are unchanged.

Span parsing is used by easing and mod-window readers. Player parsing is used by host bindings, option rows and song/theme table helpers. Equality is used by `FindInTable` and the short-list phase of deduplication. Accepted enum names are wholly ASCII, so an invalid UTF-8 sequence cannot equal any accepted name. Unrecognized non-ASCII input still returns None. All borrows keep their Lua strings alive, and this pass adds no unsafe code, dependency or cache.

## Measurements

The [raw measurements and validation receipt](2026-10-19-song-lua-value-dispatch.json) retain six alternating-order rounds and 54 samples per variant/workload: 2,808 retained samples across 26 workloads. Tables show medians before → after. Input bytes count the string payload, with zero for numeric, boolean, nil or table-only inputs; search string rows count the 32 haystack payloads. Both variants run in the same release executable and allocator. The frozen baseline exactly matches production `values.rs` from `e400088b0`, with its test-module declarations omitted.

| Case | Input bytes | CPU cycles/call | Allocations/call | Reallocations/call | Requested bytes/call | Calls/second | Throughput change |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Recognized len | 3 | 280.61 → 216.02 | 0.00 → 0.00 | 0.00 → 0.00 | 0 → 0 | 7,786,646 → 10,071,508 | +29.3% |
| Recognized mixed-case EnD | 3 | 268.70 → 214.89 | 0.00 → 0.00 | 0.00 → 0.00 | 0 → 0 | 8,128,429 → 10,195,239 | +25.4% |
| Unrecognized 64-KiB ASCII | 65,536 | 4,738.97 → 211.79 | 0.00 → 0.00 | 0.00 → 0.00 | 0 → 0 | 460,722 → 9,980,507 | +2066.3% |
| Unrecognized non-ASCII UTF-8 | 20,480 | 28,375.95 → 214.36 | 0.00 → 0.00 | 0.00 → 0.00 | 0 → 0 | 77,012 → 9,865,125 | +12709.8% |
| Invalid UTF-8 | 4,096 | 759.52 → 206.82 | 2.00 → 0.00 | 1.00 → 0.00 | 82 → 0 | 2,878,319 → 10,540,184 | +266.2% |
| Nil, unchanged control | 0 | 11.35 → 11.41 | 0.00 → 0.00 | 0.00 → 0.00 | 0 → 0 | 191,662,674 → 190,748,689 | -0.5% |

One span read clones the prepared input handle, runs the parser and drops temporary handles. Recognized `len` throughput changes by +29.3%, with cycles reduced by 23.0%; per-round throughput changes range from +14.5% to +42.4%. The unrecognized Unicode case repeats `é曲` 4,096 times. Unknown long inputs highlight avoided validation work and do not establish the frequency of those inputs in real songs. Invalid input contains 4,096 bytes of `0xFF`: the baseline returns None after two Rust allocations and one reallocation, while the new path returns None without allocation.

| Case | Input bytes | CPU cycles/call | Allocations/call | Reallocations/call | Requested bytes/call | Calls/second | Throughput change |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Recognized P1 | 2 | 195.01 → 149.69 | 0.00 → 0.00 | 0.00 → 0.00 | 0 → 0 | 11,217,050 → 14,535,940 | +29.6% |
| Recognized PlayerNumber_P2 | 15 | 222.51 → 171.03 | 0.00 → 0.00 | 0.00 → 0.00 | 0 → 0 | 9,817,878 → 12,759,171 | +30.0% |
| Recognized mixed-case long name | 15 | 222.29 → 163.60 | 0.00 → 0.00 | 0.00 → 0.00 | 0 → 0 | 9,844,942 → 13,291,686 | +35.0% |
| Unrecognized 64-KiB ASCII | 65,536 | 4,751.40 → 144.91 | 0.00 → 0.00 | 0.00 → 0.00 | 0 → 0 | 457,225 → 14,301,676 | +3027.9% |
| Unrecognized non-ASCII UTF-8 | 20,480 | 28,956.85 → 150.91 | 0.00 → 0.00 | 0.00 → 0.00 | 0 → 0 | 75,670 → 13,763,441 | +18088.7% |
| Invalid UTF-8 | 4,096 | 718.53 → 144.98 | 2.00 → 0.00 | 1.00 → 0.00 | 82 → 0 | 3,047,387 → 14,847,810 | +387.2% |
| Integer one, unchanged control | 0 | 3.69 → 3.35 | 0.00 → 0.00 | 0.00 → 0.00 | 0 → 0 | 592,066,311 → 650,195,059 | +9.8% |

One player read borrows a prepared Value. The short and long recognized names change throughput by +29.6% and +30.0%, with cycles reduced by 23.2% and 23.1%. Long-name per-round throughput changes range from +22.4% to +35.9%. The invalid-string case removes two allocations and one reallocation while retaining None. Valid inputs retain zero allocation. The integer control uses unchanged numeric code.

| Case | Input bytes | CPU cycles/call | Allocations/call | Reallocations/call | Requested bytes/call | Calls/second | Throughput change |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Integer/number comparison | 0 | 9.49 → 4.51 | 0.00 → 0.00 | 0.00 → 0.00 | 0 → 0 | 230,786,984 → 484,378,784 | +109.9% |
| Boolean comparison | 0 | 8.64 → 3.99 | 0.00 → 0.00 | 0.00 → 0.00 | 0 → 0 | 251,414,205 → 547,495,209 | +117.8% |
| Equal short ASCII | 5 | 312.78 → 309.59 | 0.00 → 0.00 | 0.00 → 0.00 | 0 → 0 | 6,986,900 → 7,061,150 | +1.1% |
| Equal 64-KiB ASCII | 65,536 | 4,891.59 → 4,928.03 | 0.00 → 0.00 | 0.00 → 0.00 | 0 → 0 | 445,721 → 442,945 | -0.6% |
| Equal non-ASCII UTF-8 | 20,480 | 27,759.46 → 27,857.64 | 0.00 → 0.00 | 0.00 → 0.00 | 0 → 0 | 78,415 → 78,148 | -0.3% |
| Identical invalid UTF-8 | 4,096 | 300.93 → 301.10 | 0.00 → 0.00 | 0.00 → 0.00 | 0 → 0 | 7,235,890 → 7,249,003 | +0.2% |
| Identical table handles | 0 | 207.97 → 198.20 | 0.00 → 0.00 | 0.00 → 0.00 | 0 → 0 | 10,534,078 → 11,030,831 | +4.7% |

One comparison borrows two prepared Values. The integer/number case uses the same Value shapes as the preceding pass's numeric control. Against the frozen current baseline it changes throughput by +109.9% and cycles by -52.5%, with per-round throughput changes from +90.8% to +117.2%. Its median elapsed time is 4.33 ns before and 2.06 ns after. This comparison is within one executable; timings from separate passes are not combined. String and table rows expose dispatch/code-layout effects on those paths. Equal strings share Lua storage in these fixtures. The existing invalid-string comparison remains false and allocation-free.

| Case | Input bytes | CPU cycles/call | Allocations/call | Reallocations/call | Requested bytes/call | Calls/second | Throughput change |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 32 integers, matching float at end | 0 | 256.49 → 125.11 | 0.00 → 0.00 | 0.00 → 0.00 | 0 → 0 | 8,467,401 → 17,182,131 | +102.9% |
| 32 integers, missing float | 0 | 253.96 → 122.70 | 0.00 → 0.00 | 0.00 → 0.00 | 0 → 0 | 8,561,644 → 17,286,085 | +101.9% |
| 32 strings, match at end | 288 | 9,829.65 → 9,825.15 | 0.00 → 0.00 | 0.00 → 0.00 | 0 → 0 | 222,311 → 222,990 | +0.3% |
| 32 strings, no match | 288 | 9,597.97 → 9,648.23 | 0.00 → 0.00 | 0.00 → 0.00 | 0 → 0 | 227,436 → 226,267 | -0.5% |
| Lua callback/pairs, 32 integers | 0 | 7,652.43 → 7,412.19 | 0.00 → 0.00 | 0.00 → 0.00 | 0 → 0 | 285,845 → 295,286 | +3.3% |
| Lua callback/pairs, 32 strings | 288 | 24,439.13 → 24,218.20 | 33.00 → 33.00 | 0.00 → 0.00 | 528 → 528 | 89,471 → 90,256 | +0.9% |

Each vector search scans 32 prepared Values with `position`, either finding the last item or returning no match. Numeric needles are floating-point Values; entries are integers. Strings use nine-byte names `player_00` through `player_31`. Numeric searches change throughput by +102.9% and +101.9%; string searches change by +0.3% and -0.5%. Every vector search remains allocation-free.

The two callback cases match the production `FindInTable` pairs/equality path and are invoked through `mlua::Function::call`. They include argument-handle clones, table iteration and returned-key destruction. Both variants find key 32. Numeric and string callback throughput changes are +3.3% and +0.9%. Table iteration materializes string Values through mlua, causing the same 33 Rust allocations and 528 requested bytes in both string callback variants. Numeric callbacks have no measured Rust allocation. These call-path costs are included, rather than inferred from direct comparison timings.

Negative aggregated throughput changes: `span_nil_control` -0.5% (rounds -28.1% to +10.2%); `dispatch_equal_ascii` -0.6% (rounds -14.8% to +11.8%); `dispatch_equal_utf8` -0.3% (rounds -9.1% to +1.4%); `search_strings_missing` -0.5% (rounds -3.7% to +6.7%)

Every sample warms the relevant parser, comparison or callback before timing, including mlua handle storage. VM/fixture construction and final teardown are excluded. Any garbage collection during the measured loop is included. These are helper and warmed search measurements; they do not establish whole-song initialization or gameplay-FPS gains. Inlining choices can affect code size and other callers, and the tests here do not profile every host binding.

## Behavior and validation

Four new release checks pass alongside the four previous value checks. Enum coverage exercises every ASCII case variant of accepted names, whitespace and NUL boundaries, full-width/non-ASCII alternatives, malformed UTF-8 sequences, and every possible byte substitution at every position of each accepted spelling. Explicit expectations and comparisons with the frozen parser preserve numeric boundaries and strict accepted names. Equality coverage compares all pairs in a 20-value matrix, searches and deduplicates a repeated mixed list, and checks table identity for tables with metatables. String handles remain valid across explicit garbage collection and separate Lua states. Lua callback checks compare returned values and conversion diagnostics for enum names, numeric indices, invalid binary strings, search results and invalid table arguments. Allocation checks warm each fixture, verify zero churn for valid reads/equality, and verify removal of invalid-enum error allocations and reallocations.

The default-feature release library suite has 725 passes, 13 failures and twelve ignored performance benchmarks. All thirteen failures match `e400088b0`'s normalized diagnostics, and the retained baseline log hash matches its committed receipt. All 23 draw checks and 7 state-reader checks pass. The JSON retains individual diagnostic hashes, source/executable hashes and the build-receipt hash. Normalization removes Rust source locations, workspace prefixes, temporary process/thread identifiers and the backtrace hint. Source verification proves the frozen production file matches `git show e400088b0`, only the three selected readers/dispatch changed, and the extracted string body and nonstring equality rules are unchanged. Existing fixture data, tolerances and acceptance criteria are unchanged.

## Reproduction

```powershell
cargo test -p deadsync-song-lua --lib --release --config profile.release.lto=false --no-run --message-format=json
python scripts/benchmark_song_lua_draw_capture.py --binary <test-executable> --baseline-commit e400088b0 --test-filter values::dispatch_perf::benchmark_value_dispatch --out docs/performance/2026-10-19-song-lua-value-dispatch.json
```

Run the executable with `--test-threads=1` for the full suite. Each round has one warmup plus nine retained samples per variant/workload, pinned to logical CPU 2. Each sample performs 256–100,000 calls depending on the input; search samples contain 1,000 complete searches. Windows, Xeon E5-2696 v4 (44 logical CPUs), rustc 1.98.1, release optimization with LTO disabled. Both variants use the counted System allocator. No builds or other benchmarks overlap measurements.

QueryThreadCycleTime measures scheduled current-thread CPU cycles including kernel execution, not a user-only hardware PMU counter. Allocation accounting covers current-thread Rust GlobalAlloc, including mlua, and excludes direct native malloc. Accounting overhead is included. Requested bytes describe allocation/reallocation traffic rather than RSS; raw samples retain frees. Inlining results depend on the compiler and build profile, so this report makes claims for the measured profile.
