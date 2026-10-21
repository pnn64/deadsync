These synthetic actors cover a lane background and press light below a foreground
receptor, plus a tap explosion whose ActorFrame and Sprite have independent
judgment commands. The asset test generates the required textures in a temporary
directory. No installed noteskin is required.

`native-timing-expected.json` contains the flash alpha sampled from native
ITGmania. Regenerate the full native response with:

```text
itgmania-harness-rs actor-conformance crates/deadsync-noteskin/tests/fixtures/actor-layers/native-timing-input.json --out target/actor-layer-native-timing.json
```

For each sample, retain its time and the `flash` actor's `current.diffuse[0][3]`,
along with the response provenance. The hold remains opaque until 0.31 seconds,
then the zero-duration alpha setter hides it.
