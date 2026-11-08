# Song Lua borrowed compatibility arguments and streaming version checks

This pass starts from `7a4bdbb55`, following the [ASCII-enum and equality-dispatch pass](2026-10-19-song-lua-value-dispatch.md). It makes three optimizations guided by M-HOTPATH in [rust-performance.md](../../rust-performance.md):

1. Borrow split arguments from their existing Lua strings. `string.split(text, separator)` and global `split(separator, text)` share a private helper while retaining their opposite argument order. Valid, nonempty text and separator inputs avoid two owned Rust String copies.
2. Borrow the `Basename` input while selecting the final path component. The existing path helper and returned Lua string are unchanged; the temporary Rust input copy is removed.
3. Stream version-comparison arguments through an enum iterator instead of collecting a temporary Vec. A valid first string still supplies exactly three parsed components and ignores later arguments. Numeric inputs still use the original `read_f32` rounding/coercion rules. Both exact and minimum comparisons stop when their result is determined.

The public `version_args` Vec-returning API, version-prefix parser and product-version wrappers remain unchanged. New borrowing helpers accept the same UTF-8 strings as `read_string`; absent, nonstring and invalid-UTF-8 arguments retain the original empty-string fallback. They do not coerce numbers to text. BorrowedStr keeps its Lua string alive while the unchanged split/path helper and output construction run. No unsafe code, dependency or cache is added.

## Measurements

The [raw measurements and validation receipt](2026-10-19-song-lua-compat-args.json) retain six alternating-order rounds and 54 samples per variant/workload: 3,024 samples across 28 workloads. Tables show medians before → after. Input bytes count the text/separator payload or version string; numeric argument lists have zero string payload bytes. Both variants use one release executable and counted System allocator. The baseline freezes the three original callback bodies as helper functions and the original production version module from `7a4bdbb55`. Borrowing MultiValue in the baseline adapters leaves the callback body and input clones unchanged; full callback rows retain native argument conversion and returned handle destruction.

| Case | Input bytes | CPU cycles/call | Allocations/call | Reallocations/call | Requested bytes/call | Calls/second | Throughput change |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Short comma-separated text, helper | 6 | 2,138.53 → 1,626.55 | 4.00 → 2.00 | 0.00 → 0.00 | 128 → 112 | 1,015,744 → 1,340,258 | +31.9% |
| 512 comma-delimited tokens, helper | 4,609 | 140,533.16 → 135,949.38 | 4.00 → 2.00 | 0.00 → 0.00 | 12,888 → 8,272 | 15,523 → 16,084 | +3.6% |
| 32 Unicode characters, empty separator | 80 | 7,879.28 → 7,433.42 | 3.00 → 2.00 | 0.00 → 0.00 | 656 → 576 | 277,612 → 292,471 | +5.4% |
| 8-KiB text, absent separator | 8,193 | 15,436.16 → 14,236.20 | 4.00 → 2.00 | 0.00 → 0.00 | 8,280 → 80 | 141,354 → 151,848 | +7.4% |
| Short text through string.split callback | 6 | 3,333.82 → 2,839.62 | 7.00 → 5.00 | 0.00 → 0.00 | 240 → 224 | 654,579 → 770,208 | +17.7% |
| Long text through string.split callback | 4,609 | 114,265.19 → 113,886.20 | 7.00 → 5.00 | 0.00 → 0.00 | 13,000 → 8,384 | 19,146 → 19,238 | +0.5% |
| Short text through global split callback | 6 | 3,344.85 → 2,814.43 | 7.00 → 5.00 | 0.00 → 0.00 | 240 → 224 | 653,926 → 775,179 | +18.5% |
| Unicode through global split callback | 80 | 8,119.36 → 7,840.27 | 6.00 → 5.00 | 0.00 → 0.00 | 768 → 688 | 268,555 → 278,397 | +3.7% |
| Invalid UTF-8 text, valid separator callback | 33 | 3,372.51 → 3,007.92 | 8.00 → 7.00 | 1.00 → 1.00 | 282 → 274 | 647,396 → 727,141 | +12.3% |

One helper call constructs and drops the output table handle from prepared arguments. Native Lua allocations can occur during construction or collection and are outside Rust GlobalAlloc counting; Lua work remains inside CPU/elapsed timing. Short text is `a,b,c` with separator comma. Long text repeats `abcdefgh,` 512 times, including the trailing empty output field. Unicode input repeats `é曲` sixteen times with an empty separator; this retains character-based splitting. The absent-separator case uses 8,192 ASCII bytes and a pipe separator.

Short-helper throughput changes by +31.9%, with cycles reduced by 23.9%; per-round throughput changes range from +28.4% to +52.2%. The two nonempty inputs remove two temporary copy allocations. Empty separator needs no original copy allocation, so that case removes one. Large inputs also remove their copied payload bytes. The complete short `string.split` and global `split` callbacks change throughput by +17.7% and +18.5%. Existing output and mlua argument allocations remain; the complete callback is not allocation-free. Invalid UTF-8 text still falls back to empty text and retains its UTF-8 error construction; only the valid separator's temporary copy is removed in that row.

| Case | Input bytes | CPU cycles/call | Allocations/call | Reallocations/call | Requested bytes/call | Calls/second | Throughput change |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Filename without directories, helper | 8 | 665.96 → 520.00 | 1.00 → 0.00 | 0.00 → 0.00 | 8 → 0 | 3,279,764 → 4,199,916 | +28.1% |
| 40-byte path, helper | 40 | 691.53 → 535.36 | 1.00 → 0.00 | 0.00 → 0.00 | 40 → 0 | 3,153,828 → 4,079,967 | +29.4% |
| 14-KiB path, helper | 14,345 | 2,565.79 → 1,327.43 | 1.00 → 0.00 | 0.00 → 0.00 | 14,345 → 0 | 852,569 → 1,648,397 | +93.3% |
| Invalid UTF-8 input, helper | 32 | 1,505.11 → 1,578.37 | 2.00 → 2.00 | 1.00 → 1.00 | 82 → 82 | 1,450,642 → 1,387,011 | -4.4% |
| 40-byte path through callback | 40 | 1,374.56 → 1,190.18 | 2.00 → 1.00 | 0.00 → 0.00 | 56 → 16 | 1,587,617 → 1,838,235 | +15.8% |
| 14-KiB path through callback | 14,345 | 3,315.11 → 2,040.80 | 2.00 → 1.00 | 0.00 → 0.00 | 14,361 → 16 | 659,022 → 1,066,411 | +61.8% |
| Number input, unchanged empty-result control | 0 | 682.43 → 682.59 | 0.00 → 0.00 | 0.00 → 0.00 | 0 → 0 | 3,211,304 → 3,183,193 | -0.9% |

One helper call clones the prepared input Value handle, computes the path component, creates the Lua string and drops the returned handle. The short path is `Themes/default/graphics/screen/thing.png`; the long path repeats `folder/` 2,048 times before `thing.png`. Valid helper calls change from one Rust allocation to zero. The short-path helper changes throughput by +29.4% and cycles by -22.6%; per-round throughput changes range from +26.8% to +40.0%. The short-path callback changes throughput by +15.8%, with its Rust allocation count changing from two to one. Invalid UTF-8 still produces an empty result after the same error allocations; the number-input control remains empty without coercion. The invalid-input helper control has -4.4% aggregate throughput change, with per-round changes from -47.6% to +73.1%. Its large variation tracks alternating variant order; these measurements do not isolate that cause or establish a uniform slowdown.

| Case | Input bytes | CPU cycles/call | Allocations/call | Reallocations/call | Requested bytes/call | Calls/second | Throughput change |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Exact version, string arguments | 5 | 454.20 → 347.61 | 1.00 → 0.00 | 0.00 → 0.00 | 24 → 0 | 4,813,014 → 6,290,099 | +30.7% |
| Minimum version, string arguments | 5 | 451.86 → 339.54 | 1.00 → 0.00 | 0.00 → 0.00 | 24 → 0 | 4,818,929 → 6,407,381 | +33.0% |
| Exact version, numbers arguments | 0 | 321.73 → 179.88 | 1.00 → 0.00 | 0.00 → 0.00 | 32 → 0 | 6,778,972 → 12,138,132 | +79.1% |
| Minimum version, numbers arguments | 0 | 325.90 → 172.32 | 1.00 → 0.00 | 0.00 → 0.00 | 32 → 0 | 6,709,383 → 12,719,410 | +89.6% |
| Exact version, early arguments | 0 | 2,557.84 → 123.00 | 1.00 → 0.00 | 4.00 → 0.00 | 992 → 0 | 855,326 → 17,765,145 | +1977.0% |
| Minimum version, early arguments | 0 | 2,544.32 → 123.62 | 1.00 → 0.00 | 4.00 → 0.00 | 992 → 0 | 859,731 → 17,710,086 | +1960.0% |
| Exact version, empty arguments | 0 | 23.21 → 16.24 | 0.00 → 0.00 | 0.00 → 0.00 | 0 → 0 | 93,632,959 → 133,333,333 | +42.4% |
| Minimum version, empty arguments | 0 | 23.86 → 16.84 | 0.00 → 0.00 | 0.00 → 0.00 | 0 → 0 | 91,157,703 → 128,700,129 | +41.2% |

Direct comparisons borrow prepared arguments against example product version `5.1.0`. String input is `5.1.0`; numeric input is the three integers 5, 1 and 0. Early-mismatch input contains 64 integers equal to 7. Empty input retains false for exact equality and true for minimum comparison. Numeric exact/minimum throughput changes by +79.1% and +89.6%, with cycles reduced by 44.1% and 47.1%. Each nonempty direct check removes one Vec allocation; early mismatch also avoids four resizes and unnecessary remaining numeric reads. Version parsing and `read_f32` remain unchanged. Conversions return Option and do not invoke authored code; short-circuiting can avoid discarded conversion errors without changing the Boolean result.

| Case | Input bytes | CPU cycles/call | Allocations/call | Reallocations/call | Requested bytes/call | Calls/second | Throughput change |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Exact version callback, string arguments | 5 | 1,170.21 → 1,041.53 | 3.00 → 2.00 | 0.00 → 0.00 | 80 → 56 | 1,867,762 → 2,098,416 | +12.3% |
| Minimum version callback, string arguments | 5 | 1,198.14 → 1,095.80 | 3.00 → 2.00 | 0.00 → 0.00 | 80 → 56 | 1,820,002 → 1,993,819 | +9.6% |
| Exact version callback, numbers arguments | 0 | 942.48 → 795.74 | 2.00 → 1.00 | 0.00 → 0.00 | 152 → 120 | 2,316,155 → 2,746,121 | +18.6% |
| Minimum version callback, numbers arguments | 0 | 980.18 → 920.80 | 2.00 → 1.00 | 0.00 → 0.00 | 152 → 120 | 2,236,761 → 2,380,669 | +6.4% |

Version callbacks use the same MultiValue-to-Boolean pattern as the production product-version globals. Exact and minimum callbacks with numeric inputs change throughput by +18.6% and +6.4%. Both remove one Rust allocation. Callback setup uses example product `5.1.0`; the production wrappers continue using the existing product constant. Both variants receive identical input handles and return the same Boolean.

Negative aggregated throughput changes: `basename_core_invalid` -4.4% (rounds -47.6% to +73.1%); `basename_api_nonstring` -0.9% (rounds -5.4% to +19.6%)

Every sample warms its exact direct-helper or callback path, including mlua handle storage, before timing. Fixture/VM creation and final VM teardown are excluded. Returned handle drops and temporary Rust output destruction are timed; any collection during the loop is included. All repeated calls use the same prepared input. The results establish helper and warmed callback effects for these inputs, without a whole-song load-time or gameplay-FPS claim.

## Behavior and validation

Four new release checks pass. Split coverage compares exact bytes, array lengths and table shape over Unicode, empty separators, adjacent/trailing separators, NUL, missing arguments, invalid UTF-8 and nonstring values. It checks both argument orders and character-split output through a Lua callback. Basename checks cover Unix/Windows separators, trailing separators, root-like paths, Unicode/NUL, nonstring input and invalid UTF-8. Callback checks compare returned values and preserve the absence of `__tostring` coercion. Version checks compare both predicates against the frozen implementation for every three-value combination from an eleven-value coercion pool across eight product strings: 21,296 comparisons, plus long lists, empty input, overflow/rounding values and explicit extra-component/string-first cases. Allocation checks verify the two removed split copies, removed basename copy and allocation-free warmed direct version predicates for string and numeric input.

The default-feature release library suite has 729 passes, 13 failures and thirteen ignored performance benchmarks. All thirteen failures match the preceding commit's normalized diagnostics, and its retained log hash matches the committed receipt. All 8 prior value checks, 23 draw checks and 7 state-reader checks pass. The JSON retains failure/diagnostic hashes, source/executable hashes and the build-receipt hash. Normalization removes Rust source locations, workspace prefixes, temporary process/thread identifiers and the backtrace hint. Source verification checks frozen callback bodies and version code against `git show 7a4bdbb55`; the public Vec-returning version API, parser, existing tests, split/path/value backends, other compatibility code and embedded Lua remain unchanged. Existing fixtures, acceptance criteria and tolerances are unchanged.

## Reproduction

```powershell
cargo test -p deadsync-song-lua --lib --release --config profile.release.lto=false --no-run --message-format=json
python scripts/benchmark_song_lua_draw_capture.py --binary <test-executable> --baseline-commit 7a4bdbb55 --test-filter string_args::perf::benchmark_compat_args --out docs/performance/2026-10-19-song-lua-compat-args.json
```

Run the executable with `--test-threads=1` for the full suite. Each round has one warmup plus nine retained samples per variant/workload, pinned to logical CPU 2. Each sample contains 64–2,000 split calls, 2,000 basename calls, 10,000 direct version checks or 2,000 version callbacks. Windows, Xeon E5-2696 v4 (44 logical CPUs), rustc 1.98.1, release optimization with LTO disabled. No builds or other benchmarks overlap measurements.

QueryThreadCycleTime measures scheduled current-thread CPU cycles including kernel execution, not a user-only hardware PMU counter. Allocation accounting covers current-thread Rust GlobalAlloc, including mlua, and excludes direct native malloc. Accounting overhead is included. Requested bytes describe allocation/reallocation traffic rather than RSS; raw samples retain frees.
