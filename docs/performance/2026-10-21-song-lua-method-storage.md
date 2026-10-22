# Song Lua native method storage

All eight complete library fixtures improve throughput by 0.50–4.29%, use fewer worker cycles and allocate 11.5–13.4% fewer bytes. All complete outputs match the baseline. Native-class setup improves by about 15% and actor-metatable setup by 6%. Larger isolated binding cases slow by up to 5.5% while reducing byte traffic by about 26%; the complete pipeline measurements support retaining the smaller storage despite this local CPU tradeoff.

Baseline: `626d9f1f7`. Three optimizations use the information already available during actor setup:

1. Method binding stores `(LuaString, Function)` handles directly, replacing `(Value, Value)` entries. Each temporary entry shrinks from 80 to 48 bytes on this target. The two-phase collector, checked conversions, filtering, move order and destination handling are unchanged.
2. Native class tables reserve their exact method inventory length; their one-field inheritance metatables reserve one hash entry.
3. Actor instance metatables reserve their four existing metamethod fields.

The changes add no unsafe code, cache, new callback wrapper or extra scan. Bound destination tables retain their original creation timing and growth policy. Native callbacks and actor metatables remain independent objects. Existing aliases, C/Lua classification, invalid-key error atomicity and inheritance paths are retained.

Six alternating paired rounds, nine retained samples per variant/workload (54 per variant). Complete Debug DTO snapshots match exactly. Parent-process Windows Toolhelp audits retain and repeat complete batches observing cargo/rustc/cl/link; no performance-based rejection. Scoped runs call production helpers and frozen baseline copies with unchanged shared dependencies.

| Complete library workload | Before compiles/s | After compiles/s | Throughput | Worker cycles | Allocations before → after | Allocated bytes before → after |
|---|---:|---:|---:|---:|---:|---:|
| offline_basic | 4.57 | 4.74 | +3.80% | -3.11% | 252,635 → 252,236 | 45,452,181 → 39,440,341 |
| offline_capture | 2.28 | 2.32 | +1.85% | -1.69% | 378,575 → 378,162 | 54,059,426 → 47,818,266 |
| live_wrappers | 1.72 | 1.75 | +1.92% | -1.79% | 707,768 → 706,613 | 138,176,802 → 119,618,375 |
| live_actors | 2.07 | 2.10 | +1.54% | -1.62% | 594,001 → 593,086 | 111,517,781 → 97,053,861 |
| live_two_layers | 1.16 | 1.19 | +2.98% | -2.87% | 1,165,039 → 1,163,104 | 232,543,759 → 201,372,239 |
| live_timing4096 | 1.71 | 1.73 | +1.27% | -1.22% | 715,753 → 714,612 | 139,282,585 → 120,724,925 |
| offline_actor_state | 2.20 | 2.30 | +4.29% | -4.00% | 384,248 → 383,787 | 59,573,355 → 52,545,955 |
| offline_sampling_buffers | 1.69 | 1.70 | +0.50% | -0.48% | 645,613 → 644,954 | 88,286,373 → 78,015,733 |

| Setup workload | Before units/s | After units/s | Throughput | Worker cycles | Allocations/unit before → after | Allocated bytes/unit before → after |
|---|---:|---:|---:|---:|---:|---:|
| bind0 | 1,045,752 | 1,041,497 | -0.41% | +0.25% | 4.016 → 4.016 | 185 → 185 |
| bind8 | 173,348 | 177,310 | +2.29% | -2.21% | 8.016 → 8.016 | 1,708 → 1,324 |
| bind128 | 14,062 | 13,930 | -0.94% | +1.14% | 12.016 → 12.016 | 30,771 → 22,707 |
| bind512 | 3,764 | 3,609 | -4.11% | +4.27% | 14.016 → 14.016 | 123,735 → 91,095 |
| bind128_existing | 14,277 | 13,491 | -5.50% | +5.38% | 11.000 → 11.000 | 30,706 → 22,642 |
| native_classes | 5,044 | 5,808 | +15.15% | -13.19% | 946.812 → 913.812 | 75,217 → 58,457 |
| actor_metatable | 274,237 | 291,406 | +6.26% | -5.69% | 17.008 → 15.008 | 1,016 → 896 |

Validation: `test result: FAILED. 797 passed; 15 failed; 27 ignored; 0 measured; 0 filtered out; finished in 217.94s`. All 15 baseline failure diagnostics match after process/path/source-location normalization. Three new checks compare binding identities/aliases/filters and error atomicity, every native class inventory and its inheritance/calls/errors, plus actor metatable lookup, assignment, concatenation, tostring and distinct identity. Existing complete compiler oracle and fixtures also run in the suite.

Evidence: [JSON](2026-10-21-song-lua-method-storage.json) includes 1620 retained samples, all source/binary hashes, build receipt hashes, normalized baseline diagnostics and overlap audits. Raw measurements remain under target/method-storage-*.

Measurement limits:

- Default-feature normal library; root app test-support configuration and whole-app/FPS are not measured.
- QueryThreadCycleTime scheduled worker cycles including kernel execution, not hardware-PMU retired instructions.
- Whole-compile all-process Rust GlobalAlloc includes mlua; scoped counters are current-thread only. Native malloc excluded; allocation traffic is not retained memory/RSS. Counted System allocator overhead is included.
- Release opt-level3 with LTO disabled; rustc1.98.1; CPU2; 32MiB worker stacks. Scoped units use panic=unwind; normal fixture binaries retain panic=abort. Both sides of each comparison use the same configuration.
- Background desktop activity is not fully controlled. One-second process audits reject observed compiler overlap, not all interference.
- Native-class scope units are full seven-class inventories, not individual classes. Isolated metatable scopes use actors without own methods to separate capacity from method collection. All five binder input sizes/destination states are reported.
