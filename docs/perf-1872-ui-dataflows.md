# Performance pass: UI text dataflows

Branch: `perf/1872-ui-dataflows-20261009`. Starting committed main: `807df3fe4405e4df269fad0298de9ef234abd70b`. Version: **0.5.1871 -> 0.5.1872**, updated in Cargo.toml and all three matching workspace entries in Cargo.lock.

Read rust-performance.md and checked 27 distinct unmerged performance branch heads. None changes either selected production file. Work is isolated in this pass's worktree and remains based on its starting main commit. The original checkout and its independent work are untouched. No merge or push was performed.

## Changes

1. **Grade headings:** borrow fixed grade labels and the existing localized Unplayed string while building both combined and per-player grade-grouped song lists. Share each new heading with its wheel entry instead of copying a separate current-group string. Remove one allocation per song plus one per heading, without hoisting or dropping localization lookups.
2. **Lobby status:** append the disconnect prompt directly to the existing message using the existing translation writer. Remove intermediate prompt strings and the formatted prompt Arc. Keep notice precedence, lock conditions, countdown values, pluralization and prefix text intact.
3. **Option choices:** borrow static literals for input handling, removing the literal-to-Arc-to-String round trip. Keep shared localized text and Arc-based display values unchanged. Keep the original shared collector as the existing trait's default, and specialize the input collector to borrow literals directly. Display values still use the original Choice conversion.

All production changes are outside Song Lua, confined to screens/select_music.rs and screens/options/choice_text.rs. The existing choice-conversion trait selects the collector once per list; no new production cache or dependency is introduced. Frozen originals from the starting commit are compiled only for regression tests and benchmarks.

## Benchmark method

Windows x86-64/MSVC, Intel Xeon E5-2696 v4 at 2.20 GHz, `rustc 1.98.1 (48a229cea 2026-09-01)`, release optimization with LTO disabled. Four build jobs, serial test execution. These measurements are function-level wall time and allocation traffic, not application FPS or CPU cycles. This pass runs no build or native compatibility job during benchmarks; unrelated workstation activity remains untouched.

Each case alternates original/current for ten sample pairs, discards the warm-up pair, and reports the median of nine samples per implementation. Batches calibrate fast cases toward approximately 2 ms. Fixtures are built outside timing. Grade benchmarks reuse separate warmed ranking workspaces and a prepared song-order index, as the production caller does. They include ranking, building the entire entry vector and dropping it. Lobby and choice benchmarks include construction and destruction of the complete returned value. Inputs and outputs are black-boxed. Variant selection occurs outside the timed loops; each loop calls its own concrete closure, with no per-operation benchmark selector.

Thread-local counters wrap the System allocator and are enabled for separate allocation measurements. The wrapper remains installed during timing. Allocation counts exclude fixture preparation, ranking-workspace warm-up and later destruction of retained outputs. Output-vector creation remains included. Requested bytes include realloc requests; they are not peak RSS. Returned lobby String capacity is measured separately. Grade regressions also check equal output-vector capacities and unchanged song Arc identity.

## Timing results

| Case | Original ns/op | Current ns/op | Throughput ratio |
|---|---:|---:|---:|
| choices-0-literal-input | 3.53 | 3.53 | 1.00x |
| choices-0-literal-display | 3.36 | 3.36 | 1.00x |
| choices-1-literal-input | 243.38 | 69.90 | 3.48x |
| choices-1-literal-display | 150.43 | 155.17 | 0.97x |
| choices-8-literal-input | 1,544.58 | 124.31 | 12.43x |
| choices-8-literal-display | 1,084.33 | 1,083.05 | 1.00x |
| choices-64-literal-input | 12,946.50 | 505.06 | 25.63x |
| choices-64-literal-display | 6,153.33 | 6,072.00 | 1.01x |
| choices-32-mixed-input | 5,060.75 | 2,162.56 | 2.34x |
| choices-32-mixed-display | 2,166.22 | 2,204.33 | 0.98x |
| choices-32-localized-input | 5,378.40 | 5,472.00 | 0.98x |
| choices-32-localized-display | 1,473.15 | 1,498.08 | 0.98x |
| grades-0-unplayed-both | 84.63 | 83.52 | 1.01x |
| grades-1-unplayed-both | 575.13 | 422.99 | 1.36x |
| grades-1-same-p1 | 536.40 | 419.48 | 1.28x |
| grades-128-unplayed-both | 52,075.00 | 42,174.00 | 1.23x |
| grades-128-mixed-both | 148,620.00 | 137,535.00 | 1.08x |
| grades-128-same-p1 | 88,710.00 | 79,570.00 | 1.11x |
| grades-2048-unplayed-both | 880,480.00 | 725,490.00 | 1.21x |
| grades-2048-mixed-both | 3,272,800.00 | 3,142,540.00 | 1.04x |
| grades-2048-mixed-p1 | 2,100,970.00 | 1,839,940.00 | 1.14x |
| grades-8192-unplayed-both | 3,974,740.00 | 3,539,940.00 | 1.12x |
| grades-8192-same-p2 | 11,902,660.00 | 10,927,890.00 | 1.09x |
| lobby-no-lobby | 4.38 | 4.87 | 0.90x |
| lobby-unlocked | 37.63 | 38.10 | 0.99x |
| lobby-notice | 76.13 | 77.59 | 0.98x |
| lobby-basic | 567.29 | 471.18 | 1.20x |
| lobby-evaluation | 435.38 | 352.18 | 1.24x |
| lobby-holding | 1,084.35 | 975.27 | 1.11x |
| lobby-reconnecting | 466.39 | 369.98 | 1.26x |
| lobby-long-reconnecting | 474.76 | 356.42 | 1.33x |

The primary table is the first complete run of the final revision, including controls.

Three fixed repeats used the same binary. Results for every initially slower case follow.

| Repeat | Case | Original ns/op | Current ns/op | Current time change |
|---:|---|---:|---:|---:|
| 1 | choices-1-literal-display | 149.92 | 149.13 | -0.53% |
| 1 | choices-32-mixed-display | 2,293.14 | 2,346.56 | +2.33% |
| 1 | choices-32-localized-input | 5,151.50 | 5,140.00 | -0.22% |
| 1 | choices-32-localized-display | 1,585.31 | 1,565.23 | -1.27% |
| 1 | lobby-no-lobby | 4.33 | 4.87 | +12.47% |
| 1 | lobby-unlocked | 36.99 | 37.19 | +0.54% |
| 1 | lobby-notice | 74.96 | 74.34 | -0.83% |
| 2 | choices-1-literal-display | 149.36 | 148.83 | -0.35% |
| 2 | choices-32-mixed-display | 2,405.44 | 2,295.14 | -4.59% |
| 2 | choices-32-localized-input | 5,158.00 | 4,908.00 | -4.85% |
| 2 | choices-32-localized-display | 1,628.50 | 1,765.43 | +8.41% |
| 2 | lobby-no-lobby | 4.17 | 4.39 | +5.28% |
| 2 | lobby-unlocked | 34.71 | 34.06 | -1.87% |
| 2 | lobby-notice | 80.22 | 73.91 | -7.87% |
| 3 | choices-1-literal-display | 157.58 | 168.74 | +7.08% |
| 3 | choices-32-mixed-display | 2,442.88 | 2,383.33 | -2.44% |
| 3 | choices-32-localized-input | 5,481.75 | 5,234.00 | -4.52% |
| 3 | choices-32-localized-display | 1,652.92 | 1,605.00 | -2.90% |
| 3 | lobby-no-lobby | 4.24 | 4.54 | +7.08% |
| 3 | lobby-unlocked | 39.02 | 39.84 | +2.10% |
| 3 | lobby-notice | 73.70 | 69.69 | -5.44% |

All 31 allocation records and all eight retained-text capacities are identical across the initial run and three fixed repeats. Eight literal input choices: 12.02-12.43x throughput. The 2,048-song unplayed list: 1.21-1.27x throughput. The basic locked-lobby message: 1.08-1.20x throughput. These ranges include every final run.

Controls are not uniformly faster. Measured current-minus-original time ranges across all four runs follow; positive values mean slower.

| Control | Time change | Absolute change |
|---|---:|---:|
| choices-0-literal-input | -0.84% to +4.05% | -0.03 to +0.14 ns |
| choices-0-literal-display | +0.00% to +6.25% | +0.00 to +0.21 ns |
| choices-1-literal-display | -0.53% to +7.08% | -0.79 to +11.16 ns |
| choices-8-literal-display | -0.12% to +4.01% | -1.28 to +42.81 ns |
| choices-64-literal-display | -1.32% to +0.55% | -86.75 to +38.00 ns |
| choices-32-mixed-display | -4.59% to +2.33% | -110.30 to +53.42 ns |
| choices-32-localized-input | -4.85% to +1.74% | -250.00 to +93.60 ns |
| choices-32-localized-display | -2.90% to +8.41% | -47.92 to +136.93 ns |
| grades-0-unplayed-both | -1.83% to +7.80% | -1.56 to +5.99 ns |
| grades-8192-same-p2 | -12.27% to +2.00% | -1162210.00 to +221780.00 ns |
| lobby-no-lobby | +5.28% to +12.47% | +0.22 to +0.54 ns |
| lobby-unlocked | -1.87% to +2.10% | -0.65 to +0.82 ns |
| lobby-notice | -7.87% to +1.92% | -6.31 to +1.46 ns |

These results do not establish a speedup for every workload or for the full application. The repeatable improvements claimed here are reduced allocation churn, lower requested allocation bytes, unchanged retained capacities, and throughput gains for the representative populated cases quoted above. No behavioral regressions were observed in the tests and comparisons below. Raw earlier-revision measurements remain under target/*-control-regression/ and target/before-shared-helper-check/. The final benchmark selects each implementation outside its timed loop, and the display collector retains the original conversion loop. No forced-inline attribute remains in production.

## Allocation traffic and retained text

Cells are **allocations / reallocations / frees / requested bytes / freed bytes** per call. Counts include output vectors; fixture/index construction and workspace warm-up are excluded.

| Case | Original | Current |
|---|---:|---:|
| choices-0-literal-input | 0 / 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 / 0 |
| choices-0-literal-display | 0 / 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 / 0 |
| choices-1-literal-input | 3 / 0 / 1 / 56 / 24 | 1 / 0 / 0 / 24 / 0 |
| choices-1-literal-display | 2 / 0 / 0 / 40 / 0 | 2 / 0 / 0 / 40 / 0 |
| choices-8-literal-input | 17 / 0 / 8 / 476 / 216 | 1 / 0 / 0 / 192 / 0 |
| choices-8-literal-display | 9 / 0 / 0 / 344 / 0 | 9 / 0 / 0 / 344 / 0 |
| choices-64-literal-input | 129 / 0 / 64 / 3,808 / 1,728 | 1 / 0 / 0 / 1,536 / 0 |
| choices-64-literal-display | 65 / 0 / 0 / 2,752 / 0 | 65 / 0 / 0 / 2,752 / 0 |
| choices-32-mixed-input | 49 / 0 / 16 / 1,448 / 416 | 17 / 0 / 0 / 896 / 0 |
| choices-32-mixed-display | 17 / 0 / 0 / 928 / 0 | 17 / 0 / 0 / 928 / 0 |
| choices-32-localized-input | 33 / 0 / 0 / 1,024 / 0 | 33 / 0 / 0 / 1,024 / 0 |
| choices-32-localized-display | 1 / 0 / 0 / 512 / 0 | 1 / 0 / 0 / 512 / 0 |
| grades-0-unplayed-both | 1 / 0 / 0 / 1,600 / 0 | 1 / 0 / 0 / 1,600 / 0 |
| grades-1-unplayed-both | 4 / 0 / 2 / 1,720 / 16 | 2 / 0 / 0 / 1,704 / 0 |
| grades-1-same-p1 | 4 / 0 / 2 / 1,708 / 4 | 2 / 0 / 0 / 1,704 / 0 |
| grades-128-unplayed-both | 131 / 0 / 129 / 12,896 / 1,032 | 2 / 0 / 0 / 11,864 / 0 |
| grades-128-mixed-both | 159 / 0 / 143 / 13,114 / 890 | 16 / 0 / 0 / 12,224 / 0 |
| grades-128-same-p1 | 131 / 0 / 129 / 12,122 / 258 | 2 / 0 / 0 / 11,864 / 0 |
| grades-2048-unplayed-both | 2,051 / 0 / 2,049 / 181,856 / 16,392 | 2 / 0 / 0 / 165,464 / 0 |
| grades-2048-mixed-both | 2,079 / 0 / 2,063 / 179,204 / 13,380 | 16 / 0 / 0 / 165,824 / 0 |
| grades-2048-mixed-p1 | 2,089 / 0 / 2,068 / 177,253 / 11,309 | 21 / 0 / 0 / 165,944 / 0 |
| grades-8192-unplayed-both | 8,195 / 0 / 8,193 / 722,528 / 65,544 | 2 / 0 / 0 / 656,984 / 0 |
| grades-8192-same-p2 | 8,195 / 0 / 8,193 / 673,370 / 16,386 | 2 / 0 / 0 / 656,984 / 0 |
| lobby-no-lobby | 0 / 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 / 0 |
| lobby-unlocked | 0 / 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 / 0 |
| lobby-notice | 1 / 0 / 0 / 6 / 0 | 1 / 0 / 0 / 6 / 0 |
| lobby-basic | 2 / 2 / 1 / 329 / 165 | 1 / 2 / 0 / 287 / 123 |
| lobby-evaluation | 2 / 1 / 1 / 171 / 85 | 1 / 1 / 0 / 129 / 43 |
| lobby-holding | 4 / 2 / 3 / 437 / 273 | 2 / 2 / 1 / 297 / 133 |
| lobby-reconnecting | 2 / 2 / 1 / 147 / 87 | 1 / 2 / 0 / 105 / 45 |
| lobby-long-reconnecting | 2 / 1 / 1 / 5,142 / 1,742 | 1 / 1 / 0 / 5,100 / 1,700 |

| Retained lobby text capacity (bytes) | Original | Current |
|---|---:|---:|
| lobby-no-lobby | 0 | 0 |
| lobby-unlocked | 0 | 0 |
| lobby-notice | 6 | 6 |
| lobby-basic | 164 | 164 |
| lobby-evaluation | 86 | 86 |
| lobby-holding | 164 | 164 |
| lobby-reconnecting | 60 | 60 |
| lobby-long-reconnecting | 3,400 | 3,400 |

## Regression checks

- Theme release tests: **1270 passed, 1 failed, 5 ignored**. All 1264 baseline outcomes are preserved. Nine new regression tests pass; three new benchmark tests pass when run explicitly.
- Grade cases compare complete header fields and song identity/order for empty, unplayed, mixed and uniform-grade libraries, both player histories, missing or mixed-case chart types, inactive charts, duplicate songs, empty/Unicode titles and workspace reuse. Tests check that allocation savings equal the song count plus heading count.
- Lobby cases compare complete messages across no lobby, unlocked, gameplay/evaluation locks, reconnects, notices, both hold timers, completed countdowns, singular/plural wording and nonfinite limits. Reconnect prefixes containing placeholder syntax stay unchanged. Tests check removed prompt allocations and no larger retained output.
- Option cases compare complete input and display vectors for empty, literal, localized, mixed, Unicode and missing-translation choices. Tests verify static borrowing, unchanged sharing for existing translations, mutable Cow behavior, owned dynamic values and identical display allocation traffic.
- Countdown comparisons capture original/current within one displayed second, retrying only if the real clock crosses that boundary. Mismatching messages are never retried or normalized. Timed holding cases use completed countdowns so their output is stable.
- Existing shared-allocation-helper callers pass in timing, effect-clock, software stripe-bin, Vulkan staging and score-wheel checks: **27 tests across five commands**. The existing no-churn assertion remains available and delegates to the new measurement function.
- Edited Rust files pass rustfmt checks. Frozen originals, exact production scope, patch bump, pending branches, file allowlist and Git whitespace audits pass.

Existing theme failures: screens::components::gameplay::notefield::tests::cyber_model_tap_scale_uses_model_height_not_logical_height. All baseline failure diagnostics are unchanged after normalizing panic thread IDs. The full baseline and current failure logs are retained with the measurements.

## Song Lua / ITGmania compatibility

Fresh baseline/current builds run the native semantic/actor harness and the same six selected full-song archives. The affected UI outputs are covered separately by the frozen-original comparisons above; the native harness provides additional compatibility coverage.

| Suite | Passed | Failed | Ignored | Baseline/current comparison |
|---|---:|---:|---:|---|
| semantic | 142 | 38 | 77 | All 257 outcomes and 38 failure diagnostics unchanged |
| actor | 30 | 1 | 0 | All 31 outcomes and 1 failure diagnostics unchanged |

The native suites are **not fully green**: their 39 pre-existing failures remain. Diagnostic comparisons normalize panic thread IDs and the ordering of known unordered `Sprite.Load` lines; all other diagnostic content and ordering must match.

| Archive | Passing comparisons before and after |
|---|---:|
| 319\|TECH SOUP\|[lv.P.Clark] Epidermis (`0f2ef3f987449e6218525fba450d1db1201c9f733d4956061d3b36529b922305.tar.zst`) | 363,873 |
| 319\|TECH SOUP\|[lv.P.Clark] Epidermis (`af2f887d212d8ab702b772cea9c953500254ce01a6b78b423276fca9cf5a8306.tar.zst`) | 363,873 |
| 280\|MODS\|[MASTER] Sharkmode (`b05379b7d12bfb02149e03f166be5d496dc3e7687dcc042f0bf525cc46aa918b.tar.zst`) | 304,425 |
| Warp Zone (`b38698ececd60bdf311a15de205f43d0f3466eac6d41296b9abf469485ebab4d.tar.zst`) | 212,220 |
| Let Me Hear That (`0229b74d092e8f04278d64b7c994877a0a3bcc52e9d7ddb00d6a98f1f5244cd0.tar.zst`) | 205,071 |
| 272\|MODS\|[lv.02] Riddle (`920eb361ce688e70b4364258337b30c9e4bf7d337f2c83a30e59215eb838735d.tar.zst`) | 201,471 |

All six selected archives pass **1,650,933 comparisons** before and after. Coverage is limited to those selected archives; unavailable local-only fixtures are not claimed as tested.

## Reproduction

From this worktree in PowerShell:

```powershell
$env:CARGO_BUILD_JOBS = '4'
cargo test --release --locked --config profile.release.lto=false -p deadsync-theme-simply-love --lib -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false -p deadsync-theme-simply-love --lib benchmark_ -- --ignored --nocapture --test-threads=1
cargo test --release --locked --config profile.release.lto=false --test song_lua_itgmania_semantic_parity -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false --test itgmania_actor_conformance -- --test-threads=1
# Run separately for each archive filename above (no libtest flags):
cargo test --release --locked --config profile.release.lto=false --test full_song_lua -- '0f2ef3f987449e6218525fba450d1db1201c9f733d4956061d3b36529b922305.tar.zst'
```

Raw baseline/current logs, selectors and comparisons remain in this worktree's ignored `target/` directory. Builds reused `C:/GitHub/deadsync-perf-1859-runtime-reuse-20261008/target/build`; temporary files stayed within this worktree. No excluded input or automation file is committed.
