# Profile ownership performance pass, 0.5.1860

Branch: `perf/1860-core-streamlining-20261008`.
Starting committed main: `d70f069f27c8fc853a8d8887a026942c7f6278a4`.
Version: **0.5.1859 -> 0.5.1860**, including all three workspace-version entries in Cargo.lock.
Worktree: `C:/GitHub/deadsync-perf-1860-core-streamlining-20261008`.
Date: 2026-10-08. No merge into main; original checkout and its uncommitted work were not edited.

## Changes and affected callers

1. Profile settings and credential saves now serialize the required fields while borrowing the profile, then release the mutex before directory callbacks and atomic file writes. This removes whole-profile snapshots, including favorites and known-pack collections, and deletes the snapshot helper. Player option updates, last-played updates, and credential saves use these paths. The currently selected style is still stored before serialization. Rendering is now inside the mutex; file I/O and external callbacks remain outside it, and the protected work no longer scales with favorites counts.
2. Profile and machine-default INI loading uses the existing borrowed INI parser. Keys, section names, and intermediate values reference the file text instead of being copied into an owned map. Typed consumers retain their existing owned-value contract. The redundant INI file-loading wrapper is removed; read failures and invalid UTF-8 retain the same load-report behavior.
3. Applying player options consumes the snapshot that both callers already own. Strings and shared animation references move into the live profile. This removes the clone-to-snapshot-to-clone round trip during style changes, profile loading, and Player Options updates. Equality checks and saved style snapshots remain unchanged.

The unmerged rendering/chart, draw-data, audio/resampling, resource, score-state, artwork, Linux input/clock, and font perf branches were reviewed. These production optimizations do not duplicate their pending work. The shared test allocator and paired benchmark helper match the earlier pending passes. No Song Lua source changed, and none of `deadsync-song.json.gz`, `rust-performance.md`, `optimize.sh`, or `optimize.ps1` is part of this commit.

## Measurements

Windows x86-64, Intel Xeon E5-2696 v4 (22 cores / 44 threads), rustc 1.98.1. Both variants use `--release --locked --config profile.release.lto=false`. These measurements do not cover the default fat-LTO build. No builds from this pass ran during timing.

Each benchmark alternates original/current order for ten pairs, discards the first pair, and reports the median of the remaining nine samples. Inputs consumed by the setter are prepared outside timing. The starting routines are frozen in `perf_original.rs` and audited against the base commit, allowing renaming and rustfmt formatting only.

Save preparation measures style capture (for Profile.ini), snapshot creation, serialization, and snapshot destruction; it excludes mutex acquisition, path lookup, and disk I/O. The runtime differential test separately runs the actual save functions, verifies file bytes and callbacks, and checks reduced allocations there too. The large fixture has 10,000 favorites and 1,000 known packs; the 1,000-favorite fixture has 100 known packs. These are operation-level results, not whole-game frame-rate measurements.

| Operation / fixture | Original ns/op | Current ns/op | Throughput ratio |
| --- | ---: | ---: | ---: |
| apply style/defaults | 767.85 | 410.59 | 1.87x |
| set changed options/defaults | 1,087.94 | 753.47 | 1.44x |
| apply style/custom | 2,427.10 | 1,338.08 | 1.81x |
| set changed options/custom | 3,510.53 | 2,467.63 | 1.42x |
| INI parse/empty | 28.90 | 25.70 | 1.12x |
| typed defaults/empty | 4,379.40 | 4,539.20 | 0.96x |
| INI parse/template | 45,251.90 | 16,796.80 | 2.69x |
| typed defaults/template | 85,985.10 | 46,497.70 | 1.85x |
| INI parse/four styles | 196,845.30 | 74,265.60 | 2.65x |
| typed defaults/four styles | 205,214.30 | 80,735.90 | 2.54x |
| profile save preparation/0 favorites | 41,734.00 | 30,985.00 | 1.35x |
| groovestats save preparation/0 favorites | 8,928.00 | 627.00 | 14.24x |
| arrowcloud save preparation/0 favorites | 8,353.00 | 310.00 | 26.95x |
| profile save preparation/1000 favorites | 211,348.00 | 32,169.00 | 6.57x |
| groovestats save preparation/1000 favorites | 115,462.00 | 456.00 | 253.21x |
| arrowcloud save preparation/1000 favorites | 118,392.00 | 308.00 | 384.39x |
| profile save preparation/10000 favorites | 1,308,534.00 | 30,319.00 | 43.16x |
| groovestats save preparation/10000 favorites | 1,194,982.00 | 436.00 | 2740.78x |
| arrowcloud save preparation/10000 favorites | 1,079,009.00 | 255.00 | 4231.41x |

The four-style INI parser fixture is a full rendered Profile.ini. The corresponding typed-defaults control intentionally has no machine-default sections, so it exercises parsing plus fallback defaults, not a full runtime profile load.

The initial empty typed-defaults control varied by roughly 4%. Six further complete paired INI runs were collected to check repeatability. The following table reports the median across those six run medians, with all six pairs of run medians retained in `target/ini-repeat-summary.json`. The empty typed-defaults control is effectively unchanged (0.15% difference):

| Operation / fixture | Original ns/op | Current ns/op | Throughput ratio |
| --- | ---: | ---: | ---: |
| INI parse/empty | 30.80 | 26.40 | 1.17x |
| typed defaults/empty | 4,342.15 | 4,348.55 | 1.00x |
| INI parse/template | 45,764.25 | 15,304.05 | 2.99x |
| typed defaults/template | 83,667.85 | 45,386.90 | 1.84x |
| INI parse/four styles | 189,488.60 | 71,200.50 | 2.66x |
| typed defaults/four styles | 188,564.40 | 74,156.00 | 2.54x |

Allocation counts exclude reallocations; allocated bytes include all allocation and reallocation requests, so they measure churn rather than peak resident memory:

| Operation | Allocations, original -> current | Requested bytes, original -> current |
| --- | ---: | ---: |
| Profile.ini preparation, 10k favorites | 11,105 -> 17 | 915,574 -> 43,440 |
| GrooveStats preparation, 10k favorites | 11,089 -> 1 | 872,344 -> 210 |
| ArrowCloud preparation, 10k favorites | 11,089 -> 1 | 872,225 -> 91 |
| Typed machine defaults, populated template | 502 -> 201 | 34,263 -> 27,099 |
| Applying custom saved style | 30 -> 16 | 858 -> 445 |

## Regression and ITGmania compatibility results

- Baseline profile suite: 228 passed. Current: **232 passed**, including four new differential/allocation tests. All 228 existing outcomes are preserved. Three ignored paired benchmarks were run separately and passed.
- Option tests cover all six play styles, populated and default options, changed and unchanged setters, unrelated profile fields, and NaN payload preservation.
- INI tests compare every parsed key with the original owned parser, including duplicate sections/keys, empty and missing sections, Unicode, invalid values, and typed fallback defaults.
- Actual save tests compare exact bytes and state, reentrant duplicate-profile callbacks, errors when the destination is a file, guest no-ops, load reports, invalid UTF-8, and missing sidecars. A callback changing the live profile cannot change the already captured sidecar contents.
- ITGmania semantic harness: **112 passed, 61 existing failures, 74 ignored**. All 247 outcomes and all 61 failure diagnostics match the baseline.
- ITGmania actor conformance: **30 passed, 1 existing failure**. All 31 outcomes and the failure diagnostic match the baseline. Only process/thread IDs were normalized in the diagnostic comparison.
- Full-song harness: both Epidermis archives and Sharkmode were attempted on both builds. All three stop before comparisons at the same existing `noteskin dependency changed: common/common/Fallback Receptor.lua` guard, with identical expected/actual hashes. They are **blocked compatibility checks, not passes**; zero full-song comparisons ran.
- Formatting and `git diff --check` pass. Production Rust has a net reduction of five lines, including the test-module declaration.

## Reproduce

Run from this worktree. The serial setting matches the existing profile tests' shared runtime globals.

```powershell
cargo test --release --locked --config profile.release.lto=false -p deadsync-profile --lib -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false -p deadsync-profile --lib perf_tests::benchmark_ -- --ignored --nocapture --test-threads=1
cargo test --release --locked --config profile.release.lto=false -p deadsync-profile --lib perf_tests::ownership_changes_reduce_allocation_churn -- --exact --nocapture --test-threads=1
cargo test --release --locked --config profile.release.lto=false --test song_lua_itgmania_semantic_parity -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false --test itgmania_actor_conformance -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false --test full_song_lua -- b40903481944c3395db4c848c671e27a383d2cbea51c74b029b7dea9dbccf16a.tar.zst
cargo test --release --locked --config profile.release.lto=false --test full_song_lua -- a867a12d25c984a49c2dd29fa06d47f88afe01841ee9d410dbf1332800797336.tar.zst
cargo test --release --locked --config profile.release.lto=false --test full_song_lua -- db60b3a89af8957a5e4fb1be2ec3157d09c436a564938536ec91b8ab064afa45.tar.zst
```

Raw evidence is retained in this worktree's ignored `target/`: baseline/current native logs, baseline/final profile test logs, `final-deadsync_profile-bench.log`, `final-allocations.log`, INI repeat logs, and `compatibility-comparison.json`.
