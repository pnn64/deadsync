# Mawaru7 parity investigation

The target is the local `mawaru7/mawaru7.sm` dance-single Challenge chart,
description `TaroNuke (converted by MrThatKid4)`, meter 12, frozen hash
`f7fdd8fafaee6188`. The frozen project manifest is unchanged. All song assets
and reference code used in this investigation are local; nothing was downloaded.

## Corrections

- Model Simply Love's Judgment ActorFrame and its `JudgmentWithOffsets` Sprite.
  Preserve song-driven sheet loads and dimensions, prewarm every sheet at the
  gameplay transition, and select the active sheet when drawing judgments.
- Restore command queues before replaying a cross-actor message probe. Queue
  mutations from the first probe must not contaminate the second probe.
- Consume the current frame delta when a parent starts a child's recurring
  command after the frame's update plan was built.
- Preserve empty tween tails. A command propagated to an already cycling child
  must write the tail after its sleep, rather than retarget an earlier tween.
  A setter changing the back tween must also leave earlier active tween
  destinations intact, including on the frame a recurring command restarts.
- Implement `Actor::ZoomTo`: write zoom axes relative to intrinsic dimensions.
  `setsize` and `zoomto` have different contracts. Keep post-queue startup zoom
  writes scheduled until a positive update, while visibility remains immediate.
- Advance spin tween countdowns with native float subtraction and exact zero
  completion. Double countdowns with an epsilon can release a tween one frame
  early and change subsequent spin rotations.
- Include the shared ScreenGameplay translation before projecting captured
  overlay geometry. The live presentation path already applies that translation.
- Preserve generic Sprite texture loads as a song-time binding timeline and
  prewarm all recorded images at the gameplay transition. The renderer must
  select the active image and its frame mapping, including after a backward
  seek. Geometry alone cannot detect same-size texture swaps.
- Preserve the initial custom `Frames` table during startup queue discovery.
  `Load` resets states; `SetTexture` keeps their UV mapping from the previous
  sheet. Record intrinsic size changes immediately, as the native Sprite does.
  Apply `setstate` immediately too, so a same-command selection follows the
  load reset. Cached loads use the same dimension/state update path.
- Audit queued startup setter arguments before the same frame advances their
  tween. Zero-duration easing names have no effect on their completed state.

The independent C++ geometry fixtures construct fixed-size actors. Their Lua
counterparts now use `setsize` explicitly instead of relying on the former
incorrect `zoomto` implementation. Native geometry, sampling and tolerances
were preserved. Refreshed semantic references also contain draw-color checks
that were absent from some older captures; their exact check counts increased.

## Reference generation

The reference tree is clean ITGmania revision
`5b205125ad53b9867bb4a494ff858f8d38ad4406`. The harness independently checks
`zoomto` against linked C++ Actor geometry. Whole-chart JSON is streamed in
64 KiB chunks to a native buffer, retaining every observation while avoiding
large nested temporary Lua strings. Both new small references are byte-for-byte
identical with the earlier serializer. A large escaped-name fixture verifies
that arrays, strings and actor records survive multiple output chunks.

The retained `parent-rotation-cycle` case passes 543 comparisons, and
`zero-message-size` passes 498 comparisons in the initial focused audits.
Their provenance files pin local source bytes and the native capture hashes.

The refreshed whole-song capture completed. Adding generic texture binding
checks produced 874,663 of 874,665 passing comparisons; two quest-arrow
bounds differ at beat 198.887.
The `repeat-boundary` fixture reproduces the lost rotation frame at 78.017s.
All 72 recorded Sprite loads match native resource selection. The retained
whole-song reference has 618,106 events, no runtime errors and no dropped
events; its compressed form round-trips to the exact native bytes.
The final automated whole-song audit passes all 874,665 comparisons,
including the two quest-arrow bounds at the recurring-command boundary.
The retained boundary case passes 4,915 comparisons; the Sprite load case
passes 23 native comparisons and the actual renderer checks its texture keys
and UVs before/after swaps and a backward seek. All 122 regular semantic tests
pass, including the retained Mawaru5 and Mawaru8 local regression guards.
The frozen Mawaru7 chart identity matches the local chart. The no-input
headless audit does not constitute an interactive GPU screenshot or audible
playback check.

## Main repository validation

The MAIN copy is version `0.5.1752`, exactly one patch above `0.5.1751`.
It independently passes all 122 regular semantic tests, including the full
Mawaru7 audit, and the song Lua/player bridge package suites. `Cargo.toml`
and `Cargo.lock` are included; frozen chart hashes remain unchanged.

## Local chart identity issues

The following frozen hashes are intentionally unchanged for the user to review:

| Chart | Frozen hash | Available local hash |
| --- | --- | --- |
| Mawaru5 | `74765c1936186d20` | `8e0b6274cb33af5e` |
| Mawaru8 | `cadefe09888e9ab8` | `8224fb7e0b05040f` |
| Brain Power - [Cardboard Box] | `a73ec5f2f3015620` | `f40ebaf45ea6e26d` |

The frozen GetIntoIt chart (`9c208360a9b25133`) and RhythmHell chart
(`be38aa9e3c88c32b`) were not found in the available local inventory.
