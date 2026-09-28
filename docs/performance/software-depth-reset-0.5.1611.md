# Lazy software depth resets

Baseline: `560d55905` (0.5.1611). Review range: the eight commits in
`11a60726b..189902d20` since the previous performance pass.

## Finding and change

`10bf77082` gave the software renderer a depth buffer to isolate 3D mines. Every
frame filled the whole buffer (8.3 MB at 1920x1080) before drawing, even when no
draw tested depth. Every depth reset (`ClearDepth`) filled the whole buffer again,
one row stripe at a time. A mine requests a reset before and after itself. With
N mines between other draws, that is 2N+1 full fills per frame, or 65 for 32 mines.

Only depth-tested textured meshes read or write depth. `draw_rows` now marks a
reset as pending. It fills a stripe's depth just before the first depth-tested
draw that reaches that stripe. The eager fills at frame start and for offscreen
passes are removed. A stripe that no depth-tested geometry reaches never writes
depth. Rasterization and depth comparison are unchanged. Every depth-tested draw
still sees the same reset history.

This follows `rust-performance.md`'s M-HOTPATH and M-THROUGHPUT guidance ("avoid
empty cycles"). It adds no allocation, cache, unsafe code, or dependency.

## Behavior validation

A temporary harness compiled the baseline backend source and the changed source
into one binary. It mirrored `draw` after surface acquisition: prepare, bin,
clear, and rasterize. It rendered four fixed 1080p scenes and 160 random op
streams. The streams randomized reset flags, depth modes, backface culling, model
depth, and full-screen models. Each list was replayed forward, then backward,
through one renderer, so every frame inherited the previous frame's stale depth.
This ran on 1 thread (single-pass path) and 8 threads (stripe bins and staged
meshes). Pixels and vertex counts matched for all 656 frames. A mutation that
ignored reset markers failed on the second frame. The harness is not committed.

Committed unit tests:

- `model_depth_groups` now starts from stale depth.
- `depth_resets_skip_rows_without_depth_draws` checks three things:
  - a stale buffer is reset before its first use;
  - a reset request isolates a later, farther model;
  - staged stripes without depth-tested draws are never written.

All six software test targets pass: 84 test executions, because the include-based
harnesses rerun the unit tests. Three manual benchmarks are ignored. The
performance Clippy lint passes; existing style warnings remain.

```powershell
cargo test --locked -p deadlib-render-backend-software
cargo clippy --locked -p deadlib-render-backend-software --lib --tests -- -D clippy::perf
```

## Measurement method

Windows x86-64, Intel Xeon E5-2696 v4, Rust 1.98.1 / LLVM 22.1.8. Repository
release profile (`opt-level=3`, full LTO). Both variants ran in the same
executable against the same frames, textures, and warmed buffers. Each 1920x1080
frame contains:

- a full-screen background sprite;
- 216 note sprites (64 px);
- a 600-glyph text mesh.

The mine scenes add three depth-tested, back-face-culled layers per mine (320,
192, and 32 triangles). The layers carry reset flags like
`compose_flat_mine_layers` output, and note runs sit between mines. The
full-screen depth scene adds one large depth-tested model and no resets. No
surface is presented and no GPU is involved.

- **Single-threaded:** pinned to logical CPU 6. Cycles come from
  `QueryThreadCycleTime`.
- **8 threads:** a rayon pool pinned to logical CPUs 8-22 (even numbers only; one
  per physical core). Cycles come from `QueryProcessCycleTime`, which covers
  all threads.
- **Timing:** each variant ran warmup frames, then seven batches of 24 frames. The
  result is the median of the batch means. There were three runs, with variant
  order reversed in run 2.
- **Allocations:** counted on the calling thread over one warmed frame.

## Results

Times and cycles are medians of the three run medians. Reductions are the median
paired reduction, with the range across the three runs. Raw results are in the
[CSV](software-depth-reset-0.5.1611.csv).

| Scene | Threads | Before ms/frame | After ms/frame | Time reduction (range) | Cycles/frame before -> after | Frames/s before -> after |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| No depth draws | 1 | 233.9 | 230.6 | 0.0% (-0.2 to 1.5%) | 512M -> 505M | 4.28 -> 4.34 |
| 8 model mines | 1 | 248.2 | 241.6 | 2.7% (2.6 to 2.7%) | 544M -> 529M | 4.03 -> 4.14 |
| 32 model mines | 1 | 293.9 | 262.6 | 10.4% (10.2 to 10.7%) | 644M -> 575M | 3.40 -> 3.81 |
| Full-screen depth model | 1 | 447.4 | 438.8 | 1.9% (-0.4 to 3.0%) | 980M -> 961M | 2.23 -> 2.28 |
| No depth draws | 8 | 36.0 | 36.0 | -0.1% (-0.3 to 0.3%) | 565M -> 566M | 27.79 -> 27.80 |
| 8 model mines | 8 | 38.4 | 38.0 | 1.2% (0.9 to 1.5%) | 596M -> 589M | 26.04 -> 26.35 |
| 32 model mines | 8 | 45.0 | 42.2 | 6.4% (6.3 to 6.7%) | 685M -> 643M | 22.22 -> 23.70 |
| Full-screen depth model | 8 | 69.5 | 69.6 | -0.2% (-0.6 to 0.0%) | 1,065M -> 1,068M | 14.39 -> 14.37 |

The gain scales with the number of mine resets. With 32 mines, single-threaded
work drops by about 31 ms and 69M cycles per frame. With 8 threads, frame time
drops 2.8 ms and total CPU work drops 42M cycles. With no depth draws, only the
frame-start fill is saved. That is below the noise of a frame this size, so no
gain is claimed there. The full-screen depth model still fills every stripe once,
so it is expected to be neutral. Its single-threaded range includes a
regression, and its 8-thread results are flat.

Every measured frame had zero allocations, reallocations, and frees on the
calling thread, before and after. Absolute frame times depend on this scalar
fixture. They are not game frame rates, and they say nothing about GPU backends.
