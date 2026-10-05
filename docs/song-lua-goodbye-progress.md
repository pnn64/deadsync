# Goodbye whole-song parity

Frozen chart: `b21f3f83be7fbaad`, `Goodbye/Goodbye.ssc`, dance-single
Challenge. The local identity matches the frozen chart. All resources and
reference data came from the local workspace. Nothing was downloaded, and
the frozen project manifest is unchanged.

## Gaps fixed

The previous trace projected the hidden image cache but omitted all 51 visible
face sprites that borrow its textures. The harness now retains immutable image
handles with native PNG source, frame, image, and texture dimensions. It records
aliased texture paths and all 529 binding or size changes. Source image handles
remain valid when their original sprite loads another image.

DeadSync now preserves frame dimensions on borrowed texture handles, including
sprite sheets. The old whole-sheet getter fallback was removed. Logical PNG
texture extents match the native padded power-of-two values; decoded physical
image sizes and render UVs remain separate. Parent callbacks that enqueue a
zero-duration child state after the child's update preserve the current final
draw instead of applying the pending destination early. The initial zero-delta
callback's queues are retained too, preserving the child's initial draw.

The flat sprite and quad path now converts native rotation to the renderer's
Y-up coordinates. Previously, rotated Goodbye faces turned in the opposite
direction even though their bounds and state matrices passed the audit.
Tilted and off-center rotated sprites and quads use the existing native matrix
path with an orthographic projection. This preserves the tilted alignment
pivot and combined-axis geometry instead of folding angles around the center.
The projection uses ITGmania's orthographic depth range of -1000 to +1000;
the actual face checks reject mesh vertices outside clip depth.

Shadow offsets and colors are immediate state in ITGmania, outside its tween
state. DeadSync now applies these setters immediately even after `sleep(0)`.
The former size-only capture helper was replaced by a shared immediate vector
capture. The reference records current shadow offsets and color separately
from pending child color and crop changes.

Goodbye creates its invisible image cache with Lua `pairs`, whose order differs
between Lua versions. The audit now verifies the same complete drawable
membership and strict visible draw order. Only uniquely named resources proven
invisible for their full lifetime, with no proxy target, can differ in declaration
order. Renaming, removing, revealing, or reordering visible actors fails retained
counterchecks. Projected actors map by unique identity so texture checks cannot
accidentally compare two different hidden resources.

## Complete reference

The expanded trace passes **118,437/118,437** observations: 4 compile checks,
2 layer checks, 252 final render checks, 529 texture bindings, 13,894 geometry
checks, 27,352 color checks, 14,548 crop channels, 21,822 shadow channels,
3,637 vibration checks, one timeline check, and 36,396 runtime modifier targets.
No tolerances were widened. The regression also exercises the production
renderer for the first and last on-screen sample of all 51 visible faces,
including cropped and rotated corners and world-space shadow offsets. It
rejects missing alias bindings and crop updates. The original projected corners
describe the uncropped plane; fully cropped faces produce no draw in either
engine, following local `Sprite::DrawTexture`.

The trace covers both players with cel at 854x480, seed 1, 60 FPS, beat step
0.125, through beat 223.5 (124.16666666666669 seconds). It contains 913,311
events, one Lua layer, zero runtime errors and zero dropped events. The
38,073,690-byte JSON compresses to 1,946,900 bytes with an exact round trip.
Raw SHA-256:
`f1ad4f59f867cfab96ea43723c8ca0748288ebb89e2d06300bd1ecc24887cbab`.
Provenance pins 102 local source files. Four native micro references cover
immutable texture aliases and backward seeking, hidden-cache ordering, and
the last parent callback's pending child color, plus immediate shadow setters
alongside queued child color. Each retains checks that reject the old behavior.

## Validation and publication

REWORK and MAIN independently pass all 134 semantic tests and 966 song-Lua
package tests. The local harness passes all 144 tests. The semantic runs include
the complete Goodbye reference, production face draw checks, all four new
micro references and their counterchecks, and all prior retained whole songs.
MAIN is version `0.5.1757`, exactly one patch above `0.5.1756`, with Cargo.toml
and Cargo.lock included in the curated commit. REWORK keeps its changes
uncommitted.

These checks cover headless execution, modifier evaluation, and production
render lists. They do not verify interactive GPU screenshots or audible
playback. Other frozen/local identity discrepancies remain in
[`song-lua-mawaru6-progress.md`](song-lua-mawaru6-progress.md) for user correction.
