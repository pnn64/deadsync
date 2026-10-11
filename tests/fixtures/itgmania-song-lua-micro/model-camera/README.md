This control checks Model cameras and depth placement through both production
song Model render paths. `default.lua` and `control.json` describe the same
four actors using the three-vertex MilkShape mesh in `../model-material/`.

`native.json` contains eight real ITGmania Model draws captured by harness
0.1.37 at `7ba1318`, linked to ITGmania `5c737928`. It retains the native
vertices, transforms, materials and draw states, with hashes of the inputs,
native executable and complete native output. Reproduce that output with
`itgmania-harness-rs actor-conformance control.json` from this directory.

The scene covers an inherited perspective camera with an off-center vanish
point, an intermediate ActorFrame without a camera setting, an explicit
zero-FOV reset, nonzero actor Z and a rotated Model under a vertically scaled
parent. The production compositor compares local, world, view, clip, NDC
and screen coordinates for all diffuse and glow vertices. Across the song Model and
noteskin Model paths, it checks 16 draws and 1,056 coordinate values.

`manual.lua` reuses the captured Perspective and Orthographic actors in
equivalent draw scopes. The perspective actor lives under a zero-FOV storage
frame but is drawn directly under the root camera; drawing the Reset frame
then installs its zero-FOV camera. A repeated direct draw verifies restoration
of the root camera. Both production paths compare twelve diffuse/glow draws
and 792 coordinate values against the corresponding native draws. These
checks use each compiled draw operation's camera, rather than the automatic
ancestor camera. The native coordinate baseline is the C++ scene above;
the manual Lua callback itself has not been recaptured natively.

`Model::Model` enables the Z buffer. `ActorFrame::DrawPrimitives` installs
declared cameras; `RageDisplay::LoadMenuPerspective` implements the
perspective and zero-FOV reset. The Model's actor transform retains Z before
its geometry is transformed and projected. These native source rules govern
the comparison; the expected coordinates come from the native capture.

This is a draw and shader input control. It does not establish texture pixels,
lighting, independent depth write/test modes or complete song parity. The
whole-song Model acceptance guard remains closed pending those comparisons.
