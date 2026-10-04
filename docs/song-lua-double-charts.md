# Double-chart Lua parity (0.5.1735)

The frozen project's feelyourtouch e.d.e.n DX chart is dance-double
Challenge, hash `d1ab790f73670414`. Its single chart has a different hash,
`ecc9cbae7195f915`, and passing the single-chart test did not cover DX.

## Runtime correction

`GetCurrentSteps():GetStepsType()` previously returned
`StepsType_Dance_Single` while the active style exposed eight columns. The
song consequently allocated four spline endpoints and failed when it
reached column five. Current Steps and stage-stat Steps now use the active
style; Trails derive their steps type from their Steps.

The optimized multitap compiler also read the single `Challenge` table
instead of `Double_Challenge`. This produced wrong lanes, arrow rotations,
and explosion visibility after the Lua error was fixed. It now selects
the double table and installs explosion judgment commands for all eight
columns, replacing the four-column assumption. All eight explosion trees
contain 55 commands each in the compiled chart.

## Native verification

The harness now accepts `--steps-type dance-double`. Explicit selection
requires the requested style and difficulty and fails if that chart is
absent. The native selection regression checks a real mixed-style simfile,
including an absent double Challenge chart that must not select single.

The full fresh DX capture uses ITGmania revision
`5b205125ad53b9867bb4a494ff858f8d38ad4406`, seed 1, cel, P1 enabled,
854 by 480, a 60 Hz update clock and samples every 0.125 beat. It runs
through beat 312 (120.7741935483871 seconds), contains 7,248 update frames,
and has zero runtime errors or dropped events.

| Comparison | Passed / total |
| --- | ---: |
| Compile info | 4 / 4 |
| Layer order | 2 / 2 |
| Final render | 444 / 444 |
| Multitap zoom | 135,360 / 135,360 |
| Multitap writes | 262,488 / 262,488 |
| Projected geometry | 104 / 104 |
| Projected vibration | 104 / 104 |
| Total | **398,506 / 398,506** |

The pre-fix runtime fails 17,940 checks against this same native capture.
Fixing only Steps metadata still fails 9,697 multitap writes. No comparison
tolerances were changed. The dedicated ignored test pins the frozen chart
hash before compiling and comparing the whole song.

The compressed native fixture is
`tests/fixtures/itgmania-song-lua-selected/feelyourtouch e.d.e.n/feelyourtouch eden.ssc.double.semantic.json.zst`.
Its exact uncompressed SHA-256 is
`e0064e33bd3837089c25b1fb04f0da476046d401065447209e6376c53e02c508`.
The simfile SHA-256 is
`66db5c1884fb2b633bfd26de2f32825a27f7fd9e8e08a0ae89d1ec4f27b8d645`.
Capture host SHA-256 is
`a93cc6d58aa7b88bdb1a88178dc3755fd8ec50dd6925df97207ab00026b470e0`.
Native actor source SHA-256 is
`ede41c27560de2a36b4645018e09f5d36102ba7f5978d38b25136b9d49e22ee8`.
Native Lua bridge source SHA-256 is
`cfb973fc40b3eed66eaf8098e34ae62453d5297515f586bb2d167a270a13e15f`.
The fixture additionally pins the noteskin dependency bytes.

Regenerate in the rework workspace with:

```powershell
itgmania-harness-rs/target/debug/itgmania-harness-rs.exe song-lua-semantic-baseline `
  "lua-songs/feelyourtouch e.d.e.n" --out .tmp/feelyourtouch-double-native `
  --steps-type dance-double --difficulty Challenge --beat-step 0.125 --max-events 2000000
```

Set `ITGMANIA_SONG_LUA_NOTESKIN_ROOT` to DeadSync's `assets/noteskins`
directory and `ITGMANIA_SONG_LUA_NOTESKIN` to `cel`. Compress the completed
capture without altering its JSON, then run:

```text
cargo test --test song_lua_itgmania_semantic_parity corpora::lua_songs::feelyourtouch_double -- --ignored
```

## Botanic Panic recapture

The same pass freshly captures the project's Botanic Panic chart,
hash `30448d01c606fce9`, using the corrected motion harness. It covers
both Lua layers, 8,154 frames through beat 362, and reports no runtime
errors or dropped events. All **435,923 / 435,923** comparisons pass,
including 41,732 geometry checks and 10,769 vibration checks. The selected
fixture retains its 0.125-beat sampling and all source hashes; its manifest
records the updated native provenance and exact compressed/raw hashes.

## Regression validation

Both the rework and main checkouts pass 970 Lua/profile/playback unit
tests and all 97 regular semantic tests. The workspace also passes 31
actor tests; the harness passes 126 tests, with its local-corpus test
ignored. The 72 ignored semantic tests include the full-song comparisons
run separately for this pass.

The main checkout passes the fresh DX and Botanic comparisons above. Its
existing feelyourtouch single-chart fixture also passes all
459,556 / 459,556 checks. That single-chart result is regression coverage
against the existing reference, rather than a fresh native recapture.

## Remaining project scope

These results cover two exact charts. They do not establish fresh parity
for all 63 frozen project entries. Jumper's fresh native capture currently
reports an initialization error in its Wordle save-data path. Get Into It
and Rhythm Hell still need their source charts located and audited. The
other entries also require the ongoing fresh audit.
