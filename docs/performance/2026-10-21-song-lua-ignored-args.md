# Song Lua unused callback arguments

Measured receiver calls improve throughput by 9.0–56.7% and remove one allocation per call. Complete library fixtures range from -2.88% to +3.28% throughput; this does not establish a broad compile-speed win. Eight-input callback cases retain small measured overheads. All complete outputs match the baseline.

Baseline: `1c744eebf`. Three changes remove receiver-buffer allocations from ignored callback inputs:

1. Actor query, command and texture callbacks in lua_util.rs (139 declarations).
2. Host/global query callbacks in host.rs (63 declarations).
3. Compatibility/query-table callbacks in tables.rs (144 declarations).

Callback bodies and consumed-input methods are unchanged. The private input type stores zero inputs as Empty and one checked Value inline. Two or more inputs delegate to the original MultiValue conversion. All converted values remain owned through the callback, preserving lifetimes, conversion order, errors and Rust panic propagation from extra inputs. It neither skips extra-value validation nor changes Lua stack limits. The unsafe override follows mlua FromLuaMulti’s stack contract and calls only its checked conversions.

An initial SmallVec prototype improved receiver calls but slowed multi-input cases by 8–12%; it was rejected. The final zero/one specialization was benchmarked and behavior-checked before production edits. Its prototype improves the one-receiver case by about 52%, removing one allocation; the larger-list prototype cases retain small measured overhead. Source verification proves the production storage/implementation matches that prototype and all 346 callback changes affect declarations only.

Six alternating paired rounds, nine retained samples per variant/workload (54 per variant). All complete Debug DTO snapshots match exactly. One-second parent-process Windows Toolhelp audits retain/repeat any scoped run or whole pair observing cargo/rustc/cl/link. No performance-based sample rejection.

| Complete library workload | Before compiles/s | After compiles/s | Throughput | Worker cycles | Allocations before → after |
|---|---:|---:|---:|---:|---:|
| offline_basic | 4.449 | 4.422 | -0.61% | +0.55% | 252,722 → 252,661 |
| offline_capture | 2.259 | 2.293 | +1.49% | -1.35% | 377,754 → 377,693 |
| live_wrappers | 1.659 | 1.634 | -1.49% | +2.12% | 707,320 → 706,998 |
| live_actors | 2.094 | 2.033 | -2.88% | +2.49% | 593,167 → 593,104 |
| live_two_layers | 1.151 | 1.189 | +3.28% | -2.90% | 1,164,971 → 1,164,397 |
| live_timing4096 | 1.725 | 1.757 | +1.84% | -1.81% | 715,298 → 714,993 |
| offline_actor_state | 2.316 | 2.323 | +0.31% | -0.33% | 383,427 → 383,350 |
| offline_sampling_buffers | 1.699 | 1.707 | +0.46% | -0.38% | 645,701 → 645,640 |

| Production callback / input shape | Throughput | Worker cycles | Allocations/call before → after |
|---|---:|---:|---:|
| actor_width_empty0 | +1.58% | -1.69% | 0.00 → 0.00 |
| actor_width_receiver1 | +8.98% | -8.25% | 1.00 → 0.00 |
| actor_width_extra8 | -1.24% | +1.38% | 1.00 → 1.00 |
| host_top_screen_empty0 | -0.04% | +0.05% | 0.00 → 0.00 |
| host_top_screen_receiver1 | +13.36% | -10.99% | 1.00 → 0.00 |
| host_top_screen_extra8 | -0.29% | +0.06% | 1.00 → 1.00 |
| style_width_empty0 | +13.40% | -13.35% | 0.00 → 0.00 |
| style_width_receiver1 | +56.69% | -36.17% | 1.00 → 0.00 |
| style_width_extra8 | -1.85% | +1.31% | 1.00 → 1.00 |

Scoped tests call the actual registered GetWidth (actor/style) and SCREENMAN.GetTopScreen callbacks from prepared Lua loops, using zero, one receiver, or eight mixed inputs. Each measured batch makes 8192 calls; full warmup, VM/receiver construction and destruction are excluded. GC is stopped. Argument/result drops are included. Whole compiles retain normal GC and include host construction, initialization/startup, extraction and finalization; returned DTO/VM destruction and Debug validation are outside timing.

Validation: `test result: FAILED. 792 passed; 15 failed; 24 ignored; 0 measured; 0 filtered out; finished in 214.25s` All 15 baseline diagnostics match after normalizing source positions and process/thread IDs. Four new checks cover checked values/order, nils/binary strings/coroutines, reference pressure/recovery, caught Rust panic inputs, real actor getter values/errors, host screen identity/loading/side effects, and style getters/unchanged consumed-argument methods.

Reproduction (PowerShell; preserve parent executable first):

```powershell
cargo +1.98.1 build -p deadsync-song-lua --example compile_pipeline --release --locked --offline -j 2 --config profile.release.lto=false --message-format=json > target/song-lua-ignored-args-build.jsonl
cargo +1.98.1 test -p deadsync-song-lua --lib --release --locked --offline -j 2 --config profile.release.lto=false --no-run --message-format=json > target/song-lua-ignored-args-test-build.jsonl
python scripts/benchmark_song_lua_compile.py --suite ignored_args --binary target/release/deps/deadsync_song_lua-df5ce19c80cb89a8.exe --out target/ignored-args-scopes.json --rounds 6
python scripts/benchmark_song_lua_pipeline.py --suite startup --before target/ignored-args-2026-10-21/before.exe --after target/ignored-args-2026-10-21/after.exe --out target/ignored-args-pairs --pairs 6 --samples 9
python scripts/benchmark_song_lua_pipeline.py --suite actor_state --before target/ignored-args-2026-10-21/before.exe --after target/ignored-args-2026-10-21/after.exe --out target/ignored-args-extra-pairs --pairs 6 --samples 9
```

The companion JSON embeds all 1,836 authoritative samples (864 whole compiles and 972 callback batches), both prototypes, source/binary/build hashes, baseline receipt, validation and build audits. Raw reports/DTOs remain in `target/ignored-args-pairs`, `target/ignored-args-extra-pairs` and `target/ignored-args-scopes.json`.

- Default-feature normal library; root app test-support configuration and whole-app/FPS are not measured.
- QueryThreadCycleTime scheduled worker cycles including kernel execution, not hardware-PMU retired instructions.
- Whole-compile all-process Rust GlobalAlloc includes mlua; scoped counters are current-thread only. Native malloc excluded; allocation traffic is not retained memory/RSS. Counted System allocator overhead is included.
- Release opt-level3 with LTO disabled; rustc1.98.1; CPU2; 32MiB worker stacks. Scoped unit/prototype uses panic=unwind; normal fixture binaries retain panic=abort. Both sides of each comparison use the same configuration.
- Background desktop activity is not fully controlled. One-second process audits reject observed compiler overlap, not all interference.
- Zero/one inputs are specialized. Larger ignored argument lists preserve the original MultiValue buffer/conversion path and can retain small dispatch/tag overhead; actual zero/one/eight-input scopes are all reported.
