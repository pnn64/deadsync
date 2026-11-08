# Song Lua short method forwarding

Common combined forwarding cases improve throughput by 8.72–18.78% and remove two buffer allocations. Complete library fixtures range from -1.21% to +2.90%; this does not establish a broad compile-speed win. All complete outputs match the baseline. Oversized lists and each isolated change are reported below.

Baseline: `3d2bb123f`. Three changes avoid generic conversion work and short-lived buffers:

1. Compact binder entries become Value arguments at the table API. The temporary Vec retains its smaller handle pairs, and Lua ownership/drop order remains on the existing owned-value path. Wrapping a handle in Value does not allocate.
2. Native class forwarding retains zero through three checked argument Values inline. Exact count and order are preserved, including explicit/trailing nils. Four or more arguments delegate to MultiValue.
3. Forwarded zero/one-result lists reuse the existing checked IgnoredArgs storage with an IntoLuaMulti implementation. Larger return lists delegate to MultiValue. The original return count is preserved, including zero returns versus one nil.

The checked readers convert every real stack value from left to right, preserving panic propagation from extra arguments and returns. Push implementations delegate to mlua tuple/MultiValue APIs, retaining all handles through the pushes. A trailing unit in the tuple preserves the original n+1 stack reservation and adds no Lua value. No raw pointer or borrowed Lua value escapes. The unsafe implementations and storage were measured and behavior-checked in prototypes before production edits; source checks match those implementations.

Observed trace calls retain the original MultiValue argument/result conversion, query-phase order and event reporter. A fresh-process trace oracle compares exact events, return values/counts, errors and nested-query suppression against the frozen adapter. Production trace implementation is unchanged.

Prototype selection: borrowed-handle binder writes slowed larger cases, while explicit Value writes improved the current compact binder. A generic callback setter projection showed no win and was rejected. A wide inline-result container was also measured. The zero/one-result specialization has a smaller representation and reuses existing checked code; larger result lists retain their original buffer path. The wider container can be faster for short multi-result lists. All variants and rejected measurements remain in JSON evidence.

Six alternating paired rounds, nine retained samples per variant/workload (54 per variant). Complete Debug DTO snapshots match exactly. Compiler-overlap audits retain/repeat complete batches; no performance-based rejection. Prototypes use balanced rotated/reversed variant orders.

| Complete library workload | Before compiles/s | After compiles/s | Throughput | Worker cycles | Allocations before → after | Allocated bytes before → after |
|---|---:|---:|---:|---:|---:|---:|
| offline_basic | 4.73 | 4.71 | -0.57% | +0.61% | 252,236 → 251,933 | 39,440,341 → 39,421,901 |
| offline_capture | 2.27 | 2.29 | +0.97% | -1.10% | 378,778 → 377,683 | 47,879,146 → 47,808,066 |
| live_wrappers | 1.73 | 1.73 | +0.20% | -0.20% | 707,229 → 704,886 | 119,679,255 → 119,538,415 |
| live_actors | 2.08 | 2.07 | -0.39% | +0.38% | 593,702 → 590,818 | 97,114,741 → 96,941,461 |
| live_two_layers | 1.17 | 1.17 | -0.40% | +0.41% | 1,163,720 → 1,159,329 | 201,433,119 → 201,169,399 |
| live_timing4096 | 1.73 | 1.73 | -0.09% | +0.05% | 715,228 → 712,882 | 120,785,805 → 120,644,792 |
| offline_actor_state | 2.18 | 2.24 | +2.90% | -2.83% | 384,403 → 383,684 | 52,606,835 → 52,562,795 |
| offline_sampling_buffers | 1.71 | 1.69 | -1.21% | +1.23% | 644,954 → 643,459 | 78,015,733 → 77,915,853 |

| Scoped workload | Before units/s | After units/s | Throughput | Worker cycles | Allocations/unit before → after | Allocated bytes/unit before → after |
|---|---:|---:|---:|---:|---:|---:|
| bind0 | 1,034,762 | 1,057,851 | +2.23% | -2.14% | 4.016 → 4.016 | 185 → 185 |
| bind8 | 186,671 | 188,402 | +0.93% | -1.58% | 8.016 → 8.016 | 1,324 → 1,324 |
| bind128 | 14,490 | 14,263 | -1.57% | +1.94% | 12.016 → 12.016 | 22,707 → 22,707 |
| bind512 | 3,558 | 3,751 | +5.40% | -5.25% | 14.016 → 14.016 | 91,095 → 91,095 |
| bind128_existing | 13,847 | 14,398 | +3.98% | -3.58% | 11.000 → 11.000 | 22,642 → 22,642 |
| inputs_receiver1_return1 | 861,749 | 916,834 | +6.39% | -6.06% | 5.000 → 4.000 | 200 → 160 |
| inputs_scalar2_return1 | 1,084,064 | 1,179,469 | +8.80% | -7.86% | 2.000 → 1.000 | 120 → 40 |
| inputs_size3_return1 | 1,024,942 | 1,134,516 | +10.69% | -9.92% | 2.000 → 1.000 | 160 → 40 |
| inputs_extra8_return1 | 795,885 | 783,354 | -1.57% | +1.83% | 2.000 → 2.000 | 360 → 360 |
| inputs_scalar2_return0 | 919,091 | 985,610 | +7.24% | -6.70% | 4.000 → 3.000 | 216 → 136 |
| inputs_scalar2_return3 | 771,982 | 836,068 | +8.30% | -7.40% | 5.000 → 4.000 | 336 → 256 |
| inputs_extra8_return8 | 589,804 | 586,929 | -0.49% | +0.72% | 2.000 → 2.000 | 640 → 640 |
| results_receiver1_return1 | 845,704 | 928,151 | +9.75% | -9.05% | 5.000 → 4.000 | 200 → 160 |
| results_scalar2_return1 | 1,089,992 | 1,158,879 | +6.32% | -5.89% | 2.000 → 1.000 | 120 → 80 |
| results_size3_return1 | 1,037,954 | 1,115,856 | +7.51% | -7.01% | 2.000 → 1.000 | 160 → 120 |
| results_extra8_return1 | 764,718 | 808,137 | +5.68% | -5.32% | 2.000 → 1.000 | 360 → 320 |
| results_scalar2_return0 | 923,276 | 936,994 | +1.49% | -1.49% | 4.000 → 4.000 | 216 → 216 |
| results_scalar2_return3 | 799,333 | 780,306 | -2.38% | +2.57% | 5.000 → 5.000 | 336 → 336 |
| results_extra8_return8 | 602,787 | 578,979 | -3.95% | +4.10% | 2.000 → 2.000 | 640 → 640 |
| both_receiver1_return1 | 913,277 | 992,885 | +8.72% | -7.84% | 5.000 → 3.000 | 200 → 120 |
| both_scalar2_return1 | 1,064,975 | 1,234,814 | +15.95% | -13.59% | 2.000 → 0.000 | 120 → 0 |
| both_size3_return1 | 1,042,962 | 1,238,791 | +18.78% | -15.79% | 2.000 → 0.000 | 160 → 0 |
| both_extra8_return1 | 791,020 | 796,841 | +0.74% | -0.75% | 2.000 → 1.000 | 360 → 320 |
| both_scalar2_return0 | 936,801 | 1,022,779 | +9.18% | -8.70% | 4.000 → 3.000 | 216 → 136 |
| both_scalar2_return3 | 812,026 | 855,740 | +5.38% | -5.28% | 5.000 → 4.000 | 336 → 256 |
| both_extra8_return8 | 581,422 | 574,914 | -1.12% | +0.99% | 2.000 → 2.000 | 640 → 640 |

Validation: `test result: FAILED. 798 passed; 15 failed; 29 ignored; 0 measured; 0 filtered out; finished in 209.55s`. All 15 baseline failure diagnostics match after process/path/source-location normalization. The added forwarding check verifies exact arity/order, nils, binary/reference identities, coroutines, bounded reference-pressure parity/recovery and panic payloads in argument/result positions 1, 2, 3, 4 and 8. The ignored fresh-process trace check also passes. Existing complete compiler oracle and fixtures run in the full suite.

Evidence: [JSON](2026-10-21-song-lua-forward-values.json) contains 3672 retained production-scope/fixture samples, prototype variants, all source/binary hashes, receipt/log hashes, normalized baseline diagnostics and overlap audits. Raw measurements remain under target/forward-values-* and target/method-storage-prototype.

Measurement limits:

- Default-feature normal library; root app test-support configuration and whole-app/FPS are not measured.
- QueryThreadCycleTime scheduled worker cycles including kernel execution, not hardware-PMU retired instructions.
- Whole-compile all-process Rust GlobalAlloc includes mlua; scoped counters are current-thread only. Native malloc excluded; allocation traffic is not retained memory/RSS. Counted System allocator overhead is included.
- Release opt-level3 with LTO disabled; rustc1.98.1; CPU2; 32MiB worker stacks. Units/prototypes use panic=unwind; normal fixture binaries retain panic=abort. Both sides of each comparison use the same configuration.
- Background desktop activity is not fully controlled. One-second process audits reject observed compiler overlap, not all interference.
- Complete fixture timing covers compilation/startup. The eight live-frame advances used in complete snapshot validation occur outside timing; whole-frame or game FPS gains are not established.
- Forward arguments specialize lengths zero through three; results specialize zero/one. Larger lists use the original MultiValue path, retaining enum dispatch/move overhead. All measured large-list shapes are reported.
- Forwarding timing uses fresh native class adapters and Lua semantic callbacks with logging off. Trace equality is verified separately in a fresh process with logging enabled. Partial adapters isolate argument and result storage; full fixtures use production adapters.
