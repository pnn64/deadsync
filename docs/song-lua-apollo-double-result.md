# Apollo project double result (0.5.1788)

The frozen project chart is dance-double Challenge, description
`FS+ XO+ BR BU ST- MV FX`, hash `450ff4f0585fdae3`. Its retained result now
passes **11,224/11,224** existing semantic and runtime-modifier comparisons.
The corpus test pins the hash, description, style, difficulty and seed.
The previous single-chart reference is retained and identified in the
manifest; it does not cover this project entry.

## Final callback correction

The replay converted the rounded end beat back into seconds. Apollo's
last replay beat became 411.999977 instead of reaching the beat-412
callback, so its final fade never started. Remove that end-time round
trip and retain the supplied music horizon for every timing clock.
The native final-frame alpha, 0.0052490234375, now matches exactly.
The regression fixture also checks the preceding frame and a shorter
horizon, where the callback must remain absent. The strict-beat-boundary
regression passes. No tolerances or comparator scope were changed.

## Reference

Captured with the local headless semantic CLI, cel, dance-double
Challenge, random seed 1, beat step 0.125, max events 2,000,000.
The complete capture has 9,207 update frames, ends at beat 412 and
153.4271751681802 seconds, and has zero Lua errors or dropped events.
It preserves every emitted observation. Raw SHA-256:
`3149d18955f798a568681923b72ed101071260a65d8823a732f94737d570d4c8`.
Compression has an exact verified byte round trip. Source and harness
hashes are stored in the manifest. No downloads or ITGmania windows.

Validation used MAIN's isolated checkout with the curated sources. Both
Cargo files advance exactly once, from 0.5.1787 to 0.5.1788. The frozen
project data and supplied resources are unchanged. This does not claim
whole-game pixel or interactive-input parity.
