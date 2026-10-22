# Song Lua captured event finalization

The captured-event fixture changes throughput by +0.22% and worker cycles by -0.22%. Standard complete fixtures range from -1.49% to +4.21%. All complete outputs match exactly. Scoped improvements and temporary-allocation costs are shown below.

Baseline: `87e58ae89`. Three changes consume and organize captured events before assigning final actors:

1. Sprite textures are grouped by actor pointer and stably sorted once. Unique actors take ownership of their event list. Earlier aliases receive clones and the last alias takes ownership, preserving independent Arc lists for every destination. Empty captures clear prior bindings without reading table pointers.
2. Judgment textures use the fixed player array. Each list is stably sorted and consecutively deduplicated once, then moved into its final judgment destination. Assignments remain in original tracked-actor order so a later Player assignment still overwrites an earlier judgment.
3. Larger runtime message merges index borrowed message names and mutable beat fields. Each name has a stable beat ordering; removing the last eligible entry preserves original last-index tie breaking. Windows below 32 actions or 16 broadcasts retain the existing scan, and empty broadcasts skip its allocation. Unmatched events append in runtime order after releasing the borrowed index. No new unsafe code.

Behavior checks compare extracted baseline helpers, including aliased sprite tables, empty/orphan captures, signed zero and NaN ordering, judgment deduplication and player overwrites. Message comparisons cover 84 generated combinations with eight input sets each (672 comparisons), reserved names, frozen action windows, prefixes, persistence flags, ties, signed zeros, infinities and NaN payload bits. The added complete fixture records 72 messages, 288 sprite texture entries and 18 judgment entries; its entire DTO matches the old executable.

Reproduction: build the default-feature compile_pipeline example and library test target with cargo +1.98.1, --release --locked --offline -j 2 --config profile.release.lto=false and retain their JSON artifact receipts. Run scripts/benchmark_song_lua_compile.py --suite finalization_events --binary TEST_EXE --out SCOPES_JSON --rounds 6. Run scripts/benchmark_song_lua_pipeline.py against preserved before/after executables with --suite startup, actor_state and finalization_events, --pairs 6 --samples 9. The recorded runs split pairs into audited single-pair batches and alternate --reverse-first.

Six alternating paired rounds, nine retained samples per variant/workload (54 per variant). Complete Debug DTO snapshots match exactly. Compiler-overlap audits retain and repeat complete batches; no performance-based rejection.

| Complete library workload | Before compiles/s | After compiles/s | Throughput | Worker cycles | Allocations before -> after | Allocated bytes before -> after |
|---|---:|---:|---:|---:|---:|---:|---:|
| offline_basic | 4.52 | 4.71 | +4.21% | -4.01% | 251,933 -> 251,933 | 39,421,931 -> 39,421,945 |
| offline_capture | 2.26 | 2.26 | +0.03% | -0.03% | 378,452 -> 378,451 | 47,886,372 -> 47,886,363 |
| live_wrappers | 1.71 | 1.70 | -0.39% | +0.51% | 705,689 -> 705,656 | 119,617,755 -> 119,616,768 |
| live_actors | 2.06 | 2.07 | +0.14% | -0.09% | 591,581 -> 591,588 | 97,019,419 -> 97,019,814 |
| live_two_layers | 1.18 | 1.16 | -0.98% | +1.04% | 1,160,092 -> 1,160,099 | 201,247,357 -> 201,247,752 |
| live_timing4096 | 1.72 | 1.70 | -0.89% | +0.94% | 713,660 -> 713,655 | 120,723,541 -> 120,723,318 |
| offline_actor_state | 2.20 | 2.16 | -1.49% | +1.58% | 384,453 -> 384,452 | 52,641,101 -> 52,641,099 |
| offline_sampling_buffers | 1.70 | 1.70 | -0.06% | +0.16% | 643,460 -> 643,453 | 77,915,930 -> 77,915,780 |
| offline_finalization_events | 1.41 | 1.42 | +0.22% | -0.22% | 503,855 -> 503,563 | 66,798,868 -> 66,782,376 |

| Scoped workload | Before units/s | After units/s | Throughput | Worker cycles | Allocations/unit before -> after | Allocated bytes/unit before -> after |
|---|---:|---:|---:|---:|---:|---:|---:|
| sprites_empty128 | 26,497 | 34,353 | +29.65% | -23.07% | 128.000 -> 128.000 | 2,048 -> 2,048 |
| sprites_one8 | 554,593 | 798,005 | +43.89% | -31.77% | 10.000 -> 3.000 | 1,944 -> 1,956 |
| sprites_distinct32_512 | 7,579 | 15,091 | +99.12% | -49.83% | 576.000 -> 69.000 | 136,082 -> 129,580 |
| sprites_alias64_512 | 137 | 210 | +52.78% | -34.60% | 32960.000 -> 32386.000 | 12,259,456 -> 6,593,090 |
| sprites_orphan128_512 | 7,797 | 9,846 | +26.29% | -20.65% | 128.000 -> 135.000 | 2,048 -> 22,988 |
| judgment_empty2 | 960,961 | 996,885 | +3.74% | -3.90% | 0.000 -> 0.000 | 14 -> 14 |
| judgment_one128 | 51,721 | 72,218 | +39.63% | -28.42% | 66.000 -> 2.000 | 16,518 -> 15,046 |
| judgment_two512 | 7,984 | 13,412 | +67.98% | -40.50% | 516.000 -> 4.000 | 134,030 -> 122,254 |
| judgment_alias8_512 | 2,050 | 4,252 | +107.46% | -51.81% | 2064.000 -> 1546.000 | 536,126 -> 280,510 |
| messages_empty512 | 3,809,524 | 6,666,667 | +75.00% | -65.51% | 1.000 -> 0.000 | 512 -> 0 |
| messages_small8_8 | 2,253,521 | 2,352,941 | +4.41% | -4.20% | 1.000 -> 1.000 | 8 -> 8 |
| messages_edge32_16 | 387,409 | 490,798 | +26.69% | -21.45% | 1.000 -> 6.000 | 32 -> 908 |
| messages_32_32 | 268,908 | 349,727 | +30.05% | -23.67% | 1.000 -> 11.000 | 32 -> 1,452 |
| messages_64_64 | 72,993 | 198,634 | +172.13% | -63.55% | 1.000 -> 11.000 | 64 -> 1,964 |
| messages_one512_512 | 1,216 | 34,239 | +2716.33% | -96.45% | 1.000 -> 2.000 | 512 -> 8,340 |
| messages_many512_512 | 1,323 | 28,204 | +2032.17% | -95.34% | 1.000 -> 70.000 | 512 -> 16,572 |
| messages_missing512_512 | 1,761 | 16,226 | +821.55% | -89.22% | 513.000 -> 583.000 | 38,876 -> 87,576 |

Validation: `test result: FAILED. 800 passed; 15 failed; 30 ignored; 0 measured; 0 filtered out; finished in 212.59s`. All 15 baseline failure diagnostics match after process/path/source-location normalization. The two new comparison tests pass. Existing complete compiler oracle and fixtures run in the full suite.

Evidence: [JSON](2026-10-21-song-lua-finalization-events.json) contains 2808 retained samples, source/binary and receipt/log hashes, normalized baseline diagnostics and overlap audits. Raw measurements remain under target/finalization-events-*.

Measurement limits:

- Default-feature normal library; root app test-support configuration and whole-app/FPS are not measured.
- QueryThreadCycleTime scheduled worker cycles including kernel execution, not hardware-PMU retired instructions.
- Whole-compile all-process Rust GlobalAlloc includes mlua; scoped counters are current-thread only. Native malloc excluded; allocation traffic is not retained memory/RSS. Counted System allocator overhead is included.
- Release opt-level3 with LTO disabled; rustc1.98.1; CPU2; 32MiB worker stacks. Units use panic=unwind; normal fixture binaries retain panic=abort. Both sides use the same configuration.
- Background desktop activity is not fully controlled. One-second process audits reject observed cargo/rustc/cl/link processes, not all interference. Version probes can also trigger this conservative rejection after timing ends.
- Complete fixture timing covers compilation/startup. Eight live-frame advances in complete snapshot validation occur outside timing; whole-frame or game FPS gains are not established.
- Direct message timing uses runtime beats after all scheduled beats, so removals come from the ends of candidate vectors. Chronological interleaving is exercised in the complete captured-event fixture; out-of-order/float-edge behavior is compared in the generated test.
- Sprite/judgment grouping affects offline finalization. Larger message windows use a borrowed-name index; small windows keep the original scan. Empty broadcasts return immediately. The added fixture emits broadcasts without listener side effects; indexed matching is also measured directly in helper scopes. Grouping/indexing adds temporary buffers, and removing an indexed candidate can shift later entries within its name group; all measured small and unmatched cases are reported.
