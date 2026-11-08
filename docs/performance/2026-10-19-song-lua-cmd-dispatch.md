# Song Lua syntax dispatch, command streaming and newline counting

This pass starts from `7ae304506`, following the [borrowed-source and syntax-scan pass](2026-10-19-song-lua-cmd.md). It makes three production changes guided by M-HOTPATH in [rust-performance.md](../../rust-performance.md):

1. Dispatch the outer preprocessor by its current byte. Comments, quoted strings, long brackets and command probes have disjoint ASCII starting bytes, so other token handlers no longer run before selecting the relevant one.
2. Stream parsed command slices directly to the formatter instead of collecting them in a temporary SmallVec. The iterator validates each slice before yielding it, retains the final empty/whitespace slice for line preservation, and stops at the first error. Large command lists avoid their former heap allocation and resizes.
3. Count line feeds as bytes, using a scalar byte count for short slices and memchr counting for longer slices. Emit the known number of line feeds directly. Existing Unicode-aware trim boundaries still choose the exact same slices; counting ASCII LF bytes preserves the original character-filter result.

Borrowed/owned output choices, output capacity, grammar, operator/numeral parsing, comment removal, command order and source-line retention are unchanged. The existing loaders keep their borrowed-source integration. This pass adds no unsafe code, dependency or cache. Formatting is pure string construction: on an error the partially built output is discarded, preserving the public error contract. Streaming can construct earlier command output before encountering a later invalid command, while the collected parser validates the list first. That late-error cost is not timed separately here.

## Measurements

The [raw results and validation receipt](2026-10-19-song-lua-cmd-dispatch.json) retain six alternating-order rounds, 54 samples per variant/workload and 2,160 samples overall. Tables show medians before → after. Both variants run in one release executable. The baseline is frozen from `7ae304506`, with visibility adapters for direct quoted scans and command-list parsing, plus an adapter containing the exact old newline-count expression.

| Case | Input bytes | CPU cycles/call | Allocations/call | Reallocations/call | Requested bytes/call | Calls/second |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Spooky-door fixture | 575 | 924.09 → 928.37 | 0.00 → 0.00 | 0.00 → 0.00 | 0 → 0 | 2,343,249 → 2,356,190 |
| Runtime-eases fixture | 801 | 1,560.08 → 1,558.79 | 0.00 → 0.00 | 0.00 → 0.00 | 0 → 0 | 1,401,972 → 1,402,740 |
| Dense c/space markers | 45,056 | 363,040.99 → 225,133.24 | 0.00 → 0.00 | 0.00 → 0.00 | 0 → 0 | 6,026 → 9,728 |
| Large ordinary source | 67,618 | 43,361.55 → 42,751.05 | 0.00 → 0.00 | 0.00 → 0.00 | 0 → 0 | 50,493 → 51,249 |
| Two-command expansion | 20 | 1,649.04 → 1,664.47 | 1.00 → 1.00 | 2.00 → 2.00 | 140 → 140 | 1,319,758 → 1,314,843 |
| 512 operator-heavy commands | 14,860 | 235,403.46 → 211,784.91 | 2.00 → 1.00 | 7.00 → 1.00 | 77,092 → 44,580 | 9,297 → 10,342 |
| 512 commented command pairs | 12,300 | 320,535.19 → 309,532.73 | 3.00 → 2.00 | 8.00 → 1.00 | 114,468 → 49,188 | 6,826 → 7,072 |
| Fixture through public String API | 575 | 1,085.92 → 1,018.19 | 1.00 → 1.00 | 0.00 → 0.00 | 575 → 575 | 2,001,173 → 2,137,341 |
| Newline-heavy two-command source | 10,259 | 119,708.09 → 106,498.66 | 1.00 → 1.00 | 0.00 → 0.00 | 10,259 → 10,259 | 18,262 → 20,464 |

Sources use the regular loaders' borrowed/owned preprocessor unless the row explicitly names the public String API. Temporary output and errors are dropped inside timing. The dense-marker control is the same repeated c/space input used in the preceding pass; it is not valid Lua. Its median throughput changes by +61.4%, with per-round changes from +57.9% to +66.3%. Operator-heavy command preprocessing changes throughput by +11.2%, with per-round changes from -1.2% to +16.9%.

The two repository fixture preprocessing cases change throughput by +0.6% and +0.1%. The ordinary-source case changes by +1.5%, and the commented-command case by +3.6%. No-expansion and small-command allocation traffic is unchanged. The two large transformed cases each remove one temporary allocation and its resizes; their output strings retain the original capacity and formatting policy.

| Case | Input bytes | CPU cycles/call | Allocations/call | Reallocations/call | Requested bytes/call | Calls/second |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Short ASCII whitespace | 6 | 10.77 → 7.93 | 0.00 → 0.00 | 0.00 → 0.00 | 0 → 0 | 202,429,150 → 273,972,603 |
| Long ASCII whitespace | 24,576 | 34,193.12 → 1,876.04 | 0.00 → 0.00 | 0.00 → 0.00 | 0 → 0 | 63,572 → 1,161,525 |
| Long Unicode whitespace/text | 45,056 | 59,337.88 → 3,662.91 | 0.00 → 0.00 | 0.00 → 0.00 | 0 → 0 | 36,931 → 597,294 |
| UTF-8 text without line feeds | 28,672 | 39,093.29 → 2,186.42 | 0.00 → 0.00 | 0.00 → 0.00 | 0 → 0 | 56,057 → 998,440 |

Direct newline scans compare the original UTF-8 character/filter count with the byte counter, retaining zero allocation in both variants. The short, long ASCII, Unicode and no-line-feed cases change median throughput by +35.3%, +1727.1%, +1517.3% and +1681.1%. The complete newline-heavy two-command source changes throughput by +12.1%, with per-round changes from +11.5% to +13.2%.

| Case | Input bytes | CPU cycles/call | Allocations/call | Reallocations/call | Requested bytes/call | Calls/second |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Unchanged short quoted scan | 15 | 29.22 → 29.23 | 0.00 → 0.00 | 0.00 → 0.00 | 0 → 0 | 74,934,432 → 74,906,367 |
| Unchanged 64-KiB quoted scan | 65,538 | 3,194.75 → 3,119.30 | 0.00 → 0.00 | 0.00 → 0.00 | 0 → 0 | 683,943 → 698,499 |

The quoted algorithm is unchanged; these rows are controls. Their median throughput changes by -0.0% and +2.1%. Both retain zero allocation.

| Case | Input bytes | CPU cycles/call | Allocations/call | Reallocations/call | Requested bytes/call | Calls/second |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Four commands, fits original inline storage | 36 | 228.39 → 274.81 | 0.00 → 0.00 | 0.00 → 0.00 | 0 → 0 | 9,578,544 → 7,964,954 |
| 512 commands, original heap storage | 4,608 | 27,720.46 → 32,887.28 | 1.00 → 0.00 | 6.00 → 0.00 | 32,512 → 0 | 78,910 → 66,577 |

One command-list call parses and consumes all slices from a four-command or 512-command body, including the final empty/whitespace slice. It excludes output formatting. The baseline builds its SmallVec before consumption; the optimized iterator yields each slice directly. The small case has no allocation in either variant. The large case removes one allocation and all list resizes. Its median throughput changes by -15.6%, with per-round changes from -19.5% to -11.1%.

| Case | Input bytes | CPU cycles/call | Allocations/call | Reallocations/call | Requested bytes/call | Calls/second |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Spooky-door fixture | 575 | 53,012.69 → 53,369.34 | 45.38 → 45.38 | 42.00 → 42.00 | 10,808 → 10,808 | 41,325 → 40,803 |
| Large ordinary source | 67,618 | 2,965,554.88 → 2,919,295.19 | 16.00 → 16.00 | 29.00 → 29.00 | 165,298 → 165,298 | 739 → 748 |
| Two-command expansion | 20 | 14,072.01 → 13,768.05 | 25.00 → 25.00 | 15.00 → 15.00 | 2,286 → 2,286 | 154,828 → 158,426 |

Chunk preparation includes preprocessing, Lua compilation with a stable name and Function handle drop. A fresh VM is set up and one function compiled before each sample. Final VM teardown is excluded; collection occurring inside the loop is timed. Functions are compiled without execution or file I/O. Fixture, large-source and two-command preparation change median throughput by -1.3%, +1.3% and +2.3%. The measurements establish effects for these helpers and inputs, without a whole-song load-time or gameplay-FPS claim.

## Behavior and validation

Four new release checks pass. Command-list comparison covers 2,744 combinations of fourteen body fragments, plus grouped/function arguments, Unicode strings, missing arguments/terminators, leading/trailing newlines and repeated empty commands. It compares every borrowed slice and the first error with the frozen collected parser; the iterator ends after an error. Source checks compare exact bytes, errors and borrowed/owned choices for 2,744 generated combinations, repository fixtures, comments, malformed strings and Unicode/NUL input. Newline checks compare counts and emitted bytes across short/long ASCII, Unicode, CRLF and no-line-feed text. Quoted controls retain boundary/escape/Unicode checks against the unchanged algorithm. VM checks compare dumped bytecode, returned values and syntax/runtime diagnostics after temporary source storage is dropped. Allocation checks verify unchanged small/no-expansion contracts, zero-allocation list consumption, and the removed allocation/resizes in a 512-command expansion.

The default-feature release library suite has 717 passes, 13 failures and ten ignored performance benchmarks. Every failure matches the preceding commit's normalized diagnostic; the retained baseline log hash matches its committed receipt. All four new checks pass. Normalization removes Rust source locations, workspace prefixes, temporary process/thread identifiers and the backtrace hint. The JSON records individual failure/diagnostic hashes, source/executable hashes and the build-receipt hash. The frozen parser and newline expression were checked against `git show 7ae304506`. Operator/expression parsing, other grammar helpers, the quoted-scan algorithm, existing parser tests and embedded Lua were verified unchanged. Formatter changes were verified as iterator consumption plus byte-equivalent newline counting/emission on the original slices. Native fixtures, acceptance criteria and tolerances are unchanged.

## Reproduction

```powershell
cargo test -p deadsync-song-lua --lib --release --config profile.release.lto=false --no-run --message-format=json
python scripts/benchmark_song_lua_draw_capture.py --binary <test-executable> --baseline-commit 7ae304506 --test-filter cmd::dispatch_perf::benchmark_cmd_dispatch --out docs/performance/2026-10-19-song-lua-cmd-dispatch.json
```

Run the executable with `--test-threads=1` for the full suite. Each round has one warmup plus nine retained samples per variant/workload, pinned to logical CPU 2. Samples repeatedly use the same in-memory source: 64–512 preprocesses, 128 or 20,000 quoted/newline scans, 64 or 2,000 complete command-list parses, or 8–256 chunk preparations. Fixture setup and final destruction are excluded; temporary results and Function handle drops are timed. Both variants use the same counted System allocator on Windows, Xeon E5-2696 v4 (44 logical CPUs), rustc 1.98.1, release optimization with LTO disabled. No builds or other benchmarks overlap measurements.

QueryThreadCycleTime measures scheduled current-thread CPU cycles including kernel execution, not a user-only hardware PMU counter. Allocation accounting covers current-thread Rust GlobalAlloc, including mlua, and excludes direct native malloc. Accounting overhead is included. Requested bytes describe allocation/reallocation traffic rather than RSS; raw samples also retain frees.
