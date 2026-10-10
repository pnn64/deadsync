This control exercises the GL/GLES2 Model geometry path supporting per-vertex
texture matrix scaling. Native `RageModelGeometry::MergeMeshes` appends the
second of exactly two equally named meshes to the first. It retains the second
mesh, and the first mesh keeps its material and rigid-bone binding. Matching
that behavior requires six vertices in the first draw and three in the second,
for both diffuse and glow passes.

`control.json` and `default.lua` describe the same Model. `native.json` retains
actual C++ Model draws from harness 0.1.40, its source and executable pins,
input hashes and the full native output hash. Reproduce the native scene with
`itgmania-harness-rs actor-conformance control.json` from this directory.

The production song Model and noteskin Model paths compare eight draws and
792 local/world/view/clip/NDC/screen coordinates at the existing 0.002 tolerance.
The parser also checks case-sensitive mesh names; differently named meshes
remain separate. This control has no bones or texture images and does not
establish complete Model animation, texture, lighting or whole-song parity.
