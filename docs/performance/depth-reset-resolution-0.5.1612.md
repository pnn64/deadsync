# Resolve depth resets before rendering

Baseline: `11db554e3` (0.5.1612). Review range: the eight commits in
`11a60726b..189902d20` since the previous performance pass.

## Finding and change

`10bf77082` isolates each 3D mine by requesting a depth reset before its first
visible mesh and after its last. `finish_frame` copied those requests into
`TexturedMeshRun::clear_depth` and `clear_depth_after`. Nine backend loops then
rebuilt "clear after the previous op" from them: OpenGL (4), Vulkan (2), wgpu,
Metal (2), plus the software preparation loop.

Every backend already starts each pass with cleared depth, and only depth-tested
runs read or write it. Three kinds of reset were therefore redundant:

- the first reset in a pass;
- one of the two resets between neighboring mines, when other draws sit between
  them;
- the reset after the last mine.

Redundant resets are expensive on some backends. wgpu and Metal reset depth by
drawing a full-screen triangle that writes depth, and then drop their cached
bindings. Vulkan records a full-extent `vkCmdClearAttachments`, and OpenGL calls
`glClear`.

`finish_frame` now resolves the requests itself. A pending request becomes
`clear_depth` on the next depth-tested run, but only if an earlier depth-tested
run in the pass may have written depth. `TexturedMeshRun::clear_depth_after` is
removed, and each backend loop now makes a single check. The nine copies of
"clear after" tracking are gone.

`TexturedMeshRun` shrinks from 32 to 24 bytes; `DrawOp` stays at 32. Producer
flags on actors, flat draws, and payloads are unchanged, and so is instancing.
This follows `rust-performance.md`'s M-THROUGHPUT and M-HOTPATH guidance. It adds
no allocation, cache, unsafe code, or dependency.

## Behavior validation

- **Committed property test:** `depth_resets_follow_depth_writes` generates 2,048
  random item streams. They mix sprites, skipped meshes (empty or untextured),
  instanced and transient meshes, depth modes, and reset requests. The test checks
  three things:
  - each depth-tested instance sees exactly the depth writes that the producer's
    requests leave visible;
  - no reset lands on a run without depth testing;
  - every reset discards at least one write.

  Three mutations each fail this test: removing the written-depth check,
  dropping after-requests, and resetting on runs without depth testing.
- **Updated tests:** `model_depth_resets`, the software depth tests, and the GPU
  test `model_depth_isolation` (Vulkan via wgpu, OpenGL, Vulkan) now follow the
  new contract. All of them pass.
- **Temporary same-binary A/B (not committed):** a frozen `11db554e3`
  `finish_frame`, replayed with that version's backend rule, was compared with
  production. The comparison covered 5 scenes and 20,000 random streams. Results
  were identical apart from reset flags: ops, instances, and geometry counts. Every
  depth-tested instance saw the same earlier depth writes, and the new version
  never issued more clears.
- **GPU pixel check:** a 1920x1080 frame with 32 mines was rendered with both the
  baseline's 64 reset positions and the new 31. Captures were identical on Vulkan,
  Vulkan (wgpu), DirectX 12 (wgpu), and OpenGL. OpenGL (wgpu) and software cannot
  capture; the op-level comparison covers them.
- **Metal:** the library and its tests type-check for `x86_64-apple-darwin`, and
  the performance Clippy lint passes. Metal was not run.

`cargo check --workspace --tests` passes except for two test targets. Both fail
before and after this change, because earlier commits left them stale; neither
touches this change:

- `deadsync-noteskin` `safe_parsing`: `script_random` is private.
- `deadsync-song-lua` `actor_capture_perf`: its frozen baseline lacks the new
  `Actor` fields.

Tests pass for `deadlib-present`, `deadlib-render-core`, `deadlib-render`, and the
software, GL, Vulkan, and wgpu backends.

## Measurement method

Windows x86-64, Intel Xeon E5-2696 v4, NVIDIA GeForce GTX 1650 SUPER (driver
32.0.15.9186), Rust 1.98.1 / LLVM 22.1.8.

**Composition.** `finish_frame` was measured in a temporary release example
(`panic=abort`, full LTO), pinned to logical CPU 6. An unwinding test binary is
misleading here: it changed `finish_frame` code generation and produced a false
5-12% slowdown in the no-mine scene. Each of five runs alternated variant order.
A run took the median of seven batches of 4,096 calls, with a fresh builder per
call. Builder setup is excluded, and clock overhead is included equally for both
variants.

**Backends.** Each backend drew one prepared frame through `deadlib-render` into
a hidden 1920x1080 window with immediate presentation. The process was pinned to
8 physical cores. The frame contains:

- a background sprite and 216 note sprites;
- a text strip;
- 32 mines, each made of three depth-tested, back-face-culled layers and preceded
  by a hold-strip mesh.

In this frame, every baseline reset falls on a mesh run. The baseline's 64 reset
positions can therefore be replayed exactly with the single flag; the new
resolution uses 31. Each variant drew 30 warmup frames, then seven batches of
120 frames (8 for software). Reported per frame:

- wall time;
- calling-thread cycles (`QueryThreadCycleTime`);
- process cycles (`QueryProcessCycleTime`, which includes driver threads);
- allocations on the calling thread, counted over one extra frame.

There were three runs, with variant order reversed in run 2.

Depth resets issued by the backends per frame (from the A/B):

| Scene | Before | After |
| --- | ---: | ---: |
| No mines | 0 | 0 |
| 8 mines between note runs | 16 | 7 |
| 32 mines between note runs | 64 | 31 |
| 32 adjacent mines | 33 | 31 |
| 32 mines after hold strips | 64 | 31 |

## Results

Values are medians of the run medians. Reductions are median paired reductions,
with the range across runs. Raw results are in the
[CSV](depth-reset-resolution-0.5.1612.csv).

### Backends, 32 mines at 1920x1080

| Backend | Before µs/frame | After µs/frame | Time reduction (range) | Thread cycles | Process cycles | Frames/s |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Vulkan (wgpu) | 1,497.0 | 1,002.4 | 33.0% (32.4 to 33.5%) | 1,439k -> 1,389k | 1,451k -> 1,398k | 668.0 -> 997.6 |
| DirectX 12 (wgpu) | 2,011.0 | 1,392.5 | 30.8% (29.8 to 32.5%) | 2,130k -> 1,895k | 2,136k -> 1,901k | 497.3 -> 718.1 |
| OpenGL (wgpu) | 3,466.0 | 3,266.4 | 5.8% (2.2 to 8.2%) | 7,512k -> 7,082k | 7,515k -> 7,085k | 288.5 -> 306.1 |
| OpenGL | 496.4 | 470.5 | 6.6% (3.9 to 6.8%) | 1,076k -> 1,018k | 1,077k -> 1,019k | 2,014.5 -> 2,125.4 |
| Vulkan | 305.4 | 294.2 | 3.7% (-1.4 to 5.5%) | 644k -> 636k | 646k -> 639k | 3,274.0 -> 3,398.9 |
| Software | 52,131.3 | 51,748.8 | 0.6% (0.3 to 1.1%) | 4,180k -> 4,175k | 797,041k -> 797,172k | 19.2 -> 19.3 |

On wgpu the frame is GPU-bound: wall time falls about 30%, while calling-thread
work falls only 4-11%. Each full-screen reset triangle removed saves about 15-19 µs
of GPU time on this card. OpenGL and native Vulkan gain less, because their
clears are cheap; one of the three Vulkan runs was slower. Software resets were
already applied lazily per stripe in `11db554e3`, so software is neutral.

Every run had the same allocation count. The native backends and software made
zero allocations, reallocations, and frees per frame. The wgpu backends allocate
internally on every frame:

| Backend | Allocations | Reallocations | Requested bytes |
| --- | ---: | ---: | ---: |
| Vulkan (wgpu) | 102 -> 102 | 27 -> 24 | 152,005 -> 137,669 |
| DirectX 12 (wgpu) | 88 -> 88 | 26 -> 22 | 138,485 -> 133,621 |
| OpenGL (wgpu) | 97 -> 97 | 36 -> 32 | 1,253,009 -> 1,248,145 |

### Composition (`finish_frame`)

| Scene | Before ns/frame | After ns/frame | Time reduction (range), wins | Thread cycles | Million items/s |
| --- | ---: | ---: | ---: | ---: | ---: |
| No mines (217 sprites, 8 text meshes) | 611 | 583 | 5.4% (2.0 to 7.8%), 5/5 | 2,683 -> 2,551 | 368.4 -> 385.9 |
| 8 mines between note runs | 1,463 | 1,438 | 3.4% (-3.8 to 3.9%), 4/5 | 4,502 -> 4,414 | 170.2 -> 173.2 |
| 32 mines between note runs | 3,909 | 3,891 | 1.4% (-1.3 to 2.5%), 3/5 | 9,948 -> 9,983 | 82.1 -> 82.5 |
| 32 adjacent mines | 4,053 | 3,993 | 2.0% (-2.0 to 5.2%), 3/5 | 10,336 -> 10,179 | 79.2 -> 80.4 |
| 32 mines after hold strips | 5,318 | 5,194 | 4.0% (0.9 to 5.0%), 5/5 | 13,099 -> 12,746 | 66.4 -> 68.0 |

Resolving resets adds two flags to the textured-mesh branch. Composition cost is
unchanged within noise. The small median gains are consistent with the smaller
run type and with code layout, not with removed work, so no composition speedup
is claimed. Every call made zero allocations, reallocations, and frees.

These fixtures isolate depth resets. They are not full game frames; the gain in
a real song depends on how many 3D mines are on screen.
