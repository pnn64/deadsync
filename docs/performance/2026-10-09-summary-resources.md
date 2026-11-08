# UI resource performance pass - 2026-10-09

Branch: `perf/1872-summary-resources-20261009`. Starting main: `3b0f61b73348db057f8d2a189ac86d46da9b6513`. Version: **0.5.1871 -> 0.5.1872** in Cargo.toml and all three affected Cargo.lock package records. No merge or push.

## Changes

1. Preview texture collection borrows keys during deduplication and clones an Arc only when adding a retained texture. Repeated taps, lifts, columns, and model materials no longer increment and decrement texture reference counts just to discard the duplicate. First-seen order, original Arc identity, additive materials, and the parent slot's model flag are preserved.
2. Component search constructs results with their final empty pane label, shared across results. It no longer looks up a translated pane name that is immediately discarded or allocates a separate empty Arc for every matching choice. Normal setting search still uses its translated pane names; ranking, stable ties, thumbnails, Unicode matching, and displayed labels are unchanged.
3. Help-panel fitting truncates and reuses its input vector. It no longer allocates another vector and moves the retained prefix into it. The existing line truncation routine and all height calculations are unchanged. The retained vector keeps its incoming capacity, avoiding buffer churn at the cost of retaining any pre-existing spare capacity until the description layout is dropped.

Production changes are confined to these three UI functions and the private search-result constructor. No new dependency or public API. One immutable empty-string Arc is shared by component search. Song Lua source is unchanged.

## Paired benchmarks

Actual crate functions are compared with frozen starting-main function bodies in the same executable. The frozen bodies are checked against the Git blob after rustfmt, allowing only function renames, constructor return naming, and calls to the frozen constructor. Fixtures use shipped default/cel/cyber skins plus empty/single-slot controls, synthetic component-choice lists, and synthetic help blocks.

Windows x86_64, Intel Xeon E5-2696 v4 (22 cores / 44 logical processors), Rust 1.98.1 / LLVM 22.1.8. Release opt-level 3, LTO disabled for both variants. Four fresh processes, affinity to logical CPU 2, AboveNormal priority, one test thread, after this pass's builds and compatibility runs completed. Each process alternates old/new order over ten batches, discards calibration, and reports the median of nine batches. Values below are medians of the four process medians. These are function throughput measurements, not whole-game FPS claims.

Fixture construction is outside timing except for help fitting: both owned-input variants include the same input-vector/Arc clone and output drop. All other outputs are also consumed and dropped. Thread-local System allocation counting is inactive during timing; the test allocator still performs its TLS check, so absolute timings include that overhead. Counts exclude reallocations; requested bytes include each allocation and the new requested size of each reallocation. They measure allocation churn, not peak live memory or RSS. Search allocation counts are after the shared empty label's one-time initialization.

| Workload | Original ns/op | Current ns/op | Throughput | Allocations | Reallocations | Requested bytes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| textures/empty | 270.71 | 262.61 | 1.031x | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| textures/one | 393.57 | 367.30 | 1.072x | 1 -> 1 | 0 -> 0 | 96 -> 96 |
| textures/default | 8,548.68 | 5,186.26 | 1.648x | 1 -> 1 | 1 -> 1 | 288 -> 288 |
| textures/cel | 8,357.17 | 5,624.93 | 1.486x | 1 -> 1 | 2 -> 2 | 672 -> 672 |
| textures/cyber | 8,653.01 | 5,721.01 | 1.512x | 1 -> 1 | 2 -> 2 | 672 -> 672 |
| help/empty | 10.09 | 9.52 | 1.060x | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| help/fits | 107.81 | 105.37 | 1.023x | 1 -> 1 | 0 -> 0 | 64 -> 64 |
| help/one-overflow | 751.66 | 694.71 | 1.082x | 5 -> 4 | 2 -> 2 | 321 -> 289 |
| help/30-overflow | 1,046.94 | 902.28 | 1.160x | 4 -> 3 | 0 -> 0 | 1,952 -> 992 |
| help/256-overflow | 5,774.75 | 5,636.67 | 1.024x | 5 -> 4 | 1 -> 1 | 16,481 -> 8,289 |
| search/empty | 45.41 | 46.47 | 0.977x | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| search/one | 416.56 | 259.78 | 1.603x | 3 -> 2 | 0 -> 0 | 720 -> 704 |
| search/eight | 2,689.28 | 1,444.96 | 1.861x | 17 -> 9 | 1 -> 1 | 2,416 -> 2,288 |
| search/256-all | 80,867.20 | 40,445.10 | 1.999x | 513 -> 257 | 6 -> 6 | 98,144 -> 94,048 |
| search/256-query | 141,769.58 | 98,115.67 | 1.445x | 642 -> 386 | 6 -> 6 | 142,752 -> 138,656 |
| search/256-miss | 121,033.52 | 121,478.15 | 0.996x | 128 -> 128 | 0 -> 0 | 1,600 -> 1,600 |
| settings/all | 24,396.03 | 24,147.56 | 1.010x | 2 -> 2 | 5 -> 5 | 42,368 -> 42,368 |
| settings/query | 25,142.40 | 24,901.47 | 1.010x | 2 -> 2 | 0 -> 0 | 704 -> 704 |

All shipped-skin, nonempty matching-search, and overflowing-help workloads improved in all four paired runs. The unchanged empty-search control's median increased by 1.06 ns and the no-match control by 0.37%; their paired ratios span 0.930-1.025x and 0.977-1.015x respectively, so these runs do not establish a consistent slowdown. Their allocations were unchanged. Ordinary settings-search controls improved about 1% at the median with unchanged allocations. Per-process measurements and paired ranges are retained in `target/benchmark-summary.json`; no full-game performance claim is made.

## Behavioral validation

Seven new regression tests compare full search results (including ordering/ties and translated ordinary pane labels), texture order/flags/ownership including additive materials, and help output across 322 height/count/type combinations. Allocation checks verify one empty-string allocation removed per search result and one result-vector allocation removed per overflowing help panel. The empty and one-slot texture cases are checked explicitly. Three ignored benchmark tests were run explicitly in four fresh processes.

Fresh baseline and current release suites used the same commands and fixtures:

| Suite | Passed | Failed (pre-existing) | Ignored | Added tests/benchmarks |
| --- | ---: | ---: | ---: | ---: |
| semantic | 142 | 39 | 77 | 0 |
| actor | 30 | 1 | 0 | 0 |
| deadsync_simfile-tests | 203 | 0 | 0 | 0 |
| deadsync_theme_simply_love-tests | 1270 | 1 | 6 | 10 |

Baseline outcomes and failure diagnostics were compared, not assumed clean. All newly added behavioral tests pass. Thread IDs are normalized in panic messages; only the unordered missing-actor diagnostic lines in `image_texture_aliases_match_native_draws` are sorted. No other diagnostic normalization was needed.

The Song Lua compatibility harness replayed six ITGmania full-song captures, each against both starting main and this pass. All 1,650,933 comparisons passed in both versions, with identical selectors and counts. These are checked-in recorded ITGmania references, not a newly recorded live-engine session. They provide a compatibility guard; the affected UI behavior is covered by the direct regression tests.

- `0f2ef3f987449e6218525fba450d1db1201c9f733d4956061d3b36529b922305.tar.zst` (319-TECH SOUP-[lv.P.Clark] [Tech Spectrum Super - _TRUE GAMERS CLICK HERE - EXTRA CHARTS]/Venetian Snares - Epidermis.ssc): 363,873 passing comparisons.
- `af2f887d212d8ab702b772cea9c953500254ce01a6b78b423276fca9cf5a8306.tar.zst` (319-TECH SOUP-[lv.P.Clark] [Tech Spectrum Super]/Venetian Snares - Epidermis.ssc): 363,873 passing comparisons.
- `b05379b7d12bfb02149e03f166be5d496dc3e7687dcc042f0bf525cc46aa918b.tar.zst` (280-MODS-[MASTER] Sharkmode/Sharkmode.ssc): 304,425 passing comparisons.
- `ded0f7ff19513e30119e0d6f65b57a661f1683872fc9ffced27737db06c49993.tar.zst` ((R10) Warp Zone/warp zone.ssc): 212,220 passing comparisons.
- `2a77063dd2ab8c43bbdbb9e101be7fcf73156302e21e9771c88797218317d327.tar.zst` ((R5) Let Me Hear That/let me hear that.sm): 205,071 passing comparisons.
- `7ffacb89fd95ab788758e273ba878c30380c4622406f1bb22d8c51292ca78d83.tar.zst` (272-MODS-[lv.02] Riddle/Riddle.ssc): 201,471 passing comparisons.

Pre-existing failures retained by the current build:

- `semantic`: `and_drugs_whole_song_matches_native`
- `semantic`: `bad_apple_whole_song_matches_native`
- `semantic`: `bank_account_complete_semantics_match_itgmania`
- `semantic`: `base_rotation_keeps_getters_and_draw_pose`
- `semantic`: `brogamer_dizzy_and_confusion_do_not_leak_between_authored_windows`
- `semantic`: `corpora::allowed::fixtures_cover_corpus_and_tests`
- `semantic`: `corpora::lua_songs::fixtures_cover_corpus_and_tests`
- `semantic`: `delightful_day_movie_and_hidden_layers_match_itgmania`
- `semantic`: `final_render_samples_unfinished_native_fade`
- `semantic`: `finite_queue_controls_match_native`
- `semantic`: `goodbye_whole_song_matches_native`
- `semantic`: `hidden_cache_keeps_membership_and_visible_order`
- `semantic`: `i_ai_whole_native`
- `semantic`: `igaku_whole_song_matches_native`
- `semantic`: `image_texture_aliases_match_native_draws`
- `semantic`: `karachi_whole_native`
- `semantic`: `mawaru5_local_draw_colors_match_native`
- `semantic`: `mawaru6_whole_song_matches_native`
- `semantic`: `mawaru7_idle_holds_match_native`
- `semantic`: `mawaru7_whole_song_matches_native`
- `semantic`: `mawaru8_local_messages_match_native`
- `semantic`: `multitap::edgar_countdown_onsets_and_hit_commands`
- `semantic`: `multitap::multitap_boundaries_and_noteskin_commands_match`
- `semantic`: `nested_global_probes_match_native`
- `semantic`: `oshama_whole_native`
- `semantic`: `queued_lua_state_matches_native_dispatch`
- `semantic`: `recurring_ease_tables_match_native_shared_state`
- `semantic`: `recurring_road_loop_matches_native`
- `semantic`: `recurring_stop_matches_native`
- `semantic`: `runtime_modifiers::hidden_actor_tweens_drive_modifiers_without_probe_state`
- `semantic`: `runtime_modifiers::lane_stealth_survives_lua_writes_and_fresh_options`
- `semantic`: `runtime_modifiers::motion_suboptions_survive_lua_writes_and_reset`
- `semantic`: `save_tears_whole_native`
- `semantic`: `sharkmode_whole_song_matches_native`
- `semantic`: `step_your_game_up_critical_render_states_match_itgmania`
- `semantic`: `stopped_position_matches_native`
- `semantic`: `warp_zone_whole_song_matches_native`
- `semantic`: `whole_song_archives::consolidated_song_lua_references_resolve_and_are_compressed`
- `semantic`: `whole_song_archives::whole_song_archive_index_and_streamed_members_are_valid`
- `actor`: `geometry::lua_align_matches_native`
- `deadsync_theme_simply_love-tests`: `screens::components::gameplay::notefield::tests::cyber_model_tap_scale_uses_model_height_not_logical_height`

Before the authoritative baseline runs, 240 noteskin assets were restored to their exact committed Git blob bytes because the Windows checkout initially introduced CRLF conversion. The unnormalized setup runs failed reference hashes and were discarded. Baseline and current measurements use the same canonical assets; no asset content changes are committed.

Validation and the final measurements finished on 2026-10-10; the branch and report use the pass's start date.

## Reproduction

From this branch's worktree with noteskin assets checked out without line-ending conversion:

```powershell
cargo test --release --locked --config profile.release.lto=false -p deadsync -p deadsync-theme-simply-love --lib resource_
cargo test --release --locked --config profile.release.lto=false -p deadsync -p deadsync-theme-simply-love --lib benchmark_resources -- --ignored --nocapture --test-threads=1
cargo test --release --locked --config profile.release.lto=false -p deadsync-theme-simply-love --lib -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false -p deadsync-simfile --lib -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false -p deadsync --test song_lua_itgmania_semantic_parity -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false -p deadsync --test itgmania_actor_conformance -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false -p deadsync --test full_song_lua -- <archive-selector-listed-above>
```

The theme unit-test build selected both `deadsync` and `deadsync-theme-simply-love`, then ran only the emitted theme executable. Root compatibility builds selected `deadsync` and `deadsync-simfile`. Both baseline/current use identical package selections.

Timing methodology can be reproduced by building the benchmark executable once, then running it four times with the affinity/priority above. Existing suite failures listed above must be accounted for when comparing outcomes.

## Isolation and audit

Created a new worktree from committed main and kept all source edits, tests, logs, scripts, and the commit there. The original checkout was not edited, reset, switched, or stashed. During the pass, main independently advanced to `11615108a83bbd79e68b299fc7b5211c6a81cedd` and acquired ongoing Song Lua source/fixture edits; those were left untouched. This branch retains its starting-main parent. An existing ignored Cargo build cache at `C:/GitHub/deadsync-perf-1872-data-churn-20261009/target/build` was reused; this pass's TEMP/TMP and evidence are under its own target directory.

Audited 67 unmerged local/remote perf refs across 41 distinct commits, and repeated the audit before committing. None changes the three selected production files. The allocation helper extension is identical to existing perf-branch test infrastructure; the crate-root additions only register test support. The commit excludes `deadsync-song.json.gz`, `rust-performance.md`, `optimize.sh`, and `optimize.ps1`.

Raw logs, baseline/current executable manifests, four benchmark logs, source hashes, comparison results, and audit records remain under this worktree's ignored `target/` directory. The compiled source hashes were verified again before commit.
