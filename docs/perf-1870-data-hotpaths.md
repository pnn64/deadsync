# Reduce allocations in the pack browser

- Branch: `perf/1870-data-hotpaths-20261009`.
- Base: `1c41dfa1e9cadf9e14fe64051c9c9cfa387a6e3e`, the latest committed local main when this worktree was created.
- Version: **0.5.1869 → 0.5.1870**, exactly one patch increment in Cargo.toml and all three workspace-version entries in Cargo.lock.
- Nineteen pre-existing unmerged `perf/` branches were checked. None changes these three pack-browser files.

## Changes

1. `SongRow::subline` appends borrowed artist, BPM, length, and credit text directly into one exactly sized string. It removes the temporary BPM string and heap vector used for joining. The content-browser detail view calls this for visible song rows.
2. `Entry::into_pack` moves the popularity response’s name and selected banner URL into `PopularPack`. It replaces the cloning banner helper, preserving small → medium → full precedence, including a present empty URL. Pagination, ranking, and network requests are unchanged.
3. Archive `merge_ranges` sorts and coalesces the consumed vector in place. It removes the second range allocation and copies into it. The same helper serves range union, response coverage checks, and folder download planning; ordering, gap limits, and saturating arithmetic are preserved.

Windows checkout preparation: system Git `core.autocrlf=true` initially converted noteskin assets to CRLF and triggered the native SHA-256 guards. Before the measured baseline, 240 files in this worktree were restored byte-for-byte from their committed blobs; the Git index tree stayed unchanged. This matches the original checkout’s LF bytes. Both final baseline and current runs use those exact files, with all hash guards intact. Future Windows worktrees can be created with `git -c core.autocrlf=false worktree add ...` to avoid this checkout artifact.

## Measurements

Windows x86-64, Intel Xeon E5-2696 v4 @ 2.20 GHz; rustc 1.98.1 (48a229cea), LLVM 22.1.8. Release builds, LTO disabled for both implementations, two Cargo build jobs. These are focused kernel measurements, not whole-application frame rates or network download times.

Each result is the median of nine measurements after discarding the first of ten alternating original/current pairs. Inputs are prepared outside timing, preserving capacities; input consumption and output destruction are timed. Final timings use the real `deadsync-online` test binary with this pass’s builds and harness idle. Other user jobs were left untouched.

The test-only System allocator records allocation churn separately; counting is disabled during timings. Bytes below are newly requested allocation bytes, not RSS or retained heap size. Input allocations are excluded. Frozen original implementations under `tests/support/perf_1870_*.rs` were checked against the base commit, with only method-to-function and loop-body extraction adapters.

| Case | Original ns/op | Current ns/op | Throughput | Allocations before → after | New bytes before → after |
|---|---:|---:|---:|---:|---:|
| `ranges/0/sparse` | 17.50 | 5.00 | 3.50× | 0 → 0 | 0 → 0 |
| `ranges/1/sparse` | 108.90 | 34.50 | 3.16× | 1 → 0 | 16 → 0 |
| `ranges/8/sparse` | 159.46 | 74.94 | 2.13× | 1 → 0 | 128 → 0 |
| `ranges/8/overlap` | 155.82 | 78.94 | 1.97× | 1 → 0 | 128 → 0 |
| `ranges/128/sparse` | 1090.23 | 662.11 | 1.65× | 1 → 0 | 2048 → 0 |
| `ranges/128/overlap` | 1187.11 | 1018.36 | 1.17× | 1 → 0 | 2048 → 0 |
| `ranges/128/shuffled` | 2831.64 | 2510.55 | 1.13× | 1 → 0 | 2048 → 0 |
| `ranges/4096/sparse` | 42533.98 | 23432.03 | 1.82× | 1 → 0 | 65536 → 0 |
| `ranges/4096/overlap` | 32924.61 | 31982.81 | 1.03× | 1 → 0 | 65536 → 0 |
| `ranges/4096/shuffled` | 141462.89 | 123652.73 | 1.14× | 1 → 0 | 65536 → 0 |
| `subtitle/empty` | 76.94 | 15.29 | 5.03× | 1 → 0 | 64 → 0 |
| `subtitle/artist` | 145.64 | 82.23 | 1.77× | 2 → 1 | 70 → 6 |
| `subtitle/bpm` | 364.81 | 86.52 | 4.22× | 3 → 1 | 99 → 11 |
| `subtitle/full` | 402.08 | 104.44 | 3.85× | 3 → 1 | 131 → 43 |
| `subtitle/unicode` | 292.34 | 101.27 | 2.89× | 3 → 1 | 109 → 37 |
| `subtitle/long` | 393.07 | 126.92 | 3.10× | 3 → 1 | 1271 → 1183 |
| `popular/0/banners0` | 15.79 | 16.02 | 0.99× | 0 → 0 | 0 → 0 |
| `popular/1/banners0` | 598.04 | 533.05 | 1.12× | 2 → 1 | 75 → 64 |
| `popular/1/banners1` | 683.01 | 508.83 | 1.34× | 3 → 1 | 98 → 64 |
| `popular/1/banners7` | 779.98 | 597.05 | 1.31× | 3 → 1 | 96 → 64 |
| `popular/100/banners0` | 12515.62 | 4946.88 | 2.53× | 101 → 1 | 7500 → 6400 |
| `popular/100/banners1` | 21670.31 | 7503.12 | 2.89× | 201 → 1 | 9800 → 6400 |
| `popular/100/banners7` | 25560.94 | 11059.38 | 2.31× | 201 → 1 | 9600 → 6400 |
| `popular/1200/banners0` | 172053.12 | 72918.75 | 2.36× | 1201 → 1 | 90000 → 76800 |
| `popular/1200/banners1` | 308381.25 | 116057.81 | 2.66× | 2401 → 1 | 117600 → 76800 |
| `popular/1200/banners7` | 395126.56 | 189540.62 | 2.08× | 2401 → 1 | 115200 → 76800 |

Subtitle cases cover empty, single-field, complete, Unicode, and long text. Range cases cover empty, singleton, sparse, overlapping, and shuffled inputs with the production 64 KiB merge gap. Popularity cases contain 0, 1, 100, or 1,200 entries; banner masks 0/1/7 mean none/full/all three URLs. The popularity batch benchmark includes one equally sized output-vector allocation in both versions. Every current case has zero reallocations. The original BPM/full/long subtitle cases each also reallocate once. All 25 cases with differing work improve; the empty ranking control does no entry conversion in either variant and measures 15.79 versus 16.02 ns (a 0.23 ns difference).

## Regression checks

| Suite | Original cases | Current passed | Current failed | Current ignored |
|---|---:|---:|---:|---:|
| `deadsync_online-tests` | 355 | 359 | 0 | 4 |
| `semantic` | 249 | 138 | 37 | 74 |
| `actor` | 31 | 30 | 1 | 0 |

All original test outcomes are unchanged. Five new behavioral/allocation tests pass, and the three ignored paired benchmarks were explicitly run and passed. New checks cover 48 subtitle field/text combinations plus long Unicode text, 670 range cases plus buffer ownership, all 16 banner/name combinations with string pointer identity, and parsed JSON defaults/stale banner variants. Existing archive tests exercise byte contents, tail splitting, multipart requests, batching, missing ranges, and budgets against an in-memory server.

The Song Lua ITGmania semantic and actor conformance suites were run before and after. All 38 failure diagnostic sections match after normalizing panic thread IDs and the unordered three missing-actor diagnostics in `image_texture_aliases_match_native_draws`. Those three lines come from HashMap iteration; three focused reruns of the unchanged current binary confirmed different permutations. All other diagnostic content matches exactly. Existing failures were not altered; this is evidence of unchanged tested behavior, not a fully passing compatibility suite.

- Full-song `319|TECH SOUP|[lv.P.Clark] Epidermis` / `1c0225a7d2fc61cdd507b7884bef6a80a9a1fc5fecf5c313e2c5c18fa0466857.tar.zst`: identical before/after result **FAILED**, 0 comparisons passed, 0 failed (0 total).
  Existing blocker at `tests\song_lua_itgmania_semantic_parity\whole_song_archives.rs:593:10`: `missing native Song::GetLastSecond endpoint; recapture this archive`.
- Full-song `319|TECH SOUP|[lv.P.Clark] Epidermis` / `a84ff944882c91f701e6b0ea4064c25949f0eee87705e843ea5b209b89eaa83a.tar.zst`: identical before/after result **FAILED**, 0 comparisons passed, 0 failed (0 total).
  Existing blocker at `tests\song_lua_itgmania_semantic_parity\whole_song_archives.rs:593:10`: `missing native Song::GetLastSecond endpoint; recapture this archive`.
- Full-song `280|MODS|[MASTER] Sharkmode` / `c0a1741ca9d19cd8b8baf1292402b010423732e0eebad4d371fadb8d6430e85a.tar.zst`: identical before/after result **FAILED**, 0 comparisons passed, 0 failed (0 total).
  Existing blocker at `tests\song_lua_itgmania_semantic_parity\whole_song_archives.rs:593:10`: `missing native Song::GetLastSecond endpoint; recapture this archive`.
- Full-song `Warp Zone` / `b38698ececd60bdf311a15de205f43d0f3466eac6d41296b9abf469485ebab4d.tar.zst`: identical before/after result **ok**, 212220 comparisons passed, 0 failed (212220 total).
- Full-song `Let Me Hear That` / `0229b74d092e8f04278d64b7c994877a0a3bcc52e9d7ddb00d6a98f1f5244cd0.tar.zst`: identical before/after result **ok**, 205071 comparisons passed, 0 failed (205071 total).
- Full-song `272|MODS|[lv.02] Riddle` / `920eb361ce688e70b4364258337b30c9e4bf7d337f2c83a30e59215eb838735d.tar.zst`: identical before/after result **ok**, 201471 comparisons passed, 0 failed (201471 total).

Warp Zone, Let Me Hear That, and Riddle pass all **618,762 native comparisons** before and after. The two legacy Epidermis fixtures and Sharkmode stop before comparisons because they lack the newly required native `Song::GetLastSecond` endpoint. No archive, reference hash, Song Lua source content, or golden result was changed. Live online services were not exercised.

## Reproduction

From this worktree, run:

```powershell
$env:CARGO_BUILD_JOBS = "2"
cargo test --release --locked --config profile.release.lto=false -p deadsync-online --lib -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false -p deadsync-online --lib benchmark_ -- --ignored --nocapture --test-threads=1
cargo test --release --locked --config profile.release.lto=false --test song_lua_itgmania_semantic_parity --test itgmania_actor_conformance -- --test-threads=1
```

Run each full-song archive above with `cargo test --release --locked --config profile.release.lto=false --test full_song_lua -- <archive>`. Raw before/after build, test, and benchmark logs and `compatibility-comparison.json` remain in this worktree’s ignored `target/` directory.

The original checkout and its uncommitted work were not edited. This pass is committed only on its isolated perf branch; it is not merged. The commit excludes deadsync-song.json.gz, rust-performance.md, optimize.sh, and optimize.ps1.
