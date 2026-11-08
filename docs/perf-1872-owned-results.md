# Performance pass: content browser ownership

Branch: `perf/1872-owned-results-20261009`. Starting committed main: `374c24c4c3c41631d3a8e50962c7fec3ed94bad3`. Version: **0.5.1871 -> 0.5.1872**, updated in Cargo.toml and all three matching workspace entries in Cargo.lock.

Read rust-performance.md and checked 26 distinct unmerged performance branch heads. None changes the content browser files. Work is isolated in this pass's worktree and remains based on its starting main commit. The original checkout and its independent work are untouched. No merge or push was performed.

## Changes

1. **Banner requests:** iterate the featured indices and visible list/doubles windows by reference. Remove four temporary index-vector copies per populated call. Request order, duplicate requests, source precedence, lookahead windows and detail-page jackets are preserved.
2. **Doubles rendering:** borrow each existing column instead of copying the entire column before drawing its visible rows. Scrolling and scrollbar geometry still use the full column length. Work no longer includes copying every off-screen index on each frame.
3. **Detail facts:** consume the freshly built fact array, move its formatted values into actors, and use static storage for its eight fixed labels. Remove copies that were immediately discarded. Reserve exactly the author suffix length before appending it, so moving long author strings avoids retaining the old growth buffer. Text, layout, colors and loading spinners are unchanged.

All production changes are outside Song Lua and confined to these three paths (the fact table and its existing formatter). No new production helper or abstraction is introduced. Frozen originals from the starting commit are compiled only for regression tests and benchmarks.

## Benchmark method

Windows x86-64/MSVC, Intel Xeon E5-2696 v4 at 2.20 GHz, `rustc 1.98.1 (48a229cea 2026-09-01)`, release optimization with LTO disabled. Four build jobs, serial test execution. These measurements are function-level wall time and allocation traffic, not application FPS or CPU cycles. This pass runs no build or native compatibility job during benchmarks; unrelated workstation activity remains untouched.

Each case alternates original/current for ten sample pairs, discards the warm-up pair, and reports the median of nine samples per implementation. Batches calibrate fast cases toward approximately 2 ms. Fixtures are built outside timing. Render output vectors are preallocated outside timing; construction and destruction of generated actors are timed on both sides. Banner timing includes destruction of the returned requests. Inputs and outputs are black-boxed.

Thread-local counters wrap the System allocator and are enabled for separate allocation measurements. The wrapper remains installed during timing. Allocation counts exclude fixture/output-vector preparation and later destruction of retained outputs. Requested bytes include realloc requests; they are not peak RSS. Returned owned text capacity is measured separately, including any spare capacity retained by moving formatted strings.

## Timing results

| Case | Original ns/op | Current ns/op | Throughput ratio |
|---|---:|---:|---:|
| facts-idle | 11,492.00 | 10,703.67 | 1.07x |
| facts-loading | 12,968.40 | 12,100.00 | 1.07x |
| facts-ready | 16,546.80 | 14,167.71 | 1.17x |
| facts-long-authors | 15,106.00 | 15,165.20 | 1.00x |
| facts-error | 16,983.20 | 14,765.60 | 1.15x |
| doubles-0-top-no-art | 5,673.91 | 6,125.00 | 0.93x |
| doubles-1-top-no-art | 11,248.67 | 10,821.33 | 1.04x |
| doubles-24-top-no-art | 65,384.00 | 63,492.00 | 1.03x |
| doubles-200-top-no-art | 63,956.67 | 66,582.50 | 0.96x |
| doubles-9500-top-no-art | 76,253.33 | 60,212.50 | 1.27x |
| doubles-9500-scrolled-no-art | 76,923.33 | 59,232.00 | 1.30x |
| doubles-9500-scrolled-waiting-art | 68,410.00 | 62,060.00 | 1.10x |
| banners-0-missing-list | 22.84 | 16.46 | 1.39x |
| banners-1-urls-list | 601.19 | 393.31 | 1.53x |
| banners-24-missing-list | 1,704.83 | 1,351.00 | 1.26x |
| banners-24-urls-list | 5,933.00 | 5,710.00 | 1.04x |
| banners-200-urls-list | 6,171.00 | 5,704.50 | 1.08x |
| banners-9500-urls-list | 5,528.50 | 5,075.25 | 1.09x |
| banners-200-urls-detail | 6,831.00 | 6,346.50 | 1.08x |

The primary table is the first complete run of the final revision, including controls.

Three fixed repeats used the same binary. Results for every initially slower case follow.

| Repeat | Case | Original ns/op | Current ns/op | Current time change |
|---:|---|---:|---:|---:|
| 1 | facts-long-authors | 11,810.00 | 9,058.80 | -23.30% |
| 1 | doubles-0-top-no-art | 4,900.29 | 4,819.44 | -1.65% |
| 1 | doubles-200-top-no-art | 47,542.00 | 47,570.00 | +0.06% |
| 2 | facts-long-authors | 11,956.00 | 10,100.00 | -15.52% |
| 2 | doubles-0-top-no-art | 4,506.77 | 4,528.79 | +0.49% |
| 2 | doubles-200-top-no-art | 56,204.00 | 54,864.00 | -2.38% |
| 3 | facts-long-authors | 12,505.50 | 9,880.67 | -20.99% |
| 3 | doubles-0-top-no-art | 4,712.22 | 4,821.00 | +2.31% |
| 3 | doubles-200-top-no-art | 61,425.00 | 59,104.00 | -3.78% |

The initial long-author result was effectively flat (+0.39% time); all three fixed repeats were faster (15.52% to 23.30% less time). The initially slower 200-pack doubles case (+4.11%) repeated at +0.06%, -2.38%, and -3.78%. The empty doubles control performs no column copying and has identical allocation traffic: its repeats varied from -1.65% to +2.31%, so no timing improvement is claimed there. The 24-pack doubles case also varied (two repeats about 14% faster, one +0.74%). Larger doubles lists, banner requests, and ready-state facts improved in every final-revision run. Allocation and retained-capacity results were identical across all four runs; they provide the deterministic evidence for removed work.

## Allocation traffic and retained text

Cells are **allocations / reallocations / frees / requested bytes / freed bytes** per call. Rendering counts exclude the output vector reserved before measurement.

| Case | Original | Current |
|---|---:|---:|
| facts-idle | 19 / 1 / 7 / 171 / 80 | 7 / 1 / 3 / 80 / 34 |
| facts-loading | 23 / 1 / 7 / 363 / 80 | 11 / 1 / 3 / 272 / 34 |
| facts-ready | 26 / 2 / 10 / 308 / 155 | 10 / 2 / 2 / 144 / 34 |
| facts-long-authors | 26 / 2 / 10 / 10,156 / 7,541 | 10 / 2 / 2 / 5,068 / 2,496 |
| facts-error | 27 / 1 / 11 / 187 / 88 | 11 / 1 / 3 / 88 / 34 |
| doubles-0-top-no-art | 4 / 0 / 0 / 54 / 0 | 4 / 0 / 0 / 54 / 0 |
| doubles-1-top-no-art | 13 / 3 / 8 / 358 / 238 | 12 / 3 / 7 / 350 / 230 |
| doubles-24-top-no-art | 130 / 30 / 100 / 4,162 / 3,220 | 128 / 30 / 98 / 3,970 / 3,028 |
| doubles-200-top-no-art | 130 / 30 / 100 / 5,570 / 4,628 | 128 / 30 / 98 / 3,970 / 3,028 |
| doubles-9500-top-no-art | 130 / 30 / 100 / 79,970 / 79,028 | 128 / 30 / 98 / 3,970 / 3,028 |
| doubles-9500-scrolled-no-art | 130 / 30 / 100 / 80,046 / 79,066 | 128 / 30 / 98 / 4,046 / 3,066 |
| doubles-9500-scrolled-waiting-art | 144 / 30 / 100 / 80,718 / 79,066 | 142 / 30 / 98 / 4,718 / 3,066 |
| banners-0-missing-list | 0 / 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 / 0 |
| banners-1-urls-list | 7 / 0 / 3 / 248 / 24 | 4 / 0 / 0 / 224 / 0 |
| banners-24-missing-list | 4 / 0 / 4 / 352 / 352 | 0 / 0 / 0 / 0 / 0 |
| banners-24-urls-list | 49 / 4 / 4 / 5,728 / 2,272 | 45 / 4 / 0 / 5,376 / 1,920 |
| banners-200-urls-list | 49 / 4 / 4 / 5,728 / 2,272 | 45 / 4 / 0 / 5,376 / 1,920 |
| banners-9500-urls-list | 49 / 4 / 4 / 5,728 / 2,272 | 45 / 4 / 0 / 5,376 / 1,920 |
| banners-200-urls-detail | 56 / 4 / 4 / 5,959 / 2,272 | 52 / 4 / 0 / 5,607 / 1,920 |

| Retained owned text capacity (bytes) | Original | Current |
|---|---:|---:|
| facts-idle | 91 | 46 |
| facts-loading | 91 | 46 |
| facts-ready | 153 | 110 |
| facts-long-authors | 2,615 | 2,572 |
| facts-error | 99 | 54 |
| doubles-0-top-no-art | 54 | 54 |
| doubles-1-top-no-art | 120 | 120 |
| doubles-24-top-no-art | 942 | 942 |
| doubles-200-top-no-art | 942 | 942 |
| doubles-9500-top-no-art | 942 | 942 |
| doubles-9500-scrolled-no-art | 980 | 980 |
| doubles-9500-scrolled-waiting-art | 980 | 980 |

## Regression checks

- Theme release tests: **1270 passed, 1 failed, 5 ignored**. All 1264 baseline outcomes are preserved. Nine new regression tests pass; three new benchmark tests pass when run explicitly.
- Banner cases compare complete ordered requests, duplicates, invalid indices, empty/missing artwork, source precedence, scroll windows and detail jackets. The no-art populated case verifies zero allocation churn after removing all four index copies.
- Doubles cases compare full actor properties for empty, short and 9,500-pack lists, both columns/focus states, scrolled/out-of-range windows, duplicate/invalid indices, loading/error states and failed artwork. Allocation checks prove the removed byte count equals the two column lengths times `size_of::<usize>()`.
- Detail cases compare all labels and actor properties across idle/loading/ready/error phases, three widths, mismatched pages, partial metadata, extreme counts/sizes and long Unicode author strings. Tests verify static label storage, removal of the eight label allocations plus each nonempty value copy, and no increase in retained text capacity for the measured states.
- Actor comparisons normalize only text storage (`Owned` versus `Static`) to compare bytes and all other fields. They capture original/current within one real spinner clock cell; retries happen only when that clock cell changes, never for mismatching output. Clock cells themselves are not removed from the comparison.
- Edited Rust files pass rustfmt checks. Frozen originals, exact production scope, patch bump, pending branches, file allowlist and Git whitespace audits pass.

The one existing theme failure is `screens::components::gameplay::notefield::tests::cyber_model_tap_scale_uses_model_height_not_logical_height`. Its guard reports equal logical/model heights (`60.162445`) at `notefield/tests.rs:421`. The complete diagnostic is unchanged after normalizing the panic thread ID. This pass leaves that separate noteskin compatibility issue untouched.

## Song Lua / ITGmania compatibility

Fresh baseline/current builds run the native semantic/actor harness and the same six selected full-song archives. Browser output regressions are covered separately by the frozen-original actor comparisons above; the native harness is additional compatibility coverage.

| Suite | Passed | Failed | Ignored | Baseline/current comparison |
|---|---:|---:|---:|---|
| semantic | 143 | 37 | 77 | All 257 outcomes and 37 failure diagnostics unchanged |
| actor | 30 | 1 | 0 | All 31 outcomes and 1 failure diagnostics unchanged |

The native suites are **not fully green**: their 38 pre-existing failures remain. Diagnostic comparisons normalize panic thread IDs and the ordering of known unordered `Sprite.Load` lines; all other diagnostic content and ordering must match.

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
