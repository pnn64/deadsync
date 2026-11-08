# I (Ai) whole-song parity

Frozen/local chart `f3905e4e0bb5398f` matches `I (Ai)/I (Ai).ssc`, dance-single
Challenge, `MODS BR+ BT FS XO BU-(24ths)`. The Edit chart shares its note hash
but takes a different branch; the reference explicitly selects Challenge.
Both background and foreground layers are retained. Resources came entirely
from the local workspace; song files and frozen identities remain unchanged.

The earlier reference ended at the last note, beat 444. The final blackout and
TV shutoff execute when the beat exceeds 444, so that capture missed them.
The discovery bridge now exports Song::GetSpecifiedLastSecond and
Song::GetSpecifiedLastBeat using native parsing and TimingData. The baseline
uses the later of the note endpoint and the native specified endpoint.
Discovery wire version 2 reflects the added fields; the semantic request wire
is unchanged. Explicitly bounded micro-fixtures retain their endpoints.

I (Ai)'s native LASTSECONDHINT is 119.83146667480469 music seconds, beat
445.78375244140625. Trace seconds are relative to beat zero, so this is
118.87566731770832 trace seconds. An optional `--until-beat` minimum extends
this capture through beat 450 / 120 trace seconds, covering the completed TV
effect at beat 446.125 and its following quiet frames. It cannot shorten the
native endpoint. A generated local fixture tests the hint, minimum extension
and rejection of invalid endpoints without downloading anything.

The final capture contains 39,066 events, no Lua errors or dropped events,
7,201 update frames and 84 projected samples across six drawables. The audit
passes 241,877 comparisons, including 86,412 alpha/visibility comparisons at
every drawable update, runtime modifiers, both layers and projected geometry.
A deliberately corrupted TV alpha track must fail the frame audit.

Another 598 comparisons check production image geometry/tint and two empty
draw results when the TV reaches zero size. The shared draw audit now checks
the absence of corners, sprite instances and draw operations for a collapsed
native plane. This replaces its unconditional assumption that every visible
positive-alpha image produces a sprite. Existing image and capture audits
retain their comparison counts and tolerances.

The three background star sprites remain invisible in the native capture.
Their authored ShowBG2MessageCommand is not invoked by playcommand("ShowBG2").
The audit retains their invisible state on every frame without changing the
song to force an effect. The foreground blackout, TV and white flash use the
local 800x600, 2880x416 and 1280x720 images; production decoding verifies these
source dimensions before composition.

The 4,542,252-byte reference compresses to 195,801 bytes with an exact round
trip. Raw SHA-256:
`5675f99d47fee2b7f6bc3ed5816b1307996201ca08e48932ac6f8188348267f1`.

REWORK and MAIN each independently pass 138 semantic tests and 965 song-Lua
package tests; the reference harness passes 148 tests. MAIN is `0.5.1761`,
exactly one patch above `0.5.1760`, with both Cargo files in the curated
commit. REWORK retains its changes uncommitted.

This checks no-input execution and production render lists. It does not
simulate ScreenGameplay ending transitions, capture native framebuffer pixels,
verify audible playback or test interactive input. Existing frozen/local hash
discrepancies remain unchanged in [the inventory report](song-lua-mawaru6-progress.md).
