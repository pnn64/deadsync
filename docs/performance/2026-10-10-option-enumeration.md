# Option enumeration performance

Branch: `perf/1872-option-enumeration-20261010`.
Base: `aa38cfffb493f7906049b78c7378b6b0451496b1`, the latest committed local main when this worktree was created.
Worktree: `D:/deadsync-perf-1874-runtime-copy-removal-20261010`.
Version: **0.5.1871 -> 0.5.1872**, exactly once in `Cargo.toml` and all three workspace package entries in `Cargo.lock`.

## Changes

1. Audio sample-rate value/index lookups traverse borrowed rates and stop at the requested result. They retain first-occurrence order, Auto, empty-device fallbacks, duplicate handling, and out-of-range behavior without building temporary vectors. The two materialized-list functions used for labels are byte-for-byte unchanged from the base source.
2. Resolution rebuilding filters monitor modes into the existing options-state vector, appends presets or the selected physical mode, then sorts and deduplicates once. This removes an intermediate list, a second sort, the uniqueness wrapper, and allocated preset lists. Ascending display order and non-square-pixel mode retention are preserved.
3. Refresh-rate rebuilding reuses its state vector and preserves Default independently of an advertised zero rate. FPS seeding scans for a maximum directly, eliminating a sorted temporary vector. Nearest-rate selection, ties, the strict 10 Hz cutoff, cursor updates, and FPS reseeding remain intact.

The two production files contain 65 added and 71 removed lines. No new cache or persistent state fields were added. The retained graphics buffers belong to the options screen and retain their high-water capacities across rebuilds; a later smaller catalog or windowed mode does not shrink them. This exchanges some retained spare capacity for eliminating repeated allocations. These measurements cover menu preparation and selection work, not gameplay FPS or hardware discovery.

## Benchmarks

Windows x86-64 MSVC; Intel Xeon E5-2696 v4 at 2.20 GHz, 22 cores / 44 logical processors; rustc 1.98.1 (48a229cea), LLVM 22.1.8. Release builds use `--locked --config profile.release.lto=false` for both variants.

Each row is the median of four fresh-process medians. Each process runs three warmups and nine alternating timed batches after calibration, pinned to logical CPU 2 at AboveNormal priority. The paired range is the minimum and maximum ratio from those four processes, not a confidence interval. Formal runs start after the builds, baseline native captures, and diagnostic reruns finish. Timing excludes fixture construction and includes result consumption/destruction. Scoped thread-local counters wrap the System allocator; timing runs have counting disabled. Allocated-byte totals are cumulative requested sizes, including reallocation requests, rather than resident-memory measurements.

`query` is a pair: retrieve choice 2 and find 48,000 Hz. `scan` consumes the full materialized numeric list; `labels` calls the actual Cow-based options label path. Both are unchanged-code controls. Audio fixtures cover empty/fallback, four typical rates, 128 duplicate-heavy rates, and 64 unique rates. Graphics fixtures cover no modes, 64 mixed modes, 1,024 mixed modes, and a windowed control.

Resolution timing uses the optimized refresh rebuilding function in both variants, isolating the resolution change. Correctness tests also compare the complete original resolution/refresh pipeline. Refresh timing covers the entire rebuild, including selection and FPS side effects; the `fps` rows isolate automatic FPS seeding. Original function bodies and the one-call resolution benchmark adapter are checked against the base Git commit.

| Operation | Original ns | Current ns | Throughput | Paired range | Allocations | Reallocations | Allocated bytes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| resolution/fallback | 168.93 | 109.05 | 1.549x | 1.509-1.632x | 1 -> 0 | 0 -> 0 | 24 -> 0 |
| refresh/fallback | 171.13 | 83.84 | 2.041x | 1.848-2.072x | 1 -> 0 | 0 -> 0 | 16 -> 0 |
| fps/fallback | 14.30 | 8.57 | 1.669x | 1.608-1.830x | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| resolution/typical | 1370.78 | 444.44 | 3.084x | 2.938-3.227x | 1 -> 0 | 0 -> 0 | 512 -> 0 |
| refresh/typical | 406.15 | 195.02 | 2.083x | 2.013-2.189x | 2 -> 0 | 1 -> 0 | 64 -> 0 |
| fps/typical | 229.47 | 40.49 | 5.667x | 5.520-5.913x | 1 -> 0 | 1 -> 0 | 48 -> 0 |
| resolution/many_modes | 8670.18 | 4410.30 | 1.966x | 1.902-2.035x | 1 -> 0 | 0 -> 0 | 8192 -> 0 |
| refresh/many_modes | 1642.38 | 1342.99 | 1.223x | 1.186-1.225x | 2 -> 0 | 5 -> 0 | 1024 -> 0 |
| fps/many_modes | 1461.07 | 401.24 | 3.641x | 3.614-3.684x | 1 -> 0 | 5 -> 0 | 1008 -> 0 |
| resolution/windowed | 1000.42 | 203.53 | 4.915x | 4.838-5.005x | 1 -> 0 | 0 -> 0 | 512 -> 0 |
| refresh/windowed | 60.25 | 5.79 | 10.407x | 9.454-11.171x | 1 -> 0 | 0 -> 0 | 4 -> 0 |
| fps/windowed | 33.05 | 33.55 | 0.985x | 0.945-1.005x | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| sound/fallback/scan | 153.24 | 161.09 | 0.951x | 0.921-0.981x | 1 -> 1 | 1 -> 1 | 40 -> 40 |
| sound/fallback/query | 317.74 | 19.30 | 16.467x | 15.519-17.712x | 2 -> 0 | 2 -> 0 | 80 -> 0 |
| sound/fallback/labels | 548.80 | 552.62 | 0.993x | 0.952-1.039x | 5 -> 5 | 1 -> 1 | 136 -> 136 |
| sound/typical/scan | 77.25 | 77.56 | 0.996x | 0.958-1.056x | 1 -> 1 | 0 -> 0 | 40 -> 40 |
| sound/typical/query | 149.97 | 18.96 | 7.910x | 7.689-7.917x | 2 -> 0 | 0 -> 0 | 80 -> 0 |
| sound/typical/labels | 855.06 | 818.75 | 1.044x | 1.001-1.064x | 7 -> 7 | 1 -> 1 | 216 -> 216 |
| sound/duplicates/scan | 383.65 | 354.52 | 1.082x | 1.034-1.153x | 1 -> 1 | 0 -> 0 | 1032 -> 1032 |
| sound/duplicates/query | 697.47 | 17.49 | 39.878x | 37.645-42.584x | 2 -> 0 | 0 -> 0 | 2064 -> 0 |
| sound/duplicates/labels | 987.57 | 991.21 | 0.996x | 0.969-0.998x | 6 -> 6 | 0 -> 0 | 1160 -> 1160 |
| sound/unique/scan | 1861.55 | 1824.16 | 1.020x | 0.979-1.037x | 1 -> 1 | 0 -> 0 | 520 -> 520 |
| sound/unique/query | 3711.61 | 349.92 | 10.607x | 10.425-10.796x | 2 -> 0 | 0 -> 0 | 1040 -> 0 |
| sound/unique/labels | 9291.08 | 9307.37 | 0.998x | 0.979-1.003x | 67 -> 67 | 0 -> 0 | 2600 -> 2600 |

The optimized query pairs and warmed graphics rebuilds have zero allocation churn in every formal run. Ordinary regression tests also assert this property. Unchanged controls and their variability are included above; no control speedup is claimed. The unchanged fallback numeric-list scan measured 0.951x (about 5.1% slower), and windowed FPS seeding measured 0.985x (about 1.5% slower). Label controls were within about 0.7% slower to 4.4% faster in aggregate with unchanged allocation counts. The fallback scan slowdown appeared in all four paired samples despite the materialized-list functions being unchanged, so the measurements do not support a claim that every measured operation became faster. An exploratory fully streamed label path was rejected after its iterator size hint increased final-vector growth. The committed label-list functions are unchanged.

## Regression and native compatibility

| Suite | Passed | Existing failures | Ignored |
| --- | ---: | ---: | ---: |
| deadsync_shell-tests | 355 | 17 | 6 |
| deadsync_theme_simply_love-tests | 1270 | 1 | 5 |
| deadsync_simfile-tests | 203 | 0 | 0 |
| semantic | 149 | 41 | 77 |
| actor | 30 | 1 | 0 |

**2007 passed; 60 pre-existing failures matched baseline.** Seven new regression tests pass; two benchmark tests are explicitly ignored in ordinary runs. All existing test outcomes and failure diagnostics match, with only thread IDs and the separately audited unordered diagnostics normalized. The root library and integration binaries also compiled; the executed suites are listed above.

The new tests exercise 130 generated rate-list lengths, duplicate/zero/maximum rates, absent devices and invalid indices; shared and owned labels; aspect changes, presets and non-square modes; refresh ties and exact cutoff boundaries; mode transitions and FPS seeding; and warmed storage identity/allocation churn.

The 17 shell failures were rerun in isolated processes on both binaries: 15 pass individually and two reproduce the same underlying failures (hidden-score footer fade and root-player proxy segment count). The full suite has shared-lock poisoning after the first failure. The existing theme Cybermodel-height failure and actor geometry alignment failure also match. Semantic failures include existing unsupported behavior and missing fixture archives. The three unordered Sprite.Load alias lines were checked across three reruns per variant; the complete set of two missing non-local archives was checked from the identical index/references and ten reruns per variant.

Six retained whole-song ITGmania captures were run sequentially against saved baseline and optimized binaries, using the same local temporary directory. Every capture passed with zero mismatches in both variants:

| Capture | Archive prefix | Comparisons per variant | Mismatches |
| --- | --- | ---: | ---: |
| 319\|TECH SOUP\|[lv.P.Clark] Epidermis | `0f2ef3f98744` | 363,873 | 0 |
| 319\|TECH SOUP\|[lv.P.Clark] Epidermis | `af2f887d212d` | 363,873 | 0 |
| 280\|MODS\|[MASTER] Sharkmode | `b05379b7d12b` | 304,425 | 0 |
| Warp Zone | `ded0f7ff1951` | 212,220 | 0 |
| Let Me Hear That | `2a77063dd2ab` | 205,071 | 0 |
| 272\|MODS\|[lv.02] Riddle | `7ffacb89fd95` | 201,471 | 0 |

**1,650,933 native comparisons per variant, zero mismatches.** These Song Lua/actor captures exercise the runtime. Options-list behavior is checked directly against frozen originals and existing options tests; the Song Lua harness does not drive the options UI.

## Isolation and reproduction

Audited 84 unmerged local/remote-tracking perf refs (54 distinct heads). Neither production file is changed by those pending heads. A final ref audit found 0 changed/new pending refs. The original checkout was used read-only apart from authorized Git worktree/branch metadata. Its ongoing Song Lua edits were never copied into or modified by this pass. The new branch remains local and unmerged.

Source edits and the commit are confined to the new worktree. Generated Cargo artifacts use the existing shared cache at `C:/GitHub/deadsync-perf-1872-data-churn-20261009/target/build`; baseline executables were copied into this worktree and hashed before optimized builds. Source and tested executable hashes were checked before committing. No compiler warnings were emitted. Changed Rust files and the appended test module pass rustfmt checks; the existing options test file is otherwise preserved exactly. Git diff whitespace checks pass.

The commit excludes `deadsync-song.json.gz`, `rust-performance.md`, `optimize.sh`, and `optimize.ps1`.

From the new worktree:

```powershell
cargo test --release --locked --config profile.release.lto=false -p deadsync-theme-simply-love --lib options::tests::enumeration -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false -p deadsync-theme-simply-love --lib benchmark_option_enumeration -- --ignored --nocapture --test-threads=1
cargo test --release --locked --config profile.release.lto=false -p deadsync-shell -p deadsync-simfile -p deadsync-theme-simply-love --lib -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false -p deadsync --test song_lua_itgmania_semantic_parity --test itgmania_actor_conformance -- --test-threads=1
# Run the full_song_lua binary with each archive listed in target/current-fullsong-selectors.json.
```

The native suites require the existing repository ITGmania harness setup. Raw formal samples (`target/final-1.log` through `final-4.log`), benchmark summaries, unit/native logs, selector lists, diagnostic audits, and hash manifests remain in the ignored worktree target directory. Benchmark source and frozen references are committed under `tests/perf/option_*.rs`.
