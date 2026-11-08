# Selection data performance pass — 2026-10-10

Branch: `perf/1872-select-data-20261010`.

Starting main: `58e888a2404a7e7b68e7028b8da444c2784b1032`. Version: **0.5.1871 → 0.5.1872**, exactly one patch increment in Cargo.toml and the three workspace package entries in Cargo.lock.

## Changes

1. **Scorebox reconciliation:** `entries_with_local_self_state` borrows an already-failed self row's pane when its machine tag is present, or when blank local initials cannot provide a tag. It previously cloned the entire pane to assign values it already contained. Rows needing a fail flag, a machine tag, or self identification still follow the original copy path. The first existing self row still wins.
2. **Content-browser metadata:** `meta_line` writes into one correctly sized result, borrowing the search explanation. It removes the field vector, song-count temporary, added-date wrapper string, explanation clone, and final join. The existing byte/date formatters remain unchanged. Empty optional fields retain their separators.
3. **Content-browser badges:** type badges compare the borrowed type directly, removing the lowercase allocation and duplicate branches. Fixed type/status labels stay static; a changing percentage uses the existing inline text storage. The unchanged u32 percentage calculation produces at most eleven bytes, within the fourteen-byte inline capacity.

All production edits are outside Song Lua. The shared allocation counter and benchmark helper are test-only. No new production cache, dependency, or unsafe code is introduced.

## Paired benchmarks

Windows x86_64, Rust 1.98.1 / LLVM 22.1.8, `Intel(R) Xeon(R) CPU E5-2696 v4 @ 2.20GHz` (44 logical CPUs). Release optimization, LTO disabled, the same scoped System allocator instrumentation for both implementations.

Five frozen function bodies were checked byte-for-byte against starting main. The original row renderer calls its original metadata and badge helpers. Both variants share immutable inputs. Actor buffers are preallocated and reused in both variants; output creation and disposal are timed. Setup is outside the measurement.

Each fresh process warms each variant three times, calibrates toward 25 ms per batch (capped at one million iterations), and alternates their execution order across nine measured batches. Four sequential processes run on logical CPU 2 with AboveNormal priority after builds and regression runs finish. Values below are medians of the four process medians. The range is the four paired original/current ratios. These are local workload measurements, not whole-game FPS claims.

| Case | Original ns/op | Current ns/op | Throughput | Paired range | Allocations | Reallocations | Requested bytes | Peak added live bytes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| scorebox/noop-13 | 3486.65 | 25.05 | 139.160× | 135.594–143.779× | 40 → 0 | 0 → 0 | 1445 → 0 | 1445 → 0 |
| scorebox/noop-64 | 14544.81 | 42.12 | 345.318× | 338.296–352.284× | 193 → 0 | 0 → 0 | 7106 → 0 | 7106 → 0 |
| scorebox/noop-empty-tag-64 | 14716.28 | 47.66 | 308.776× | 306.891–312.440× | 192 → 0 | 0 → 0 | 7103 → 0 | 7103 → 0 |
| scorebox/set-fail-64 | 15498.78 | 15086.48 | 1.027× | 0.994–1.029× | 193 → 193 | 0 → 0 | 7106 → 7106 | 7106 → 7106 |
| scorebox/set-tag-64 | 15146.21 | 14556.94 | 1.040× | 1.007–1.055× | 193 → 193 | 0 → 0 | 7105 → 7105 | 7105 → 7105 |
| scorebox/match-name-64 | 14699.31 | 14533.67 | 1.011× | 0.995–1.062× | 193 → 193 | 0 → 0 | 7106 → 7106 | 7106 → 7106 |
| scorebox/passing-13 | 18.95 | 19.34 | 0.980× | 0.976–1.022× | 0 → 0 | 0 → 0 | 0 → 0 | 0 → 0 |
| scorebox/empty | 14.99 | 15.25 | 0.983× | 0.920–1.062× | 0 → 0 | 0 → 0 | 0 → 0 | 0 → 0 |
| badges/queued | 308.83 | 90.66 | 3.406× | 3.123–3.514× | 3 → 0 | 0 → 0 | 18 → 0 | 13 → 0 |
| badges/downloading | 348.61 | 129.35 | 2.695× | 2.674–2.762× | 3 → 0 | 0 → 0 | 19 → 0 | 11 → 0 |
| badges/installed | 309.70 | 93.90 | 3.298× | 3.262–3.349× | 3 → 0 | 0 → 0 | 15 → 0 | 12 → 0 |
| badges/unavailable | 169.83 | 95.41 | 1.780× | 1.705–1.899× | 2 → 1 | 0 → 0 | 20 → 12 | 12 → 12 |
| badges/unknown-type | 303.69 | 136.64 | 2.223× | 2.035–2.385× | 3 → 1 | 0 → 0 | 18 → 12 | 15 → 12 |
| badges/empty-type | 92.18 | 87.18 | 1.057× | 1.045–1.070× | 1 → 1 | 0 → 0 | 12 → 12 | 12 → 12 |
| metadata/basic | 664.86 | 497.72 | 1.336× | 1.315–1.362× | 4 → 2 | 0 → 0 | 133 → 29 | 133 → 29 |
| metadata/full | 1222.03 | 814.70 | 1.500× | 1.398–1.524× | 7 → 3 | 2 → 1 | 249 → 95 | 213 → 87 |
| metadata/long-explanation | 1268.92 | 839.14 | 1.512× | 1.447–1.558× | 7 → 3 | 2 → 1 | 3221 → 1581 | 3185 → 1573 |
| metadata/zero-songs | 527.92 | 458.51 | 1.151× | 1.132–1.170× | 3 → 2 | 0 → 0 | 112 → 16 | 112 → 16 |
| metadata/empty-fields | 703.81 | 509.58 | 1.381× | 1.341–1.404× | 4 → 2 | 0 → 0 | 140 → 32 | 140 → 32 |
| page/7-rows | 14751.81 | 9871.57 | 1.494× | 1.398–1.594× | 84 → 35 | 14 → 7 | 2126 → 917 | 761 → 563 |

`scorebox/noop-*` has a matching failed local score and an already-failed self row. The empty-tag case has no tag and blank initials. The set-fail, set-tag, match-name, passing-score, and empty-pane cases are controls for paths that still need copying or already borrowed their data.

Metadata cases cover ordinary rows, dates and search explanations, a long Unicode explanation, and zero/empty optional fields. Badge cases cover queue statuses, numeric download progress, installed items, hidden badges, unknown Unicode types, and missing types. The existing library-name lookup allocation is included and unchanged in cases without a queue entry. `page/7-rows` measures seven complete row compositions with metadata, alternating mixed/keyboard types and queued/downloading statuses, placeholder art, and a selected row. This combined result includes both browser optimizations.

Allocation figures are exact per operation and identical across all four processes. Requested bytes include every allocation/reallocation request, including temporary buffers. Peak added live bytes is the scoped high-water allocated-minus-freed balance above the pre-existing inputs and actor buffers; it is not process RSS or allocator overhead.

Timing controls below parity:

- `scorebox/passing-13`: 0.39 ns slower (2.06%), paired range 0.976–1.022×.
- `scorebox/empty`: 0.26 ns slower (1.70%), paired range 0.920–1.062×.

The two sub-nanosecond scorebox control differences have paired ranges crossing parity; this run does not establish a consistent timing change for those controls. Their allocation counts remain zero. All twenty workloads have equal or lower allocations, reallocations, requested bytes and peak added live bytes.

## Behavioral validation

Both starting main and the final sources ran the same release suites. **1,647 tests passed; 42 pre-existing failures retained their individual outcomes and diagnostics.** Seven regression tests were added, with four separately ignored benchmark entrypoints. Existing ignored tests remain ignored.

| Suite | Passed | Failed | Ignored |
| --- | ---: | ---: | ---: |
| deadsync_theme_simply_love-tests | 1270 | 1 | 7 |
| deadsync_simfile-tests | 203 | 0 | 0 |
| semantic | 144 | 40 | 77 |
| actor | 30 | 1 | 0 |

New tests compare all leaderboard fields against the frozen original across pane kinds, flags, scores including NaN/infinity, tag states, Unicode/blank initials, name matches and first-self precedence. They verify zero churn for already-reconciled rows and ownership for actual edits. Browser tests cover exact text and capacity across size/count boundaries, unusual dates and optional fields; complete badge text/color/geometry/order/visibility across queue and library states; saturated u32 percentages; zero temporary allocations for visible queue badges; and the complete seven-row actor sequence. Actor comparison normalizes only text storage, preserving text and every drawing field.

The Song Lua compatibility harness ran against six checked-in ITGmania full-song captures on both implementations:

| Capture | Archive | Comparisons | Failures |
| --- | --- | ---: | ---: |
| 319\|TECH SOUP\|[lv.P.Clark] Epidermis | `0f2ef3f987449e6218525fba450d1db1201c9f733d4956061d3b36529b922305.tar.zst` | 363,873 | 0 |
| 319\|TECH SOUP\|[lv.P.Clark] Epidermis | `af2f887d212d8ab702b772cea9c953500254ce01a6b78b423276fca9cf5a8306.tar.zst` | 363,873 | 0 |
| 280\|MODS\|[MASTER] Sharkmode | `b05379b7d12bfb02149e03f166be5d496dc3e7687dcc042f0bf525cc46aa918b.tar.zst` | 304,425 | 0 |
| Warp Zone | `ded0f7ff19513e30119e0d6f65b57a661f1683872fc9ffced27737db06c49993.tar.zst` | 212,220 | 0 |
| Let Me Hear That | `2a77063dd2ab8c43bbdbb9e101be7fcf73156302e21e9771c88797218317d327.tar.zst` | 205,071 | 0 |
| 272\|MODS\|[lv.02] Riddle | `7ffacb89fd95ab788758e273ba878c30380c4622406f1bb22d8c51292ca78d83.tar.zst` | 201,471 | 0 |

**1,650,933 native comparisons passed, zero failures, on each side.** All 848 tracked noteskin asset blobs matched Git before the baseline. These native captures guard Song Lua and shared rendering behavior; the changed browser and scorebox paths are directly covered by frozen-original differential tests.

The suite is not fully green on starting main. Baseline failures include the Cyber model-height fixture, native Lua/actor mismatches, absent external song directories and two missing non-local archive references. Failure comparison normalizes thread IDs. Fresh unchanged-binary reruns additionally proved two unordered diagnostics: identical missing-actor line sets for the alias test, and the same set of two missing archive references whose first reported member varies. No other diagnostic differences are ignored.

Formatting, `git diff --check`, frozen-body checks, exact version-only Cargo diffs, and source hashes passed.

## Reproduction and evidence

Build the same combined feature graph from the pass worktree:

```powershell
cargo test --release --locked --config profile.release.lto=false -p deadsync -p deadsync-simfile -p deadsync-theme-simply-love --lib --test song_lua_itgmania_semantic_parity --test itgmania_actor_conformance --test full_song_lua --no-run --message-format=json
```

Use the executable paths from the Cargo JSON artifacts. Run the theme and simfile libraries, semantic target and actor target with `--test-threads=1`. Run the custom `full_song_lua` executable once for each archive listed above. The root library is compiled to preserve the combined graph, but not run.

Run the theme executable's benchmarks with `benchmark_select_data --ignored --nocapture --test-threads=1`, in four fresh processes on the same CPU. The committed `tests/perf/select_data_support.rs` contains the measurement procedure; the scoped counter is in `tests/support/perf.rs`.

Local ignored evidence is under this worktree's `target/`: baseline/current build logs, test logs and executable manifests; saved original binaries and SHA-256 hashes; native archive selectors/logs; diagnostic rerun audits; `final-1.log` through `final-4.log`; `benchmark-summary.json`; `compatibility-comparison.json`; `verified-source-hashes.json`; and pending-branch audit snapshots. Compilation reused the generated build cache in `C:/GitHub/deadsync-perf-1872-data-churn-20261009/target/build`. No generated artifacts are committed.

## Isolation

Created a new LF worktree from the latest committed main at pass start. Audited 72 unmerged local/remote perf refs (46 distinct heads). The only production-file overlap among the selected paths was the pending `push_doubles` slice-borrowing change; this pass leaves that function unchanged. No pending optimization is duplicated.

The original checkout and its uncommitted work were not modified. No merge or push was performed. The commit excludes `deadsync-song.json.gz`, `rust-performance.md`, `optimize.sh`, and `optimize.ps1`.

Main advanced independently during this pass to `2963d1ebe6da487a5610591c2cc2967af09d42f2`. This branch retains its required starting-main base. Read-only original-checkout snapshots are saved alongside the local evidence.
