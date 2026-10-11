# Runtime stream analysis performance pass

Branch: `perf/1872-runtime-streams-20261009`. Starting committed main: `65351713dc9b0e7f92ce979c135a81946a5bb9c4`.
Patch version: **0.5.1871 -> 0.5.1872**, with Cargo.toml and Cargo.lock updated.

## Changes

1. Advance fixed-width note rows directly, avoiding general line splitting and trimming on the common path. The guarded scanner preserves short lines, LF/CRLF endings, whitespace, comments, delimiters and arbitrary bytes. Remove the end-of-chart flag by returning after the semicolon callback.
2. For run-progress queries, stop testing rows for steps once the measure reaches the requested threshold. Run membership needs only that comparison, so exact counts beyond it do no useful work. Exact density callers still count every row; byte densities retain their existing saturation at 32.
3. Reserve counter-segment storage only when the first segment is emitted. Charts with no qualifying stream return an empty vector without an allocation. Nonempty outputs retain the original initial reservation and growth behavior.

Production changes are confined to `crates/deadsync-rules/src/stream.rs`. These paths prepare gameplay stream counters and compute failed-stream progress for score display. No Song Lua production code changed. The initial audit covered 63 unmerged perf refs / 37 distinct heads; the final audit covered 63 refs / 37 distinct heads, with no pending diff in this production file. This follows `rust-performance.md` by replacing general line splitting/trimming and removing unnecessary exact counts and empty-result allocations before optimizing remaining work.

## Behavioral validation

- **1152 unit tests passed** across gameplay, chart, rules, core and simfile. The ignored benchmark entry was run explicitly in four independent processes.
- Row-scan tests compare against frozen starting functions across 11,520 deterministic random buffer/lane combinations, plus explicit malformed short lines, internal newlines, CRLF, whitespace, comments, NUL/non-UTF-8 bytes and missing terminators. Golden tests cover exact and saturated counts for 4/5/8/10 lanes; differential tests also cover fallback lane values.
- Progress tests cover empty/sparse/dense/mixed charts, LF/CRLF, threshold zero through usize::MAX, every requested measure through two beyond the chart, run positions/lengths and semicolon early termination.
- Segment tests cover lengths 0, 1, 2, 3, 16, 64, 257 and 2048, eight density patterns and seven thresholds. They assert exact segment equality, leading/internal/trailing breaks, counter-only output integration, no allocation increase and zero allocations for empty output. Short nonempty lists retain the original allocation size.
- Fresh baseline/current Song Lua harness runs replayed six recorded native ITGmania captures. All **1,650,933 comparisons passed** on both runs.
- Semantic parity: **143 passed / 38 failed / 77 ignored**. Actor conformance: **30 passed / 1 failed**. All baseline failure names and diagnostics match after normalizing panic thread IDs and the known unordered missing-actor diagnostic lines.

Native validation replays existing captures; no new live ITGmania capture or exhaustive native coverage of these stream functions is claimed. Differential and golden tests directly exercise the changed paths.

| Native capture | Comparisons | Baseline / current |
| --- | ---: | --- |
| 319\|TECH SOUP\|[lv.P.Clark] Epidermis (`0f2ef3f98744`) | 363873 | pass / pass |
| 319\|TECH SOUP\|[lv.P.Clark] Epidermis (`af2f887d212d`) | 363873 | pass / pass |
| 280\|MODS\|[MASTER] Sharkmode (`b05379b7d12b`) | 304425 | pass / pass |
| Warp Zone (`b38698ececd6`) | 212220 | pass / pass |
| Let Me Hear That (`0229b74d092e`) | 205071 | pass / pass |
| 272\|MODS\|[lv.02] Riddle (`920eb361ce68`) | 201471 | pass / pass |

## Paired benchmarks

Release opt-level=3, LTO disabled for both variants. Rustc 1.98.1 / LLVM 22.1.8, x86_64-pc-windows-msvc; Xeon E5-2696 v4, 22 cores / 44 logical CPUs, Ultimate Performance power plan. Each benchmark child is pinned to logical CPU 2 with AboveNormal priority. Other processes are left untouched.

The same binary contains frozen starting function bodies and current implementations, using unchanged common row/run helpers. Black-boxed function pointers and inputs prevent folding or inlining into the timing loop. Fixtures are constructed outside timing. Density and segment timings include output construction and destruction; progress returns a small value without allocating. Nonempty note fixtures normally have 256 measures, with 16 rows per measure for sparse/stream cases and 192 for dense cases. The changed lane column rotates each row. CRLF and decorated/irregular/empty controls are included.

Each process alternates variant order over 10 paired rounds, discards the first round, and reports medians of the remaining nine. Warmup calibrates toward five milliseconds per batch, capped at five million iterations. Tables report the median of four independent process medians and the full range of paired process ratios. Ratios above 1 indicate higher throughput. These are function microbenchmarks, not whole-game frame-rate measurements.

| Case | Starting ns/op | Current ns/op | Throughput | 4-run range |
| --- | ---: | ---: | ---: | --- |
| density-4-sparse | 31930.29 | 22870.16 | 1.396x | 1.260-1.548 |
| progress-4-sparse | 17245.88 | 12756.36 | 1.352x | 1.344-1.417 |
| density-4-stream | 31951.67 | 19842.37 | 1.610x | 1.607-1.700 |
| progress-4-stream | 38148.25 | 24169.44 | 1.578x | 1.539-1.641 |
| density-4-dense | 276473.88 | 194230.63 | 1.423x | 1.363-1.524 |
| progress-4-dense | 377962.50 | 238070.82 | 1.588x | 1.532-1.658 |
| density-4-crlf | 33761.38 | 21059.22 | 1.603x | 1.597-1.659 |
| density-5-sparse | 35968.39 | 23658.08 | 1.520x | 1.396-1.584 |
| progress-5-sparse | 16970.95 | 13886.85 | 1.222x | 1.185-1.357 |
| density-5-stream | 33466.00 | 19373.84 | 1.727x | 1.599-1.805 |
| progress-5-stream | 32634.74 | 24044.57 | 1.357x | 1.336-1.380 |
| density-5-dense | 301885.60 | 202969.44 | 1.487x | 1.406-1.513 |
| progress-5-dense | 335370.58 | 225534.61 | 1.487x | 1.473-1.639 |
| density-5-crlf | 37005.46 | 21231.32 | 1.743x | 1.675-1.762 |
| density-8-sparse | 45363.07 | 28037.92 | 1.618x | 1.584-1.693 |
| progress-8-sparse | 25204.13 | 18060.21 | 1.396x | 1.357-1.460 |
| density-8-stream | 41571.28 | 24984.67 | 1.664x | 1.583-1.747 |
| progress-8-stream | 46955.00 | 27214.07 | 1.725x | 1.602-1.744 |
| density-8-dense | 379487.50 | 236931.99 | 1.602x | 1.533-1.628 |
| progress-8-dense | 538606.82 | 264666.11 | 2.035x | 1.986-2.086 |
| density-8-crlf | 43542.01 | 25915.82 | 1.680x | 1.502-1.682 |
| density-10-sparse | 53065.59 | 33103.63 | 1.603x | 1.460-1.605 |
| progress-10-sparse | 27786.97 | 20246.63 | 1.372x | 1.295-1.418 |
| density-10-stream | 47111.32 | 29249.44 | 1.611x | 1.604-1.654 |
| progress-10-stream | 49322.20 | 31861.94 | 1.548x | 1.465-1.588 |
| density-10-dense | 445925.32 | 275571.26 | 1.618x | 1.569-1.703 |
| progress-10-dense | 537382.33 | 286630.41 | 1.875x | 1.856-1.909 |
| density-10-crlf | 52308.51 | 31902.17 | 1.640x | 1.615-1.782 |
| density-control-empty | 75.05 | 72.84 | 1.030x | 0.910-1.071 |
| density-control-ragged | 91.30 | 95.61 | 0.955x | 0.822-1.002 |
| density-control-decorated | 50127.15 | 41260.29 | 1.215x | 1.167-1.361 |
| progress-control-empty | 7.19 | 6.12 | 1.174x | 1.042-1.241 |
| progress-control-outside | 37530.53 | 23275.49 | 1.612x | 1.452-1.649 |
| progress-control-crlf | 431065.00 | 264363.04 | 1.631x | 1.542-1.726 |
| segments-empty | 11.80 | 5.56 | 2.121x | 1.955-2.278 |
| segments-none | 240.89 | 155.59 | 1.548x | 1.316-1.548 |
| segments-below | 231.24 | 147.63 | 1.566x | 1.469-1.661 |
| segments-all | 383.73 | 336.63 | 1.140x | 0.987-1.210 |
| segments-alternating | 543.71 | 578.12 | 0.940x | 0.821-1.045 |
| segments-mixed | 395.65 | 392.79 | 1.007x | 0.922-1.072 |
| segments-delayed | 283.18 | 253.98 | 1.115x | 1.038-1.190 |
| segments-short | 74.71 | 76.69 | 0.974x | 0.946-1.034 |

### Threshold cap in isolation

These eight diagnostic cases compare the **uncapped current scanner** with the final capped progress query, isolating the second optimization. Their reference is not the starting implementation. Both variants return the same value. The starting-to-final progress measurements appear above.

| Case | Uncapped ns/op | Capped ns/op | Throughput | 4-run range |
| --- | ---: | ---: | ---: | --- |
| isolate-cap-4-stream | 25431.47 | 23770.22 | 1.070x | 1.024-1.073 |
| isolate-cap-4-dense | 243984.79 | 237920.00 | 1.025x | 0.927-1.068 |
| isolate-cap-5-stream | 25085.60 | 24381.69 | 1.029x | 1.026-1.129 |
| isolate-cap-5-dense | 276780.00 | 221773.38 | 1.248x | 1.160-1.309 |
| isolate-cap-8-stream | 28203.87 | 30152.21 | 0.935x | 0.928-0.992 |
| isolate-cap-8-dense | 331869.21 | 262282.20 | 1.265x | 1.217-1.308 |
| isolate-cap-10-stream | 33023.38 | 30833.49 | 1.071x | 1.039-1.097 |
| isolate-cap-10-dense | 405051.43 | 304414.71 | 1.331x | 1.225-1.417 |

Canonical LF density cases improved 1.40-1.73x and CRLF cases improved 1.60-1.74x. Starting-to-final progress queries improved 1.22-2.04x across the nonempty lane fixtures. The cap in isolation improved dense 5/8/10-lane queries 1.25-1.33x; the four-lane dense median was 1.03x with a paired range crossing 1.0.

The no-stream and below-threshold segment fixtures improved 1.55-1.57x and removed one 512-byte allocation per call. Nonempty output allocations and growth remained unchanged.

Some controls had lower pooled throughput: ragged density input took 4.7% more time (4.31 ns), alternating segments 6.3% (34.41 ns), and the short segment list 2.7% (1.98 ns). Each of those paired run ranges crossed 1.0. The isolated cap on the 8-lane, 16-row fixture cost 6.9% more time in the pooled medians and was slower in all four runs, while its starting-to-final progress query remained 1.73x faster. These results do not claim a speedup on every input. No behavioral regressions were detected.

## Allocation measurements

The test-only System allocator wrapper records per-thread requests outside timed batches. Its disabled TLS check remains in both timed variants. Fixture allocations are excluded; returned values are released after recording. Requested bytes sum allocation/reallocation requests; peak bytes are live requested bytes within the operation and exclude allocator metadata. All four runs produced identical counts. No production allocator or dependency changed.

| Case | Old alloc / realloc | New alloc / realloc | Old requested bytes | New requested bytes | Old peak bytes | New peak bytes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| density-4-sparse | 1 / 0 | 1 / 0 | 263 | 263 | 263 | 263 |
| progress-4-sparse | 0 / 0 | 0 / 0 | 0 | 0 | 0 | 0 |
| density-4-stream | 1 / 0 | 1 / 0 | 263 | 263 | 263 | 263 |
| progress-4-stream | 0 / 0 | 0 / 0 | 0 | 0 | 0 | 0 |
| density-4-dense | 1 / 0 | 1 / 0 | 3079 | 3079 | 3079 | 3079 |
| progress-4-dense | 0 / 0 | 0 / 0 | 0 | 0 | 0 | 0 |
| density-4-crlf | 1 / 0 | 1 / 0 | 314 | 314 | 314 | 314 |
| density-5-sparse | 1 / 0 | 1 / 0 | 262 | 262 | 262 | 262 |
| progress-5-sparse | 0 / 0 | 0 / 0 | 0 | 0 | 0 | 0 |
| density-5-stream | 1 / 0 | 1 / 0 | 262 | 262 | 262 | 262 |
| progress-5-stream | 0 / 0 | 0 / 0 | 0 | 0 | 0 | 0 |
| density-5-dense | 1 / 0 | 1 / 0 | 3078 | 3078 | 3078 | 3078 |
| progress-5-dense | 0 / 0 | 0 / 0 | 0 | 0 | 0 | 0 |
| density-5-crlf | 1 / 0 | 1 / 0 | 305 | 305 | 305 | 305 |
| density-8-sparse | 1 / 0 | 1 / 0 | 260 | 260 | 260 | 260 |
| progress-8-sparse | 0 / 0 | 0 / 0 | 0 | 0 | 0 | 0 |
| density-8-stream | 1 / 0 | 1 / 0 | 260 | 260 | 260 | 260 |
| progress-8-stream | 0 / 0 | 0 / 0 | 0 | 0 | 0 | 0 |
| density-8-dense | 1 / 0 | 1 / 0 | 3076 | 3076 | 3076 | 3076 |
| progress-8-dense | 0 / 0 | 0 / 0 | 0 | 0 | 0 | 0 |
| density-8-crlf | 1 / 0 | 1 / 0 | 289 | 289 | 289 | 289 |
| density-10-sparse | 1 / 0 | 1 / 0 | 259 | 259 | 259 | 259 |
| progress-10-sparse | 0 / 0 | 0 / 0 | 0 | 0 | 0 | 0 |
| density-10-stream | 1 / 0 | 1 / 0 | 259 | 259 | 259 | 259 |
| progress-10-stream | 0 / 0 | 0 / 0 | 0 | 0 | 0 | 0 |
| density-10-dense | 1 / 0 | 1 / 0 | 3075 | 3075 | 3075 | 3075 |
| progress-10-dense | 0 / 0 | 0 / 0 | 0 | 0 | 0 | 0 |
| density-10-crlf | 1 / 0 | 1 / 0 | 283 | 283 | 283 | 283 |
| density-control-empty | 1 / 0 | 1 / 0 | 1 | 1 | 1 | 1 |
| density-control-ragged | 1 / 0 | 1 / 0 | 18 | 18 | 18 | 18 |
| density-control-decorated | 1 / 0 | 1 / 0 | 477 | 477 | 477 | 477 |
| progress-control-empty | 0 / 0 | 0 / 0 | 0 | 0 | 0 | 0 |
| progress-control-outside | 0 / 0 | 0 / 0 | 0 | 0 | 0 | 0 |
| progress-control-crlf | 0 / 0 | 0 / 0 | 0 | 0 | 0 | 0 |
| segments-empty | 0 / 0 | 0 / 0 | 0 | 0 | 0 | 0 |
| segments-none | 1 / 0 | 0 / 0 | 512 | 0 | 512 | 0 |
| segments-below | 1 / 0 | 0 / 0 | 512 | 0 | 512 | 0 |
| segments-all | 1 / 0 | 1 / 0 | 512 | 512 | 512 | 512 |
| segments-alternating | 1 / 1 | 1 / 1 | 1536 | 1536 | 1024 | 1024 |
| segments-mixed | 1 / 0 | 1 / 0 | 512 | 512 | 512 | 512 |
| segments-delayed | 1 / 0 | 1 / 0 | 512 | 512 | 512 | 512 |
| segments-short | 1 / 0 | 1 / 0 | 32 | 32 | 32 | 32 |

## Reproduction and scope

```powershell
cargo test --release --locked --config profile.release.lto=false -p deadsync -p deadsync-simfile -p deadsync-gameplay -p deadsync-chart -p deadsync-rules -p deadsync-core --lib -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false -p deadsync-rules --lib benchmark_stream_paths -- --ignored --nocapture --test-threads=1
cargo test --release --locked --config profile.release.lto=false -p deadsync --test song_lua_itgmania_semantic_parity -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false -p deadsync --test itgmania_actor_conformance -- --test-threads=1
# Run once per archive listed above, passing its unique archive prefix:
cargo test --release --locked --config profile.release.lto=false -p deadsync --test full_song_lua -- 0f2ef3f987449
```

The focused compatibility commands retain the baseline failures listed above. To reproduce the timing protocol, build once, then invoke the generated rules test executable in four independent processes, pinned to logical CPU 2 at AboveNormal priority. Source fixtures, frozen original functions, regression cases and the benchmark are committed. Raw baseline/current logs, four final timing logs, allocation data and machine-readable comparisons remain under this worktree's ignored `target/` directory.

The baseline and current builds reused the ignored Cargo cache at `C:/GitHub/deadsync-perf-1872-data-churn-20261009/target/build`; all edited sources, temporary files and reports live in the new worktree. The original checkout was not written to. No merge or push was performed. `deadsync-song.json.gz`, `rust-performance.md`, `optimize.sh` and `optimize.ps1` are excluded from the commit.
