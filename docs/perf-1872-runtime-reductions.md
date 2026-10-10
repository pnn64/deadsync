# Runtime reductions, patch 0.5.1872

Branch: `perf/1872-runtime-reductions-20261009`. Starting committed main: `d49e2923567b5f1bc32e599b0bceda123c67ea56`. Version: **0.5.1871 -> 0.5.1872**, updated in Cargo.toml and all three matching Cargo.lock package entries.

Read `rust-performance.md` and audited 34 distinct pending perf heads (60 local/remote refs). None changes the three production files selected here. This pass removes redundant work in audio timing, cabinet lighting, and asset scanning; it changes no Song Lua source.

- Cabinet-light selection stops at the first exact standard-difficulty match. Later charts cannot improve that distance or replace the first tie. A 16-chart generated-light plan with Hard and Medium first: **936.90 -> 239.84 ns/op (3.906x throughput)**.
- Playback-map insertion evaluates the music-boundary FMA and tolerance only when stream continuity and rate already allow merging. Validation, coalescing tolerance, arithmetic order on evaluated paths, cleanup, queue contents, and capacities are preserved. A 128-packet sequence with rate changes: **1378.64 -> 1083.03 ns/op (1.273x throughput)**. Steady contiguous 1,024-packet control: **12083.50 -> 12247.25 ns/op (0.987x throughput)**.
- Resource-fork filtering checks the ASCII `._` prefix directly in the OS filename bytes. It no longer validates or converts the entire filename to a lossy Unicode string. Mixed 1,024-file catalog: **145402.00 -> 82922.50 ns/op (1.753x throughput)**. Invalidly encoded filenames avoid their original temporary allocation entirely.

No production data structures, dependencies, caches, or public APIs were added. The reference implementations and performance helpers compile only in tests.

The paired benchmarks ran on an Intel Xeon E5-2696 v4, Windows x86_64-pc-windows-msvc, rustc 1.98.1 / LLVM 22.1.8, Ultimate Performance power plan. Each benchmark process was pinned to logical CPU 2 (affinity mask 4). Both variants used release opt-level 3 with LTO disabled. Four independent process runs each alternate variants for ten batches, discard warmup, and take the median of nine batches. Batches calibrate toward two milliseconds, capped at one million operations. The table shows medians across the four runs and the range of their paired ratios, not a confidence interval. Reference bodies were checked against the starting commit, allowing only test visibility/name changes and rustfmt whitespace/trailing commas.

Inputs are constructed outside timing and black-boxed. Lighting measures full plan creation and destruction, including unchanged hash ownership. Resource-fork cases measure the complete production path predicate, and catalog cases count matching paths across all 1,024 entries. Each audio operation clears a warmed map and inserts the stated packet count, including backlog cleanup; it reuses the map allocation. These are single-thread elapsed-throughput measurements, not hardware cycle measurements or a whole-game FPS claim. The filename cases also report scoped allocation counts. Their test allocator delegates to System and counts only the measuring thread; counting is disabled during timing, but an inactive TLS check remains on allocation/free calls. Invalid-filename timing gains therefore include this small instrumentation overhead; the eliminated allocation counts are exact. No RSS reduction is claimed.

| Case | Original ns/op | Current ns/op | Throughput ratio | Four-run ratio range | Allocations, original -> current | Requested bytes, original -> current |
|---|---:|---:|---:|---:|---:|---:|
| resource-fork-empty | 24.25 | 25.69 | 0.944x | 0.870-1.039x | 0 -> 0 | 0 -> 0 |
| resource-fork-root | 20.32 | 19.05 | 1.067x | 0.944-1.197x | 0 -> 0 | 0 -> 0 |
| resource-fork-parent | 36.67 | 36.56 | 1.003x | 0.943-1.121x | 0 -> 0 | 0 -> 0 |
| resource-fork-short-miss | 51.62 | 41.06 | 1.257x | 1.139-1.392x | 0 -> 0 | 0 -> 0 |
| resource-fork-short-hit | 49.39 | 43.25 | 1.142x | 1.101-1.187x | 0 -> 0 | 0 -> 0 |
| resource-fork-ascii-miss-200 | 280.51 | 146.70 | 1.912x | 1.776-2.076x | 0 -> 0 | 0 -> 0 |
| resource-fork-ascii-hit-200 | 280.26 | 149.45 | 1.875x | 1.777-1.977x | 0 -> 0 | 0 -> 0 |
| resource-fork-unicode-miss-64 | 293.20 | 139.47 | 2.102x | 2.082-2.207x | 0 -> 0 | 0 -> 0 |
| resource-fork-unicode-hit-64 | 305.63 | 138.43 | 2.208x | 2.185-2.225x | 0 -> 0 | 0 -> 0 |
| resource-fork-nested-miss | 51.38 | 42.23 | 1.217x | 1.078-1.363x | 0 -> 0 | 0 -> 0 |
| resource-fork-nested-hit | 55.08 | 43.00 | 1.281x | 1.146-1.466x | 0 -> 0 | 0 -> 0 |
| resource-fork-invalid-miss | 127.02 | 42.61 | 2.981x | 2.561-3.160x | 1 -> 0 | 10 -> 0 |
| resource-fork-invalid-hit | 116.16 | 41.53 | 2.797x | 2.570-2.942x | 1 -> 0 | 6 -> 0 |
| resource-fork-invalid-long-miss | 365.44 | 134.34 | 2.720x | 2.263-3.116x | 1 -> 0 | 189 -> 0 |
| resource-fork-invalid-long-hit | 336.08 | 126.69 | 2.653x | 2.437-2.741x | 1 -> 0 | 185 -> 0 |
| resource-fork-catalog-ascii-1024 | 57670.50 | 45221.50 | 1.275x | 1.241-1.311x | 0 -> 0 | 0 -> 0 |
| resource-fork-catalog-mixed-1024 | 145402.00 | 82922.50 | 1.753x | 1.676-1.877x | 0 -> 0 | 0 -> 0 |
| audio-insert-contiguous-1 | 9.19 | 9.24 | 0.994x | 0.913-1.073x | - -> - | - -> - |
| audio-insert-contiguous-128 | 1329.63 | 1228.44 | 1.082x | 1.030-1.149x | - -> - | - -> - |
| audio-insert-contiguous-1024 | 12083.50 | 12247.25 | 0.987x | 0.971-1.033x | - -> - | - -> - |
| audio-insert-gap-1 | 8.94 | 8.84 | 1.011x | 0.980-1.035x | - -> - | - -> - |
| audio-insert-gap-128 | 1223.79 | 1021.44 | 1.198x | 1.121-1.245x | - -> - | - -> - |
| audio-insert-gap-1024 | 13687.75 | 11539.00 | 1.186x | 1.093-1.354x | - -> - | - -> - |
| audio-insert-rate-1 | 9.18 | 8.55 | 1.073x | 0.984-1.125x | - -> - | - -> - |
| audio-insert-rate-128 | 1378.64 | 1083.03 | 1.273x | 1.177-1.362x | - -> - | - -> - |
| audio-insert-rate-1024 | 14607.00 | 11679.25 | 1.251x | 1.109-1.259x | - -> - | - -> - |
| audio-insert-rate-sparse-1 | 9.71 | 10.04 | 0.967x | 0.896-1.010x | - -> - | - -> - |
| audio-insert-rate-sparse-128 | 1345.92 | 1205.78 | 1.116x | 1.029-1.177x | - -> - | - -> - |
| audio-insert-rate-sparse-1024 | 11997.25 | 12087.00 | 0.993x | 0.944-1.078x | - -> - | - -> - |
| audio-insert-music-gap-1 | 8.34 | 8.39 | 0.993x | 0.923-1.080x | - -> - | - -> - |
| audio-insert-music-gap-128 | 1314.16 | 1303.47 | 1.008x | 0.916-1.069x | - -> - | - -> - |
| audio-insert-music-gap-1024 | 15823.25 | 14842.00 | 1.066x | 0.979-1.134x | - -> - | - -> - |
| lights-explicit-first-0 | 8.11 | 8.43 | 0.962x | 0.948-1.026x | - -> - | - -> - |
| lights-explicit-last-0 | 8.03 | 7.67 | 1.047x | 0.979-1.166x | - -> - | - -> - |
| lights-generated-first-0 | 7.45 | 7.69 | 0.969x | 0.936-1.088x | - -> - | - -> - |
| lights-no-exact-0 | 7.86 | 8.24 | 0.954x | 0.908-1.224x | - -> - | - -> - |
| lights-explicit-first-2 | 106.39 | 102.66 | 1.036x | 0.947-1.150x | - -> - | - -> - |
| lights-explicit-last-2 | 106.43 | 103.00 | 1.033x | 0.927-1.195x | - -> - | - -> - |
| lights-generated-first-2 | 259.48 | 227.10 | 1.143x | 0.988-1.170x | - -> - | - -> - |
| lights-no-exact-2 | 259.50 | 240.47 | 1.079x | 0.963-1.188x | - -> - | - -> - |
| lights-explicit-first-16 | 106.91 | 100.48 | 1.064x | 1.016-1.223x | - -> - | - -> - |
| lights-explicit-last-16 | 110.50 | 107.16 | 1.031x | 0.980-1.105x | - -> - | - -> - |
| lights-generated-first-16 | 936.90 | 239.84 | 3.906x | 3.615-3.998x | - -> - | - -> - |
| lights-no-exact-16 | 878.77 | 859.40 | 1.023x | 0.992-1.061x | - -> - | - -> - |
| lights-explicit-first-256 | 319.42 | 98.83 | 3.232x | 2.875-3.871x | - -> - | - -> - |
| lights-explicit-last-256 | 330.37 | 330.06 | 1.001x | 0.921-1.133x | - -> - | - -> - |
| lights-generated-first-256 | 12477.75 | 463.95 | 26.895x | 25.718-28.826x | - -> - | - -> - |
| lights-no-exact-256 | 11439.25 | 11104.50 | 1.030x | 0.959-1.134x | - -> - | - -> - |

No case was slower in all four runs. Small ratios around 1.0 should be treated as noise, not speedups.

Affected-crate unit tests and baseline comparisons:

| Suite | Starting passed | Final passed | Failed | Ignored benchmarks |
|---|---:|---:|---:|---:|
| deadlib_audio_core-tests | 48 | 50 | 0 | 1 |
| deadsync_lights-tests | 42 | 43 | 0 | 1 |
| deadsync_simfile-tests | 203 | 205 | 0 | 1 |

Five added differential tests compare exact light choices across 512 three-chart combinations, clamped preferences, ties, case-insensitive types, invalid fallback indices and missing note data; resource-fork decisions for empty/root/parent/nested paths, ASCII and Unicode filenames, all 65,536 possible UTF-16 units in three prefix positions plus surrogate pairs, and allocation-free current results; and audio state, floating-point bits, search/inverse results, capacities, invalid values, coalescing thresholds and 5,000 mixed insertions. Existing tests retain their baseline outcomes. The three ignored benchmark tests were run explicitly in all four timing runs.

Fresh baseline and final Song Lua compatibility runs used recorded native ITGmania fixtures. This is a replay comparison against native captures, not a new live ITGmania capture. All six archives passed on both versions:

| Native archive | Comparisons passed, each version |
|---|---:|
| 319\|TECH SOUP\|[lv.P.Clark] Epidermis (`0f2ef3f98744`) | 363,873 |
| 319\|TECH SOUP\|[lv.P.Clark] Epidermis (`af2f887d212d`) | 363,873 |
| 280\|MODS\|[MASTER] Sharkmode (`b05379b7d12b`) | 304,425 |
| Warp Zone (`b38698ececd6`) | 212,220 |
| Let Me Hear That (`0229b74d092e`) | 205,071 |
| 272\|MODS\|[lv.02] Riddle (`920eb361ce68`) | 201,471 |

Total: **1,650,933 native-trace comparisons passed**, with no changed archive outcomes.

The semantic suite has 143 passing, 38 failing, and 77 ignored tests on both versions; all 38 failure diagnostics match the baseline.

The actor suite has 30 passing, 1 failing, and 0 ignored tests on both versions; the failure diagnostic matches the baseline.

The focused compatibility suites are therefore not green: 39 failures predate this pass. Comparison normalizes thread IDs and sorts only the known unordered `Sprite.Load has no matching DeadSync actor` diagnostics in `image_texture_aliases_match_native_draws`. Every other diagnostic, test outcome, full-song comparison count, and panic outcome is compared directly. Lighting output selection and audio mapping receive separate differential tests; the replay harness does not exercise physical lighting hardware or a live audio device.

Reproduce the affected-crate tests and paired benchmarks from this worktree (Cargo commands below do not pin affinity; the recorded runs used `target/run-bench.py` with `SetProcessAffinityMask` set to 4):

```powershell
cargo test --release --locked --config profile.release.lto=false -p deadlib-audio-core -p deadsync-lights -p deadsync-simfile --lib -- --test-threads=1
1..4 | ForEach-Object {
    cargo test --release --locked --config profile.release.lto=false -p deadlib-audio-core -p deadsync-lights -p deadsync-simfile --lib benchmark_runtime -- --ignored --nocapture --test-threads=1
}
cargo test --release --locked --config profile.release.lto=false -p deadsync --test song_lua_itgmania_semantic_parity -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false -p deadsync --test itgmania_actor_conformance -- --test-threads=1
```

Run each complete archive below with `cargo test --release --locked --config profile.release.lto=false -p deadsync --test full_song_lua -- <archive-name>`:

- `0f2ef3f987449e6218525fba450d1db1201c9f733d4956061d3b36529b922305.tar.zst`
- `af2f887d212d8ab702b772cea9c953500254ce01a6b78b423276fca9cf5a8306.tar.zst`
- `b05379b7d12bfb02149e03f166be5d496dc3e7687dcc042f0bf525cc46aa918b.tar.zst`
- `b38698ececd60bdf311a15de205f43d0f3466eac6d41296b9abf469485ebab4d.tar.zst`
- `0229b74d092e8f04278d64b7c994877a0a3bcc52e9d7ddb00d6a98f1f5244cd0.tar.zst`
- `920eb361ce688e70b4364258337b30c9e4bf7d337f2c83a30e59215eb838735d.tar.zst`

Local evidence remains under this worktree's ignored `target/`: baseline/current build and test logs, six replay logs for each version, `branch-audit.json`, `compatibility-comparison.json`, `final-1.log` through `final-4.log`, `benchmark-results.json`, and `source-hashes.json`. Builds reused the existing ignored Cargo cache under `C:/GitHub/deadsync-perf-1872-data-churn-20261009/target/build`. Source edits and the commit are confined to this pass's worktree. Nothing is merged or pushed. The four excluded filenames are absent from the commit.
