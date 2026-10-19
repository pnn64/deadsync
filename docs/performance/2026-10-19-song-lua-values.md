# Song Lua numeric conversion, equality and hexadecimal output

This pass starts from `54bde073a`, following the [command-dispatch pass](2026-10-19-song-lua-cmd-dispatch.md). It makes three production changes in `values.rs`, guided by M-HOTPATH in [rust-performance.md](../../rust-performance.md):

1. Reuse the validated, trimmed string when integer parsing falls back to `f32`. This removes a second Lua string borrow, UTF-8 validation and trim. The fallback retains its original `f32` precision, rounding and saturating cast.
2. Compare Lua string lengths and bytes before validating UTF-8. Equal byte buffers need one validation; identical backing pointers skip the byte comparison. Invalid UTF-8 remains unequal, even for the same string handle. Checking validity with the standard library avoids creating mlua UTF-8 error objects.
3. Fill an exactly sized byte vector with uppercase hex digits, then consume it using safe `String::from_utf8`. This replaces two character pushes per source byte. Output still uses one allocation of twice the input length; the final UTF-8 check is included in timings.

Production callers use these helpers for integer table fields and multivertex ranges, table search/deduplication, and the `BinaryToHex` compatibility global. Other value types, numeric coercion rules, table identity and nonstring hex results are unchanged. The string borrows retain the Lua strings while bytes are inspected. The pointer check compares addresses without dereferencing raw pointers. This pass adds no unsafe code, dependency or cache.

## Measurements

The [raw results and validation receipt](2026-10-19-song-lua-values.json) retain six alternating-order rounds and 54 samples per variant/workload: 2,592 retained samples across 24 workloads. Tables show medians before → after. Input bytes count one string payload, with zero for nonstrings. Each sample warms the relevant helper before timing. Both variants use the same release executable and counted System allocator. The baseline is an exact frozen copy of `values.rs` from `54bde073a`, with only a provenance comment added.

| Case | Input bytes | CPU cycles/call | Allocations/call | Requested bytes/call | Calls/second | Throughput change |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Integer string, direct parse | 2 | 275.10 → 273.62 | 0.00 → 0.00 | 0 → 0 | 7,935,563 → 7,960,833 | +0.3% |
| Fractional string, f32 fallback | 6 | 515.30 → 311.96 | 0.00 → 0.00 | 0 → 0 | 4,243,762 → 7,010,902 | +65.2% |
| Unicode whitespace and fraction | 11 | 552.92 → 337.99 | 0.00 → 0.00 | 0 → 0 | 3,951,476 → 6,458,697 | +63.5% |
| String overflowing to infinity | 6 | 513.02 → 311.68 | 0.00 → 0.00 | 0 → 0 | 4,252,876 → 7,020,993 | +65.1% |
| Malformed numeric string | 12 | 503.75 → 297.16 | 0.00 → 0.00 | 0 → 0 | 4,344,898 → 7,302,202 | +68.1% |
| Number input, unchanged control | 0 | 18.88 → 18.99 | 0.00 → 0.00 | 0 → 0 | 114,810,563 → 114,285,714 | -0.5% |

One read includes the input Value clone, conversion and destruction of temporary handles. The fractional fallback changes throughput by +65.2% and cycles by -39.5%; per-round throughput changes range from +62.2% to +75.4%. Integral strings exercise the existing successful first parse. The number control never enters a changed branch. Valid UTF-8 numeric inputs, including malformed numeric text and nonfinite strings, retain zero allocation after warming mlua handle storage. First-use handle-storage growth is outside this steady-state measurement.

| Case | Input bytes | CPU cycles/call | Allocations/call | Requested bytes/call | Calls/second | Throughput change |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Equal short ASCII, same VM | 5 | 393.72 → 301.56 | 0.00 → 0.00 | 0 → 0 | 5,506,305 → 7,197,610 | +30.7% |
| Equal 64-KiB ASCII, same VM | 65,536 | 15,549.77 → 4,738.54 | 0.00 → 0.00 | 0 → 0 | 140,791 → 462,177 | +228.3% |
| Different 64-KiB ASCII, first byte | 65,536 | 9,642.57 → 296.67 | 0.00 → 0.00 | 0 → 0 | 226,599 → 7,150,838 | +3055.7% |
| Equal 64-KiB ASCII, separate VMs | 65,536 | 16,358.75 → 11,210.79 | 0.00 → 0.00 | 0 → 0 | 133,594 → 194,175 | +45.3% |
| Equal non-ASCII UTF-8, same VM | 20,480 | 57,058.85 → 27,511.24 | 0.00 → 0.00 | 0 → 0 | 38,371 → 79,628 | +107.5% |
| Equal non-ASCII UTF-8, separate VMs | 20,480 | 57,589.60 → 30,741.15 | 0.00 → 0.00 | 0 → 0 | 37,940 → 71,240 | +87.8% |
| Identical invalid UTF-8, same handle | 4,096 | 1,397.12 → 296.76 | 4.00 → 0.00 | 164 → 0 | 1,536,216 → 7,366,483 | +379.5% |
| Integer/number, unchanged control | 0 | 7.70 → 8.93 | 0.00 → 0.00 | 0 → 0 | 284,333,239 → 242,895,312 | -14.6% |

One comparison borrows two already prepared Values. Same-VM equal strings share interned Lua storage in these fixtures. Separate-VM cases force distinct backing storage and exercise byte equality. The differing ASCII case changes the first byte; it does not represent all mismatch positions. Unequal invalid strings can perform a byte comparison before UTF-8 rejection; large invalid buffers with late mismatches are not measured separately here. Non-ASCII input repeats `é曲` 4,096 times. Invalid input contains 4,096 bytes of `0xFF` and remains unequal; the baseline creates four Rust allocations and two reallocations per comparison while the new path creates none. Its throughput changes by +379.5% with cycles reduced by 78.8%. Equal 64-KiB same-VM ASCII throughput changes by +228.3% (rounds +224.3% to +243.7%); separate-VM ASCII changes by +45.3%.

| Case | Input bytes | CPU cycles/call | Allocations/call | Requested bytes/call | Calls/second | Throughput change |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Empty byte string | 0 | 226.70 → 236.05 | 0.00 → 0.00 | 0 → 0 | 9,644,132 → 9,228,923 | -4.3% |
| Eight bytes | 8 | 358.43 → 363.08 | 1.00 → 1.00 | 16 → 16 | 6,030,636 → 6,013,952 | -0.3% |
| 20-byte digest-sized input | 20 | 388.72 → 392.98 | 1.00 → 1.00 | 40 → 40 | 5,632,851 → 5,568,549 | -1.1% |
| 32-byte digest-sized input | 32 | 428.44 → 409.31 | 1.00 → 1.00 | 64 → 64 | 5,101,130 → 5,350,312 | +4.9% |
| 256-byte binary input | 256 | 1,070.12 → 791.68 | 1.00 → 1.00 | 512 → 512 | 2,032,211 → 2,756,150 | +35.6% |
| 4-KiB binary input | 4,096 | 12,022.34 → 7,520.45 | 1.00 → 1.00 | 8,192 → 8,192 | 180,434 → 290,365 | +60.9% |
| 64-KiB binary input | 65,536 | 219,488.85 → 140,105.31 | 1.00 → 1.00 | 131,072 → 131,072 | 9,955 → 15,579 | +56.5% |
| Nil input, unchanged control | 0 | 26.34 → 25.13 | 0.00 → 0.00 | 0 → 0 | 82,508,251 → 86,505,190 | +4.8% |

One encoding includes input handle cloning, helper execution, safe UTF-8 validation and returned String destruction. Binary inputs cycle through every byte value. Nonempty encodings retain one allocation requesting exactly twice the input size, with zero reallocation. Empty and nil inputs remain allocation-free. The 4-KiB case changes throughput by +60.9% and cycles by -37.4%; per-round throughput changes range from +54.6% to +70.1%. The 20-byte and 32-byte cases represent small digest-sized conversions without asserting a measured production frequency.

| Case | Input bytes | CPU cycles/call | Allocations/call | Requested bytes/call | Calls/second | Throughput change |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 20-byte input through Lua callback | 20 | 1,319.14 → 1,318.70 | 2.00 → 2.00 | 56 → 56 | 1,657,001 → 1,658,650 | +0.1% |
| 4-KiB input through Lua callback | 4,096 | 13,861.08 → 9,261.02 | 2.00 → 2.00 | 8,208 → 8,208 | 157,947 → 236,293 | +49.6% |

The callback uses the same helper/create_string path as the production `BinaryToHex` binding, invoked through `mlua::Function::call`. It includes conversion back to a Lua string and returned handle destruction. Both variants also incur the same Rust allocation in the mlua call path, so these rows have two allocations per call instead of the core helper's one. Each sample warms the callback once, then repeatedly encodes the same binary input. Callback/VM construction and final VM teardown are excluded; any collection during calls is included. The 20-byte callback changes throughput by +0.1%, and the 4-KiB callback by +49.6%.

The mixed integer/number control uses the unchanged numeric branch and is 14.6% slower, with 16.1% more cycles. Its median call time changes from 3.52 ns to 4.12 ns. Its per-round throughput changes range from -15.1% to -12.8%. These measurements do not isolate the code-generation or layout cause.

Negative aggregated throughput changes: `integer_number_control` -0.5% (rounds -0.6% to -0.5%); `equal_number_control` -14.6% (rounds -15.1% to -12.8%); `hex_empty` -4.3% (rounds -5.4% to -2.2%); `hex_8` -0.3% (rounds -2.9% to +2.8%); `hex_digest_20` -1.1% (rounds -3.7% to +0.4%)

These measurements describe the tested helpers and warmed callback workloads. They do not establish whole-song load-time or gameplay-FPS gains. Small inputs and unchanged controls expose fixed overhead, code layout and scheduling variation; all raw timings are retained.

## Behavior and validation

Four new release checks pass. Integer coverage checks explicit rounding, saturation and `f32` precision boundaries; integer/string nonfinite distinctions; Unicode trimming; invalid UTF-8 and embedded NUL. A further 6,147 generated numeric strings are compared against the frozen implementation. Equality coverage compares every pair in a 29-value matrix spanning numeric boundaries, NaN, booleans, strings, tables, functions, threads, userdata and light userdata. Long strings, distinct VMs, explicit Lua garbage collection, valid UTF-8, invalid bytes, embedded NUL and same-handle invalid comparisons retain their prior outcomes. Hex checks compare all byte values and sizes through 64 KiB against independent uppercase formatting and decode the results back to the original bytes. Allocation checks warm mlua handle storage before verifying zero allocation on valid reads/equality, removal of invalid-string equality error allocations, and exact hex output allocation sizes.

The default-feature release library suite has 721 passes, 13 failures and eleven ignored performance benchmarks. All thirteen failures match the preceding commit's normalized diagnostics, and the retained baseline log hash matches its committed receipt. The 23 draw checks and 7 state-reader checks pass. Normalization removes Rust source locations, workspace prefixes, temporary process/thread identifiers and the backtrace hint. The JSON records individual failure/diagnostic hashes, source/executable hashes and the build-receipt hash. Source verification confirms the frozen file matches `git show 54bde073a`, and that production edits are confined to the three selected functions plus the test-module declaration. Existing fixtures, acceptance criteria and tolerances are unchanged.

## Reproduction

```powershell
cargo test -p deadsync-song-lua --lib --release --config profile.release.lto=false --no-run --message-format=json
python scripts/benchmark_song_lua_draw_capture.py --binary <test-executable> --baseline-commit 54bde073a --test-filter values::perf::benchmark_values --out docs/performance/2026-10-19-song-lua-values.json
```

Run the executable with `--test-threads=1` for the full suite. Each round has one warmup plus nine retained samples per variant/workload, pinned to logical CPU 2. Each sample performs 128–100,000 reads/comparisons/encodings/callbacks, depending on the workload. Fixture setup and final destruction are excluded. Windows, Xeon E5-2696 v4 (44 logical CPUs), rustc 1.98.1, release optimization with LTO disabled. No builds or other benchmarks overlap measurements.

QueryThreadCycleTime measures scheduled current-thread CPU cycles including kernel execution, not a user-only hardware PMU counter. Allocation accounting covers current-thread Rust GlobalAlloc, including mlua, and excludes direct native malloc. Accounting overhead is included. Requested bytes describe allocation traffic rather than RSS; raw samples retain reallocations and frees. Reallocation is zero in both variants except the baseline invalid-string equality case, which performs two reallocations per comparison; the new path removes those reallocations.
