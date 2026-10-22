This synthetic skin covers a Sprite with combined base X/Y/Z rotations and
sixteen-frame hold/roll strips. Images are original fixture art; the installed
Spinning noteskin is not required.

Native `Actor::BeginDraw` applies X/Y rotations after Z. With a 180-degree X or Y
rotation, replacing that rotation with a texture mirror requires reversing the
Z turn because our texture mirrors precede rotation. Both half turns together
preserve the Z turn. The native cases include cardinal directions and 45-degree
controls with no reflection, either reflection, or both.

From the repository root, reproduce the native vertex capture:

```text
itgmania-harness-rs actor-conformance crates/deadsync-noteskin/tests/fixtures/base-xy-rotation/native-input.json --out target/base-xy-native.json
```

`native-output.json` retains native provenance and each Sprite's name, base
rotation and drawn world-space vertex XY coordinates. It omits other capture
metadata. The asset regression compares those vertices through both Lua and
direct bitmap loads, including tap notes, fake notes and hold/roll heads.
Fresh and serialized compilation results are tested with preview/gameplay loads.

The test copies `Body 16x1.png` under each button's active/inactive hold/roll body
and bottom-cap filenames. It checks sixteen-frame state tables and changing UVs
at four points in the four-beat NoteDisplay animation cycle.
