# Spooky Lua parity (0.5.1737)

The frozen project's issue 699 refers to dance-single Challenge, hash
`d5bd4dd7224f68ff`, description `[FX] XO-`, meter 7. The song's title
contains “Medium”, but this pass checks its Challenge chart. The corpus
test pins the chart hash, description, steps type, difficulty and seed.

A fresh, complete native capture passes **3,316 / 3,316** checks:

| Comparison | Passed / total |
| --- | ---: |
| Compile info | 4 / 4 |
| Layer order | 2 / 2 |
| Final render | 6 / 6 |
| Player ranges | 4 / 4 |
| Projected geometry | 51 / 51 |
| Projected vibration | 18 / 18 |
| Timeline | 2 / 2 |
| Message commands | 3 / 3 |
| Runtime modifiers | 3,226 / 3,226 |
| Total | **3,316 / 3,316** |

No additional production fix is needed for this chart. The old reference
used a 0.25-beat interval; this capture uses 0.125 beat and the current
native harness. Comparison tolerances are unchanged. The reference file
is copied without rewriting its native data, and all other allowed-corpus
manifest entries are preserved.

ITGmania revision: `5b205125ad53b9867bb4a494ff858f8d38ad4406`, clean.
Context: seed 1, cel noteskin, both players, 854 by 480, 60 Hz updates,
0.125-beat samples. Capture: 7,082 update frames, ending at beat
275.3541564941406 and 118.00892421177456 seconds. Runtime errors and
dropped events are both zero. Capture SHA-256:
`fa1813f13f70d668dfbc32efcff31f47bbff9fea221870bf0c6a5a9ee46a8220`.
The allowed manifest records simfile, loaded Lua, harness-source and host
hashes. Its source-song hashes match the previous capture exactly.

Regenerate from the rework workspace with the cel noteskin environment:

```powershell
itgmania-harness-rs/target/debug/itgmania-harness-rs.exe song-lua-semantic-baseline `
  "allowed/[07] Spooky (SM) [Scrypts]" --out .tmp/spooky-native `
  --difficulty Challenge --steps-type dance-single --random-seed 1 `
  --beat-step 0.125 --max-events 2000000
```

Validation includes the named Spooky corpus test, the regular semantic
suite and Jumper's complete regression after sharing the chart guard.
