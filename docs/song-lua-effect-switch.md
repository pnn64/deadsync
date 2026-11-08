# Native actor effect selection

ITGmania stores one active `Actor::Effect`. Its vibration, rainbow, bob, pulse,
and other effect setters replace that selector. DeadSync previously retained
independent vibration and rainbow flags alongside the prior motion effect.
Mawaru8's boss switched from bob to vibration but continued bobbing with the
new vibration magnitude, moving its center away from the native position.

Effect selection now updates the shared mode and its vibration/rainbow render
aliases together. Starting pulse or bob clears vibration; starting vibration
clears bob and rainbow. `stopeffect` uses the same selection path, replacing
its duplicate flag clearing. Rainbow follows the native macro: it ignores
arguments, selects rainbow immediately, resets the actor timer when the effect
changes, and sets the period to two seconds. Glyph rainbow scrolling remains
an independent text property.

The old HSV rainbow and multiplication by the original RGB tint are removed.
Rainbow now uses `Actor::PreDraw`'s sinusoidal interpolation and cosine RGB
channels, replacing the actor RGB while retaining its original alpha. It uses
the captured actor phase and authored timing/offset, including the beat clock.

The new local `effect-switch.lua` fixture switches a leaf and a parent through
bob, vibration, pulse, and stop. The leaf also exercises rainbow, including a
false argument and a nonwhite initial tint. The local harness verifies 482
native vibration selectors and 2,896 native screen-coordinate values against
actual ITGmania actors. Production checks cover all 950 semantic comparisons,
2,169 local selector fields, 1,448 rendered native corners, and 1,448 native
draw color channels. Native colors are bytes, so the new color check permits
one byte quantization step. Existing tolerances were not widened. Random
vibration offsets retain the existing separate magnitude coverage.

The previous MAIN build passed 827/950 of the semantic fixture checks.
The effect-selection correction improves the local Mawaru8 audit from
**98,581/98,602** to **98,583/98,602**, with projected geometry improving to
68,859/68,871. The boss center and bounds now agree. The tween-boundary
correction below also restores both native player rotation ranges, bringing
the audit to **98,585/98,602**. Full-song parity remains unfinished: five
speculative message probes and twelve projected visibility checks remain.
The whole-song audit retains the earlier complete
native trace; native rainbow RGB is verified by the new actual-actor draw
fixture, since that whole-song trace does not compare RGB output.

Capture provenance: clean local ITGmania revision
`5b205125ad53b9867bb4a494ff858f8d38ad4406`; 854 by 480, 120 BPM, 60 Hz,
seed 1. The new micro-capture semantic host SHA-256 is
`0794b69f9ababa162f95fd0f5e55469fda208cc1b47ec58fa0253e26f8c6ea82`.
Raw semantic SHA-256:
`ddb55a40f80c259b1ba69366b72d86089ceb26d9a9f393dc41b227e4e02d7c63`.
Raw native actor draw SHA-256:
`39a43ca7bfb764ec51e2379dd6de5a3f2431399091564af97cf53c82cce46d14`.
Both compressed fixtures were verified by exact round trip.

Broader playback checks exposed two gaps left by the earlier native float
queue-clock change. The compact paused-update fixture retained the old
double-countdown positions; its seven changed positions now come from the
same local native capture used by the semantic suite (raw SHA-256
`5a297a11cb1f95cf072a6e51884d4d7b7bfcec4c1eaea3871046865cf721c6d3`).
All 72 sample identities and timestamps remain unchanged.

Retargeted tweens were also completed up to 0.0000001 seconds early in the
property capture path. At 0.7 seconds a callback wrote directly to `Before`'s
current Y instead of its pending destination, producing 121 instead of the
native 120.5. Remove that early-completion threshold from pending-tween
selection and completion; preserve the strict native playback comparisons.
Completed property states retain actor queue order. A trailing zero-time
reset waits for its preceding tween even when rounded cursor endpoints put
the reset first. The existing queued-message reset fixture catches the
otherwise persistent displacement of the runner and TV frame.

Final validation passes all 781 song-Lua unit tests, 171 production
playback integration tests, two AMV tests, and 104 regular semantic tests
in rework and MAIN. The local native harness passes 133 tests. The full
Mawaru8 audit retains the 17 failures listed above. No comparison
tolerances were widened.

No downloads, original song edits, or frozen project hash changes are included.
