These controls compare native Model material clocks through both production
song Model and noteskin Model builders. The two actors use one texture with
velocity and global offsets; the second retains unequal state delays and
different translations while both states reference the same image.

`control.json` supplies ten native updates, including exact delay boundaries,
large jumps and repeated timestamps. `native.json` retains all forty actual
diffuse and glow draws. A zero-delta update may advance a state after a large
jump, so the comparison renders each known update prefix separately.

`control-frames.json` supplies 151 updates at 60 Hz. `native-frames.json`
retains all 604 native draws. This comparison uses the production compiler's
chronological replay and retained material tracks without supplying expected
UVs or overriding its clocks.

The captures use harness 0.1.39 at `1ceab4b`, linked to ITGmania `5c737928`.
Each capture retains source hashes, the native executable pin and output
hash. Reproduce either input with
`itgmania-harness-rs actor-conformance control.json` or the corresponding
`control-frames.json` from this directory.

ITGmania `AnimatedTexture::Update` advances once with a strict `>` boundary.
`GetTextureTranslate` includes elapsed cycle percentage, global offsets and
current-state translations. `Model::DrawPrimitives` applies that translation
to diffuse draws and omits it from glow draws. The triangle also disables
texture translation independently on two vertices, matching the native
`Texture matrix scaling.vert` shader. Older harness captures omitted this
shader step and cannot establish transformed UV parity.

Across both builders these tests check 1,288 draws and 7,728 transformed UV
coordinates at a 0.000001 tolerance. This establishes the controlled material
clocks and replay integration. Complete song parity, lighting and secondary
material animation remain separate checks; the whole-song Model acceptance
guard remains closed.
