# Song Lua navigation and message input ownership

Reload consumes its existing entry paths, function-action capture constructs actor inputs directly, and cross-actor message capture borrows source handles. Before/after throughput, scheduled worker cycles and allocation traffic are retained for every workload, including negative values.

At 128 actors, direct action-input construction saves 1 allocation, 5 reallocations and 4,992 allocated bytes per capture. A 128-command message queue saves 4,032 allocated bytes per capture. An eight-layer reload saves 9 allocations and 864 allocated bytes.

Scoped CPU and throughput observations vary by workload; allocation/copy reductions are the reason to retain these changes. Every scoped paired throughput band spans zero, so these runs do not establish a general CPU speedup.

The primary offline dedicated-fixture slowdown did not repeat in the short interleaved confirmation: median paired throughput is +0.10% offline and -0.55% live, with both bands spanning zero. The original negative result is retained below. These observations support no general compiler CPU speed claim.

Baseline: `82decba519d78edd9360f9ac0757e37c73519c03`. The three changes follow the temporary-allocation, collection-capacity and repeated-copy guidance in [rust-performance.md](../../rust-performance.md):

1. Move `scene.entry_paths` into reload preparation because reload already consumes the scene and its VM. Layer order, primary index, end/startup output and error handling keep their existing order.
2. Build the complete function-action actor list with `enumerate().map().collect()` instead of allocating all integer indices and looking each index up again through a filtering iterator. The exact iterator length also avoids geometric growth in the table vector.
3. Queue borrowed source tables directly alongside their source indices. The old queue cloned the same source handle for every command; the new queue retains the existing reference and avoids an index-to-table lookup. Overlay command additions still happen after probing finishes.

No unsafe code, production cache or extra production helper is added. The baseline passes 825 tests with zero failures and 37 ignored benchmarks; the candidate passes 828 with zero failures and 39 ignored benchmarks. Three new independent frozen-body comparisons exercise empty and populated actor lists, aliases, errors, semantic-state/clock restoration, message targets and reload layer/startup behavior. All three frozen functions match the committed baseline after identical formatting. Every complete fixture snapshot agrees exactly across timed variants and selected pairs.

Six alternating paired rounds retain 54 samples per variant/workload: eleven scopes and eight complete compiler fixtures (2,052 selected samples). Original attempts remain in JSON. The small dedicated fixture exercises a stable cross-actor message and a function action; the separate reload scopes exercise one, two and eight entry layers.

| Scoped workload (unit) | Before units/s | After units/s | Throughput | Worker cycles/unit before -> after | Allocations/unit before -> after | Reallocations/unit before -> after | Allocated bytes/unit before -> after |
|---|---:|---:|---:|---:|---:|---:|---:|
| action_inputs_empty (captures) | 18,395.03 | 18,872.38 | +2.59% | 118,259.047 -> 115,868.562 | 25.844 -> 25.844 | 7 -> 7 | 5,230.125 -> 5,230.125 |
| action_inputs_one (captures) | 9,882.03 | 9,592.61 | -2.93% | 220,347.141 -> 227,254.516 | 81.906 -> 80.906 | 9 -> 9 | 11,127.062 -> 11,023.062 |
| action_inputs_128_untouched (captures) | 15,271.18 | 14,823.05 | -2.93% | 143,103.703 -> 147,243.328 | 27.281 -> 26.281 | 12 -> 7 | 14,287.375 -> 9,295.375 |
| action_inputs_128_all (captures) | 205.05 | 210.87 | +2.84% | 10,661,526.781 -> 10,367,705.125 | 5,010.812 -> 5,009.812 | 287 -> 282 | 736,054.344 -> 731,062.344 |
| message_inputs_empty (captures) | 9,552,238.81 | 9,411,764.71 | -1.47% | 144.062 -> 157.750 | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| message_inputs_one (captures) | 4,793.47 | 4,663.06 | -2.72% | 454,052.875 -> 469,160.719 | 225.812 -> 225.812 | 13.031 -> 13.031 | 30,061.406 -> 29,997.406 |
| message_inputs_32_one (captures) | 159.10 | 156.26 | -1.79% | 13,735,905.391 -> 13,982,390.062 | 6,412.188 -> 6,412.188 | 422.031 -> 422.031 | 822,935.312 -> 821,975.312 |
| message_inputs_32_four (captures) | 39.07 | 39.97 | +2.32% | 55,898,480.797 -> 54,637,301.969 | 25,249.812 -> 25,249.812 | 1,674 -> 1,674 | 2,828,181.688 -> 2,824,149.688 |
| reload_one_layer (reloads) | 6.60 | 6.60 | +0.12% | 331,131,889.500 -> 330,758,186.500 | 226,510 -> 226,508 | 10,766 -> 10,766 | 37,478,935 -> 37,478,827 |
| reload_two_layers (reloads) | 6.80 | 6.64 | -2.31% | 320,989,663 -> 328,645,601 | 230,233 -> 230,230 | 10,928 -> 10,928 | 38,123,321 -> 38,123,105 |
| reload_eight_layers (reloads) | 5.66 | 5.77 | +1.89% | 385,934,446.500 -> 378,610,833 | 252,537 -> 252,528 | 11,886 -> 11,886 | 41,929,314 -> 41,928,450 |

| Scoped workload | Median paired throughput | 95% pair-bootstrap band |
|---|---:|---:|
| action_inputs_empty | +0.50% | -3.57% to +2.55% |
| action_inputs_one | +0.16% | -3.95% to +3.28% |
| action_inputs_128_untouched | +0.62% | -8.26% to +3.36% |
| action_inputs_128_all | +2.62% | -0.78% to +7.38% |
| message_inputs_empty | -1.61% | -8.12% to +8.39% |
| message_inputs_one | -4.84% | -9.21% to +3.98% |
| message_inputs_32_one | -1.20% | -4.33% to +1.43% |
| message_inputs_32_four | -0.89% | -4.88% to +7.10% |
| reload_one_layer | -1.25% | -7.27% to +7.48% |
| reload_two_layers | -1.61% | -5.20% to +1.61% |
| reload_eight_layers | +4.38% | -5.49% to +12.00% |

| Complete workload | Before compiles/s | After compiles/s | Throughput | Worker cycles/compile before -> after | Allocations/compile before -> after | Reallocations/compile before -> after | Allocated bytes/compile before -> after |
|---|---:|---:|---:|---:|---:|---:|---:|
| offline_basic | 4.32 | 4.41 | +2.06% | 505,141,826 -> 494,143,780.500 | 252,087 -> 252,087 | 13,076 -> 13,076 | 39,435,494 -> 39,435,494 |
| offline_capture | 1.85 | 1.85 | +0.17% | 1,180,663,071.500 -> 1,178,992,779.500 | 383,751 -> 383,743 | 21,826 -> 21,810 | 48,372,510 -> 48,367,262 |
| live_wrappers | 1.46 | 1.41 | -3.05% | 1,499,921,089.500 -> 1,548,139,754.500 | 710,927 -> 710,927 | 29,973 -> 29,973 | 120,102,065 -> 120,102,065 |
| live_actors | 1.66 | 1.68 | +1.34% | 1,315,921,380 -> 1,300,052,513.500 | 596,859 -> 596,859 | 25,957 -> 25,957 | 97,505,111 -> 97,505,111 |
| live_two_layers | 0.98 | 1.00 | +1.99% | 2,235,440,132 -> 2,191,385,062.500 | 1,165,376 -> 1,165,376 | 43,512 -> 43,512 | 201,733,393 -> 201,733,393 |
| live_timing4096 | 1.41 | 1.48 | +4.80% | 1,549,376,532 -> 1,479,601,976.500 | 718,953 -> 718,953 | 38,173 -> 38,173 | 121,210,000 -> 121,210,000 |
| offline_navigation_inputs | 2.75 | 2.65 | -3.74% | 794,194,138.500 -> 825,410,659 | 275,133 -> 275,132 | 16,550 -> 16,550 | 41,826,193 -> 41,826,073 |
| live_navigation_inputs | 2.91 | 3.03 | +3.92% | 750,176,038.500 -> 721,450,739.500 | 259,735 -> 259,735 | 16,568 -> 16,568 | 41,408,442 -> 41,408,442 |

| Complete workload | Median paired throughput | 95% pair-bootstrap band |
|---|---:|---:|
| offline_basic | +1.91% | -0.93% to +5.84% |
| offline_capture | -1.21% | -1.80% to +1.54% |
| live_wrappers | -1.58% | -11.57% to +1.46% |
| live_actors | +1.43% | -5.67% to +6.04% |
| live_two_layers | +1.24% | -1.91% to +7.76% |
| live_timing4096 | -0.38% | -4.45% to +7.79% |
| offline_navigation_inputs | -2.97% | -8.75% to -0.05% |
| live_navigation_inputs | +0.21% | -1.18% to +1.40% |

The offline dedicated fixture's primary pooled result (-3.74%) and paired result (-2.97%) prompted an independent confirmation with more frequent alternation. The original results above are unchanged. Six audited chunks of three alternating pairs retain three samples per variant/pair, giving 18 pairs and 54 samples per variant/mode (216 additional selected samples). All attempts remain in JSON.

| Interleaved confirmation workload | Before compiles/s | After compiles/s | Pooled throughput | Worker cycles/compile before -> after | Cycle change |
|---|---:|---:|---:|---:|---:|
| offline_navigation_inputs | 2.36 | 2.38 | +0.66% | 919,414,269.500 -> 917,734,992.500 | -0.18% |
| live_navigation_inputs | 2.65 | 2.63 | -0.54% | 823,227,826.500 -> 825,739,787 | +0.31% |

| Interleaved confirmation workload | Median paired throughput | 95% pair-bootstrap band |
|---|---:|---:|
| offline_navigation_inputs | +0.10% | -1.50% to +1.61% |
| live_navigation_inputs | -0.55% | -4.23% to +2.18% |

Confirmation chunks with observed compiler overlap or audit errors in their selected attempt: none. Every confirmation output fingerprint matches the primary campaign.

The confirmation uses the same saved binaries, sources and full-output fingerprints. Its bootstrap resamples 18 pair effects; these observations are separate from the primary campaign, rather than replacing it.

Paired tables compare medians within each alternating round or short batch. Deterministic bootstrap bands resample six pair effects 10,000 times; they are descriptive and do not account for all desktop activity or build-layout effects. Pooled and paired effects are both reported.

Compiler audit: original selected batches with observed compiler overlap or audit errors: navigation_inputs-pair4. Use the first attempt without observed overlap/error, otherwise the third; selection never uses performance values. The original campaign scopes are used.

Reproduction: preserve baseline `compile_pipeline` and library test executables. Build the candidate with `cargo +1.98.1`, `--release --locked --offline -j 2 --config profile.release.lto=false`. Run `scripts/benchmark_song_lua_compile.py --suite navigation_inputs --binary TEST_EXE --rounds 6 --out SCOPES_JSON`. Compare normal compiler binaries with `scripts/benchmark_song_lua_pipeline.py --suite startup` and `--suite navigation_inputs`; use six alternating pairs and nine samples per variant/case. Single-pair batches alternate `--reverse-first`.

Measurement limits:

- Windows QueryThreadCycleTime measures scheduled worker cycles including kernel execution, rather than user-only PMU instructions. CPU 2, 32 MiB stacks, rustc 1.98.1, release opt-level 3, disabled LTO and static MSVC runtime are shared across variants.
- Counted System allocation overhead is included. Scopes count current-thread Rust GlobalAlloc traffic, including mlua; normal whole-compile samples count all-process Rust allocation traffic. Native malloc, retained memory and RSS are excluded. Allocated bytes include reallocation traffic.
- Message/action scopes prepare VMs, clocks, actors and native callbacks outside timing. One warmup and full collection precede timing, then GC is stopped. Each retained sample runs 32 captures including snapshots/restoration, output clearing and temporary/result destruction; VM/input destruction is excluded.
- Reload scopes prepare a fresh retained scene outside each sample and consume it through the real reload path once, including context selection, file reads, host setup, Lua callbacks and compilation. Returned DTO/VM destruction is excluded, and the first of ten samples is discarded. GC behavior remains unchanged for reload.
- Complete fixtures measure compiler/startup work excluding final DTO/VM destruction and output formatting. Snapshot equality covers debug-visible compiled output; retained VM internals are tested behaviorally rather than serialized. Unit scopes use panic unwinding; normal compiler executables use panic abort. These measurements do not establish a game-frame or steady-state rendering improvement.
- One-second process snapshots audit cargo/rustc/cl/link/cmake/ninja. Short overlaps and other desktop activity may be missed; version probes can conservatively flag finished timing batches.

A pre-commit index-based queue candidate was superseded by direct source borrowing. Its three scoped attempts and two completed whole-compile pairs remain in the JSON evidence, with archived source/binary hashes. Its measurements are separate from the final candidate tables.

Evidence: [JSON](2026-10-22-song-lua-navigation-inputs.json) retains raw selected samples, every attempted batch/audit, source/binary/build-receipt hashes and full-output fingerprints. Final logs and complete snapshots remain under `target/song-lua-navigation-inputs-direct-campaign-20261022`; the superseded candidate remains under `target/song-lua-navigation-inputs-campaign-20261022`.
