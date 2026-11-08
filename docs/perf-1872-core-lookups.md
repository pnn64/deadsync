# Performance pass: timing cursors and font discovery

Branch: `perf/1872-core-lookups-20261009`. Starting main: `59ea18661e1ee0a054405f77faf724c601d79b67` (`test(song-lua): refresh native shadow archive`). Version: **0.5.1871 → 0.5.1872**, updated in Cargo.toml and all three inheriting Cargo.lock packages.

Audited 64 unmerged local/remote perf refs, covering 38 distinct heads. Neither production file overlaps a pending performance branch. Work is confined to this separate worktree; main and the original checkout were not edited. Nothing was merged or pushed.

## Changes

1. **Reuse the BPM position already held by timing cursors.** Time-to-beat and beat-to-time updates no longer binary-search a strictly ordered BPM table when the cursor already identifies the correct interval. A chart-lifetime flag validates strict ordering once. The interval check preserves fractional-row rounding; duplicate/NaN tables and mismatched hints retain the original lookup. Empty and single-BPM tables return directly. No new heap storage or synchronization is introduced.
2. **Skip unused speed-clock work.** Instantaneous speed changes return their ratio before converting float seconds to nanoseconds or loading transition runtime data. Queries before the first segment also avoid unused conversion. Gradual transitions retain the original arithmetic and operation order.
3. **Filter font filenames before metadata checks.** Font discovery rejects unrelated names before querying filesystem metadata. Matching regular entries use the directory entry’s file type; symlinks and type-query errors retain the Path::is_file fallback. Directory iteration errors, filtering, and lexical output order are preserved. The original single full-path allocation is retained; there is no temporary filename allocation for selected files.

## Regression and native compatibility checks

Fresh release baseline and current builds used `--locked --config profile.release.lto=false`. The current relevant library suites passed **1,331 tests** (baseline: 1,327). Four new differential tests passed. Two benchmark tests are ignored in normal runs and were executed explicitly. A separate symlink test is ignored on Windows: attempting the fixture on this host returned error 1314 (missing symlink privilege). That platform behavior was not exercised here; the fallback remains in production and the test runs on Unix or can be explicitly enabled on a privileged Windows host.

The frozen originals are copied from the starting commit, and their function bodies were checked against git after formatting. Timing coverage includes arbitrary cursor hints, empty/single/multi-BPM tables, duplicate and malformed tables, fractional events, offsets, rewinds, stop/delay/warp events, both speed units, nonfinite values and extreme timestamps. Timing outputs are compared bitwise or as exact integer/state values. Font coverage includes mixed-case extensions, prefixes, stroke exclusions, directories named .png, Unicode filenames, sorting, missing directories, and non-directory errors.

The Song Lua harness replayed the same six recorded native ITGmania captures on baseline and current: both Epidermis archives, Sharkmode, Warp Zone, Let Me Hear That, and Riddle. All **1,650,933 comparisons passed** in each build. Captures were not regenerated.

The focused suites retained **40 pre-existing failures**: 39 semantic and one actor-conformance failure. Every original test outcome and failure diagnostic matched the baseline. Diagnostic comparison only normalizes panic thread IDs and the already-known unordered missing-actor lines in `image_texture_aliases_match_native_draws`; it retains every line and count. Existing ignored native tests were not enabled.

| Suite | Passed | Failed | Ignored |
| --- | ---: | ---: | ---: |
| deadsync_core-tests | 9 | 0 | 0 |
| deadsync_chart-tests | 32 | 0 | 0 |
| deadsync_rules-tests | 114 | 0 | 1 |
| deadsync_gameplay-tests | 794 | 0 | 0 |
| deadsync_simfile-tests | 203 | 0 | 0 |
| deadlib_present-tests | 179 | 0 | 2 |
| semantic | 142 | 39 | 77 |
| actor | 30 | 1 | 0 |

Native archive selectors (same on both runs):

- `0f2ef3f987449e6218525fba450d1db1201c9f733d4956061d3b36529b922305.tar.zst`
- `af2f887d212d8ab702b772cea9c953500254ce01a6b78b423276fca9cf5a8306.tar.zst`
- `b05379b7d12bfb02149e03f166be5d496dc3e7687dcc042f0bf525cc46aa918b.tar.zst`
- `ded0f7ff19513e30119e0d6f65b57a661f1683872fc9ffced27737db06c49993.tar.zst`
- `2a77063dd2ab8c43bbdbb9e101be7fcf73156302e21e9771c88797218317d327.tar.zst`
- `920eb361ce688e70b4364258337b30c9e4bf7d337f2c83a30e59215eb838735d.tar.zst`

## Benchmark method

Windows x86_64, Intel Xeon E5-2696 v4 (22 cores / 44 logical processors), Ultimate Performance power plan, rustc 1.98.1 / LLVM 22.1.8. Release optimization level 3, LTO disabled equally for both variants. Benchmarks use actual crate test binaries, with frozen original/current implementations in the same binary, and black-box inputs/results. Four fresh process runs were pinned to logical processor 2 with AboveNormal priority. Each case alternates variant order across ten batches, discards the first calibration batch, and takes the median of the remaining nine. Fast cases calibrate to roughly 5 ms per batch. Values below are medians across the four process runs; ranges are the minimum/maximum paired speedups.

A clock operation is **1,024 sequential queries**, including event-cursor setup, rather than a single lookup. Chart construction is outside timing: the new one-time linear BPM-order validation is not measured by these steady-state clock cases. `elapsed=false` is time-to-beat; `elapsed=true` is beat-to-time. A speed operation is one query; `ns=false` uses float seconds and `ns=true` uses nanoseconds. Font operations enumerate and sort one temporary directory of 8–2,048 files; the `font` prefix selects at most five PNG pages, an empty prefix admits all PNG names, and `missing` returns none. Fixtures are outside timed regions, file contents are empty because discovery does not decode them, and the filesystem cache is warm. Filesystem gains are host-specific, not portable CPU-only claims. These are operation-level throughput measurements, not whole-game FPS results.

The existing shared Cargo build cache was reused at `C:/GitHub/deadsync-perf-1872-data-churn-20261009/target/build`; source changes and final benchmark fixtures stayed in this pass’s worktree. No build started by this pass overlapped the final benchmark runs. Other builds were active elsewhere on the shared machine, so the paired ranges capture some scheduling/frequency noise; they are not confidence intervals.

## Results and controls

With 64-1,024 distinct BPM events, clock throughput improved **1.16-1.69x**. Instantaneous float-time speed queries improved **1.18-1.55x**, with nanosecond queries improving **1.04-1.12x**. Font discovery with the representative `font` prefix improved **5.92-38.48x** on this Windows filesystem.

Controls are included below, including slower results: the constant-BPM time-to-beat and beat-to-time medians were 3.6% and 3.0% slower; two-BPM time-to-beat was 1.1% slower. Empty speed calls shifted by 0.05-0.18 ns, roughly 3-9% in these very small microbenchmarks. These costs are not hidden or described as improvements. No behavioral regression was observed in the executed tests.

| Case | Original ns/op | Current ns/op | Throughput | Paired range |
| --- | ---: | ---: | ---: | ---: |
| `clock/1/duplicate=false/elapsed=false` | 34677.56 | 35908.60 | 0.966× | 0.924–1.004× |
| `clock/1/duplicate=false/elapsed=true` | 24714.76 | 25461.81 | 0.971× | 0.929–0.998× |
| `clock/2/duplicate=false/elapsed=false` | 48457.75 | 49002.50 | 0.989× | 0.970–1.039× |
| `clock/2/duplicate=false/elapsed=true` | 30616.40 | 29485.33 | 1.038× | 0.977–1.086× |
| `clock/64/duplicate=false/elapsed=false` | 61371.35 | 53068.83 | 1.156× | 1.116–1.199× |
| `clock/64/duplicate=false/elapsed=true` | 51359.96 | 33944.38 | 1.513× | 1.381–1.514× |
| `clock/64/duplicate=true/elapsed=false` | 56495.57 | 56248.12 | 1.004× | 0.822–1.047× |
| `clock/64/duplicate=true/elapsed=true` | 44442.50 | 41880.52 | 1.061× | 1.006–1.131× |
| `clock/1024/duplicate=false/elapsed=false` | 112410.71 | 75124.04 | 1.496× | 1.469–1.633× |
| `clock/1024/duplicate=false/elapsed=true` | 83341.79 | 49210.29 | 1.694× | 1.687–1.929× |
| `speed/0/delay=0/before=false/ns=false` | 1.91 | 2.00 | 0.957× | 0.898–0.990× |
| `speed/0/delay=0/before=false/ns=true` | 1.96 | 2.09 | 0.938× | 0.937–1.016× |
| `speed/0/delay=0/before=true/ns=false` | 2.02 | 2.10 | 0.962× | 0.959–0.965× |
| `speed/0/delay=0/before=true/ns=true` | 1.95 | 2.12 | 0.922× | 0.904–0.923× |
| `speed/0/delay=0.5/before=false/ns=false` | 1.94 | 1.98 | 0.975× | 0.928–1.027× |
| `speed/0/delay=0.5/before=false/ns=true` | 1.98 | 2.11 | 0.938× | 0.935–1.010× |
| `speed/0/delay=0.5/before=true/ns=false` | 1.89 | 2.02 | 0.936× | 0.891–1.034× |
| `speed/0/delay=0.5/before=true/ns=true` | 1.92 | 2.10 | 0.917× | 0.831–1.050× |
| `speed/1/delay=0/before=false/ns=false` | 6.28 | 4.04 | 1.551× | 1.461–1.599× |
| `speed/1/delay=0/before=false/ns=true` | 4.76 | 4.26 | 1.119× | 1.095–1.137× |
| `speed/1/delay=0/before=true/ns=false` | 6.41 | 4.18 | 1.533× | 1.519–1.659× |
| `speed/1/delay=0/before=true/ns=true` | 4.58 | 4.20 | 1.090× | 1.029–1.103× |
| `speed/1/delay=0.5/before=false/ns=false` | 29.80 | 29.22 | 1.020× | 1.019–1.128× |
| `speed/1/delay=0.5/before=false/ns=true` | 27.27 | 27.05 | 1.008× | 0.968–1.067× |
| `speed/1/delay=0.5/before=true/ns=false` | 6.06 | 3.92 | 1.547× | 1.461–1.664× |
| `speed/1/delay=0.5/before=true/ns=true` | 4.12 | 3.82 | 1.077× | 1.031–1.083× |
| `speed/64/delay=0/before=false/ns=false` | 18.61 | 15.79 | 1.179× | 1.155–1.194× |
| `speed/64/delay=0/before=false/ns=true` | 16.81 | 16.17 | 1.040× | 1.017–1.054× |
| `speed/64/delay=0/before=true/ns=false` | 17.52 | 15.27 | 1.148× | 1.122–1.165× |
| `speed/64/delay=0/before=true/ns=true` | 15.37 | 14.91 | 1.031× | 0.952–1.089× |
| `speed/64/delay=0.5/before=false/ns=false` | 56.58 | 54.11 | 1.046× | 1.013–1.078× |
| `speed/64/delay=0.5/before=false/ns=true` | 53.77 | 53.09 | 1.013× | 0.984–1.023× |
| `speed/64/delay=0.5/before=true/ns=false` | 16.14 | 15.14 | 1.066× | 1.022–1.156× |
| `speed/64/delay=0.5/before=true/ns=true` | 14.80 | 14.85 | 0.997× | 0.941–1.056× |
| `font/8/prefix="font"` | 872176.19 | 147258.83 | 5.923× | 5.598–6.190× |
| `font/8/prefix=""` | 890376.19 | 141664.67 | 6.285× | 5.991–6.347× |
| `font/8/prefix="missing"` | 900048.57 | 137230.35 | 6.559× | 6.089–7.079× |
| `font/64/prefix="font"` | 6361950.00 | 279695.83 | 22.746× | 22.646–23.524× |
| `font/64/prefix=""` | 6514600.00 | 304331.11 | 21.406× | 21.242–23.492× |
| `font/64/prefix="missing"` | 6360850.00 | 278316.67 | 22.855× | 22.070–24.510× |
| `font/512/prefix="font"` | 52821450.00 | 1546704.17 | 34.151× | 33.708–36.922× |
| `font/512/prefix=""` | 52659050.00 | 1770483.33 | 29.743× | 27.884–35.388× |
| `font/512/prefix="missing"` | 53027600.00 | 1556700.00 | 34.064× | 32.539–36.110× |
| `font/2048/prefix="font"` | 203349400.00 | 5284650.00 | 38.479× | 33.400–44.108× |
| `font/2048/prefix=""` | 207734800.00 | 6571950.00 | 31.609× | 29.584–33.756× |
| `font/2048/prefix="missing"` | 202264700.00 | 5896950.00 | 34.300× | 33.435–34.615× |

## Reproduction

The committed tests preserve the originals and workloads. From this worktree:

```powershell
cargo test --release --locked --config profile.release.lto=false -p deadsync-core -p deadsync-chart -p deadsync-rules -p deadsync-gameplay -p deadsync-simfile -p deadlib-present --lib
cargo test --release --locked --config profile.release.lto=false -p deadsync-rules -p deadlib-present --lib benchmark_core_lookups -- --ignored --nocapture --test-threads=1
cargo test --release --locked --config profile.release.lto=false --test song_lua_itgmania_semantic_parity -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false --test itgmania_actor_conformance -- --test-threads=1
```

Run `cargo test --release --locked --config profile.release.lto=false --test full_song_lua -- <archive>` with each archive selector listed above. The focused native suites have the baseline failures described above. Direct test-binary execution was used for the measured runs to apply process affinity without including Cargo startup time.

Local, uncommitted evidence is under `target/`: baseline/current build and suite logs, six full-song logs per build, `compatibility-comparison.json`, `final-1.log` through `final-4.log`, `benchmark-summary.json`, `pending-audit.json`, and `source-hashes.json`. Source hashes were frozen before the final benchmarks and native replay, then rechecked before commit. No excluded file is staged.
