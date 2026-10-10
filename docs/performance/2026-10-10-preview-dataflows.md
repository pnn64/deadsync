# Preview setup, SRPG text, and download views

Date: 2026-10-10

Branch: `perf/1872-preview-dataflows-20261010`

Starting main: `49d5874b7f53b3d3a087305d39b3f5ec8489b454`

Version: **0.5.1871 → 0.5.1872**, exactly one patch increment in Cargo.toml and all three workspace-version entries in Cargo.lock.

## Changes

1. **Preview model setup:** collect borrowed unique model slots while walking the skin, then prewarm that list. Removes a second field traversal and repeated cache lookups/geometry Arc clones for shared slots. The cache retains the first slot for each stable ID, the same capacity, sealing, and reset statistics. The slot visitor's explicit borrow lifetime permits retaining references without copying slots; its traversal is unchanged.
2. **SRPG summaries:** write stat and skill lines directly into the final strings. Removes two temporary-vector helpers, intermediate line/join buffers, uppercase buffers, and the final trim-and-copy round trip. Qualifier ordering, the five-input grouping rule, zero-gain filtering, duplicates, Unicode uppercase expansion, and trailing-whitespace behavior remain intact.
3. **Downloads overlay:** retain the six visible snapshots, with global completion/retry counts and total length only for longer queues. Fully visible small queues derive status on cache misses. The cache no longer clones or compares names and error strings from every offscreen download. Global status changes still refresh the header/hint, and scrolling, visible changes, fonts, colors, and screen dimensions still invalidate the presentation.

All production changes are outside Song Lua. The pending-branch audit covered **70 refs / 44 distinct unmerged heads**. The only production-file overlap is `chart_window.rs` with `perf/1872-summary-resources-20261009`, whose pending optimization changes `preview_skin_textures`; this pass changes `preview_skin_models`.

## Validation

Fresh release baseline and modified builds used the same combined package graph, `--locked`, and `profile.release.lto=false`. Dependencies were built in the existing cache at `C:/GitHub/deadsync-perf-1872-data-churn-20261009/target/build`, outside the original checkout. Temporary files and evidence are in this worktree's ignored `target/`. All 848 tracked noteskin asset blobs were verified against Git before baseline execution.

| Suite | Passed | Existing failures | Ignored | New passing tests |
| --- | --- | --- | --- | --- |
| deadsync_theme_simply_love-tests | 1269 | 1 | 6 | 6 |
| deadsync_simfile-tests | 203 | 0 | 0 | 0 |
| semantic | 144 | 40 | 77 | 0 |
| actor | 30 | 1 | 0 | 0 |

Individual outcomes and failure diagnostics matched the baseline. Thread IDs were normalized; two unordered diagnostics were normalized only after fresh unchanged-binary reruns proved the variation. The alias diagnostic retained the same three actor lines; the archive diagnostic named one of the same two missing non-local captures. No failures were silently excluded.

The six new regression tests compare native-skin geometry/animation and sealing, SRPG grouping combinations and Unicode/boundary cases, and complete rendered download actor trees plus cache invalidation behavior against frozen originals. Twelve original function/type bodies were mechanically checked against the starting commit.

**1,650,933 full-song comparisons passed, zero failed**, independently on both revisions, using recorded ITGmania captures rather than a live ITGmania run.

| Capture | Archive | Comparisons |
| --- | --- | --- |
| 319\|TECH SOUP\|[lv.P.Clark] Epidermis | 0f2ef3f987449e6218525fba450d1db1201c9f733d4956061d3b36529b922305.tar.zst | 363873 |
| 319\|TECH SOUP\|[lv.P.Clark] Epidermis | af2f887d212d8ab702b772cea9c953500254ce01a6b78b423276fca9cf5a8306.tar.zst | 363873 |
| 280\|MODS\|[MASTER] Sharkmode | b05379b7d12bfb02149e03f166be5d496dc3e7687dcc042f0bf525cc46aa918b.tar.zst | 304425 |
| Warp Zone | ded0f7ff19513e30119e0d6f65b57a661f1683872fc9ffced27737db06c49993.tar.zst | 212220 |
| Let Me Hear That | 2a77063dd2ab8c43bbdbb9e101be7fcf73156302e21e9771c88797218317d327.tar.zst | 205071 |
| 272\|MODS\|[lv.02] Riddle | 7ffacb89fd95ab788758e273ba878c30380c4622406f1bb22d8c51292ca78d83.tar.zst | 201471 |

Existing failures include unavailable external fixture directories, two missing referenced archives, an obsolete archive-clock expectation, and baseline semantic, actor-alignment, and Cyber model-sizing discrepancies. Exact failure output remains in `target/baseline-*.log` and `target/current-*.log`; `target/compatibility-comparison.json` records the comparison.

## Paired benchmarks

Windows x86-64, rustc 1.98.1 (LLVM 22.1.8), release optimization with LTO disabled. Each pair invokes the actual modified function and a frozen original in the same executable. Four fresh processes ran sequentially after our builds/tests finished, each pinned to logical CPU 2 (mask 4) at AboveNormal priority. Each variant gets three warm-up calls, calibration toward 25 ms batches (maximum 1,000,000 calls), and nine alternating timed batches. The table reports the median of four process medians; ranges are the four paired original/current ratios. Fixtures are shared immutable inputs; mutable presentation caches are separate for each variant.

Timing includes output construction/destruction or cache refresh where applicable. Fixture creation, native-skin loading, and formatting benchmark labels are outside timing. A separate scoped System-allocator sample measures allocation calls, reallocations, and requested bytes. Requested bytes are cumulative churn, not peak RSS; zero allocation does not mean zero CPU work. This is path throughput, not a whole-game frame-rate claim. The 1,024-download case is a stress case; small/empty views and unchanged cache hits are controls.

| Case | Original ns/op | Current ns/op | Speedup | Paired range |
| --- | --- | --- | --- | --- |
| progress/box-empty | 1162.99 | 1054.37 | 1.103× | 1.080–1.134× |
| progress/overlay-empty | 974.34 | 902.18 | 1.080× | 1.072–1.087× |
| progress/box-three | 2185.52 | 1239.40 | 1.763× | 1.735–1.772× |
| progress/overlay-three | 2140.52 | 1247.08 | 1.716× | 1.675–1.741× |
| progress/box-five | 2983.95 | 1514.53 | 1.970× | 1.917–1.971× |
| progress/overlay-five | 3167.23 | 1545.72 | 2.049× | 1.993–2.127× |
| progress/box-unicode | 1834.94 | 1172.03 | 1.566× | 1.523–1.608× |
| progress/overlay-unicode | 3154.14 | 1505.66 | 2.095× | 2.000–2.136× |
| downloads/cold-0 | 976.12 | 1005.31 | 0.971× | 0.929–1.014× |
| downloads/stable-0 | 43.89 | 48.63 | 0.902× | 0.872–0.981× |
| downloads/cold-6 | 9116.76 | 8892.10 | 1.025× | 0.994–1.030× |
| downloads/stable-6 | 98.59 | 97.25 | 1.014× | 0.964–1.015× |
| downloads/visible-6 | 9290.98 | 8776.95 | 1.059× | 0.984–1.078× |
| downloads/cold-64 | 14956.99 | 9095.81 | 1.644× | 1.612–1.733× |
| downloads/stable-64 | 480.61 | 150.58 | 3.192× | 3.149–3.272× |
| downloads/visible-64 | 14907.97 | 8778.78 | 1.698× | 1.628–1.717× |
| downloads/offscreen-64 | 15620.04 | 152.25 | 102.598× | 96.631–103.162× |
| downloads/cold-1024 | 174818.54 | 10589.09 | 16.509× | 14.568–19.150× |
| downloads/stable-1024 | 11889.98 | 1099.84 | 10.811× | 10.382–11.807× |
| downloads/visible-1024 | 125328.98 | 11339.26 | 11.053× | 10.588–11.448× |
| downloads/offscreen-1024 | 141320.61 | 1133.43 | 124.684× | 124.185–128.454× |
| models/sprite | 1729.97 | 557.11 | 3.105× | 2.906–3.268× |
| models/cyber | 21063.48 | 17851.10 | 1.180× | 1.160–1.180× |
| models/shared | 8745.80 | 5338.89 | 1.638× | 1.605–1.672× |

| Case | Allocations old → new | Reallocations old → new | Requested bytes old → new |
| --- | --- | --- | --- |
| progress/box-empty | 7 → 5 | 2 → 2 | 233 → 144 |
| progress/overlay-empty | 6 → 5 | 1 → 1 | 163 → 106 |
| progress/box-three | 14 → 5 | 5 → 2 | 370 → 144 |
| progress/overlay-three | 14 → 5 | 5 → 2 | 471 → 234 |
| progress/box-five | 19 → 5 | 8 → 3 | 605 → 280 |
| progress/overlay-five | 19 → 5 | 9 → 3 | 1049 → 490 |
| progress/box-unicode | 12 → 5 | 4 → 2 | 398 → 144 |
| progress/overlay-unicode | 19 → 5 | 8 → 2 | 757 → 234 |
| downloads/cold-0 | 5 → 5 | 0 → 0 | 7009 → 7009 |
| downloads/stable-0 | 0 → 0 | 0 → 0 | 0 → 0 |
| downloads/cold-6 | 37 → 37 | 13 → 13 | 40642 → 40642 |
| downloads/stable-6 | 0 → 0 | 0 → 0 | 0 → 0 |
| downloads/visible-6 | 36 → 36 | 13 → 13 | 40178 → 40178 |
| downloads/cold-64 | 104 → 37 | 13 → 13 | 46868 → 40642 |
| downloads/stable-64 | 0 → 0 | 0 → 0 | 0 → 0 |
| downloads/visible-64 | 103 → 36 | 13 → 13 | 46404 → 40178 |
| downloads/offscreen-64 | 103 → 0 | 13 → 0 | 46404 → 0 |
| downloads/cold-1024 | 1201 → 37 | 13 → 13 | 150670 → 40642 |
| downloads/stable-1024 | 0 → 0 | 0 → 0 | 0 → 0 |
| downloads/visible-1024 | 1200 → 36 | 13 → 13 | 150206 → 40178 |
| downloads/offscreen-1024 | 1200 → 0 | 13 → 0 | 150206 → 0 |
| models/sprite | 1 → 1 | 0 → 0 | 32 → 32 |
| models/cyber | 23 → 23 | 3 → 3 | 167616 → 167616 |
| models/shared | 9 → 9 | 1 → 1 | 56984 → 56984 |

An initial four-run implementation still computed aggregate status on every small-queue cache hit: unchanged empty queues measured 46.21 → 49.78 ns and unchanged six-row queues 100.69 → 107.29 ns (medians). The final implementation defers that work until a cache miss when all rows are visible, while retaining aggregate comparisons for longer queues. The regression test also checks a long queue shrinking to six identical visible rows. Initial logs and their summary remain in `target/initial-final-*.log` and `target/initial-benchmark-summary.json`; the table above uses all four fresh runs of the final code with the same benchmark protocol.

All four final runs are retained in `target/final-{1..4}.log`; `target/benchmark-summary.json` contains per-process values. Allocation counts were identical across the four runs.

## Control results and limits

The six-row unchanged-cache control measured 98.59 to 97.25 ns (1.014x), with a paired range crossing parity. Empty-view cache hits retain a small cost: 43.89 to 48.63 ns, approximately 4.7 ns or 10.8% slower, with zero allocations in both implementations. Empty cold construction measured 976.12 to 1005.31 ns; its paired range crossed parity. These controls limit the speedup claim: the three targeted paths improve, but this is not a claim that every input is faster. There were no new test failures or native-capture differences.

## Reproduction

In this worktree, use a target directory outside the user's original checkout:

```powershell
$env:CARGO_TARGET_DIR = "$PWD/target/reproduce"
cargo test --release --locked --config profile.release.lto=false -p deadsync -p deadsync-simfile -p deadsync-theme-simply-love --lib --test song_lua_itgmania_semantic_parity --test itgmania_actor_conformance --test full_song_lua --no-run --message-format=json
cargo test --release --locked --config profile.release.lto=false -p deadsync -p deadsync-simfile -p deadsync-theme-simply-love --lib benchmark_preview_dataflows -- --ignored --nocapture --test-threads=1
```

The first command's compiler-artifact JSON identifies the test executables. Run the theme and simfile libraries and the semantic/actor executables with `--test-threads=1`. Invoke the `full_song_lua` executable once per archive filename listed above. The recorded runs used `target/run-compat.py`, `target/run-bench.py`, `target/compare-results.py`, and `target/analyze-bench.py`; those local orchestration scripts are ignored evidence, while the regression fixtures, frozen originals, and benchmark helper are committed.

The original checkout was not edited. No merge or push was performed. `deadsync-song.json.gz`, `rust-performance.md`, `optimize.sh`, and `optimize.ps1` are excluded from the commit.

At the final original-checkout audit, main had independently advanced to `33b5a318211e2b9ab11a02976c97facc0bbbbe7d` (`test(song-lua): validate fixture 35 with native timing`). The original checkout had an edited `tests/song_lua_itgmania_semantic_parity/multitap.rs` and an untracked `tests/fixtures/itgmania-actors/edgar-multitap-frames.json`; this pass did not edit either. The perf branch stays based on the main commit recorded above. The pending perf refs were also rechecked before committing, with no changes since the initial audit.
