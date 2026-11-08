# Pack metadata and lobby text performance — 2026-10-10

Branch: `perf/1872-runtime-traversal-20261010`. Base: `4668154878bf69ec48f06a1d3ffb76ce760c823e` (committed main when this worktree was created).

DeadSync patch version: **0.5.1871 → 0.5.1872** in both Cargo.toml and Cargo.lock. This branch is intended for review and has not been merged or pushed.

## Changes

1. **Pack sync metadata:** `pack_ini_with_sync` borrows unchanged lines and uses static replacement strings. An exact-size slice supplies missing keys, avoiding a temporary vector inside `Vec::splice`. The final newline participates in the join, avoiding a final output-buffer growth. This removes per-line String allocation/copying while preserving line endings, comments, repeated sections, duplicate keys and insertion positions.
2. **Lobby HUD rebuilding:** score ordering now lives in its only caller. It sorts up to eight score-screen players in an inline SmallVec and streams other players in their original order. Original player indices fully break score ties, making unstable sorting equivalent to the original stable sort. The empty lobby returns before ordering. Larger scoring populations spill safely to the heap. No persistent cache or new runtime state is added.
3. **Selected-song lobby text:** title truncation appends directly to the final text buffer; chart and rate details use that same buffer. The title-length check reads only the bounded visible prefix instead of counting every Unicode scalar. The rate reserve covers typical values; unusually wide finite rates can grow the buffer normally. Output bytes and Unicode scalar truncation remain identical.

These are persistence and lobby rebuild paths. Lobby cache hits continue to share their existing actors; the measurements below do not imply an improvement to every rendered frame. Song Lua production code is unchanged.

## Paired measurements

Release builds used rustc 1.98.1 (LLVM 22.1.8), Windows x86-64 MSVC, on an Intel Xeon E5-2696 v4. LTO was disabled for both variants. Each benchmark runs frozen committed-main functions and current production functions in the same executable, with black-boxed inputs and consumed outputs. Inputs are prepared outside the measured work; output destruction is included.

Four fresh processes per test binary were pinned to logical CPU 2 at AboveNormal priority after this pass's builds and baseline native tests completed. Each process performs three warmups, calibrates toward a 25 ms batch (capped at one million iterations), then alternates old/new order for nine timed batches. The table reports the median of the four process medians; the range is the four paired process ratios. Allocation counts are separate scoped thread-local System-allocator observations, identical across the four runs. They count requested bytes, including realloc requests, rather than OS committed memory. Peak added live bytes is a logical allocation-lifetime measure; it excludes allocator-internal temporary overlap during realloc. This is an instrumented microbenchmark; it does not measure hardware CPU cycles or end-to-end network/disk latency.

Frozen reference bodies are checked against base Git objects, allowing only reference-name changes and formatting. The complete HUD body is benchmarked, including formatting, rather than a detached ordering helper.

| Case | Original ns/op | Current ns/op | Throughput | Paired range | Allocations | Reallocations | Requested bytes | Peak added live bytes |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| pack/empty | 133.84 | 135.91 | 0.985× | 0.984–1.004× | 1 → 1 | 0 → 0 | 88 → 88 | 88 → 88 |
| pack/typical | 1374.51 | 771.50 | 1.782× | 1.767–1.794× | 9 → 2 | 1 → 0 | 506 → 207 | 396 → 207 |
| pack/insert | 1451.77 | 809.99 | 1.792× | 1.739–1.795× | 10 → 2 | 1 → 0 | 626 → 254 | 486 → 254 |
| pack/large | 247483.04 | 108810.33 | 2.274× | 2.202–2.351× | 1006 → 2 | 1 → 0 | 155302 → 49129 | 122222 → 49129 |
| text/title | 175.03 | 82.06 | 2.133× | 2.116–2.206× | 2 → 1 | 0 → 0 | 39 → 25 | 39 → 25 |
| text/chart | 414.34 | 89.00 | 4.655× | 4.588–4.750× | 3 → 1 | 1 → 0 | 113 → 38 | 83 → 38 |
| text/rate | 753.00 | 346.75 | 2.172× | 2.159–2.242× | 4 → 1 | 1 → 0 | 125 → 41 | 87 → 41 |
| text/full | 876.70 | 359.42 | 2.439× | 2.411–2.520× | 4 → 1 | 2 → 0 | 145 → 46 | 95 → 46 |
| text/unicode | 1091.49 | 507.57 | 2.150× | 2.131–2.165× | 4 → 1 | 3 → 0 | 544 → 187 | 373 → 187 |
| text/long_title | 3508.66 | 497.69 | 7.050× | 6.822–7.356× | 4 → 1 | 3 → 0 | 544 → 187 | 373 → 187 |
| text/large_rate | 2177.39 | 1519.47 | 1.433× | 1.411–1.461× | 4 → 1 | 4 → 1 | 334 → 138 | 151 → 92 |
| text/long_label | 943.32 | 373.81 | 2.524× | 2.491–2.530× | 4 → 1 | 2 → 0 | 2478 → 633 | 1841 → 633 |
| hud/empty | 217.37 | 207.89 | 1.046× | 1.039–1.093× | 1 → 1 | 1 → 1 | 108 → 108 | 72 → 72 |
| hud/browse8 | 1051.96 | 854.50 | 1.231× | 1.185–1.283× | 2 → 1 | 1 → 0 | 996 → 804 | 932 → 804 |
| hud/score2 | 1171.95 | 1107.18 | 1.058× | 1.033–1.070× | 2 → 1 | 0 → 0 | 292 → 228 | 292 → 228 |
| hud/score8 | 3539.49 | 3377.20 | 1.048× | 1.039–1.090× | 2 → 1 | 1 → 0 | 996 → 804 | 932 → 804 |
| hud/mixed8 | 2258.72 | 2044.31 | 1.105× | 1.086–1.109× | 2 → 1 | 1 → 0 | 996 → 804 | 932 → 804 |
| hud/score32 | 13230.38 | 13306.49 | 0.994× | 0.979–1.046× | 2 → 2 | 3 → 1 | 4068 → 3876 | 3620 → 3620 |
| hud/mixed32 | 9610.80 | 9336.80 | 1.029× | 0.997–1.031× | 2 → 2 | 3 → 1 | 4068 → 3876 | 3620 → 3620 |
| hud/browse256 | 23465.09 | 22494.17 | 1.043× | 1.022–1.081× | 2 → 1 | 6 → 0 | 32740 → 24612 | 28708 → 24612 |

`pack/empty` exercises the unchanged new-file formatting path. `hud/empty` exercises the waiting-for-players output. Typical pack input has five lines; insertion input has BOM/CRLF and a following section; the large pack has 1,000 contributor lines. HUD cases include two/eight scoring players, mixed lobbies, a 32-player spill case and a 256-player non-score stress case. Text cases include no details, chart/rate details, Unicode truncation, a 36 KiB title, a long chart label and f32::MAX. Stress cases characterize scaling and fallback behavior, not an assertion that typical lobbies contain those inputs.

Measured cases below parity: `pack/empty` 0.985× (1.5% higher latency); `hud/score32` 0.994× (0.6% higher latency). Their paired ranges span parity; these measurements do not establish a speedup for those cases.

## Behavioral validation

Seven added regression/allocation tests pass. They compare the actual production outputs to frozen committed-main implementations: 1,024 generated INI rewrites plus explicit edge cases; score ordering and full HUD text across screen types, ties, signed zero, NaN/infinity, mixed populations and spill sizes; and title/detail combinations covering Unicode, whitespace, absent fields, subnormal and maximum finite rates. Existing truncation tests now also check that appending preserves a pre-existing output prefix. Three ignored tests provide the paired benchmarks.

| Suite | Passed | Failed | Ignored | Baseline comparison |
|---|---:|---:|---:|---|
| deadsync_shell-tests | 358 | 17 | 7 | All pre-existing outcomes and failure diagnostics match |
| deadsync_theme_simply_love-tests | 1267 | 1 | 5 | All pre-existing outcomes and failure diagnostics match |
| deadsync_simfile-tests | 203 | 0 | 0 | All pre-existing outcomes and failure diagnostics match |
| semantic | 151 | 41 | 77 | All pre-existing outcomes and failure diagnostics match |
| actor | 30 | 1 | 0 | All pre-existing outcomes and failure diagnostics match |

Total: **2009 passed, 60 baseline failures, 89 ignored**. The suites are not globally green. The 17 shell suite failures were rerun in isolated processes for both variants: 15 pass in isolation; the existing hidden-score footer fade and root-player proxy segment-count failures remain. The theme model-height and actor-alignment failures also remain. The full diagnostic comparison normalizes thread IDs and only two audited unordered diagnostics: the order of three missing Sprite.Load aliases and which of two missing non-local archives is reported first. Repeated unchanged-binary reruns verify those variations; all other failure text must match.

The Song Lua compatibility harness compares recorded ITGmania reference behavior. Six full-song captures run separately for baseline and current: both Epidermis captures, Sharkmode, Warp Zone, Let Me Hear That and Riddle.

| Capture | Comparisons per variant | Mismatches |
|---|---:|---:|
| 319\|TECH SOUP\|[lv.P.Clark] Epidermis (`0f2ef3f9`) | 363,873 | 0 |
| 319\|TECH SOUP\|[lv.P.Clark] Epidermis (`af2f887d`) | 363,873 | 0 |
| 280\|MODS\|[MASTER] Sharkmode (`b05379b7`) | 304,425 | 0 |
| Warp Zone (`ded0f7ff`) | 212,220 | 0 |
| Let Me Hear That (`2a77063d`) | 205,071 | 0 |
| 272\|MODS\|[lv.02] Riddle (`7ffacb89`) | 201,471 | 0 |

**1,650,933 native comparisons per variant; zero mismatches or panics.** These native tests are a broad integration guard. The Rust differential tests above directly cover the modified lobby presentation and INI rewriting behavior.

## Reproduction and integrity

```powershell
cargo test --release --locked --no-fail-fast --config profile.release.lto=false -p deadsync-shell -p deadsync-theme-simply-love -p deadsync-simfile --lib -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false -p deadsync-shell -p deadsync-theme-simply-love --lib benchmark_runtime_traversal -- --ignored --nocapture --test-threads=1
cargo test --release --locked --no-fail-fast --config profile.release.lto=false --test song_lua_itgmania_semantic_parity --test itgmania_actor_conformance -- --test-threads=1
```

Build and invoke the `full_song_lua` executable once per archive listed above to repeat the full-song comparisons. Ignored worktree `target/` artifacts contain the exact executable manifests and hashes, paired logs, native capture selectors/logs, differential results, audit data and validation runners. Native archive extraction uses one dedicated local TEMP directory for both variants, and the captures run sequentially. Generated Cargo artifacts reuse `C:/GitHub/deadsync-perf-1872-data-churn-20261009/target/build`; all source edits and the commit belong to this worktree.

The initial audit inspected 85 unmerged local/remote-tracking perf refs (55 distinct heads). The final audit found 85 refs (55 heads) with no overlap in the three selected production files. Original checkout main at final audit: `37b94e1416b49bfea3d1875e12f719e147bb0ab3`. Its final status was recorded separately in the ignored audit log; changes there were not made by this pass.

Cargo version changes were verified byte-for-byte as exactly +1. Rustfmt, whitespace checks, frozen-reference checks, source/executable hashes, and the explicit commit allowlist pass. The baseline executables were copied and hashed before editing source. The final tested executables and source hashes are checked again before committing. No dependency changes, merge or push are included. `deadsync-song.json.gz`, `rust-performance.md`, `optimize.sh` and `optimize.ps1` are excluded.

### Exact full-song selectors

```powershell
$captures = @(
    '0f2ef3f987449e6218525fba450d1db1201c9f733d4956061d3b36529b922305.tar.zst',
    'af2f887d212d8ab702b772cea9c953500254ce01a6b78b423276fca9cf5a8306.tar.zst',
    'b05379b7d12bfb02149e03f166be5d496dc3e7687dcc042f0bf525cc46aa918b.tar.zst',
    'ded0f7ff19513e30119e0d6f65b57a661f1683872fc9ffced27737db06c49993.tar.zst',
    '2a77063dd2ab8c43bbdbb9e101be7fcf73156302e21e9771c88797218317d327.tar.zst',
    '7ffacb89fd95ab788758e273ba878c30380c4622406f1bb22d8c51292ca78d83.tar.zst'
)
foreach ($capture in $captures) {
    cargo test --release --locked --config profile.release.lto=false --test full_song_lua -- $capture
}
```
