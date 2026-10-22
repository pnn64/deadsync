This synthetic skin exercises shock mines without installing or bundling a
third-party noteskin. Original images contain a plain core, sixteen horizontal
electricity bars with changing colors, and a sixteen-frame hit explosion.

The loader rotates the mine's ActorFrame for each dance button. A child's
button-specific rotation keeps the electricity horizontal. Its state offsets
and 0.05-second frame delays remain independent of NoteDisplay's beat seek.
The explosion wraps an implicit sprite sheet with metric Init/HitMine commands.

The native inputs can be reproduced from the repository root:

```text
itgmania-harness-rs actor-conformance crates/deadsync-noteskin/tests/fixtures/shock-mine/native-rotation-input.json --out target/shock-native-rotation.json
itgmania-harness-rs actor-conformance crates/deadsync-noteskin/tests/fixtures/shock-mine/native-animation-input.json --out target/shock-native-animation.json
```

`native-rotation-output.json` retains native provenance and each Spark sprite's
drawn world-space vertices, with its parent position subtracted. All four bars
have horizontal long edges. `cases.json` supplies the fixture's button settings.
The asset test compares those native vertices with the compiled layer transforms
through preview/gameplay loads and fresh/serialized compilation results.

`native-animation-output.json` records successful assertions against native
Sprite Load, SetAllStateDelays, animation seeks and PlayCommand. A HitMine fade
preserves every one of the sixteen current states. An explicit seek resets it.
`Player.cpp` sends mine judgments with Bright=false; `GhostArrowRow::DidTapNote`
plays Judgment, Dim and HitMine on actors that continue updating between hits.
The asset and notefield tests ensure HitMine uses that continuous frame clock,
while explicit hit seeks still use time relative to the hit.
