# Input configuration performance pass, 0.5.1861

Branch: `perf/1861-input-dataflows-20261008`.
Starting committed main: `83fbce5457d789be942be4059eef4fdbb3191c34`.
Version: **0.5.1860 -> 0.5.1861**, including all three workspace-version entries in Cargo.lock. Main remained at 0.5.1860 when this independent pass started; the previous noteskin pass is a separate, unmerged branch.
Worktree: `C:/GitHub/deadsync-perf-1861-input-dataflows-20261008`.
Date: 2026-10-08. No merge into main. The original checkout and its Song Lua edits were not modified.

## Changes and callers

1. **Index editable bindings by action.** The 32 actions already have stable indices. An array of boxed binding slices replaces the action-keyed HashMap, removing hashing, bucket growth, and Vec capacity fields. Mappings screen display, protected-default checks, serialization, and configuration edits use these lookups. Missing and empty bindings retain the same public behavior. The compiled event path and reverse-map update routines are unchanged.
2. **Read defaults directly.** A shared static binding list supplies both default-keymap creation and primary-default queries. INI loading fills missing actions directly from these lists, deleting the temporary default Keymap, its reverse maps, repeated indexed lookups, and copied intermediate vectors. Explicit entries, duplicate entries, default restoration, and missing sections retain their existing behavior.
3. **Move edited binding lists.** Keyboard rebinding, gamepad rebinding, and clearing copy each source list once and transfer its buffer into the result. A new internal owned-binding method maintains the same reverse mappings. Appending a binding reserves exactly the one required slot; boxed slices release spare capacity after removals. This removes the vector-to-slice-to-vector copy round trip used by configuration helpers and the Mappings screen.

These two production source files have a net reduction of **49 lines**, including the test-module declaration. The thirteen existing unmerged perf branches were checked: rendering/chart, presentation, audio/resampling, resources, score state, artwork, profile ownership, noteskin loading, Linux input/clock, and font work. This pass does not duplicate their production changes. Shared test helpers match earlier pending passes.

No Song Lua implementation changed. None of `deadsync-song.json.gz`, `rust-performance.md`, `optimize.sh`, or `optimize.ps1` is included.

## Measurements

Windows x86-64, Intel Xeon E5-2696 v4 (22 cores / 44 threads), rustc 1.98.1. Both variants use `--release --locked --config profile.release.lto=false`; default fat LTO was not measured. No builds or compatibility runs from this pass overlapped the reported timings.

Ten original/current pairs alternate execution order. The first pair is discarded; results are the medians of nine samples per variant. Allocation counters are enabled separately from timing. Frozen routines and data structures are audited against the starting commit, allowing formatting, names, and test visibility changes only.

The storage benchmark compares original HashMap lookups with indexed lookups. For the other two optimizations, the frozen algorithms are also compiled against the new storage, isolating default loading and ownership improvements. Rows marked `all changes` compare the entire original path with the final implementation. Construction/edit timings include result destruction; source maps and INI inputs are prepared outside timing. These are operation-level results, not whole-game frame-rate measurements.

| Operation | Original ns/op | Current ns/op | Throughput ratio |
| --- | ---: | ---: | ---: |
| test keymap::perf_tests::benchmark_dense_action_storage ... lookup 32 actions | 643.28 | 111.11 | 5.79x |
| construct default map/storage only | 5,504.90 | 4,314.20 | 1.28x |
| construct default map/all changes | 5,746.50 | 4,587.85 | 1.25x |
| keyboard edit/ownership only | 12,445.00 | 9,991.70 | 1.25x |
| gamepad edit/ownership only | 12,072.00 | 10,027.30 | 1.20x |
| clear binding/ownership only | 12,021.00 | 9,306.40 | 1.29x |
| keyboard edit/all changes | 18,564.00 | 10,682.60 | 1.74x |
| sparse INI/defaults only | 11,055.60 | 5,466.90 | 2.02x |
| complete INI/defaults only | 13,284.00 | 9,915.00 | 1.34x |
| complete INI/all changes | 18,185.40 | 10,293.70 | 1.77x |

The default fixture uses the shipped 32-action keymap configuration. The sparse INI fixture overrides P1 Left with a key and device-specific raw button. Edit fixtures contain defaults plus keyboard, wildcard/device directions, and raw-button bindings with device/UUID filters across all 32 actions.

| Operation | Allocations, original -> current | Reallocations, original -> current | Requested bytes, original -> current |
| --- | ---: | ---: | ---: |
| Default map construction, storage only | 50 -> 46 | 0 -> 0 | 7,900 -> 5,856 |
| Sparse INI loading, defaults only | 117 -> 52 | 0 -> 0 | 15,660 -> 6,764 |
| Complete INI loading, defaults only | 112 -> 66 | 0 -> 0 | 14,912 -> 9,056 |
| Keyboard edit, ownership only | 99 -> 67 | 4 -> 9 | 15,944 -> 11,344 |
| Gamepad edit, ownership only | 99 -> 67 | 4 -> 10 | 15,904 -> 11,544 |
| Clear binding, ownership only | 99 -> 67 | 4 -> 5 | 16,104 -> 11,104 |

Requested bytes include allocation/reallocation requests and measure churn. Live default-keymap storage, calculated as outstanding requested heap bytes plus the inline Keymap size, falls from **7,232 to 6,624 bytes** (8.4%). This excludes allocator metadata and is not an RSS measurement. The fixed table increases the inline Keymap from **304 to 768 bytes**; completely empty maps pay that 464-byte increase, while populated default maps use less total storage. The compiled input state layout is unchanged.

Owned binding insertion reuses an exactly sized vector without allocations or frees when reverse-list capacity is available; a test checks both allocator counters and storage identity. Edit reallocations compact shortened lists instead of keeping unused capacity.

## Regression and ITGmania compatibility

- Baseline input unit suite: **71 passed**. Final: **76 passed**, with every original outcome preserved and five new differential/ownership/event tests. All three paired benchmarks pass separately.
- Binding tests compare full forward and reverse state through 512 generated rebindings, duplicate bindings, empty lists, all actions, and clones. INI tests cover absent, empty, sparse, complete, duplicate/case-varied, invalid, and 100 generated configurations.
- Edit tests compare the original and current behavior across all actions, indices 0/1/2/usize::MAX, stolen/protected defaults, shared inputs, device and UUID filters, and unchanged/changed clears.
- Compiled keyboard/pad event tests compare actions, presses/releases, source and timestamps, repeats, default system controls, and debounce behavior after editing. Due events at the same time are compared independently of internal pad-slot numbering.
- The integration suites preserve **22 passes and 1 existing ignored manual timing test**, covering mapping, debounce/timestamps, reconfiguration, and allocation-free configured input processing.
- ITGmania semantic harness: **112 passed, 61 existing failures, 74 ignored**. All 247 outcomes and all 61 failure diagnostics match baseline.
- ITGmania actor conformance: **30 passed, 1 existing failure**. All 31 outcomes and the failure diagnostic match baseline. Only thread IDs were normalized in failure comparisons.
- Both Epidermis archives and Sharkmode were attempted on both builds. All three stop at the same existing `noteskin dependency changed: common/common/Fallback Receptor.lua` guard with identical expected/actual hashes. These checks are **blocked, not passed**; zero full-song comparisons ran.
- Changed Rust files pass rustfmt checks; `git diff --check` passes. Cargo.toml/Cargo.lock contain the exact +1 patch bump, and excluded files are absent from the commit.

## Reproduce

Run from this worktree:

```powershell
cargo test --release --locked --config profile.release.lto=false -p deadsync-input --lib -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false -p deadsync-input --lib perf_tests::benchmark_ -- --ignored --nocapture --test-threads=1
cargo test --release --locked --config profile.release.lto=false -p deadsync-input --test mapping --test input_pipeline_timing --test input_pipeline_allocations -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false --test song_lua_itgmania_semantic_parity -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false --test itgmania_actor_conformance -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false --test full_song_lua -- b40903481944c3395db4c848c671e27a383d2cbea51c74b029b7dea9dbccf16a.tar.zst
cargo test --release --locked --config profile.release.lto=false --test full_song_lua -- a867a12d25c984a49c2dd29fa06d47f88afe01841ee9d410dbf1332800797336.tar.zst
cargo test --release --locked --config profile.release.lto=false --test full_song_lua -- db60b3a89af8957a5e4fb1be2ec3157d09c436a564938536ec91b8ab064afa45.tar.zst
```

Raw evidence remains in this worktree's ignored `target/`: baseline/current native and integration logs, baseline/final unit logs, `final-deadsync_input-bench.log`, and `compatibility-comparison.json`.
