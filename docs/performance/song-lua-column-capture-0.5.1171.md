# Song-Lua column capture and replay - 0.5.1171

Parent: `7199d99cc` (`0.5.1170`). This pass follows the local
`rust-performance.md` guidance on measured hot paths (M-HOTPATH), storage reuse
(M-MEM-REUSE), initial capacity (M-INITIAL-CAPACITY), and avoiding repeated
work (M-THROUGHPUT).

## Three changes

1. **Merge ordered column samples directly into window output.** Capture visits
   players, columns, and transform targets in key order. Window construction now
   checks that order and merges the two inputs linearly. It avoids the temporary
   key vector, duplicate-key searches, and two full input scans per key. First
   occurrences still win, missing values retain target-specific defaults, and
   neutral windows are omitted exactly as before. Arbitrarily ordered public
   inputs use the existing fallback. The update compiler appends directly to its
   final output vector; the public function keeps its existing Vec-returning API.
2. **Validate position splines while reading them.** Uniform-Y extraction keeps
   the first Y value and a validity flag, removing the temporary `Vec<[f32; 2]>`
   and its second traversal. The point lookup and numeric-conversion order is
   unchanged. A geometric mismatch does not short-circuit later reads, so a
   later malformed table still returns the same error. Missing or unreadable
   coordinates retain their earlier short-circuit behavior. The existing owned
   mode string remains; this query is not entirely allocation-free.
3. **Size populated replay buffers once and retain the BPM prefix.** With a
   populated BPM map, the frame count determines output capacity before replay
   begins, eliminating buffer growth. For ordered finite BPM segments with
   positive BPM/rate values, the clock advances a segment cursor instead of
   rescanning all earlier segments for every frame. Each timestamp and delta
   uses the original formula and
   arithmetic order, preserving exact double-precision bits at boundaries.
   Unordered or malformed timing uses the original per-frame conversion.
   Populated replay keeps one heap allocation: the tested streaming iterator
   regressed long replay with no BPM segments by 32% in calling-thread cycles. Keeping
   a pre-sized buffer reduces churn; the cursor improves throughput on
   BPM-heavy replay. Empty timing maps retain the original build path because
   pre-sizing them showed slowdowns under the full benchmark workload
   (up to 2.4x time in the paired trials).

These are song-Lua compilation improvements, not measured whole-game frame-rate
or total song-load-time gains. No dependencies or production unsafe code were
added. Emitted windows and their optional easing strings still require owned
storage.

## Behavior and validation

Three test-only baseline functions freeze the parent algorithms. The audit
compares their bodies with the parent, allowing formatting, test visibility,
and spelling the unchanged 60 FPS constant as `60.0_f32` in the oracle.
Shared scalar/timing helpers are unchanged from the parent.

Eight new tests cover ordered and unordered column samples, duplicate and
missing keys, arbitrary player/column identifiers, all four transform targets,
prefixed output preservation, optional window metadata, signed zero, infinities,
and NaN payloads. Generated window cases exercise 24 seeds at five input sizes,
in both arbitrary and sorted order. Spline cases cover mode handling, missing
points, conversion errors, malformed later tables after an early geometry
mismatch, and the original numeric short-circuit behavior. Replay comparisons
check every timestamp and delta bit for BPM boundaries, duplicate and negative
segment beats, nonzero starts, rate changes, fractional and reversed spans,
and malformed/unordered fallback contexts. Invalid BPM fixtures use bounded
intervals because the parent can otherwise produce enormous frame counts.

Allocation assertions exclude fixtures and retained outputs. Ordered neutral
column sampling has zero Rust heap churn, and active appends with enough output
capacity and no easing strings also have zero churn. Replay with a populated
BPM map allocates once without reallocations, checked for zero, short, and long
spans with one and 128 BPM segments. A 256-point spline read now allocates only
its existing 27-byte mode string, with no point vector.

Validation commands and outcomes:

- `cargo test -p deadsync-song-lua --lib column_capture_perf --locked`:
  **eight passed**, one manual benchmark ignored. All eight also pass in the
  full debug and release runs.
- `cargo test -p deadsync-song-lua --lib --locked` and the same command with
  `--release`: **415 passed, five failed, nine ignored** in each profile.
- The recorded parent run has **407 passed, the same five failed, eight
  ignored**. Comparing assertion text confirms the same failures in both
  current profiles; source line numbers shifted. This uses the full-suite log
  recorded for `0.5.1170` during the prior pass, rather than a fresh parent
  build. The existing failures are
  `compile_song_lua_extracts_actorproxy_targets`,
  `compile_song_lua_layers_share_init_globals_and_actor_refs`,
  `compile_song_lua_resolves_local_and_hidden_screen_proxy_targets`,
  `compile_song_lua_runs_cmd_queuecommand_builders`, and
  `compile_song_lua_supports_notefield_column_api`.
- `cargo check --all-targets --offline`: passed.
- `cargo clippy -p deadsync-song-lua --all-targets --locked -- -D clippy::perf`:
  passed. Existing warnings outside the performance lint group remain.
- Three isolated paired release benchmark runs: all 23 scenarios passed their
  output comparisons; allocation counts were identical across runs.
- Frozen baseline audit, exact patch/lockfile audit, and `git diff --check`:
  passed.

## Measurement method and limits

Windows x86-64, Intel Xeon E5-2696 v4 @ 2.20 GHz, Rust 1.98.0
(`88d9e12ae`, LLVM 22.1.8), release optimization with full LTO. The shared
`tests/support/perf.rs` helper records seven timing samples after three warmups,
then measures allocation churn separately. Three isolated executable runs
alternate old/new order (old first, new first, old first); no Cargo compilation
runs during measurement. Tables use medians of the three per-run medians.
Windows `QueryThreadCycleTime` measures the calling thread's cycles, not retired
instructions or a cross-machine metric.

Both implementations use the same fixture and black-box boundaries in the same
binary. Correctness checks precede timing. Output destruction is included in
both versions. Fixtures are built outside timing. Benchmark units and sizes:

- `windows_C_active_A_ordered_O`: C columns for each of two players, four
  transform targets per column; throughput counts sample keys. The unordered
  case reverses input order. Each operation returns a fresh output vector.
- `window_batch_CxF_active_A`: F compiler samples using the same input;
  throughput counts frame pairs. The old path creates temporary key/output
  vectors per sample and extends the final vector; the new path appends directly.
  Both start with a fresh final output buffer and include its growth/destruction.
- `spline_final_N`: N uniform Lua spline points; throughput counts points
  (one call for zero points). Each pair shares one warmed Lua actor. Lua GC is
  stopped during timing, then restarted and collected afterward.
- `replay_S_end_E_rate_R_ordered_O`: S BPM segments, end beat E, rate R/100;
  throughput counts replay frames. Both consume every timestamp into the same
  black-boxed checksum. Both measurements include the vector's construction
  and destruction.

Each timing sample uses 512 window operations, eight compiler batches, 512
spline reads (32 for 256/4,096 points), or 16 replay operations (1,024 for
zero/one-beat spans). Allocation counts are from the existing thread-local
Rust global-allocator wrapper. Lua's C allocator and other threads are not
counted. Byte totals include reallocations and represent cumulative requested
and freed bytes, not peak live memory, RSS, or retained heap size.

Representative measured improvements:

- **Column batches (two players, 16 columns each, 600 samples): 71.68% fewer thread cycles**, 3.53x throughput, 42.32% fewer requested bytes (39,991,808 -> 23,068,320 B per operation).
- **Position spline (256 points): 1.22% fewer thread cycles**, 1.01x throughput, 98.70% fewer requested bytes (2,075 -> 27 B per operation).
- **Replay (128 BPM segments, 512 beats): 93.76% fewer thread cycles**, 16.01x throughput, 71.68% fewer requested bytes (1,048,528 -> 296,976 B per operation).

Spline CPU gains are modest because Lua table access still dominates the read;
the larger benefit is removal of per-query point-buffer allocation. Neutral
ordered column batches allocate nothing. Active batches still allocate their
final output. Replay with populated BPM maps uses one allocation without
buffer growth; empty BPM maps retain the original allocation behavior.
Small control-case differences and all slower cases are
reported below; these microbenchmarks do not establish whole-application gains.

## Results

Positive cycle savings indicate an improvement on this workload and machine.

| Scenario | Old us/op | New us/op | Old cycles/op | New cycles/op | Fewer cycles |
|---|---:|---:|---:|---:|---:|
| windows_0_active_false_ordered_true | 0.0213 | 0.0172 | 49.7 | 40.3 | 18.91% |
| windows_1_active_true_ordered_true | 0.6453 | 0.2832 | 1,419.0 | 624.2 | 56.01% |
| windows_4_active_true_ordered_true | 2.3869 | 0.8092 | 5,232.8 | 1,778.7 | 66.01% |
| windows_16_active_true_ordered_true | 30.3393 | 2.5465 | 66,499.9 | 5,584.4 | 91.60% |
| windows_16_active_false_ordered_true | 27.6854 | 1.1943 | 60,588.0 | 2,624.1 | 95.67% |
| windows_16_active_true_ordered_false | 30.1158 | 30.5174 | 66,004.3 | 66,889.6 | -1.34% |
| window_batch_4x600_active_true | 2,692.2000 | 1,600.8875 | 5,894,672.2 | 3,506,649.8 | 40.51% |
| window_batch_16x600_active_true | 23,606.1875 | 6,687.2625 | 51,696,282.9 | 14,638,317.9 | 71.68% |
| window_batch_16x600_active_false | 16,800.1750 | 729.0625 | 36,822,853.1 | 1,597,987.1 | 95.66% |
| spline_final_0 | 0.4680 | 0.4686 | 1,029.8 | 1,030.6 | -0.08% |
| spline_final_1 | 0.8096 | 0.7385 | 1,779.2 | 1,623.1 | 8.77% |
| spline_final_16 | 3.2283 | 3.1064 | 7,086.6 | 6,819.5 | 3.77% |
| spline_final_256 | 42.0562 | 41.5969 | 92,361.5 | 91,236.6 | 1.22% |
| spline_final_4096 | 706.9531 | 709.0719 | 1,549,539.7 | 1,554,101.2 | -0.29% |
| replay_0_end_0_rate_100_ordered_true | 0.0689 | 0.0695 | 152.4 | 153.7 | -0.85% |
| replay_0_end_1_rate_100_ordered_true | 0.4987 | 0.5003 | 1,096.4 | 1,096.0 | 0.04% |
| replay_0_end_512_rate_100_ordered_true | 56.8250 | 47.6062 | 124,538.8 | 104,317.4 | 16.24% |
| replay_1_end_512_rate_100_ordered_true | 363.8625 | 204.5062 | 795,522.8 | 448,328.7 | 43.64% |
| replay_16_end_512_rate_100_ordered_true | 782.7125 | 140.1062 | 1,716,105.9 | 307,300.0 | 82.09% |
| replay_128_end_512_rate_100_ordered_true | 2,316.0000 | 144.6938 | 5,076,376.6 | 316,944.2 | 93.76% |
| replay_128_end_512_rate_150_ordered_true | 1,475.3938 | 136.1500 | 3,233,962.1 | 298,629.8 | 90.77% |
| replay_16_end_512_rate_100_ordered_false | 722.5062 | 633.9375 | 1,583,198.7 | 1,388,145.4 | 12.32% |
| replay_128_end_16_rate_100_ordered_true | 7.8250 | 3.9312 | 17,258.2 | 8,711.4 | 49.52% |

A/R/F means allocation/reallocation/free calls. Bytes include reallocations and common input/output ownership costs.

| Scenario | Old A/R/F | New A/R/F | Old requested/freed B | New requested/freed B | Old units/s | New units/s |
|---|---:|---:|---:|---:|---:|---:|
| windows_0_active_false_ordered_true | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 46,972,477.1 | 58,181,818.2 |
| windows_1_active_true_ordered_true | 2/2/2 | 1/1/1 | 1,344/1,344 | 1,056/1,056 | 12,397,094.4 | 28,248,275.9 |
| windows_4_active_true_ordered_true | 2/6/2 | 1/3/1 | 6,720/6,720 | 5,280/5,280 | 13,406,431.6 | 39,546,222.5 |
| windows_16_active_true_ordered_true | 2/10/2 | 1/5/1 | 28,224/28,224 | 22,176/22,176 | 4,218,956.2 | 50,265,378.1 |
| windows_16_active_false_ordered_true | 1/5/1 | 0/0/0 | 6,048/6,048 | 0/0 | 4,623,383.6 | 107,172,526.6 |
| windows_16_active_true_ordered_false | 2/10/2 | 2/10/2 | 28,224/28,224 | 28,224/28,224 | 4,250,257.8 | 4,194,330.8 |
| window_batch_4x600_active_true | 1201/3610/1201 | 1/13/1 | 9,796,352/9,796,352 | 5,766,816/5,766,816 | 222,866.1 | 374,792.1 |
| window_batch_16x600_active_true | 1201/6010/1201 | 1/15/1 | 39,991,808/39,991,808 | 23,068,320/23,068,320 | 25,417.1 | 89,722.8 |
| window_batch_16x600_active_false | 600/3000/600 | 0/0/0 | 3,628,800/3,628,800 | 0/0 | 35,713.9 | 822,974.7 |
| spline_final_0 | 1/0/1 | 1/0/1 | 27/27 | 27/27 | 2,136,894.8 | 2,134,222.6 |
| spline_final_1 | 2/0/2 | 1/0/1 | 35/35 | 27/27 | 1,235,223.2 | 1,354,139.1 |
| spline_final_16 | 2/0/2 | 1/0/1 | 155/155 | 27/27 | 4,956,137.7 | 5,150,581.6 |
| spline_final_256 | 2/0/2 | 1/0/1 | 2,075/2,075 | 27/27 | 6,087,085.7 | 6,154,308.5 |
| spline_final_4096 | 2/0/2 | 1/0/1 | 32,795/32,795 | 27/27 | 5,793,877.8 | 5,776,565.3 |
| replay_0_end_0_rate_100_ordered_true | 1/0/1 | 1/0/1 | 16/16 | 16/16 | 14,504,249.3 | 14,382,022.5 |
| replay_0_end_1_rate_100_ordered_true | 1/4/1 | 1/4/1 | 976/976 | 976/976 | 44,112,003.1 | 43,974,233.8 |
| replay_0_end_512_rate_100_ordered_true | 1/13/1 | 1/13/1 | 524,240/524,240 | 524,240/524,240 | 180,237,571.5 | 215,139,818.8 |
| replay_1_end_512_rate_100_ordered_true | 1/14/1 | 1/0/1 | 1,048,528/1,048,528 | 491,536/491,536 | 84,430,244.9 | 150,220,347.8 |
| replay_16_end_512_rate_100_ordered_true | 1/14/1 | 1/0/1 | 1,048,528/1,048,528 | 323,872/323,872 | 25,861,347.6 | 144,476,067.3 |
| replay_128_end_512_rate_100_ordered_true | 1/14/1 | 1/0/1 | 1,048,528/1,048,528 | 296,976/296,976 | 8,014,248.7 | 128,277,828.2 |
| replay_128_end_512_rate_150_ordered_true | 1/13/1 | 1/0/1 | 524,240/524,240 | 198,000/198,000 | 8,387,591.4 | 90,892,398.1 |
| replay_16_end_512_rate_100_ordered_false | 1/14/1 | 1/0/1 | 1,048,528/1,048,528 | 327,712/327,712 | 28,348,543.7 | 32,309,178.7 |
| replay_128_end_16_rate_100_ordered_true | 1/9/1 | 1/0/1 | 32,720/32,720 | 9,312/9,312 | 74,376,996.8 | 148,044,515.1 |

Per-run paired cycle savings for representative cases:

- `window_batch_16x600_active_true`: 71.81%, 71.97%, 71.38%.
- `spline_final_256`: 1.65%, 0.33%, 0.01%.
- `replay_128_end_512_rate_100_ordered_true`: 93.79%, 92.10%, 93.76%.

Cases with higher median cycle counts:

- `windows_16_active_true_ordered_false`: 1.34% more cycles; elapsed 30115.8 -> 30517.4 ns/op.
- `spline_final_0`: 0.08% more cycles; elapsed 468.0 -> 468.6 ns/op.
- `spline_final_4096`: 0.29% more cycles; elapsed 706953.1 -> 709071.9 ns/op.
- `replay_0_end_0_rate_100_ordered_true`: 0.85% more cycles; elapsed 68.9 -> 69.5 ns/op.

## Reproduction

```powershell
cargo test -p deadsync-song-lua --lib --locked
cargo test -p deadsync-song-lua --lib --release --locked
cargo test -p deadsync-song-lua --lib --release --locked column_capture_bench -- --ignored --test-threads=1 --nocapture
$env:DEADSYNC_PERF_REVERSE = '1'
cargo test -p deadsync-song-lua --lib --release --locked column_capture_bench -- --ignored --test-threads=1 --nocapture
Remove-Item Env:DEADSYNC_PERF_REVERSE
cargo test -p deadsync-song-lua --lib --release --locked column_capture_bench -- --ignored --test-threads=1 --nocapture
```

The version changes exactly `0.5.1170 -> 0.5.1171`. `Cargo.lock` changes
only the three packages inheriting the workspace version.
