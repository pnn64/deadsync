# Rejected native argument-buffer experiment

Rejected the native argument-buffer change after its consistently slower native-loader scope. Snapshot and restore filters are retained; the third optimization is replaced by command-name scanning. The recorded source patch and new source files reconstruct this uncommitted candidate from its parent. The measurement tables below are retained as experimental evidence; they do not describe the final applied change set.

# Song Lua actor-state keys and native argument forwarding

Baseline: `0908f3cff`. Three changes preserve the existing snapshot formats and restore policy:

1. Actor vector and Lua-table snapshots classify validated keys on the rooted VM string, copying Rust text only for retained fields. Callback traversal also removes iterator cursor handle churn.
2. Scalar-global restore validates/hashes rooted key bytes directly and retains only removal handles, collecting all removals before any writes.
3. Native Overlay, StepStatistics and NoteField `LoadActor` callbacks remove the path with `pop_front` and forward their existing `MultiValue`, removing the trailing-argument collection.

The first two helpers use the unchanged stack-restoring read-only VM guard. Each reserves one slot, pushes a checked string, validates UTF-8 and performs only Rust key classification/copying/hashing. No pointer or borrowed VM text escapes; Lua writes and authored callbacks remain outside the scope. Snapshot key reads use the actor owning VM, preserving the previous cross-VM behavior for supported simple actors. Invalid UTF-8 retains the same conversion error, including excluded/non-scalar keys. Snapshot order, clone policy, live destination overlay and wrapper identity remain unchanged.

Six alternating paired rounds with nine retained samples per variant/workload (54 per variant). Every whole-compile Debug DTO snapshot matches exactly across all runs. A parent-process Windows Toolhelp audit checks for cargo/rustc/cl/link every second; scoped runs and complete one-pair batches observing a build are retained separately and repeated. Only batches with no observed build enter the reported medians. The observer is outside the measured worker process/counters. The runner reverse-first flag lets short independent batches retain alternating order; the Rust benchmark harness remains unchanged.

All original six complete workloads are retained. Their allocation deltas showed little snapshot-path coverage, so two additional offline cases were measured afterward: a bounded synthetic 16-actor message/wrapper capture fixture and the existing sampling_buffers.lua update/message fixture. The Python runner changed only by adding this case selection; its earlier source/hash and an exact configuration-only comparison are retained. Both datasets use the same verified Rust binaries, harness and counters. The synthetic fixture pilot produced matching complete DTOs, real message captures and no skipped captures; its concurrent pilot timings are excluded.

| Complete library workload | Before compiles/s | After compiles/s | Throughput | Worker cycles | Rust allocations before → after |
|---|---:|---:|---:|---:|---:|
| offline_basic | 4.689 | 4.415 | -5.86% | +6.29% | 252,730 → 252,708 |
| offline_capture | 2.356 | 2.427 | +2.99% | -2.92% | 376,784 → 376,586 |
| live_wrappers | 1.773 | 1.780 | +0.40% | -0.40% | 706,084 → 706,062 |
| live_actors | 2.189 | 2.167 | -1.01% | +0.98% | 591,935 → 591,913 |
| live_two_layers | 1.181 | 1.184 | +0.29% | -0.27% | 1,163,739 → 1,163,717 |
| live_timing4096 | 1.774 | 1.764 | -0.60% | +0.62% | 714,097 → 714,047 |
| offline_actor_state | 2.257 | 2.305 | +2.11% | -2.05% | 389,939 → 382,525 |
| offline_sampling_buffers | 1.701 | 1.660 | -2.46% | +2.29% | 646,896 → 646,473 |

| Production-helper scope | Throughput | Worker cycles | Allocations/op before → after |
|---|---:|---:|---:|
| actor_vector_snapshot | +122.76% | -54.92% | 158.02 → 23.00 |
| actor_table_snapshot | +103.45% | -51.65% | 158.00 → 27.00 |
| restore_scalar_globals | +110.72% | -52.65% | 170.02 → 3.00 |
| native_loadactor | -3.69% | +3.64% | 173.05 → 172.05 |

The native-loader scope is an allocation/code-removal tradeoff: 1 fewer allocation and 232 fewer allocated bytes per call, with -3.69% throughput and +3.64% worker cycles. No speedup is claimed for that scope. The table reports the observed production-helper result rather than substituting an isolated argument-buffer microbenchmark.

Actor scopes include deep native values and identity-preserving wrapper membership. The native scope invokes the actual callback/resource parser with disabled ColumnCues settings; it is not a standalone Vec microbenchmark. VM/input construction is excluded and GC is stopped. Temporary/result drops are included; non-restore scopes also include the same fixed drop of 64 prepared scalar snapshots captured by the test closure. VM/other input destruction is excluded. Whole compiles retain their normal GC behavior. Whole timings include host creation, Lua initialization/startup, extraction and finalization; returned DTO/VM destruction and Debug validation are outside timing.

Validation: `test result: FAILED. 788 passed; 15 failed; 21 ignored; 0 measured; 0 filtered out; finished in 230.77s` All 15 failure diagnostics match the baseline after normalizing source locations and process/thread IDs. Four new differential checks cover order, deep values, wrapper identity, live destination state, cross-VM compatibility for simple actors, UTF-8 errors and stack restoration, validation-before-write behavior/authored setters, native forwarding/path errors. The existing 10,000-node snapshot test also passes.

Pre-measurement review caught the cross-VM case while the first unit binary was compiling. The verified stale compiler was stopped, receipts retained, the key read corrected and both candidate binaries rebuilt. All reported authoritative timings use the corrected source.

The first full suite then caught an incomplete native-loader test fixture (missing SL.Global.GameMode) before forwarding was reached. Only that fixture line changed; the unit binary was rebuilt and the full suite repeated. The incomplete-fixture run and receipts are retained separately; no authoritative timing used it.

Prototype evidence was collected before adding raw helpers to production. Final canonical comparison verifies both helper bodies against that prototype and four frozen helpers against the baseline commit. The prototype was rebuilt/repeated after formatting so the committed source and executable hashes also match the final receipt.

Reproduction (PowerShell):

```powershell
cargo +1.98.1 build -p deadsync-song-lua --example compile_pipeline --release --locked --offline -j 2 --config profile.release.lto=false --message-format=json > target/song-lua-actor-state-build.jsonl
cargo +1.98.1 test -p deadsync-song-lua --lib --release --locked --offline -j 2 --config profile.release.lto=false --no-run --message-format=json > target/song-lua-actor-state-test-build.jsonl
python scripts/benchmark_song_lua_compile.py --suite actor_state --binary target/release/deps/deadsync_song_lua-df5ce19c80cb89a8.exe --out target/actor-state-scopes.json --rounds 6
python scripts/benchmark_song_lua_pipeline.py --suite startup --before target/actor-state-2026-10-21/before.exe --after target/actor-state-2026-10-21/after.exe --out target/actor-state-pairs --pairs 6 --samples 9
python scripts/benchmark_song_lua_pipeline.py --suite actor_state --before target/actor-state-2026-10-21/before.exe --after target/actor-state-2026-10-21/after.exe --out target/actor-state-extra-pairs --pairs 6 --samples 9
```

Raw timing/sample receipts and full Debug snapshots: `target/actor-state-pairs`, `target/actor-state-extra-pairs`, `target/actor-state-scopes.json`, `target/state-key-prototype*`. The companion committed JSON embeds all sample metrics, executable/source/build receipt hashes, baseline receipt and validation diagnostics.

- Default-feature normal library, not root application test-support configuration or whole-app/FPS measurement.
- QueryThreadCycleTime scheduled worker cycles including kernel execution, not a hardware PMU instruction counter.
- Whole-compile all-process Rust GlobalAlloc includes mlua; scoped counters are current-thread only. Direct native malloc excluded; allocation traffic is not retained memory/RSS.
- Release opt-level3 with LTO disabled; pinned rustc1.98.1; CPU2; 32MiB worker stacks; counted System allocator overhead included.
- Early raw-helper prototype was measured before production edits. Source was subsequently rustfmt-formatted only; final prototype rebuild/repeat ties the committed source hash to its binary. Both raw helpers are canonically identical to production.
- No new cache, membership index, snapshot format or allocation-size assumptions; public snapshot nested table shape remains unchanged.
