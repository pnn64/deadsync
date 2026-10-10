# Owned payloads and cache serialization — 2026-10-10

Branch: `perf/1872-owned-pipelines-20261010`. Base: `415cb4223d61df13cbe9983bde8451bb612cfd37`, committed main at worktree creation. DeadSync **0.5.1871 → 0.5.1872**, in Cargo.toml and Cargo.lock. Separate worktree: `D:/deadsync-perf-1872-owned-pipelines-20261010`. No merge or push.

Three changes outside Song Lua:

1. Gameplay and practice entry move the two loaded chart payloads into Arcs, removing deep copies of notes, parsed notes, beat tables, timing segments and attacks. Cabinet-light extraction still reads trailing payloads first. Loader requests, preload selection, errors, player order and runtime setup are unchanged.
2. Cached single-song sync analysis takes its owned result and moves all five plot vectors into the UI event. Applied estimates still return zero bias and empty plots; legacy curves still expand the column count to the time-axis length. Batch analysis keeps its borrowing API.
3. Oversized sync-cache writes clear and reuse the JSON buffer when evicting oldest plots. The 64 MiB limit, warnings, estimates, eviction order, temporary-file handling and serialized bytes remain unchanged. The initial buffer capacity stays allocated until the write finishes, avoiding overlapping old/new buffers on retries.

The performance guide was read. The initial audit covered **83 unmerged perf refs / 53 distinct heads**; none touches the four production implementation files. Two pending font branches touch an unrelated texture-upload hunk in the existing shell regression-test file. This pass only adjusts allocator delegation and its import there, preserving existing global counters while adding thread-local observations. New test modules contain the regression cases, benchmarks and frozen originals.

## Paired release measurements

Windows x86-64, Intel Xeon E5-2696 v4 (22 cores / 44 logical processors), Rust 1.98.1 / LLVM 22.1.8 / MSVC. Release optimization with `profile.release.lto=false`, locked dependencies. Four fresh single-threaded processes, affinity mask 4 (logical CPU 2), above-normal priority. Each warms both variants and measures nine alternating batches after calibration. Formal timings start after this pass's builds and baseline native captures finish.

The table contains medians of the four process medians. Original expressions/functions under `tests/perf/owned_*_original.rs` were verified against the base commit; both variants run in the same binary. Timing and allocation measurements include cloning identical fixture inputs, consuming them and dropping outputs. This common setup makes transfer speedups conservative. Moves retain the input vectors and their capacities; no extra shrinking allocation is introduced. Separate assertions isolate production handoff: two outer Arc allocations for charts and zero allocations for moving the five cached-view buffers.

Cache timings exercise the complete writer, including local SSD writes and file replacement in TEMP. These are not pure JSON CPU timings. Inputs respect the 48 MiB raw-plot budget. `fits` does not retry; `drop-all` retains estimates only; `drop-one` keeps three of four plots; `drop-many` keeps four of eight. Charts cover 256, 4,096 and 65,536 rows per player plus a cabinet-light case with two trailing payloads. Cached views cover empty, 256, 16,384 and 262,144 columns plus an applied-result control.

| Case | Original µs/op | Current µs/op | Throughput ratio | Paired process range |
| --- | ---: | ---: | ---: | ---: |
| charts/small | 6.804 | 3.524 | 1.931× | 1.852–1.974× |
| charts/normal | 439.778 | 132.074 | 3.330× | 1.962–3.993× |
| charts/large | 5130.168 | 2498.355 | 2.053× | 1.933–2.131× |
| charts/cabinet | 236.371 | 132.079 | 1.790× | 1.732–1.931× |
| cached/empty | 0.055 | 0.034 | 1.618× | 1.366–2.353× |
| cached/small | 1.627 | 0.773 | 2.107× | 2.076–2.162× |
| cached/normal | 431.435 | 82.661 | 5.219× | 2.377–8.006× |
| cached/large | 9425.100 | 4411.375 | 2.137× | 2.106–2.163× |
| cached/applied | 0.800 | 0.785 | 1.019× | 1.007–1.047× |
| cache/fits | 1767.230 | 1764.253 | 1.002× | 0.998–1.016× |
| cache/drop-all | 203528.100 | 205027.450 | 0.993× | 0.984–1.031× |
| cache/drop-one | 581150.100 | 537095.750 | 1.082× | 1.035–1.110× |
| cache/drop-many | 1240211.200 | 971622.500 | 1.276× | 1.232–1.306× |

| Case | Allocations, original → current | Reallocations | Cumulative requested bytes | Peak added live bytes |
| --- | ---: | ---: | ---: | ---: |
| charts/small | 39 → 21 | 0 → 0 | 52,656 → 27,416 | 52,656 → 27,416 |
| charts/normal | 39 → 21 | 0 → 0 | 805,296 → 403,736 | 805,296 → 403,736 |
| charts/large | 39 → 21 | 0 → 0 | 12,847,536 → 6,424,856 | 12,847,536 → 6,424,856 |
| charts/cabinet | 57 → 39 | 0 → 0 | 1,207,928 → 806,368 | 1,207,928 → 806,368 |
| cached/empty | 0 → 0 | 0 → 0 | 0 → 0 | 0 → 0 |
| cached/small | 10 → 5 | 0 → 0 | 24,576 → 12,288 | 24,576 → 12,288 |
| cached/normal | 10 → 5 | 0 → 0 | 1,572,864 → 786,432 | 1,572,864 → 786,432 |
| cached/large | 10 → 5 | 0 → 0 | 25,165,824 → 12,582,912 | 25,165,824 → 12,582,912 |
| cached/applied | 5 → 5 | 0 → 0 | 12,288 → 12,288 | 12,288 → 12,288 |
| cache/fits | 22 → 22 | 9 → 9 | 80,475 → 80,473 | 46,618 → 46,618 |
| cache/drop-all | 17 → 16 | 24 → 21 | 302,039,517 → 302,037,595 | 167,818,238 → 167,818,238 |
| cache/drop-one | 35 → 34 | 40 → 21 | 436,256,727 → 302,039,125 | 226,528,622 → 167,819,768 |
| cache/drop-many | 62 → 58 | 100 → 21 | 1,255,964,751 → 316,441,165 | 310,439,526 → 182,221,808 |

Allocation counts and bytes agree across all four processes. Cumulative bytes include each realloc's new requested size; they are not bytes copied. Peak added live bytes measure requested live sizes on the measured thread, including setup, rather than process RSS or internal allocator transients. Empty/applied cached results and non-retrying writes are controls, not claimed allocation wins. Cache accounting includes filename formatting: the two-byte `fits` difference comes from the reference/current filenames, not the optimization. Material cache gains are the removed reallocations and MiB-scale byte/peak reductions. Control timing differences and file-I/O variance are shown above. The drop-all control has a 0.7% slower aggregate median, but individual paired runs span 0.984-1.031x; no speed or peak-memory benefit is claimed for that control. Normal-sized transfer timing varies considerably across processes (see the full ranges). No application-wide frame-rate claim is made.

## Regression and native comparison

Eight baseline executables were copied and hashed before rebuilding. Both variants use the same six committed full-song archives and local extraction directory. All 848 noteskin files were verified against base Git blobs. The user's uncommitted archive was not copied.

Eight new ordinary tests pass: complete chart fields/order, owned-buffer retention, missing-payload rejection, cached/applied/legacy/missing plot equivalence, one-time result extraction, byte-identical cache eviction and reload, and retry allocation/peak reduction. Three ignored benchmark tests run explicitly in all four timing processes.

| Suite | Original cases | Current passed | Current failed | Current ignored |
| --- | ---: | ---: | ---: | ---: |
| deadsync_shell-tests | 378 | 363 | 17 | 9 |
| deadsync_theme_simply_love-tests | 1267 | 1263 | 1 | 3 |
| deadsync_simfile-tests | 203 | 203 | 0 | 0 |
| semantic | 267 | 149 | 41 | 77 |
| actor | 31 | 30 | 1 | 0 |

Every pre-existing outcome and failure diagnostic matches baseline. Normalization is limited to thread IDs, the optional backtrace hint on the shell fade failure, and two separately audited unordered diagnostics: the same three missing sprite-alias lines, and the first of the same two missing non-local archive references. Three alias reruns and ten missing-reference reruns per variant verify these exceptions. A targeted two-test rerun proves the new expected-panic test consumes the backtrace hint before the existing fade failure; isolated fade runs still print identical hints and failure values. Numeric, geometry and timing failure content is compared without normalization.

The shell's existing `hidden_score_proxy_keeps_overlay_fade` failure poisons its shared lock, causing 16 follow-on failures. All 17 were rerun individually in both variants: **15 pass; two retain identical failures** (the fade mismatch, and `root_player_proxy_uses_repeatable_direct_field_and_hud_segments`, 3 versus 2). Thus the lock cascade does not mask this pass's regression checks. The existing theme Cyber model-height and native compatibility failures remain outside scope. The full suite is not globally green.

| ITGmania recording | Archive prefix | Comparisons per variant | Mismatches |
| --- | --- | ---: | ---: |
| 319\|TECH SOUP\|[lv.P.Clark] Epidermis | `0f2ef3f98744` | 363,873 | 0 |
| 319\|TECH SOUP\|[lv.P.Clark] Epidermis | `af2f887d212d` | 363,873 | 0 |
| 280\|MODS\|[MASTER] Sharkmode | `b05379b7d12b` | 304,425 | 0 |
| Warp Zone | `ded0f7ff1951` | 212,220 | 0 |
| Let Me Hear That | `2a77063dd2ab` | 205,071 | 0 |
| 272\|MODS\|[lv.02] Riddle | `7ffacb89fd95` | 201,471 | 0 |

Total: **1,650,933 recorded native comparisons per variant**, zero mismatches. Root library/harness targets compile; ordinary unit results above cover shell, theme and simfile. Final compilation emits no warnings. `git diff --check` and rustfmt checks pass for the formatted changed files. The existing shell regression-test file retains unrelated pre-existing rustfmt drift; an exact source check limits its edits to allocator delegation/imports and preserves line numbers. Frozen originals, Cargo contents and tested source/binary hashes are verified before commit.

## Reproduction and isolation

From this branch, with a generated CARGO_TARGET_DIR and writable local TEMP/TMP:

```powershell
cargo test --release --locked --config profile.release.lto=false -p deadsync-shell --lib owned_pipeline_tests -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false -p deadsync-shell --lib benchmark_owned_pipelines -- --ignored --nocapture --test-threads=1
cargo test --release --locked --config profile.release.lto=false -p deadsync -p deadsync-shell -p deadsync-theme-simply-love -p deadsync-simfile --lib --test song_lua_itgmania_semantic_parity --test itgmania_actor_conformance --test full_song_lua --no-run --message-format=json
```

Run the built semantic/actor executables with `--test-threads=1`, and `full_song_lua` once for each archive above. Logs, four timing runs, allocation data, comparison manifests and hashes remain in this worktree's ignored `target` directory. The shared generated build cache is `C:/GitHub/deadsync-perf-1872-data-churn-20261009/target/build`; extraction uses its `owned-pipelines-415cb422-tmp` subdirectory.

Final audit: 83 other unmerged perf refs / 53 heads; 0 refs changed since the initial audit. The original checkout was only read, apart from shared Git metadata for branch/worktree creation. All source edits and the commit are in this new worktree; the original's status may change independently during other work. Excluded from the commit: `deadsync-song.json.gz`, `rust-performance.md`, `optimize.sh`, `optimize.ps1`.
