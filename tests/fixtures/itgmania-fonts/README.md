# ITGmania font fixtures

These JSON files are native ITGmania outputs for every `.ini` under
`assets/fonts`. DeadSync's `font_itgmania_parity` integration test parses the
same bundled assets and checks font metrics, imported pages, glyph selection,
advance, placement, and sample rectangles exactly, including float bit
patterns.

The fixtures are deliberately committed so normal DeadSync tests do not need a
C++ toolchain or an ITGmania checkout. Regenerate them only when the font
assets, parser behavior, or chosen ITGmania reference changes:

```powershell
cd ..\itgmania-harness-rs
cargo run -- font-baseline ..\deadsync\assets\fonts `
  --out ..\deadsync\tests\fixtures\itgmania-fonts
cd ..\deadsync
cargo test --test font_itgmania_parity
```

`_manifest.json` records the harness version and ITGmania revision. Review all
fixture changes rather than updating expected output automatically in the
DeadSync test.

The current batch reports one native diagnostic: probing `Common default.ini`
as a top-level font reaches ITGmania's normal `Common default` fallback and its
recursion guard. ITGmania still returns the font's geometry, which is included.
