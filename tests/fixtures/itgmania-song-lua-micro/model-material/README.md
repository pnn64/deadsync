This control checks unlit Model material colors in the production song Lua
and noteskin render paths. `default.lua` and `control.json` describe the same
four actors. Cyber uses the original bundled meshes and textures.

`native.json` retains the real ITGmania Model draw materials, mesh sizes, and
diffuse/glow modes captured by harness 0.1.35 at commit `e8b6048`, linked to
ITGmania `5c737928`. The reference includes executable, native output, and
input file hashes. Run the committed native harness with
`actor-conformance control.json` to reproduce the full native draw output.

With lighting off, `RageDisplay_Legacy::SetMaterial` sets RGB to diffuse plus
emissive plus ambient; alpha comes from diffuse. GL clamps that color before
texture modulation. The reference's `unlit_color` is derived from those
captured material values using that source rule. This is a draw metadata and
shader input comparison, not a sampled GPU framebuffer capture.

The controls cover RGB saturation after tinting, diffuse alpha distinct from
MilkShape transparency, the untinted fixed material for mesh index -1, and
glow independent of diffuse material color. The production compositor test
checks 4,128 vertex color components through each of its two model paths.

Geometry transforms, pass ordering, lighting, skeletal animation, and sampled
texture pixels are outside this control. Full-song Model references remain
rejected until their actual geometry and draw states are compared.
