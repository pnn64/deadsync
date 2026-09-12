# Song-Lua actor captures — 0.5.1169

Parent: `fd056aa05` (`0.5.1168`). This pass follows the local
`rust-performance.md` guidance on measured hot paths (M-HOTPATH), retaining
owned storage (M-MEM-REUSE), and batching work (M-THROUGHPUT).

## Three changes

1. **Fold owned actor trees in place.** Player Y folding now computes its cosine
   once per tree and recursively changes offsets and text X scales through
   mutable references. The previous implementation moved each large `Actor`
   through a by-value match and recursive return. Frame, camera, and shadow
   children retain their existing storage. Shared and retained wrappers keep
   their established boundaries; folding does not descend into their children.
2. **Apply capture styles in place.** Tint, glow, shadow/stroke colors, blend,
   and saturated Z shifts update the owned tree directly. The precise arithmetic
   and wrapper rules remain unchanged, including unmodified owned-frame
   backgrounds and shared-child styles. This removes recursive actor rebuilding.
3. **Batch retained-frame expansion.** ActorProxy normalization replaces an
   identity retained wrapper using an exact-size `Vec::splice` iterator. The
   tail moves once per wrapper instead of once per inserted child. Capacity
   grows at most once per expansion, and inserted nested wrappers are still
   examined. Empty and nonidentity wrappers retain their previous behavior.

These paths serve song-Lua player captures and proxies in the gameplay screen.
They are not a claim of a whole-game frame-rate improvement. Existing borrowed
capture paths and ordinary songs that do not materialize these trees may see
no benefit. No production unsafe code or dependency changes were introduced.

## Behavior and allocation checks

The integration test calls the production functions through the existing
`test-support` feature. Its baseline freezes 14 functions from the parent;
the source audit checks function bodies against that commit, allowing only
formatting and test visibility changes. Unchanged normalization helpers and
the existing fixed proxy-sort capacity are used consistently.

Six new regression tests cover all 14 actor variants, owned/shared/retained
subtrees, cameras and shadows, blend overrides, saturated Z, text attributes,
mesh geometry, clipping and other metadata, invalid fold parameters, signed
zero, infinities, and NaN payloads. They compare complete actor debug output
plus exact float bits on every field that can change. Independent known-output
checks cover folding, tint/stroke output, expansion order, stable equal-Z
ordering, and camera barriers. Expansion also covers empty/nested wrappers,
each nonidentity wrapper condition, repeated calls, tight capacity and spare
capacity, and preservation of immutable source trees.

Separate allocation assertions exclude fixture setup and destruction. Folding
and styling allocate/reallocate/free nothing and preserve child Vec capacities,
Box addresses, text-attribute storage and shared-resource addresses. Expansion
of the tested retained sprite children, with enough capacity and a retained
source owner, also has zero heap churn. Cloning owned nested children can still
allocate their own buffers, as before.

Validation commands and outcomes:

- `cargo test -p deadsync-notefield --lib --locked`: **370 passed**.
- `cargo test -p deadsync-theme-simply-love --lib --locked -- --test-threads=1`:
  **1,264 passed**, four existing manual tests ignored.
- `cargo test -p deadsync-theme-simply-love --features test-support --test actor_capture_perf --locked`:
  **six passed**, one manual benchmark ignored; also passes with `--release`.
- `cargo check --all-targets --offline`: **passed**.
- `cargo clippy -p deadsync-notefield -p deadsync-theme-simply-love --all-targets --features test-support --locked -- -D clippy::perf -A clippy::large_enum_variant`:
  **passed**. Strict `-D clippy::perf` reports one pre-existing
  `large_enum_variant` error for `SimplyLoveRuntimeRequest` in
  `crates/deadsync-theme-simply-love/src/effects.rs:925`. That file and its
  palette payload are unchanged from the parent. The exception is a command
  flag; no source suppression was added. Other existing non-performance
  warnings remain.
- `git diff --check`: **passed**. The version audit confirms exactly
  `0.5.1168 → 0.5.1169` and only the three inherited package-version changes
  in `Cargo.lock`.

## Measurement method and limits

Windows x86-64, Intel Xeon E5-2696 v4 @ 2.20 GHz, Rust 1.98.0
(`88d9e12ae`, LLVM 22.1.8), release optimization with full LTO. The shared
`tests/support/perf.rs` helper records seven timing samples after three warmups,
then measures allocation churn separately. Each case runs 128–512 iterations
per sample. Three isolated executable runs alternate old/new order
(old first, new first, old first), with no concurrent Cargo builds. Tables use
the median of the three per-run medians. Cycle counts come from Windows
`QueryThreadCycleTime` for the calling thread; they are not hardware-retired
instructions or a cross-machine metric.

The measured `Actor` size is 400 bytes and the existing proxy-sort capacity
is 416 actors. Both implementations run in the same binary with the same fixture and
black-box boundaries. Fixture construction and correctness comparisons are
outside timing. Input cloning and output destruction are included equally.
Fold/style counts denote top-level children; the mixed case also has nested
children. The `flat_256` cases call the folding/styling function on each actor
as the player capture caller does. Expansion names give retained child count, untouched tail count,
and whether spare capacity was supplied. Expansion throughput counts the
resulting actors. `proxy_capture_385` includes normalization (expansion,
stable local sorting and Z clearing), folding, and styling of 385 actors.

Allocation bytes are cumulative requested/freed bytes, including reallocations
and the common input copy. They measure heap traffic, not peak RSS. The
transform-only zero-churn tests distinguish existing input ownership from
additional transformation allocations. Allocation counts may stay unchanged
while avoiding large data copies substantially reduces CPU time.

Reproduce the benchmark:

```powershell
cargo test -p deadsync-theme-simply-love --features test-support --test actor_capture_perf --release --locked actor_capture_bench -- --ignored --test-threads=1 --nocapture
```

Set `DEADSYNC_PERF_REVERSE=1` for the reversed pair order. Raw logs and the
source-audit/aggregation scripts used for this pass are kept locally under
`target/actor-capture-perf/`; the frozen baselines and benchmark are committed.

## Results

For 256-child owned trees, folding uses **49.23% fewer thread cycles** and
styling **39.36% fewer**. Flat 256-actor lists improve by **9.58%** and
**6.58%**, respectively. Expanding 128 retained children before a 256-actor
tail with spare capacity uses **95.51% fewer cycles**. The combined 385-actor
proxy operation drops from **554.33 us to 56.11 us**, or **89.88% fewer cycles**.

The 512-child/eight-tail growth case reduces reallocations from **six to one**
and cumulative requested/freed bytes from **508,000 to 212,400** (**58.19% less
heap traffic**). Folding/styling allocation counts match the parent; the CPU
gain comes from less copying and repeated calculation. The no-retained-frame
control is **1.05% slower in cycles**, an elapsed difference of **0.10 us** per
256-actor input including its clone/destruction. That small control regression
is retained explicitly alongside the productive improvements.

Positive cycle savings indicate an improvement on this workload and machine.

| Scenario | Old us/op | New us/op | Old cycles/op | New cycles/op | Fewer cycles |
|---|---:|---:|---:|---:|---:|
| fold_1 | 0.3379 | 0.1484 | 746.0 | 331.0 | 55.63% |
| style_1 | 0.1684 | 0.1172 | 374.7 | 262.4 | 29.97% |
| fold_32 | 2.7395 | 1.5176 | 6,017.4 | 3,337.1 | 44.54% |
| style_32 | 2.3793 | 1.4762 | 5,228.6 | 3,201.6 | 38.77% |
| fold_256 | 22.3398 | 11.3246 | 48,941.6 | 24,847.2 | 49.23% |
| style_256 | 18.9910 | 11.4180 | 41,313.2 | 25,053.0 | 39.36% |
| fold_1024 | 99.0102 | 57.7980 | 217,028.9 | 126,673.8 | 41.63% |
| style_1024 | 91.8305 | 62.1492 | 200,694.2 | 135,652.7 | 32.41% |
| fold_mixed | 2.2863 | 1.3352 | 5,021.5 | 2,933.2 | 41.59% |
| style_mixed | 1.9496 | 1.3611 | 4,275.1 | 2,980.4 | 30.28% |
| fold_identity | 1.2539 | 1.2488 | 2,755.8 | 2,743.3 | 0.45% |
| expand_0_0_grow | 0.1430 | 0.1383 | 322.4 | 313.8 | 2.67% |
| expand_0_0_spare | 0.1422 | 0.1383 | 322.4 | 313.8 | 2.67% |
| expand_1_8_grow | 0.6828 | 0.5914 | 1,507.3 | 1,306.7 | 13.31% |
| expand_1_8_spare | 0.6695 | 0.5938 | 1,478.2 | 1,311.9 | 11.25% |
| expand_32_128_grow | 61.3508 | 8.3531 | 133,932.7 | 18,306.0 | 86.33% |
| expand_32_128_spare | 61.7406 | 8.2250 | 133,903.6 | 18,026.4 | 86.54% |
| expand_128_8_grow | 106.8227 | 7.0555 | 234,201.4 | 15,498.8 | 93.38% |
| expand_128_8_spare | 106.8445 | 6.8531 | 232,839.8 | 15,056.3 | 93.53% |
| expand_128_256_grow | 489.8250 | 23.8602 | 1,073,296.7 | 52,198.1 | 95.14% |
| expand_128_256_spare | 492.7375 | 22.4281 | 1,075,723.2 | 48,324.3 | 95.51% |
| expand_512_8_grow | 1,573.3461 | 31.6297 | 3,448,101.5 | 69,368.9 | 97.99% |
| expand_512_8_spare | 1,548.7820 | 30.5234 | 3,394,687.5 | 66,962.9 | 98.03% |
| expand_512_1024_grow | 13,534.4680 | 548.5844 | 29,659,961.5 | 1,200,788.5 | 95.95% |
| expand_512_1024_spare | 13,243.7891 | 106.0656 | 29,018,292.7 | 232,514.0 | 99.20% |
| fold_flat_256 | 21.8668 | 19.7738 | 47,952.2 | 43,359.0 | 9.58% |
| style_flat_256 | 19.4070 | 18.1391 | 42,569.3 | 39,768.9 | 6.58% |
| expand_no_retained | 10.2531 | 10.3531 | 22,478.2 | 22,714.0 | -1.05% |
| proxy_capture_385 | 554.3297 | 56.1102 | 1,214,399.2 | 122,884.0 | 89.88% |

A/R/F means allocation/reallocation/free calls. Byte totals include reallocations and the common input copy.

| Scenario | Old A/R/F | New A/R/F | Old requested/freed B | New requested/freed B | Old actors/s | New actors/s |
|---|---:|---:|---:|---:|---:|---:|
| fold_1 | 1/0/1 | 1/0/1 | 400/400 | 400/400 | 2,959,537.6 | 6,736,842.1 |
| style_1 | 1/0/1 | 1/0/1 | 400/400 | 400/400 | 5,939,675.2 | 8,533,333.3 |
| fold_32 | 1/0/1 | 1/0/1 | 12,800/12,800 | 12,800/12,800 | 11,681,163.6 | 21,086,229.1 |
| style_32 | 1/0/1 | 1/0/1 | 12,800/12,800 | 12,800/12,800 | 13,449,351.5 | 21,677,692.5 |
| fold_256 | 1/0/1 | 1/0/1 | 102,400/102,400 | 102,400/102,400 | 11,459,346.0 | 22,605,636.2 |
| style_256 | 1/0/1 | 1/0/1 | 102,400/102,400 | 102,400/102,400 | 13,480,058.4 | 22,420,800.5 |
| fold_1024 | 1/0/1 | 1/0/1 | 409,600/409,600 | 409,600/409,600 | 10,342,373.3 | 17,716,861.6 |
| style_1024 | 1/0/1 | 1/0/1 | 409,600/409,600 | 409,600/409,600 | 11,150,983.0 | 16,476,474.2 |
| fold_mixed | 9/0/9 | 9/0/9 | 8,880/8,880 | 8,880/8,880 | 6,123,355.5 | 10,485,664.1 |
| style_mixed | 9/0/9 | 9/0/9 | 8,880/8,880 | 8,880/8,880 | 7,180,925.7 | 10,285,550.3 |
| fold_identity | 9/0/9 | 9/0/9 | 8,880/8,880 | 8,880/8,880 | 11,165,109.0 | 11,210,509.9 |
| expand_0_0_grow | 1/0/1 | 1/0/1 | 800/800 | 800/800 | 6,994,535.5 | 7,231,638.4 |
| expand_0_0_spare | 1/0/1 | 1/0/1 | 800/800 | 800/800 | 7,032,967.0 | 7,231,638.4 |
| expand_1_8_grow | 1/0/1 | 1/0/1 | 4,000/4,000 | 4,000/4,000 | 14,645,308.9 | 16,908,850.7 |
| expand_1_8_spare | 1/0/1 | 1/0/1 | 4,400/4,400 | 4,400/4,400 | 14,935,822.6 | 16,842,105.3 |
| expand_32_128_grow | 1/1/1 | 1/1/1 | 156,000/156,000 | 156,000/156,000 | 2,624,253.5 | 19,274,223.7 |
| expand_32_128_spare | 1/0/1 | 1/0/1 | 64,800/64,800 | 64,800/64,800 | 2,607,683.4 | 19,574,468.1 |
| expand_128_8_grow | 1/4/1 | 1/1/1 | 124,000/124,000 | 58,800/58,800 | 1,282,499.5 | 19,417,561.7 |
| expand_128_8_spare | 1/0/1 | 1/0/1 | 55,200/55,200 | 55,200/55,200 | 1,282,236.9 | 19,990,880.1 |
| expand_128_256_grow | 1/1/1 | 1/1/1 | 309,600/309,600 | 309,600/309,600 | 785,995.0 | 16,135,686.5 |
| expand_128_256_spare | 1/0/1 | 1/0/1 | 154,400/154,400 | 154,400/154,400 | 781,349.1 | 17,165,946.8 |
| expand_512_8_grow | 1/6/1 | 1/1/1 | 508,000/508,000 | 212,400/212,400 | 331,141.4 | 16,471,866.8 |
| expand_512_8_spare | 1/0/1 | 1/0/1 | 208,800/208,800 | 208,800/208,800 | 336,393.4 | 17,068,850.8 |
| expand_512_1024_grow | 1/1/1 | 1/1/1 | 1,231,200/1,231,200 | 1,231,200/1,231,200 | 113,561.9 | 2,801,756.8 |
| expand_512_1024_spare | 1/0/1 | 1/0/1 | 615,200/615,200 | 615,200/615,200 | 116,054.4 | 14,491,028.5 |
| fold_flat_256 | 1/0/1 | 1/0/1 | 102,400/102,400 | 102,400/102,400 | 11,707,247.4 | 12,946,405.6 |
| style_flat_256 | 1/0/1 | 1/0/1 | 102,400/102,400 | 102,400/102,400 | 13,191,095.4 | 14,113,188.0 |
| expand_no_retained | 1/0/1 | 1/0/1 | 102,400/102,400 | 102,400/102,400 | 24,967,997.6 | 24,726,833.7 |
| proxy_capture_385 | 1/1/1 | 1/1/1 | 309,600/309,600 | 309,600/309,600 | 694,532.5 | 6,861,502.9 |

Per-run paired cycle savings for representative productive cases:

- `fold_256`: 49.23%, 51.09%, 60.81%.
- `style_256`: 39.35%, 40.54%, 39.49%.
- `expand_128_256_spare`: 95.55%, 91.98%, 95.56%.
- `proxy_capture_385`: 89.88%, 89.90%, 89.77%.

Cases with higher median cycle counts are reported explicitly:

- `expand_no_retained`: 1.05% more cycles; elapsed 10253.1 to 10353.1 ns/op.
