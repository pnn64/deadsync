Original synthetic actors exercise the `rainbow()` effect used by receptor
sprites. The white square is generated test artwork; no installed noteskin is
required.

`native-input.json` samples ITGmania's beat and timer clocks, the two-second Lua
default, a one-second period, offsets, and custom timing with holds. Its
`noteskin_command` fields provide equivalent commands for the Rust test. The
non-white diffuse color and partial alpha check that rainbow replaces RGB and
preserves opacity. `native-output.json` retains only the native provenance and
sampled diffuse colors from `Actor::PreDraw`.

Reproduce the native capture from the repository root:

```text
itgmania-harness-rs actor-conformance crates/deadsync-noteskin/tests/fixtures/rainbow-effect/native-input.json --out target/rainbow-native.json
```

The asset test also compiles and reloads the Lua receptor, checking the effect
in all columns through full and receptor-only loads. The additive press overlay
remains separate from the rainbow actor.
