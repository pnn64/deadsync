# Igaku whole-song parity

Frozen chart: `3e9fa53d70b9aa19`, `Igaku/Igaku.ssc`, dance-single Challenge,
description `FS XO BR DS MODS`, meter 11. The local chart matches the frozen
identity. Song sources, resources, and the frozen project manifest are unchanged.
All resources come from the local workspace; nothing was downloaded.

## Reference and fixes

The first local audit passed 325,132 of 331,102 comparisons. Its first DeadSync
error occurred at beat 116.025: the duplicate-player spawn callback tried to add
an offset to a missing `ScreenGameplay.PlayerP1TwoPlayersTwoSidesX` metric.
This interrupted the handler that controls later messages and modifiers.

The harness also lacked these metrics and substituted zero. Both implementations
now use the checked-out Simply Love metrics: side fields sit a quarter of the
clamped logical width from center, while double/shared fields are centered.
At 854 pixels, the side positions are 213.5 and 640.5. DeadSync uses the actual
compile-context width, including widths outside the 640–854 clamp interval.

`BeatPeriod` now passes through attack parsing, Lua easing, active/persisted
targets, approach speeds, resets, and the note renderer. The X wave uses native
`ArrowEffects::GetXPos`'s divisor `(BeatPeriod * 15) + 15`. Composed notes and
hold-body/cap meshes are checked with positive and negative periods.
Direct update writes also pass the Lua capture whitelist; option-string writes
alone had left the first part of the authored pulse unrepresented.

The next interrupted callback, at beat 211.792, called native snake-case column
and spline methods. Those names now share the existing handlers, including
`get_rot_handler`, `get_spline`, and the spline setters. Igaku's single-point
looped rotations are captured by the production column evaluator. The audit
now checks all 680 recorded rotation writes and rejects a missing rotation track.

The harness's old `ArrowEffects.GetYPos` always assumed normal scroll. Its
replacement follows the local `ArrowGetReverseShiftAndScale`, querying linked
native option amounts for Reverse, Mini, and Centered. Column reverse amounts
include lane Reverse, Split, Alternate, Cross, and native folding. DeadSync's
Lua query uses the same reference behavior. Native getter names share handler
identity in the harness as well. Regression checks cover ordinary/reversed
fields, custom receptor spacing, negative/folded amounts, and four/eight columns.

The multitap comparator previously searched each native actor in every song
layer, producing 56 false missing-actor checks in Igaku's background layer.
It now resolves ownership from the initial definition/instance parents, retaining
earlier writes even if a child is later removed. A regression covers equal actor
names in different layers, removed children, and an actual missing foreground
actor. The first scoped audit had 331,057 comparisons, retaining observations
in their owning layers. Restored late updates add 15 projected checks, and the
rotation audit adds 680 recorded column writes.

The regenerated local reference has 317,331 events, two Lua layers, zero runtime
errors, and zero dropped events. Seed 1, both players, cel, 854×480, 60 FPS,
beat step 0.125, traced through beat 335.5. Its raw SHA-256 is
`faff9c975146cfc2d7bb18528de3e9a55dc967d837c66c8d50562e593a62b5cd`.
The 16,269,777-byte JSON compresses to 737,288 bytes with an exact round trip.
The fixture provenance pins 35 local reference, harness, and song files.

## Validation and publication

Igaku passes all **331,752/331,752** whole-song comparisons, including all
680 recorded column rotations and all 70,548 runtime modifier targets.
All layer, final render, proxy, range, multitap, geometry, color, vibration,
timeline, and message checks pass. The missing-rotation countercheck fails
as intended without omitting observations or loosening tolerances.

Rework and MAIN independently pass the same 127 semantic tests and 2,572
package tests, 2,699 per repository. The harness passes 142 tests. MAIN is
version `0.5.1754`, exactly one patch above `0.5.1753`; Cargo.toml and Cargo.lock
are included in the curated commit. Song sources and frozen identities remain
unchanged. Rework retains the changes without committing them.

These checks cover a no-input native trace and production renderer geometry;
they do not constitute an interactive GPU screenshot or audible playback check.
Frozen/local hash discrepancies remain recorded in
[`song-lua-mawaru6-progress.md`](song-lua-mawaru6-progress.md) for user correction.
