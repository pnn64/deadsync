# Parser streamlining performance pass

Branch: `perf/1872-parse-streamlining-20261009`.
Base: `3d5ca438bb626722aec9e6c3e46d148b2018b859`, committed local main when the worktree was created.
Version: **0.5.1871 -> 0.5.1872**, including Cargo.toml and all three workspace-version records in Cargo.lock.

## Changes

1. Timing tag sorting now compares finite beats directly. Rounding beats to note rows is monotone, and the previous comparator broke equal-row ties using the same beat comparison. Removing the redundant conversions preserves signed-zero ordering, stable equal-beat winners, row deduplication and saturated row behavior. This applies to time signatures, tick counts and combo segments, including their cached representations.
2. Latest tag extraction transfers the decoded string when unescaping returns the unchanged borrowed input. Legacy CP1252 values no longer allocate and copy a second string before dropping the first. UTF-8 decoding, escape handling, duplicate-tag selection and the public owned return type are unchanged.
3. Background media filtering checks for a dot before comparing the following three bytes against `ini` and `xml`. Ordinary filename bytes no longer perform two case-insensitive extension comparisons. Substring rejection, rather than suffix-only rejection, is preserved.

All changes are outside Song Lua. The pending-branch audit found no changes to the three selected production files across 32 distinct unmerged heads (54 local/remote refs). Test allocation/timing support reuses the preceding pass's helper, with its unused owned-input benchmark removed. No production dependency, cache or unsafe code was added.

## Measurements

- **Timing sorts:** 256 shuffled records: signatures 1.460x, ticks 1.467x, combos 1.501x. For 4,096 ordered records: signatures 1.021x, ticks 1.058x, combos 1.064x.
- **Legacy tag ownership:** legacy-short 1.519x, legacy-128 1.230x, legacy-4096 1.041x. Three unescaped CP1252 fields use three allocations instead of six, with no reallocations. Escaped and UTF-8 cases remain in the table as controls.
- **Background media check:** ordinary movie names 1.535x, 128-byte paths 1.606x, early rejection 1.143x, and dot-heavy paths 1.488x. Both versions allocate zero times.

Cases with a lower median ratio: `tags/ascii-escaped` 0.964x (range 0.914-1.007); `timing/signatures/0/sorted` 0.988x (range 0.943-1.015); `timing/combos/0/sorted` 0.976x (range 0.945-1.003); `timing/ticks/1/sorted` 0.984x (range 0.977-1.010); `timing/combos/1/sorted` 0.976x (range 0.828-0.990). The single-combo control is lower in all four runs, by about 4.7 ns/call at the median. The other lower-median controls have ranges crossing parity; no across-the-board throughput improvement is claimed.

The table includes every fixed benchmark case. Times are elapsed nanoseconds per call. Ratios greater than one favor the change. Allocation columns show allocations + reallocations; requested bytes count allocator requests, not RSS. Peak bytes are the largest allocation-minus-free balance within the measured call; these fixtures start without tracked live allocations, and outputs are retained until after the count is read.

| Case | Original ns/op | Current ns/op | Throughput ratio | Four-run ratio range | Allocations + reallocations, original -> current | Requested bytes, original -> current |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| media/empty | 1.08 | 1.07 | 1.005 | 0.962-1.066 | 0 + 0 -> 0 + 0 | 0 -> 0 |
| media/tiny | 1.10 | 1.08 | 1.023 | 1.000-1.046 | 0 + 0 -> 0 + 0 | 0 -> 0 |
| media/movie | 13.65 | 8.89 | 1.535 | 1.506-1.541 | 0 + 0 -> 0 + 0 | 0 -> 0 |
| media/path-128 | 132.49 | 82.47 | 1.606 | 1.525-1.614 | 0 + 0 -> 0 + 0 | 0 -> 0 |
| media/path-4096 | 3973.48 | 2425.54 | 1.638 | 1.517-1.691 | 0 + 0 -> 0 + 0 | 0 -> 0 |
| media/early-reject | 4.96 | 4.33 | 1.143 | 1.108-1.281 | 0 + 0 -> 0 + 0 | 0 -> 0 |
| media/late-reject | 132.22 | 83.09 | 1.591 | 1.497-1.612 | 0 + 0 -> 0 + 0 | 0 -> 0 |
| media/dot-heavy | 155.02 | 104.21 | 1.488 | 1.396-1.569 | 0 + 0 -> 0 + 0 | 0 -> 0 |
| media/unicode | 20.53 | 14.73 | 1.394 | 1.275-1.652 | 0 + 0 -> 0 + 0 | 0 -> 0 |
| tags/absent | 24.23 | 19.75 | 1.227 | 0.993-1.433 | 0 + 0 -> 0 + 0 | 0 -> 0 |
| tags/ascii | 439.84 | 433.39 | 1.015 | 0.916-1.107 | 3 + 0 -> 3 + 0 | 66 -> 66 |
| tags/utf8 | 460.18 | 457.30 | 1.006 | 0.972-1.098 | 3 + 0 -> 3 + 0 | 51 -> 51 |
| tags/legacy-short | 771.62 | 507.98 | 1.519 | 1.447-1.593 | 6 + 0 -> 3 + 0 | 54 -> 27 |
| tags/legacy-128 | 1503.09 | 1221.69 | 1.230 | 1.178-1.257 | 6 + 0 -> 3 + 0 | 864 -> 432 |
| tags/legacy-4096 | 21925.39 | 21066.79 | 1.041 | 0.990-1.089 | 6 + 0 -> 3 + 0 | 27648 -> 13824 |
| tags/legacy-escaped | 873.20 | 868.33 | 1.006 | 0.976-1.071 | 6 + 0 -> 6 + 0 | 84 -> 84 |
| tags/ascii-escaped | 605.04 | 627.57 | 0.964 | 0.914-1.007 | 3 + 0 -> 3 + 0 | 69 -> 69 |
| timing/signatures/0/sorted | 74.00 | 74.91 | 0.988 | 0.943-1.015 | 1 + 0 -> 1 + 0 | 12 -> 12 |
| timing/ticks/0/sorted | 76.34 | 72.91 | 1.047 | 1.005-1.105 | 1 + 0 -> 1 + 0 | 8 -> 8 |
| timing/combos/0/sorted | 72.91 | 74.73 | 0.976 | 0.945-1.003 | 1 + 0 -> 1 + 0 | 12 -> 12 |
| timing/signatures/1/sorted | 189.11 | 183.07 | 1.033 | 1.011-1.179 | 1 + 0 -> 1 + 0 | 24 -> 24 |
| timing/ticks/1/sorted | 173.81 | 176.71 | 0.984 | 0.977-1.010 | 1 + 0 -> 1 + 0 | 16 -> 16 |
| timing/combos/1/sorted | 193.51 | 198.24 | 0.976 | 0.828-0.990 | 1 + 0 -> 1 + 0 | 24 -> 24 |
| timing/signatures/16/sorted | 1988.41 | 1960.45 | 1.014 | 0.962-1.129 | 1 + 0 -> 1 + 0 | 204 -> 204 |
| timing/ticks/16/sorted | 1762.78 | 1734.29 | 1.016 | 0.964-1.088 | 1 + 0 -> 1 + 0 | 136 -> 136 |
| timing/combos/16/sorted | 2113.71 | 2042.38 | 1.035 | 1.011-1.123 | 1 + 0 -> 1 + 0 | 204 -> 204 |
| timing/signatures/256/shuffled | 58361.94 | 39961.46 | 1.460 | 1.398-1.550 | 1 + 0 -> 1 + 0 | 3084 -> 3084 |
| timing/ticks/256/shuffled | 48792.29 | 33270.01 | 1.467 | 1.421-1.567 | 1 + 0 -> 1 + 0 | 2056 -> 2056 |
| timing/combos/256/shuffled | 61165.00 | 40757.81 | 1.501 | 1.487-1.585 | 1 + 0 -> 1 + 0 | 3084 -> 3084 |
| timing/signatures/4096/sorted | 594150.00 | 582175.00 | 1.021 | 0.965-1.051 | 2 + 0 -> 2 + 0 | 98316 -> 98316 |
| timing/ticks/4096/sorted | 541912.50 | 512156.25 | 1.058 | 0.987-1.066 | 2 + 0 -> 2 + 0 | 65544 -> 65544 |
| timing/combos/4096/sorted | 631187.50 | 593050.00 | 1.064 | 1.052-1.110 | 2 + 0 -> 2 + 0 | 98316 -> 98316 |
| timing/signatures/4096/reverse | 584100.00 | 582850.00 | 1.002 | 0.973-1.009 | 2 + 0 -> 2 + 0 | 98316 -> 98316 |
| timing/ticks/4096/reverse | 525787.50 | 517987.50 | 1.015 | 1.013-1.087 | 2 + 0 -> 2 + 0 | 65544 -> 65544 |
| timing/combos/4096/reverse | 629250.00 | 593437.50 | 1.060 | 1.033-1.061 | 2 + 0 -> 2 + 0 | 98316 -> 98316 |
| timing/signatures/4096/duplicates | 1024400.00 | 741725.00 | 1.381 | 1.364-1.400 | 2 + 0 -> 2 + 0 | 98316 -> 98316 |
| timing/ticks/4096/duplicates | 845962.50 | 635237.50 | 1.332 | 1.320-1.349 | 2 + 0 -> 2 + 0 | 65544 -> 65544 |
| timing/combos/4096/duplicates | 1067125.00 | 771912.50 | 1.382 | 1.337-1.452 | 2 + 0 -> 2 + 0 | 98316 -> 98316 |

For three 4,096-byte CP1252 values, requested bytes fall **27,648 -> 13,824**, and measured peak live bytes fall **18,432 -> 13,824**. Returned storage stays **13,824 bytes**. The pinned decoder reserves the exact UTF-8 output size, so transferring these buffers retains no extra capacity. Timing output allocation counts and sizes are unchanged; media filtering remains allocation-free. No RSS or whole-game speedup is claimed.

## Method and scope

Windows/MSVC, Intel Xeon E5-2696 v4 at 2.20 GHz, rustc 1.98.1 (`48a229cea`, 2026-09-01). Release builds use `profile.release.lto=false`. Frozen original function bodies are checked against the starting commit, ignoring only visibility and formatting. The new benchmarks call the actual production functions. Original and current versions run in the same binary.

Four fixed runs follow exploratory measurements. Each case alternates original/current order for ten batches, discards the first batch, and reports the median of nine. The table takes the median of the four run medians and reports the range of paired run ratios. Initial batches calibrate loop counts toward two milliseconds, capped at one million iterations. Inputs are prepared outside timing; each timed call includes result destruction and uses `black_box`. Own builds and baseline compatibility runs finish before fixed measurements; unrelated host work is left running. These are microbenchmarks, not whole-game loading or frame-time measurements.

Thread-local counters delegate unchanged allocations to the System allocator. Allocation tracking runs separately from timing; the wrapper still checks whether tracking is enabled during timings, which can affect allocation-heavy ratios. Counts are identical across all four runs.

Timing cases measure the complete parse, stable sort and row deduplication for all three public segment types. Ordered, reversed, shuffled, same-row duplicate, empty and single-record inputs are included. Tag cases request three named values and cover ASCII, UTF-8, CP1252, escaped values and missing tags. CP1252 payload lengths are 8, 128 and 4,096 bytes per requested tag. Media cases cover empty/tiny values, ordinary movie names, long paths, early/late rejection, many dots and Unicode.

## Regression and ITGmania compatibility

- `deadsync_simfile-tests`: 210 passed, 0 failed, 3 ignored; all 203 pre-existing outcomes and 0 failure diagnostics are unchanged.
- `semantic`: 143 passed, 38 failed, 77 ignored; all 258 pre-existing outcomes and 38 failure diagnostics are unchanged.
- `actor`: 30 passed, 1 failed, 0 ignored; all 31 pre-existing outcomes and 1 failure diagnostics are unchanged.
- All three ignored paired benchmarks were explicitly run and passed in each of four fixed runs, covering 38 cases per run.
- The 39 existing semantic/actor failures remain unresolved. No new behavioral failure was detected.
- Six selected full-song archives: **1,650,933 comparisons passed, zero failed**, on both builds with identical selectors and outcomes.

Seven new regression tests compare float bits and stable duplicate winners, generated timing inputs, row rounding/saturation boundaries, randomized tag bytes, encodings and escapes, output capacities, case combinations and generated filenames. Existing integration tests exercise the calling paths.

Compatibility tests replay DeadSync against recorded native ITGmania traces; no new native capture is performed. Each result and failure diagnostic is compared against a fresh pristine-base run. Panic thread IDs are normalized. Only the known unordered `Sprite.Load has no matching DeadSync actor` lines within `image_texture_aliases_match_native_draws` are sorted; other diagnostic contents and ordering must match.

| Archive selector | Comparisons passed, baseline and current |
| --- | ---: |
| `0f2ef3f987449e6218525fba450d1db1201c9f733d4956061d3b36529b922305.tar.zst` (319\|TECH SOUP\|[lv.P.Clark] Epidermis) | 363,873 |
| `af2f887d212d8ab702b772cea9c953500254ce01a6b78b423276fca9cf5a8306.tar.zst` (319\|TECH SOUP\|[lv.P.Clark] Epidermis) | 363,873 |
| `b05379b7d12bfb02149e03f166be5d496dc3e7687dcc042f0bf525cc46aa918b.tar.zst` (280\|MODS\|[MASTER] Sharkmode) | 304,425 |
| `b38698ececd60bdf311a15de205f43d0f3466eac6d41296b9abf469485ebab4d.tar.zst` (Warp Zone) | 212,220 |
| `0229b74d092e8f04278d64b7c994877a0a3bcc52e9d7ddb00d6a98f1f5244cd0.tar.zst` (Let Me Hear That) | 205,071 |
| `920eb361ce688e70b4364258337b30c9e4bf7d337f2c83a30e59215eb838735d.tar.zst` (272\|MODS\|[lv.02] Riddle) | 201,471 |

## Reproduce

From this worktree in PowerShell:

```powershell
$env:TEMP = Join-Path $PWD 'target/tmp'
$env:TMP = $env:TEMP
New-Item -ItemType Directory -Force $env:TEMP | Out-Null
cargo test --release --locked --config profile.release.lto=false -p deadsync -p deadsync-simfile --lib -- --test-threads=1
1..4 | ForEach-Object {
    cargo test --release --locked --config profile.release.lto=false -p deadsync -p deadsync-simfile --lib benchmark_parse -- --ignored --nocapture --test-threads=1
}
cargo test --release --locked --config profile.release.lto=false --test song_lua_itgmania_semantic_parity -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false --test itgmania_actor_conformance -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false --test full_song_lua -- <archive-name.tar.zst>
```

Run the last command for each selector above. Run compatibility commands at the base commit in a separate worktree for comparison. Paired benchmarks already contain both versions. Raw results remain under ignored `target/baseline-*`, `target/current-*`, `target/final-[1-4].log`, `target/benchmark-results.json` and `target/compatibility-comparison.json`.

Formatting, frozen-source equivalence, the exact patch bump, pending branch scope, diff whitespace and the explicit staged file list were checked. The original checkout and its uncommitted work were left untouched. The four excluded user files and build artifacts are absent from the commit. Nothing was merged or pushed.
