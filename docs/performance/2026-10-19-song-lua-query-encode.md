# Song Lua URL sizing, direct query entries and borrowed scalar text

This pass starts from `11cd0e266`, following the [color-input pass](2026-10-19-song-lua-color-inputs.md). It makes three optimizations guided by M-HOTPATH, M-INITIAL-CAPACITY and M-MEM-REUSE in [rust-performance.md](../../rust-performance.md):

1. Size percent-encoded URL output exactly before allocation, using a compile-time 256-byte table of extra output bytes. Unreserved inputs use a bulk copy. This removes buffer growth while preserving the original byte encoder for escaped output.
2. Write each query entry into one exactly sized String. Keys, the equals sign and values append directly, removing the two temporary component Strings and format-based assembly. The existing entry vector, full-entry sort and final join remain.
3. Borrow valid Lua strings during query construction, format integers in an itoa stack buffer and use static Boolean spellings. Invalid UTF-8 still uses the same lossy conversion; floating point retains the same Rust to_string formatting. The public owned query_value_text API remains unchanged.

The callback constructor and argument dispatch are unchanged. Unsupported keys are skipped and unsupported values become empty strings. Query iteration remains raw; no __pairs or __tostring callback is introduced. Every full encoded key=value entry is sorted lexicographically before joining with ampersands, including colliding textual spellings from different key types. Percent encoding still uses uppercase hex, encodes spaces as %20 and encodes UTF-8 bytes individually. String guards keep their Lua roots alive through the private callback; borrowed data cannot escape. No unsafe code, dependency, mutable cache or actual network operation is added.

## Measurements

The [raw measurements and validation receipt](2026-10-19-song-lua-query-encode.json) retain six alternating-order rounds and 54 samples per variant/workload: 2,376 retained samples across 22 workloads. Tables show medians before → after. Both variants run in the same release executable with the counted System allocator. The baseline freezes all production net.rs code from `11cd0e266` verbatim; its existing test module is omitted. The tests use actual mlua values and actual EncodeQueryParameters callbacks.

| Case | CPU cycles/call | Allocations/call | Reallocations/call | Requested bytes/call | Calls/second | Throughput change |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Empty input | 13.29 → 8.68 | 0 → 0 | 0 → 0 | 0 → 0 | 161,550,889 → 250,312,891 | +54.9% |
| abc-._~ | 174.75 → 130.99 | 1 → 1 | 0 → 0 | 8 → 7 | 12,528,188 → 16,533,025 | +32.0% |
| a b+c/% | 376.84 → 170.54 | 1 → 1 | 1 → 0 | 24 → 15 | 5,819,367 → 12,829,559 | +120.5% |
| é曲 | 383.98 → 171.06 | 1 → 1 | 1 → 0 | 24 → 15 | 5,707,763 → 12,818,869 | +124.6% |
| a, NUL, b | 164.93 → 159.95 | 1 → 1 | 0 → 0 | 8 → 5 | 13,207,423 → 13,710,838 | +3.8% |
| 8-KiB unreserved | 24,247.89 → 5,450.63 | 1 → 1 | 10 → 0 | 16,376 → 8,192 | 90,327 → 395,458 | +337.8% |
| 3-KiB escaped | 23,450.27 → 19,466.26 | 1 → 1 | 11 → 0 | 32,760 → 9,216 | 93,026 → 111,826 | +20.2% |
| 5.5-KiB mixed | 35,419.24 → 31,096.98 | 1 → 1 | 11 → 0 | 32,760 → 13,824 | 61,719 → 70,364 | +14.0% |

Each URL operation returns and drops an owned Rust String. The long inputs are abcdefgh repeated 1,024 times (8,192 bytes), space/slash/percent repeated 1,024 times (3,072 bytes), and abcé曲 followed by space/slash/percent repeated 512 times (5,632 bytes). Their encoded output lengths are 8,192, 9,216 and 13,824 bytes. Every nonempty result now needs one allocation and zero reallocations. Long unreserved throughput changes by +337.8% and cycles by -77.5%; its per-round throughput changes range from +311.3% to +379.2%. The heavily escaped case changes throughput by +20.2% and cycles by -17.0%, with per-round changes from +9.2% to +25.3%.

| Case | CPU cycles/call | Allocations/call | Reallocations/call | Requested bytes/call | Calls/second | Throughput change |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Valid Lua string | 356.34 → 236.76 | 1 → 0 | 0 → 0 | 9 → 0 | 6,106,124 → 9,237,022 | +51.3% |
| i64::MIN | 174.78 → 40.65 | 1 → 0 | 0 → 0 | 20 → 0 | 12,500,000 → 53,633,682 | +329.1% |
| Boolean true | 128.92 → 16.86 | 1 → 0 | 0 → 0 | 4 → 0 | 16,774,302 → 128,534,704 | +666.3% |
| Float formatting control | 362.57 → 356.87 | 1 → 1 | 0 → 0 | 8 → 8 | 6,043,026 → 6,106,311 | +1.0% |
| Invalid UTF-8 control | 1,086.15 → 1,097.06 | 1 → 1 | 2 → 2 | 224 → 224 | 2,010,879 → 1,987,143 | -1.2% |

Scalar reads clone a prepared Value handle and consume its optional text within the call. The baseline creates/drops its owned String; the new private adapter borrows or uses stack text. Valid text is two words, the integer is i64::MIN and the Boolean is true. These three remove one allocation each. The floating-point control is 12.345 and retains the same allocation-based formatting. Invalid text contains 32 bytes of 0xFF and retains the same lossy conversion, including its allocation and two reallocations. Timing changes in these two controls are not attributed to a new formatting algorithm. The public query_value_text function still returns an owned Option<String> and is not optimized by this pass.

| Case | CPU cycles/call | Allocations/call | Reallocations/call | Requested bytes/call | Calls/second | Throughput change |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Empty table | 248.58 → 249.35 | 0 → 0 | 0 → 0 | 0 → 0 | 8,748,906 → 8,722,198 | -0.3% |
| One string pair | 3,004.74 → 1,873.54 | 9 → 5 | 2 → 0 | 214 → 162 | 729,076 → 1,170,001 | +60.5% |
| 64 string pairs | 176,333.96 → 95,336.73 | 450 → 194 | 186 → 4 | 12,481 → 7,479 | 12,353 → 22,979 | +86.0% |
| 64 integer pairs | 79,606.48 → 34,909.93 | 322 → 66 | 4 → 4 | 7,308 → 3,641 | 27,303 → 62,658 | +129.5% |
| Eight invalid string values | 22,989.33 → 15,540.82 | 58 → 34 | 25 → 9 | 1,335 → 975 | 95,359 → 140,951 | +47.8% |

String queries contain key N → value/N pairs for N starting at zero. Integer queries contain numeric N → N+1 pairs; mlua returns values according to the current Lua backend. The lossy query replaces eight string values with bytes FF 00 61. One operation clones the table handle, iterates all entries, encodes and sorts them, joins the result and drops the owned String. Fixture creation is excluded. The one-pair allocation count drops 9 → 5. Its four removed allocations come from two owned scalar copies and two component outputs. The invalid-value case retains its lossy text allocation, removing three allocations per pair. Remaining vector growth and mlua pair-handle allocations are included. The 64-string result changes throughput by +86.0% and cycles by -45.9%, with per-round changes from +77.9% to +88.3%. The 64-integer result changes throughput by +129.5% and cycles by -56.1%, with per-round changes from +121.9% to +135.8%.

| Case | CPU cycles/call | Allocations/call | Reallocations/call | Requested bytes/call | Calls/second | Throughput change |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Empty table | 1,665.46 → 1,760.39 | 2 → 2 | 0 → 0 | 96 → 96 | 1,313,801 → 1,243,085 | -5.4% |
| One string pair | 4,724.74 → 3,442.86 | 11 → 7 | 2 → 0 | 310 → 258 | 462,920 → 636,132 | +37.4% |
| 64 string pairs | 197,611.74 → 97,371.39 | 452 → 196 | 186 → 4 | 12,577 → 7,575 | 11,068 → 22,364 | +102.1% |
| Eight invalid string values | 24,400.72 → 17,250.94 | 60 → 36 | 25 → 9 | 1,431 → 1,071 | 89,313 → 126,461 | +41.6% |

Each API operation calls the actual EncodeQueryParameters function returned by create_network_table, passing its receiver and query table, then drops the returned LuaString handle. The same query fixtures are used. Rust callback argument/output conversion and all encoder work are included. The one-pair API changes throughput by +37.4% and cycles by -27.1%, with per-round throughput changes from +25.3% to +49.4%. The 64-string API changes throughput by +102.1% and cycles by -50.7%, with per-round changes from +83.0% to +125.7%. Empty queries retain their original allocation counts and are controls. Query improvements combine all three optimizations; these are not independent additive speedup estimates.

The empty API control changes from 761.15 to 804.45 ns, with per-round throughput changes from -23.7% to +21.8%. Its timing change was not isolated. Empty component calls take only 6.19 ? 4.00 ns and have no allocation change.

Negative aggregated throughput changes: `text_lossy` -1.2% (rounds -2.7% to +6.6%); `query_empty` -0.3% (rounds -21.7% to +2.3%); `api_empty` -5.4% (rounds -23.7% to +21.8%)

Every sample constructs a fresh fixture and warms both scalar adapters, the selected URL helper, or four complete query operations before timing. The fixture remains alive throughout timing. Returned Rust String destruction and temporary Lua handle drops are included. Final VM teardown is excluded; any garbage collection triggered during timed calls is included. Inputs are reused within a sample. These helper and warmed-callback measurements do not establish whole-song loading or gameplay FPS improvements. Direct native Lua malloc is outside Rust allocation accounting.

## Behavior and validation

Four new release checks pass. Component checks cover explicit percent spelling, all ASCII bytes, embedded NUL, Unicode byte encoding, long mixed/unreserved strings and valid Unicode scalars sampled every 127 code points. Query checks compare exact frozen output for mixed key types, colliding textual keys, empty strings, separators, invalid UTF-8, unsupported keys/values, raw iteration and table preservation across garbage collection. Scalar checks compare the public and private conversion paths for signed integer extremes, negative zero, NaN, infinities, a tiny float, invalid text and unsupported values. Callback checks retain the existing receiver/argument behavior for colon calls, explicit receivers, a lone query table, missing values and wrong types. Allocation checks verify exact component capacity, allocation-free borrowed string/integer/Boolean consumption and four removed allocations per ordinary pair.

The default-feature release library suite has 741 passes, 13 failures and sixteen ignored performance benchmarks. All thirteen failures match the preceding commit's normalized diagnostics; the retained baseline log hash matches its committed receipt. All 16 prior value/argument checks, 23 draw checks and 7 state-reader checks pass. The JSON retains individual diagnostic hashes, measured-source/executable hashes and the build-receipt hash. Normalization removes source locations, workspace prefixes, temporary process/thread identifiers and the backtrace hint. Source verification compares frozen production code to git show 11cd0e266 and verifies the unchanged constructor, public value conversion, existing tests and neighboring production files. Existing fixtures, tolerances and acceptance criteria are unchanged.

## Reproduction

```powershell
cargo test -p deadsync-song-lua --lib --release --config profile.release.lto=false --no-run --message-format=json
python scripts/benchmark_song_lua_draw_capture.py --binary <test-executable> --baseline-commit 11cd0e266 --test-filter net::perf::benchmark_query_encode --out docs/performance/2026-10-19-song-lua-query-encode.json
```

Run the executable with --test-threads=1 for the full suite. Each round has one warmup plus nine retained samples per variant/workload, pinned to logical CPU 2. Samples perform 20,000 empty URL calls; 10,000 short URL/scalar calls; 512 long URL calls; 128 queries/API calls with 64 entries; or 1,000 other query/API calls. Windows, Xeon E5-2696 v4 (44 logical CPUs), rustc 1.98.1, release optimization with LTO disabled. No build or other benchmark overlaps the authoritative measurements.

QueryThreadCycleTime measures scheduled current-thread CPU cycles including kernel execution, not a user-only hardware PMU counter. Allocation accounting covers current-thread Rust GlobalAlloc, including mlua, and excludes direct native malloc. Accounting overhead is included. Requested bytes describe allocation/reallocation traffic rather than RSS; raw samples retain frees.
