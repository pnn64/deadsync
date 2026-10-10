# Direct scans for wheel meters, option anchors and heat graph ranges

Pass: 2026-10-10. Branch: `perf/1872-direct-scans-20261010`.
Base: `4e59125883b7cf5331779a0a95627fea47930fd9`, latest committed main at worktree creation.
Version: **0.5.1871 -> 0.5.1872**, exactly one patch increment in Cargo.toml and all three workspace-version Cargo.lock entries.

## Changes

1. Wheel meter lookup selects edit winners directly instead of building and sorting a temporary edit-index Vec and immediately reducing it to one chart per meter. Standard difficulty priority is unchanged. Edits override standard charts at the same meter; edit winners retain the largest step count, then largest Unicode lowercase description, with the last source chart winning exact ties. Only the final distinct-meter SmallVec is sorted. Common standard-only songs now avoid the temporary heap allocation altogether.
2. Parent-row anchor lookup counts visible rows while finding the parent, removing the display-index lookup followed by a second prefix traversal. Missing or hidden parent rows return immediately. It preserves first-match display-order behavior, missing row entries, child-to-parent mapping and the original usize-to-i32 conversion.
3. Heat graph range calculation finds minimum and maximum in one traversal. Percentile selection and constant-value expansion stay unchanged. The two accumulators see values in their original order, preserving NaN, infinity and signed-zero behavior.

All three production changes are in deadsync-theme-simply-love and outside Song Lua. No production dependency, cache, unsafe code or new persistent state was added.

## Paired release measurements

Four fresh processes each ran three warmups, calibration toward 25 ms capped at 4,096 operations, and nine measured batches in alternating variant order. The table shows medians of the four process medians and ranges of paired throughput ratios. The original bodies are frozen from the pinned main commit and checked against its Git blobs; both variants run in the same executable.

Inputs are prepared outside timing and allocation counting, then passed through black_box. Result consumption/destruction is inside. Input-batch Vec traversal and destruction are common to both variants. Allocation requests are counted with a test-only scoped thread-local System allocator wrapper; both timed variants use that wrapper with counting disabled. The production allocator is unchanged.

Windows x86-64 MSVC, Intel Xeon E5-2696 v4 @ 2.20 GHz (44 logical processors), rustc 1.98.1 / LLVM 22.1.8. Release optimization with `profile.release.lto=false`. Processes use logical CPU 2 and AboveNormal priority. Formal measurements ran after baseline compatibility work and before final compatibility work, without this pass's build or native harness running concurrently. These are function throughput measurements, not frame-rate or CPU-cycle claims.

| Operation | Original ns/op | Current ns/op | Throughput | Four-process range | Allocations | Reallocations | Requested bytes |
|---|---:|---:|---:|---:|---:|---:|---:|
| meter/standard_4 | 226.78 | 117.91 | 1.931x | 1.885-1.991x | 1 -> 0 | 0 -> 0 | 32 -> 0 |
| meter/edits_4 | 277.76 | 120.52 | 2.329x | 2.276-2.412x | 1 -> 0 | 0 -> 0 | 32 -> 0 |
| meter/duplicates_64 | 5,366.76 | 1,982.74 | 2.714x | 2.685-2.778x | 1 -> 0 | 0 -> 0 | 512 -> 0 |
| meter/unique_64 | 4,514.35 | 3,814.52 | 1.198x | 1.179-1.208x | 2 -> 1 | 3 -> 3 | 2432 -> 1920 |
| anchor/visible_8 | 36.27 | 34.68 | 1.005x | 0.936-1.142x | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| anchor/visible_64 | 160.43 | 137.75 | 1.154x | 1.145-1.393x | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| anchor/visible_118 | 347.99 | 259.94 | 1.336x | 1.177-1.392x | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| anchor/hidden_64 | 58.83 | 14.04 | 4.264x | 3.175-4.338x | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| graph/range_32 | 62.66 | 44.98 | 1.392x | 1.331-1.439x | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| graph/range_4096 | 6,774.34 | 4,775.56 | 1.388x | 1.366-1.465x | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| graph/range_65536 | 107,619.46 | 76,692.62 | 1.419x | 1.356-1.441x | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| graph/constant_4096 | 6,550.43 | 4,769.81 | 1.379x | 1.347-1.428x | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| graph/percentile_control | 7,210.52 | 7,122.19 | 1.013x | 0.987-1.042x | 1 -> 1 | 0 -> 0 | 8192 -> 8192 |

Meter fixtures use four standard charts, four edits, or 64 edits with either four repeated meters or distinct meters. Descriptions mix ASCII and Unicode. They compare the complete meter-index operation, including output-buffer growth and destruction. Allocation counts are identical across all four processes. Requested bytes sum allocation and reallocation requests; they are not process RSS.

Anchor fixtures place a parent at the end of short, medium and full row orders with all flags enabled. The hidden-parent fixture places DataVisualizations late with flags disabled. Both implementations allocate zero bytes. These synthetic layouts measure the lookup itself, not complete option-screen rendering.

The eight-row anchor case is roughly neutral (1.005x median paired throughput, 0.936-1.142x across processes); no speedup is claimed for it. The fused helper has an inline hint. Longer visible-parent and hidden-parent cases improve in every final process.

Graph fixtures use deterministic finite samples with 32, 4,096 or 65,536 values, a constant matrix, and a percentile-path control. Range-only paths allocate nothing. Percentile selection retains its original working buffer; no speedup is claimed for that unchanged control. Floating-point outputs are compared by bits.

## Behavioral validation

| Suite | Passed | Failed, unchanged from baseline | Ignored |
|---|---:|---:|---:|
| deadsync_shell-tests | 355 | 17 | 6 |
| semantic | 153 | 42 | 77 |
| actor | 30 | 1 | 0 |
| deadsync_simfile-tests | 203 | 0 | 0 |
| deadsync_theme_simply_love-tests | 1270 | 1 | 6 |
| deadsync_profile-tests | 228 | 0 | 0 |

Seven new regression tests pass. All three ignored benchmark tests pass in each of four explicit invocations. Wheel tests compare standard/edit priority, Unicode case folding and expansion, exact stable ties, mixed chart types and difficulties, unavailable meters, and seeded/reordered inputs of 0/1/6/7/64/257 charts. They compare selected chart identities as well as meter arrays and assert that standard-only lookup changes one allocation to zero.

Anchor tests compare every RowId across zero/all/alternating/seeded visibility flags, reversed/truncated/repeated orders, missing rows, absent parents and out-of-range child lookups. Graph tests compare exact output bits for empty input, NaN payloads, infinities, both signed-zero orders, constants, seeded matrix sizes and ordinary/degenerate/reversed percentile bounds.

All existing unit and ITGmania semantic/actor outcomes and failure diagnostics match saved baseline. Comparison normalizes thread IDs and two verified nondeterministic diagnostics: ordering of the same three missing Sprite.Load aliases, and which of the same two missing non-local archives is reported first. Those diagnostics were repeated three and ten times per binary respectively; reference/index/test files match the pinned main blobs.

The baseline is not fully green. Of 17 shell failures, 15 pass when rerun individually in both variants; the same two underlying failures poison the shared test lock. Existing theme and native fixture/compatibility failures remain. This pass introduces no observed behavioral regressions and does not establish complete Song Lua compatibility.

Six native full-song captures were run against ITGmania for both saved baseline and final executables, with identical selectors and extraction directory:

| Capture and archive prefix | Comparisons per variant | Mismatches | Panics |
|---|---:|---:|---:|
| 319\|TECH SOUP\|[lv.P.Clark] Epidermis (`0f2ef3f9`) | 363,873 | 0 | 0 |
| 319\|TECH SOUP\|[lv.P.Clark] Epidermis (`af2f887d`) | 363,873 | 0 | 0 |
| 280\|MODS\|[MASTER] Sharkmode (`b05379b7`) | 304,425 | 0 | 0 |
| Warp Zone (`ded0f7ff`) | 212,220 | 0 | 0 |
| Let Me Hear That (`2a77063d`) | 205,071 | 0 | 0 |
| 272\|MODS\|[lv.02] Riddle (`7ffacb89`) | 201,471 | 0 | 0 |

**1,650,933 native comparisons per variant, zero mismatches and zero panics.** This covers the exercised captures; capture wall times are not performance measurements.

Both release builds completed without compiler warnings. Edited Rust is formatted and git diff --check passes. Source and executable SHA-256 manifests bind the tested changes to the committed source.

## Isolation and pending work

The initial audit covered 89 unmerged local and remote-tracking perf refs and 59 distinct heads. The final audit covered 89 refs and 59 heads; no pending branch changes the three selected functions. None of the selected production files is changed by those pending heads. The pending `perf/1872-core-dataflows-20261009` optimizes the general edit-index helper in deadsync-chart; this pass removes the wheel caller's need for that helper and does not modify or duplicate its implementation.

All source edits and pass scripts are in `D:/deadsync-perf-1872-direct-scans-20261010`. The original checkout and uncommitted work were not edited by this pass. The original checkout changed externally during validation; those changes were left in place. Main advanced during the pass (final audit: `3deaec5386bf4134ad76592b639b1807772fa891`); this branch remains based on the pinned commit that was latest at creation. The commit is local and unmerged. It excludes deadsync-song.json.gz, rust-performance.md, optimize.sh and optimize.ps1.

Generated Cargo artifacts reuse `C:/GitHub/deadsync-perf-1872-data-churn-20261009/target/build` for capacity. Baseline executables were copied and hash-checked before edits. Raw logs, hash manifests and orchestration scripts remain ignored under this worktree's target directory.

## Reproduction

```powershell
cargo test --release --locked --config profile.release.lto=false --no-fail-fast -p deadsync -p deadsync-shell -p deadsync-profile -p deadsync-simfile -p deadsync-theme-simply-love --lib --test song_lua_itgmania_semantic_parity --test itgmania_actor_conformance -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false -p deadsync-theme-simply-love --lib benchmark_direct_scans -- --ignored --nocapture --test-threads=1
```

Repeat the benchmark in four fresh processes with the affinity and priority above. The harness prints timing and allocation rows. The unit/semantic command returns nonzero for the documented baseline failures; compare with the pinned main commit under the same fixture/environment conditions.

Run full-song captures separately:

```powershell
cargo test --release --locked --config profile.release.lto=false -p deadsync --test full_song_lua -- 0f2ef3f987449e6218525fba450d1db1201c9f733d4956061d3b36529b922305.tar.zst
cargo test --release --locked --config profile.release.lto=false -p deadsync --test full_song_lua -- af2f887d212d8ab702b772cea9c953500254ce01a6b78b423276fca9cf5a8306.tar.zst
cargo test --release --locked --config profile.release.lto=false -p deadsync --test full_song_lua -- b05379b7d12bfb02149e03f166be5d496dc3e7687dcc042f0bf525cc46aa918b.tar.zst
cargo test --release --locked --config profile.release.lto=false -p deadsync --test full_song_lua -- ded0f7ff19513e30119e0d6f65b57a661f1683872fc9ffced27737db06c49993.tar.zst
cargo test --release --locked --config profile.release.lto=false -p deadsync --test full_song_lua -- 2a77063dd2ab8c43bbdbb9e101be7fcf73156302e21e9771c88797218317d327.tar.zst
cargo test --release --locked --config profile.release.lto=false -p deadsync --test full_song_lua -- 7ffacb89fd95ab788758e273ba878c30380c4622406f1bb22d8c51292ca78d83.tar.zst
```

The pass ran executables directly after the equivalent --no-run --message-format=json build to preserve saved baseline binaries. Both variants used the generated extraction directory in target/native-temp.json. target/compare-results.py compares individual test outcomes and failure diagnostics; target/bench-summary.json contains all four process samples and allocation counts.
