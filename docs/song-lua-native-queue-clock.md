# Native queued-command timing

ITGmania's `Actor::UpdateTweening` subtracts float frame deltas from float
tween durations. DeadSync used double subtraction and snapped very small
remainders to zero. This can move a queued command across a frame boundary,
changing shared state, modifier writes and later random choices.

Use native float subtraction for the compile-frame delta, pending queue
steps and recurring command countdown. Retain a zero-time command until
the frame has positive delta left. Remove the small-remainder snap and the
comment that incorrectly treated the headless harness's double arithmetic
as native behavior. Song timestamps and chart timing retain their existing
precision; actor update callbacks receive the native float frame delta.

The local harness also used double tween countdowns. Its float replacement
is checked against linked ITGmania actors by `tween_boundaries_match_native`.
Refresh the affected micro references using the local original Lua files,
the same end beat and sampling cadence. Keep strict parity assertions and
update their comparison counts to reflect the newly captured operations.
The recurring-stop regression still asserts the native final 151 calls.
No comparison tolerances are widened.

The refreshed fixtures are `queue-recurring`, `queue-vibration`,
`global-probe`, `global-probe-sibling`, `paused-updates`, `queued-effects`,
`queued-chain`, `queued-fade`, `callback-phase`, `finish-queue`,
`phase-update`, `recurring-follow`, `recurring-follow-offset`,
`recurring-stop` and `retarget-update`. Each capture has zero runtime
errors and dropped events. Compressed files have an exact zstd round-trip
check. Original captures are retained locally under `.tmp/float-native-micros`.

Validation: all 781 song-Lua unit tests, 102 regular DeadSync semantic
tests and 131 local harness tests pass.
The queued-vibration unit expectation follows the refreshed native
`DoneCommand` frame at beat 1.8166667.

Mawaru8's audit against the float reference improves from 94,778/97,234
before this change to 96,618/97,327. All 6,854 update-value comparisons,
254 modifier-target comparisons and 1,088 final actor-state comparisons
pass. The additional comparisons reflect changed native command frames;
these totals do not constitute complete song parity.

Investigating the remaining random-placement differences found another
reference gap: `GetSecsIntoEffect()` returned zero in the harness. Mawaru8
uses that getter to advance its twin minigame. The harness now returns the
actor's clock and effect delta; a separate regression checks 724 getter
values exactly against actual ITGmania actors, including timer wrapping
and the music clock.

The complete unmodified Mawaru8 capture with both corrections reports
98,165/98,602. The closing sparkles now match. Its 16,118 frames reach
beat 716 with zero native runtime errors or dropped events. All update
values, modifier targets, final render state, render persistence,
vibration, timeline and observed message-command checks pass. Remaining
failures are five speculative message captures, two player rotation
ranges and 430 projected-geometry comparisons (two actor animations and
visibility boundaries). Mawaru8 is still not a complete pass.

The new local capture is
`.tmp/mawaru8-clock-float-native/mawaru8.sm.semantic.json` in rework, with
SHA-256 `f4ee6112290cc8f5a8ae69633ade4e955f052dee98145ad329567b43dfe9e3a9`.

Capture provenance: clean local ITGmania revision
`5b205125ad53b9867bb4a494ff858f8d38ad4406`; linked native timing and actor
oracle; bundled Lua; seed 1; dance-single Challenge; both human players;
854x480; 60 Hz updates. The semantic host SHA-256 is
`8254e16e41b79a833adda87b5ed9de07920ae04556800073ca72155b6b4fdf10`.
Short captures without a simfile retain continuous-BPM song timing; the
SM/SSC captures use the native song clock, including chart pauses.

The supplied local Mawaru8 chart hash is `8224fb7e0b05040f`; the frozen
project lists `cadefe09888e9ab8`. The remaining local mismatches are
Mawaru5 (`8e0b6274cb33af5e`, project `74765c1936186d20`) and Brain Power
(`f40ebaf45ea6e26d`, project `a73ec5f2f3015620`). Get Into It
(`9c208360a9b25133`) and Rhythm Hell (`be38aa9e3c88c32b`) were not found
in the local song resources. No song resources were downloaded and the
frozen project metadata remains unchanged.
