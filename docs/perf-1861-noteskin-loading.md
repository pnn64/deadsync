# Noteskin loading performance pass, 0.5.1861

Branch: `perf/1861-runtime-cleanups-20261008`.
Starting committed main: `1ce6f146b28b0e7862f6cbf6ab7f6c37ca9e8267`.
Version: **0.5.1860 -> 0.5.1861**, including all three workspace-version entries in Cargo.lock.
Worktree: `C:/GitHub/deadsync-perf-1861-runtime-cleanups-20261008`.
Date: 2026-10-08. No merge into main. The original checkout was only read.

## Changes and callers

1. **INI parsing retains the current section.** Instead of keeping a cloned section name and hashing it for every property, the parser writes through the current section's mutable reference. This removes one section-name allocation per header and a hash lookup per property. Noteskin metrics, animated model texture INIs, workshop loading, and noteskin compilation use this parser. Duplicate sections/keys, unnamed sections, whitespace, comments, and ASCII-only case folding retain their existing behavior.
2. **Fallback loading moves parsed metrics.** The loader reads the next fallback name before consuming the temporary INI. Empty destinations adopt the entire map; new sections move their maps; missing properties move their keys and values. Requested-skin values still win over default/common fallbacks. Workshop base metrics use the same consuming merge, preserving selection overrides. Shared cached common metrics keep the existing borrowed merge. This removes the parse-to-clone-to-drop round trip.
3. **Runtime cache lookups borrow names.** Gameplay loading, profile previews, and Player Options use the cache. A borrowed key handles style, trimmed names, defaults, and ASCII case folding; only insertion allocates the stored normalized name. The map uses the existing hashbrown dependency and its default hasher. Folded blocks avoid heap normalization and per-byte hasher calls for long workshop keys. Weak ownership, mutex poison recovery, errors, reentrant loading, and the concurrent winner check are preserved. Load callbacks remain outside the mutex.

The twelve existing unmerged perf branches were reviewed: rendering/chart, presentation, audio/resampling, resources, score state, artwork, profile ownership, Linux input/clock, and font work. These production changes do not duplicate them. Shared allocation/benchmark helpers match the earlier pending passes. No Song Lua implementation changed. None of `deadsync-song.json.gz`, `rust-performance.md`, `optimize.sh`, or `optimize.ps1` is included.

## Measurements

Windows x86-64, Intel Xeon E5-2696 v4 (22 cores / 44 threads), rustc 1.98.1. Both variants use `--release --locked --config profile.release.lto=false`. The default fat-LTO configuration was not measured. No builds or compatibility runs from this pass overlapped these timings.

Each benchmark alternates original/current order for ten pairs, discards the first pair, and reports the median of nine samples per variant. Frozen routines, data structures, and the original runtime cache hasher are audited against the starting commit, allowing test visibility, names, and formatting changes only. Allocation counting is separate from timing and disabled during timing.

Parsing measures the full corpus of **25 shipped metrics INIs**, including result destruction. The merge benchmark uses a requested/default/common chain, with the actual shipped dance/default and common/common metrics. Consumed input maps are prepared outside timing; both variants include input/output destruction. A second benchmark includes parsing and merging together. Cache timings include locking and Weak upgrade; the long workshop key is 85 bytes. These are operation-level results, not whole-game frame-rate measurements.

| Operation | Original ns/op | Current ns/op | Throughput ratio |
| --- | ---: | ---: | ---: |
| parse shipped INI corpus | 1,065,058.00 | 865,656.00 | 1.23x |
| merge requested/default/common | 82,150.80 | 44,042.80 | 1.87x |
| parse and merge requested/default/common | 189,560.20 | 127,795.00 | 1.48x |
| resident lowercase | 156.33 | 84.75 | 1.84x |
| resident mixed case | 147.25 | 80.13 | 1.84x |
| resident default | 158.59 | 91.13 | 1.74x |
| resident long pack key | 188.01 | 127.28 | 1.48x |
| missing lookup | 146.74 | 65.89 | 2.23x |
| resident get_or_load | 156.24 | 86.80 | 1.80x |
| cold load and release | 357.42 | 359.12 | 1.00x |

Cold load/release is effectively unchanged (0.5% slower in this sample); the cache gains are in lookups that previously normalized and allocated a temporary key.

Allocation counts exclude reallocations (none occurred). Requested bytes measure allocation churn, not peak resident memory:

| Operation | Allocations, original -> current | Requested bytes, original -> current |
| --- | ---: | ---: |
| Parse 25 shipped metrics INIs | 3,831 -> 3,712 | 309,124 -> 307,875 |
| Merge requested/default/common | 350 -> 10 | 27,047 -> 15,440 |
| Resident cache lookup, cel | 1 -> 0 | 3 -> 0 |
| Resident cache lookup, long workshop key | 1 -> 0 | 85 -> 0 |
| Missing cache lookup | 1 -> 0 | 13 -> 0 |

The first owned merge into an empty destination performs **zero allocations or frees**, adopting the original string storage. Cache hit and miss lookups remain allocation-free for mixed case, whitespace/default aliases, and names through 4 KiB, checked separately from timing.

## Regression and ITGmania compatibility

- Baseline noteskin unit suite: **261 passed**. Final: **266 passed**, preserving every existing result and adding five differential/allocation/concurrency tests. Three paired benchmarks pass when run separately.
- Parser checks compare every section/key/value against the frozen parser for all shipped files, explicit edge cases, and 512 deterministic generated inputs. Merge checks cover all 625 ordered pairs of shipped metrics plus explicit fallback precedence and ownership checks.
- Cache checks compare behavior with the frozen implementation across styles, aliases, ASCII/non-ASCII names, load errors, expiry, clear, reentrant loading, and simultaneous loads. Hash and allocation checks cover 32-byte boundaries and UTF-8 sequences through 4 KiB.
- All **5 noteskin pack integration tests pass**, including customized metric precedence, base/common fallbacks, independent runtime assets, relocated model textures, and cache identities.
- ITGmania semantic harness: **112 passed, 61 existing failures, 74 ignored**. All 247 outcomes and 61 failure diagnostics are unchanged.
- ITGmania actor conformance: **30 passed, 1 existing failure**. All 31 outcomes and the failure diagnostic are unchanged. Only thread IDs were normalized when comparing diagnostics.
- Both Epidermis archives and Sharkmode were attempted on both builds. All three stop before comparisons at the same existing `noteskin dependency changed: common/common/Fallback Receptor.lua` guard, with identical expected/actual hashes. These full-song checks are **blocked, not passed**; zero full-song comparisons ran.
- Changed Rust files pass rustfmt checks; `git diff --check` passes.

## Reproduce

Run from this worktree:

```powershell
cargo test --release --locked --config profile.release.lto=false -p deadsync-noteskin --lib -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false -p deadsync-noteskin --lib perf_tests::benchmark_ -- --ignored --nocapture --test-threads=1
cargo test --release --locked --config profile.release.lto=false -p deadsync-noteskin --test packs -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false --test song_lua_itgmania_semantic_parity -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false --test itgmania_actor_conformance -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false --test full_song_lua -- b40903481944c3395db4c848c671e27a383d2cbea51c74b029b7dea9dbccf16a.tar.zst
cargo test --release --locked --config profile.release.lto=false --test full_song_lua -- a867a12d25c984a49c2dd29fa06d47f88afe01841ee9d410dbf1332800797336.tar.zst
cargo test --release --locked --config profile.release.lto=false --test full_song_lua -- db60b3a89af8957a5e4fb1be2ec3157d09c436a564938536ec91b8ab064afa45.tar.zst
```

Raw evidence remains in this worktree's ignored `target/`: baseline/current native logs, baseline/final noteskin unit logs, `final-deadsync_noteskin-bench.log`, `current-packs.log`, and `compatibility-comparison.json`.
