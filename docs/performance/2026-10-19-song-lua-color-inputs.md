# Song Lua fixed named colors, borrowed color names and option defaults

This pass starts from `552763528`, following the [text-formatting pass](2026-10-19-song-lua-text-format.md). It makes three optimizations guided by M-HOTPATH in [rust-performance.md](../../rust-performance.md):

1. Decode fixed named-color literals during compilation. Custom difficulty, stage and judgment lookup arms return the same precomputed RGBA values. Dark player colors and the Edit difficulty constant use the same path. The private constant decoder validates the existing literals and performs the same u8-to-f32 division by 255.
2. Borrow Lua string names in the three difficulty/stage/judgment value adapters. This removes the temporary Rust String copy while retaining exact name matching, UTF-8 validation, type checks and fallback results. Steps/trail lookup still invokes GetDifficulty through the unchanged code path.
3. Match built-in option-row names with ASCII-insensitive comparisons. This removes the owned lowercase input copy. The returned String, existing custom option lookup and original-name fallback remain unchanged.

The general color parser and palette routines are unchanged, including numeric CSV rules, plus-sign acceptance in radix parsing and existing UTF-8 boundary behavior. Named-color aliases and letter case are unchanged. Borrowed string guards keep their Lua roots alive until the lookup completes; no borrowed data escapes into results. Option-name matching keeps the original ASCII-only case behavior. No dependency, unsafe code or mutable cache is added.

## Measurements

The [raw measurements and validation receipt](2026-10-19-song-lua-color-inputs.json) retain six alternating-order rounds and 54 samples per variant/workload: 3,024 retained samples across 28 workloads. Tables show medians before → after. Both variants use the same release executable and counted System allocator. The baseline freezes core color code and option-default function bodies from `552763528`, with parent visibility adapters for the four private value/getter helpers.

| Case | CPU cycles/call | Allocations/call | Reallocations/call | Requested bytes/call | Calls/second | Throughput change |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Custom difficulty Hard | 40.96 → 17.07 | 0.00 → 0.00 | 0.0000 → 0.0000 | 0.00 → 0.00 | 53,526,027 → 128,287,364 | +139.7% |
| Stage_1st | 41.74 → 15.52 | 0.00 → 0.00 | 0.0000 → 0.0000 | 0.00 → 0.00 | 52,522,388 → 140,815,321 | +168.1% |
| JudgmentLine_W1 | 41.24 → 15.47 | 0.00 → 0.00 | 0.0000 → 0.0000 | 0.00 → 0.00 | 52,782,983 → 141,342,756 | +167.8% |
| Dark player zero | 44.10 → 4.24 | 0.00 → 0.00 | 0.0000 → 0.0000 | 0.00 → 0.00 | 49,573,666 → 510,594,843 | +930.0% |
| Edit difficulty five | 40.94 → 5.38 | 0.00 → 0.00 | 0.0000 → 0.0000 | 0.00 → 0.00 | 53,543,223 → 393,313,668 | +634.6% |
| Unknown stage, unchanged result | 14.30 → 13.73 | 0.00 → 0.00 | 0.0000 → 0.0000 | 0.00 → 0.00 | 153,127,632 → 159,375,249 | +4.1% |

One named-color call takes a prepared string or scalar and returns copied RGBA data. The custom, stage and judgment inputs are `Hard`, `Stage_1st` and `JudgmentLine_W1`. Dark player index zero and difficulty index five exercise the two fixed-color branches. Unknown stage input is `Stage_Unknown`. The stage lookup changes throughput by +168.1% and cycles by -62.8%, with per-round throughput changes from +139.4% to +200.2%. All direct lookups remain allocation-free. These primitive results do not include Lua table construction.

| Case | CPU cycles/call | Allocations/call | Reallocations/call | Requested bytes/call | Calls/second | Throughput change |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Lua name Hard | 453.11 → 274.00 | 1.00 → 0.00 | 0.0000 → 0.0000 | 8.00 → 0.00 | 4,820,439 → 7,978,299 | +65.5% |
| Unknown difficulty name | 420.69 → 274.84 | 1.00 → 0.00 | 0.0000 → 0.0000 | 8.00 → 0.00 | 5,186,050 → 7,938,083 | +53.1% |
| Lua name Stage_1st | 464.42 → 274.86 | 1.00 → 0.00 | 0.0000 → 0.0000 | 9.00 → 0.00 | 4,711,980 → 7,937,138 | +68.4% |
| Unknown stage name | 430.80 → 277.52 | 1.00 → 0.00 | 0.0000 → 0.0000 | 8.00 → 0.00 | 5,081,688 → 7,888,302 | +55.2% |
| Lua name JudgmentLine_W1 | 472.08 → 300.58 | 1.00 → 0.00 | 0.0000 → 0.0000 | 15.00 → 0.00 | 4,620,965 → 7,287,833 | +57.7% |
| Unknown judgment name | 422.27 → 277.59 | 1.00 → 0.00 | 0.0000 → 0.0000 | 8.00 → 0.00 | 5,168,760 → 7,887,679 | +52.6% |
| Invalid UTF-8, retained error construction | 829.63 → 826.42 | 2.00 → 2.00 | 1.0000 → 1.0000 | 82.00 → 82.00 | 2,641,833 → 2,651,500 | +0.4% |
| Integer input, unchanged control | 14.29 → 15.04 | 0.00 → 0.00 | 0.0000 → 0.0000 | 0.00 → 0.00 | 151,285,930 → 143,884,892 | -4.9% |

One value-adapter read clones a prepared Value handle, validates its string and returns an optional color. Known and unknown valid string names remove one temporary allocation. Recognized stage reads change throughput by +68.4% and cycles by -40.8%; per-round throughput changes range from +46.1% to +78.8%. Valid names become allocation-free. Invalid input contains 32 bytes of 0xFF and still produces None through the same two allocations and one reallocation used to construct discarded UTF-8 errors. Nonstring inputs remain uncoerced and return None. The integer control changes throughput by -4.9% and elapsed time from 6.61 ns to 6.95 ns; its per-round throughput changes range from -10.2% to -4.9%. The source of that dispatch/code-layout effect was not isolated.

| Case | CPU cycles/call | Allocations/call | Reallocations/call | Requested bytes/call | Calls/second | Throughput change |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| CustomDifficultyToColor pattern | 3,047.92 → 2,710.06 | 5.00 → 4.00 | 0.0000 → 0.0000 | 192.00 → 184.00 | 718,946 → 808,293 | +12.4% |
| StageToColor pattern | 2,897.95 → 2,715.00 | 5.00 → 4.00 | 0.0000 → 0.0000 | 193.00 → 184.00 | 750,962 → 806,517 | +7.4% |
| JudgmentLineToStrokeColor pattern | 2,747.04 → 2,603.43 | 5.00 → 4.00 | 0.0000 → 0.0000 | 199.00 → 184.00 | 793,667 → 841,043 | +6.0% |
| StepsOrTrailToColor with GetDifficulty | 3,841.91 → 3,517.87 | 6.00 → 5.00 | 0.0005 → 0.0005 | 208.02 → 200.02 | 569,784 → 616,837 | +8.3% |

These callbacks match the production MultiValue/value-adapter/make_color_table patterns. The judgment callback includes unchanged tone_color processing. The steps/trail case receives a table whose authored GetDifficulty method returns Hard; getter calls and output table construction are inside timing. Custom and stage callbacks use the recognized names above. All return four numeric table components, with the original white or black fallbacks for unsupported values. The stage callback changes throughput by +7.4% and cycles by -6.3%, with per-round throughput changes from -10.4% to +12.7%. All four remove one Rust allocation; output and mlua argument allocations remain. The getter callback retains the same 0.0005 reallocations per call in both variants. The table and raw samples retain this small shared cost; its source was not isolated.

| Case | CPU cycles/call | Allocations/call | Reallocations/call | Requested bytes/call | Calls/second | Throughput change |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| SpeedModType | 251.93 → 162.87 | 2.00 → 1.00 | 0.0000 → 0.0000 | 13.00 → 1.00 | 8,676,790 → 13,460,762 | +55.1% |
| Mini | 253.37 → 150.59 | 2.00 → 1.00 | 0.0000 → 0.0000 | 6.00 → 2.00 | 8,625,895 → 14,368,848 | +66.6% |
| Perspective | 251.47 → 162.64 | 2.00 → 1.00 | 0.0000 → 0.0000 | 19.00 → 8.00 | 8,664,010 → 13,464,387 | +55.4% |
| NoteSkinVariant | 261.70 → 166.80 | 2.00 → 1.00 | 0.0000 → 0.0000 | 22.00 → 7.00 | 8,218,615 → 13,110,456 | +59.5% |
| JudgmentGraphic custom fallback | 334.64 → 203.58 | 2.00 → 1.00 | 0.0000 → 0.0000 | 19.00 → 4.00 | 6,505,969 → 10,755,579 | +65.3% |
| Unknown short name | 195.85 → 55.63 | 1.00 → 0.00 | 0.0000 → 0.0000 | 7.00 → 0.00 | 10,953,502 → 39,370,079 | +259.4% |
| Empty name control | 48.78 → 40.64 | 0.00 → 0.00 | 0.0000 → 0.0000 | 0.00 → 0.00 | 44,583,148 → 53,821,313 | +20.7% |
| 7-KiB unknown name | 2,310.90 → 1,187.26 | 2.00 → 1.00 | 0.0000 → 0.0000 | 14,336.00 → 7,168.00 | 946,862 → 1,845,376 | +94.9% |

One option-default call accepts a prepared Rust str and constructs/drops its returned String. Built-in and custom-fallback cases remove one lowercase input allocation. SpeedModType changes throughput by +55.1% and cycles by -35.4%, with per-round throughput changes from +39.7% to +67.1%. The custom JudgmentGraphic case still delegates to the existing custom lookup, changing throughput by +65.3%. Unknown short names return an empty String without allocation. The 7,168-byte long unknown name still uses one allocation inside the unchanged custom-lookup normalization path; it previously needed two. These helper measurements exclude option-row actor/frame creation, which is a production caller.

| Case | CPU cycles/call | Allocations/call | Reallocations/call | Requested bytes/call | Calls/second | Throughput change |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Unchanged #FF5D47 parser | 37.18 → 36.42 | 0.00 → 0.00 | 0.0000 → 0.0000 | 0.00 → 0.00 | 57,820,179 → 60,024,010 | +3.8% |
| Unchanged numeric CSV parser | 229.56 → 230.62 | 0.00 → 0.00 | 0.0000 → 0.0000 | 0.00 → 0.00 | 9,414,423 → 9,429,514 | +0.2% |

The parser controls use #FF5D47 and 1,.5,.25,1. Their algorithms are byte-for-byte unchanged; timing differences in these rows are not attributed to parser optimization. All raw samples and per-round statistics remain available.

Negative aggregated throughput changes: `name_nonstring` -4.9% (rounds -10.2% to -4.9%)

Every sample warms the relevant direct read or callback before timing, including Lua handle storage. Callbacks are warmed four times. Fixture/VM construction and final VM teardown are excluded. Returned String destruction and Lua handle drops are included; any collection during timed calls is included. All samples repeatedly use the same prepared inputs. These are helper and warmed callback results, without a whole-song load-time or gameplay-FPS claim. Rust allocation accounting does not measure direct native Lua malloc.

## Behavior and validation

Four new release checks pass. Named-color checks compare exact f32 bits for fifty name cases, including 41 recognized spellings and nine boundary/unknown cases, plus mutated names, dark-player fallbacks and both decorative modes over difficulties -16 through 16. Static source verification checks every existing literal and all 43 aliases. Borrowed-name checks retain nil/boolean/integer/NaN/table behavior, invalid UTF-8, embedded NUL and string roots across explicit garbage collection. Steps/trail checks compare returned colors and exact authored-call counts for ordinary getters, __index-provided getters, errors and nonfunction fields. Option checks compare original/upper/lower/mixed-case spellings, whitespace, custom fallbacks, empty/NUL/Unicode and long unknown names. Allocation checks verify removal of temporary name and lowercase Strings while preserving owned outputs.

The default-feature release library suite has 737 passes, 13 failures and fifteen ignored performance benchmarks. All thirteen failures match the preceding commit's normalized diagnostics, and the retained baseline log hash matches its committed receipt. All 16 prior value/argument/formatting checks, 23 draw checks and 7 state-reader checks pass. The JSON retains individual diagnostic hashes, source/executable hashes and the build-receipt hash. Normalization removes Rust source locations, workspace prefixes, temporary process/thread identifiers and the backtrace hint. Source verification checks frozen code against `git show 552763528`, confirms unchanged literals/aliases/fallbacks and proves that the general parser, palettes, getter logic, callback bodies, existing tests and embedded Lua are unchanged. Existing fixtures, tolerances and acceptance criteria are unchanged.

## Reproduction

```powershell
cargo test -p deadsync-song-lua --lib --release --config profile.release.lto=false --no-run --message-format=json
python scripts/benchmark_song_lua_draw_capture.py --binary <test-executable> --baseline-commit 552763528 --test-filter theme_colors::input_perf::benchmark_color_inputs --out docs/performance/2026-10-19-song-lua-color-inputs.json
```

Run the executable with --test-threads=1 for the full suite. Each round has one warmup plus nine retained samples per variant/workload, pinned to logical CPU 2. Each sample performs 100,000 direct fixed-color lookups, 10,000 value/option/parser reads or 2,000 Lua callback calls. Windows, Xeon E5-2696 v4 (44 logical CPUs), rustc 1.98.1, release optimization with LTO disabled. No builds or other benchmarks overlap measurements.

QueryThreadCycleTime measures scheduled current-thread CPU cycles including kernel execution, not a user-only hardware PMU counter. Allocation accounting covers current-thread Rust GlobalAlloc, including mlua, and excludes direct native malloc. Accounting overhead is included. Requested bytes describe allocation/reallocation traffic rather than RSS; raw samples retain frees.
