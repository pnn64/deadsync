# Song Lua numeric callback prefixes

Valid wrap-width, two-axis size and tween calls remove one allocation and improve throughput by 1.30–6.41%. Complete library fixtures range from -1.53% to +1.87%; this does not establish a broad compile-speed win. Empty wrap/size calls slow by about 30%. All complete outputs match the baseline.

Baseline: `d95fdfe52`. Three optimizations remove argument-buffer allocations from actor factories:

1. Wrap width decodes its receiver/value prefix inline and keeps its existing optional-receiver rule.
2. Two-axis size decodes its receiver/width/height inline and keeps its existing early exits and coercions.
3. Tween creation decodes its positional duration inline, preserving separate float and exact-duration conversions.

The private tuple adapter delegates to mlua's checked FromLuaMulti::from_stack_multi, preserving MultiValue's direct stack-error propagation. The tuple holds three Value fields and a final MultiValue for every remaining input. No extra argument is skipped; retained input handles live through the callback. No public API, numeric coercion, default, receiver heuristic, tween policy or downstream setter changes. Missing tuple values are nil, which preserves each factory's numeric fallback. Existing typed scalar factories remain unchanged.

The prototype was measured before production edits: two/three-input calls remove one allocation, with all five input shapes and CPU cycles in the JSON evidence. It checks nil/binary/reference identity, coroutine calls, bounded reference-pressure parity/recovery, and caught Rust-panic payloads in prefix and tail positions. Empty input initializes three nils and retains a measured cost. The production trait implementation matches the prototype; frozen factory bodies match the baseline.

Six alternating paired rounds, nine retained samples per variant/workload (54 per variant). Complete Debug DTO snapshots match exactly. Parent-process Windows Toolhelp audits retain and repeat complete batches observing cargo/rustc/cl/link; no performance-based rejection. Scoped setters run actual production factories with unchanged shared downstream helpers. Zero-duration tweens keep the queue bounded; those scopes do not characterize arbitrary live animation.

| Complete library workload | Before compiles/s | After compiles/s | Throughput | Worker cycles | Allocations before → after |
|---|---:|---:|---:|---:|---:|
| offline_basic | 4.61 | 4.69 | +1.87% | -1.59% | 252,661 → 252,635 |
| offline_capture | 2.31 | 2.31 | -0.36% | +0.38% | 378,001 → 377,959 |
| live_wrappers | 1.70 | 1.69 | -0.26% | +0.27% | 707,306 → 707,156 |
| live_actors | 2.02 | 2.00 | -1.00% | +1.95% | 593,412 → 593,385 |
| live_two_layers | 1.16 | 1.14 | -1.51% | +1.56% | 1,164,705 → 1,164,423 |
| live_timing4096 | 1.71 | 1.72 | +0.14% | -0.11% | 715,291 → 715,158 |
| offline_actor_state | 2.31 | 2.28 | -1.53% | +1.56% | 383,658 → 383,632 |
| offline_sampling_buffers | 1.71 | 1.70 | -0.78% | +0.80% | 645,640 → 645,613 |

| Callback workload | Before calls/s | After calls/s | Throughput | Worker cycles | Allocations/call before → after |
|---|---:|---:|---:|---:|---:|
| numeric_wrap_empty0 | 8,200,200 | 5,763,737 | -29.71% | +42.02% | 0.000 → 0.000 |
| numeric_wrap_scalar2 | 1,098,521 | 1,168,933 | +6.41% | -6.08% | 1.000 → 0.000 |
| numeric_wrap_size3 | 999,750 | 1,123,631 | +12.39% | -10.95% | 1.000 → 0.000 |
| numeric_wrap_extra8 | 699,230 | 728,605 | +4.20% | -3.99% | 2.000 → 2.000 |
| numeric_size_empty0 | 8,537,335 | 5,957,168 | -30.22% | +43.18% | 0.000 → 0.000 |
| numeric_size_scalar2 | 3,277,914 | 3,751,431 | +14.45% | -12.68% | 1.000 → 0.000 |
| numeric_size_size3 | 654,462 | 674,916 | +3.13% | -3.19% | 4.000 → 3.000 |
| numeric_size_extra8 | 485,669 | 513,520 | +5.73% | -5.54% | 5.000 → 5.000 |
| numeric_tween_empty0 | 148,242 | 146,027 | -1.49% | +1.43% | 6.079 → 6.079 |
| numeric_tween_scalar2 | 141,008 | 142,840 | +1.30% | -1.23% | 7.079 → 6.079 |
| numeric_tween_size3 | 142,904 | 144,191 | +0.90% | -0.90% | 7.079 → 6.079 |
| numeric_tween_extra8 | 133,349 | 135,138 | +1.34% | -0.91% | 8.079 → 8.079 |

Validation: `test result: FAILED. 794 passed; 15 failed; 25 ignored; 0 measured; 0 filtered out; finished in 220.94s`. All 15 baseline failure diagnostics match after process/path/source-location normalization. Two added checks compare actual factory state/return values across coercions, omitted receiver and nil/extra inputs, plus coroutines and panic payloads in positions 1, 2, 3, 4 and 8. Existing complete compiler oracle and fixtures also run in the suite.

Evidence: [JSON](2026-10-21-song-lua-prefix-args.json) includes 2160 retained samples, all source/binary hashes, build receipt hashes, normalized baseline diagnostics, prototype and overlap audits. Raw measurements remain under target/prefix-args-*.

Measurement limits:

- Default-feature normal library; root app test-support configuration and whole-app/FPS are not measured.
- QueryThreadCycleTime scheduled worker cycles including kernel execution, not hardware-PMU retired instructions.
- Whole-compile all-process Rust GlobalAlloc includes mlua; scoped counters are current-thread only. Native malloc excluded; allocation traffic is not retained memory/RSS. Counted System allocator overhead is included.
- Release opt-level3 with LTO disabled; rustc1.98.1; CPU2; 32MiB worker stacks. Scoped unit/prototype uses panic=unwind; normal fixture binaries retain panic=abort. Both sides of each comparison use the same configuration.
- Background desktop activity is not fully controlled. One-second process audits reject observed compiler overlap, not all interference.
- Inline prefix decoding also initializes missing values to nil; empty calls can cost more. All zero/two/three/eight-input scopes are retained. Extra arguments remain checked and owned.
