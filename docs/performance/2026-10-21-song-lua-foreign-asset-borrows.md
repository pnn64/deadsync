# Song Lua foreign asset and sound-path borrows

Complete fixture throughput changes range from -1.41% to +2.22%. All complete outputs match exactly. Scoped effects and controls are reported below; these results do not establish a broad whole-app or frame-speed improvement.

Foreign sound setters change throughput by +35.06% (short) and +36.10% (long), with one allocation removed for nonempty strings. Sound-path scopes remove one or two allocations, but several latency measurements regress. The same-VM sound control measures -7.20% throughput (+14.09 ns/call) despite the unchanged same-VM branch. All cases are retained below.

Baseline: `494323364`. Three changes remove remaining temporary string copies:

1. Foreign-VM texture strings pass their validated borrowed UTF-8 view to the target table write. This preserves cross-VM copying into Lua while removing the intermediate Rust String. The same-VM direct-handle path is unchanged.
2. Foreign-VM sound-file strings use the same borrowed view. Validation still happens before the write, and the validation temporary remains alive through the write and its metamethod callback, matching the baseline.
3. Sound-path recording retains the lossy path view instead of forcing an owned String. Call-log and unique-path writes borrow that view, and ordered duplicate checks use the same bytes. Valid paths avoid the owned key and optional call-log clone; malformed Windows paths retain only their necessary lossy buffer.

The borrowed Lua string pins its source VM and reference slot without retaining a VM lock. Its immutable bytes remain valid while the target VM copies them. Embedded NULs and invalid UTF-8 preserve their prior length/error behavior. No new unsafe code.

Behavior checks compare both setters against frozen baseline bodies while collecting the source VM inside target metamethod callbacks, including failures, Unicode, embedded NULs and invalid UTF-8. Bounded pressure retains 5,000 source handles and creates 4,000 more inside the target callback, then checks recovery. The existing 336-case asset oracle also runs unchanged.

Sound-path checks compare 192 combinations of valid/missing/non-audio inputs, capture mode, call-log presence, path-table state, duplicate paths and malformed Windows directory names. They compare results and partial log/path/runtime-event changes even when conversion fails. The prepared runtime clock is reflected in captured event beats.

The added full fixture calls SOUND:PlayOnce and SOUND:PlayMusicPart 64 times each and verifies identical offline/live output with the registered sound path. Existing captured-event and repeated asset-setter fixtures remain in the paired set.

Reproduction: build the default-feature compile_pipeline example and library test target with cargo +1.98.1, --release --locked --offline -j 2 --config profile.release.lto=false and retain JSON artifact receipts. Run scripts/benchmark_song_lua_compile.py --suite foreign_assets --binary TEST_EXE --out SCOPES_JSON --rounds 6. Run scripts/benchmark_song_lua_pipeline.py against preserved executables with --suite startup, actor_state, finalization_events, asset_strings and sound_paths, --pairs 6 --samples 9. Recorded single-pair batches alternate --reverse-first and audit compiler processes.

Six alternating paired rounds, nine retained samples per variant/workload (54 per variant). Complete Debug DTO snapshots match exactly. Observed compiler-process overlaps retain and repeat complete batches; no performance-based rejection.

| Complete library workload | Before compiles/s | After compiles/s | Throughput | Worker cycles | Allocations before -> after | Allocated bytes before -> after |
|---|---:|---:|---:|---:|---:|---:|---:|
| offline_basic | 4.72 | 4.74 | +0.44% | -0.70% | 251,933 -> 251,933 | 39,421,957 -> 39,421,957 |
| offline_capture | 2.22 | 2.22 | +0.04% | +0.09% | 380,986 -> 380,986 | 48,100,574 -> 48,100,574 |
| live_wrappers | 1.70 | 1.69 | -0.19% | +0.18% | 708,190 -> 708,190 | 119,830,946 -> 119,830,946 |
| live_actors | 2.04 | 2.03 | -0.31% | +0.34% | 594,122 -> 594,122 | 97,233,992 -> 97,233,992 |
| live_two_layers | 1.16 | 1.16 | +0.40% | -0.46% | 1,162,633 -> 1,162,633 | 201,461,930 -> 201,461,930 |
| live_timing4096 | 1.69 | 1.67 | -1.41% | +1.42% | 716,189 -> 716,185 | 120,937,496 -> 120,937,283 |
| offline_actor_state | 2.16 | 2.16 | -0.08% | +0.04% | 386,987 -> 386,987 | 52,855,310 -> 52,855,310 |
| offline_sampling_buffers | 1.74 | 1.74 | -0.45% | +0.43% | 643,453 -> 643,453 | 77,915,792 -> 77,915,792 |
| offline_finalization_events | 1.40 | 1.41 | +0.76% | -0.79% | 505,802 -> 505,802 | 66,994,502 -> 66,994,502 |
| offline_asset_strings | 1.04 | 1.03 | -0.74% | +0.82% | 437,669 -> 437,669 | 82,222,122 -> 82,222,122 |
| live_asset_strings | 1.11 | 1.13 | +1.98% | -1.97% | 406,963 -> 406,963 | 81,475,481 -> 81,475,481 |
| offline_sound_paths | 2.79 | 2.78 | -0.52% | +0.46% | 268,514 -> 268,386 | 41,538,304 -> 41,529,216 |
| live_sound_paths | 3.14 | 3.21 | +2.22% | -1.31% | 255,838 -> 255,710 | 40,854,701 -> 40,845,613 |

| Scoped workload | Before units/s | After units/s | Throughput | Worker cycles | Allocations/unit before -> after | Allocated bytes/unit before -> after |
|---|---:|---:|---:|---:|---:|---:|---:|
| foreign_texture_short | 587,202 | 602,832 | +2.66% | -3.02% | 3.000 -> 2.000 | 72 -> 48 |
| foreign_texture_long | 142,161 | 146,703 | +3.20% | -3.20% | 3.000 -> 2.000 | 12,288 -> 8,192 |
| foreign_texture_empty | 871,332 | 895,736 | +2.80% | -2.83% | 0.000 -> 0.000 | 0 -> 0 |
| foreign_sound_short | 3,105,383 | 4,194,143 | +35.06% | -26.21% | 1.000 -> 0.000 | 24 -> 0 |
| foreign_sound_long | 1,384,719 | 1,884,583 | +36.10% | -26.28% | 1.000 -> 0.000 | 4,096 -> 0 |
| foreign_sound_empty | 4,767,364 | 4,986,153 | +4.59% | -4.60% | 0.000 -> 0.000 | 0 -> 0 |
| same_texture_control | 634,409 | 636,937 | +0.40% | -0.57% | 2.000 -> 2.000 | 48 -> 48 |
| same_sound_control | 5,505,006 | 5,108,824 | -7.20% | +8.53% | 0.000 -> 0.000 | 0 -> 0 |
| sound_path_new64 | 9,359 | 9,135 | -2.39% | +2.43% | 42.562 -> 40.562 | 4,080 -> 3,902 |
| sound_path_duplicate | 10,162 | 10,086 | -0.75% | +0.72% | 11.047 -> 9.047 | 1,219 -> 1,043 |
| sound_path_no_calls | 10,156 | 10,058 | -0.96% | +0.97% | 10.031 -> 9.031 | 1,100 -> 1,012 |
| sound_path_no_capture | 10,253 | 10,350 | +0.95% | -1.06% | 9.016 -> 8.016 | 888 -> 800 |
| sound_path_search128 | 8,053 | 8,053 | -0.01% | +0.01% | 138.047 -> 136.047 | 12,561 -> 12,381 |
| sound_path_missing | 26,094 | 25,576 | -1.99% | +1.79% | 10.016 -> 10.016 | 901 -> 901 |
| sound_path_not_audio | 10,542 | 10,340 | -1.91% | +2.30% | 7.016 -> 7.016 | 705 -> 705 |
| sound_path_lossy | 9,785 | 9,845 | +0.61% | -0.44% | 11.047 -> 10.047 | 1,291 -> 1,197 |

Validation: `test result: FAILED. 804 passed; 15 failed; 33 ignored; 0 measured; 0 filtered out; finished in 210.16s`. All 15 baseline failure diagnostics match after process/path/source-location normalization. The two new comparison tests pass. Existing complete compiler oracle and fixtures run in the full suite.

Evidence: [JSON](2026-10-21-song-lua-foreign-asset-borrows.json) contains 3132 retained samples, source/binary and receipt/log hashes, normalized baseline diagnostics and overlap audits. Raw measurements remain under target/foreign-asset-borrows-*.

Measurement limits:

- Default-feature normal library; root app test-support configuration and whole-app/FPS are not measured.
- QueryThreadCycleTime scheduled worker cycles including kernel execution, not hardware-PMU retired instructions.
- Whole-compile all-process Rust GlobalAlloc includes mlua; scoped counters are current-thread only. Native malloc excluded; allocation traffic is not retained memory/RSS. Counted System allocator overhead is included.
- Release opt-level3 with LTO disabled; rustc1.98.1; CPU2; 32MiB worker stacks. Units use panic=unwind; normal fixture binaries retain panic=abort. Both sides use the same configuration.
- Background desktop activity is not fully controlled. One-second process audits reject observed cargo/rustc/cl/link processes, not all interference. Version probes can also trigger conservative rejection after timing ends.
- Complete fixture timing covers compilation/startup. Eight live-frame advances in complete snapshot validation occur outside timing; whole-frame or game FPS gains are not established.
- Foreign-VM setter inputs normally come from Rust callers. Complete compiler fixtures use their own VM and therefore control the unchanged same-VM path; foreign cases are measured directly through the production setter.
- Sound-path scopes include real filesystem existence/type checks, path resolution, runtime capture at a prepared beat, call-log writes and ordered deduplication. They do not decode/play audio. Outputs persist until after timing. Missing-file and non-audio controls return before creating the path key.
- Malformed Windows paths retain the required lossy buffer; borrowing the key still removes the later call-log String clone. The prepared test runtime clock matches the normal capture path instead of constructing missing-runtime errors.
