# Song Lua file-open path and snapshot reuse

This pass starts from `6a09e355d`, following the [buffered-read and virtual-store pass](2026-10-19-song-lua-rage-file.md). It makes three changes guided by M-HOTPATH and M-INITIAL-CAPACITY in [rust-performance.md](../../rust-performance.md):

1. Borrow path text when it contains no backslashes. Backslash input retains the existing replacement expression and owned result.
2. Normalize ordinary relative paths directly from the base and input components into the handle's existing `PathBuf`. Reserve an upper bound before appending components. Rooted, drive-relative and verbatim-base cases retain standard `Path::join` behavior; absolute inputs can be borrowed while their components are collapsed into the reused output.
3. Use `Vec::clone_from` for virtual closed-file reads, copying bytes into each handle's existing buffer. Open handles keep private snapshots, and physical-file fallback still performs a fresh `std::fs::read` on each open.

The open operation still records its side effect, closes prior output, resolves its path, resets cursor/EOF and clears the buffer before checking modes and loading bytes. The checks, error strings and ordering remain intact. There is no new path or file-content cache, unsafe code or dependency. These helpers run in the song Lua worker's compile/replay path.

## Measurements

The [raw results and validation receipt](2026-10-19-song-lua-file-open.json) contain six alternating-order rounds, 54 retained samples per variant/workload and 1,944 samples overall. Tables show medians before → after. Both variants run in one release executable. The frozen baseline is the implementation from `6a09e355d`, with visibility adapters and an adapter for its exact separator-replacement expression.

| Case | CPU cycles/conversion | Allocations/conversion | Reallocations/conversion | Requested bytes/conversion | Conversions/second |
| --- | ---: | ---: | ---: | ---: | ---: |
| ASCII name, no backslashes | 125.58 → 18.12 | 1.00 → 0.00 | 0.00 → 0.00 | 13 → 0 | 17,455,815 → 120,554,551 |
| UTF-8 name, no backslashes | 134.74 → 20.29 | 1.00 → 0.00 | 0.00 → 0.00 | 21 → 0 | 16,270,745 → 107,816,712 |
| Backslash name | 135.84 → 151.97 | 1.00 → 1.00 | 0.00 → 0.00 | 23 → 23 | 16,095,284 → 14,354,926 |

These cases isolate the separator expression. Clean ASCII and UTF-8 names avoid the old allocation. Backslash names still allocate; their median throughput changes by -10.8%, with per-round changes from -13.4% to +15.8%. The extra detection step is included.

| Case | CPU cycles/path | Allocations/path | Reallocations/path | Requested bytes/path | Paths/second |
| --- | ---: | ---: | ---: | ---: | ---: |
| Warm relative path | 2,925.72 → 801.61 | 3.00 → 0.00 | 3.00 → 0.00 | 207 → 0 | 749,513 → 2,728,513 |
| Warm relative path with parent component | 3,272.47 → 1,172.95 | 3.00 → 0.00 | 3.00 → 0.00 | 217 → 0 | 669,355 → 1,834,862 |
| Warm backslash path | 3,318.40 → 1,319.63 | 3.00 → 1.00 | 3.00 → 0.00 | 217 → 23 | 660,677 → 1,659,338 |
| Warm absolute path | 2,116.25 → 766.11 | 3.00 → 0.00 | 2.00 → 0.00 | 191 → 0 | 1,034,956 → 2,857,959 |
| Warm drive-relative path | 1,603.50 → 1,247.47 | 3.00 → 1.00 | 1.00 → 0.00 | 70 → 31 | 1,367,428 → 1,755,772 |
| Warm path under verbatim base | 4,338.86 → 4,232.56 | 11.00 → 9.00 | 5.00 → 5.00 | 1,119 → 1,088 | 503,151 → 517,505 |
| Fresh relative output buffer | 2,935.81 → 968.00 | 3.00 → 1.00 | 3.00 → 0.00 | 207 → 45 | 746,547 → 2,251,238 |

Warm optimized path cases reuse one output buffer; the baseline returns and drops its original fresh output. The fresh-buffer case creates and drops a new output every iteration for both variants, so it includes the new reservation cost. Relative paths skip the joined-path temporary. Special platform prefixes use the original standard-library join before the same lexical collapse; the base itself is never converted to UTF-8. These are lexical path operations without filesystem calls.

| Case | CPU cycles/copy | Allocations/copy | Reallocations/copy | Requested bytes/copy | Copies/second |
| --- | ---: | ---: | ---: | ---: | ---: |
| 64-byte snapshot | 307.90 → 187.67 | 1.00 → 0.00 | 0.00 → 0.00 | 64 → 0 | 7,069,636 → 11,604,294 |
| 64-KiB snapshot | 40,113.63 → 6,413.51 | 1.00 → 0.00 | 0.00 → 0.00 | 65,536 → 0 | 54,597 → 340,924 |

Snapshot cases isolate virtual-store lookup and copying bytes. The baseline allocates and drops a fresh vector on each copy; the optimized path overwrites a pre-sized private buffer. Setup and final buffer destruction are excluded. Both cases copy the entire file; later writes remain unable to change an already-open reader's bytes.

| Case | CPU cycles/open | Allocations/open | Reallocations/open | Requested bytes/open | Opens/second |
| --- | ---: | ---: | ---: | ---: | ---: |
| One file, 64 bytes | 4,314.97 → 3,155.74 | 5.00 → 1.00 | 3.00 → 0.00 | 287 → 16 | 506,154 → 690,725 |
| Last of 256 files, 64 bytes | 4,615.93 → 3,364.09 | 5.00 → 1.00 | 3.00 → 0.00 | 287 → 16 | 473,526 → 649,952 |
| One file, 64 KiB | 29,124.05 → 9,008.07 | 5.00 → 1.00 | 3.00 → 0.00 | 65,759 → 16 | 74,681 → 242,103 |
| Parent-component alias, 64 bytes | 4,938.11 → 3,632.47 | 5.00 → 1.00 | 3.00 → 0.00 | 297 → 16 | 441,703 → 598,725 |
| Backslash alias, 64 bytes | 4,848.72 → 3,843.18 | 5.00 → 2.00 | 3.00 → 0.00 | 297 → 39 | 449,892 → 569,712 |
| Alternate 5-byte and 64-KiB files | 14,011.13 → 6,266.90 | 5.00 → 1.00 | 3.00 → 0.00 | 32,994 → 16 | 154,338 → 348,726 |

Each open calls the actual Lua binding against an already-populated virtual output store. It includes path conversion, normalization, lookup, byte copying and side-effect recording. File creation, Lua setup and final VM teardown are excluded. The alternating case warms the destination to the larger file size before timing and then switches sizes on every call. Median throughput changes across the six API cases range from +26.6% to +224.2%; CPU cycles fall 20.7% to 69.1%. Single-file per-round throughput changes range from +34.4% to +40.8%.

Buffer reuse retains capacity in each handle until it is replaced, transferred on a writable close, or dropped. A path buffer reserves for the uncollapsed base plus input length; a read buffer can retain capacity from a larger previous virtual file. This reduces repeated allocation traffic at the cost of possible higher retained capacity after a later shorter path/file. The tables measure allocation traffic, not RSS or whole-song memory. No disk I/O or whole-song/gameplay-FPS gain is measured here.

## Behavior and validation

Four new release checks pass. Exact path spelling comparisons cover 22,711 base/input combinations plus 19 non-UTF-8 Windows base cases: empty and parent components, drive-relative paths, different drives, rooted paths, UNC, device/verbatim prefixes, mixed separators, Unicode and NUL-containing text. Comparisons use `OsStr` equality against the frozen implementation. Additional checks cover repeated snapshot grows/shrinks, empty and binary files, writes after a reader opens, lossy UTF-8 and NUL-truncated Lua paths, invalid modes, failed opens, cursor/EOF observations, side-effect counts, fresh physical-file updates and missing-file errors. Allocation checks verify clean path-text borrowing, zero allocation for warm ordinary paths, and zero allocation for warmed snapshot copying.

The default-feature release library suite has 709 passes, 13 failures and eight ignored performance benchmarks. Every failure matches the previous commit's normalized diagnostic, and the retained baseline log hash matches its committed receipt. All four new tests pass. Normalization removes Rust source locations, workspace prefixes, temporary process/thread identifiers and the backtrace hint. The JSON records each failure and diagnostic hash, source/executable hashes and the build-receipt hash. Frozen source was checked against `git show 6a09e355d`; store/read methods, remaining file bindings and embedded production Lua were verified unchanged. Existing native fixtures, acceptance criteria and tolerances are unchanged.

## Reproduction

```powershell
cargo test -p deadsync-song-lua --lib --release --config profile.release.lto=false --no-run --message-format=json
python scripts/benchmark_song_lua_draw_capture.py --binary <test-executable> --baseline-commit 6a09e355d --test-filter rage_file::file_open_perf::benchmark_file_open --out docs/performance/2026-10-19-song-lua-file-open.json
```

Run the executable with `--test-threads=1` for the full suite. Each round performs one warmup plus nine retained samples per variant/workload, pinned to logical CPU 2. Samples perform 20,000 separator conversions, 2,000 paths, 128 or 2,000 snapshot copies, or 128 or 512 opens. Fixture setup and teardown are excluded. Temporary returned values are dropped inside timing; retained buffers are destroyed after timing. Both variants use the same counted System allocator on Windows, Xeon E5-2696 v4 (44 logical CPUs), rustc 1.98.1, release optimization with LTO disabled. No builds or other benchmarks overlap measurements.

QueryThreadCycleTime measures scheduled current-thread CPU cycles including kernel execution, not a user-only hardware PMU counter. Allocation accounting covers current-thread Rust GlobalAlloc, including mlua, and excludes direct native malloc. Accounting overhead is included. Requested bytes represent allocation and reallocation traffic; raw samples also retain frees.
