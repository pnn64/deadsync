# Song Lua inline comparison text

Complete fixture throughput changes range from -1.87% to +5.17%. All complete outputs match exactly. Scoped effects and fallback controls are reported below; these results do not establish a broad whole-app or frame-speed improvement.

Native texture tags change throughput by +6.19% with one allocation removed. A 64-name probe changes throughput by +92.79% and removes all 64 string allocations. A 128-entry sound scan changes throughput by +5.57% and removes 128 allocations. Long fallback controls regress: tag -5.20%, probe -14.71%, and path scan -9.33%. All controls remain in the evidence.

Baseline: `142996521`. Three comparison paths reuse the existing validated inline text reader:

1. Ordered duplicate checks in sound-path recording read up to 256 UTF-8 bytes inline instead of allocating a Rust String per visited entry. The source table is still scanned in the original order, with the same early match and error behavior.
2. ActorMultiVertex native texture-tag checks read up to 32 bytes inline. Tag matching remains case-sensitive and raw, and the original Lua texture-table identity is retained.
3. Function-ease probe classification reads up to 32 bytes inline, then borrows the validated text for its existing split/match logic. It retains early returns before later invalid sequence values.

The existing LuaFieldText implementation keeps String coercions and diagnostics, including direct-stack UTF-8 errors, and falls back to owned storage for longer strings. Its implementation and unsafe code are unchanged. No new unsafe code.

Comparison checks cover 729 two-value probe sequences, sparse/empty lists, raw native tags, native texture identity, missing values, numeric coercion, booleans/tables, embedded NULs and invalid UTF-8. Boundary/long cases include 31/32/33-byte probe/tag values and 255/256/257/4096-byte path entries. Sound tests compare partial capture/log/path state and errors before and after early matches. Existing 336-case setter and 192-case sound recording oracles also run unchanged except fixture-helper visibility.

The new full fixture writes a native texture handle to a mesh 128 times and declares 32 player-X function eases. Both offline and live snapshots retain one textured mesh; the offline compiler bakes all 32 eases with zero unsupported captures. Full snapshots match the old executable.

Reproduction: build the default-feature compile_pipeline example and library test target with cargo +1.98.1, --release --locked --offline -j 2 --config profile.release.lto=false and retain JSON artifact receipts. Run scripts/benchmark_song_lua_compile.py --suite comparison_text --binary TEST_EXE --out SCOPES_JSON --rounds 6. Run scripts/benchmark_song_lua_pipeline.py against preserved executables with --suite startup, actor_state, finalization_events, asset_strings, sound_paths and comparison_text, --pairs 6 --samples 9. Recorded single-pair batches alternate --reverse-first and audit compiler processes.

Six alternating paired rounds, nine retained samples per variant/workload (54 per variant). Complete Debug DTO snapshots match exactly. Observed compiler-process overlaps retain and repeat complete batches; no performance-based rejection.

| Complete library workload | Before compiles/s | After compiles/s | Throughput | Worker cycles | Allocations before -> after | Allocated bytes before -> after |
|---|---:|---:|---:|---:|---:|---:|---:|
| offline_basic | 4.65 | 4.56 | -1.87% | +1.97% | 251,928 -> 251,933 | 39,421,769 -> 39,421,949 |
| offline_capture | 2.12 | 2.16 | +1.71% | -1.67% | 381,756 -> 381,756 | 48,177,187 -> 48,177,187 |
| live_wrappers | 1.63 | 1.64 | +0.68% | -0.68% | 708,960 -> 708,950 | 119,907,559 -> 119,907,066 |
| live_actors | 1.95 | 1.94 | -0.80% | +0.84% | 594,892 -> 594,892 | 97,310,605 -> 97,310,605 |
| live_two_layers | 1.13 | 1.13 | +0.01% | -0.02% | 1,163,403 -> 1,163,403 | 201,538,543 -> 201,538,543 |
| live_timing4096 | 1.61 | 1.62 | +0.73% | -0.71% | 716,955 -> 716,959 | 121,013,896 -> 121,014,109 |
| offline_actor_state | 2.14 | 2.16 | +1.01% | -1.02% | 387,757 -> 387,757 | 52,931,923 -> 52,931,923 |
| offline_sampling_buffers | 1.77 | 1.80 | +2.11% | -2.09% | 643,447 -> 643,459 | 77,915,557 -> 77,915,946 |
| offline_finalization_events | 1.34 | 1.37 | +2.13% | -2.09% | 506,571 -> 506,569 | 67,070,955 -> 67,070,635 |
| offline_asset_strings | 0.98 | 0.99 | +0.77% | -0.87% | 438,439 -> 438,406 | 82,298,735 -> 82,297,417 |
| live_asset_strings | 1.04 | 1.04 | +0.35% | -0.75% | 407,733 -> 407,733 | 81,552,094 -> 81,552,094 |
| offline_sound_paths | 2.50 | 2.63 | +5.17% | -4.92% | 269,156 -> 269,029 | 41,605,829 -> 41,596,812 |
| live_sound_paths | 2.90 | 2.94 | +1.34% | -1.31% | 256,480 -> 256,353 | 40,922,226 -> 40,913,209 |
| offline_comparison_text | 2.24 | 2.27 | +1.29% | -1.25% | 318,805 -> 318,645 | 44,749,706 -> 44,748,042 |
| live_comparison_text | 2.69 | 2.66 | -1.30% | +1.31% | 267,314 -> 267,186 | 41,759,610 -> 41,758,202 |

| Scoped workload | Before units/s | After units/s | Throughput | Worker cycles | Allocations/unit before -> after | Allocated bytes/unit before -> after |
|---|---:|---:|---:|---:|---:|---:|---:|
| tag_native | 871,457 | 925,399 | +6.19% | -5.57% | 1.001 -> 0.001 | 11 -> 0 |
| tag_unknown | 751,801 | 796,833 | +5.99% | -5.36% | 3.000 -> 2.000 | 332 -> 320 |
| tag_long | 624,034 | 591,566 | -5.20% | +5.57% | 3.000 -> 3.000 | 4,416 -> 4,416 |
| tag_empty | 794,823 | 799,602 | +0.60% | -0.66% | 2.000 -> 2.000 | 320 -> 320 |
| probe_empty | 11,863,867 | 12,076,362 | +1.79% | -1.54% | 0.000 -> 0.000 | 0 -> 0 |
| probe_one | 4,527,343 | 6,366,674 | +40.63% | -28.89% | 1.000 -> 0.000 | 8 -> 0 |
| probe_repeat64 | 115,240 | 222,172 | +92.79% | -48.15% | 64.000 -> 0.000 | 512 -> 0 |
| probe_mixed12 | 563,832 | 972,200 | +72.43% | -42.08% | 12.000 -> 0.000 | 96 -> 0 |
| probe_unknown_first | 5,639,155 | 8,659,162 | +53.55% | -34.46% | 1.000 -> 0.000 | 9 -> 0 |
| probe_long | 1,335,507 | 1,139,035 | -14.71% | +17.58% | 1.000 -> 1.000 | 4,096 -> 4,096 |
| probe_number | 1,174,034 | 1,137,391 | -3.12% | +3.49% | 2.000 -> 2.000 | 18 -> 18 |
| scan_empty | 9,546 | 9,562 | +0.17% | -0.13% | 8.062 -> 7.078 | 997 -> 910 |
| scan_one | 9,839 | 9,900 | +0.62% | -0.70% | 8.047 -> 7.047 | 996 -> 908 |
| scan_128 | 8,275 | 8,736 | +5.57% | -5.46% | 135.062 -> 7.062 | 12,335 -> 925 |
| scan_long_128 | 6,652 | 6,031 | -9.33% | +10.33% | 136.047 -> 135.062 | 525,347 -> 525,261 |

Validation: `test result: FAILED. 807 passed; 15 failed; 35 ignored; 0 measured; 0 filtered out; finished in 206.89s`. All 15 baseline failure diagnostics match after process/path/source-location normalization. The three new comparison tests pass. Existing complete compiler oracle and fixtures run in the full suite.

Evidence: [JSON](2026-10-21-song-lua-inline-comparison-text.json) contains 3240 retained samples, source/binary and receipt/log hashes, normalized baseline diagnostics and overlap audits. Raw measurements remain under target/inline-comparison-text-*.

Measurement limits:

- Default-feature normal library; root app test-support configuration and whole-app/FPS are not measured.
- QueryThreadCycleTime scheduled worker cycles including kernel execution, not hardware-PMU retired instructions.
- Whole-compile all-process Rust GlobalAlloc includes mlua; scoped counters are current-thread only. Native malloc excluded; allocation traffic is not retained memory/RSS. Counted System allocator overhead is included.
- Release opt-level3 with LTO disabled; rustc1.98.1; CPU2; 32MiB worker stacks. Units use panic=unwind; normal fixture binaries retain panic=abort. Both sides use the same configuration.
- Background desktop activity is not fully controlled. One-second process audits reject observed cargo/rustc/cl/link processes, not all interference. Version probes can also trigger conservative rejection after timing ends.
- Complete fixture timing covers compilation/startup. Eight live-frame advances in complete snapshot validation occur outside timing; whole-frame or game FPS gains are not established.
- Inline text capacity is 32 bytes for tags/probe names and 256 bytes for sound-path comparisons. Longer strings spill to heap storage and still perform UTF-8 validation; numeric/non-string inputs retain the original String conversion fallback.
- Sound scopes include real filesystem checks, path resolution and captured events. They reuse the existing prepared runtime clock/fixture helper and exclude setup/output/VM destruction. No audio decoding or playback is measured.
- The existing inline reader and its unsafe implementation are unchanged. This pass changes only its safe call sites; long, empty and numeric controls are reported alongside short-string cases.
