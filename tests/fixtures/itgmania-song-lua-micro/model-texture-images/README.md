This control retains 183 draws from an independent native ITGmania actor
tree across 61 updates at 60 Hz. Diffuse and secondary materials use four
distinct images, unequal frame delays, and a queued `setstate(1)` at 0.5s.
The native source is ITGmania `5c737928`; harness 0.1.42 at `a80f15b` records
registered native texture identity, filtering, wrapping and sphere mapping.

Reproduce with `itgmania-harness-rs actor-conformance control.json --out
native-raw.json`. `native.json` retains every draw, input SHA-256 hashes,
the pinned executable and source, and the raw output hash. `default.lua`
runs through DeadSync's production compiler without clock overrides.

The playback regression checks the submitted secondary frame rectangle
and shared native diffuse translation at every update in both song Model
and noteskin Model paths: 122 observations, 488 coordinates at 0.000001
tolerance. It also requires a bound secondary texture. Before the fix,
DeadSync selected white at 0.25s while native ITGmania still selected green.
The compiler now retains each material's own strict, single-step update
clock. Queued state changes clamp both materials without resetting age.

This is a material-clock regression. DeadSync still combines the diffuse
and secondary stages, while the captured desktop GL profile emits a
separate additive pass and forces linear filtering on it. Per-image atlas
wrapping, per-vertex secondary matrix scaling, and framebuffer parity
remain unverified. Whole-song Model acceptance stays closed.
