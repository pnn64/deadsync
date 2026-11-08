These original, solid-color sheets reproduce native note-color UV translations
without requiring an installed noteskin. The vertical sheet has two columns and
nine color rows, with explicit single-frame Lua states and metric spacing 0.0625.
Its 32x144 image uses a native 32x256 allocation. This has the same coordinate
ratio as a 128x576 image in a 128x1024 allocation.

`NoteDisplay.cpp` translates by the metric spacing in backing-texture units.
`RageTexture::CreateFrameRects` also normalizes frame coordinates to that
allocation. DeadSync uploads only the prepared image, so both coordinate domains
must identify the same pixels. Merely adding 0.0625 to an image-normalized UV
selects partial rows. Horizontal, stretched, power-of-two and tiny sheets cover
the corresponding allocation controls.

From the repository root, capture the native loader:

```text
itgmania-harness-rs actor-conformance crates/deadsync-noteskin/tests/fixtures/note-color-texture/native-input.json --out target/note-color-native.json
```

`native-output.json` retains the capture metadata and each case's `name`,
`dimensions` and `frame_rects`; decoded/uploaded pixel arrays are omitted.
The asset regression compares native coordinates and samples the complete color
row at multiple animation phases, for every column and quantization, through
both fresh and serialized compilation results. This is the shared slot UV path
used by the modifier preview and gameplay.
