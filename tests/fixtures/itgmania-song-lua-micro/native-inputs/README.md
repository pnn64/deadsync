# Native actor capture requests

These requests are copied unchanged from `itgmania-harness/fixtures/actors` at
`91f69c2333db9600c7ab251145ac7269a46cfd64` (0.1.95). They complete the bundled
input recipes for the independent native micro controls. The other recipes
already live beside their captures or in `tests/fixtures/itgmania-actors`.

`../native-refresh-inputs.json` maps each repository input and its resource
dependencies to a capture. Regenerate with:

```powershell
python -B -X utf8 scripts/refresh_native_actor_fixtures.py --harness ../itgmania-harness/target/debug/itgmania-harness-rs.exe --harness-source ../itgmania-harness --manifest tests/fixtures/itgmania-song-lua-micro/native-refresh-inputs.json --output-root .
```

Run the command from the repository root. Ordinary tests use the bundled inputs
and captures and do not need a harness checkout.
