# Performance pass: resource traversal and model assembly

Branch: `perf/1872-resource-traversal-20261009`. Starting committed main: `3a64350287c8fda7f3d0bfa697cfa39c40c4af2c` (`test(song-lua): refresh native spooky archive`). Version: **0.5.1871 -> 0.5.1872**, updated in Cargo.toml and all three inheriting Cargo.lock packages.

Audited 65 unmerged local/remote perf refs covering 39 distinct heads, both when selecting work and before committing. None touches the four production files in this pass. All source edits are confined to the separate worktree. The original checkout continued to receive independent work and was not edited, cleaned, reset, or stashed by this pass. Nothing was merged or pushed.

## Changes

1. **Reuse the asset search path buffer.** `AssetPaths::resolve_asset_path` no longer allocates a fallback path before finding a hit, or a new candidate for every search root. It clears and reuses a local PathBuf. Absolute inputs, filesystem checks, overlay precedence, and unresolved fallback paths keep their original behavior. No filesystem result is cached.
2. **Reuse the noteskin source hashing buffer.** `source_hash` clears one byte buffer between files instead of allocating and freeing each file's contents. File labels, sorting, hash inputs, compiler schema/version, and error text are unchanged. Storage is local to one call; it retains the largest capacity until that call finishes. No cache or streaming-hash semantic change is introduced.
3. **Collect merged model vertices directly into the final Arc.** The native two-equal-name mesh rule previously copied both input slices into a Vec, then copied that Vec into an Arc. Collecting the chained slices directly into Arc removes the intermediate full vertex buffer. Bounds, the first mesh's material/bone binding, and the retained second draw are preserved. No Song Lua production file changes.

The production changes introduce no dependency, public API, runtime abstraction, or unsafe code. Frozen originals, regression tests, and allocation instrumentation are all behind cfg(test).

## Regression and native compatibility checks

Fresh release baseline and current library builds used `--locked --config profile.release.lto=false`. All **1,828 current library tests passed** (baseline: 1,822). Six new regression tests cover equivalence and allocation reductions; three ignored paired benchmarks were run explicitly. Frozen function bodies were verified against the starting commit after formatting.

Path coverage includes overlay precedence, successive live removals, Unicode, directories, absolute inputs, empty/dot/dot-dot inputs, invalid NUL paths, missing files and empty root lists. Hash coverage includes mixed file sizes, empty files, Unicode and mixed-case names, ignored files, duplicate/missing/empty roots, game variants and changes to file contents. Model coverage compares full layers and bitwise vertex/bounds fields for matching/different names, empty and large meshes, the native fixture, a third empty mesh, malformed indices/nonfinite normals and invalid files.

The Song Lua compatibility harness replayed the same six recorded native ITGmania full-song captures on baseline and current: both Epidermis archives, Sharkmode, Warp Zone, Let Me Hear That, and Riddle. All **1,650,933 comparisons passed** in each build. The focused semantic/actor suites retained **40 pre-existing failures** (39 semantic, one actor); all original outcomes and failure diagnostics matched. The comparison only normalizes panic thread IDs and sorts the known unordered missing-actor diagnostic lines in `image_texture_aliases_match_native_draws`. Existing ignored tests were not enabled.

Additionally, all **nine native model playback tests passed** with `test-support`, including the exact mesh-merge fixture, camera/signed-scale cases, texture history/replay/commands, and material passes/mapping. These compare the current production paths against checked-in ITGmania captures. Captures were not regenerated. This extra focused playback run was performed on current; the fresh baseline/current library tests and frozen parser differential tests separately establish unchanged parser behavior.

| Suite | Passed | Failed | Ignored |
| --- | ---: | ---: | ---: |
| deadsync_simfile-tests | 203 | 0 | 0 |
| deadsync_gameplay-tests | 794 | 0 | 0 |
| deadsync_chart-tests | 32 | 0 | 0 |
| deadsync_core-tests | 9 | 0 | 0 |
| deadsync_rules-tests | 111 | 0 | 0 |
| deadlib_present-tests | 178 | 0 | 0 |
| deadsync_noteskin-tests | 267 | 0 | 2 |
| deadsync_config-tests | 234 | 0 | 1 |
| semantic | 142 | 39 | 77 |
| actor | 30 | 1 | 0 |

## Benchmark method

Windows x86_64, Intel Xeon E5-2696 v4 (22 cores / 44 logical processors), Ultimate Performance plan, rustc 1.98.1 / LLVM 22.1.8. Release optimization level 3; LTO disabled equally. Actual crate test binaries contain both frozen original and current functions. Inputs/outputs pass through black_box; output destruction is included. Fixtures are prepared outside measurement; filesystem caches are warm.

Each case calibrates an initial sample to approximately 5 ms and discards it, then alternates variant order across nine measured batches. Four fresh process runs execute all 24 cases, pinned to logical CPU 2 at AboveNormal priority. Tables show medians across those four per-process medians. Paired ranges are the min/max original/current ratio across processes, not confidence intervals. This is a shared host, not an isolated CPU/frequency environment; no build started by this pass overlapped the final benchmarks.

Allocation counters are thread-local and disabled for timing. Untimed measurements count System allocator calls, reallocations, and requested bytes across each entire operation. Bytes include allocator requests during growth, not peak live memory, RSS or actual heap usable sizes. Filesystem and directory traversal allocations are included. Temporary directory names contain process IDs; their varying length can produce fractional median byte counts across processes. Each pair uses the same fixture.

The shared Cargo cache at `C:/GitHub/deadsync-perf-1872-data-churn-20261009/target/build` was reused; source changes, logs, scripts and benchmark fixtures stay in this pass's worktree. Source SHA-256 hashes were frozen before final benchmark/native replay and verified again before committing.

## Results and limits

- Searching 16 roots and finding the last asset reduced allocation calls from **65 to 49** (24.6%); misses reduced them from **65 to 50**. Windows filesystem work dominates this end-to-end operation, so there is no consistent throughput claim.
- Hashing 16 files of 256 KiB reduced requested bytes from **4,228,792 to 296,632** (93.0%) and allocations from **145 to 130**, with **1.022x** measured throughput. With 64 files of 4 KiB, allocations fell **529 to 466** and requested bytes fell **66.5%**, while timing was essentially flat.
- Parsing two merging meshes of 1,024 triangles each removed one **245,760-byte** intermediate vertex buffer, reducing whole-parser requested bytes **30.9%**, and improved throughput **1.233x**. At 8,192 triangles per mesh, it removed **1,966,080 bytes** and improved throughput **1.128x**.

All controls and slower medians are retained below. The largest slower median is about 2.2% (empty hash); some small-file, empty-model and path cases also shifted down slightly. The measurements establish allocation savings and model-parser throughput gains, not universal latency improvements or a whole-game FPS improvement. No behavioral regression was observed in the executed checks; the existing compatibility failures remain.

| Case | Original ns/op | Current ns/op | Throughput | Paired range |
| --- | ---: | ---: | ---: | ---: |
| `hash/0/0` | 239307.70 | 244659.36 | 0.978x | 0.962-1.001x |
| `hash/1/0` | 496856.25 | 499832.78 | 0.994x | 0.938-1.022x |
| `hash/1/4096` | 336025.00 | 340555.00 | 0.987x | 0.949-0.994x |
| `hash/16/4096` | 2783625.00 | 2780825.00 | 1.001x | 0.998-1.011x |
| `hash/64/4096` | 10354900.00 | 10396500.00 | 0.996x | 0.973-1.029x |
| `hash/16/262144` | 3772100.00 | 3690350.00 | 1.022x | 1.012-1.054x |
| `model/0/merge=true` | 104637.50 | 106617.77 | 0.981x | 0.971-1.006x |
| `model/1/merge=true` | 112837.50 | 113010.34 | 0.998x | 0.988-1.018x |
| `model/64/merge=true` | 181662.50 | 170278.57 | 1.067x | 1.047-1.122x |
| `model/1024/merge=true` | 960250.00 | 778858.33 | 1.233x | 1.166-1.252x |
| `model/8192/merge=true` | 6239500.00 | 5530600.00 | 1.128x | 1.109-1.170x |
| `model/1024/merge=false` | 664950.00 | 641168.75 | 1.037x | 0.971-1.089x |
| `path/1/first.png` | 120065.28 | 117978.01 | 1.018x | 0.977-1.033x |
| `path/1/last.png` | 111090.64 | 110881.56 | 1.002x | 0.972-1.006x |
| `path/1/missing.png` | 43801.18 | 42782.53 | 1.024x | 0.994-1.033x |
| `path/1/absolute` | 115.34 | 108.57 | 1.062x | 1.021-1.189x |
| `path/4/first.png` | 110548.72 | 112938.24 | 0.979x | 0.954-1.011x |
| `path/4/last.png` | 240060.62 | 244926.95 | 0.980x | 0.951-1.016x |
| `path/4/missing.png` | 168101.79 | 169420.23 | 0.992x | 0.982-1.007x |
| `path/4/absolute` | 112.11 | 107.88 | 1.039x | 0.981-1.079x |
| `path/16/first.png` | 109941.09 | 110353.85 | 0.996x | 0.989-1.009x |
| `path/16/last.png` | 804180.00 | 807471.43 | 0.996x | 0.981-1.010x |
| `path/16/missing.png` | 745435.71 | 726133.93 | 1.027x | 0.987-1.056x |
| `path/16/absolute` | 107.73 | 107.66 | 1.001x | 0.975-1.051x |

| Case | Allocations original -> current | Reallocations original -> current | Median requested bytes original -> current |
| --- | ---: | ---: | ---: |
| `hash/0/0` | 15 -> 15 | 13 -> 13 | 4,072.0 -> 4,072.0 |
| `hash/1/0` | 24 -> 24 | 18 -> 18 | 6,045.0 -> 6,045.0 |
| `hash/1/4096` | 25 -> 25 | 18 -> 18 | 10,141.0 -> 10,141.0 |
| `hash/16/4096` | 145 -> 130 | 95 -> 95 | 100,024.0 -> 38,584.0 |
| `hash/64/4096` | 529 -> 466 | 337 -> 337 | 388,264.0 -> 130,216.0 |
| `hash/16/262144` | 145 -> 130 | 95 -> 95 | 4,228,792.0 -> 296,632.0 |
| `model/0/merge=true` | 5 -> 5 | 0 -> 0 | 778.0 -> 778.0 |
| `model/1/merge=true` | 19 -> 18 | 0 -> 0 | 2,540.0 -> 2,300.0 |
| `model/64/merge=true` | 19 -> 18 | 0 -> 0 | 51,430.0 -> 36,070.0 |
| `model/1024/merge=true` | 19 -> 18 | 0 -> 0 | 796,394.0 -> 550,634.0 |
| `model/8192/merge=true` | 19 -> 18 | 0 -> 0 | 6,358,764.0 -> 4,392,684.0 |
| `model/1024/merge=false` | 17 -> 17 | 0 -> 0 | 304,858.0 -> 304,858.0 |
| `path/1/first.png` | 5 -> 4 | 6 -> 6 | 2,389.5 -> 2,380.5 |
| `path/1/last.png` | 5 -> 4 | 6 -> 6 | 2,386.5 -> 2,378.5 |
| `path/1/missing.png` | 5 -> 5 | 6 -> 6 | 2,395.5 -> 2,395.5 |
| `path/1/absolute` | 1 -> 1 | 0 -> 0 | 125.5 -> 125.5 |
| `path/4/first.png` | 5 -> 4 | 6 -> 6 | 2,389.5 -> 2,380.5 |
| `path/4/last.png` | 17 -> 13 | 24 -> 24 | 9,522.0 -> 9,167.5 |
| `path/4/missing.png` | 17 -> 14 | 24 -> 24 | 9,549.0 -> 9,202.5 |
| `path/4/absolute` | 1 -> 1 | 0 -> 0 | 125.5 -> 125.5 |
| `path/16/first.png` | 5 -> 4 | 6 -> 6 | 2,389.5 -> 2,380.5 |
| `path/16/last.png` | 65 -> 49 | 96 -> 96 | 38,064.0 -> 36,323.5 |
| `path/16/missing.png` | 65 -> 50 | 96 -> 96 | 38,163.0 -> 36,430.5 |
| `path/16/absolute` | 1 -> 1 | 0 -> 0 | 125.5 -> 125.5 |

## Reproduction

Run from this worktree, with a writable TEMP/TMP directory. The normal test command excludes the three ignored benchmarks; the second runs the frozen original/current comparisons in the same release binaries.

```powershell
cargo test --release --locked --config profile.release.lto=false -p deadsync-core -p deadsync-chart -p deadsync-rules -p deadsync-gameplay -p deadsync-simfile -p deadlib-present -p deadsync-noteskin -p deadsync-config --lib -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false -p deadsync-config -p deadsync-noteskin --lib benchmark_resource_traversal -- --ignored --nocapture --test-threads=1
cargo test --release --locked --config profile.release.lto=false -p deadsync --test song_lua_itgmania_semantic_parity -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false -p deadsync --test itgmania_actor_conformance -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false -p deadsync-song-lua --features test-support --test playback native_model_ -- --test-threads=1
```

The semantic and actor commands retain the documented baseline failures. Run each archive below with `cargo test --release --locked --config profile.release.lto=false -p deadsync --test full_song_lua -- ARCHIVE`:

- `0f2ef3f987449e6218525fba450d1db1201c9f733d4956061d3b36529b922305.tar.zst`
- `af2f887d212d8ab702b772cea9c953500254ce01a6b78b423276fca9cf5a8306.tar.zst`
- `b05379b7d12bfb02149e03f166be5d496dc3e7687dcc042f0bf525cc46aa918b.tar.zst`
- `ded0f7ff19513e30119e0d6f65b57a661f1683872fc9ffced27737db06c49993.tar.zst`
- `2a77063dd2ab8c43bbdbb9e101be7fcf73156302e21e9771c88797218317d327.tar.zst`
- `920eb361ce688e70b4364258337b30c9e4bf7d337f2c83a30e59215eb838735d.tar.zst`

Local evidence: `target/baseline-*.log`, `target/current-*.log`, `target/final-{1,2,3,4}.log`, `target/benchmark-summary.json`, `target/compatibility-comparison.json`, `target/source-hashes.json`, and `target/pending-audit.json`. These are uncommitted build artifacts; the tests and full summarized results are committed here. The four user-excluded files are absent from the commit.
