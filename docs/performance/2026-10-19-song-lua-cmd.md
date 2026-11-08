# Song Lua command preprocessing and quoted-string scanning

This pass starts from `77d08fc56`, following the [file-open reuse pass](2026-10-19-song-lua-file-open.md). It makes three production changes guided by M-HOTPATH and M-INITIAL-CAPACITY in [rust-performance.md](../../rust-performance.md):

1. Delay allocation until the first real `cmd(...)` expansion. A crate-private preprocessor returns borrowed source when no expansion occurs. The regular file loader and `loadstring` compatibility binding keep that borrow through Lua compilation. The public `preprocess_lua_cmd_syntax` API still returns an owned `String` with identical bytes and errors.
2. Skip ordinary source in byte spans between the five ASCII syntax markers. A 16-byte scalar prefix handles nearby markers, followed by 64-byte windows searched with the existing `memchr` dependency. Bounding both delimiter searches avoids quadratic rescanning when one marker set is absent. Marker positions and EOF are UTF-8 boundaries.
3. Keep the scalar quoted-string loop for its first 32 bytes, then search for quote/backslash delimiters with `memchr2`. Eight consecutive nearby escapes select the original scalar loop for the rest of that literal. Escape priority, byte skipping and unterminated-string errors remain unchanged.

The parser still validates quoted and long-string boundaries even when there is no command expansion. Grammar helpers, comment removal, command argument parsing, command order and source-line retention are unchanged. The implementation adds no unsafe code or dependency. Borrowed input remains alive through synchronous Lua compilation; the resulting Function owns the compiled prototype. The existing noteskin call uses the unchanged public String API.

## Measurements

The [raw results and validation receipt](2026-10-19-song-lua-cmd.json) contain six alternating-order rounds, 54 retained samples per variant/workload and 2,160 samples overall. Tables show medians before → after. Both variants run in one release executable. The baseline parser is frozen from `77d08fc56`, with only quoted-scanner visibility adjusted for direct checks.

| Case | Input bytes | CPU cycles/call | Allocations/call | Reallocations/call | Requested bytes/call | MiB/second |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Spooky-door fixture, loader preprocessing | 575 | 3,858.61 → 963.31 | 1.00 → 0.00 | 0.00 → 0.00 | 575 → 0 | 311.39 → 1,240.11 |
| Runtime-eases fixture, loader preprocessing | 801 | 5,114.95 → 1,587.09 | 1.00 → 0.00 | 0.00 → 0.00 | 801 → 0 | 327.15 → 1,052.79 |
| Large ordinary source, loader preprocessing | 67,618 | 415,390.03 → 42,903.67 | 1.00 → 0.00 | 0.00 → 0.00 | 67,618 → 0 | 339.34 → 3,289.42 |
| Large ordinary quoted literal, loader preprocessing | 77,850 | 180,185.50 → 3,729.79 | 1.00 → 0.00 | 0.00 → 0.00 | 77,850 → 0 | 902.09 → 43,393.49 |
| Dense markers, loader preprocessing | 45,056 | 278,250.56 → 352,722.76 | 1.00 → 0.00 | 0.00 → 0.00 | 45,056 → 0 | 338.30 → 266.26 |
| Two-command expansion | 20 | 1,150.23 → 1,099.64 | 1.00 → 1.00 | 2.00 → 2.00 | 140 → 140 | 36.31 → 37.98 |
| 512 pairs of commented commands | 12,300 | 312,811.49 → 294,505.55 | 3.00 → 3.00 | 8.00 → 8.00 | 114,468 → 114,468 | 82.07 → 87.07 |
| Command with long literal | 77,845 | 163,316.56 → 26,799.57 | 1.00 → 1.00 | 1.00 → 1.00 | 233,535 → 233,535 | 996.40 → 6,063.02 |
| Command with escaped literal | 49,173 | 137,467.02 → 138,015.78 | 1.00 → 1.00 | 1.00 → 1.00 | 147,519 → 147,519 | 746.34 → 743.19 |
| Unterminated literal error | 65,544 | 59,525.65 → 3,378.24 | 2.00 → 1.00 | 0.00 → 0.00 | 65,567 → 23 | 2,290.97 → 40,307.19 |
| Spooky-door fixture, public String API | 575 | 3,591.96 → 1,152.80 | 1.00 → 1.00 | 0.00 → 0.00 | 575 → 575 | 333.84 → 1,038.51 |
| Large ordinary source, public String API | 67,618 | 419,829.77 → 50,174.62 | 1.00 → 1.00 | 0.00 → 0.00 | 67,618 → 67,618 | 335.96 → 2,814.14 |
| Dense markers, public String API | 45,056 | 276,823.81 → 344,872.22 | 1.00 → 1.00 | 0.00 → 0.00 | 45,056 → 45,056 | 338.80 → 272.57 |

Loader preprocessing compares the original owned output with the new borrowed/owned result used by the regular loaders. Public String cases exercise the retained API, including its required output copy for unchanged text. Temporary results, output buffers and errors are dropped inside timing. Empty syntax-free output is covered by correctness tests. The dense-marker control deliberately uses repeated `c` tokens separated by spaces; it is preprocessed but is not valid Lua. Its loader-helper median throughput changes by -21.3%, with per-round changes from -27.3% to -17.4%.

Spooky-door fixture preprocessing changes throughput by +298.3% and CPU cycles by -75.0%; per-round throughput changes range from +218.6% to +353.5%. Runtime-eases fixture preprocessing changes throughput by +221.8%. No-expansion loader cases remove the old output allocation. Transformed output retains owned storage. Unterminated ordinary literals avoid allocating the unused output buffer before returning the same error.

| Case | Input bytes | CPU cycles/call | Allocations/call | Reallocations/call | Requested bytes/call | MiB/second |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Long literal | 65,538 | 59,486.21 → 3,269.35 | 0.00 → 0.00 | 0.00 → 0.00 | 0 → 0 | 2,283.60 → 41,787.64 |
| Dense escapes | 24,578 | 20,948.53 → 21,092.58 | 0.00 → 0.00 | 0.00 → 0.00 | 0 → 0 | 2,451.18 → 2,434.28 |
| Small literal | 15 | 26.23 → 28.68 | 0.00 → 0.00 | 0.00 → 0.00 | 0 → 0 | 1,193.58 → 1,091.99 |
| Unterminated literal | 65,537 | 59,547.09 → 3,367.10 | 1.00 → 1.00 | 0.00 → 0.00 | 23 → 23 | 2,300.31 → 40,589.15 |

Direct quoted scans isolate the third change. They return an end byte position or the original error; they do not create output source. The small-literal control changes median throughput by -8.5%, and the dense-escape control by -0.7%. The long-literal case changes throughput by +1729.9%, with per-round changes from +1664.9% to +1741.1%.

| Case | Input bytes | CPU cycles/call | Allocations/call | Reallocations/call | Requested bytes/call | MiB/second |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Spooky-door fixture | 575 | 57,176.31 → 53,544.28 | 46.00 → 45.00 | 42.00 → 42.00 | 11,334 → 10,759 | 20.92 → 22.27 |
| Large ordinary source | 67,618 | 3,471,186.69 → 3,056,057.38 | 17.00 → 16.00 | 29.00 → 29.00 | 232,881 → 165,263 | 40.59 → 46.12 |
| Two-command expansion | 20 | 14,036.00 → 14,132.03 | 25.02 → 25.02 | 15.00 → 15.00 | 2,251 → 2,251 | 2.96 → 2.95 |

Prepared-chunk cases include preprocessing, Lua bytecode compilation with a stable chunk name, and Function handle drop. A fresh Lua VM is constructed and one function compiled before each timed sample. Final VM teardown is excluded; collection that occurs inside the loop is timed. These cases do not execute the function or read a file. Spooky-door preparation changes throughput by +6.5%, large-source preparation by +13.6%, and two-command preparation by -0.3%. They establish scoped parser/preparation effects, without a whole-song load-time or gameplay-FPS claim.

## Behavior and validation

Four new release checks pass. The byte/error comparison covers 2,744 generated ASCII scanner combinations, two repository fixtures, Unicode/NUL-containing source, comments and long strings containing fake commands, comments around command parentheses, and every UTF-8 truncation boundary of a mixed quoted/command sample. Quoted scans compare scalar-prefix and search-window boundary lengths, short/long escaped strings, Unicode bytes, missing terminators and every byte offset of a mixed sample. VM checks compare dumped bytecode, returned values, parser errors, Lua syntax errors and runtime stack diagnostics after temporary source buffers are dropped. Allocation checks verify zero allocation for unchanged borrowed output, the retained public String allocation, owned transformed output and the removed unused allocation on an unterminated literal error.

The default-feature release library suite has 713 passes, 13 failures and nine ignored performance benchmarks. Every failure matches the preceding commit's normalized diagnostic, and its retained baseline log hash matches the committed receipt. All four new checks pass. Normalization removes Rust source locations, workspace prefixes, temporary process/thread identifiers and the backtrace hint. The JSON records failure and diagnostic hashes, source/executable hashes and the build-receipt hash. Frozen parser source was checked against `git show 77d08fc56`. The remaining parser/grammar helpers and existing parser tests were verified unchanged; loader changes were verified as the intended two call-site edits, with embedded Lua unchanged. Existing native fixtures, acceptance criteria and tolerances are unchanged.

After measurement, one blank line at EOF was removed from the frozen fixture to pass whitespace checks. `source_sha256` retains the exact measured inputs; `committed_source_sha256` records the final files. The receipt verifies that appending the recorded CRLF to the committed fixture reproduces its measured hash exactly. No code or token-bearing line changed, so compilation, tests and timing were not repeated for that normalization.

## Reproduction

```powershell
cargo test -p deadsync-song-lua --lib --release --config profile.release.lto=false --no-run --message-format=json
python scripts/benchmark_song_lua_draw_capture.py --binary <test-executable> --baseline-commit 77d08fc56 --test-filter cmd::perf::benchmark_cmd --out docs/performance/2026-10-19-song-lua-cmd.json
```

Run the executable with `--test-threads=1` for the full suite. Each round performs one warmup plus nine retained samples per variant/workload, pinned to logical CPU 2. Samples repeatedly scan the same in-memory source: 64–512 preprocesses, 128 or 20,000 direct quoted scans, or 8–256 chunk preparations. Fixture setup and final destruction are excluded; temporary returned output/error and Function handle drops are timed. Source sizes and individual samples are retained in JSON. Both variants use the same counted System allocator on Windows, Xeon E5-2696 v4 (44 logical CPUs), rustc 1.98.1, release optimization with LTO disabled. No builds or other benchmarks overlap measurements.

QueryThreadCycleTime measures scheduled current-thread CPU cycles including kernel execution, not a user-only hardware PMU counter. Allocation accounting covers current-thread Rust GlobalAlloc, including mlua, and excludes direct native malloc. Accounting overhead is included. Requested bytes represent allocation/reallocation traffic rather than RSS; raw samples also retain frees. MiB/second equals input bytes times completed calls/second divided by 1,048,576, including the error and invalid-source controls.
