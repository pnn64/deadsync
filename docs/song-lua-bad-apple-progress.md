# Bad Apple!! whole-song parity

Frozen chart: `0a977b622e3d2b0f`, `Bad Apple!!/Bad Apple!!.sm`, dance-single
Challenge. The local identity matches the frozen chart. All resources and
reference data came from the local workspace. Nothing was downloaded and the
frozen project manifest is unchanged.

## Compatibility gaps

Native BOOL_INTERFACE methods accept only a boolean first argument, return
the previous boolean, and chain when the second argument is any boolean.
DeadSync now follows that convention for all 40 methods. The generic numeric
argument conversion and boolean approach-speed writes were removed. Native
and production tests cover invalid queries, return arity, chaining with false,
and ignored numeric second arguments.

The audit retains native getters before and after the 42 direct boolean
setters. DeadSync records actual calls during chronological replay only in
test-support builds; missing calls and wrong previous values fail counterchecks.
Direct Song-level turns and transforms retain option state without rewriting
already-loaded notes, matching Player.cpp's separate Stage transform path.

The harness previously omitted the AVI sprite's geometry. It now reads video
stream source dimensions from bounded RIFF headers, skipping audio streams and
movie payloads. Local ffprobe confirms the real movie is 320x240. Two small
valid local AVI fixtures test audio-first streams, stale main-header dimensions,
and negative top-down bitmap height. This covers source geometry, not movie
texture allocation or decoded pixels.

The expanded frame audit also exposed an old endpoint heuristic that snapped
nearby player destinations to the initial position. That heuristic was removed
from production and its implementation-only unit test was deleted. Chronological replay preserves the actual destination instead. A queued tween
whose tiny remaining duration rounds onto the prior timestamp now starts after
that prior frame. Following tweens also wait for preceding zero-time states to
reach their actor update before displaying the copied destination. Both changes
preserve native queue order. The frame comparison retains the existing tolerance.

## Retained reference

The reference covers both players using cel at 854x480, seed 1, 60 FPS and beat
step 0.125, through beat 416 (208.69525146484375 seconds). It contains 1,874
events, one Lua layer, zero runtime errors and zero dropped events. It retains
8,013 transform changes per player, 42 boolean calls, and the movie geometry
and texture binding. Texture checks verify both the saved path and the actual
renderer key. Every player transform axis is compared through the production
gameplay evaluator on all 12,523 updates, including quiet frames. The movie also
goes through the production render-list path, with a countercheck for a missing
binding.

The 2,260,446-byte JSON compresses to 231,042 bytes with an exact round trip.
Raw SHA-256:
`d73ac9b597b81dbda491d23b4ff0b0082f205a94f77393840506aaf2f7058cc8`.
Provenance pins 33 local source files.

## Validation and publication

The complete audit passes **275,833/275,833** comparisons at the existing
tolerances, including 275,506 player transform axes. REWORK and MAIN each
independently pass 135 semantic tests and 965 song-Lua package tests; the
harness passes 145 tests. MAIN is `0.5.1758`, exactly one patch above
`0.5.1757`, with Cargo.toml and Cargo.lock included in the curated commit.
REWORK keeps its changes uncommitted.

These checks cover no-input headless execution, production gameplay evaluation,
local production movie poster decoding and render lists. Full movie frames,
audible playback, live-score branches and interactive GPU screenshots are not
verified. Other frozen/local identity
discrepancies remain in
[`song-lua-mawaru6-progress.md`](song-lua-mawaru6-progress.md) for user correction.
