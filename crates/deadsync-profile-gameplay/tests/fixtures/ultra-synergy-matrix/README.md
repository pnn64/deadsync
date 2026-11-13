# ULTRA SYNERGY MATRIX Reverse regression

`lua/default.lua` and `lua/easing.lua` are unchanged copies from
`In The Golf - Beta Pines/ULTRA SYNERGY MATRIX (MYUKKE. REMIX)`.
`control.ssc` supplies a small chart at the original 160 BPM; music and visual
assets are unnecessary for this scroll regression.

The song reads `ModsLevel_Song:Reverse()` during construction. Its modifier
template starts Reverse at zero, then schedules restoration at beat zero only
when that initial query was nonzero. Missing saved options therefore turn a
Reverse player's scroll upwards as soon as the template runs.

The regression runs these original scripts through the production profile
context, Lua compiler and gameplay modifier evaluator. It checks normal and
Reverse profiles on either single-player side, plus both mixed versus pairings.

```powershell
cargo test -p deadsync-profile-gameplay ultra_synergy_matrix_preserves_saved_reverse_through_song_lua
```

ITGmania's `ModsGroup::Assign(ModsLevel_Preferred, ...)` seeds all four option
levels. `PlayerState::GetPlayerOptions` returns the requested level directly.
The relevant source contracts are unchanged between the native oracle pin
`5c737928d93778c2c9f68e276052b220b43a468f` and the inspected source checkout
`704cdd5624155f743b59b8b64fedeb0588dae067`:

- `src/ModsGroup.h`: `Assign`, `Get`, `Update`.
- `src/PlayerState.cpp`: `GetPlayerOptions`, `SetPlayerOptions`.
- `src/PlayerOptions.cpp`: Reverse scalar binding and `FromString`.

`native-options.json` independently asserts initialization, the song's
save/reset/restore sequence, Current approach, and fresh assignment resets
against compiled ITGmania. `native-result.json` is the unmodified successful
oracle response, captured with:

```powershell
../itgmania-harness/target/debug/itgmania-harness-rs.exe actor-conformance crates/deadsync-profile-gameplay/tests/fixtures/ultra-synergy-matrix/native-options.json --out crates/deadsync-profile-gameplay/tests/fixtures/ultra-synergy-matrix/native-result.json
```

SHA-256 of the original Lua bytes:

- `lua/default.lua`: `ebab47c70d311c955fdd1a36cfc4f670ffe93bdb82e1b8ba8e59ceb0c9e07070`
- `lua/easing.lua`: `c1ca37ef394c7e0b73215c9562dd4e41f81ac790499f89c99b51ddf322df64b0`
- Oracle executable: `cbd32ce27f8e02d9dca7d9e2148ee7322acd9d90e4348431c9032c06df76b908`
