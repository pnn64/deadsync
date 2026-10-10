# Overlay preparation ownership — 2026-10-10

Branch: `perf/1872-overlay-ownership-20261010`. Starting committed main: `f686ea82f94ef92967f791f8dede7494f492c020`. DeadSync patch version: **0.5.1871 → 0.5.1872**, in both Cargo.toml and Cargo.lock. This worktree remains unmerged.

## Three changes

1. **Updater:** retain translated titles, body lines, and footers in their existing shared storage. Remove the Arc → String → retained-text round trip and prepare the body vector directly in its final element type. The existing public `phase_strings` API still constructs owning strings directly through the same phase selection; its allocation counts are checked against the original.
2. **FFmpeg installer:** pass translations and formatted translation results directly into the retained panel. Preserve version tags, progress/ETA/speed text, and animated footers.
3. **Workshop installer:** replace the temporary zero/one-line body vector with an optional line and append directly to the final vector. Retain translated text directly as in the other panels.

The shared constructor still uses inline storage for short text and shared storage for long text. Rendering, input handling, progress formatting, and truncation bodies are unchanged. No Song Lua production code changed. These are phase-preparation improvements, including download progress refreshes; rendering already reuses cached panels. No game-wide FPS claim is made.

## Paired release benchmarks

Host: Windows x86-64, Intel Xeon E5-2696 v4 (22 cores / 44 logical processors), Rust 1.98.1 (`48a229cea`), LLVM 22.1.8, MSVC target.

Each original function is frozen from the starting commit in test-only code; its body was checked against Git. Original and optimized implementations execute in the same release test binary with `profile.release.lto=false`, warm translation caches, black-boxed inputs/results, and result destruction included. Each process alternates nine timed batches per variant after calibration (approximately 25 ms per batch). Four fresh processes were pinned to logical CPU 2 at AboveNormal priority. Formal timing started after this pass’s builds and baseline native run completed. The table uses medians of the four process medians. Allocation counters are disabled during timing and measured separately with a thread-local System allocator wrapper. Bytes mean requested allocation/reallocation bytes, not process RSS.

| Preparation | Original ns/op | Current ns/op | Throughput | Allocations | Reallocations | Requested bytes | Peak added live bytes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| ffmpeg/idle | 3.28 | 7.83 | 0.419× | 0 → 0 | 0 → 0 | 0 → 0 | 0 → 0 |
| ffmpeg/checking | 883.81 | 418.62 | 2.111× | 6 → 1 | 0 → 0 | 189 → 24 | 153 → 24 |
| ffmpeg/unsupported | 1164.05 | 499.75 | 2.329× | 8 → 1 | 0 → 0 | 394 → 48 | 309 → 48 |
| ffmpeg/download | 3969.94 | 3766.98 | 1.054× | 15 → 12 | 8 → 8 | 441 → 354 | 246 → 198 |
| ffmpeg/confirm | 2777.03 | 1666.39 | 1.666× | 17 → 5 | 1 → 1 | 689 → 186 | 431 → 176 |
| updater/idle | 3.44 | 3.44 | 1.001× | 0 → 0 | 0 → 0 | 0 → 0 | 0 → 0 |
| updater/checking | 863.84 | 427.80 | 2.019× | 6 → 1 | 0 → 0 | 158 → 24 | 124 → 24 |
| updater/ready | 983.39 | 563.52 | 1.745× | 7 → 2 | 0 → 0 | 188 → 33 | 148 → 33 |
| updater/download | 2570.66 | 2371.52 | 1.084× | 12 → 9 | 4 → 4 | 353 → 281 | 224 → 184 |
| updater/confirm | 4643.82 | 3273.64 | 1.419× | 25 → 11 | 2 → 2 | 848 → 424 | 417 → 328 |
| workshop/idle | 8.62 | 8.18 | 1.055× | 0 → 0 | 0 → 0 | 0 → 0 | 0 → 0 |
| workshop/preparing | 1510.90 | 700.11 | 2.158× | 11 → 2 | 0 → 0 | 389 → 56 | 294 → 56 |
| workshop/installed | 1323.81 | 532.31 | 2.487× | 10 → 1 | 0 → 0 | 460 → 48 | 368 → 48 |
| workshop/download | 2256.57 | 1655.92 | 1.363× | 13 → 6 | 2 → 2 | 376 → 168 | 240 → 120 |
| workshop/error | 1786.48 | 1133.29 | 1.576× | 10 → 3 | 1 → 1 | 780 → 524 | 568 → 424 |

Idle controls allocate nothing in both variants; their nanosecond timings are not claimed as improvements. The FFmpeg idle control measured 3.28 → 7.83 ns (about +4.55 ns) consistently across the four processes. Its production `prepare` function, including the early return, is byte-for-byte unchanged from the starting commit. This small timing cost is retained here as a measured limitation; no universal throughput improvement is claimed. All active benchmark cases reduce allocation churn and requested bytes.

| Active case | Paired throughput range across four processes |
| --- | ---: |
| ffmpeg/checking | 1.948–2.140× |
| ffmpeg/unsupported | 2.181–2.403× |
| ffmpeg/download | 1.025–1.087× |
| ffmpeg/confirm | 1.612–1.694× |
| updater/checking | 1.972–2.068× |
| updater/ready | 1.658–1.851× |
| updater/download | 1.068–1.108× |
| updater/confirm | 1.387–1.450× |
| workshop/preparing | 2.118–2.199× |
| workshop/installed | 2.354–2.632× |
| workshop/download | 1.346–1.431× |
| workshop/error | 1.534–1.591× |

Reproduce the paired benchmarks from this branch:

```powershell
cargo test --release --locked --config profile.release.lto=false -p deadsync-theme-simply-love --lib benchmark_overlay_ownership -- --ignored --nocapture --test-threads=1
```

## Behavioral and native validation

Eight new regression tests pass. They cover 644 phase configurations (115 updater, 496 FFmpeg, 33 Workshop), all footer animation frames, full deterministic actor equality at two colors, Unicode/version/truncation boundaries, known/unknown/zero totals, all error categories, allocation reductions, retained Arc identity, and unchanged allocation churn in the public string helper. Only wall-clock frame selection is frozen for actor comparisons; spinner enablement and every prepared footer frame are checked separately.

Both baseline and current release builds include the root library, theme and simfile libraries, native semantic harness, native actor conformance harness, and full-song capture harness. The root library test executable is built, not run.

| Executed suite | Baseline tests | Current passed | Current failed | Current ignored |
| --- | ---: | ---: | ---: | ---: |
| deadsync_theme_simply_love-tests | 1267 | 1271 | 1 | 6 |
| deadsync_simfile-tests | 203 | 203 | 0 | 0 |
| semantic | 264 | 146 | 41 | 77 |
| actor | 31 | 30 | 1 | 0 |

Every pre-existing test outcome is unchanged. All **43 existing failures** have matching diagnostics. Normalization is limited to thread IDs, proven unordered Sprite.Load diagnostic lines, and the first reported member of two verified missing non-local reference archives. Repeated unchanged-binary runs established both ordering variations. This is not an all-green test suite. The theme failure is `cyber_model_tap_scale_uses_model_height_not_logical_height`; the actor failure is `geometry::lua_align_matches_native`. The semantic failures include pre-existing parity gaps and the missing-reference check.

Six full-song ITGmania reference captures pass in **both** variants:

| Capture | Archive | Comparisons per variant |
| --- | --- | ---: |
| 319\|TECH SOUP\|[lv.P.Clark] Epidermis | `0f2ef3f987449e6218525fba450d1db1201c9f733d4956061d3b36529b922305.tar.zst` | 363,873 |
| 319\|TECH SOUP\|[lv.P.Clark] Epidermis | `af2f887d212d8ab702b772cea9c953500254ce01a6b78b423276fca9cf5a8306.tar.zst` | 363,873 |
| 280\|MODS\|[MASTER] Sharkmode | `b05379b7d12bfb02149e03f166be5d496dc3e7687dcc042f0bf525cc46aa918b.tar.zst` | 304,425 |
| Warp Zone | `ded0f7ff19513e30119e0d6f65b57a661f1683872fc9ffced27737db06c49993.tar.zst` | 212,220 |
| Let Me Hear That | `2a77063dd2ab8c43bbdbb9e101be7fcf73156302e21e9771c88797218317d327.tar.zst` | 205,071 |
| 272\|MODS\|[lv.02] Riddle | `7ffacb89fd95ab788758e273ba878c30380c4622406f1bb22d8c51292ca78d83.tar.zst` | 201,471 |

**1,650,933 comparisons per variant, zero mismatches** across those captures. Baseline and current used identical assets and the same local temporary extraction directory, sequentially. All 848 noteskin files were verified byte-for-byte against the baseline Git blobs before testing.

Validation commands:

```powershell
cargo test --release --locked --config profile.release.lto=false -p deadsync-theme-simply-love -p deadsync-simfile --lib -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false --test song_lua_itgmania_semantic_parity -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false --test itgmania_actor_conformance -- --test-threads=1
# Run each archive from the table as a selector:
cargo test --release --locked --config profile.release.lto=false --test full_song_lua -- <archive.tar.zst>
```

Formatting, Git whitespace checks, exact patch-version checks, frozen-function checks, and source/tested-binary SHA-256 checks passed. The final release build emitted no compiler warnings.

## Isolation and pending work

The initial audit covered 82 unmerged local/remote perf references, 52 distinct heads. None changed the three production files selected here. The final audit covered 82 references and 52 distinct heads; any new references were checked for overlapping production changes.

All source edits and the commit belong to the separate `D:/deadsync-perf-1872-overlay-ownership-20261010` worktree. The original checkout was only read with optional Git locks disabled; its uncommitted work was not edited, staged, stashed, or reset. It could advance independently during this pass. Shared Cargo cache access was confined to generated build artifacts. No merge or push was performed.

The commit excludes `deadsync-song.json.gz`, `rust-performance.md`, `optimize.sh`, and `optimize.ps1`. Local raw logs, hashes, branch audits, and benchmark samples remain in this worktree’s ignored `target/` directory.
