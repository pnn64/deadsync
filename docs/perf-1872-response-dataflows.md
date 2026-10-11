# Performance pass: response data flows

Branch: `perf/1872-response-dataflows-20261009`. Starting committed main: `361286535d5458eead4c0fa73f4f345559aea936`. Version: **0.5.1871 -> 0.5.1872**, in Cargo.toml and all three matching workspace entries in Cargo.lock.

Read rust-performance.md and inspected 23 existing unmerged perf branches before selecting this work. The pending pack-page changes in `perf/1870-data-hotpaths-20261009` and `perf/1872-browser-dataflows-20261009` concern `SongRow::subline` and `SongRow::low_meter`; both methods remain byte-for-byte unchanged. No inspected pending branch changes banners.rs. Work is isolated in a new worktree; the original checkout continued to receive independent user work. No merge or push was performed.

## Changes

1. **Banner cache:** return immediately when the total slot count cannot exceed the 192-banner bound. This removes the per-frame hash-table scan, usage lookups and temporary vector for the bounded cache. Pending and failed slots conservatively count toward this guard. The existing overflow selection, tie ordering and state mutations remain unchanged above the bound.
2. **Histogram numbers:** accumulate ASCII digits directly with checked u32 arithmetic. This removes one temporary String per numeric piece. All digits anywhere in a comma-separated piece still concatenate; empty/non-numeric and overflowing pieces still disappear, and arbitrarily long leading zeros remain valid.
3. **Pack-page HTML:** borrow table cells, titles and artists from the response until final text cleanup. This removes ten temporary allocations per complete song row and shrinks the cell-vector entries from String to &str. Tag handling, entity decoding, credit separators, malformed-row handling and returned owned data are unchanged.

All production changes are outside Song Lua. Frozen starting implementations and regression/benchmark support compile only for tests. Pack-response tests use distinct module/file names to avoid a collision with the pending subline pass. The online checks and benchmarks were rerun after this naming change; the first full run is retained under target/before-test-rename-*. Native tests used byte-identical production code; only cfg(test) module names and test filenames changed afterwards.

## Benchmark method and results

Windows x86-64/MSVC, Intel Xeon E5-2696 v4 at 2.20 GHz, `rustc 1.98.1 (48a229cea 2026-09-01)`, release optimization with LTO disabled. The crate-local test binary benchmarks the production functions against frozen starting implementations, using the same input and production data types. This pass had no compilation or compatibility jobs running during measurements; unrelated work on the shared workstation was left alone.

Timing uses ten alternating original/current sample pairs, discards the first warm-up pair, and reports the median of nine samples per implementation. Stable inputs are prepared outside timing; inputs/results are black-boxed, and output destruction is included. Fast batches calibrate to approximately 2 ms. The two eviction controls restore cloned maps outside each timed call and include identical timer overhead. They preserve map seeds and iteration order for exact LRU-tie comparisons. These are function-level wall-time measurements, not whole-application FPS or CPU-cycle claims.

| Case | Original ns/op | Current ns/op | Throughput ratio |
|---|---:|---:|---:|
| banner-empty | 23.24 | 18.40 | 1.26x |
| banner-24 | 724.63 | 16.85 | 43.00x |
| banner-80 | 1,984.80 | 17.29 | 114.78x |
| banner-192 | 4,431.80 | 18.29 | 242.30x |
| banner-mixed-192 | 2,054.00 | 16.85 | 121.93x |
| banner-pending-400 | 613.12 | 586.73 | 1.04x |
| banner-done-80-total-480 | 2,524.20 | 2,648.00 | 0.95x |
| banner-evict-193 | 5,580.00 | 5,543.00 | 1.01x |
| banner-evict-512 | 50,121.00 | 52,669.00 | 0.95x |
| rows-0-ascii | 60.30 | 63.54 | 0.95x |
| rows-1-ascii | 11,863.89 | 11,503.81 | 1.03x |
| rows-24-ascii | 270,826.67 | 264,420.00 | 1.02x |
| rows-200-ascii | 2,213,830.00 | 2,132,833.33 | 1.04x |
| rows-200-unicode | 2,281,476.67 | 2,209,883.33 | 1.03x |
| rows-200-incomplete | 96,176.67 | 75,796.67 | 1.27x |
| page-200-combined | 2,395,146.67 | 2,309,183.33 | 1.04x |
| numbers-empty | 15.98 | 15.90 | 1.00x |
| numbers-24 | 2,108.67 | 569.40 | 3.70x |
| numbers-60 | 4,856.80 | 1,085.74 | 4.47x |
| numbers-200 | 18,373.00 | 3,061.57 | 6.00x |
| numbers-mixed | 13,502.50 | 2,457.50 | 5.49x |
| numbers-zeros-overflow | 17,605.00 | 6,515.00 | 2.70x |

All measured cases are shown, including controls. Cases with a higher current median: `banner-done-80-total-480` +123.80 ns (4.90%); `banner-evict-512` +2548.00 ns (5.08%); `rows-0-ascii` +3.24 ns (5.37%).

The two slower over-bound banner controls were checked in three fixed additional runs of the same final binary (without changing code or selecting a preferred run):

| Repeat | Case | Original ns/op | Current ns/op | Current time change |
|---:|---|---:|---:|---:|
| 1 | banner-done-80-total-480 | 2,605.80 | 2,643.50 | +1.45% |
| 1 | banner-evict-512 | 48,604.00 | 48,816.50 | +0.44% |
| 2 | banner-done-80-total-480 | 2,587.70 | 2,617.70 | +1.16% |
| 2 | banner-evict-512 | 45,732.50 | 45,404.50 | -0.72% |
| 3 | banner-done-80-total-480 | 3,021.10 | 2,973.20 | -1.59% |
| 3 | banner-evict-512 | 47,136.00 | 52,788.00 | +11.99% |

Including the main run, the mixed over-bound control ranges from 1.59% faster to 4.90% slower, and the 512-banner eviction control from 0.72% faster to 11.99% slower. These controls retain identical allocation counts and eviction results, but this shared-host measurement does not establish unchanged overflow throughput. No overflow speedup is claimed. The bounded-cache fast path consistently eliminates allocation churn; all nonempty parser cases improve in both full benchmark runs. Raw repeat logs are retained as target/banner-control-repeat-1.log through -3.log.

Allocation tracking uses a scoped thread-local System allocator counter in separate calls, disabled during timing; both variants still use the same test allocator wrapper. Counts include all intermediate allocations and reallocations; returned results are kept alive until counting stops. Bytes are cumulative requested allocation sizes (including reallocations), not peak live memory or RSS.

| Case | Allocations old -> new | Reallocations old -> new | Requested bytes old -> new |
|---|---:|---:|---:|
| banner-empty | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| banner-24 | 1 -> 0 | 3 -> 0 | 960 -> 0 |
| banner-80 | 1 -> 0 | 5 -> 0 | 4,032 -> 0 |
| banner-192 | 1 -> 0 | 6 -> 0 | 8,128 -> 0 |
| banner-mixed-192 | 1 -> 0 | 5 -> 0 | 4,032 -> 0 |
| banner-pending-400 | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| banner-done-80-total-480 | 1 -> 1 | 5 -> 5 | 4,032 -> 4,032 |
| banner-evict-193 | 1 -> 1 | 6 -> 6 | 8,128 -> 8,128 |
| banner-evict-512 | 1 -> 1 | 7 -> 7 | 16,320 -> 16,320 |
| rows-0-ascii | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| rows-1-ascii | 84 -> 74 | 9 -> 9 | 2,915 -> 2,478 |
| rows-24-ascii | 1,993 -> 1,753 | 219 -> 219 | 63,370 -> 52,798 |
| rows-200-ascii | 16,601 -> 14,601 | 1,806 -> 1,806 | 533,706 -> 444,566 |
| rows-200-unicode | 16,601 -> 14,601 | 2,596 -> 2,596 | 567,116 -> 471,976 |
| page-200-combined | 16,624 -> 14,614 | 2,599 -> 2,599 | 567,649 -> 472,429 |
| numbers-empty | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| numbers-24 | 25 -> 1 | 3 -> 3 | 432 -> 240 |
| numbers-60 | 61 -> 1 | 4 -> 4 | 976 -> 496 |
| numbers-200 | 201 -> 1 | 6 -> 6 | 3,632 -> 2,032 |
| numbers-mixed | 82 -> 1 | 44 -> 4 | 1,784 -> 496 |
| numbers-zeros-overflow | 4 -> 1 | 19 -> 0 | 24,584 -> 16 |

## Behavioral and native compatibility checks

Nine new regression tests cover 48 combinations of banner bounds/pending/failure/LRU ties; exact slot, URL, usage, clock and shared-failure state; preserved channels; allocation-free bounded caches with sparse capacity; 37,449 exhaustive short numeric strings; u32 limits, leading zeros and Unicode; histogram indexing and saturating totals; malformed HTML and every UTF-8 truncation boundary of a two-song page; full pages up to 200 songs; and allocation reductions.

| Suite | Passed | Failed | Ignored | Baseline comparison |
|---|---:|---:|---:|---|
| deadsync_online-tests | 363 | 0 | 4 | All 355 original outcomes unchanged; 12 added |
| semantic | 141 | 37 | 74 | All 252 original outcomes unchanged; 0 added |
| actor | 30 | 1 | 0 | All 31 original outcomes unchanged; 0 added |

The native suites retain **38 baseline failures**; they are not fully green. Every failure body matches before/after after removing process-specific panic thread IDs and sorting only the known unordered `Sprite.Load has no matching DeadSync actor` diagnostics in `image_texture_aliases_match_native_draws`. No other failure text/order is normalized.

Six selected full-song ITGmania archives pass before and after with identical comparison counts:

| Archive source | Comparisons passed per run |
|---|---:|
| `319-TECH SOUP-[lv.P.Clark] [Tech Spectrum Super - _TRUE GAMERS CLICK HERE - EXTRA CHARTS]/Venetian Snares - Epidermis.ssc` | 363,873 |
| `319-TECH SOUP-[lv.P.Clark] [Tech Spectrum Super]/Venetian Snares - Epidermis.ssc` | 363,873 |
| `280-MODS-[MASTER] Sharkmode/Sharkmode.ssc` | 304,425 |
| `(R10) Warp Zone/warp zone.ssc` | 212,220 |
| `(R5) Let Me Hear That/let me hear that.sm` | 205,071 |
| `272-MODS-[lv.02] Riddle/Riddle.ssc` | 201,471 |

**1,650,933 full-song comparisons pass per run.** The harness warns that two unselected local-only archives are unavailable; this report does not claim complete archive coverage.

## Reproduction

```powershell
cargo test --release --locked --config profile.release.lto=false -p deadsync-online --lib
cargo test --release --locked --config profile.release.lto=false -p deadsync-online --lib benchmark_ -- --ignored --nocapture --test-threads=1
cargo test --release --locked --config profile.release.lto=false --test song_lua_itgmania_semantic_parity -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false --test itgmania_actor_conformance -- --test-threads=1
```

Run the custom full-song binary through Cargo with each exact archive selector:

```powershell
cargo test --release --locked --config profile.release.lto=false --test full_song_lua -- 0f2ef3f987449e6218525fba450d1db1201c9f733d4956061d3b36529b922305.tar.zst
cargo test --release --locked --config profile.release.lto=false --test full_song_lua -- af2f887d212d8ab702b772cea9c953500254ce01a6b78b423276fca9cf5a8306.tar.zst
cargo test --release --locked --config profile.release.lto=false --test full_song_lua -- b05379b7d12bfb02149e03f166be5d496dc3e7687dcc042f0bf525cc46aa918b.tar.zst
cargo test --release --locked --config profile.release.lto=false --test full_song_lua -- b38698ececd60bdf311a15de205f43d0f3466eac6d41296b9abf469485ebab4d.tar.zst
cargo test --release --locked --config profile.release.lto=false --test full_song_lua -- 0229b74d092e8f04278d64b7c994877a0a3bcc52e9d7ddb00d6a98f1f5244cd0.tar.zst
cargo test --release --locked --config profile.release.lto=false --test full_song_lua -- 920eb361ce688e70b4364258337b30c9e4bf7d337f2c83a30e59215eb838735d.tar.zst
```

Formatting was checked on all changed Rust files, `git diff --check` passed, and an explicit file allowlist excludes deadsync-song.json.gz, rust-performance.md, optimize.sh and optimize.ps1. Raw baseline/current logs, executable manifests, pending-branch scope records and comparison JSON are retained under this worktree's ignored target directory.
