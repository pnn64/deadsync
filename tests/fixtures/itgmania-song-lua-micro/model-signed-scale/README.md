This control checks independent, signed Model scales through the production
song Model and noteskin Model paths. `default.lua` and `control.json` describe
the same five actors using the three-vertex mesh in `../model-material/`.

`native.json` contains ten actual ITGmania Model draws from harness 0.1.38
at `eb173c2`, linked to ITGmania `5c737928`. It retains every vertex and
draw state, input hashes, the executable pin and the complete output hash.
Reproduce the native output with
`itgmania-harness-rs actor-conformance control.json` from this directory.

The scene exercises distinct X/Y/Z zoom, a negative value on each axis,
rotation on all three axes, and a child under independently scaled parents
with negative Y and Z. Native `Actor::BeginDraw` multiplies each signed
current axis by its corresponding base axis. Z does not inherit Y, and
negative scales mirror geometry instead of collapsing it.

The production comparison checks all diffuse and glow vertices at the
existing 0.002 coordinate tolerance. Across both paths, it checks twenty
draws and 840 local/clip/NDC/screen coordinate values. The camera control
uses the same assertions, including native depth defaults and an explicit
Lua depth override.

This establishes controlled geometry and shader inputs. Texture clocks,
lighting, independent depth modes, texture pixels and complete song parity
remain separate comparisons. The whole-song Model acceptance guard stays
closed.
