# Profile Mini and 7th Gear's spline paths

`options-request.json` runs the real ITGmania PlayerOptions bindings. Assigning
`M730, 60% Mini` at Preferred makes Mini 0.6 available at Preferred, Stage, Song,
and Current before song Lua constructs its actors.

`splines-request.json` uses 7th Gear's authored reverse-path equation at M730
with 60% and -40% Mini. The native CubicSpline solver evaluates the knots and
the endpoint clamp. At 60% Mini the correct path has 15 points, the reversed
receptor is at logical Y=375, and its final point is above the screen at
Y=-86.5484 after Mini. Building the path as if Mini were zero produces 10
points; applying the real 60% Mini later puts its endpoint at Y=37.79, near
the score, where later notes pile up.

`runtime-request.json` checks native Mini writes after initializing the saved
60% Mini. Song-level setters (including the pulse values 32% then 60%),
`FromString`, and direct Current setters assign absolute amounts. They do not
add the saved Mini. The old Rust runtime instead restored 120% Mini, shrinking
the field to 0.4 instead of 0.7. With the otherwise correct 15-point path this
put the normal receptor at Y=162.86 and the clamped endpoint at Y=426.57;
reversed scrolling mirrored both errors. The row is stacked future notes.

The `*-native.json` outputs were generated with harness 0.1.93 using pinned
ITGmania source `5c737928d93778c2c9f68e276052b220b43a468f`:

```powershell
../itgmania-harness/target/release/itgmania-harness-rs.exe actor-conformance tests/fixtures/itgmania-song-lua-micro/profile-mini/options-request.json --out tests/fixtures/itgmania-song-lua-micro/profile-mini/options-native.json
../itgmania-harness/target/release/itgmania-harness-rs.exe actor-conformance tests/fixtures/itgmania-song-lua-micro/profile-mini/splines-request.json --out tests/fixtures/itgmania-song-lua-micro/profile-mini/splines-native.json
../itgmania-harness/target/release/itgmania-harness-rs.exe actor-conformance tests/fixtures/itgmania-song-lua-micro/profile-mini/runtime-request.json --out tests/fixtures/itgmania-song-lua-micro/profile-mini/runtime-native.json
```

The focused regression is
`song_lua_runtime_tests::saved_mini_reaches_song_lua_constructor_and_reverse_path`
in `deadsync-profile-gameplay`. It checks production profile-to-Lua setup,
constructor reads, all four option levels, P1/P2 mapping, compiled path length,
native sample positions, and the compiled setter's actual runtime Mini and
resulting receptor/endpoint coordinates. The isolated controls do not establish full-song
pixel parity.
