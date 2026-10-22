# Song Lua snapshot, restore and command-name keys

The four helper scopes improve throughput by 61.5–110.8% and remove 137–164 allocations per operation. Complete library fixtures range from -3.93% to +1.54% throughput, so this pass does not establish a broad compile-speed win. All complete DTO snapshots match the baseline.

Baseline: `0908f3cff`. Three changes avoid temporary key wrappers while preserving snapshot formats and traversal policy:

1. Actor vector and Lua-table snapshots classify validated rooted keys and copy Rust text only for retained fields. Callback traversal removes iterator cursor handle churn.
2. Scalar-global restore validates/hashes rooted key bytes and retains only removal handles, validating all string keys before any writes.
3. Message-command scans filter rooted key text and move selected owned names into their collectors, removing the guard allocation for ignored function names and the second name copy for retained commands.

The first native argument-buffer candidate was rejected after its native-loader scope consistently slowed down. Its source reconstruction, validation, all measurements and tradeoffs remain in [the rejected experiment](2026-10-21-song-lua-actor-state-rejected.md). The final native-loader functions match the baseline. All final measurements below use freshly rebuilt binaries.

Both raw helpers use the unchanged stack-restoring read-only VM guard: one reserved slot, a checked rooted string, UTF-8 validation and only Rust classification/copying/hashing. No pointer or borrowed VM text escapes. Snapshot keys use the actor owning VM, preserving the previous cross-VM behavior for supported simple actors. Snapshot/global restore validate excluded keys; message scans continue to ignore invalid UTF-8 function names as before. Authored callbacks and Lua writes remain outside the raw scope.

Six alternating paired rounds, nine retained samples per variant/workload (54 per variant). All complete Debug DTO snapshots match exactly. Windows Toolhelp process snapshots are polled every second by the parent; batches observing cargo/rustc/cl/link are retained separately and repeated. All original six workloads remain, plus an existing update/message fixture and a bounded message-heavy fixture added because the general suite barely exercised these helpers.

| Complete library workload | Before compiles/s | After compiles/s | Throughput | Worker cycles | Rust allocations before → after |
|---|---:|---:|---:|---:|---:|
| offline_basic | 4.662 | 4.640 | -0.47% | +0.51% | 252,730 → 252,722 |
| offline_capture | 2.431 | 2.400 | -1.28% | +1.31% | 376,938 → 376,676 |
| live_wrappers | 1.731 | 1.678 | -3.05% | +3.11% | 706,238 → 706,240 |
| live_actors | 2.144 | 2.147 | +0.13% | -0.15% | 592,089 → 592,089 |
| live_two_layers | 1.180 | 1.181 | +0.11% | -0.11% | 1,163,893 → 1,163,893 |
| live_timing4096 | 1.717 | 1.744 | +1.54% | -1.53% | 714,236 → 714,239 |
| offline_actor_state | 2.373 | 2.320 | -2.20% | +2.22% | 389,939 → 382,349 |
| offline_sampling_buffers | 1.709 | 1.642 | -3.93% | +4.14% | 646,891 → 645,696 |

| Production-helper scope | Throughput | Worker cycles | Allocations/op before → after |
|---|---:|---:|---:|
| actor_vector_snapshot | +93.90% | -48.33% | 166.02 → 23.00 |
| actor_table_snapshot | +86.04% | -46.34% | 166.00 → 27.00 |
| restore_scalar_globals | +110.77% | -52.98% | 167.00 → 3.00 |
| message_command_scan | +61.52% | -38.11% | 148.00 → 11.00 |

Scopes prepare the VM, a 128-method/eight-message actor with deep state and wrapper membership, globals and restore inputs outside timing. GC is stopped. Each sample performs 64 operations; output/temporary drops and consumed restore-input destruction are included, VM/other input destruction excluded. The message collector uses the same copy-versus-move boundary as the original/final production call sites. Whole compiles retain normal GC and include host construction, Lua initialization/startup, extraction and finalization; DTO/VM destruction and Debug validation are outside timing.

Validation: `test result: FAILED. 788 passed; 15 failed; 21 ignored; 0 measured; 0 filtered out; finished in 213.41s` All 15 baseline diagnostics match after normalizing source locations and process/thread IDs. Four new differential checks cover ordered snapshots, deep values, wrapper identity, live destination state, cross-VM compatibility, UTF-8 errors/stack restoration, validation-before-write/authored setters, and command order/function identity/invalid and non-function keys/Unicode/NUL/raw lookup. The existing 10,000-node snapshot test passes. The added complete message fixture produces real captures without skipped captures.

Reproduction (PowerShell; preserve the parent executable before rebuilding):

```powershell
cargo +1.98.1 build -p deadsync-song-lua --example compile_pipeline --release --locked --offline -j 2 --config profile.release.lto=false --message-format=json > target/song-lua-actor-state-final-build.jsonl
cargo +1.98.1 test -p deadsync-song-lua --lib --release --locked --offline -j 2 --config profile.release.lto=false --no-run --message-format=json > target/song-lua-actor-state-final-test-build.jsonl
python scripts/benchmark_song_lua_compile.py --suite actor_state --binary target/release/deps/deadsync_song_lua-df5ce19c80cb89a8.exe --out target/actor-state-final-scopes.json --rounds 6
python scripts/benchmark_song_lua_pipeline.py --suite startup --before target/actor-state-final-2026-10-21/before.exe --after target/actor-state-final-2026-10-21/after.exe --out target/actor-state-final-pairs --pairs 6 --samples 9
python scripts/benchmark_song_lua_pipeline.py --suite actor_state --before target/actor-state-final-2026-10-21/before.exe --after target/actor-state-final-2026-10-21/after.exe --out target/actor-state-final-extra-pairs --pairs 6 --samples 9
```

Raw run reports/DTOs: `target/actor-state-final-pairs`, `target/actor-state-final-extra-pairs`, `target/actor-state-final-scopes.json`. The companion JSON embeds all 1,296 authoritative samples (864 whole compiles and 432 scoped batches of 64 operations), prototype evidence, build/source/binary hashes, baseline receipt, audits and validation.

- Default-feature normal library; not the root application test-support configuration, whole-app/FPS measurement or a universal compile speedup claim.
- QueryThreadCycleTime scheduled worker cycles including kernel execution, not a hardware PMU retired-instruction counter.
- Whole-compile all-process Rust GlobalAlloc includes mlua; scoped counters are current-thread only. Native malloc excluded; allocation traffic is not retained memory/RSS. Counted System allocator overhead is included.
- Release opt-level3 with LTO disabled; pinned rustc1.98.1; CPU2; 32MiB worker stacks. Desktop background activity is not fully controlled. One-second process audits detect observed compiler overlap, not all system interference.
- Raw key helper prototypes were measured before production edits. Their source formatting was later normalized and rebuilt/repeated. Final helper bodies are canonically identical after renaming.
- Synthetic message fixture is bounded at 16 actors. All original six cases and the existing sampling_buffers fixture are retained; no performance-based sample rejection.
