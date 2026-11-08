This synthetic two-color sprite sheet covers the implicit animation used by
plain image actors and `Def.Sprite` without a Frames table. ITGmania creates
sequential states with 0.1-second delays. Hit fades and zooms do not seek the
sprite; explicit `setstate` commands do.

The asset test compiles and reloads these actors, checking W1–W5 in both
brightness modes. Explicit single-state Frames tables and paused sprites remain
fixed. The image contains generated test colors, not noteskin artwork.

From the repository root, reproduce the native assertions with:

```text
itgmania-harness-rs actor-conformance crates/deadsync-noteskin/tests/fixtures/implicit-animation/native-input.json --out target/implicit-animation-native.json
```

`native-output.json` records the native source revision and successful assertions.
