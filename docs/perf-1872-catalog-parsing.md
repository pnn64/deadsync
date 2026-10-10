# Catalogue search and metadata performance pass

Branch: `perf/1872-catalog-parsing-20261009`.
Base: `4ca2c55fba8006d83b9ea427b1922a4dc6448b31` (committed main at creation).
Version: **0.5.1871 -> 0.5.1872**, exactly one patch increment in Cargo.toml and all three workspace package entries in Cargo.lock.

## Changes

1. **Catalogue name search:** seed the accumulator directly. `parse_catalog` rejects duplicate IDs before publishing a catalogue, so checking every prior hit for duplicates during the first pass was redundant and quadratic in the number of matches. Credit/title passes still use the original merge/nudge logic. Order, score, reason, cap handling and context-sensitive Unicode lowercasing are preserved; no secondary index or cached lowercase representation was added.
2. **Detail-page scraping:** use literal attribute prefixes, borrow text through date validation and allocate a date only when accepted. This removes 400 temporary allocations per 200 valid rows, or 600 when their nonempty dates are invalid. HTML substring matching, missing/malformed data, entity replacement order, URLs and the existing date-validation rules are unchanged.
3. **Audio/chart file classification:** borrow the extension and compare ASCII case without allocating a lowercase String for each predicate. Keep lowercase owned extensions at the two output boundaries that require them. Preview search still examines entries in the same order, preserves Unicode basename matching, prioritizes clip/music/fallback identically and retains the first match.

Reviewed all 22 unmerged `perf/` branches before selection. In particular, the pending search pass owns runtime query borrowing, date sorting and URL encoding: those functions remain byte-for-byte equal to this base. No Song Lua, native fixture or golden-reference edits. The original checkout was not edited. This branch is committed separately and not merged.

## Method

Windows x86_64 MSVC; Intel Xeon E5-2696 v4 @ 2.20 GHz; rustc 1.98.1 (48a229cea, LLVM 22.1.8). Release tests, locked dependencies, `profile.release.lto=false`, two Cargo jobs. Reused the existing build cache; timings ran after this pass's compilation/tests finished and before the compatibility build. Unrelated user jobs were left alone.

Frozen original functions/loop come from the exact base above, with visibility/argument adapters only, and are checked by the pass audit. Original and current run in the same executable on identical inputs. Each benchmark uses ten alternating old/new pairs, discards the first, calibrates subsequent batches toward 2 ms, and reports each variant's median of nine samples. Inputs are prepared outside timing; outputs are consumed and dropped inside. Allocation counts are measured separately with a thread-local System allocator wrapper; counting is disabled during timings. Requested bytes include reallocations and are allocation churn, not peak live memory or RSS.

These are isolated CPU-work/elapsed-time and allocation measurements, not whole-application FPS, hardware cycle counts or network throughput. Empty/no-match inputs are included as controls; no speedup is expected when the removed work was never performed. All measured nonempty cases that exercise the removed work improved. Controls varied: the empty catalogue's `soup` case was 1.34 ns slower (15.04 -> 16.38 ns), the empty valid-details case was 4.12 ns slower, and the 9,000-row no-match search was 0.21% slower. No throughput benefit is claimed for those controls; allocations are unchanged.

## Paired release measurements

`catalogue/N/query` scans N mixed ASCII/Unicode pack names. `details/N/valid` parses N complete JSON/HTML rows; `invalid-dates` uses rejected nonempty dates. `preview/N/case` searches a song folder containing N images plus two audio entries (zero is an empty folder); `clip` finds the clip and `fallback` misses both requested names. `chart-files/N` classifies N mixed chart/audio/image names.

| Case | Original ns/op | Current ns/op | Throughput | Allocations old -> new | Reallocations old -> new | Requested bytes old -> new |
|---|---:|---:|---:|---:|---:|---:|
| details/0/valid | 370.06 | 374.18 | 0.99x | 3 -> 3 | 0 -> 0 | 368 -> 368 |
| details/0/invalid-dates | 366.51 | 361.79 | 1.01x | 3 -> 3 | 0 -> 0 | 368 -> 368 |
| details/1/valid | 3,807.25 | 3,756.54 | 1.01x | 23 -> 21 | 7 -> 5 | 2173 -> 2122 |
| details/1/invalid-dates | 3,756.92 | 3,189.52 | 1.18x | 23 -> 20 | 7 -> 5 | 2193 -> 2122 |
| details/200/valid | 833,220.00 | 751,240.00 | 1.11x | 3406 -> 3006 | 1008 -> 608 | 321712 -> 311512 |
| details/200/invalid-dates | 932,240.00 | 848,320.00 | 1.10x | 3406 -> 2806 | 1008 -> 608 | 325712 -> 311512 |
| details/1200/valid | 4,628,830.00 | 4,121,780.00 | 1.12x | 20406 -> 18006 | 6011 -> 3611 | 2005560 -> 1944360 |
| details/1200/invalid-dates | 4,575,100.00 | 3,921,110.00 | 1.17x | 20406 -> 16806 | 6011 -> 3611 | 2029560 -> 1944360 |
| catalogue/0/unmatched | 15.04 | 15.05 | 1.00x | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| catalogue/0/soup | 15.04 | 16.38 | 0.92x | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| catalogue/0/pack | 14.59 | 14.59 | 1.00x | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| catalogue/200/unmatched | 33,421.67 | 33,085.00 | 1.01x | 200 -> 200 | 25 -> 25 | 3964 -> 3964 |
| catalogue/200/soup | 36,246.00 | 35,990.00 | 1.01x | 226 -> 226 | 28 -> 28 | 6589 -> 6589 |
| catalogue/200/pack | 51,938.00 | 43,870.00 | 1.18x | 326 -> 326 | 30 -> 30 | 15169 -> 15169 |
| catalogue/1200/unmatched | 232,380.00 | 228,510.00 | 1.02x | 1200 -> 1200 | 150 -> 150 | 24714 -> 24714 |
| catalogue/1200/soup | 258,640.00 | 241,700.00 | 1.07x | 1351 -> 1351 | 156 -> 156 | 46384 -> 46384 |
| catalogue/1200/pack | 445,750.00 | 265,200.00 | 1.68x | 1951 -> 1951 | 158 -> 158 | 113224 -> 113224 |
| catalogue/9000/unmatched | 1,574,650.00 | 1,577,930.00 | 1.00x | 9000 -> 9000 | 1125 -> 1125 | 194364 -> 194364 |
| catalogue/9000/soup | 2,149,400.00 | 1,631,310.00 | 1.32x | 10126 -> 10126 | 1134 -> 1134 | 368169 -> 368169 |
| catalogue/9000/pack | 13,240,690.00 | 1,910,820.00 | 6.93x | 14626 -> 14626 | 1136 -> 1136 | 900189 -> 900189 |
| preview/0/clip | 387.08 | 385.42 | 1.00x | 4 -> 4 | 0 -> 0 | 40 -> 40 |
| preview/0/fallback | 347.12 | 336.08 | 1.03x | 4 -> 4 | 0 -> 0 | 42 -> 42 |
| preview/4/clip | 944.33 | 523.55 | 1.80x | 9 -> 3 | 0 -> 0 | 49 -> 31 |
| preview/4/fallback | 2,005.27 | 1,255.67 | 1.60x | 20 -> 8 | 0 -> 0 | 118 -> 82 |
| preview/32/clip | 3,243.00 | 1,055.45 | 3.07x | 37 -> 3 | 0 -> 0 | 133 -> 31 |
| preview/32/fallback | 6,507.50 | 2,297.78 | 2.83x | 76 -> 8 | 0 -> 0 | 286 -> 82 |
| preview/256/clip | 21,805.00 | 5,641.50 | 3.87x | 261 -> 3 | 0 -> 0 | 805 -> 31 |
| preview/256/fallback | 43,875.00 | 11,471.00 | 3.82x | 524 -> 8 | 0 -> 0 | 1630 -> 82 |
| chart-files/200 | 17,108.50 | 3,527.71 | 4.85x | 200 -> 0 | 0 -> 0 | 550 -> 0 |
| chart-files/9000 | 821,073.00 | 171,798.00 | 4.78x | 9000 -> 0 | 0 -> 0 | 24750 -> 0 |

## Regression and compatibility results

- Online tests: **363 passed, 0 failed, 4 ignored**. All 355 pre-existing test outcomes are unchanged. Added nine passing regression tests and three explicitly executed benchmark tests; the pre-existing live-network test stayed ignored. Benchmarks: `ok. 3 passed; 0 failed; 0 ignored; 0 measured; 364 filtered out; finished in 6.20s`.
- Differential coverage includes 192 attribute cases, date text/shape/Unicode cases, malformed JSON/rows/columns, valid and invalid pages through 1,200 rows, 55 catalogue/query combinations, final sigma and expanding lowercase forms, duplicate-ID rejection, subsequent score/reason merges, mixed-case/non-ASCII extension and path cases, 1,920 ASCII insertion cases, 144 preview/music combinations, first-match precedence, empty folders, and exact allocation reductions.
- Native Song Lua semantic harness: **141 passed, 37 failed, 74 ignored** before and after.
- ITGmania actor conformance: **30 passed, 1 failed** before and after.
- All 283 native test outcomes and all 38 existing failure diagnostics match the baseline. Normalization is limited to panic thread IDs and the order of `Sprite.Load has no matching DeadSync actor` lines in `image_texture_aliases_match_native_draws`, which are emitted from an unordered map. No other diagnostic differences are accepted.
- Six available full-song ITGmania archives passed before and after: **1,650,933 comparisons per run, zero comparison failures**. Two unselected local-only archives are unavailable; this is not a claim about the entire archive catalogue.

| Archive | Comparisons per run | Fixture archive |
|---|---:|---|
| 319-TECH SOUP-[lv.P.Clark] [Tech Spectrum Super - _TRUE GAMERS CLICK HERE - EXTRA CHARTS]/Venetian Snares - Epidermis.ssc | 363,873 | `0f2ef3f987449e6218525fba450d1db1201c9f733d4956061d3b36529b922305.tar.zst` |
| 319-TECH SOUP-[lv.P.Clark] [Tech Spectrum Super]/Venetian Snares - Epidermis.ssc | 363,873 | `af2f887d212d8ab702b772cea9c953500254ce01a6b78b423276fca9cf5a8306.tar.zst` |
| 280-MODS-[MASTER] Sharkmode/Sharkmode.ssc | 304,425 | `b05379b7d12bfb02149e03f166be5d496dc3e7687dcc042f0bf525cc46aa918b.tar.zst` |
| (R10) Warp Zone/warp zone.ssc | 212,220 | `b38698ececd60bdf311a15de205f43d0f3466eac6d41296b9abf469485ebab4d.tar.zst` |
| (R5) Let Me Hear That/let me hear that.sm | 205,071 | `0229b74d092e8f04278d64b7c994877a0a3bcc52e9d7ddb00d6a98f1f5244cd0.tar.zst` |
| 272-MODS-[lv.02] Riddle/Riddle.ssc | 201,471 | `920eb361ce688e70b4364258337b30c9e4bf7d337f2c83a30e59215eb838735d.tar.zst` |

## Reproduction

```powershell
cargo test --release --locked --config profile.release.lto=false -p deadsync-online --lib -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false -p deadsync-online --lib benchmark_ -- --ignored --nocapture --test-threads=1
cargo test --release --locked --config profile.release.lto=false --test song_lua_itgmania_semantic_parity -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false --test itgmania_actor_conformance -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false --test full_song_lua -- <archive-filename-from-table>
```

The same compatibility commands were run against the base before source changes. Raw baseline/current logs, executable manifests, benchmark output and the exact comparison summary remain in this worktree's ignored `target/` directory. The commit excludes `deadsync-song.json.gz`, `rust-performance.md`, `optimize.sh` and `optimize.ps1`.
