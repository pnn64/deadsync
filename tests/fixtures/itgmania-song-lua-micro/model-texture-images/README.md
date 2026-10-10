This control retains 183 draws from an independent native ITGmania actor
tree across 61 updates at 60 Hz. Diffuse and secondary materials use four
distinct images, unequal frame delays, and a queued `setstate(1)` at 0.5s.
The native source is ITGmania `5c737928`; harness 0.1.42 at `a80f15b` records
registered native texture identity, filtering, wrapping and sphere mapping.

Reproduce with `itgmania-harness-rs actor-conformance control.json --out
native-raw.json`. `native.json` retains every draw, input SHA-256 hashes,
the pinned executable and source, and the raw output hash. `default.lua`
runs through DeadSync's production compiler without clock overrides.

The original clock regression at DeadSync `ad222a896` checked the submitted secondary frame rectangle
and shared native diffuse translation at every update in both song Model
and noteskin Model paths: 122 observations, 488 coordinates at 0.000001
tolerance. It also requires a bound secondary texture. Before the fix,
DeadSync selected white at 0.25s while native ITGmania still selected green.
The compiler now retains each material's own strict, single-step update
clock. Queued state changes clamp both materials without resetting age.

The current `native_model_material_passes_match_production` regression
compares all three separate production passes against the same retained
native draws in both builders: 366 draws, 2,196 transformed UV coordinates
and 4,392 unlit material color components at 0.000001 tolerance. It checks
blend order, glow mode, independent frame selection, shared diffuse
translation and per-vertex texture-matrix scaling. Before the pass repair,
production emitted two draws while native emitted three at the first update.

Native coordinates address one image; this control explicitly maps them
into the current two-image atlas. It does not establish physical image
binding, per-image wrapping or framebuffer parity. Native's forced linear
secondary filtering also remains unresolved. Whole-song Model acceptance
stays closed. The separate `model-material-mapping` control retains native
per-material sphere state and exercises geometry cache separation.
