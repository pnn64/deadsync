# Direct data performance pass - 2026-10-09

Branch: `perf/1872-direct-data-20261009`. Starting main: `def9a12f131b4dcbc0cc3ed4c3f9fd2f61f54ec1`. Version: **0.5.1871 -> 0.5.1872** in workspace Cargo.toml and all three affected Cargo.lock package records. No merge or push.

## Changes

1. Player-option initialization borrows display order and mutates the separate row array directly. This removes three temporary vectors per nonempty pane/player initialization and the repeated row lookups. The three callback passes, display order, missing-row handling, and repeated IDs are preserved.
2. Workshop grouping uses static slot keys and clones the group ID only for a new choice. Filename case folding is limited to holds/rolls, and label case folding for animation metrics is limited to arrows. Unicode case folding, stable choice order, collision errors, and manifest contents are preserved.
3. Workshop family filtering reads borrowed path components before normalizing an owned output path. Other-family assets require no path allocations. Accepted paths and malformed-path diagnostics retain the original slash normalization, including mixed separators.

No new production types, caches, dependencies, or public APIs. The production edits are confined to workshop compilation and player-option initialization. Song Lua source is unchanged.

## Benchmarks

Measured the actual crate functions against frozen bodies from starting main. Accepted-only workshop fixtures isolate the grouping allocation reduction because every normalized path is still retained. Other-family fixtures isolate deferred normalization because they exit before grouping. Mixed-family cases measure their combination. Fixtures are synthetic and fully constructed outside the timed region; this is not an end-to-end frame-rate or pack-install claim.

Windows x86_64, Intel Xeon E5-2696 v4 (22 cores / 44 logical processors), Ultimate Performance power plan, Rust 1.98.1 / LLVM 22.1.8. Release opt-level 3 with LTO disabled for both implementations. Four fresh-process runs, logical CPU 2 affinity, AboveNormal priority, one test thread, with this pass's builds and compatibility runs finished. Each process alternates old/new order across ten batches, discards the calibration batch, and reports the median of nine batches. The table shows medians of the four process medians. System allocator instrumentation is thread-local and disabled during timing.

Allocation calls exclude reallocations. Requested bytes include allocation sizes and each reallocation's requested new size; they are allocation churn, not peak live memory or RSS. The output is consumed and dropped in both timed variants.

| Workload | Original ns/op | Current ns/op | Throughput | Allocations | Reallocations | Requested bytes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| workshop/empty | 23.05 | 24.14 | 0.955x | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| workshop/single-file-groups | 318,468.18 | 261,467.65 | 1.218x | 1,052 -> 812 | 93 -> 93 | 80,879 -> 75,559 |
| workshop/720-files | 1,707,045.83 | 1,431,437.50 | 1.193x | 4,936 -> 2,946 | 793 -> 793 | 242,031 -> 207,523 |
| workshop/4096-files | 10,224,350.00 | 8,840,450.00 | 1.157x | 28,047 -> 16,728 | 4,497 -> 4,497 | 1,437,205 -> 1,238,205 |
| workshop/mixed-family-4096 | 6,001,800.00 | 4,861,900.00 | 1.234x | 16,271 -> 8,573 | 2,238 -> 2,238 | 896,713 -> 639,811 |
| workshop/other-family-4096 | 2,018,816.67 | 1,452,137.50 | 1.390x | 4,096 -> 0 | 0 -> 0 | 314,048 -> 0 |
| rows/empty | 13.51 | 2.75 | 4.922x | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| rows/uncommon | 277.05 | 80.53 | 3.440x | 3 -> 0 | 0 -> 0 | 240 -> 0 |
| rows/advanced | 6,435.93 | 6,145.70 | 1.047x | 5 -> 2 | 0 -> 0 | 1,672 -> 16 |
| rows/display | 14,314.22 | 14,156.33 | 1.011x | 12 -> 9 | 0 -> 0 | 672 -> 72 |

Paired throughput ranges across the four process medians:

- `workshop/empty`: 0.927x to 0.992x.
- `workshop/single-file-groups`: 1.178x to 1.225x.
- `workshop/720-files`: 1.179x to 1.199x.
- `workshop/4096-files`: 1.151x to 1.172x.
- `workshop/mixed-family-4096`: 1.199x to 1.261x.
- `workshop/other-family-4096`: 1.382x to 1.429x.
- `rows/empty`: 4.825x to 5.159x.
- `rows/uncommon`: 3.397x to 3.664x.
- `rows/advanced`: 1.026x to 1.105x.
- `rows/display`: 0.999x to 1.039x.

The empty workshop control measured 23.05 -> 24.14 ns/op (a 1.09 ns increase), with zero allocations in both versions and the same source-level early return. No improvement is claimed for this control. Display-pane CPU throughput is close to parity; its three-allocation reduction is repeatable. All nonempty workload median throughputs improve relative to starting main.

## Behavioral validation

Fresh baseline and final executions, all using one test thread:

| Suite | Passed | Failed | Ignored | Comparison |
| --- | ---: | ---: | ---: | --- |
| semantic | 142 | 39 | 77 | Matches baseline (normalized diagnostics) |
| actor | 30 | 1 | 0 | Matches baseline (normalized diagnostics) |
| deadsync_simfile-tests | 203 | 0 | 0 | Matches baseline (normalized diagnostics) |
| deadsync_score-tests | 237 | 1 | 0 | Matches baseline (normalized diagnostics) |
| deadsync_noteskin-tests | 267 | 0 | 1 | Matches baseline (normalized diagnostics) |
| deadsync_theme_simply_love-tests | 1266 | 1 | 4 | Matches baseline (normalized diagnostics) |

The seven added regression tests pass. They cover both player slots, sparse/repeated/reversed option orders, callback order and count, allocation deltas, manifest contents and ordering, Unicode names, slug collisions, mixed path separators, non-UTF8 Windows paths, and error messages. The two added benchmark tests are intentionally ignored in ordinary test runs.

The baseline has **42 existing failures**: 39 Song Lua semantic failures, one actor-conformance failure, `tests::lua_submit_allowlist_requires_known_hash`, and `screens::components::gameplay::notefield::tests::cyber_model_tap_scale_uses_model_height_not_logical_height`. Failure outcomes are unchanged; diagnostics match after the normalizations below. Diagnostic comparison normalizes thread IDs and the order of explicitly unordered missing-actor lines in `image_texture_aliases_match_native_draws`; all values and line counts are retained. One archive-index failure additionally normalizes the first missing filename to the complete two-file missing set: `c084614875dcb51da42e1e087fbfaf34d39ace2c508ee3fb271d36a098b450a5.tar.zst` and `d6b0ff8c5ada41bbb143da698c6bd2dd2cece58aafbad627d16b2a0669fcc426.tar.zst`. Three targeted reruns of each unchanged binary reported both filenames. The test, reference index, and archive index are byte-identical to starting main; their HashMap iteration chooses which missing archive fails first.

ITGmania recorded full-song captures:

| Capture | Comparisons passed | Failed |
| --- | ---: | ---: |
| 319 / TECH SOUP / [lv.P.Clark] Epidermis (`0f2ef3f98744`) | 363,873 | 0 |
| 319 / TECH SOUP / [lv.P.Clark] Epidermis (`af2f887d212d`) | 363,873 | 0 |
| 280 / MODS / [MASTER] Sharkmode (`b05379b7d12b`) | 304,425 | 0 |
| Warp Zone (`ded0f7ff1951`) | 212,220 | 0 |
| Let Me Hear That (`2a77063dd2ab`) | 205,071 | 0 |
| 272 / MODS / [lv.02] Riddle (`920eb361ce68`) | 201,471 | 0 |

All **1,650,933** full-song comparisons pass on both starting main and the final patch. These are replays of recorded ITGmania captures, not a new live-engine recording.

## Reproduction

From this worktree (PowerShell):

```powershell
cargo test --release --locked --config profile.release.lto=false -p deadsync-score -p deadsync-noteskin -p deadsync-theme-simply-love --lib -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false -p deadsync-noteskin -p deadsync-theme-simply-love --lib benchmark_direct_data -- --ignored --nocapture --test-threads=1
cargo test --release --locked --config profile.release.lto=false -p deadsync-simfile --lib -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false -p deadsync --test song_lua_itgmania_semantic_parity -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false -p deadsync --test itgmania_actor_conformance -- --test-threads=1
```

The known-failing suites return a nonzero exit code. Build each benchmark executable once, then launch the benchmark filter in four fresh processes with CPU affinity mask `4` and AboveNormal priority to reproduce the reported timing protocol. The local `target/run-bench.py final 4` runner does this. Local build/test logs and comparisons are under `target/`; `target/run-compat.py` enumerates and runs the six exact archive selectors.

Full-song selectors:

```powershell
cargo test --release --locked --config profile.release.lto=false -p deadsync --test full_song_lua -- 0f2ef3f987449e6218525fba450d1db1201c9f733d4956061d3b36529b922305.tar.zst
cargo test --release --locked --config profile.release.lto=false -p deadsync --test full_song_lua -- af2f887d212d8ab702b772cea9c953500254ce01a6b78b423276fca9cf5a8306.tar.zst
cargo test --release --locked --config profile.release.lto=false -p deadsync --test full_song_lua -- b05379b7d12bfb02149e03f166be5d496dc3e7687dcc042f0bf525cc46aa918b.tar.zst
cargo test --release --locked --config profile.release.lto=false -p deadsync --test full_song_lua -- ded0f7ff19513e30119e0d6f65b57a661f1683872fc9ffced27737db06c49993.tar.zst
cargo test --release --locked --config profile.release.lto=false -p deadsync --test full_song_lua -- 2a77063dd2ab8c43bbdbb9e101be7fcf73156302e21e9771c88797218317d327.tar.zst
cargo test --release --locked --config profile.release.lto=false -p deadsync --test full_song_lua -- 920eb361ce688e70b4364258337b30c9e4bf7d337f2c83a30e59215eb838735d.tar.zst
```

## Isolation and audit

Audited unmerged local and remote perf refs against the starting main: 66 refs, 40 distinct heads. Neither production file overlaps their changes. The only shared implementation addition is the existing allocation-measurement test helper, copied identically from `perf/1857-state-dataflows-20261008`; this reuses the existing System allocation counter and is common benchmark plumbing, not an additional production optimization. Cargo version files overlap by the required per-pass bump.

All source changes, test fixtures/scripts/logs, and the commit are confined to this new worktree. Builds reuse the ignored Cargo target cache at `C:/GitHub/deadsync-perf-1872-data-churn-20261009/target/build`. The original checkout, including concurrent Song Lua test edits and untracked captures, was not written by this pass. None of the four excluded files is added or modified by this commit.

Validated source SHA-256 values (recorded before final builds/benchmarks and checked again before commit):

| File | SHA-256 |
| --- | --- |
| `Cargo.lock` | `7d46948079d5087bd563e6545f9eb0b01e9e0a1149718fec27c5d26e5984c546` |
| `Cargo.toml` | `bbc638ab41ead687f1ae28199b9850c7792c746d57602fff99ca802f41f8f280` |
| `crates/deadsync-noteskin/src/workshop.rs` | `ba162e1a7b8fe70f3a5f75907d5fabe37b122c3f7ef401eec3cc9ca31e72ae7b` |
| `crates/deadsync-noteskin/src/workshop_perf.rs` | `f36e9ed7d92d942b942a321fc8b77ab7c1857facfa9fd3584db2c7c7923317c1` |
| `crates/deadsync-theme-simply-love/src/screens/player_options/panes/direct_data_perf.rs` | `fb5ba3bd645b6daecf4464432079336667275169799bed13f1cc4c7d069f5300` |
| `crates/deadsync-theme-simply-love/src/screens/player_options/panes/mod.rs` | `6a156cbd542866ac7758bf79cb8e40469d01f8c23473e3850d13b6337f5d88f1` |
| `tests/perf/direct_data_original_rows.rs` | `6dd322b297a5c70513704c07d548d7cd159e960982d6df9f14329b6dbcf56068` |
| `tests/perf/direct_data_original_workshop.rs` | `12c031a629ec122b6e1a6202ff399f662a8df947f35877b9bc9b046c36de4a50` |
| `tests/perf/direct_data_support.rs` | `6023097643a2bc7fd82468219d5c669aa300c3cbffb57d82e330340fed83686f` |
| `tests/support/perf.rs` | `a3f8569751df174568c3e6aa34216edebbf8aa13d865bcf2c0201eff19ef385a` |

At the final original-checkout audit, main had independently advanced to `3b0f61b73348db057f8d2a189ac86d46da9b6513`. This pass uses the starting commit recorded above, which is the baseline used for all comparisons.
