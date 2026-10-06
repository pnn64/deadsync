# Finite current result (0.5.1795)

The frozen project single Edit chart `finite/sta - Finite.sm`, hash
`c4ba45c6133c8abd`, description `TaroNuke (Mods 17)`, passes
**99,928/99,928** existing comparisons. Its seed, style, difficulty,
description and hash are pinned. Previously pinned Challenge selections
remain identical when difficulty becomes explicit in the shared guard.

The earlier reference reports 26,590/26,666 with current code. A complete
local capture using corrected native float song beats and countdowns closes
the stale schedule differences but exposes two real Invert failures, P1 and
P2 at beat 72.005005. Native writes zero; DeadSync incorrectly writes one.
The auxiliary actor receives `sleep(0):x(1)` between two queued reader
calls on that same frame. Its zero-time destination must wait for the
actor update instead of changing the current position immediately.

Use the existing pending zero-tail clock for both position getters and
captured tween scheduling. Keep prior current XYZ until the actor advances,
including a later sibling whose update is still pending on this frame.
Replace the duplicated zero-tail detection with the shared calculation.
No song-specific production conditions or tolerance changes are introduced.

The four-second native regression reads earlier and later siblings twice
on the broadcast frame. It fails before this correction and passes all
331 comparisons, including 322 modifier values, afterward. Existing queued
actor order and queued-command regressions also pass.

The complete Finite result includes 32,320 boolean API and 67,580 runtime
modifier comparisons, 17 timeline checks and all other existing sections.
It retains 6,473 update frames, ends at beat
258 and 107.86400604248048 seconds,
and has zero errors or dropped events. Capture settings: cel, single Edit,
seed 1, beat step 0.125, max events 2,000,000. Raw SHA-256:
`efbbdf63c585472b1c43602c53920da88446316bb16e7340dcc0b05bd2b4f897`. Exact compression round trip is verified. The manifest
retains harness, executable, simfile, Lua and noteskin provenance.

Delete the superseded raw reference; retain the complete compressed capture
and focused native regression. Validation uses MAIN's isolated checkout.
No downloads, game windows, resource edits, frozen-list edits or expanded
comparison scope. Both Cargo files bump exactly +1: 0.5.1794 -> 0.5.1795.
