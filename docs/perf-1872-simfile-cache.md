# Simfile cache performance pass, 0.5.1872

Branch: `perf/1872-data-churn-20261009`. Starting committed main:
`9710f833e6e582817b28532e6217152186400fa0` (0.5.1871).
`Cargo.toml` and all three workspace-version entries in `Cargo.lock` advance
exactly once to 0.5.1872. Work is isolated in its own worktree; no merge or push.
The 28 distinct existing unmerged perf heads had no simfile-crate overlap.

## Changes

1. After the parse fallback writes its cache, move each requested chart's owned
   buffers into the existing gameplay payload builder. Keep the public borrowed
   API, exact invalid-index errors, request order, and independent duplicate
   outputs. Reuse the existing duplicate-request collector.
2. Encode computed chart metadata one chart at a time in the existing Vec wire
   format. Remove the intermediate metadata vector and retention of every
   chart's temporary measure-seconds vector. Encoded bytes are identical.
3. Short-circuit asset-path validation across groups after the first failure.
   Remove eager temporary booleans and subsequent filesystem queries for an
   already unusable cache. Keep the existing query order on successful checks,
   optional-path rules, trimming, and non-file background targets.

All production changes are in `crates/deadsync-simfile/src/cache.rs`.
Song Lua code and cache layout/version are unchanged.

## Allocation measurements

Thread-local allocator accounting around the production calls, with inputs
prepared outside the scope. Counts are requested allocation bytes, not RSS or
allocator overhead. Header output storage was warmed and reused.

| Workload | Original | Current |
| --- | ---: | ---: |
| Materialize two distinct 16,384-row charts: allocations | 49 | 23 |
| Same: requested bytes | 1,739,712 | 1,443,408 |
| Encode 64 charts, 512 measures each: allocations | 705 | 704 |
| Same: requested bytes | 172,032 | 142,336 |
| Same: peak temporary requested bytes | 160,944 | 2,224 |

Gameplay requests 17.0% fewer bytes and makes 53.1% fewer allocations. Header
encoding requests 17.3% fewer bytes and reduces measured peak temporary memory
by 98.6%. Gameplay peak counters are deliberately not reported: it frees some
input allocations that predate the measurement scope.

## Paired timing results

Windows/MSVC, Intel Xeon E5-2696 v4 @ 2.20 GHz, rustc 1.98.1
(48a229cea 2026-09-01), release optimization, LTO disabled, locked dependencies.
Four fixed complete runs; each run alternates original/current order for ten
samples, discards the warmup, and reports the median of nine samples. The table
uses the median original and current times across those four runs. The range
shows all four paired speed ratios. Owned builds and native tests were idle
throughout timing; the machine was not exclusively reserved. Both variants use
the same instrumented System allocator; allocation tracking is disabled during
timed samples.

Original code is frozen from the starting commit in `cache_perf_original.rs`;
its seven blocks and the unchanged shared production helpers were audited
against that commit. Fixture construction is outside timing. Gameplay uses
three-chart songs, requests one chart, two distinct charts, or the same chart
twice. Both variants include destruction of the owned parsed song and output
charts, matching the parse fallback's ownership lifecycle. Sixteen prepared
inputs are measured per batch. Initial exploratory measurements excluded source
cleanup and therefore charged transferred-input cleanup only to the new path;
those measurements were superseded by this corrected scope before these four
runs. No production change was made to tune those results.

Header labels are chart count / parsed rows per chart / measures per chart.
Path fixtures use a real local file with warmed filesystem caches. Each count
populates both background layers, foreground, both Lua-path lists, and chart
music. `None` means valid; `Some(0)` is the first background target missing;
`Some(4)` is the first foreground missing; `Some(8)` is a missing banner;
`Some(11)` is missing song music. These are function-level measurements, not
whole-game frame-rate or cold-disk claims.

| Case | Original us/op | Current us/op | Speed ratio | Four-run range |
| --- | ---: | ---: | ---: | ---: |
| `gameplay/0/single` | 3.741 | 2.844 | 1.315x | 1.289-1.337x |
| `gameplay/0/two` | 6.037 | 3.928 | 1.537x | 1.391-1.655x |
| `gameplay/0/duplicate` | 5.909 | 4.059 | 1.456x | 1.433-1.486x |
| `gameplay/128/single` | 11.028 | 6.928 | 1.592x | 1.537-1.617x |
| `gameplay/128/two` | 15.231 | 9.838 | 1.548x | 1.371-1.624x |
| `gameplay/128/duplicate` | 12.719 | 10.025 | 1.269x | 1.247-1.325x |
| `gameplay/16384/single` | 660.422 | 613.897 | 1.076x | 1.037-1.149x |
| `gameplay/16384/two` | 1216.116 | 1039.441 | 1.170x | 1.147-1.211x |
| `gameplay/16384/duplicate` | 1110.650 | 957.350 | 1.160x | 1.129-1.192x |
| `gameplay/65536/single` | 1885.237 | 1792.125 | 1.052x | 1.015-1.109x |
| `gameplay/65536/two` | 3090.178 | 2870.153 | 1.077x | 1.059-1.111x |
| `gameplay/65536/duplicate` | 2917.894 | 2811.762 | 1.038x | 1.017-1.046x |
| `header/0/0/0` | 0.269 | 0.262 | 1.028x | 0.885-1.058x |
| `header/1/0/0` | 1.928 | 1.803 | 1.069x | 1.063-1.143x |
| `header/1/128/32` | 7.553 | 7.475 | 1.010x | 0.986-1.016x |
| `header/8/128/512` | 227.238 | 224.531 | 1.012x | 1.009-1.053x |
| `header/64/128/512` | 1871.831 | 1837.406 | 1.019x | 1.001-1.030x |
| `paths/1/None` | 566.312 | 559.800 | 1.012x | 0.996-1.020x |
| `paths/1/Some(0)` | 304.550 | 16.010 | 19.022x | 18.826-19.125x |
| `paths/1/Some(4)` | 448.594 | 210.729 | 2.129x | 2.026-2.185x |
| `paths/1/Some(8)` | 389.188 | 386.050 | 1.008x | 0.989-1.019x |
| `paths/1/Some(11)` | 534.038 | 537.350 | 0.994x | 0.984-1.004x |
| `paths/8/None` | 3139.525 | 3112.588 | 1.009x | 0.991-1.016x |
| `paths/8/Some(0)` | 1594.588 | 15.659 | 101.830x | 99.583-103.950x |
| `paths/8/Some(4)` | 2675.825 | 1480.700 | 1.807x | 1.800-1.874x |
| `paths/8/Some(8)` | 2935.838 | 3023.875 | 0.971x | 0.957-0.986x |
| `paths/8/Some(11)` | 3137.375 | 3145.738 | 0.997x | 0.989-1.007x |
| `paths/64/None` | 23888.725 | 23680.513 | 1.009x | 0.988-1.010x |
| `paths/64/Some(0)` | 12026.825 | 16.178 | 743.387x | 734.489-757.735x |
| `paths/64/Some(4)` | 20943.513 | 11837.163 | 1.769x | 1.737-1.787x |
| `paths/64/Some(8)` | 23446.325 | 23571.625 | 0.995x | 0.987-1.003x |
| `paths/64/Some(11)` | 23995.287 | 24084.050 | 0.996x | 0.979-1.019x |

All gameplay aggregate ratios improve (1.038-1.592x), and every individual
run of each gameplay case improves. Header CPU time is approximately neutral
to modestly faster; its principal benefit is memory. Early invalid path cases
improve 1.769-743.387x. Valid-cache aggregates are 1.009-1.012x. Late-failure
controls range from 0.971x to 1.008x: the eight-entry missing-banner control
measured 2.9% slower. Those cases execute the same filesystem queries; no
performance improvement is claimed for them. The empty-header control also
has one slower individual run (0.885x), despite its 1.028x aggregate. No samples
or cases were dropped from the four final runs.

## Regression and native compatibility validation

The untouched simfile baseline passed 203 tests. The changed crate passed 211
tests with zero failures, preserving all baseline outcomes; one explicit
benchmark test is ignored by default and passed all four timed runs. Eight new
tests cover requested order, duplicates, independent mutation, transferred
buffer identity, offset/timing queries, exact errors and no mutation on an
invalid request, byte-identical headers through 251 charts, all twelve path
locations, optional/non-file targets, allocation reductions, and an end-to-end
parse -> cache write -> gameplay move -> cache reload comparison.

Fresh baseline and changed Song Lua/ITGmania harness runs have identical
individual outcomes and failure diagnostics:

| Suite | Passed | Existing failures | Ignored |
| --- | ---: | ---: | ---: |
| semantic | 142 | 38 | 77 |
| actor | 30 | 1 | 0 |

All 39 pre-existing failure diagnostics match after normalizing process thread
IDs and only the known unordered unmatched-Sprite diagnostic lines. There are
no new behavioral regressions in these runs; the native suites are not fully
green at the starting commit.

The same six full-song archives pass before and after, with 1,650,933 successful
comparisons and zero failures. No archive panic was accepted.

| Archive | Comparisons passed | Selector |
| --- | ---: | --- |
| 319 / TECH SOUP / [lv.P.Clark] Epidermis | 363,873 | `0f2ef3f987449e6218525fba450d1db1201c9f733d4956061d3b36529b922305.tar.zst` |
| 319 / TECH SOUP / [lv.P.Clark] Epidermis | 363,873 | `af2f887d212d8ab702b772cea9c953500254ce01a6b78b423276fca9cf5a8306.tar.zst` |
| 280 / MODS / [MASTER] Sharkmode | 304,425 | `b05379b7d12bfb02149e03f166be5d496dc3e7687dcc042f0bf525cc46aa918b.tar.zst` |
| Warp Zone | 212,220 | `b38698ececd60bdf311a15de205f43d0f3466eac6d41296b9abf469485ebab4d.tar.zst` |
| Let Me Hear That | 205,071 | `0229b74d092e8f04278d64b7c994877a0a3bcc52e9d7ddb00d6a98f1f5244cd0.tar.zst` |
| 272 / MODS / [lv.02] Riddle | 201,471 | `920eb361ce688e70b4364258337b30c9e4bf7d337f2c83a30e59215eb838735d.tar.zst` |

Reproduce each archive with `cargo test --release --locked --config
profile.release.lto=false --test full_song_lua -- <selector>` (one command line).
The checked-in fallback receptor fixture remains byte-identical to the base
(SHA-256 `96623726284f5ae0c5b12e05e0ae841eae40e100341a20042db68c9b80b52c74`).


The first changed native build failed when the shared target cache disappeared
during compilation. Validation was retried with a private build cache in this
worktree, seeded by read-only copies of existing artifacts. Source and dependency
versions were unchanged; no original-checkout files were modified.

## Reproduce

```powershell
cargo test --release --locked --config profile.release.lto=false -p deadsync-simfile --lib -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false -p deadsync-simfile --lib benchmark_cache_data_churn -- --ignored --nocapture --test-threads=1
cargo test --release --locked --config profile.release.lto=false -p deadsync-simfile --lib gameplay_and_header_allocate_less -- --nocapture --test-threads=1
cargo test --release --locked --config profile.release.lto=false --test song_lua_itgmania_semantic_parity --test itgmania_actor_conformance -- --test-threads=1
```

Run the benchmark command four times without concurrent owned builds/tests.
Native baseline/current logs, the six full-song selectors, raw benchmark logs,
allocation results, source audit, and comparison JSON are retained under this
worktree's ignored `target/` directory. The excluded task files are not committed.
