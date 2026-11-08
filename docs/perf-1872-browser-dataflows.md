# Browser data flows, 0.5.1872

Branch: `perf/1872-browser-dataflows-20261009`. Base: `735a994c75240ae043dc153204572d6952ed14fc`, the latest committed main when this worktree was created. Workspace version: **0.5.1871 -> 0.5.1872**, changed once in Cargo.toml and all three workspace-package entries in Cargo.lock. The original checkout was left untouched; this branch is not merged.

Checked all 21 pre-existing unmerged `perf/` branches. The pending pack-page change optimizes `subline`; this pass changes `low_meter`. The pending search, leaderboard, archive, catalogue and SRPG changes are separate. No Song Lua source, reference hashes, archives, or golden data changed.

## Changes

1. **Meter prefixes:** borrow the leading ASCII digit slice after `trim_start`, then parse it directly. Removes the collected temporary String for every song considered by beginner-pack classification. Unicode leading whitespace, first-number behavior, overflow and missing values retain their previous meaning.
2. **Verdict JSON:** serialize `HashMap<u64, bool>` directly, removing the temporary `HashMap<String, bool>` and every allocated key. The JSON object still has decimal string keys and boolean values. Object order is unspecified in both implementations. Parent creation, temporary write, rename and failed-rename cleanup are unchanged.
3. **View descriptions:** remove the runtime's second owned map and keep descriptions in the published snapshot. Only mutations with an outstanding reader copy the map; phase changes reuse it. Publication also reuses the snapshot when uniquely owned. Old snapshots and separately retained map Arcs remain immutable, and phases, revision increments, generation checks, lookup suppression and refresh behavior are preserved.

## Measurements

Windows x86_64 MSVC; Intel Xeon E5-2696 v4 @ 2.20 GHz; rustc 1.98.1 (48a229cea, 2026-09-01), LLVM 22.1.8. Release, locked dependencies, `profile.release.lto=false`. No pass-owned compilation or compatibility run overlapped the benchmark process; unrelated user work was left running. These are operation benchmarks, not whole-app FPS, network latency, disk throughput, RSS or hardware-cycle measurements.

Each case alternates original/current order across ten pairs, discards the first pair and reports each variant's median of nine. The discarded pair calibrates each variant to roughly two milliseconds per measured batch, capped at one million operations and a 1,000-fold increase; inputs remain prepared outside timing. A test-only System allocator records allocation calls and requested bytes in a separate invocation, with counting disabled during timing. Requested bytes include reallocations and are not peak/live memory. Original implementations are frozen from the exact base and audited against it; meter wrappers use the same SongRow and PackPage inputs. The verdict benchmark measures the exact serialization expression/block used by each file writer, excluding filesystem latency. File-side effects have separate regression tests.

View publication cases vary map size, whether view phases changed, and whether a reader holds the immediately preceding snapshot. Response cases replace 200 or seven rows of an existing 200/1,200-row map, with every retained-reader case holding the immediate predecessor. Incoming response data is prepared outside timing; consumption and drops are included. First-load allocation savings are separately asserted in the tests.

| Operation | Original ns/op | Current ns/op | Throughput | Allocations | Reallocations | Requested bytes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| verdict/0 | 92.35 | 66.38 | 1.39x | 1 -> 1 | 0 -> 0 | 128 -> 128 |
| verdict/1 | 347.41 | 93.40 | 3.72x | 3 -> 1 | 0 -> 0 | 296 -> 128 |
| verdict/7 | 1454.01 | 334.36 | 4.35x | 9 -> 1 | 1 -> 1 | 804 -> 384 |
| verdict/60 | 10748.56 | 1881.45 | 5.71x | 62 -> 1 | 4 -> 4 | 9408 -> 3968 |
| verdict/1200 | 306081.93 | 29889.16 | 10.24x | 1202 -> 1 | 9 -> 9 | 222544 -> 130944 |
| verdict/9000 | 2063200.00 | 246585.00 | 8.37x | 9002 -> 1 | 11 -> 11 | 1244848 -> 524160 |
| meter/single | 84.58 | 10.92 | 7.74x | 1 -> 0 | 0 -> 0 | 8 -> 0 |
| meter/range | 88.48 | 10.41 | 8.50x | 1 -> 0 | 0 -> 0 | 8 -> 0 |
| meter/unicode-space | 88.34 | 14.06 | 6.28x | 1 -> 0 | 0 -> 0 | 8 -> 0 |
| meter/overflow | 224.69 | 33.80 | 6.65x | 1 -> 0 | 1 -> 0 | 24 -> 0 |
| meter/empty | 8.17 | 5.77 | 1.41x | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| meter/invalid | 9.63 | 7.98 | 1.21x | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| meter/page-7 | 640.28 | 89.27 | 7.17x | 6 -> 0 | 1 -> 0 | 64 -> 0 |
| meter/page-200 | 18265.00 | 2539.90 | 7.19x | 172 -> 0 | 28 -> 0 | 1824 -> 0 |
| meter/page-1000 | 101161.00 | 14708.50 | 6.88x | 857 -> 0 | 143 -> 0 | 9144 -> 0 |
| view/0-moved-true-retained-false | 510.79 | 273.29 | 1.87x | 4 -> 2 | 0 -> 0 | 252 -> 116 |
| view/0-moved-true-retained-true | 528.73 | 434.71 | 1.22x | 4 -> 3 | 0 -> 0 | 252 -> 188 |
| view/7-moved-true-retained-false | 3374.76 | 272.50 | 12.38x | 40 -> 2 | 0 -> 0 | 1749 -> 116 |
| view/7-moved-true-retained-true | 3665.31 | 447.30 | 8.19x | 40 -> 3 | 0 -> 0 | 1749 -> 188 |
| view/200-moved-true-retained-false | 78886.00 | 249.59 | 316.06x | 1005 -> 2 | 0 -> 0 | 45094 -> 116 |
| view/200-moved-true-retained-true | 80318.00 | 391.08 | 205.38x | 1005 -> 3 | 0 -> 0 | 45094 -> 188 |
| view/1200-moved-true-retained-false | 473920.00 | 231.19 | 2049.93x | 6005 -> 2 | 0 -> 0 | 311446 -> 116 |
| view/1200-moved-true-retained-true | 538160.00 | 433.66 | 1240.98x | 6005 -> 3 | 0 -> 0 | 311446 -> 188 |
| view/3000-moved-true-retained-false | 1228395.00 | 251.78 | 4878.91x | 15005 -> 2 | 0 -> 0 | 696934 -> 116 |
| view/3000-moved-true-retained-true | 1326145.00 | 440.12 | 3013.11x | 15005 -> 3 | 0 -> 0 | 696934 -> 188 |
| view/1200-moved-false-retained-false | 385.52 | 250.36 | 1.54x | 3 -> 2 | 0 -> 0 | 188 -> 116 |
| view/1200-moved-false-retained-true | 390.22 | 413.40 | 0.94x | 3 -> 3 | 0 -> 0 | 188 -> 188 |
| view-response/200-200-retained-false | 127160.00 | 34376.00 | 3.70x | 1005 -> 2 | 0 -> 0 | 45404 -> 116 |
| view-response/200-200-retained-true | 119890.00 | 118756.00 | 1.01x | 1005 -> 1005 | 0 -> 0 | 45404 -> 45094 |
| view-response/200-7-retained-false | 88230.00 | 1735.56 | 50.84x | 1005 -> 2 | 0 -> 0 | 45115 -> 116 |
| view-response/200-7-retained-true | 79988.00 | 88948.00 | 0.90x | 1005 -> 1005 | 0 -> 0 | 45115 -> 45094 |
| view-response/1200-7-retained-false | 494115.00 | 1610.41 | 306.83x | 6005 -> 2 | 0 -> 0 | 311467 -> 116 |
| view-response/1200-7-retained-true | 531310.00 | 524575.00 | 1.01x | 6005 -> 6005 | 0 -> 0 | 311467 -> 311446 |

31/33 measured medians improved. Slower medians are retained here: `view/1200-moved-false-retained-true` (390.22 -> 413.40 ns, +23.18 ns); `view-response/200-7-retained-true` (79988.00 -> 88948.00 ns, +8960.00 ns). Retained readers still require a map copy when rows change: those response cases retain the original allocation count, and no reliable throughput improvement is claimed for them. The initial short-batch run had two retained-reader medians about 1% slower (410 -> 415 ns for unchanged views; 472350 -> 479105 ns for a 1,200-row response). Its logs remain in `target/initial-measurements/`; the table uses the subsequent calibrated batches. Nanosecond-scale controls and percent-level variation on this shared host are not evidence of a whole-app change. Allocation counts provide the deterministic evidence of removed work.

The shared metadata-only control retains its allocation count and measured **390.22 -> 413.40 ns**, an additional **23.18 ns** on a path where snapshot reuse adds an ownership check. The 200-row/seven-update retained-reader case measured 11.20% slower in the calibrated run. Seven additional fresh-process runs of the response benchmark investigated that variability, without source changes or selection of a winning run:

| Retained-reader response | Median current-time change across seven runs | Observed range |
| --- | ---: | ---: |
| view-response/200-200-retained-true | +0.78% | -3.87% to +14.47% |
| view-response/200-7-retained-true | +1.02% | -9.19% to +9.31% |
| view-response/1200-7-retained-true | -1.05% | -3.89% to +3.77% |

Those response controls show no reliable throughput improvement; their allocation counts remain equal to the original, while the redundant persistent runtime map is removed. Both the calibrated results and every diagnostic repeat are retained in `target/`. These tradeoffs are reported separately from the behavioral regression results below.

## Regression and ITGmania compatibility checks

- `deadsync-online --lib`: baseline **354 passed, 0 failed, 1 ignored**; current **364 passed, 0 failed, 5 ignored**. All four paired benchmarks were also explicitly run and passed; the existing live-network test remains ignored.
- New tests cover 288 whitespace/prefix/suffix combinations, all ASCII bytes in two positions, long/overflow strings, literal boundary results, and 512 beginner-majority combinations; meter/classification operations allocate nothing.
- Verdict tests cover empty and large maps, zero/u32-max/u64-max keys, boolean preservation, nested directory creation, replacement, empty replacement, rename failure cleanup, and a file blocking parent-directory creation.
- Description tests compare current and frozen original states across phases, both views, retries, empty/error/duplicate-row replies, stale generations and counter wrap. They verify retained snapshots, separately retained maps, unshared map reuse, refresh, unchanged lookup/publication behavior, and first-load/update allocation reductions.
- ITGmania semantic suite: **140 passed, 37 failed, 74 ignored** before and after.
- ITGmania actor suite: **30 passed, 1 failed, 0 ignored** before and after.
- Every baseline test outcome and all **38 existing failure bodies** match. Comparison normalizes panic thread IDs and, only in `image_texture_aliases_match_native_draws`, the order of the existing `Sprite.Load has no matching DeadSync actor` lines emitted by HashMap iteration. Counts and all other failure text remain compared.
- Six full-song ITGmania archives passed before and after: **1,650,933 comparisons per run, zero mismatches**. The harness warns about two unselected local-only archives; this report covers the six selected archives below, not the whole archive catalogue.

- `319-TECH SOUP-[lv.P.Clark] [Tech Spectrum Super - _TRUE GAMERS CLICK HERE - EXTRA CHARTS]/Venetian Snares - Epidermis.ssc`: **363,873** comparisons; `0f2ef3f987449e6218525fba450d1db1201c9f733d4956061d3b36529b922305.tar.zst`.
- `319-TECH SOUP-[lv.P.Clark] [Tech Spectrum Super]/Venetian Snares - Epidermis.ssc`: **363,873** comparisons; `af2f887d212d8ab702b772cea9c953500254ce01a6b78b423276fca9cf5a8306.tar.zst`.
- `280-MODS-[MASTER] Sharkmode/Sharkmode.ssc`: **304,425** comparisons; `b05379b7d12bfb02149e03f166be5d496dc3e7687dcc042f0bf525cc46aa918b.tar.zst`.
- `(R10) Warp Zone/warp zone.ssc`: **212,220** comparisons; `b38698ececd60bdf311a15de205f43d0f3466eac6d41296b9abf469485ebab4d.tar.zst`.
- `(R5) Let Me Hear That/let me hear that.sm`: **205,071** comparisons; `0229b74d092e8f04278d64b7c994877a0a3bcc52e9d7ddb00d6a98f1f5244cd0.tar.zst`.
- `272-MODS-[lv.02] Riddle/Riddle.ssc`: **201,471** comparisons; `920eb361ce688e70b4364258337b30c9e4bf7d337f2c83a30e59215eb838735d.tar.zst`.

## Reproduce

```powershell
cargo test --release --locked --config profile.release.lto=false -p deadsync-online --lib
cargo test --release --locked --config profile.release.lto=false -p deadsync-online --lib benchmark_ -- --ignored --nocapture --test-threads=1
cargo test --release --locked --config profile.release.lto=false --test song_lua_itgmania_semantic_parity -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false --test itgmania_actor_conformance -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false --test full_song_lua -- <archive filename above>
```

Run baseline validation in an LF checkout of the base commit, then the same commands here. Raw baseline/current build logs, test logs, benchmark output, archive selections and `compatibility-comparison.json` remain under this worktree's ignored `target/`. Only source, test support, this report and the two Cargo version files are committed. `deadsync-song.json.gz`, `rust-performance.md`, `optimize.sh` and `optimize.ps1` are excluded.
