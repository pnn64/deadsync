# Song Lua asset string conversions

Complete fixture throughput changes range from -1.73% to +2.81%. All complete outputs match exactly. Setter/path-key scopes and controls are shown below; these results do not establish a broad whole-app or frame-speed improvement.

Scoped sound setters improve throughput by 66.33% (short) and 134.89% (long), with 39.97% and 57.34% fewer worker cycles. Nonempty same-VM texture/sound writes remove one allocation per call; valid nonempty path keys go from two allocations to one. Foreign sound throughput changes by -0.83%, retaining its original allocation.

Baseline: `0c335501b`. Three changes remove temporary string copies:

1. The texture setter validates a Lua string, then writes its existing handle directly when the actor and value belong to the same VM. Foreign-VM values retain the original owned UTF-8 String conversion before the table write.
2. The sound-file setter uses the same direct-handle path for same-VM strings. The original nil/absence clearing rules and handling of other value kinds are unchanged.
3. Sprite, ActorMultiVertex and late Sprite texture keys share a small path-to-Arc helper. It copies from the lossy path view directly, removing the intermediate owned String for valid UTF-8 paths. Invalid Windows UTF-16 paths preserve replacement characters and their required lossy buffer.

Both setters retain the existing UTF-8 validation before writing, including binary-string errors. The validation handle remains alive through the table write and its metamethod callback, as does the original validation temporary. VM identity checks compare weak-pointer identity; foreign strings keep their cross-VM copy path. Metamethod access order, texture clearing, native texture-table handling and video classification remain on the original paths. No new unsafe code.

Behavior checks compare 336 combinations of setter, actor type, value, clearing and callback failure against the frozen helpers. They cover embedded NULs, Unicode, invalid UTF-8, foreign VMs, forced GC inside metamethod writes, native texture tables, exact errors/logs/raw fields and bounded reference pressure/recovery. Path checks cover empty, ASCII, Unicode, long and unpaired-surrogate Windows paths, including independent Arc storage.

The added startup fixture executes 1,024 native Sprite:SetTexture writes and 512 Sound:Load writes across 16 sprites and eight sounds. Both offline and live full outputs match the old executable. The existing captured-event fixture also matches all 72 messages, 288 sprite texture entries and 18 judgment entries.

Reproduction: build the default-feature compile_pipeline example and library test target with cargo +1.98.1, --release --locked --offline -j 2 --config profile.release.lto=false and retain their JSON artifact receipts. Run scripts/benchmark_song_lua_compile.py --suite asset_strings --binary TEST_EXE --out SCOPES_JSON --rounds 6. Run scripts/benchmark_song_lua_pipeline.py against preserved executables with --suite startup, actor_state, finalization_events and asset_strings, --pairs 6 --samples 9. Recorded single-pair batches alternate --reverse-first and audit compiler processes.

Six alternating paired rounds, nine retained samples per variant/workload (54 per variant). Complete Debug DTO snapshots match exactly. Observed compiler-process overlaps retain and repeat complete batches; no performance-based rejection.

| Complete library workload | Before compiles/s | After compiles/s | Throughput | Worker cycles | Allocations before -> after | Allocated bytes before -> after |
|---|---:|---:|---:|---:|---:|---:|---:|
| offline_basic | 4.59 | 4.72 | +2.81% | -2.67% | 251,933 -> 251,929 | 39,421,895 -> 39,421,747 |
| offline_capture | 2.22 | 2.18 | -1.73% | +1.79% | 379,071 -> 379,074 | 47,945,937 -> 47,946,046 |
| live_wrappers | 1.69 | 1.69 | -0.09% | +0.13% | 706,280 -> 706,291 | 119,676,489 -> 119,676,502 |
| live_actors | 2.02 | 2.01 | -0.43% | +0.44% | 592,212 -> 592,192 | 97,079,535 -> 97,078,497 |
| live_two_layers | 1.16 | 1.16 | -0.17% | +0.17% | 1,160,723 -> 1,160,705 | 201,307,473 -> 201,306,501 |
| live_timing4096 | 1.67 | 1.68 | +0.41% | -0.44% | 714,280 -> 714,258 | 120,783,064 -> 120,781,956 |
| offline_actor_state | 2.07 | 2.07 | +0.26% | -0.28% | 385,072 -> 385,075 | 52,700,673 -> 52,700,782 |
| offline_sampling_buffers | 1.67 | 1.69 | +1.12% | -1.20% | 643,458 -> 643,449 | 77,915,845 -> 77,915,582 |
| offline_finalization_events | 1.33 | 1.32 | -1.24% | +1.26% | 504,182 -> 503,845 | 66,841,790 -> 66,832,659 |
| offline_asset_strings | 1.02 | 1.02 | +0.26% | -0.26% | 437,262 -> 435,713 | 82,097,737 -> 82,065,894 |
| live_asset_strings | 1.11 | 1.11 | +0.42% | -0.40% | 406,605 -> 405,035 | 81,352,976 -> 81,320,052 |

| Scoped workload | Before units/s | After units/s | Throughput | Worker cycles | Allocations/unit before -> after | Allocated bytes/unit before -> after |
|---|---:|---:|---:|---:|---:|---:|---:|
| texture_short | 565,724 | 615,449 | +8.79% | -8.12% | 3.000 -> 2.000 | 72 -> 48 |
| texture_long | 145,843 | 152,729 | +4.72% | -4.31% | 3.000 -> 2.000 | 12,288 -> 8,192 |
| texture_empty | 881,377 | 892,302 | +1.24% | -1.16% | 0.000 -> 0.000 | 0 -> 0 |
| texture_foreign | 572,287 | 577,603 | +0.93% | -1.06% | 3.000 -> 3.000 | 72 -> 72 |
| texture_amv | 527,332 | 572,303 | +8.53% | -7.79% | 3.000 -> 2.000 | 72 -> 48 |
| sound_short | 3,054,095 | 5,079,838 | +66.33% | -39.97% | 1.000 -> 0.000 | 24 -> 0 |
| sound_long | 1,342,225 | 3,152,709 | +134.89% | -57.34% | 1.000 -> 0.000 | 4,096 -> 0 |
| sound_empty | 4,646,625 | 5,128,976 | +10.38% | -9.93% | 0.000 -> 0.000 | 0 -> 0 |
| sound_foreign | 3,132,456 | 3,106,560 | -0.83% | +0.94% | 1.000 -> 1.000 | 24 -> 24 |
| key_empty | 12,729,392 | 13,429,508 | +5.50% | -3.75% | 1.000 -> 1.000 | 16 -> 16 |
| key_ascii | 2,214,383 | 2,661,901 | +20.21% | -17.35% | 2.000 -> 1.000 | 63 -> 40 |
| key_unicode | 2,273,408 | 2,677,255 | +17.76% | -15.15% | 2.000 -> 1.000 | 59 -> 40 |
| key_long | 397,069 | 412,999 | +4.01% | -3.84% | 2.000 -> 1.000 | 8,237 -> 4,128 |
| key_lossy_utf16 | 6,619,531 | 6,638,843 | +0.29% | -0.08% | 2.000 -> 2.000 | 29 -> 29 |

Validation: `test result: FAILED. 802 passed; 15 failed; 31 ignored; 0 measured; 0 filtered out; finished in 208.84s`. All 15 baseline failure diagnostics match after process/path/source-location normalization. The two new comparison tests pass. Existing complete compiler oracle and fixtures run in the full suite.

Evidence: [JSON](2026-10-21-song-lua-asset-strings.json) contains 2700 retained samples, source/binary and receipt/log hashes, normalized baseline diagnostics and overlap audits. Raw measurements remain under target/asset-strings-*.

Measurement limits:

- Default-feature normal library; root app test-support configuration and whole-app/FPS are not measured.
- QueryThreadCycleTime scheduled worker cycles including kernel execution, not hardware-PMU retired instructions.
- Whole-compile all-process Rust GlobalAlloc includes mlua; scoped counters are current-thread only. Native malloc excluded; allocation traffic is not retained memory/RSS. Counted System allocator overhead is included.
- Release opt-level3 with LTO disabled; rustc1.98.1; CPU2; 32MiB worker stacks. Units use panic=unwind; normal fixture binaries retain panic=abort. Both sides use the same configuration.
- Background desktop activity is not fully controlled. One-second process audits reject observed cargo/rustc/cl/link processes, not all interference. Version probes can also trigger conservative rejection after timing ends.
- Complete fixture timing covers compilation/startup. Eight live-frame advances in complete snapshot validation occur outside timing; whole-frame or game FPS gains are not established.
- Setter scopes call the production Rust helper, including table writes and texture/video classification. Complete asset fixtures invoke native Lua methods during startup. Empty and foreign-VM controls retain their original allocation requirements.
- Valid UTF-8 path keys avoid the temporary owned String. Ill-formed Windows paths still require the original lossy replacement buffer; the Arc allocation remains for every key. Automatic Lua GC policy is unchanged.
